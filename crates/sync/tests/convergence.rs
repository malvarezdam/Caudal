//! Simulación de dos dispositivos del mismo usuario con relojes desajustados,
//! cortes de red y fallos a mitad de sincronización.

use caudal_core::model::Hlc;
use caudal_sync::memory::{MemoryRemote, MemoryRemoteError, MemoryStore};
use caudal_sync::{Record, Remote, Repository, Row, Table, sync};
use pollster::block_on;
use proptest::prelude::*;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use uuid::Uuid;

const USER: Uuid = Uuid::from_u128(0xCA0DA1);

struct Device {
    repo: Repository<MemoryStore>,
    clock: Rc<Cell<u64>>,
}

fn device(node: u64, start_millis: u64) -> Device {
    let clock = Rc::new(Cell::new(start_millis));
    let now = Rc::clone(&clock);
    let repo = block_on(Repository::open(
        MemoryStore::default(),
        USER,
        move || now.get(),
        || node,
    ))
    .unwrap();
    Device { repo, clock }
}

fn key(index: u8) -> Uuid {
    Uuid::from_u128(1_000 + u128::from(index))
}

fn tag(index: u8, value: u8) -> serde_json::Map<String, Value> {
    json!({"id": key(index).to_string(), "name": format!("v{value}"), "color": null})
        .as_object()
        .unwrap()
        .clone()
}

/// Estado comparable: id → (nombre, borrado, marca), sin el cursor del servidor.
fn state(records: Vec<Record>) -> BTreeMap<Uuid, (Value, bool, Hlc)> {
    records
        .into_iter()
        .map(|record| {
            let deleted = record.is_deleted();
            (
                record.id,
                (record.row["name"].clone(), deleted, record.updated_at),
            )
        })
        .collect()
}

fn local_state(device: &Device) -> BTreeMap<Uuid, (Value, bool, Hlc)> {
    let active = block_on(device.repo.list_active(Table::Tags)).unwrap();
    let trash = block_on(device.repo.list_trash(Table::Tags)).unwrap();
    state(active.into_iter().chain(trash).collect())
}

#[derive(Debug, Clone)]
enum Op {
    Save {
        on_b: bool,
        key: u8,
        value: u8,
    },
    Delete {
        on_b: bool,
        key: u8,
    },
    Restore {
        on_b: bool,
        key: u8,
    },
    Advance {
        on_b: bool,
        millis: u16,
    },
    Sync {
        on_b: bool,
    },
    SyncPullFails {
        on_b: bool,
    },
    /// El usuario escribe mientras la sincronización está bajando cambios.
    SyncWithWrite {
        on_b: bool,
        key: u8,
        value: u8,
    },
    Offline(bool),
}

/// Servidor que, justo antes de la primera bajada, deja escribir al dispositivo.
struct WriteDuringPull<'a> {
    remote: &'a MemoryRemote,
    device: &'a Device,
    pending: Cell<Option<Row>>,
    written: RefCell<Option<Record>>,
}

impl Remote for WriteDuringPull<'_> {
    type Error = MemoryRemoteError;

    async fn push(&self, table: Table, rows: Vec<Row>) -> Result<(), MemoryRemoteError> {
        self.remote.push(table, rows).await
    }

    async fn pull(
        &self,
        table: Table,
        after: i64,
        limit: usize,
    ) -> Result<Vec<Record>, MemoryRemoteError> {
        let records = self.remote.pull(table, after, limit).await?;
        if let Some(row) = self.pending.take() {
            let record = self.device.repo.save(Table::Tags, row).await.unwrap();
            *self.written.borrow_mut() = Some(record);
        }
        Ok(records)
    }
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (any::<bool>(), 0u8..4, any::<u8>()).prop_map(|(on_b, key, value)| Op::Save { on_b, key, value }),
        1 => (any::<bool>(), 0u8..4).prop_map(|(on_b, key)| Op::Delete { on_b, key }),
        1 => (any::<bool>(), 0u8..4).prop_map(|(on_b, key)| Op::Restore { on_b, key }),
        2 => (any::<bool>(), any::<u16>()).prop_map(|(on_b, millis)| Op::Advance { on_b, millis }),
        3 => any::<bool>().prop_map(|on_b| Op::Sync { on_b }),
        1 => any::<bool>().prop_map(|on_b| Op::SyncPullFails { on_b }),
        2 => (any::<bool>(), 0u8..4, any::<u8>()).prop_map(|(on_b, key, value)| Op::SyncWithWrite { on_b, key, value }),
        1 => any::<bool>().prop_map(Op::Offline),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn dos_dispositivos_convergen(
        ops in proptest::collection::vec(op(), 1..80),
        skew in 0u64..10_000_000,
        batch in 1usize..5,
    ) {
        let remote = MemoryRemote::default();
        // B arranca con el reloj adelantado o atrasado respecto de A.
        let a = device(0xA, 10_000_000);
        let b = device(0xB, skew * 2);
        let mut writes: BTreeMap<Uuid, Record> = BTreeMap::new();

        for op in ops {
            let pick = |on_b: bool| if on_b { &b } else { &a };
            match op {
                Op::Save { on_b, key: index, value } => {
                    let record = block_on(pick(on_b).repo.save(Table::Tags, tag(index, value))).unwrap();
                    remember(&mut writes, record);
                }
                Op::Delete { on_b, key: index } => {
                    if let Some(record) = block_on(pick(on_b).repo.delete(Table::Tags, key(index))).unwrap() {
                        remember(&mut writes, record);
                    }
                }
                Op::Restore { on_b, key: index } => {
                    if let Some(record) = block_on(pick(on_b).repo.restore(Table::Tags, key(index))).unwrap() {
                        remember(&mut writes, record);
                    }
                }
                Op::Advance { on_b, millis } => {
                    let clock = &pick(on_b).clock;
                    clock.set(clock.get() + u64::from(millis));
                }
                Op::Sync { on_b } => {
                    let _ = block_on(sync(&pick(on_b).repo, &remote, batch));
                }
                Op::SyncPullFails { on_b } => {
                    remote.set_fail_pulls(true);
                    let _ = block_on(sync(&pick(on_b).repo, &remote, batch));
                    remote.set_fail_pulls(false);
                }
                Op::SyncWithWrite { on_b, key: index, value } => {
                    let interleaved = WriteDuringPull {
                        remote: &remote,
                        device: pick(on_b),
                        pending: Cell::new(Some(tag(index, value))),
                        written: RefCell::new(None),
                    };
                    let _ = block_on(sync(&pick(on_b).repo, &interleaved, batch));
                    if let Some(record) = interleaved.written.into_inner() {
                        remember(&mut writes, record);
                    }
                }
                Op::Offline(offline) => remote.set_offline(offline),
            }
        }

        remote.set_offline(false);
        for device in [&a, &b, &a] {
            block_on(sync(&device.repo, &remote, batch)).unwrap();
        }

        let in_a = local_state(&a);
        let in_b = local_state(&b);
        let on_server = state(remote.records());
        prop_assert_eq!(&in_a, &in_b);
        prop_assert_eq!(&in_a, &on_server);

        // Cada registro termina con la escritura de marca mayor.
        let expected = state(writes.into_values().collect());
        prop_assert_eq!(in_a, expected);
    }
}

fn remember(writes: &mut BTreeMap<Uuid, Record>, record: Record) {
    let wins = writes
        .get(&record.id)
        .is_none_or(|existing| record.updated_at > existing.updated_at);
    if wins {
        writes.insert(record.id, record);
    }
}

#[test]
fn edicion_offline_gana_si_es_posterior() {
    let remote = MemoryRemote::default();
    let a = device(0xA, 1_000);
    let b = device(0xB, 1_000);

    block_on(a.repo.save(Table::Tags, tag(0, 1))).unwrap();
    block_on(sync(&a.repo, &remote, 10)).unwrap();
    block_on(sync(&b.repo, &remote, 10)).unwrap();

    // A edita sin conexión; B edita más tarde y sincroniza primero.
    remote.set_offline(true);
    a.clock.set(2_000);
    block_on(a.repo.save(Table::Tags, tag(0, 2))).unwrap();
    assert!(block_on(sync(&a.repo, &remote, 10)).is_err());
    remote.set_offline(false);
    b.clock.set(3_000);
    block_on(b.repo.save(Table::Tags, tag(0, 3))).unwrap();
    block_on(sync(&b.repo, &remote, 10)).unwrap();

    let report = block_on(sync(&a.repo, &remote, 10)).unwrap();
    assert_eq!(report.pushed, 1);
    let local = block_on(a.repo.get(Table::Tags, key(0))).unwrap().unwrap();
    assert_eq!(local.row["name"], "v3");
}

#[test]
fn el_usuario_de_la_fila_siempre_es_el_del_repositorio() {
    let a = device(0xA, 1_000);
    let mut row = tag(0, 1);
    row.insert("user_id".into(), json!(Uuid::from_u128(666).to_string()));
    row.insert("server_seq".into(), json!(99));
    let record = block_on(a.repo.save(Table::Tags, row)).unwrap();
    assert_eq!(record.row["user_id"], USER.to_string());
    assert!(!record.row.contains_key("server_seq"));
}

#[test]
fn papelera_y_recuperacion() {
    let a = device(0xA, 1_000);
    block_on(a.repo.save(Table::Tags, tag(0, 1))).unwrap();
    block_on(a.repo.delete(Table::Tags, key(0))).unwrap();
    assert!(
        block_on(a.repo.list_active(Table::Tags))
            .unwrap()
            .is_empty()
    );
    assert_eq!(block_on(a.repo.list_trash(Table::Tags)).unwrap().len(), 1);
    block_on(a.repo.restore(Table::Tags, key(0))).unwrap();
    assert_eq!(block_on(a.repo.list_active(Table::Tags)).unwrap().len(), 1);
    assert!(
        block_on(a.repo.delete(Table::Tags, key(3)))
            .unwrap()
            .is_none()
    );
}

#[test]
fn el_reloj_sobrevive_a_reabrir() {
    let store = MemoryStore::default();
    let repo = block_on(Repository::open(store, USER, || 5_000, || 7)).unwrap();
    let first = block_on(repo.save(Table::Tags, tag(0, 1))).unwrap();
    let store = repo_into_store(repo);
    // El reloj del dispositivo retrocedió al reabrir.
    let repo = block_on(Repository::open(store, USER, || 10, || 99)).unwrap();
    let second = block_on(repo.save(Table::Tags, tag(0, 2))).unwrap();
    assert!(second.updated_at > first.updated_at);
    assert!(second.updated_at.0.ends_with("0000000000000007"));
}

fn repo_into_store(repo: Repository<MemoryStore>) -> MemoryStore {
    let store = MemoryStore::default();
    for table in Table::ALL {
        for record in block_on(caudal_sync::LocalStore::list(repo.store(), table)).unwrap() {
            block_on(caudal_sync::LocalStore::apply_remote(&store, &[record])).unwrap();
        }
    }
    for key in ["hlc_node", "hlc_last"] {
        let value = block_on(caudal_sync::LocalStore::meta(repo.store(), key)).unwrap();
        if let Some(value) = value {
            block_on(caudal_sync::LocalStore::set_meta(&store, key, &value)).unwrap();
        }
    }
    store
}

/// Dos dispositivos generan la misma ocurrencia sin conexión: no se duplica y,
/// si uno la confirmó o editó, ese cambio gana a la generación del otro.
#[test]
fn ocurrencias_generadas_no_pisan_cambios_del_usuario() {
    let remote = MemoryRemote::default();
    let a = device(0xA, 5_000);
    let b = device(0xB, 9_000_000);
    let occurrence = |status: &str| {
        json!({"id": key(9).to_string(), "name": "Arriendo", "color": null, "status": status})
            .as_object()
            .unwrap()
            .clone()
    };

    // A genera y el usuario la confirma; B (reloj adelantado) la genera después, sin conexión.
    block_on(a.repo.save_generated(Table::Tags, occurrence("pending"))).unwrap();
    block_on(a.repo.save(Table::Tags, occurrence("confirmed"))).unwrap();
    block_on(b.repo.save_generated(Table::Tags, occurrence("pending"))).unwrap();
    // Generar otra vez no reemplaza lo que ya hay.
    assert!(
        block_on(a.repo.save_generated(Table::Tags, occurrence("pending")))
            .unwrap()
            .is_none()
    );

    for device in [&b, &a, &b] {
        block_on(sync(&device.repo, &remote, 10)).unwrap();
    }
    for device in [&a, &b] {
        let records = block_on(device.repo.list_active(Table::Tags)).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].row["status"], "confirmed");
    }

    // Si el usuario la borra, regenerarla no la resucita.
    block_on(a.repo.delete(Table::Tags, key(9))).unwrap();
    block_on(sync(&a.repo, &remote, 10)).unwrap();
    block_on(b.repo.save_generated(Table::Tags, occurrence("pending"))).unwrap();
    block_on(sync(&b.repo, &remote, 10)).unwrap();
    assert!(
        block_on(b.repo.list_active(Table::Tags))
            .unwrap()
            .is_empty()
    );
}
