---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [debate, concurrencia, modulos]
---

# ¿Qué pasa cuando dos módulos piden el mismo recurso en conflicto?

## Resolución

El Core arbitra. La primera solicitud que llega obtiene el recurso, la segunda espera o falla según el contrato.

En Fase 1 este arbitraje es trivial porque el Core serializa todos los contratos con un lock global: no hay dos operaciones en vuelo al mismo tiempo. Ver [[Concurrencia]].

Un mecanismo más avanzado —colas por recurso, prioridades, detección de deadlocks— se resuelve cuando exista contención medida en la práctica.

## Revisiones

### 2026-08-01 — Pasa de `decretado-parcial` a `decretado`
**Antes:** la nota quedaba abierta porque "no hay módulos todavía", y no cubría el caso de dos contratos ejecutándose a la vez ni el bloqueo de subgrafo contra un commit.
**Ahora:** el modelo de concurrencia de Fase 1 está decretado en [[Concurrencia]], y este debate queda cerrado en su alcance.

## Relacionado
- [[Concurrencia]]
- [[Sistema-de-Modulos]]
- [[Flujo-Canonico-Overview]]
