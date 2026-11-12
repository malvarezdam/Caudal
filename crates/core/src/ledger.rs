//! Saldos de cuentas, derivados siempre de los movimientos.

use crate::model::{Account, Transaction, TransactionKind, TransactionStatus};
use crate::money::{Money, MoneyError};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountBalance {
    pub balance: Money,
    /// Movimientos en otra moneda sin importe equivalente: no están en el saldo.
    pub unconverted: Vec<Uuid>,
}

/// Saldo de cada cuenta no borrada. Los movimientos pendientes y borrados no cuentan.
pub fn account_balances<'a>(
    accounts: &[Account],
    transactions: impl IntoIterator<Item = &'a Transaction>,
) -> Result<BTreeMap<Uuid, AccountBalance>, MoneyError> {
    let mut balances: BTreeMap<Uuid, (&Account, AccountBalance)> = accounts
        .iter()
        .filter(|account| !account.meta.is_deleted())
        .map(|account| {
            let balance = AccountBalance {
                balance: Money::new(account.opening_balance, account.currency.clone()),
                unconverted: Vec::new(),
            };
            (account.meta.id, (account, balance))
        })
        .collect();

    let counted = transactions
        .into_iter()
        .filter(|tx| !tx.meta.is_deleted() && tx.status == TransactionStatus::Confirmed);
    for tx in counted {
        let source_sign = match tx.kind {
            TransactionKind::Income | TransactionKind::Adjustment => Decimal::ONE,
            TransactionKind::Expense | TransactionKind::Transfer => Decimal::NEGATIVE_ONE,
        };
        apply(
            &mut balances,
            tx.account_id,
            tx,
            tx.account_amount,
            source_sign,
        )?;
        if tx.kind == TransactionKind::Transfer {
            apply(
                &mut balances,
                tx.to_account_id,
                tx,
                tx.to_amount,
                Decimal::ONE,
            )?;
        }
    }

    Ok(balances
        .into_iter()
        .map(|(id, (_, balance))| (id, balance))
        .collect())
}

fn apply(
    balances: &mut BTreeMap<Uuid, (&Account, AccountBalance)>,
    account_id: Option<Uuid>,
    tx: &Transaction,
    explicit_amount: Option<Decimal>,
    sign: Decimal,
) -> Result<(), MoneyError> {
    let Some((account, entry)) = account_id.and_then(|id| balances.get_mut(&id)) else {
        return Ok(());
    };
    let amount = explicit_amount.or((tx.currency == account.currency).then_some(tx.amount));
    let Some(amount) = amount else {
        entry.unconverted.push(tx.meta.id);
        return Ok(());
    };
    let delta = amount.checked_mul(sign).ok_or(MoneyError::Overflow)?;
    entry.balance.amount = entry
        .balance
        .amount
        .checked_add(delta)
        .ok_or(MoneyError::Overflow)?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::model::*;
    use crate::money::Currency;
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use std::str::FromStr;
    use uuid::Uuid;

    pub fn dec(value: &str) -> Decimal {
        Decimal::from_str(value).unwrap()
    }

    pub fn cur(code: &str) -> Currency {
        Currency::new(code).unwrap()
    }

    pub fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    pub fn meta(value: u128) -> SyncMeta {
        SyncMeta {
            id: id(value),
            user_id: id(9999),
            updated_at: Hlc("0001".into()),
            deleted_at: None,
            server_seq: None,
        }
    }

    pub fn account(value: u128, currency: &str, opening: &str) -> Account {
        Account {
            meta: meta(value),
            name: "cuenta".into(),
            kind: AccountKind::Bank,
            currency: cur(currency),
            opening_balance: dec(opening),
            archived: false,
            color: None,
            icon: None,
            note: None,
        }
    }

    pub fn tx(value: u128, kind: TransactionKind, amount: Decimal, currency: &str) -> Transaction {
        Transaction {
            meta: meta(value),
            kind,
            status: TransactionStatus::Confirmed,
            amount,
            currency: cur(currency),
            occurred_on: NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
            account_id: None,
            to_account_id: None,
            account_amount: None,
            to_amount: None,
            base_amount: None,
            base_currency: None,
            category_id: None,
            tag_ids: Vec::new(),
            contact_id: None,
            recurring_rule_id: None,
            occurrence_date: None,
            description: None,
            note: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::model::Hlc;
    use proptest::prelude::*;

    fn on(mut transaction: Transaction, account_id: u128) -> Transaction {
        transaction.account_id = Some(id(account_id));
        transaction
    }

    fn balance_of(balances: &BTreeMap<Uuid, AccountBalance>, account_id: u128) -> Decimal {
        balances[&id(account_id)].balance.amount
    }

    #[test]
    fn suma_saldo_inicial_ingresos_y_gastos() {
        let accounts = [account(1, "USD", "100")];
        let txs = [
            on(tx(10, TransactionKind::Income, dec("50.25"), "USD"), 1),
            on(tx(11, TransactionKind::Expense, dec("20.10"), "USD"), 1),
        ];
        let balances = account_balances(&accounts, &txs).unwrap();
        assert_eq!(balance_of(&balances, 1), dec("130.15"));
    }

    #[test]
    fn el_ajuste_admite_signo() {
        let accounts = [account(1, "USD", "100")];
        let txs = [on(
            tx(10, TransactionKind::Adjustment, dec("-30"), "USD"),
            1,
        )];
        let balances = account_balances(&accounts, &txs).unwrap();
        assert_eq!(balance_of(&balances, 1), dec("70"));
    }

    #[test]
    fn ignora_borrados_pendientes_y_sin_cuenta() {
        let accounts = [account(1, "USD", "100")];
        let mut deleted = on(tx(10, TransactionKind::Expense, dec("1"), "USD"), 1);
        deleted.meta.deleted_at = Some(Hlc("0002".into()));
        let mut pending = on(tx(11, TransactionKind::Expense, dec("2"), "USD"), 1);
        pending.status = TransactionStatus::Pending;
        let no_account = tx(12, TransactionKind::Expense, dec("3"), "USD");
        let balances = account_balances(&accounts, &[deleted, pending, no_account]).unwrap();
        assert_eq!(balance_of(&balances, 1), dec("100"));
    }

    #[test]
    fn transferencia_entre_monedas_usa_lo_recibido() {
        let accounts = [account(1, "USD", "100"), account(2, "CLP", "0")];
        let mut transfer = on(tx(10, TransactionKind::Transfer, dec("10"), "USD"), 1);
        transfer.to_account_id = Some(id(2));
        transfer.to_amount = Some(dec("9000"));
        let balances = account_balances(&accounts, &[transfer]).unwrap();
        assert_eq!(balance_of(&balances, 1), dec("90"));
        assert_eq!(balance_of(&balances, 2), dec("9000"));
    }

    #[test]
    fn avisa_si_no_puede_convertir() {
        let accounts = [account(1, "CLP", "0")];
        let txs = [on(tx(10, TransactionKind::Expense, dec("10"), "USD"), 1)];
        let balances = account_balances(&accounts, &txs).unwrap();
        assert_eq!(balance_of(&balances, 1), dec("0"));
        assert_eq!(balances[&id(1)].unconverted, vec![id(10)]);
    }

    #[test]
    fn usa_importe_en_moneda_de_la_cuenta() {
        let accounts = [account(1, "CLP", "0")];
        let mut foreign = on(tx(10, TransactionKind::Expense, dec("10"), "USD"), 1);
        foreign.account_amount = Some(dec("9000"));
        let balances = account_balances(&accounts, &[foreign]).unwrap();
        assert_eq!(balance_of(&balances, 1), dec("-9000"));
        assert!(balances[&id(1)].unconverted.is_empty());
    }

    proptest! {
        /// Una transferencia en la misma moneda nunca crea ni destruye dinero.
        #[test]
        fn transferencias_conservan_el_total(
            amounts in proptest::collection::vec((1i64..1_000_000_000, any::<bool>()), 0..50)
        ) {
            let accounts = [account(1, "USD", "1000"), account(2, "USD", "500")];
            let txs: Vec<Transaction> = amounts
                .iter()
                .enumerate()
                .map(|(index, (cents, forward))| {
                    let (from, to) = if *forward { (1, 2) } else { (2, 1) };
                    let mut transfer = on(
                        tx(100 + index as u128, TransactionKind::Transfer, Decimal::new(*cents, 2), "USD"),
                        from,
                    );
                    transfer.to_account_id = Some(id(to));
                    transfer
                })
                .collect();
            let balances = account_balances(&accounts, &txs).unwrap();
            prop_assert_eq!(balance_of(&balances, 1) + balance_of(&balances, 2), dec("1500"));
        }
    }
}
