---
name: study-workflow
description: Lee las convenciones del workflow personal (PROJECT-CONVENTIONS.md) y da un vistazo ligero al repo actual. Se dispara automáticamente al iniciar sesión (hook SessionStart) y manualmente vía /study. Nunca hace un dump completo del código — solo un índice + un tree().
---

# study-workflow

## Qué hace
1. Lee `PROJECT-CONVENTIONS.md` completo (es corto, cabe siempre en contexto).
2. Si el MCP `workflow` está disponible, llama `tree(depth=2)` UNA vez sobre el
   directorio actual.
3. Si el repo tiene su propio README, lo lee también (una sola llamada).
4. NO lee código fuente completo en este paso — eso ocurre bajo demanda cuando
   una tarea concreta lo pida, siguiendo las reglas de `read`/`grep` del MCP.

## Cuándo se dispara
- **Manual, pero úsalo proactivamente**: comando `/study`. El hook
  `SessionStart` (ver `hooks/session-start.sh`) solo inyecta
  `PROJECT-CONVENTIONS.md` como texto — un hook no puede invocar tools ni
  skills, así que el paso 2 (`tree`) y el paso 3 (README) NUNCA ocurren solos.
  Invoca `/study` tú mismo justo después de clonar/instalar el workflow, y de
  nuevo tras reestructurar carpetas o cambiar convenciones. No esperes a que
  el usuario pregunte si lo estás usando.

## Salida esperada
Un resumen de 3-5 líneas, no más: tipo de proyecto detectado, si respeta las
convenciones (monorepo, `tools/package.py` y `tools/build.py` presentes, puerto
fijo), y cualquier desviación notable. Nunca repetir de vuelta el contenido de
`PROJECT-CONVENTIONS.md` — ya está en contexto.
