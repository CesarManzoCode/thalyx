---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitiva, memoria, agente]
---

# Memoria persistente de trabajo

## Función

La IA guarda estado de tareas y lo recupera entre sesiones.

## Implementación

- Base de datos vectorial local (ej. sqlite-vss, LanceDB).
- Flujo:
  1. El agente guarda estado: `SAVE_STATE(task: "refactor-auth", state: {"files_moved": [...], "dependencies_updated": [...]})`.
  2. El estado se serializa en la base de datos vectorial, indexado por embeddings para búsqueda semántica.
  3. Cuando el usuario vuelve, el agente recupera el estado: `RESTORE_STATE(task: "refactor-auth")` y continúa.

## Distinción entre hechos e inferencias (decretado)

La memoria persistente guarda **dos capas separadas**, etiquetadas de forma distinta:

- **Hechos** (inmutables, verificables): qué se instaló, cuándo, qué versión, qué permisos se confirmaron.
  > Ejemplo: "Usuario instaló pyassist-core v2.3.1 el 31 de julio de 2026, confirmó permisos persistentes de red y lectura de /home/user/proyectos."
- **Notas de continuidad / inferencias** (mutables, descartables, propuestas por el agente): posibles siguientes pasos o contexto interpretativo que el agente había considerado.
  > Ejemplo: "Posible siguiente paso: preguntar si el usuario quiere configurar el módulo."

### Razón de esta distinción
Si la memoria solo guardara hechos crudos, el agente tendría que re-inferir desde cero el contexto de continuidad cada vez que retoma una tarea, gastando cómputo innecesariamente en reconstruir algo que ya había razonado antes.

Esta distinción matiza una sugerencia externa que proponía "guardar solo hechos" — se decidió que ambas capas son valiosas, pero deben quedar etiquetadas de forma distinta para que quede claro qué es verificable y qué es interpretación descartable.

## Distinta de la memoria de conversación

La memoria de conversación del [[Agente-Conversacional]] (historial de interacciones) es un uso relacionado pero no idéntico — permite retomar conversaciones días después.

## Qué NO es esta primitiva

Distinta del [[Journal-y-Snapshots|Journal/sistema de snapshots]]: el Journal existe para *deshacer* operaciones (rollback físico); la Memoria persistente existe para que el agente *recuerde contexto y progreso* (continuidad, no reversión).

## Relacionado
- [[Journal-y-Snapshots]]
- [[Agente-Conversacional]]
- [[Que-es-una-Tarea]]
