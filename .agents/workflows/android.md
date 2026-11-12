---
description: Compilar el APK e instalarlo en el emulador de Android en modo demostración
---

1. Bibliotecas Rust, en PowerShell desde la raíz:
   `$env:CARGO_TARGET_DIR="$env:LOCALAPPDATA\caudal-android-target"; cargo ndk -t x86_64 -t arm64-v8a -P 26 -o android/app/src/main/jniLibs build -p caudal-native --lib`
2. APK, desde `android`: `.\gradlew.bat assembleDebug`
3. Emulador: `& "$env:ANDROID_HOME\emulator\emulator" -avd Medium_Phone_API_37.0`
4. Instalar y abrir en modo demostración, con `adb` de `$env:ANDROID_HOME\platform-tools`:
   - `adb install -r app/build/outputs/apk/debug/app-debug.apk`
   - `adb shell run-as app.caudal touch files/demo`
   - `adb shell am start -n app.caudal/.MainActivity`
5. Capturas: `adb exec-out screencap -p > captura.png`. En Git Bash, anteponer `MSYS_NO_PATHCONV=1` a los comandos con rutas `/sdcard/...`.
6. `games-activity` debe tener la misma versión que el GameActivity de `android-activity` (hoy 4.4.0).
