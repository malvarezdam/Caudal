# Caudal: instrucciones para agentes

Este archivo es la fuente única de verdad del proyecto para cualquier agente (Claude Code, Antigravity u otro). `CLAUDE.md` lo importa. Si una regla cambia, se cambia aquí.

App de finanzas personales multiusuario para **Windows, Android y web**, escrita en Rust, con datos en Supabase y funcionamiento offline. El plan de ejecución está en [docs/ROADMAP.md](docs/ROADMAP.md).

Idioma del proyecto: documentación y comunicación en español; identificadores de código en inglés.

## Decisiones cerradas

| Tema | Decisión |
|---|---|
| Interfaz Windows y Android | `egui` / `eframe` |
| Interfaz web | `leptos` (WASM, CSR, instalable como PWA) |
| Backend | Supabase: Postgres, Auth, Storage y RLS |
| Usuarios | Registro abierto: cualquiera puede crear una cuenta |
| Offline | Local primero en las tres plataformas; sube al detectar internet |
| Conflictos | Gana el último cambio, por registro |
| Moneda | Ajustable por usuario; cada cuenta y activo tiene la suya |
| Mercados | Globales: principales bolsas, ETF y criptos |
| Fuentes de precios | EODHD (acciones y ETF), CoinGecko (criptos), Frankfurter (divisas) con respaldo `currency-api` para las que el BCE no publica, detrás de `PriceProvider` y `FxSource` |
| Actualización de precios | Automática, 1 o 2 veces al día, para cada activo que tenga algún usuario |

## Principio de libertad

El usuario decide cómo registra su dinero. Toda funcionalidad nueva debe respetar esto:

- Un movimiento solo exige monto y fecha. Lo demás es opcional.
- Todo es editable y borrable, incluso registros antiguos. Lo borrado va a la papelera.
- Categorías, etiquetas, cuentas y frecuencias las define el usuario. Las predeterminadas son solo sugerencias.
- Siempre se puede poner un precio manual a un activo y ajustar un saldo a mano.
- El usuario puede exportar todos sus datos y eliminar su cuenta.
- La app registra y muestra. **No da recomendaciones de inversión** ni ejecuta operaciones o pagos.

## Arquitectura

Workspace de Cargo:

```
crates/core            Dominio y cálculos puros. Sin IO. Compila a nativo y WASM.
crates/sync            Motor de sincronización y trait LocalStore.
crates/supabase        Cliente propio de Auth, PostgREST y Storage sobre reqwest.
crates/store-sqlite    LocalStore nativo (rusqlite con bundled).
crates/store-idb       LocalStore web (IndexedDB).
crates/app-state       Casos de uso y datos listos para mostrar, compartidos por ambas UI.
apps/native            UI egui para Windows y Android.
apps/web               UI Leptos.
apps/price-worker      Binario diario de precios y tipos de cambio.
supabase/migrations    Esquema SQL y políticas RLS.
```

Los paquetes llevan prefijo `caudal-` (`caudal-core`, `caudal-native`...), porque `core` chocaría con el crate estándar.

Dependencias: las UI solo hablan con `app-state`. `core` no depende de ningún otro crate del workspace. Ninguna regla de negocio vive en una UI.

## Reglas técnicas

**Dinero**
- `rust_decimal` en Rust y `NUMERIC` en Postgres. Nunca `f32` ni `f64` para importes o cantidades; solo se admite `f64` al dibujar gráficos.
- Serializar decimales como texto en JSON para no perder precisión al pasar por PostgREST.
- Cada importe lleva su moneda. Las conversiones usan el tipo de cambio de la fecha del movimiento y se guardan para que el histórico no cambie.

**Datos derivados**
- Saldos, resúmenes y ganancias se calculan desde los movimientos; no se almacenan como verdad. Se pueden cachear si se pueden reconstruir.

**Sincronización**
- IDs generados en el cliente con UUID v7.
- Toda tabla sincronizable lleva `user_id`, `updated_at` (reloj lógico híbrido, para tolerar relojes desajustados) y `deleted_at` (borrado lógico).
- Cada escritura va a la base local y a una cola de salida en la misma transacción; la UI nunca espera a la red.
- Subir la cola y luego bajar los cambios desde el último cursor. La resolución del último cambio se aplica por registro y debe ser idempotente.
- Las ocurrencias de registros periódicos usan UUID v5 derivado de regla + fecha, para que dos dispositivos offline no las dupliquen.
- Las UI escriben solo a través de `caudal_sync::Repository` (`save`, `delete`, `restore`); nunca directamente en el almacén. Se envían filas completas, porque cada subida reemplaza el registro entero.
- Todo cambio al motor debe mantener verde `crates/sync/tests/convergence.rs`. Si un cambio no la hace fallar cuando debería, la prueba está incompleta.
- Los registros generados automáticamente (ocurrencias de periódicos) se guardan con `Repository::save_generated`: id determinista y marca mínima, para que cualquier cambio del usuario gane. Nunca con `save`.
- El dinero que entra o sale por deudas y gastos compartidos se registra como ajuste de saldo (`TransactionKind::Adjustment`): mueve la cuenta, pero no cuenta como ingreso ni gasto del mes.
- Inversiones:
  - Las compras, ventas y comisiones ligadas a una cuenta también son ajustes; los dividendos son ingresos.
  - El movimiento ligado (`Trade::transaction_id`) se guarda con `AppCore::save_trade` y se borra o recupera junto con su operación. La cuenta debe estar en la moneda del activo.
  - Las posiciones usan costo promedio (`caudal_core::portfolio`) y el patrimonio incluye su valor de hoy.
- Datos del usuario (Fase 8):
  - La copia completa (`AppCore::export_backup` y `restore_backup`) incluye todas las tablas de `Table::ALL`. Una tabla nueva debe sumarse a `normalize_row` en `crates/app-state/src/backup.rs`.
  - Los comprobantes se guardan con `AppCore::add_attachment`: los datos van a `attachments` y el archivo, a la cola local hasta subir a `receipts/{user_id}/{id}`.
  - El PIN y el modo privado son del dispositivo. Nunca se sincronizan ni se guarda el PIN en claro.
- `serde_json` usa `arbitrary_precision`: PostgREST envía `numeric` como número y `normalize_numbers` lo pasa a texto. Las columnas enteras nuevas deben añadirse a `INTEGER_COLUMNS` en `crates/supabase/src/remote.rs`.

**Interfaz**
- Los datos de cada gráfico (escala, marcas, textos) se preparan en `app-state/src/charts.rs`; las UI solo dibujan. Los colores de serie están en `SERIES_COLORS`, validados con la guía de visualización; un color nuevo debe pasar `validate_palette.js`.
- En egui, toda vista que recorra los movimientos se pide a través de `app.cache` (`apps/native/src/cache.rs`), nunca directamente en cada fotograma.
- Los montos calculados (promedios, estimaciones, intereses simulados) se muestran con `format_estimate`; los registrados, con `format_money`.
- Todo monto pasa por `format_money`, `format_estimate` o `charts::compact`, que respetan el modo privado. No formatear importes a mano.
- Archivos del usuario: en egui, con `apps/native/src/files.rs` (diálogo del sistema en Windows y selector de documentos en Android por JNI); en la web, con `apps/web/src/files.rs`.

**Seguridad**
- RLS activa en todas las tablas con datos de usuario, con pruebas que demuestren que un usuario no ve datos de otro.
- La clave `service_role` y las claves de APIs de precios solo existen en los secretos del worker; nunca en un cliente ni en el repositorio.
- La clave pública (anon) sí puede ir en los clientes.
- Tokens de sesión: almacén seguro del sistema en nativo.

**Precios**
- Los clientes nunca llaman a proveedores de mercado; solo leen las tablas `asset_prices` y `fx_rates`.
- El worker corre 1 o 2 veces al día y, en cada ejecución, consulta una sola vez cada símbolo en poder de algún usuario, aunque lo tengan muchos.
- Se guarda un precio por activo y día: la segunda ejecución sobrescribe el valor del día. La cartera se valora con cantidad × último precio.
- Toda fuente implementa el trait `PriceProvider`. Si una falla, se conserva el último precio y la UI muestra su fecha.
- El worker (`apps/price-worker`) lee los símbolos con `held_symbols()` y escribe con upsert por día. Corre en `.github/workflows/prices.yml` con los secretos `SUPABASE_SECRET_KEY`, `EODHD_API_KEY` y `COINGECKO_API_KEY`.
- Las criptos se guardan en USD y la moneda de una acción la define su bolsa (`exchange_currency`). La app convierte con el tipo de cambio de la fecha del precio.
- Los clientes bajan precios y tipos con `AppCore::refresh_market`, como mucho una vez por hora tras sincronizar, y los guardan en el `meta` local para usarlos sin red.
- Las alertas de precio (`price_alerts`) se evalúan en el cliente con cada precio nuevo; solo informan.

## Comandos

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

Windows:

```bash
cargo run -p caudal-native
```

Sincronización contra el Supabase real. La ejecuta el usuario con `CAUDAL_EMAIL` y `CAUDAL_PASSWORD` definidas; ningún agente usa sus credenciales:

```bash
cargo run -p caudal-supabase --example sync_check
```

Web, desde `apps/web` (Trunk falla si `NO_COLOR=1`; usar `NO_COLOR=true`):

```bash
trunk serve --port 8088
```

Modo demostración, solo en compilaciones de desarrollo, con datos de ejemplo y sin sincronizar:
- Windows: `CAUDAL_DEMO=1`.
- Android: `adb shell run-as app.caudal touch files/demo`.
- Web: `?demo`.

Con `CAUDAL_SHOTS=<carpeta>`, la app de Windows recorre las pantallas en escritorio y móvil, guarda capturas PNG y se cierra. Así se revisa la interfaz sin controlar la pantalla.

Android, con `ANDROID_NDK_HOME` apuntando al NDK y `CARGO_TARGET_DIR` en una ruta **sin tildes**, porque el enlazador falla con la "í" de "Economía". Primero las bibliotecas y luego el APK, desde `android`:

```powershell
$env:CARGO_TARGET_DIR="$env:LOCALAPPDATA\caudal-android-target"
cargo ndk -t x86_64 -t arm64-v8a -P 26 -o android/app/src/main/jniLibs build -p caudal-native --lib
```

```bash
.\gradlew.bat assembleDebug
```

Worker de precios contra las fuentes reales, sin Supabase (EODHD usa la clave `demo` si no hay otra):

```bash
cargo run -p caudal-price-worker -- --dry-run eodhd:AAPL.US coingecko:bitcoin fx:CLP,EUR
```

Comprobación WASM:

```bash
cargo check --target wasm32-unknown-unknown -p caudal-web -p caudal-core -p caudal-app-state -p caudal-supabase -p caudal-store-idb
```

Android exige `games-activity` en la misma versión que el GameActivity incluido en `android-activity` (hoy 4.4.0).

## Entorno

Windows 11. Rust 1.97 con targets `wasm32-unknown-unknown`, `x86_64-pc-windows-msvc` y Android (aarch64, armv7, x86_64, i686) ya instalados. SDK de Android disponible en `ANDROID_HOME`.

## Riesgos abiertos

Detalle y evidencia en [docs/FASE-0.md](docs/FASE-0.md).

- **egui en Android:** winit 0.30 ignora el texto del teclado táctil; se resuelve con un puente propio sobre GameActivity (`apps/native/src/android.rs`). Las zonas del sistema (barras y teclado) las envía `MainActivity.java` por JNI. Probado en emulador; faltan pruebas en teléfono real. Plan B: Leptos instalado como PWA.
- **Licencia de datos de mercado:** los planes personales de EODHD y el gratuito de Twelve Data no permiten mostrar datos a terceros. Hace falta un plan comercial antes de abrir el registro público.
- **Límites gratuitos:** la cuota de la API y de Supabase condiciona cuántos usuarios y símbolos se soportan.

## Forma de trabajo

Estas reglas valen para cualquier agente que trabaje en el repositorio.

**Con el usuario**
- Hablar y documentar en español; el código, en inglés.
- Antes de algo grande (una fase o un cambio de alcance), explicar qué quedará y qué decisiones hay, con una recomendación concreta. Las decisiones técnicas el usuario las delega.
- Sin commits por fase ni por tarea: el usuario hará un solo commit al final. No hacer `git commit`, `push`, `reset`, `checkout` de archivos ni `stash` sin que lo pida.
- Supabase se configura pegando SQL en el SQL Editor. No usar la CLI de Supabase ni archivos `.env`.
- Nunca pedir, guardar ni usar contraseñas, la clave secreta de Supabase ni claves de APIs de precios. Esas claves viven solo en los secretos de GitHub. La clave pública sí está en el código.
- No crear cuentas en el Supabase real. Las pruebas de cuentas son de la Fase 9, con un Supabase local en Docker.

**Al programar**
- Leer este archivo y el documento de la fase en curso (`docs/FASE-*.md` y `docs/ROADMAP.md`) antes de empezar.
- La lógica va en `crates/` (`core` o `app-state`) con pruebas. Las UI de egui y Leptos solo dibujan, y cada cambio de interfaz se hace en las dos.
- Una tabla sincronizable nueva requiere todo esto:
  - una migración nueva en `supabase/migrations/` (nunca editar una ya aplicada);
  - la tabla en `supabase/tests/rls.sql`;
  - la variante en `Table` (`crates/sync/src/table.rs`) y su archivo en la prueba `coincide_con_la_migracion`;
  - la tabla en `normalize_row` (`backup.rs`) y su carga en `AppCore::load`.

  El flujo `/nueva-tabla` lo detalla.
- Las cadenas de texto van en `crates/app-state/src/i18n.rs`, en español y en inglés.

**Antes de dar algo por terminado**
- Correr `/verificar`: formato, clippy en nativo y WASM, y todas las pruebas. Si toca Android, compilar también las bibliotecas de Android.
- Revisar la interfaz con capturas (`CAUDAL_SHOTS` o `?demo` en el navegador), no solo compilando.
- Informar con evidencia: qué se probó y cómo, y qué no se pudo probar. No afirmar que algo funciona sin haberlo visto funcionar.
- Al cerrar una tarea grande, actualizar `docs/FASE-*.md` y, si cambia una regla, este archivo.

**Trabajo repartido entre agentes**
- Las tareas para delegar están en `docs/DELEGAR.md`, cada una con sus archivos y su criterio de terminado.
- Dos agentes no deben editar los mismos archivos a la vez. Antes de empezar, anotar en `docs/DELEGAR.md` qué tarea se toma (`En curso: <agente>`), y al terminar marcarla como hecha con un resumen de una línea.
- Si una tarea exige cambiar algo fuera de su lista de archivos, detenerse y avisar al usuario en vez de seguir.
