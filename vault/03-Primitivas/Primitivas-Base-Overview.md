---
tipo: overview
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitivas, arquitectura, moc]
---

# Las 4 primitivas base del sistema

Este es el mapa de contenido (MOC) de las cuatro primitivas que forman la **Cara IA** de la [[Arquitectura-Asimetrica]]. Todas cumplen el [[Criterio-de-Inclusion-de-Primitivas]]: su ausencia hoy implicaría una reescritura dolorosa después.

| Primitiva | Función | Ubicación |
|---|---|---|
| [[FS-en-Grafo]] | Consultar archivos por relaciones semánticas, no por rutas jerárquicas | Userspace (SQLite) |
| [[Permisos-JIT]] | La IA pide acceso temporal a recursos, se otorga/revoca automáticamente | Kernel (LSM) |
| [[Scheduler-Predictivo]] | Ajustar prioridades de procesos en tiempo real según contexto | Userspace (cgroups) |
| [[Memoria-Persistente]] | Guardar y recuperar estado de tareas entre sesiones | Userspace (BD vectorial) |

Ver [[Decision-Kernel-vs-Userspace]] para la justificación completa de por qué cada una vive donde vive.

Para primitivas identificadas pero **no construidas todavía**, ver [[Criterio-de-Inclusion-de-Primitivas]] (sección "primitivas futuras").

## Relacionado
- [[Arquitectura-Asimetrica]]
- [[Flujo-Canonico-Overview]] — cómo estas primitivas participan en el flujo de una acción
