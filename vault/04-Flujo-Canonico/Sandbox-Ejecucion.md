---
tipo: componente
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, sandbox, seguridad]
---

# Sandbox de ejecución de módulos

## Rol

Aísla y ejecuta el código del módulo en un entorno controlado (namespaces, cgroups, seccomp). Es una de las [[Flujo-Canonico-Overview|9 piezas fijas]] del flujo canónico.

## Decreto: el Sandbox es una pieza separada del Core

El Core valida el contrato, pero **no ejecuta el código del módulo**. El módulo se ejecuta en un entorno aislado que es una pieza independiente.

**Motivo:** si el Core ejecutara directamente el código del módulo, un bug o un ataque en el módulo podría comprometer el Core. Separarlos es una decisión de seguridad, no de complejidad.

## Decreto: el Sandbox no toca el estado oficial del sistema

Ver el detalle completo en [[Fase-Commit-Atomico]]. En resumen: el Sandbox produce un artefacto en área temporal (`/tmp/build/...`), nunca escribe directo a `/opt/modules/...` ni a ningún destino oficial. El Core verifica y publica.

## Decreto: el Sandbox nunca toca el FS en grafo ni el Journal directamente

Solo el Core actualiza el [[FS-en-Grafo]] y escribe en el [[Journal-y-Snapshots]], después de recibir y verificar el resultado del Sandbox. Esto evita que un módulo comprometido dentro del sandbox pueda corromper el índice semántico o falsificar el journal.

## Mecanismo técnico (nivel Linux)

- Namespaces de Linux (aislamiento de vista del sistema).
- cgroups (límite de recursos).
- seccomp (restricción de syscalls peligrosas).

Pendiente de detalle fino — ver [[Tareas-Pendientes]] punto sobre mecanismo de sandboxing.

## Relacionado
- [[Fase-Commit-Atomico]]
- [[Flujo-Canonico-Overview]]
- [[Sistema-de-Modulos]]
