---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-01
tags: [flujo, concurrencia, core, fase-1]
---

# Concurrencia y supuesto de usuario

## El hueco que resuelve

[[Debate-Conflicto-Recursos]] cubría el caso de dos módulos pidiendo el mismo recurso, pero nada en la bóveda decía qué pasa cuando **dos contratos se ejecutan a la vez**, ni cómo interactúa el bloqueo de subgrafo de `REFACTOR_SUBGRAPH` con un commit del Core. Tampoco estaba escrito el supuesto —evidente pero no decretado— de que hay un solo usuario y un solo agente.

## Decreto

**Fase 1 asume un usuario y una instancia de agente.**

`thalyx-core` es el **único escritor** y serializa la ejecución de contratos con un lock global: un contrato en ejecución a la vez. Los contratos que llegan durante una ejecución se encolan por orden de llegada.

Esto incluye a `REFACTOR_SUBGRAPH`: se ejecuta bajo el mismo lock, así que no puede solaparse con un commit.

## Por qué un lock global y no locks por recurso

Un lock global elimina de un golpe toda una clase de defectos —contratos que se pisan, refactorizaciones compitiendo con commits, permisos entrelazados entre operaciones— y no cuesta nada en Fase 1, donde no existe carga que serializar ni usuarios que noten la espera.

El paralelismo por recurso, con su detección de deadlocks y su ordenamiento de locks, es la optimización que corresponde **cuando exista contención medida**. Es la aplicación directa del [[Criterio-de-Inclusion-de-Primitivas]]: no se resuelve antes de tiempo un problema que aparece con la escala, y la frontera del lock es fácil de estrechar después, no de introducir después.

## Relacionado
- [[Debate-Conflicto-Recursos]]
- [[Core]]
- [[FS-en-Grafo]]
- [[Criterio-de-Inclusion-de-Primitivas]]
