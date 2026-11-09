# Caudal: memoria del proyecto

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
| Fuentes de precios | EODHD (acciones y ETF), CoinGecko (criptos), Frankfurter (divisas), detrás de `PriceProvider` |
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

Web, desde `apps/web` (Trunk falla si `NO_COLOR=1`; usar `NO_COLOR=true`):

```bash
trunk serve --port 8088
```

Android, con `ANDROID_NDK_HOME` apuntando al NDK. Primero las bibliotecas y luego el APK, desde `android`:

```bash
cargo ndk -t x86_64 -t arm64-v8a -P 26 -o android/app/src/main/jniLibs build -p caudal-native --lib
```

```bash
.\gradlew.bat assembleDebug
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

- **egui en Android:** winit 0.30 ignora el texto del teclado táctil; se resuelve con un puente propio sobre GameActivity (`android_ime`). Faltan pruebas en teléfono real; plan B: Leptos instalado como PWA.
- **Licencia de datos de mercado:** los planes personales de EODHD y el gratuito de Twelve Data no permiten mostrar datos a terceros. Hace falta un plan comercial antes de abrir el registro público.
- **Límites gratuitos:** la cuota de la API y de Supabase condiciona cuántos usuarios y símbolos se soportan.
