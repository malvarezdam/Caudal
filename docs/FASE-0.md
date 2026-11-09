# Fase 0: resultados de las validaciones

Fecha: 2026-11-09. Pruebas locales en Windows 11 y emulador Android (API 37, x86_64).

## Estado de la fase

| Punto | Estado |
|---|---|
| Workspace y Git | Hecho; repositorio `malvarezdam/Caudal` |
| Compilación Windows, WASM y Android | Hecho en local y en CI |
| CI | En verde en las tres plataformas (2026-11-09) |
| Supabase desarrollo | Hecho; conexión y clave pública comprobadas. Nuevas tablas sin exposición automática y con RLS automática |
| Supabase producción | Se crea en la Fase 9, para no gastar un proyecto gratuito que se pausaría sin uso |
| Decisión Android | Tomada, con pruebas pendientes en teléfono real |
| Decisión fuentes de precios | Tomada para desarrollo; licencia pública pendiente |

## Web (Leptos + WASM)

**Resultado: aprobada.** En navegador, IndexedDB guardó y releyó `Economía ñandú`, y `reqwest` completó una petición HTTP real. Sin errores en consola.

Trunk debe ejecutarse desde `apps/web`, y falla si `NO_COLOR=1`; usar `NO_COLOR=true`.

## Android (egui)

**Decisión: seguir con egui**, usando GameActivity y un puente de texto propio. El plan B (PWA) se activa si falla alguna prueba pendiente que no tenga arreglo razonable.

| Prueba | Resultado |
|---|---|
| Arranque, renderizado, toques | Correcto |
| Teclado aparece al tocar un campo | Correcto |
| Texto del teclado sin puente | **Falla**: winit 0.30 ignora `TextEvent` |
| Escritura con puente | Correcto |
| Sugerencia del teclado | Correcto, sin duplicar texto |
| Borrar | Correcto |
| Texto separado entre campos | Correcto |
| Tildes y ñ con teclado en español | **Sin probar**; el diff admite Unicode en test unitario |
| Editar texto ya existente o mover el cursor | **Sin probar** |
| Rotación | **Sin probar** |
| Teléfono físico | **Sin probar** |

Problemas confirmados, a resolver en la Fase 3:
- La interfaz se dibuja bajo la barra de estado: faltan márgenes seguros.
- El teclado tapa la mitad inferior y la interfaz no se reajusta; un campo bajo puede quedar oculto.
- El teclado siempre es de texto (`inputType=0x1`); el monto no abre el teclado numérico.
- Hay que volver a evaluar el puente si eframe adopta winit 0.31, cuyo soporte de IME puede hacerlo innecesario.

El APK de depuración pesa 315 MB porque incluye símbolos; el tamaño de release aún no está medido.

## Fuentes de precios

Datos leídos el 2026-11-09 en las páginas oficiales; pueden cambiar.

| Fuente | Gratis | Plan de pago más barato | Cobertura | Uso público |
|---|---|---|---|---|
| EODHD | 20 llamadas/día | 19,99 USD/mes, 100 000/día (uso personal) | Global, 150 000+ tickers | Los planes personales prohíben dar acceso a terceros; existen planes comerciales, precio sin verificar |
| Twelve Data | 800/día, 8/min | La página muestra datos contradictorios (29 o 79 USD/mes) | Gratis: 3 mercados; global desde el plan Grow | Prohíbe uso comercial del plan gratis; redistribuir requiere añadido o acuerdo |
| Alpha Vantage | 25/día | 49,99 USD/mes | No indicada | Términos no verificados |
| Finnhub | No se pudo leer | No se pudo leer | No se pudo leer | No se pudo leer |
| CoinGecko (criptos) | 10 000/mes, 100/min, con atribución | 35 USD/mes, licencia comercial | Criptos | Permite integrarlo en un producto con atribución visible |
| Frankfurter (divisas) | Sin cuotas, con límite antiabuso | No tiene | 219 monedas | Uso comercial permitido |

**Decisión**
- Acciones y ETF: **EODHD** detrás de `PriceProvider`, por ser la única con cobertura global barata y una sola llamada por precio. Desarrollo con su plan gratis (20/día).
- Criptos: **CoinGecko**, mostrando "Data provided by CoinGecko" con enlace.
- Divisas: **Frankfurter**.
- **Antes de abrir el registro público** hay que contratar un plan comercial de datos de acciones y confirmar por escrito que permite mostrarlos a usuarios finales. Esta condición se comprueba en la Fase 9.
