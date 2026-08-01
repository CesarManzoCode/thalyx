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

- Capa de indexación sobre ext4/btrfs usando SQLite.
- Etiquetas y relaciones manuales (el usuario o el agente etiquetan archivos).
- Consultas semánticas: "dame todos los archivos con etiqueta 'auth-core'".
- No es mágico: necesita un [[Parser-Mecanico|parser mecánico]] que analice imports/includes para construir el grafo automáticamente.

## Origen de esta primitiva

Fue la primera oportunidad clara de mejora identificada para que la IA operara en el mejor terreno posible — el ejemplo fundacional de qué significa que una primitiva "sea nativa para la IA" en vez de heredada del diseño pensado para humanos.

## Coherencia de caché

Se usa inotify/fanotify para invalidar el índice cuando cambian los archivos. El overhead de context switches es aceptable para operaciones deliberadas (no en el hot path del kernel), con umbral <5% (ver [[Decision-Kernel-vs-Userspace]]).

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
5. Si algo falla, revierte todo (usando snapshots de Btrfs/ZFS).

## Actualización durante el flujo canónico

**Solo el Core actualiza el FS en grafo**, nunca el [[Sandbox-Ejecucion|Sandbox]] directamente — ver [[Flujo-Canonico-Overview]] y la corrección de separación de responsabilidades ahí documentada.

## Relacionado
- [[Parser-Mecanico]]
- [[Decision-Kernel-vs-Userspace]]
- [[Flujo-Canonico-Overview]]
