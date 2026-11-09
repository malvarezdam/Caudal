# Fase 1: núcleo y base de datos

## Estado

| Punto | Estado |
|---|---|
| Modelo de dominio (`caudal-core::model`) | Hecho |
| Moneda, cambios, saldos y resúmenes | Hecho, con pruebas unitarias y de propiedades |
| Migración con RLS (`supabase/migrations/0001_schema.sql`) | Aplicada en Supabase de desarrollo (2026-11-09) |
| Prueba de seguridad (`supabase/tests/rls.sql`) | Pasa en PGlite y en Supabase de desarrollo (2026-11-09) |

## Decisiones

- **Sin claves foráneas entre tablas de usuario.** La sincronización offline puede subir filas en cualquier orden. Solo `user_id` referencia a `auth.users`, para que eliminar una cuenta borre sus datos.
- **"Gana el último cambio" también en el servidor.** Un trigger ignora las actualizaciones con `updated_at` igual o anterior, lo que hace idempotentes los reintentos, y asigna `server_seq`, el cursor de descarga.
- **`updated_at` es texto con collation `C`**, para que el reloj lógico híbrido se compare byte a byte.
- **Sin permiso DELETE para clientes.** El borrado es lógico; el borrado real solo ocurre al eliminar la cuenta.
- **`numeric(28,12)` para importes**, siempre representable en `rust_decimal`.
- **Sin catálogo global de activos.** Cada usuario guarda `provider` y `provider_symbol`; el worker reúne los símbolos únicos.
- **Lo no convertible no se oculta ni se inventa.** Saldos y resúmenes devuelven la lista `unconverted`, para avisar en la interfaz.

## Pendiente para la Fase 2

- PostgREST devuelve `numeric` como número JSON, y `rust_decimal` con `serde-str` espera texto. El cliente debe aceptar ambos sin pasar por `f64`.
- `server_seq` puede saltarse una fila si dos transacciones confirman en distinto orden. Es improbable por usuario; el motor debe tolerarlo, por ejemplo releyendo un margen hacia atrás.

## Cómo se verificó

El esquema se ejecutó en PGlite (Postgres en WASM) con `auth.users`, `auth.uid()` y roles simulados. La prueba detectó siete fallos introducidos a propósito: RLS desactivada, política abierta, DELETE concedido, inserción sin comprobar dueño, lectura anónima, trigger eliminado y tabla nueva sin cubrir.
