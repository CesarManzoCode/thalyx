---
tipo: estrategia
estado: decretado
fecha-decreto: 2026-07-31
tags: [fases, roadmap, implementacion]
---

# Estrategia de implementación (fases)

## Fase 1: Capa sobre Linux (userspace) — primeros 6-12 meses

- **Base:** Alpine Linux minimalista.
- **Componentes:**
  - Agente conversacional (basado en reglas + embeddings, **no** fine-tuning — ver [[Debate-Agente-Fine-Tuning]]).
  - Índice semántico (SQLite) — ver [[FS-en-Grafo]].
  - Orquestador de permisos JIT (daemon en userspace) — ver [[Permisos-JIT]].
  - Orquestador de scheduling (daemon en userspace) — ver [[Scheduler-Predictivo]].
  - Gestor de módulos (instalación, sandboxing, rollback).
  - Interfaz gráfica mínima (terminal + web dashboard para pruebas).
- **Objetivo:** tener un prototipo funcional que pueda demostrar el flujo completo: usuario → agente → contrato → ejecución → rollback. Ver [[Caso-Instalar-Modulo]].

## Fase 2: Validación empírica — meses 12-18

- **Medir:** overhead de context switches en el FS semántico, latencia de permisos JIT, efectividad del scheduler predictivo.
- **Benchmarks:** comparar rendimiento de la capa IA vs. hacer las mismas operaciones sin IA.
- **Decisión:** si el overhead es <5%, la capa userspace es suficiente. Si es >15%, se migra al kernel. Ver [[Decision-Kernel-vs-Userspace]].

## Fase 3: Migración al kernel (si aplica) — años 2-3

- **FS en grafo nativo:** se implementa un módulo VFS o un FS FUSE optimizado (si el overhead es alto).
- **Scheduler semántico nativo:** se implementa un módulo del scheduler del kernel (solo si el overhead es inaceptable).
- **Permisos JIT:** se implementa un módulo LSM (esta pieza ya estaba prevista desde el inicio).

## Fase 4: Ecosistema — continuo

- Repositorio comunitario: crecimiento orgánico de módulos.
- Core Modules: expansión gradual.
- Documentación: publicación de guías, API, y tutoriales.
- Comunidad: foros, Discord, contribuciones externas.

## Relacionado
- [[Condiciones-de-Adopcion]] — aplican a partir de que exista audiencia, no desde Fase 1
- [[Decision-Kernel-vs-Userspace]]
- [[Tareas-Pendientes]]
