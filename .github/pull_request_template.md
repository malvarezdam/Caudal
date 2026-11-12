## Qué cambia

## Cómo se probó

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] Comprobación WASM
- [ ] Si toca el motor de sincronización: `crates/sync/tests/convergence.rs` sigue en verde
- [ ] Si toca tablas: RLS y `supabase/tests/rls.sql` actualizados
