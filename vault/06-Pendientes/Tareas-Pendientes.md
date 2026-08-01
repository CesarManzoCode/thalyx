---
tipo: pendientes
estado: activo
fecha-decreto: 2026-07-31
tags: [pendientes, tareas, roadmap-decisiones]
---

# Tareas pendientes (explícitas)

Lista viva de decisiones y trabajo que todavía falta cerrar. Actualizar el estado en el frontmatter de cada nota enlazada conforme se resuelvan.

## Pendientes de decreto formal

- [ ] **Mecanismo concreto de resolución de versiones** — qué formato de constraint se acepta (`^2.3`, `~2.3.1`, `latest`, `stable`), cómo se resuelve contra el repo comunitario, cómo se manejan conflictos de dependencias. Ver [[Resolucion-de-Versiones]].
- [ ] **Formato exacto del manifiesto de módulo (`.osmod`)** — qué campos son obligatorios (nombre, versión, permisos, firma, hash, dependencias), quién define ese schema, cómo se valida. Ver [[Sistema-de-Modulos]].
- [ ] **Mecanismo de sandboxing en detalle** — qué namespaces se usan, qué seccomp policies, cómo se maneja el acceso a recursos compartidos. Ver [[Sandbox-Ejecucion]].
- [ ] **Diseño del ISO booteable para Fase 1** — cómo se construye, qué contiene, cómo se arranca en QEMU con un solo comando. Ver [[Condiciones-de-Adopcion]].

## Ya resueltos (referencia histórica)

- [x] Re-trazar el caso de "instalar módulo" con build-then-commit — resuelto, ver [[Caso-Instalar-Modulo]].
- [x] Trazar un caso de fallo/rollback explícito — resuelto, ver [[Caso-Fallo-Rollback]].
- [x] Decidir si "resolver módulo" es contrato separado o sub-tarea sin contrato — resuelto, ver [[Resolver-vs-Instalar]] (se decidió: sub-tarea sin contrato).

## Todo lo del documento original que sigue sin tocarse en esta sesión de arquitectura

Estos temas del resumen fundacional del proyecto **siguen abiertos** — la sesión de diseño del flujo canónico no los tocó:

- Arquitectura del índice semántico a mayor escala (¿SQLite + FUSE es suficiente o se necesita algo más escalable?)
- Modelo concreto del agente para fine-tuning (¿qué modelo 3B-7B, qué dataset?) — ver [[Debate-Agente-Fine-Tuning]]
- Mecanismo del scheduler sin introducir latencia — ver [[Scheduler-Predictivo]]
- Técnicas de interpretabilidad más prometedoras — ver [[Interpretabilidad-Mecanicista]]
- Métricas de benchmark concretas para validar mejoras del FS semántico y scheduler
- Sistema de reputación resistente a Sybil attacks — deliberadamente pospuesto, ver [[Sistema-Reputacion-Sybil]]

## Relacionado
- [[00-Indice/Indice-Principal|Índice principal]]
- [[Notas-Tecnicas-Implementacion]]
