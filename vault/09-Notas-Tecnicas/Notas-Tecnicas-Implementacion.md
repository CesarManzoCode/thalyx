---
tipo: notas-tecnicas
estado: para-implementacion
fecha-decreto: 2026-07-31
tags: [implementacion, tecnico, referencia-rapida]
---

# Notas técnicas adicionales (para implementación)

Referencia rápida de supuestos y decisiones técnicas puntuales a tener presentes al escribir código. Cada punto enlaza a su nota completa.

- **Atomicidad del commit:** el Core debe usar `rename` para commits (atómico en Linux dentro del mismo filesystem). Si el commit cruza filesystems, se necesita una capa de transacción adicional (ej. copiar + journal de intención). Ver [[Fase-Commit-Atomico]].

- **Resolución de versiones:** el resolver de paquetes del Core es una de las piezas más complejas (similar a npm/apt). Se debe diseñar con cuidado, pero su existencia ya está decretada. Ver [[Resolucion-de-Versiones]].

- **Sandbox:** el Sandbox no escribe directo al sistema oficial. Produce artefactos en `/tmp/build/...`. El Core verifica y publica. Esto convierte el rollback en "no hubo commit" en lugar de "deshacer". Ver [[Sandbox-Ejecucion]].

- **Memoria persistente:** guarda hechos y notas de continuidad por separado. Los hechos son inmutables; las notas son descartables. Ver [[Memoria-Persistente]].

- **Agente:** empieza con reglas escritas a mano (if-else) + embeddings. El fine-tuning es para fases posteriores. Ver [[Debate-Agente-Fine-Tuning]].

- **Reputación:** no se resuelve ahora. Se deja un campo `reputation` en el schema para migración futura. Ver [[Sistema-Reputacion-Sybil]].

- **Core Modules:** el equipo central decide qué entra y sale de la lista. Es una decisión política, no técnica. Ver [[Debate-Core-Modules]].

## Relacionado
- [[Tareas-Pendientes]]
- [[Fase-Commit-Atomico]]
