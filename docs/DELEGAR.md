# Trabajo para delegar

Tareas listas para dar a un agente (Antigravity, Claude Code u otro). Cada una indica qué archivos puede tocar y cuándo está terminada. Las reglas generales están en `AGENTS.md`.

## Cómo usarlo

1. **Elegir una tarea** con estado "Libre" y dársela al agente. Por ejemplo, en Antigravity: *"Toma la tarea D1 de docs/DELEGAR.md"*.
2. **Al empezar**, el agente cambia el estado a `En curso: <agente>`.
3. **Al terminar**, la marca como `Hecha`, con una línea de resumen y cómo la verificó (`/verificar`).
4. **Repartir sin choques:**
   - Dos tareas en curso a la vez no pueden compartir archivos.
   - Mientras no haya commits, todos los agentes editan el mismo árbol de trabajo. Por eso conviene que solo un agente trabaje a la vez, o que las tareas en paralelo no se toquen.
5. **Revisar:** las tareas marcadas "Revisión" las revisa Claude Code antes de darlas por cerradas. Tocan seguridad o el motor de sincronización.

Estado actual: Fases 0 a 8 implementadas (ver `docs/FASE-*.md`). La siguiente es la Fase 9, endurecimiento y publicación.

## Tareas de la Fase 9

### D1. Auditoría de dependencias
- **Estado:** Libre
- **Qué:** añadir `cargo-deny` con la base de avisos de RustSec, licencias permitidas y prohibición de fuentes que no sean crates.io. Sumarlo como paso al CI.
- **Archivos:** `deny.toml` (nuevo), `.github/workflows/ci.yml`.
- **Terminado cuando:** `cargo deny check` pasa localmente o deja cada excepción anotada con su motivo, y el CI tiene el paso. Si aparece una vulnerabilidad, actualizar la dependencia en `Cargo.toml` y correr `/verificar`.

### D2. Rendimiento con 100.000 movimientos
- **Estado:** Libre
- **Qué:** llevar la prueba de tiempos de `crates/app-state/tests/many_years.rs` a 100.000 movimientos. Medir inicio, informe anual, búsqueda, `portfolio_view`, `notices`, `activity`, `export_backup`, `restore_backup` y la vista previa de un CSV de 5.000 filas.
- **Archivos:** `crates/app-state/tests/many_years.rs`, `docs/FASE-9.md` (nuevo, solo la sección de rendimiento).
- **Terminado cuando:** hay una tabla de tiempos en compilación optimizada (`cargo test --release -p caudal-app-state --test many_years -- --ignored --nocapture`). Lo que pase de 100 ms en una vista que se redibuja queda señalado con una propuesta, sin implementarla.

### D3. Versiones de la base local
- **Estado:** Libre · **Revisión**
- **Qué:** guardar una versión de esquema en `meta` de los dos almacenes locales y pasar por pasos de migración al abrir, para que una app nueva pueda leer datos de una vieja.
- **Archivos:** `crates/store-sqlite/src/lib.rs`, `crates/store-idb/src/lib.rs`, `crates/sync/src/store.rs` (solo si hace falta una constante compartida).
- **Terminado cuando:** hay una prueba que abre una base "versión 1" creada a mano y la deja usable. `crates/sync/tests/convergence.rs` sigue en verde.

### D4. Privacidad, términos y aviso de no asesoría
- **Estado:** Libre
- **Qué:** redactar en español (y su versión en inglés) la política de privacidad, los términos de uso y un aviso de que la app registra y muestra pero no da recomendaciones de inversión. Enlazarlos desde Ajustes en las dos interfaces.
- **Archivos:** `docs/legal/` (nuevo), `crates/app-state/src/i18n.rs`, `apps/native/src/ui/pages.rs` (función `settings`), `apps/web/src/ui.rs` (componente `Settings`).
- **Terminado cuando:** existen los documentos y los enlaces se ven en Windows (capturas con `/revisar-interfaz`) y en la web. El texto debe nombrar qué datos se guardan (cuenta, movimientos, comprobantes en Supabase), que los precios vienen de terceros y cómo exportar y borrar los datos.

### D5. Notificaciones nativas de Android
- **Estado:** Libre · **Revisión**
- **Qué:** avisos del sistema para cuotas vencidas o por vencer y alertas de precio cumplidas, con el permiso `POST_NOTIFICATIONS` pedido en el momento oportuno.
- **Archivos:** `android/app/src/main/java/app/caudal/MainActivity.java`, `android/app/src/main/AndroidManifest.xml`, `apps/native/src/android.rs` o un módulo nuevo en `apps/native/src/`.
- **Terminado cuando:** en el emulador (`/android`) aparece una notificación al abrir la demo con una cuota vencida, y negar el permiso no rompe nada. Los textos salen de `caudal_app_state::notices`.

## Tareas que no se delegan

| Tarea | Por qué |
|---|---|
| Pruebas de extremo a extremo con Supabase local en Docker (registro, recuperación, sincronización entre dispositivos) | Las hace Claude Code, como está acordado para la Fase 9. |
| Eliminación total de cuenta (datos, archivos de Storage y usuario de Auth) | Toca seguridad y datos irreversibles; la diseña Claude Code y la aprueba el usuario. |
| Firmar el instalador de Windows, el AAB de Android y publicar la web | Requieren certificados y cuentas del usuario. |
| Contratar el plan comercial de datos de mercado y activar los secretos en GitHub | Son decisiones y credenciales del usuario. |
| Probar en un teléfono real | Necesita el teléfono del usuario. |
