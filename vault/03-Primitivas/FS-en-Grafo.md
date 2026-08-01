---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitiva, fs, grafo, fase-1]
---

# Sistema de archivos semántico (FS en grafo)

## Función

Permite a la IA consultar archivos por relaciones, no por rutas jerárquicas.

## Implementación (Fase 1)

- Capa de indexación sobre Btrfs usando SQLite.
- Etiquetas y relaciones manuales (el usuario o el agente etiquetan archivos).
- Consultas semánticas: "dame todos los archivos con etiqueta 'auth-core'".
- No es mágico: necesita un [[Parser-Mecanico|parser mecánico]] que analice imports/includes para construir el grafo automáticamente.
- Sin montaje FUSE en Fase 1: el grafo se consulta por API y CLI. Ver [[Decision-Kernel-vs-Userspace]].

## Origen de esta primitiva

Fue la primera oportunidad clara de mejora identificada para que la IA operara en el mejor terreno posible — el ejemplo fundacional de qué significa que una primitiva "sea nativa para la IA" en vez de heredada del diseño pensado para humanos.

## Coherencia del índice

El índice **no es la fuente de verdad**: lo es el filesystem. El índice es un caché derivado.

- `thalyx-lsm` intercepta las mutaciones del filesystem (`rename`, `unlink`, `create`, cierre tras escritura) y las encola sin bloquear la operación.
- Un worker consume la cola y re-parsea los archivos afectados.
- Toda consulta al grafo devuelve, junto al resultado, si el índice está al día o si hay nodos pendientes de reprocesar.

El detalle completo del mecanismo, sus límites y qué pasa con los cambios hechos con Thalyx apagado está en [[Coherencia-Doble-Ruta]].

## Operación atómica de refactorización

El agente puede ejecutar:

```
REFACTOR_SUBGRAPH(from: "auth-core", to: "auth_v2", tag: "refactor-2026-07-30")
```

Que:
1. Bloquea el subgrafo.
2. Actualiza nodos y aristas en una transacción (usando SQLite WAL).
3. Mueve los archivos físicos.
4. Registra la operación en un journal semántico.
5. Si algo falla, revierte todo (usando snapshots de Btrfs).

Esta operación se ejecuta bajo el lock global del Core — ver [[Concurrencia]].

## Regla de honestidad de las consultas

Toda consulta devuelve las filas **junto con** el grado de actualización del índice. No son dos llamadas: quien quiere los datos recibe la advertencia por obligación.

Hacerlo separable dejaría que se olvidara, que es exactamente cómo un caché empieza a confundirse con la verdad.

Corolario descubierto al implementarlo: **el índice no puede vivir dentro del árbol que indexa**. Un caché que forma parte de su propia entrada nunca puede estar al día, porque escribirlo lo invalida. Está impedido por construcción, no por convención.

## Actualización durante el flujo canónico

**Solo el Core actualiza el FS en grafo**, nunca el [[Sandbox-Ejecucion|Sandbox]] directamente — ver [[Flujo-Canonico-Overview]] y la corrección de separación de responsabilidades ahí documentada.

## Revisiones

### 2026-08-01 — Se reemplaza inotify por interceptación del LSM
**Antes:** esta nota decía que se usaba inotify/fanotify para invalidar el índice, mientras que [[Parser-Mecanico]] decretaba modo batch sin ningún daemon vigilante en Fase 1. Contradicción directa entre dos notas decretadas.
**Ahora:** `thalyx-lsm` intercepta las mutaciones y las encola; un worker re-parsea de forma asíncrona; el índice declara siempre si está al día.
**Motivo:** con el LSM ya decretado para Fase 1 ([[Permisos-JIT]]), la interceptación pasó de ser cara a ser incremental, y es estrictamente superior a inotify: ve todas las mutaciones sin necesitar un watch por directorio y sin perder eventos por overflow. El modo batch sobrevive como reconciliación de arranque y como comando manual.

### 2026-08-01 — Se elimina FUSE y se fija Btrfs
**Motivo:** ver [[Decision-Kernel-vs-Userspace]] y [[Fases-de-Implementacion]].

## Relacionado
- [[Parser-Mecanico]]
- [[Coherencia-Doble-Ruta]]
- [[Decision-Kernel-vs-Userspace]]
- [[Flujo-Canonico-Overview]]
