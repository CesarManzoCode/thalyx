---
tipo: overview
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitivas, arquitectura, moc]
---

# Las 4 primitivas base del sistema

Este es el mapa de contenido (MOC) de las cuatro primitivas que forman la **Cara IA** de la [[Arquitectura-Asimetrica]]. Todas cumplen el [[Criterio-de-Inclusion-de-Primitivas]]: su ausencia hoy implicaría una reescritura dolorosa después.

| Primitiva | Función | Ubicación | Fase |
|---|---|---|---|
| [[FS-en-Grafo]] | Consultar archivos por relaciones semánticas, no por rutas jerárquicas | Userspace (SQLite) | 1 |
| [[Permisos-JIT]] | La IA pide acceso temporal a recursos, se otorga/revoca automáticamente | Kernel (`thalyx-lsm`) + broker userspace | 1 |
| [[Memoria-Persistente]] | Guardar y recuperar estado de tareas entre sesiones | Userspace (BD vectorial) | 1 |
| [[Scheduler-Predictivo]] | Ajustar prioridades de procesos en tiempo real según contexto | Userspace (cgroups) | 2 |

Ver [[Decision-Kernel-vs-Userspace]] para la justificación completa de por qué cada una vive donde vive.

Para primitivas identificadas pero **no construidas todavía**, ver [[Criterio-de-Inclusion-de-Primitivas]] (sección "primitivas futuras").

## Revisiones

### 2026-08-01 — El scheduler predictivo se pospone a Fase 2
**Antes:** las cuatro primitivas se construían en Fase 1.
**Ahora:** el scheduler sigue siendo una primitiva base decretada, pero se implementa en Fase 2.
**Motivo:** por decreto propio es "optimización, nunca dependencia crítica"; en el [[Caso-Instalar-Modulo|caso canónico]] el paso correspondiente dice literalmente "no aplica"; y no participa de ninguna de las demostraciones de adopción. Es el único componente de Fase 1 cuya ausencia no obliga a reescribir nada, que es exactamente la condición del [[Criterio-de-Inclusion-de-Primitivas]].

### 2026-08-01 — Se corrige la ubicación de los permisos JIT
Ver [[Permisos-JIT]] y [[Decision-Kernel-vs-Userspace]].

## Relacionado
- [[Arquitectura-Asimetrica]]
- [[Flujo-Canonico-Overview]] — cómo estas primitivas participan en el flujo de una acción
