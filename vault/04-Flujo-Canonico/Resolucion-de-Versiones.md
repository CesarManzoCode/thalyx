---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, versiones, core, fase-1]
---

# Resolución de versiones

## Problema

El agente no debe fijar la versión exacta de un módulo en el contrato (ej. `version: "2.3.1"`), porque eso podría hacer que un agente desactualizado fuerce versiones viejas.

## Solución decretada

El agente expresa una **restricción** (`constraint: "^2.3"`). El Core resuelve la versión exacta y la fija en el contrato final. Esto mantiene al agente fuera de la lógica de resolución.

## Mecanismo (decretado)

### Formato de constraint

SemVer con sintaxis de Cargo: `^2.3`, `~2.3.1`, `=2.3.1`, y rangos del tipo `>=2.0, <3.0`.

### Algoritmo de resolución en Fase 1

**La versión máxima publicada que satisface el constraint y cuya firma valida.**

Eso es todo. No hay backtracking, no hay resolución de conflictos, no hay grafo de dependencias que recorrer.

### Por qué es tan simple: no hay dependencias entre módulos

**En Fase 1, un módulo solo declara qué versión de Thalyx necesita** (`requires.thalyx`). No puede depender de otros módulos.

Toda la dificultad legendaria de apt y npm proviene **enteramente** de las dependencias transitivas: el diamante de versiones, el backtracking, los conflictos irresolubles entre requisitos incompatibles de dos ramas del grafo. Sin transitivas, el resolver deja de ser un motor de satisfacción de restricciones y pasa a ser una comparación ordenada.

El [[Caso-Instalar-Modulo|caso canónico]] se cumple entero bajo esta restricción, sin recortar nada de lo que la Fase 1 necesita demostrar.

### Cuándo se levanta la restricción

Las dependencias entre módulos, y el resolver con backtracking que exigen, se decretan **cuando exista un módulo real que las necesite** — con ese caso concreto delante, no antes. Es la aplicación directa del [[Criterio-de-Inclusion-de-Primitivas]].

Añadir dependencias transitivas después no obliga a reescribir el resto del sistema: el contrato ya lleva `constraint`, el manifiesto ya tiene una sección `[requires]`, y el punto de extensión está donde debe estar. Lo único que cambia es el algoritmo detrás de una interfaz que ya existe.

## Revisiones

### 2026-08-01 — Se decreta el mecanismo y se acota radicalmente su alcance
**Antes:** la nota advertía que el resolver era "una de las piezas de código más laboriosas de todo el Core", comparable a apt o npm, y dejaba pendiente el formato de constraint, la resolución contra el repositorio y el manejo de conflictos.
**Ahora:** SemVer estilo Cargo, resolución por máximo que satisface, y prohibición de dependencias entre módulos en Fase 1.
**Motivo:** la advertencia era correcta pero atribuía la complejidad al lugar equivocado. El resolver no es difícil por resolver versiones: es difícil por resolver *conflictos entre dependencias transitivas*. Eliminando las transitivas de la Fase 1, la pieza pasa de meses a días sin perder nada demostrable — y el problema difícil se enfrenta cuando exista un caso real que lo justifique.

## Relacionado
- [[Formato-Manifiesto-Thmod]]
- [[Caso-Instalar-Modulo]] — donde se aplica en la práctica
- [[Contrato-Estructurado]]
- [[Criterio-de-Inclusion-de-Primitivas]]
