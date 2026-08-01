---
tipo: componente
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitiva, fs-grafo, parser]
---

# Parser mecánico

Componente que construye el grafo de dependencias que consume el [[FS-en-Grafo]].

## Decreto

- Es un **componente separado** del FS en grafo.
- **Entrada:** código fuente (archivos).
- **Salida:** grafo (nodos: archivos; aristas: dependencias).
- Puede empezar como un parser tonto (regex sobre `import`/`#include`/`require`) y mejorar a tree-sitter después.

## Desacoplamiento crítico (por qué este diseño)

El parser es reemplazable y mejorable **sin tocar el contrato del grafo**. El resto del sistema solo consume el grafo, no sabe cómo se produjo.

Esto significa que se puede empezar con un parser muy simple para un lenguaje, validar que el resto del sistema (índice, consultas, refactor atómico) funciona sobre ese grafo, y después mejorar el parser a algo real (tree-sitter, por ejemplo) sin tocar nada río abajo.

Es el mismo patrón que separar "el contrato de interfaz" (grafo: nodos, aristas, tags — estable) de "el motor que lo produce" (parser — reemplazable, testeable de forma aislada porque tiene input/output determinístico).

## Modo de ejecución (decretado)

**Batch (barrido on-demand).** No hay daemon vigilante (inotify) en Fase 1. El grafo se reconstruye desde cero cuando se necesita (ej. comando `build-graph`).

Razón: más simple, determinista, y suficientemente rápido para un proyecto pequeño (<1s). El **modo incremental** (daemon + inotify) es una optimización para fases posteriores.

## Relacionado
- [[FS-en-Grafo]]
- [[Criterio-de-Inclusion-de-Primitivas]]
