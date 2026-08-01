---
tipo: componente
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, journal, rollback, auditoria]
---

# Journal / sistema de snapshots

## Rol

Registra operaciones para permitir rollback. Es una de las [[Flujo-Canonico-Overview|9 piezas fijas]] del flujo canónico. **Escrito únicamente por el Core** — nunca directamente por el [[Sandbox-Ejecucion|Sandbox]].

## Alcance declarado: qué cubre y qué no

El journal registra **únicamente las operaciones ejecutadas por Thalyx**. No es un registro completo de lo que le pasó al sistema.

Esto no es una limitación a corregir: es una consecuencia directa del [[Principio-Doble-Ruta]], que garantiza que el humano pueda operar con herramientas POSIX estándar sin pasar por el agente. Un journal que pretendiera ser completo estaría mintiendo. Ver [[Coherencia-Doble-Ruta]].

## Propósito: distinto de la Memoria persistente

El Journal existe para **deshacer** una operación a nivel de sistema (rollback físico). La [[Memoria-Persistente]] existe para que el agente **recuerde el contexto y progreso** de una tarea (continuidad, no reversión). Ambas "guardan estado", pero con propósitos distintos.

## Relación con build-then-commit

Con la arquitectura de [[Fase-Commit-Atomico|build-then-commit]], el rol del Journal se simplifica: en vez de tener que registrar y luego ejecutar snapshots complejos para revertir archivos parcialmente copiados, el Journal simplemente registra si hubo o no hubo commit. Si no hubo commit, no hay nada que revertir — el Journal registra el intento fallido como referencia.

## Tecnología de base

**Btrfs, obligatorio en Fase 1.** Los subvolúmenes se separan entre sistema, módulos y datos de usuario, y los snapshots permiten revertir cambios realizados por un módulo o por el agente.

## Dos operaciones distintas, dos comandos distintos

`rollback` y `restore` no son lo mismo y no comparten nombre. Ver [[Rollback-vs-Restore]].

## Auditoría

Cada acción del agente queda registrada en un log inmutable, incluyendo la cadena de origen de los campos del contrato que la produjo (ver [[Marcado-de-Origen]]). Los logs son revisables localmente por el usuario, no subidos automáticamente a ningún lado — ver [[Condiciones-de-Adopcion]].

## Revisiones

### 2026-08-01 — Btrfs pasa de sugerencia a requisito, y se declara el alcance del journal
**Antes:** la nota mencionaba "Btrfs/ZFS" sin que ninguna nota decretara qué filesystem exige Thalyx, y el journal no declaraba que solo cubre sus propias operaciones.
**Ahora:** Btrfs es obligatorio en Fase 1, y el alcance del journal queda escrito de forma explícita.
**Motivo:** sin snapshots no existe la demostración de adopción de rollback, que es una de las tres decretadas. Y un journal cuyo alcance no está declarado invita a construir operaciones destructivas sobre el supuesto falso de que vio todo lo que pasó.

## Relacionado
- [[Rollback-vs-Restore]]
- [[Coherencia-Doble-Ruta]]
- [[Fase-Commit-Atomico]]
- [[Ramas-de-Fallo]]
- [[Memoria-Persistente]]
