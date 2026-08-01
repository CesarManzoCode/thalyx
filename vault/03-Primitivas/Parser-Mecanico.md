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

**Incremental dirigido por eventos, con batch como reconciliación.**

- `thalyx-lsm` intercepta las mutaciones del filesystem y publica eventos en una cola. El hook **no bloquea**: encola y devuelve.
- Un worker consume la cola y re-parsea únicamente los archivos afectados.
- El **modo batch** (barrido completo) se conserva para dos casos: la reconciliación al arrancar el sistema, y el comando manual `thalyx graph build`.

### Por qué el hook no bloquea

Re-parsear dentro del hook obligaría a cada escritura del sistema a esperar al parser. Un `git checkout` o la descompresión de un paquete se arrastrarían, y un cuelgue del parser colgaría el filesystem entero. El precio de encolar es una ventana de milisegundos en la que el filesystem ya cambió y el grafo todavía no — ventana que el índice declara explícitamente en cada consulta.

## Revisiones

### 2026-08-01 — De batch puro a incremental por eventos del LSM
**Antes:** se decretaba modo batch on-demand, sin daemon vigilante, con el argumento de que era más simple y determinista.
**Ahora:** incremental dirigido por los eventos que intercepta `thalyx-lsm`, con batch reservado para arranque y comando manual.
**Motivo:** el batch puro deja al agente razonando sobre un grafo que puede tener horas de atraso. Con el LSM decretado para Fase 1, interceptar es incremental en costo, y a diferencia de inotify no requiere un watch por directorio ni pierde eventos por overflow. Se conserva del decreto original la propiedad que lo hacía valioso: el parseo sigue siendo determinista y aislado, solo cambia qué lo dispara.

## Lo que el parser no hace: resolver

El parser emite la referencia **tal como está escrita**. Decidir a qué archivo apunta `import foo.bar` no es una pregunta sobre el texto, sino sobre el árbol, y por eso vive en el grafo.

Esa separación resultó valiosa de inmediato al implementarla: la resolución necesitó tres correcciones —recortar el sufijo que nombra un ítem en vez de un archivo, no recortar tanto como para inventar aristas, y tratar `crate` como la raíz— y ninguna tocó el parser.

## Relacionado
- [[FS-en-Grafo]]
- [[Coherencia-Doble-Ruta]]
- [[Criterio-de-Inclusion-de-Primitivas]]
