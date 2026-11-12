# Fase 9: endurecimiento y publicación

## Rendimiento con 100.000 movimientos

Tiempos medidos en compilación optimizada (`cargo test --release -p caudal-app-state --test many_years -- --ignored --nocapture`) con tres años de datos y **100.000 movimientos** (más cuentas, categorías, deudas, presupuestos, metas, activos y precios de ejemplo):

| Operación | Tiempo (`--release`) | ¿Vista que se redibuja? | Estado (< 100 ms) |
|---|---:|---|---|
| `dashboard` (inicio) | 9,5 ms | Sí (cacheada en `ViewCache::dashboard`) | OK |
| `reports_view` (informe del mes) | 52,8 ms | Sí (cacheada en `ViewCache::reports`) | OK |
| `reports_view` (informe del año) | 55,3 ms | Sí (cacheada en `ViewCache::reports`) | OK |
| `transactions` (lista con búsqueda `"super"`) | 16,9 ms | Sí (cacheada en `ViewCache::transactions`) | OK |
| `budgets_view` (presupuestos) | 3,9 ms | Sí (cacheada en `ViewCache::budgets`) | OK |
| `portfolio_view` (cartera de inversiones) | 0,13 ms | Sí (cacheada en `ViewCache::portfolio`) | OK |
| `notices` (avisos del inicio) | 3,4 ms | Sí (cacheada en `ViewCache::notices`) | OK |
| `activity` (actividad reciente, 30 últimos) | **113,3 ms** | Sí (cacheada en `ViewCache::activity` al abrir Ajustes) | **Supera 100 ms** |
| `csv_io::preview` (vista previa CSV de 5.000 filas contra 100.000 movimientos) | 31,2 ms | Al elegir archivo o cambiar mapeo | OK |
| `export_backup` (copia completa JSON) | 893,6 ms | Acción puntual a pedido del usuario | — |
| `restore_backup` (restaurar 100.000 filas) | 1.475,6 ms | Acción puntual a pedido del usuario | — |

### Hallazgo (> 100 ms) y propuesta (sin implementar)

- **`notices::activity` (113,3 ms con 100.000 movimientos):** aunque en egui está protegida por `app.cache.activity` y solo se recalcula cuando cambian los datos, supera el umbral de 100 ms porque hoy construye `transaction_view(data, tx, lang)` y formatea la cadena `"{} · {}"` para **los 100.000 movimientos** antes de ordenarlos y quedarse con los `limit` (30) más recientes (`crates/app-state/src/notices.rs`).
- **Propuesta:** separar la selección del formateo. Primero recorrer todas las tablas extrayendo únicamente `(millis(&meta.updated_at), id, fuente)` sin alojar `String` ni llamar a `transaction_view`, quedarse con los `limit` mayores (con `select_nth_unstable_by` o un `BinaryHeap` acotado a `limit` en $O(N \log k)$), y recién formatear `transaction_view` y `when(...)` para esos `limit` elementos finales. Con eso el tiempo de `activity` bajaría de ~113 ms a menos de 3 ms.

## Pruebas de extremo a extremo (E2E) en el proyecto de desarrollo

Las pruebas de extremo a extremo están en `crates/app-state/tests/e2e.rs`, marcadas con `#[ignore]` y protegidas por la variable `CAUDAL_E2E=1`. Usan únicamente la URL y la clave pública (`SUPABASE_PUBLISHABLE_KEY`) del proyecto de desarrollo; nunca la clave secreta ni credenciales reales del usuario.

### Qué cubren

1. **Registro e inicio de sesión:** crea cuentas temporales `caudal-prueba-<aleatorio>@<CAUDAL_E2E_DOMAIN>` con contraseñas aleatorias generadas en memoria al momento, cierra sesión y vuelve a iniciar sesión.
2. **Subida, bajada y sincronización entre dos dispositivos:** dos instancias de `AppCore` con almacenes distintos y la misma cuenta sincronizan datos iniciales, editan sin red con marcas de reloj lógico distintas y luego se reconectan; gana el último cambio y no se duplica ningún registro.
3. **Comprobantes en Storage:** sube un comprobante PNG a `receipts/{user_id}/{id}` al sincronizar y lo descarga desde un segundo dispositivo de la misma cuenta.
4. **Aislamiento entre cuentas (RLS y Storage):** verifica que una cuenta B no ve las filas ni puede descargar o sobrescribir los archivos de la cuenta A.
5. **Eliminación total de cuenta:** al terminar cada prueba, llama a `AppCore::delete_account`, que borra los archivos de `receipts/{user_id}/`, ejecuta `public.delete_account()` (`DELETE FROM auth.users` en cascada), limpia el almacén local y comprueba que la cuenta eliminada ya no puede iniciar sesión sin afectar a otras cuentas.

### Pasos manuales previos en Supabase

1. En el **SQL Editor** del proyecto de desarrollo, haber aplicado las migraciones:
   - `supabase/migrations/0001_schema.sql`
   - `supabase/migrations/0002_prices.sql`
   - `supabase/migrations/0003_data.sql`
   - `supabase/migrations/0004_delete_account.sql`
2. Mientras corren las pruebas, desactivar **"Confirm email"** en el panel de Supabase (**Authentication → Sign In / Providers → Email**), para que el registro de `caudal-prueba-*` entregue sesión sin esperar confirmación por correo. Al terminar, volver a activarlo.
3. **Recuperación de contraseña con código:** no se automatiza porque requiere leer un correo real; se prueba una vez a mano desde la pantalla "¿Olvidaste tu contraseña?" enviando el código al correo del usuario, verificándolo y definiendo una contraseña nueva.

### Comando para ejecutarlas

En PowerShell:

```powershell
$env:CAUDAL_E2E="1"
$env:CAUDAL_E2E_DOMAIN="tudominio.com"
cargo test -p caudal-app-state --test e2e -- --ignored --nocapture
```

## Eliminación total de cuenta

La funcionalidad de eliminación completa de cuenta cumple con el principio de que el usuario es dueño absoluto de sus datos y puede borrarlos en cualquier momento sin dejar rastro en el backend ni en sus dispositivos.

### Flujo de eliminación

1. **Interfaz (Ajustes en Windows, Android y Web):**
   - Botón destructivo "Eliminar mi cuenta".
   - **Paso 1:** Diálogo de advertencia que informa la pérdida permanente de la cuenta, movimientos, comprobantes y datos locales, ofreciendo un botón para "Exportar copia completa" antes de continuar.
   - **Paso 2:** Confirmación definitiva obligatoria ("Sí, eliminar mi cuenta definitivamente").
2. **Limpieza remota de Storage:**
   - La API de Storage de Supabase no permite borrar archivos por SQL (las tablas de `storage.objects` están gestionadas por el servicio de Storage).
   - El cliente lista y borra todos los archivos bajo el prefijo `receipts/{user_id}/` (tanto los conocidos localmente como los listados en el bucket) antes de eliminar el registro del usuario.
3. **Cascada en Postgres (`public.delete_account()`):**
   - Migración `supabase/migrations/0004_delete_account.sql`.
   - Función `security definer` que no acepta parámetros (evitando cualquier riesgo de que un usuario intente pasar el ID de otro) y ejecuta `DELETE FROM auth.users WHERE id = (SELECT auth.uid())`.
   - Gracias a las restricciones `ON DELETE CASCADE` de `0001_schema.sql`, eliminar el usuario en `auth.users` borra automáticamente todas sus filas en las 20 tablas sincronizables (`profiles`, `accounts`, `transactions`, `attachments`...).
4. **Limpieza local:**
   - Se borra el archivo SQLite local (`caudal-{user_id}`) en nativo o la base IndexedDB en web.
   - Se limpia la sesión segura del sistema, los avisos en disco y el estado en memoria, regresando la interfaz a la pantalla de bienvenida/inicio de sesión.

### Verificación sin Supabase (offline)

- **PGlite:** Se ejecutó el arnés offline en Node.js aplicando las migraciones `0001` a `0004` y la suite de pruebas `supabase/tests/rls.sql`. Verifica que `public.delete_account()` no acepta parámetros, rechaza llamadas de usuarios anónimos (`anon`), borra a quien la invoca de `auth.users` y en cascada de las 20 tablas de usuario, y garantiza que un usuario A no borra ni modifica ningún dato o archivo del usuario B.
- **Mock / Wiremock:**
  - `crates/supabase/tests/client.rs` (`elimina_archivos_y_cuenta_del_usuario`): prueba el borrado de Storage y la invocación del RPC.
  - `crates/app-state/tests/attachments.rs` (`eliminar_cuenta_limpia_storage_remoto_y_almacen_local`): prueba el flujo de `AppCore::delete_account` verificando la limpieza coordinada de Storage, Auth y el almacén local.


