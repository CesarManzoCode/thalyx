---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, fallo, rollback, seguridad]
---

# Las tres ramas de fallo

Se distinguieron y decretaron tres mecanismos de fallo distintos, que **no deben tratarse como un concepto único de "error"**.

## 1. Rechazo

**Cuándo:** fallo en cualquier paso *antes* de la producción del artefacto por el Sandbox (validación de contrato, permisos denegados, etc. — pasos 2-6 del flujo).

**Qué pasa:** no hay acción física realizada, no hay nada que revertir, el Core notifica al usuario y el flujo termina ahí. No involucra al Journal como mecanismo de reversión.

## 2. Rollback

**Cuándo:** fallo *después* de que el Sandbox ya produjo un artefacto pero antes o durante el commit, o si la verificación del Core detecta un problema (pasos 7-8).

**Qué pasa:** con la arquitectura de [[Fase-Commit-Atomico|build-then-commit]], este caso se simplifica enormemente — si la verificación falla, simplemente **no hay commit**, no hace falta "deshacer" nada en el sistema oficial porque nunca se tocó. El Journal registra el intento fallido como referencia, no como algo que revertir con snapshots complejos.

Ver el trazado concreto en [[Caso-Fallo-Rollback]].

## 3. Degradación

**Específico del Orquestador de scheduling.** Si el ajuste de prioridades falla, la operación completa **NO se aborta** — continúa sin el ajuste solicitado, y el Core lo registra en el Journal como advertencia, no como error.

Razón: el scheduling es una optimización, nunca una dependencia crítica para que una operación tenga éxito. Ver [[Scheduler-Predictivo]].

## Por qué se distinguen estas tres categorías

Inicialmente el flujo solo contemplaba fallo en el paso de ejecución del Sandbox. Se identificó que un fallo antes de la ejecución física (ej. contrato inválido, permiso denegado) y un fallo durante/después de la ejecución física son mecanismos completamente distintos que comparten el mismo nombre coloquial ("falló"), pero uno no tiene nada que revertir (rechazo) y el otro sí, aunque con build-then-commit ese "revertir" se simplifica a "no publicar".

## Relacionado
- [[Flujo-Canonico-Overview]]
- [[Fase-Commit-Atomico]]
- [[Caso-Fallo-Rollback]]
- [[Journal-y-Snapshots]]
