# Fase 2: autenticación y sincronización offline

## Estado

| Punto | Estado |
|---|---|
| Reloj lógico híbrido (`caudal-core::hlc`) | Hecho |
| Motor de sincronización y `LocalStore` (`caudal-sync`) | Hecho |
| Almacén SQLite (Windows y Android) | Hecho, con persistencia en disco probada |
| Almacén IndexedDB (web) | Hecho, probado en el navegador y entre recargas |
| Cliente Supabase: registro, login, renovación, recuperación con código, cierre | Hecho, probado contra un servidor simulado |
| Subida y bajada por PostgREST | Hecho, probado contra un servidor simulado |
| Comprobación contra el Supabase real | Se hará en la prueba final de la Fase 9, junto con el registro y el login |
| Guardar la sesión en el almacén seguro del sistema | Pasa a la Fase 3, con la pantalla de login |

## Cómo funciona

1. La app escribe con `Repository::save`, `delete` o `restore`. Cada escritura recibe una marca del reloj lógico híbrido y va al almacén local y a la cola de salida en la misma transacción. La interfaz nunca espera a la red.
2. `sync` sube la cola agrupada por tabla, con la versión actual de cada registro, y la confirma.
3. Después baja, tabla por tabla, las filas con `server_seq` mayor que el último cursor, y las aplica solo si ganan a la versión local.
4. Si algo falla a mitad, basta con repetir: subir y aplicar son idempotentes.
5. Entre intentos fallidos, `Backoff` espera de 2 s a 5 min, con variación aleatoria.

El reloj de cada dispositivo se guarda en el almacén local. Así las marcas siguen creciendo aunque el reloj del sistema retroceda.

## Evidencia

- **400 simulaciones aleatorias** con dos dispositivos, relojes desajustados hasta en horas, cortes de red, fallos a mitad de sincronización y escrituras durante la sincronización. En todas, ambos dispositivos y el servidor terminan iguales, y cada registro conserva la escritura de marca mayor.
- **La simulación detecta errores reales.** La primera versión no detectaba que se ignorara la regla del último cambio, porque nunca se escribía durante una sincronización. Se añadió ese caso y ahora detecta el fallo.
- **Importes exactos.** PostgREST devuelve `numeric` como número JSON; con `serde_json/arbitrary_precision` el texto se conserva y se convierte a cadena. Probado con `1234567890123456.123456789012`.
- **En el navegador**, IndexedDB guardó, encoló, subió y otro dispositivo recibió el registro; los datos persisten tras recargar. La conexión con Supabase respondió correctamente desde la web.

## Prueba contra el Supabase real

La ejecuta el usuario, porque necesita su cuenta:

1. Crear un usuario de prueba en **Authentication → Users → Add user**, marcando la confirmación automática.
2. En una terminal, desde la carpeta del proyecto:

```powershell
$env:CAUDAL_EMAIL = "correo-de-prueba"; $env:CAUDAL_PASSWORD = "contraseña-de-prueba"
cargo run -p caudal-supabase --example sync_check
```

Simula dos dispositivos, comprueba la convergencia, el conflicto, la ausencia de duplicados y un importe de 28 dígitos, y deja los registros de prueba en la papelera.

## Limitaciones conocidas

- **Cursor.** `server_seq` se asigna al escribir, no al confirmar. Si dos subidas del mismo usuario se confirman en orden inverso, una fila puede saltarse hasta la siguiente sincronización completa. Es improbable; la Fase 3 añadirá una resincronización completa periódica, segura porque aplicar es idempotente.
- **Listados en IndexedDB.** `list` lee todos los registros y filtra por tabla. Basta para empezar; si crece, se añadirá un índice por tabla.
- **Plantilla de correo.** Para recuperar la contraseña con código, la plantilla **Reset password** de Supabase debe mostrar `{{ .Token }}`, como en Notable. Se configurará en la Fase 3, junto con la pantalla.
