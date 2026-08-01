---
tipo: overview
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, arquitectura, moc, pieza-central]
---

# El flujo canónico de una acción

Esta es la pieza central de diseño del proyecto: el mapa completo de cómo fluye una acción de punta a punta — desde que el usuario expresa una intención hasta que el sistema cambia de estado.

## Las 9 piezas fijas del sistema

| # | Pieza | Rol | ¿En la TCB? |
|---|---|---|---|
| 1 | Usuario | Origen de la intención | — |
| 2 | [[Agente-Conversacional\|Agente]] | Traduce intención a contrato estructurado, con contexto de memoria persistente | **No** |
| 3 | [[Core\|Core]] | Valida el contrato y orquesta la ejecución completa. Único punto de entrada para la ejecución. No confía en el agente: revalida todo | **Sí** |
| 4 | Orquestador de permisos JIT | Otorga y revoca permisos; el enforcement lo aplica `thalyx-lsm` | **Sí** |
| 5 | Orquestador de scheduling | Ajusta prioridades si la acción lo requiere. **Pospuesto a Fase 2** | — |
| 6 | [[FS-en-Grafo\|Índice semántico]] | Consulta y actualiza relaciones entre archivos. Actualizado únicamente por el Core | — |
| 7 | [[Memoria-Persistente]] | Guarda y restaura contexto del agente entre sesiones | — |
| 8 | [[Sandbox-Ejecucion\|Sandbox]] | Aísla el código de módulos. No escribe al sistema oficial: produce en staging, el Core verifica y publica | **No** |
| 9 | [[Journal-y-Snapshots\|Journal / snapshots]] | Registra operaciones para permitir rollback. Escrito únicamente por el Core | **Sí** |

La columna de TCB viene del [[Modelo-de-Amenaza]]: los componentes marcados con "No" pueden estar comprometidos y el sistema debe seguir siendo seguro.

## Decisiones de separación de responsabilidades (decretadas explícitamente)

1. **El Sandbox no ejecuta sobre el estado oficial del sistema.** Produce su artefacto en un área de staging y el Core lo verifica y publica. Ver [[Fase-Commit-Atomico]].

2. **El Core es validador y orquestador a la vez**, como dos fases secuenciales de la misma pieza. Ver [[Core]].

3. **El Sandbox nunca toca el FS en grafo ni el Journal directamente.** Esto evita que un módulo comprometido corrompa el índice o falsifique el journal.

4. **El agente nunca compone ni transporta una solicitud de autorización.** Ver [[Camino-Confiable]].

5. **El contenido no confiable nunca origina un campo con efecto.** Ver [[Marcado-de-Origen]].

## El flujo completo (camino feliz)

```
 1. Usuario expresa intención.
 2. Agente traduce intención a Contrato (JSON), con marcado de origen por campo.
 3. Core valida: sintaxis, origen de campos, permisos ⊆ manifiesto, política.
 4. Core resuelve la versión exacta a partir del constraint.
 5. Core genera y renderiza la confirmación por el camino confiable.
 6. Permisos confirmados quedan registrados como PENDIENTES.
 7. Core pasa el contrato al Sandbox.
 8. Sandbox produce el artefacto en el área de staging (NUNCA en el sistema oficial).
 9. Core verifica: firma, hash recalculado por él mismo, integridad, dependencias.
10. Core hace el commit atómico: rename del directorio + rename del symlink.
11. Los permisos pendientes se vuelven efectivos, dentro del mismo commit.
12. Core actualiza el FS en grafo y escribe en el Journal.
13. Core guarda hechos y notas de continuidad en la Memoria persistente.
14. Core notifica al usuario el resultado.
```

Todo el flujo se ejecuta bajo el lock global del Core: un contrato a la vez. Ver [[Concurrencia]].

## Notas de origen (por qué el flujo evolucionó así)

Este flujo se derivó de forzar el diseño abstracto contra un caso concreto real ([[Caso-Instalar-Modulo]]). Ese ejercicio reveló huecos que el diseño abstracto no mostraba, y volvió a hacerlo en la revisión del 1 de agosto de 2026:

- La necesidad de [[Tres-Tipos-de-Permiso|distinguir tipos de permiso]].
- La necesidad de [[Fase-Commit-Atomico|separar ejecución de publicación]].
- La necesidad de [[Tres-Categorias-de-Autorizacion|distinguir tres categorías de autorización]].
- Que la verificación del artefacto no tenía referente ([[Verificacion-y-Distribucion]]).
- Que la confirmación humana pasaba por el componente no confiable ([[Camino-Confiable]]).

## Revisiones

### 2026-08-01 — Se añaden los pasos de seguridad al flujo y la columna de TCB
**Antes:** el flujo tenía 12 pasos y no incluía marcado de origen, contención de permisos, camino confiable ni la distinción entre permiso pendiente y efectivo. La tabla de piezas no decía cuáles son confiables.
**Ahora:** 14 pasos, y cada pieza declara su posición respecto a la TCB.
**Motivo:** el flujo describía correctamente el camino feliz de una acción, pero no dejaba ver dónde están las fronteras de confianza — que es justamente lo que hay que tener presente al implementarlo.

## Relacionado
- [[Core]]
- [[Modelo-de-Amenaza]]
- [[Contrato-Estructurado]]
- [[Ramas-de-Fallo]]
- [[Fase-Commit-Atomico]]
- [[Caso-Instalar-Modulo]]
- [[Caso-Fallo-Rollback]]
- [[Principio-Doble-Ruta]]
