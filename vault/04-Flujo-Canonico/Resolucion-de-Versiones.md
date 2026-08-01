---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, versiones, core, pendiente-detalle]
---

# Resolución de versiones

## Problema

El agente no debe fijar la versión exacta de un módulo en el contrato (ej. `version: "2.3.1"`), porque eso podría hacer que un agente desactualizado fuerce versiones viejas.

## Solución decretada

El agente expresa una **restricción** (`constraint: "^2.3"` o `channel: "stable"`). El Core (o un resolver de paquetes dentro del Core) resuelve la versión exacta y la fija en el contrato final. Esto mantiene al agente fuera de la lógica de resolución.

## Implicación (nota de complejidad real)

El Core necesita un **resolver de dependencias**. Es una pieza compleja (similar a npm/apt), correctamente ubicada arquitectónicamente, pero **una de las piezas de código más laboriosas de todo el Core**, no un one-liner. Los resolvers de paquetes son notoriamente una de las partes más difíciles de sistemas como apt/npm, con casos como conflictos de versiones entre dependencias transitivas.

## Estado: pendiente de mecanismo concreto

Falta decidir formalmente:
- Qué formato de constraint se acepta (`^2.3`, `~2.3.1`, `latest`, `stable`, etc.)
- Cómo se resuelve contra el repo comunitario
- Cómo se manejan los conflictos de dependencias

Ver [[Tareas-Pendientes]].

## Relacionado
- [[Caso-Instalar-Modulo]] — donde se aplica en la práctica
- [[Contrato-Estructurado]]
- [[Tareas-Pendientes]]
