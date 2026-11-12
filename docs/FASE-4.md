# Fase 4: deudas y registros periódicos

## Estado

| Punto | Estado |
|---|---|
| Estado de deudas (`caudal-core::debts`) | Hecho: cuotas, pagos parciales, interés simple, vencimientos |
| Motor de recurrencia (`caudal-core::recurrence`) | Hecho: diaria, semanal, mensual, anual, cada N, último día del mes, fin |
| Contactos con saldo neto | Hecho, por moneda |
| Deudas en ambos sentidos, con cuotas e interés opcional | Hecho |
| Pago que mueve una cuenta | Hecho, solo con cuentas de la misma moneda |
| Gastos compartidos que crean deudas | Hecho: reparto en partes iguales |
| Periódicos automáticos o por confirmar | Hecho |
| Vista de suscripciones con costo mensual | Hecho |
| Calendario de los próximos 30 días | Hecho: periódicos, cuotas y vencimientos |
| Windows, Android y web | Hecho; las dos interfaces comparten la lógica |

## Criterios de salida

- **Un pago parcial actualiza la deuda y la cuenta.** Probado en `app-state` y en el navegador: un pago de 30.000 baja la deuda de 90.000 a 60.000 y sube la cuenta exactamente 30.000. Los ingresos del mes no cambian.
- **Una regla procesada en dos dispositivos sin conexión no duplica.** Probado en `caudal-sync` y en `app-state`: tres ocurrencias en cada dispositivo convergen a tres en total, y la que se confirmó en uno sigue confirmada en ambos.

## Decisiones

- **El dinero de una deuda es un ajuste de saldo, no un ingreso ni un gasto.** Prestar, devolver o cobrar mueve la cuenta, pero no infla los totales del mes.
- **En un gasto compartido, solo mi parte cuenta como gasto.** Lo adelantado por los demás sale de la cuenta como ajuste, y cada persona queda con una deuda por su parte.
- **Ocurrencias periódicas:**
  - El id deriva de regla + fecha (UUID v5), así que dos dispositivos generan el mismo registro.
  - Se guardan con la marca de reloj más baja posible (`Repository::save_generated`), así cualquier confirmación, edición o borrado del usuario gana siempre a una regeneración. La prueba falla si se usa una marca normal.
  - Se generan al cargar los datos, sin esperar a la red.
  - Lo borrado (omitido) no se vuelve a generar.
- **Con cuotas, el total es la suma de las cuotas.** Sin cuotas, es capital más interés simple anual desde la fecha de inicio.
- **El plan de cuotas se define al crear la deuda.** Al editar se cambian los datos, no el plan.
- **Una regla que empieza en el pasado genera las ocurrencias atrasadas**, con un tope de 400 por regla.

## Pendiente

- Repartos no iguales en gastos compartidos.
- Editar el plan de cuotas de una deuda existente.
- Notificaciones de vencimientos (Fase 8).
