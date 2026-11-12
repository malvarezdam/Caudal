# Caudal: memoria para Claude Code

Las instrucciones del proyecto están en `AGENTS.md`, compartido con Antigravity y otros agentes. Se importa aquí:

@AGENTS.md

## Solo para Claude Code

- Toda regla del proyecto se escribe en `AGENTS.md`, no aquí, para que Antigravity también la vea.
- Cuando el usuario pida una fase, no la programes. Define el alcance y las decisiones, escribe las tareas en `docs/DELEGAR.md` y entrega un prompt corto para pegar en Antigravity, que use `/fase`. Luego revisa lo que haga. Solo programa tú lo que Antigravity no pueda hacer o lo que el usuario pida explícitamente.
- Cuando Antigravity haya trabajado en el repositorio, revisar sus cambios antes de seguir: correr `/verificar` (los mismos pasos de `.agents/workflows/verificar.md`), mirar el diff de los archivos de su tarea en `docs/DELEGAR.md` y comprobar que cumplió el criterio de terminado.
- La revisión visual en Windows se hace con `CAUDAL_SHOTS`, porque el usuario no concedió control de pantalla. La web se revisa en el navegador integrado con `?demo`.
