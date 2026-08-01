---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitiva, scheduler, userspace]
---

# Scheduler predictivo por contexto

## Función

La IA puede ajustar prioridades de procesos en tiempo real basado en el contexto.

## Implementación (Fase 1)

- Orquestador en userspace usando cgroups, `nice` y `sched_setattr`.
- Flujo:
  1. El agente detecta que el usuario está compilando un proyecto grande.
  2. El agente ejecuta: `PRIORITIZE(process: "gcc", boost: "80%", duration: "10s")`.
  3. El orquestador aplica los cambios.
  4. Después de 10 segundos, restaura las prioridades originales.

## Naturaleza: optimización, no dependencia crítica

El scheduling es una **optimización**, no una dependencia crítica. Si falla, la operación continúa sin el ajuste (**degradación**, no aborto). Se registra como advertencia en el [[Journal-y-Snapshots|Journal]].

Ver la rama de fallo "Degradación" en [[Ramas-de-Fallo]].

## Relacionado
- [[Decision-Kernel-vs-Userspace]]
- [[Ramas-de-Fallo]]
- [[Criterio-de-Inclusion-de-Primitivas]] — el "grafo de procesos en runtime" es una primitiva futura condicionada a que este scheduler tenga un consumidor real
