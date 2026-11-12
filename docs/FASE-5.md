# Fase 5: análisis, presupuestos y metas

## Estado

| Punto | Estado |
|---|---|
| Informes por mes o año, con comparación con el periodo anterior | Hecho |
| Gasto por categoría (subcategorías suman a su madre; más de 8 van a "Otras") | Hecho |
| Gráfico de ingresos y gastos de 12 meses, con vista de tabla | Hecho |
| Gráfico de patrimonio al cierre de cada mes | Hecho |
| Presupuestos por categoría o generales, mensuales o anuales, con arrastre de sobrante | Hecho |
| Metas de ahorro con aportes o cuenta asociada, proyección y aporte necesario | Hecho |
| Simulador de pago de deudas | Hecho |
| Inicio con los tres presupuestos más usados | Hecho |
| Windows, Android y web | Hecho |

## Criterio de salida

**Los totales de cada gráfico coinciden con los movimientos filtrados.** Comprobado de tres formas:

- Una prueba con 3 años de datos (unos 50.000 movimientos) verifica que el gasto del informe anual es la suma exacta de los gastos del año.
- La suma de los 12 meses del gráfico es igual al resumen anual.
- Con los datos de ejemplo, el informe da 1.500.000 de ingresos y 607.920 de gastos, que coinciden con los movimientos. El arrastre del presupuesto (41.000) coincide con los sobrantes de septiembre (9.000) y octubre (32.000).

**Sin bloquear la interfaz.** Tiempos con 49.320 movimientos, en compilación optimizada:

| Vista | Tiempo |
|---|---|
| Inicio | 8 ms |
| Informe mensual o anual | 23 ms |
| Lista con búsqueda | 9 ms |

La web recalcula solo cuando cambian los datos. En egui, que redibuja en cada fotograma, un caché de vistas (`apps/native/src/cache.rs`) recalcula solo si cambian los datos, el idioma, el día o los filtros.

## Decisiones

- **Colores de los gráficos validados** con la herramienta de la guía de visualización, en modo claro y oscuro: azul `#2a78d6`/`#3987e5` para ingresos y naranja `#eb6834`/`#d95926` para gastos. Pasan separación para daltonismo (ΔE 24,7) y contraste.
- **Especificaciones de los gráficos:**
  - Barras de 12 px como máximo, con extremo redondeado de 4 px, y líneas de 2 px.
  - Rejilla de 1 px tenue y texto siempre en color de texto, nunca en el de la serie.
  - Leyenda cuando hay dos series, ayuda al pasar el cursor y vista de tabla.
- **Web:** las barras se dibujan con HTML y CSS, y la línea con SVG que no se deforma (`non-scaling-stroke`). Así el texto se ve nítido a cualquier ancho.
- **Patrimonio sin tipo de cambio:** suma las cuentas convertibles y avisa cuáles quedan fuera ("Sin tipo de cambio, no incluye: Ahorro en dólares"), en lugar de dejar el gráfico vacío.
- **Montos calculados redondeados** a los decimales de la moneda (`format_estimate`): un promedio o una estimación no inventan centavos en CLP. Los montos registrados se muestran siempre completos.
- **Un presupuesto nuevo empieza a contar en su mes de creación.** El arrastre mira como máximo 12 periodos atrás.
- **Meta con cuenta asociada:** lo ahorrado es el saldo de la cuenta, y no se registran aportes.
- **Simulador:** interés compuesto mensual sobre el saldo pendiente, con tope de 50 años. Avisa si el pago no cubre el interés y aclara que es un cálculo, no una recomendación.

## Pendiente

- Calendario financiero como vista mensual en cuadrícula; hoy es una lista de 30 días.
- Comparar varias deudas a la vez en el simulador.
