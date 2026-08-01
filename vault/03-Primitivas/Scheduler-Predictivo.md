---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitiva, scheduler, userspace, fase-2]
---

# Scheduler predictivo por contexto

## Función

La IA puede ajustar prioridades de procesos en tiempo real basado en el contexto.

## Implementación (Fase 2)

- Orquestador en userspace usando cgroups, `nice` y `sched_setattr`.
- Flujo:
  1. El agente detecta que el usuario está compilando un proyecto grande.
  2. El agente ejecuta: `PRIORITIZE(process: "gcc", boost: "80%", duration: "10s")`.
  3. El orquestador aplica los cambios.
  4. Después de 10 segundos, restaura las prioridades originales.

## Naturaleza: optimización, no dependencia crítica

El scheduling es una **optimización**, no una dependencia crítica. Si falla, la operación continúa sin el ajuste (**degradación**, no aborto). Se registra como advertencia en el [[Journal-y-Snapshots|Journal]].

Ver la rama de fallo "Degradación" en [[Ramas-de-Fallo]].

## Revisiones

### 2026-08-01 — Se pospone a Fase 2
**Antes:** figuraba entre los componentes a construir en Fase 1.
**Ahora:** sigue siendo una de las cuatro primitivas base decretadas, pero se implementa en Fase 2.
**Motivo:** aplicación del [[Criterio-de-Inclusion-de-Primitivas]] a la propia primitiva. Por decreto es "optimización, nunca dependencia crítica"; en el [[Caso-Instalar-Modulo|caso canónico]] el paso de scheduling dice "no aplica"; y no participa de ninguna de las tres demostraciones de adopción. Es el único componente de Fase 1 cuya omisión no obliga a reescribir nada río abajo, porque el resto del sistema ya trata su fallo como degradación aceptable — es decir, ya está diseñado para funcionar sin él.

## Relacionado
- [[Decision-Kernel-vs-Userspace]]
- [[Ramas-de-Fallo]]
- [[Fases-de-Implementacion]]
- [[Criterio-de-Inclusion-de-Primitivas]] — el "grafo de procesos en runtime" es una primitiva futura condicionada a que este scheduler tenga un consumidor real
