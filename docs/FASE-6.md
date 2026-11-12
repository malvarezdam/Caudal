# Fase 6: inversiones

## Estado

| Punto | Estado |
|---|---|
| Catálogo de activos con búsqueda por nombre o ticker (acciones, ETF y criptos de varias bolsas) | Hecho |
| Alta manual de cualquier activo, con o sin símbolo | Hecho |
| Compra, venta, dividendo, comisión, split y traspaso entre brokers | Hecho |
| Costo promedio, ganancia realizada y no realizada, por activo y total, en moneda principal | Hecho |
| Vínculo opcional de cada operación con la cuenta que paga o recibe | Hecho |
| Activos sin cotización con precio manual (inmuebles, depósitos, bonos) | Hecho |
| Distribución por tipo y valor de la cartera al cierre de cada mes | Hecho |
| El patrimonio del inicio y de los informes incluye las inversiones | Hecho |
| Windows, Android y web | Hecho |

## Criterio de salida

**Una cartera de referencia en varias monedas, con splits y ventas parciales, produce los resultados calculados de antemano.** Se comprobó de dos formas:

- `crates/core/src/portfolio.rs` tiene una cartera calculada a mano: compras a distintos precios, un split 2×1, una venta parcial, un dividendo, comisiones y un ETF en euros. El resultado es:
  - ACME: 25 acciones, costo 1.376,25, promedio 55,05, ganancia realizada 222,25, dividendos 10, valor 1.500 y ganancia no realizada 123,75.
  - Valor total: 32.655 USD.
- La cartera de ejemplo de la demostración tiene acciones y un ETF en dólares, bitcoin con una venta parcial y un depósito en pesos con precio manual. Una prueba en `app-state` comprueba los resultados contra el cálculo a mano:
  - Valor: (6 × 224 + 3 × 534 + 0,01 × 101.000) × 950 + 2.050.000 = 5.808.200 CLP.
  - Realizado más dividendos: 167.675 CLP.
  - La cuenta en dólares baja de 1.400 a 288 USD por las dos compras de Apple.

Las tres interfaces muestran las mismas cifras. En la web se registró una compra, que movió la cuenta. Luego se borró: la compra y su movimiento fueron a la papelera, y el saldo volvió a su valor.

## Decisiones

- **Método del costo promedio.** Una venta saca de la posición el costo medio de lo vendido; la diferencia es ganancia realizada. Si se vende más de lo que hay, se avisa y no se inventa costo.
- **Precio vigente:** se usa el más reciente entre el de mercado y el manual; si son de la misma fecha, gana el manual. Si no hay ninguno, se usa el de la última operación con precio. Cada posición muestra qué precio usa y de qué fecha.
- **Movimiento ligado a la cuenta:**
  - Una compra o una venta es un ajuste de saldo: mueve la cuenta, pero no cuenta como gasto ni como ingreso del mes.
  - Un dividendo es un ingreso.
  - Una comisión suelta es un ajuste negativo.
- **El movimiento ligado vive y muere con la operación:**
  - Al editar la operación se reescribe el mismo movimiento, conservando su categoría, etiquetas y nota.
  - Si se quita la cuenta, el movimiento se borra.
  - Borrar o recuperar la operación hace lo mismo con su movimiento (`AppCore::delete` y `restore`).
- **La cuenta debe estar en la moneda del activo.** Las operaciones se registran en la moneda del activo.
- **El catálogo es una ayuda, no un límite.** Tiene unos 60 activos frecuentes con su símbolo de EODHD (`AAPL.US`, `SAP.XETRA`, `SQM-B.SN`…) o su id de CoinGecko (`bitcoin`). Cualquier otro símbolo se escribe a mano.
- **Datos de mercado en memoria, solo por ahora.** `AppCore::set_market` carga precios y tipos de cambio de ejemplo en la demostración. En la Fase 7 llegarán desde las tablas `asset_prices` y `fx_rates`.
- **Sin recomendaciones.** La pantalla registra y muestra; no sugiere comprar, vender ni rebalancear.

## Pendiente

- Precios y tipos de cambio reales: Fase 7.
- Probar en el emulador de Android: la biblioteca compila para arm64 y x86_64, y el diseño móvil se revisó con las capturas de `CAUDAL_SHOTS` a 390 px.
