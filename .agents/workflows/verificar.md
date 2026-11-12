---
description: Comprobar formato, clippy (nativo y WASM) y todas las pruebas antes de dar una tarea por terminada
---

Ejecutar desde la raíz del repositorio. Si un paso falla, corregirlo antes de seguir y no dar la tarea por terminada.

1. Formato:
   `cargo fmt --all -- --check`
2. Clippy nativo, sin avisos:
   `cargo clippy --workspace --all-targets -- -D warnings`
3. Clippy WASM, sin avisos:
   `cargo clippy --target wasm32-unknown-unknown -p caudal-web -p caudal-core -p caudal-app-state -p caudal-supabase -p caudal-store-idb -- -D warnings`
4. Todas las pruebas:
   `cargo test --workspace`
5. Si la tarea tocó `apps/native` o `android/`, compilar Android en PowerShell (la ruta del proyecto tiene una "í" y el enlazador falla sin `CARGO_TARGET_DIR`):
   `$env:CARGO_TARGET_DIR="$env:LOCALAPPDATA\caudal-android-target"; cargo ndk -t x86_64 -t arm64-v8a -P 26 -o android/app/src/main/jniLibs build -p caudal-native --lib`
6. Si la tarea tocó SQL, probar `supabase/tests/rls.sql` con la nueva migración (ver `/nueva-tabla`) y avisar al usuario de qué archivo debe ejecutar en el SQL Editor.
7. Informar cuántas pruebas pasan y qué no se pudo comprobar.
