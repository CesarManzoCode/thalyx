---
tipo: overview
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, arquitectura, moc, pieza-central]
---

# El flujo canónico de una acción

Esta es la pieza central de diseño de la sesión del 31 de julio de 2026: el mapa completo de cómo fluye una acción de punta a punta en el sistema — desde que el usuario expresa una intención hasta que el sistema cambia de estado.

## Las 9 piezas fijas del sistema

| # | Pieza | Rol |
|---|---|---|
| 1 | Usuario | Origen de la intención |
| 2 | [[Agente-Conversacional\|Agente]] | Traduce intención a contrato estructurado, con contexto de memoria persistente |
| 3 | Core | Valida el contrato y orquesta la ejecución completa. Único punto de entrada para la ejecución. No confía en el agente por defecto — revalida todo |
| 4 | Orquestador de permisos JIT | Otorga/revoca permisos temporales o persistentes según el contrato |
| 5 | Orquestador de scheduling | Ajusta prioridades de procesos si la acción lo requiere (optimización, no dependencia crítica) |
| 6 | [[FS-en-Grafo\|Índice semántico / FS en grafo]] | Consulta y actualiza relaciones entre archivos. Actualizado únicamente por el Core, nunca directamente por el Sandbox |
| 7 | [[Memoria-Persistente]] | Guarda y restaura contexto del agente entre sesiones |
| 8 | [[Sandbox-Ejecucion\|Sandbox de ejecución de módulos]] | Ejecuta código de módulos de forma aislada. No escribe directo al sistema oficial — produce un artefacto en área temporal, el Core lo verifica y publica |
| 9 | [[Journal-y-Snapshots\|Journal / sistema de snapshots]] | Registra operaciones para permitir rollback. Escrito únicamente por el Core |

## Decisiones de separación de responsabilidades (decretadas explícitamente)

1. **El Sandbox no ejecuta directamente sobre el estado oficial del sistema.** Genera su artefacto en un área de build temporal (ej. `/tmp/build/...`). El Core verifica el resultado (firma, hash, integridad, dependencias) y solo entonces lo publica al destino real (ej. `/opt/modules/...`).

   Esta es **la corrección arquitectónica más importante** de todo el diseño: convierte el rollback de "deshacer lo que alcancé a copiar" (frágil) a "simplemente no hubo commit" (robusto y atómico). Ver [[Fase-Commit-Atomico]].

2. **El Core es validador y orquestador a la vez** (dos fases secuenciales de la misma pieza, no dos piezas separadas): primero valida el contrato, luego coordina la ejecución llamando a las demás piezas en orden. No hay conflicto de roles porque no ocurren simultáneamente.

3. **El Sandbox nunca toca el FS en grafo ni el Journal directamente** — solo el Core lo hace, después de recibir y verificar el resultado del Sandbox. Esto evita que un módulo comprometido dentro del sandbox pueda corromper el índice semántico o falsificar el journal.

## El flujo completo (camino feliz)

```
1. Usuario expresa intención.
2. Agente traduce intención a Contrato (JSON), con contexto de Memoria persistente.
3. Core valida el Contrato (sintaxis, permisos, límites).
4. Core solicita permiso JIT al Orquestador de permisos (tipo JIT/sesión/persistente, con confirmación si aplica).
5. Core solicita ajuste de prioridades al Orquestador de scheduling (si aplica; si falla, degrada, no aborta).
6. Core pasa Contrato al Sandbox para ejecutar acción física.
7. Sandbox ejecuta la acción en /tmp/build/... (NUNCA en el sistema oficial directamente).
8. Core verifica el resultado (firma, hash, integridad, dependencias).
9. Core publica el artefacto verificado al destino real (commit atómico, ej. rename).
10. Core actualiza el FS en grafo y escribe en el Journal.
11. Core guarda hechos y notas de continuidad en la Memoria persistente.
12. Core notifica al usuario el resultado.
```

Ver el detalle de la etapa de commit en [[Fase-Commit-Atomico]], y las ramas de fallo en [[Ramas-de-Fallo]].

## Notas de origen (por qué el flujo evolucionó así)

Este flujo se derivó de forzar el diseño abstracto contra un caso concreto real ([[Caso-Instalar-Modulo]]). Ese ejercicio reveló varios huecos que el diseño abstracto no mostraba, entre ellos:
- La necesidad de [[Tres-Tipos-de-Permiso|distinguir tipos de permiso]] (JIT/sesión/persistente).
- La necesidad de [[Fase-Commit-Atomico|separar ejecución de publicación]] (build-then-commit).
- La necesidad de [[Tres-Categorias-de-Autorizacion|distinguir autorización operacional, de capacidades, y de publicación]].

## Relacionado
- [[Contrato-Estructurado]]
- [[Ramas-de-Fallo]]
- [[Fase-Commit-Atomico]]
- [[Caso-Instalar-Modulo]]
- [[Caso-Fallo-Rollback]]
- [[Principio-Doble-Ruta]]
