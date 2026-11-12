# Caudal

Finanzas personales para **Windows, Android y web**, escritas en Rust. Registra cuentas, movimientos, deudas, presupuestos, metas e inversiones. Funciona sin internet y sincroniza cuando vuelve la conexión.

[![CI](https://github.com/malvarezdam/Caudal/actions/workflows/ci.yml/badge.svg)](https://github.com/malvarezdam/Caudal/actions/workflows/ci.yml)
[![Precios](https://github.com/malvarezdam/Caudal/actions/workflows/prices.yml/badge.svg)](https://github.com/malvarezdam/Caudal/actions/workflows/prices.yml)

## Qué hace

- **Movimientos y cuentas:** un movimiento solo exige monto y fecha; todo lo demás es opcional y editable. Lo borrado va a la papelera.
- **Varias monedas:** cada cuenta y activo tiene la suya, y el patrimonio se ve en tu moneda principal.
- **Deudas y gastos compartidos:** a quién le debes, quién te debe, cuotas y pagos.
- **Periódicos:** arriendo, sueldo o suscripciones, que aparecen solos sin duplicarse entre dispositivos.
- **Informes, presupuestos y metas:** gráficos por mes o año, arrastre de sobrante y proyección de ahorro.
- **Inversiones:**
  - Acciones, ETF y criptos de bolsas de todo el mundo, o cualquier activo con precio manual.
  - Costo promedio, ganancia realizada y no realizada.
  - Precios actualizados solos dos veces al día, y alertas de precio.
- **Copia completa, CSV y comprobantes:** exporta y restaura todo, importa la cartola del banco y adjunta fotos o PDF a cada movimiento.
- **Reglas y plantillas:** categoría sugerida según la descripción y movimientos frecuentes con un toque.
- **Privacidad:** PIN por dispositivo y modo privado que oculta los montos.
- **Tus datos son tuyos:** tú decides cómo registrar tu dinero. La app registra y muestra; **no da recomendaciones de inversión** ni ejecuta pagos u operaciones.

## Cómo está hecho

| Parte | Tecnología |
|---|---|
| Windows y Android | [egui](https://github.com/emilk/egui) / eframe |
| Web | [Leptos](https://leptos.dev) en WASM, instalable como PWA |
| Datos | Supabase: Postgres con RLS en todas las tablas y Auth |
| Offline | SQLite en nativo e IndexedDB en web, con cola de salida; gana el último cambio por registro |
| Precios | Worker diario en GitHub Actions: EODHD, CoinGecko y Frankfurter |

```
crates/core          Dominio y cálculos puros (dinero con rust_decimal, nunca f64)
crates/sync          Motor de sincronización con relojes lógicos híbridos
crates/supabase      Cliente propio de Auth y PostgREST
crates/store-*       Almacén local: SQLite e IndexedDB
crates/app-state     Casos de uso y vistas compartidas por ambas interfaces
apps/native          Interfaz egui
apps/web             Interfaz Leptos
apps/price-worker    Actualización de precios y tipos de cambio
supabase/            Esquema SQL y prueba de seguridad entre usuarios
```

## Desarrollo

```bash
cargo test --workspace
cargo run -p caudal-native          # Windows; con CAUDAL_DEMO=1, datos de ejemplo
cd apps/web && trunk serve --port 8088   # web; abrir con ?demo para datos de ejemplo
```

El plan del proyecto está en [docs/ROADMAP.md](docs/ROADMAP.md), y el detalle de cada fase en `docs/FASE-*.md`. Las reglas para agentes de código (Claude Code, Antigravity) están en [AGENTS.md](AGENTS.md), y las tareas listas para delegar, en [docs/DELEGAR.md](docs/DELEGAR.md).

## Estado

En desarrollo: las fases 0 a 8 están implementadas y la 9 (endurecimiento y publicación) está por hacer. Antes de abrir el registro público hace falta una licencia comercial de datos de mercado.
