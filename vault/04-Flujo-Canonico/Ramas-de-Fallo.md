---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, fallo, rollback, seguridad]
---

# Las tres ramas de fallo

Se distinguieron y decretaron tres mecanismos de fallo distintos, que **no deben tratarse como un concepto único de "error"**.

## 1. Rechazo

**Cuándo:** fallo en cualquier paso *antes* de que el Sandbox produzca el artefacto — validación de contrato, origen no confiable en un campo con efecto, permisos que exceden el manifiesto, permiso denegado por el usuario.

**Qué pasa:** no hay acción física realizada, no hay nada que revertir, el Core notifica al usuario y el flujo termina ahí. Los permisos que hubieran quedado en estado pendiente se descartan. No involucra al Journal como mecanismo de reversión.

## 2. Rollback

**Cuándo:** fallo después de que el Sandbox produjo un artefacto en el área de staging, pero antes del commit — típicamente porque la verificación del Core detectó un problema.

**Qué pasa:** con [[Fase-Commit-Atomico|build-then-commit]], simplemente **no hay commit**. No hace falta deshacer nada en el sistema oficial porque nunca se tocó: el trabajo fallido queda contenido en el área de staging y se descarta. Los permisos pendientes se descartan con él. El Journal registra el intento fallido como referencia.

Ver el trazado concreto en [[Caso-Fallo-Rollback]].

## 3. Degradación

**Específico del Orquestador de scheduling.** Si el ajuste de prioridades falla, la operación completa **NO se aborta** — continúa sin el ajuste solicitado, y el Core lo registra en el Journal como advertencia, no como error.

Razón: el scheduling es una optimización, nunca una dependencia crítica para que una operación tenga éxito. Ver [[Scheduler-Predictivo]].

> Nota: esta rama no se ejercita en Fase 1, porque el scheduler está pospuesto a Fase 2. Se mantiene decretada porque el resto del sistema ya está diseñado asumiendo que existe.

## Por qué se distinguen estas tres categorías

Inicialmente el flujo solo contemplaba fallo en el paso de ejecución del Sandbox. Se identificó que un fallo antes de la ejecución física y uno durante o después son mecanismos completamente distintos que comparten el mismo nombre coloquial ("falló"), pero uno no tiene nada que revertir y el otro sí — aunque con build-then-commit ese "revertir" se simplifica a "no publicar".

## Qué NO cubren estas tres ramas

`thalyx restore` no es ninguna de las tres: no es una rama de fallo de un contrato, es una operación destructiva deliberada sobre el estado del sistema. Ver [[Rollback-vs-Restore]].

## Revisiones

### 2026-08-01 — Se amplían los disparadores y se acota el alcance
**Antes:** el rechazo se describía por número de paso, la rama de rollback no mencionaba qué pasa con los permisos pendientes, y "rollback" se usaba tanto para esta rama como para la restauración de snapshots.
**Ahora:** los disparadores incluyen las nuevas validaciones (origen, contención de permisos), se especifica el descarte de permisos pendientes, y se separa explícitamente de `restore`.

## Relacionado
- [[Rollback-vs-Restore]]
- [[Flujo-Canonico-Overview]]
- [[Fase-Commit-Atomico]]
- [[Caso-Fallo-Rollback]]
- [[Journal-y-Snapshots]]
