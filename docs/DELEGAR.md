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

## Reparto entre Claude Code y Antigravity

Antigravity hace el trabajo pesado; Claude Code planifica y revisa.

1. El usuario le pide a Claude Code una fase.
2. Claude Code define el alcance y las decisiones, escribe las tareas en este archivo y entrega un prompt corto para pegar en Antigravity. El prompt usa el flujo `/fase`.
3. Antigravity implementa, verifica y termina con un informe.
4. El usuario le pasa ese informe a Claude Code, que revisa con `/verificar`, el diff y los criterios de cada tarea. Si falta algo, Claude Code entrega otro prompt con las correcciones en lugar de hacerlas él mismo.

Estado actual: Fases 0 a 8 implementadas (ver `docs/FASE-*.md`). La siguiente es la Fase 9, endurecimiento y publicación.

## Tareas de la Fase 9

### D1. Auditoría de dependencias
- **Estado:** Hecha — `deny.toml` añadido (0 vulnerabilidades de seguridad, 2 avisos de mantenimiento en dependencias transitivas anotados con su motivo) y paso en CI; verificado con `cargo deny check` y `/verificar`.
- **Qué:** añadir `cargo-deny` con la base de avisos de RustSec, licencias permitidas y prohibición de fuentes que no sean crates.io. Sumarlo como paso al CI.
- **Archivos:** `deny.toml` (nuevo), `.github/workflows/ci.yml`.
- **Nota:** `cargo-deny` no está instalado; instalarlo con `cargo install cargo-deny --locked`.
- **Terminado cuando:** `cargo deny check` pasa localmente o deja cada excepción anotada con su motivo, y el CI tiene el paso. Si aparece una vulnerabilidad, actualizar la dependencia en `Cargo.toml` y correr `/verificar`.

### D2. Rendimiento con 100.000 movimientos
- **Estado:** Hecha — Prueba de 100.000 movimientos y tabla en `docs/FASE-9.md` (`activity` tarda 113 ms y queda con propuesta de selección previa sin formatear todo); verificado con `--release` y `/verificar`.
- **Qué:** llevar la prueba de tiempos de `crates/app-state/tests/many_years.rs` a 100.000 movimientos. Medir inicio, informe anual, búsqueda, `portfolio_view`, `notices`, `activity`, `export_backup`, `restore_backup` y la vista previa de un CSV de 5.000 filas.
- **Archivos:** `crates/app-state/tests/many_years.rs`, `docs/FASE-9.md` (nuevo, solo la sección de rendimiento).
- **Terminado cuando:** hay una tabla de tiempos en compilación optimizada (`cargo test --release -p caudal-app-state --test many_years -- --ignored --nocapture`). Lo que pase de 100 ms en una vista que se redibuja queda señalado con una propuesta, sin implementarla.

### D3. Versiones de la base local
- **Estado:** Hecha — Migración por pasos con `schema_version` en `meta` (v1 → v2) en `store-sqlite` y `store-idb`, probada con base v1 creada a mano y `convergence.rs` en verde; verificado con `/verificar`. · **Revisada por Claude Code:** OK (el upgrade de IndexedDB conserva los almacenes existentes).
- **Qué:** guardar una versión de esquema en `meta` de los dos almacenes locales y pasar por pasos de migración al abrir, para que una app nueva pueda leer datos de una vieja.
- **Archivos:** `crates/store-sqlite/src/lib.rs`, `crates/store-idb/src/lib.rs`, `crates/sync/src/store.rs` (solo si hace falta una constante compartida).
- **Terminado cuando:** hay una prueba que abre una base "versión 1" creada a mano y la deja usable. `crates/sync/tests/convergence.rs` sigue en verde.

### D4. Privacidad, términos y aviso de no asesoría
- **Estado:** Hecha — Documentos en español e inglés en `docs/legal/` con marcadores `[RESPONSABLE]` y `[CORREO DE CONTACTO]`, textos desplegables en Ajustes (nativo y web); verificado con `/revisar-interfaz` y `/verificar`.
- **Qué:** redactar en español (y su versión en inglés) la política de privacidad, los términos de uso y un aviso de que la app registra y muestra pero no da recomendaciones de inversión. Enlazarlos desde Ajustes en las dos interfaces.
- **Archivos:** `docs/legal/` (nuevo), `crates/app-state/src/i18n.rs`, `apps/native/src/ui/pages.rs` (función `settings`), `apps/web/src/ui.rs` (componente `Settings`).
- **Decisión:** el nombre del responsable y el correo de contacto aún no están definidos. Dejarlos como `[RESPONSABLE]` y `[CORREO DE CONTACTO]`, que el usuario completará. No inventar datos de una empresa.
- **Terminado cuando:** existen los documentos y los enlaces se ven en Windows (capturas con `/revisar-interfaz`) y en la web. El texto debe nombrar qué datos se guardan (cuenta, movimientos, comprobantes en Supabase), que los precios vienen de terceros y cómo exportar y borrar los datos.

### D5. Notificaciones nativas de Android
- **Estado:** Hecha — `files/notices.json` con avisos de 7 días y alertas de precio, `NoticeWorker` diario en Java con deduplicación por día y `POST_NOTIFICATIONS` pedido solo al existir avisos; verificado en emulador (`/android`) y `/verificar`. · **Revisada por Claude Code:** OK; el archivo de avisos ya solo se escribe en Android.
- **Qué:** avisos del sistema para cuotas vencidas o por vencer y alertas de precio cumplidas, con el permiso `POST_NOTIFICATIONS` pedido en el momento oportuno.
- **Archivos:** `android/app/src/main/java/app/caudal/MainActivity.java`, `android/app/src/main/AndroidManifest.xml`, `apps/native/src/android.rs` o un módulo nuevo en `apps/native/src/`.
- **Decisión de diseño:** cada vez que carga los datos, la app escribe en `files/notices.json` los avisos de los próximos 7 días (fecha y texto ya traducido, desde `caudal_app_state::notices` y `recurring::upcoming`). Un `WorkManager` diario en Java lee ese archivo y publica los avisos del día. No se ejecuta Rust en segundo plano ni se usa la red. El permiso se pide la primera vez que hay un aviso que mostrar, no al abrir la app.
- **Archivos extra:** `android/app/build.gradle.kts` (dependencia `androidx.work:work-runtime`) y una clase Java nueva en `android/app/src/main/java/app/caudal/`.
- **Terminado cuando:** en el emulador (`/android`), con la demo (que tiene una cuota vencida), aparece la notificación al ejecutar el trabajo diario a mano (por ejemplo, encolando un `OneTimeWorkRequest` de prueba). Negar el permiso no rompe nada, y no se repite el mismo aviso el mismo día.

### D6. Pruebas de extremo a extremo en el proyecto de desarrollo
- **Estado:** Hecha — `crates/app-state/tests/e2e.rs` (`#[ignore]`, con `CAUDAL_E2E=1` y `CAUDAL_E2E_DOMAIN`) y guía en `docs/FASE-9.md` (escritas sin ejecutar por no estar definida `CAUDAL_E2E_DOMAIN`); verificado con `/verificar`. · **Revisada por Claude Code:** OK, sin claves; falta que el usuario la ejecute.
- **Qué:** pruebas automáticas contra el proyecto Supabase de desarrollo (`SUPABASE_URL` del código, clave pública), sin Docker. Cubren:
  - registro e inicio de sesión;
  - subida y bajada de datos;
  - sincronización entre dos dispositivos: dos `AppCore` con almacenes distintos y la misma cuenta, que editan sin red y luego se reconectan; tiene que ganar el último cambio y no puede duplicarse nada;
  - subida de un comprobante a Storage;
  - que una cuenta no ve los datos ni los archivos de otra.
- **Cuentas:**
  - Las crean las pruebas, con correos `caudal-prueba-<aleatorio>@<dominio que indique el usuario>` y contraseñas aleatorias generadas en el momento, que nunca se guardan en archivos.
  - Requieren que el usuario desactive "Confirm email" en Supabase (Authentication → Sign In / Providers → Email) mientras corren.
  - La recuperación de contraseña con código no se automatiza: la prueba el usuario a mano una vez.
- **Archivos:** `crates/app-state/tests/e2e.rs` (nuevo, con `#[ignore]` y solo si existe la variable `CAUDAL_E2E=1`), `docs/FASE-9.md` (sección de extremo a extremo con el comando y los pasos manuales).
- **Terminado cuando:** `CAUDAL_E2E=1 cargo test -p caudal-app-state --test e2e -- --ignored` está documentado y escrito. Si el usuario aún no aplicó las migraciones 0002 y 0003 o no desactivó la confirmación, dejarlo indicado en el informe en vez de forzar nada. Nunca usar la clave secreta ni credenciales del usuario.

### D7. Eliminación total de cuenta
- **Estado:** Hecha — Migración `0004_delete_account.sql`, cascada en `rls.sql` probada sin red con PGlite, borrado de Storage y almacén local con pruebas mock en Rust, doble confirmación con oferta de exportar copia en egui y Leptos; verificado con `/revisar-interfaz` y `/verificar`. · **Revisada por Claude Code:** SQL correcto. Se corrigió que el borrado local se hacía con la base aún abierta (Windows no borraba el archivo y la web quedaba esperando).
- **Qué:** botón "Eliminar mi cuenta" con doble confirmación, que ofrezca antes exportar la copia completa. Debe borrar:
  - los archivos de `receipts/{user_id}/`, con la API de Storage, porque Supabase no permite borrarlos por SQL;
  - todas las filas, en cascada desde `auth.users`;
  - el usuario de Auth;
  - y luego los datos locales del dispositivo.
- **Archivos:** una migración nueva `supabase/migrations/0004_delete_account.sql` (función `security definer` que solo borra al usuario que la llama), `crates/supabase/src/`, `crates/app-state/src/core.rs`, las pantallas de Ajustes en `apps/native/src/ui/` y `apps/web/src/ui.rs`, `supabase/tests/rls.sql` y el arnés de PGlite si hace falta. El arnés está fuera del repositorio: describir en el informe cómo se probó.
- **Terminado cuando:** hay pruebas, sin Supabase, de que la función solo borra a quien la llama, que borrar a A no toca nada de B y que A no puede borrar a B. Además, `e2e.rs` tiene la prueba contra el proyecto de desarrollo, y al final de cada prueba de D6 borra las cuentas creadas. Esas pruebas solo funcionarán después de que Claude Code revise la migración y el usuario la aplique.

## Lo que solo puede hacer el usuario

| Tarea | Por qué |
|---|---|
| Desactivar "Confirm email" mientras corren las pruebas de D6, y probar a mano la recuperación de contraseña | Son ajustes del proyecto y requieren un correo real. |
| Aplicar las migraciones en el Supabase real (SQL Editor) | Es la base de producción; el usuario lo hace tras la revisión. |
| Firmar el instalador de Windows, el AAB de Android y publicar la web | Requieren certificados y cuentas del usuario. |
| Contratar el plan comercial de datos de mercado y activar los secretos en GitHub | Son decisiones y credenciales del usuario. |
| Probar en un teléfono real | Necesita el teléfono del usuario. |
