---
description: Añadir una tabla sincronizable nueva sin romper la sincronización, la seguridad ni la copia completa
---

Una tabla con datos de usuario toca siete lugares. Saltarse uno rompe la sincronización, la copia o la seguridad.

1. **Migración nueva** en `supabase/migrations/NNNN_nombre.sql` (nunca editar una ya aplicada). Copiar el patrón de `0003_data.sql`:
   - columnas `id uuid primary key`, `user_id ... references auth.users on delete cascade`, `updated_at text collate "C"`, `deleted_at text collate "C"`, `server_seq bigint not null default 0`;
   - RLS con políticas de select, insert y update por `user_id = (select auth.uid())`, sin delete;
   - trigger `sync_guard`, índice `(user_id, server_seq)` y los grants.
2. **Prueba de seguridad:** añadir la tabla a `user_tables` en `supabase/tests/rls.sql` e insertar una fila de prueba para el usuario A.
3. **Modelo** en `crates/core/src/model.rs`, con `#[serde(flatten)] pub meta: SyncMeta`. Decimales con `rust_decimal`, nunca `f64`.
4. **Tabla** en `crates/sync/src/table.rs`: variante en `Table`, en `Table::ALL` y en `as_str`. Si es una migración nueva, añadirla al `include_str!` de la prueba `coincide_con_la_migracion`.
5. **Columnas enteras:** si las hay, añadirlas a `INTEGER_COLUMNS` en `crates/supabase/src/remote.rs`.
6. **Datos y copia:** campo en `Data` (`crates/app-state/src/data.rs`), lectura en `AppCore::load`, la tabla en la lista de la papelera de `load`, y su modelo en `normalize_row` (`crates/app-state/src/backup.rs`).
7. **Verificación:** correr `/verificar`. La prueba `crates/sync/tests/convergence.rs` debe seguir en verde. Decirle al usuario que ejecute la migración y luego `rls.sql` en el SQL Editor de Supabase.
