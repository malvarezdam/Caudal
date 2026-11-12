---
description: Revisar visualmente las pantallas de Windows (capturas automáticas) y de la web (modo demostración)
---

1. **Windows y diseño móvil:** compilar y recorrer las pantallas con datos de ejemplo. Las capturas quedan en la carpeta indicada; la app se cierra sola.
   - PowerShell: `cargo build -p caudal-native; $env:CAUDAL_DEMO="1"; $env:CAUDAL_SHOTS="$env:LOCALAPPDATA\caudal-shots"; .\target\debug\caudal.exe`
   - Si hace falta partir de datos de ejemplo limpios, borrar antes `%APPDATA%\Caudal\caudal-demo.db`.
   - Para que una pantalla nueva aparezca en el recorrido, añadirla en `apps/native/src/shots.rs`.
2. **Web:** desde `apps/web`, con `NO_COLOR=true` (Trunk falla con `NO_COLOR=1`), correr `trunk serve --port 8088` y abrir `http://localhost:8088/?demo`.
3. Mirar cada captura buscando texto cortado, montos sin formato (deben pasar por `format_money` o `format_estimate`), cadenas sin traducir y botones fuera de la pantalla en el ancho móvil (390 px).
4. Comprobar que el cambio está igual en egui y en Leptos.
