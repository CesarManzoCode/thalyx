---
tipo: componente
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, journal, rollback, auditoria]
---

# Journal / sistema de snapshots

## Rol

Registra operaciones para permitir rollback. Es una de las [[Flujo-Canonico-Overview|9 piezas fijas]] del flujo canónico. **Escrito únicamente por el Core** — nunca directamente por el [[Sandbox-Ejecucion|Sandbox]].

## Propósito: distinto de la Memoria persistente

El Journal existe para **deshacer** una operación a nivel de sistema (rollback físico). La [[Memoria-Persistente]] existe para que el agente **recuerde el contexto y progreso** de una tarea (continuidad, no reversión). Ambas "guardan estado", pero con propósitos distintos.

## Relación con build-then-commit

Con la arquitectura de [[Fase-Commit-Atomico|build-then-commit]], el rol del Journal se simplifica: en vez de tener que registrar y luego ejecutar snapshots complejos para revertir archivos parcialmente copiados, el Journal simplemente registra si hubo o no hubo commit. Si no hubo commit, no hay nada que revertir — el Journal registra el intento fallido como referencia.

## Tecnología de base

Snapshots (Btrfs/ZFS) que permiten revertir cualquier cambio realizado por un módulo o por el agente.

## Auditoría

Cada acción del agente queda registrada en un log inmutable. Ver [[Condiciones-de-Adopcion]] sección de confianza pública — los logs son revisables localmente por el usuario, no subidos automáticamente a ningún lado.

## Relacionado
- [[Fase-Commit-Atomico]]
- [[Ramas-de-Fallo]]
- [[Memoria-Persistente]]
