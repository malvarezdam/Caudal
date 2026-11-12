# Fase 3: app base en las tres plataformas

## Estado

| Punto | Estado |
|---|---|
| Casos de uso compartidos (`caudal-app-state`) | Hecho: formularios, formatos, vistas, idiomas, sincronización |
| Windows (egui) | Hecho y revisado con capturas automáticas |
| Android (egui) | Hecho y probado en emulador: márgenes, teclado numérico, guardar, persistencia |
| Web (Leptos) | Hecho y probado en navegador; instalable (manifiesto y service worker) |
| Pantallas | Acceso, bienvenida, inicio, movimientos, cuentas, categorías y etiquetas, papelera, ajustes |
| Idiomas | Español e inglés, con separadores numéricos de cada idioma |
| Sesión recordada | Windows: Administrador de credenciales. Android: almacenamiento privado. Web: `localStorage` |
| Registro e inicio de sesión reales | **Pendiente**: prueba final de la Fase 9 |
| Plantilla de correo de recuperación | **Pendiente**: la configura el usuario en Supabase |

## Cómo se verificó

- **app-state:** 25 pruebas de formatos, formularios, vistas, bienvenida y papelera. El total del workspace es de 87 pruebas.
- **Windows:** modo demostración más `CAUDAL_SHOTS`, que recorre las pantallas a 1100×720 y 390×780 y guarda capturas PNG. Las capturas revelaron cuatro fallos ya corregidos:
  - Contenido cortado a la derecha, porque `set_max_width` también agrandaba el área.
  - Símbolos que la fuente no tiene: ◀ ▶ → ⟳.
  - Botones del diálogo recortados. El área de desplazamiento calculaba su alto con el tamaño del fotograma anterior; se resolvió con `min_scrolled_height`.
  - Datos de ejemplo con efectivo negativo.
- **Android (emulador API 37):**
  - Contenido bajo la barra de estado y la de gestos; corregido con las zonas del sistema que envía `MainActivity`.
  - Teclado numérico en montos (`inputType=0x3002`).
  - Diálogo anclado arriba, visible con el teclado abierto.
  - Escritura de un monto, guardado y actualización del total del mes.
  - Datos que siguen ahí tras cerrar y reabrir la app.
- **Web:** inicio con los mismos números que la versión nativa, sin desbordamientos a 375 y 1280 px. Además:
  - Alta de un gasto escribiendo "4.990", que se interpreta como 4990 en español.
  - Persistencia tras recargar.
  - Borrar, papelera y recuperar.
  - Cambio a inglés (menús y "1,207,080 CLP").
  - Validaciones del registro sin red; service worker registrado.

## Decisiones

- **Modo demostración solo en desarrollo:**
  - Windows: `CAUDAL_DEMO=1`.
  - Android: archivo `files/demo` en la carpeta privada.
  - Web: `?demo`.

  Abre una base aparte con datos de ejemplo y no sincroniza.
- **Bienvenida tras la primera sincronización.** Un dispositivo nuevo baja antes el perfil que ya pueda existir.
- **Categorías sugeridas con id derivado del usuario** (UUID v5): dos dispositivos sin conexión no las duplican.
- **Sesión guardada mínima:** token de renovación, id y correo. El token de acceso se pide de nuevo al abrir.
- **Resincronización completa diaria.** Mitiga el posible salto de cursor descrito en la Fase 2.
- **Android compila en una carpeta sin tildes** (`CARGO_TARGET_DIR`). El enlazador no admite la "í" de "Economía" en la lista de archivos que recibe.

## Lo que debe hacer el usuario en Supabase

En **Authentication → Emails → Reset password**, el mensaje debe mostrar el código, como en Notable:

```html
<h2>Recuperar tu contraseña de Caudal</h2>
<p>Escribe este código en la app: <strong>{{ .Token }}</strong></p>
```

## Pendiente para fases siguientes

- Gráficos (Fase 5).
- Notificaciones nativas (Fase 8).
- Pantalla de acceso con contraseña visible u oculta, y botón atrás de Android, como mejoras menores.
