# Fase 8: productividad y datos

## Estado

| Punto | Estado |
|---|---|
| Copia completa (JSON, con papelera), restauración y "Borrar todos mis datos" | Hecho |
| Exportar movimientos a CSV para Excel | Hecho |
| Importar CSV con columnas detectadas, vista previa y repetidos apartados | Hecho |
| Reglas de categoría y plantillas de movimiento, sincronizadas | Hecho |
| Comprobantes en Supabase Storage, por usuario, también sin red | Hecho, falta activarlo |
| Actividad reciente: qué se creó, editó o borró y cuándo | Hecho |
| PIN por dispositivo y modo privado | Hecho |
| Avisos: cuotas vencidas o por vencer, presupuestos excedidos, precios sin actualizar | Hecho |
| Abrir y guardar archivos en Windows (diálogo del sistema), Android (selector de documentos) y web | Hecho |
| Notificaciones nativas de Android | Pendiente: Fase 9 |

## Para activarlo

1. En el SQL Editor de Supabase, ejecutar `supabase/migrations/0003_data.sql`. Crea las tablas de reglas, plantillas y comprobantes, el bucket privado `receipts` (10 MB por archivo, solo imágenes y PDF) y sus políticas.
2. Ejecutar de nuevo `supabase/tests/rls.sql`. Debe responder "Success. No rows returned".

Sin la migración, las reglas, las plantillas y los comprobantes funcionan en el dispositivo, pero no suben. La cola los guarda hasta que existan las tablas.

## Criterio de salida

**Exportar, borrar y restaurar deja una cuenta de prueba idéntica a la original.** Se comprobó de tres formas:

- **Prueba automática** (`backup::tests`): una cuenta con todos los datos de ejemplo pasa por exportar, borrar y restaurar. La segunda exportación es igual a la primera, tabla por tabla y fila por fila, papelera incluida. El inicio muestra las mismas cifras.
- **Android** (emulador): se exportó la copia a Descargas con el diálogo del sistema (40 KB, 83 registros). Luego se borró todo, con lo que el inicio quedó en 0 CLP, y se restauró eligiendo el archivo con el selector. Volvieron las cuentas, los movimientos y los presupuestos.
- **Web:** el mismo recorrido dio "Restaurados: 83 · Omitidos: 0", y el patrimonio volvió a 11.171.380 CLP.

## Cómo funciona

- **Copia completa:** todas las tablas, también lo que está en la papelera, sin `user_id` ni marcas del servidor. Al restaurar:
  - Cada fila se valida contra su modelo y se vuelve a guardar con una marca nueva, así gana a lo borrado.
  - Lo que no está en la copia no se toca.
  - Una copia de otra cuenta se puede restaurar: cambia el dueño y el perfil toma el id de la cuenta actual.
  - Los comprobantes de otra cuenta se omiten, porque su archivo vive en la carpeta de esa cuenta.
- **CSV:**
  - **Exportar:** en español usa `;` y coma decimal, y en inglés `,` y punto; con BOM, para que Excel lea las tildes.
  - **Importar:** acepta UTF-8 o Latin-1 y detecta el separador. Adivina las columnas por su nombre (fecha, monto o cargo/abono, descripción, categoría) y el orden de la fecha por los valores. También entiende "$ 5.990", "(12,50)" y "500-".
  - **Repetidos:** un movimiento con la misma fecha, cuenta y monto que uno guardado se marca y no se importa, salvo que se pida. Dos cafés iguales el mismo día en el archivo, con uno ya guardado, dejan uno nuevo.
- **Reglas:** "si la descripción contiene…", sin distinguir mayúsculas ni tildes. Gana la regla más específica (el texto más largo). Solo sugieren categoría si el campo está vacío, y se aplican al escribir la descripción y al importar.
- **Plantillas:** botones arriba del formulario de movimiento nuevo. "Guardar como plantilla" crea una a partir de lo escrito.
- **Comprobantes:**
  - El archivo se guarda en el dispositivo y entra en una cola que se sube a `receipts/{user_id}/{id}` en la siguiente sincronización.
  - Mientras espera, el comprobante se ve igual y muestra "pendiente de subir".
  - El tipo se reconoce por el contenido (JPG, PNG, WEBP, HEIC, PDF), con un tope de 10 MB.
  - Las imágenes se muestran en la app. "Abrir" las pasa al visor del sistema y "Guardar una copia" las descarga.
- **Actividad:** el id (UUID v7) dice cuándo se creó cada registro y `updated_at` cuándo cambió, así que no hace falta guardar un historial aparte. Lo generado solo, como los periódicos, no aparece.
- **Privacidad:**
  - **PIN:** de 4 a 8 dígitos. Se guarda solo su hash (50.000 rondas de SHA-256 con sal) y solo en ese dispositivo. Si se olvida, se cierra sesión y se entra con la contraseña.
  - **Modo privado:** muestra "•••••" en lugar de los montos, también en los ejes de los gráficos.

## Decisiones

- **El PIN es una cortina, no cifrado.** Protege de quien toma el teléfono prestado. Los datos siguen en la base local del dispositivo y el sistema operativo es quien los protege.
- **Borrar un comprobante lo manda a la papelera**, pero el archivo queda en Storage para poder recuperarlo. Borrar los archivos de verdad irá con la eliminación de cuenta (Fase 9).
- **Archivos en Android:** `MainActivity` abre el selector de documentos del sistema y pasa los bytes a Rust por JNI (`nativeFileOpened`, `nativeFileSaved`). No pide permisos de almacenamiento.
- **Notificaciones nativas de Android:** quedan para la Fase 9. Necesitan tareas programadas en segundo plano y un permiso que la Fase 0 no validó. Mientras tanto, los avisos se ven dentro de la app.

## Pendiente

- Probar en un teléfono real el selector de documentos y la cámara como fuente de comprobantes.
- Notificaciones nativas de Android.
- Borrar los archivos de Storage al eliminar la cuenta (Fase 9).
