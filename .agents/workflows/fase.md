---
description: Ejecutar las tareas de una fase anotadas en docs/DELEGAR.md, una por una, verificando cada una
---

Uso: `/fase` seguido de la fase o de las tareas (por ejemplo, "Fase 9" o "D1, D2 y D4").

1. Leer `AGENTS.md`, `docs/DELEGAR.md`, `docs/ROADMAP.md` y el `docs/FASE-*.md` más reciente.
2. Tomar las tareas pedidas que estén "Libre", en el orden del tablero. Una tarea cuyo criterio depende de otra (por ejemplo, D7 necesita D6) va después.
3. Para cada tarea:
   1. Cambiar su estado en `docs/DELEGAR.md` a `En curso: Antigravity`.
   2. Implementarla tocando solo los archivos que indica. Si hace falta tocar otro, detenerse y explicar por qué al usuario.
   3. Escribir las pruebas que demuestren el criterio de terminado.
   4. Correr `/verificar` hasta que todo pase. Si la tarea cambia pantallas, correr también `/revisar-interfaz`; si toca Android, `/android`.
   5. Cambiar su estado a `Hecha` con una línea de resumen y la evidencia (pruebas que pasan, capturas revisadas).
4. Escribir lo hecho en `docs/FASE-N.md` (crearlo si no existe), con el mismo formato de las fases anteriores: estado por punto, criterio de salida, cómo funciona, decisiones y pendientes.
5. No hacer commits ni push. No ejecutar nada contra el Supabase real ni usar credenciales del usuario.
6. Terminar con un informe para el usuario, que lo pasará a Claude Code para revisar:
   - Qué tareas quedaron hechas y cuáles no, con el motivo.
   - Archivos tocados por tarea.
   - Comandos de verificación y su resultado.
   - SQL que el usuario debe ejecutar en el Supabase real, si lo hay.
   - Dudas o decisiones que tomaste por tu cuenta.
