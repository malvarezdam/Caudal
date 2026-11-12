# Fase 7: actualización automática de precios

## Estado

| Punto | Estado |
|---|---|
| `PriceProvider` con EODHD (acciones y ETF) y CoinGecko (criptos) | Hecho |
| Tipos de cambio: Frankfurter (BCE) y un respaldo para las monedas que el BCE no publica | Hecho |
| Worker que consulta una vez cada símbolo en poder de algún usuario y guarda un precio por día | Hecho |
| Historial de 400 días para símbolos y monedas nuevos, y relleno de huecos | Hecho |
| GitHub Actions dos veces al día, con registro en `price_runs` y aviso si falla una fuente | Hecho, falta activarlo |
| Los clientes bajan solo los precios de sus activos y los guardan para usar sin red | Hecho |
| La app muestra la fecha del precio y avisa si quedó sin actualizar | Hecho |
| Alertas de precio, evaluadas con cada precio nuevo | Hecho |
| Siete días seguidos de ejecuciones correctas | Pendiente: empieza al activarlo |

## Para activarlo

1. **Esquema:** en el SQL Editor de Supabase, ejecutar `supabase/migrations/0002_prices.sql`.
2. **Prueba de seguridad:** ejecutar de nuevo `supabase/tests/rls.sql`. Debe responder "Success. No rows returned".
3. **Secretos:** en GitHub, en *Settings → Secrets and variables → Actions*, crear:
   - `SUPABASE_SECRET_KEY`: la clave secreta del proyecto (*Project Settings → API Keys → Secret keys*). Solo vive ahí: nunca en el código ni en el chat.
   - `EODHD_API_KEY`: sin ella se actualizan criptos y divisas, y las acciones quedan con su último precio.
   - `COINGECKO_API_KEY` (opcional): una clave demo gratuita sube el límite por minuto.
4. **Primer commit:** el flujo programado solo corre desde la rama principal, así que empieza tras el commit final. También se puede lanzar a mano desde *Actions → Precios → Run workflow*.

## Cómo funciona

**Worker** (`apps/price-worker`):

1. Pide a Supabase los símbolos en poder de algún usuario con `held_symbols()`. La función devuelve cada símbolo una vez y no dice de quién es; solo la puede llamar la clave secreta.
2. Pide el último precio de cada uno: EODHD agrupa hasta 15 por consulta y CoinGecko hasta 200.
3. Si un símbolo es nuevo o tiene un hueco de más de 5 días, pide su historial. Para no gastar la cuota de una vez, hace como máximo 25 historiales por ejecución.
4. Guarda con upsert por `(proveedor, símbolo, fecha)`: la segunda ejecución del día sobrescribe el precio de ese día.
5. Hace lo mismo con los tipos USD → cada moneda en uso.
6. Deja un registro en `price_runs`. Si una fuente entera falla, la ejecución termina con error y GitHub avisa por correo. Un símbolo suelto sin dato queda solo como aviso.

**Clientes:** después de sincronizar, como mucho una vez por hora, bajan desde Supabase solo los precios de sus símbolos y los tipos de sus monedas, desde el último dato conocido. Los guardan en el dispositivo, así que sin red la app sigue mostrando el último precio con su fecha. Nunca llaman a los proveedores.

**Valoración:** cantidad × último precio. Un precio en otra moneda se convierte con el tipo de cambio de su fecha; por ejemplo, bitcoin cotiza en dólares y un usuario puede registrarlo en pesos.

## Comprobación

- **Fuentes reales.** Con `--dry-run` y la clave demo de EODHD se obtuvieron AAPL, TSLA y VTI, bitcoin y ethereum. Llegaron tipos para BRL, EUR y JPY (BCE) y para CLP, COP y PEN (respaldo), con su historial. Un id de cripto inexistente queda como "sin dato" sin detener nada.
- **Pruebas del worker** con fuentes y almacén simulados:
  - Una sola consulta por símbolo.
  - Historial solo para lo nuevo o con hueco.
  - El límite de historiales deja el resto para después.
  - Si una fuente cae, se guarda lo demás y la ejecución falla.
  - Sin clave de EODHD solo hay un aviso.
  - Los importes van como texto en el upsert, y la clave nueva no se manda como JWT.
- **Seguridad.** `rls.sql` comprueba que un usuario no puede llamar a `held_symbols()` ni a `currencies_in_use()`, ni leer `price_runs`, y que `price_alerts` tiene RLS propia. Se ejecutó en PGlite con 10 sabotajes: todos se detectaron.
- **Cliente.** Una prueba contra un servidor simulado comprueba tres casos:
  - El primer pedido trae 400 días y el siguiente solo desde el último dato menos 3 días.
  - Un cierre corregido reemplaza al anterior.
  - Sin red, se conserva lo guardado.
- **Las tres interfaces** muestran las alertas cumplidas en el inicio y en Inversiones. "Entendido" pausa la alerta. En la web se creó una alerta y se evaluó al instante.

## Decisiones

- **Respaldo de tipos de cambio.** Frankfurter usa los tipos del BCE, que no incluyen CLP, ARS, COP, PEN ni UYU. Para esas monedas se usa el conjunto diario [`@fawazahmed0/currency-api`](https://github.com/fawazahmed0/exchange-api), de dominio público y servido por jsDelivr. Su historial se pide semanal; basta para valorar meses pasados.
- **Criptos siempre en dólares.** La app convierte a la moneda con que cada usuario registró el activo.
- **Moneda por bolsa.** La moneda la define el sufijo del símbolo (`.US` USD, `.SN` CLP, `.XETRA` EUR…), no lo que escribió un usuario. Londres cotiza en peniques: se divide por 100 y se guarda en GBP.
- **Cierre sin ajustar.** Los splits ya están en las operaciones del usuario; usar el cierre ajustado los contaría dos veces.
- **Sin instantáneas de cartera.** El valor de cualquier fecha se reconstruye con las operaciones y el historial diario de precios, así que guardarlo sería un dato derivado como verdad.
- **Alertas en el cliente.** Se evalúan con cada precio nuevo que llega al dispositivo. Avisan dentro de la app; las notificaciones del sistema son de la Fase 8. Solo informan: nunca operan.
- **Copia local:** guarda los últimos 400 días, suficientes para los gráficos de 12 meses.

## Riesgos

- **Cuota de EODHD:** el plan gratuito da 20 consultas al día, y cada símbolo cuenta una. Para más de unos pocos símbolos hace falta un plan pago. Para abrir el registro público, además, hace falta un plan comercial: los personales no permiten mostrar datos a terceros.
- **CoinGecko gratuito:** el límite es por minuto, y el worker espera entre historiales.
- **GitHub desactiva los flujos programados** de un repositorio sin actividad durante 60 días.
