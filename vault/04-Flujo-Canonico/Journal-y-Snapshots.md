---
tipo: componente
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, journal, rollback, auditoria]
---

# Journal / sistema de snapshots

## Rol

Registra operaciones para permitir rollback. Es una de las [[Flujo-Canonico-Overview|9 piezas fijas]] del flujo canónico. **Escrito únicamente por el Core** — nunca directamente por el [[Sandbox-Ejecucion|Sandbox]].

## Alcance declarado: qué cubre y qué no

El journal registra **únicamente las operaciones ejecutadas por Thalyx**. No es un registro completo de lo que le pasó al sistema.

Esto no es una limitación a corregir: es una consecuencia directa del [[Principio-Doble-Ruta]], que garantiza que el humano pueda operar con herramientas POSIX estándar sin pasar por el agente. Un journal que pretendiera ser completo estaría mintiendo. Ver [[Coherencia-Doble-Ruta]].

## Propósito: distinto de la Memoria persistente

El Journal existe para **deshacer** una operación a nivel de sistema (rollback físico). La [[Memoria-Persistente]] existe para que el agente **recuerde el contexto y progreso** de una tarea (continuidad, no reversión). Ambas "guardan estado", pero con propósitos distintos.

## Relación con build-then-commit

Con la arquitectura de [[Fase-Commit-Atomico|build-then-commit]], el rol del Journal se simplifica: en vez de tener que registrar y luego ejecutar snapshots complejos para revertir archivos parcialmente copiados, el Journal simplemente registra si hubo o no hubo commit. Si no hubo commit, no hay nada que revertir — el Journal registra el intento fallido como referencia.

## Tecnología de base

**Btrfs, obligatorio en Fase 1.** Los subvolúmenes se separan entre sistema, módulos y datos de usuario, y los snapshots permiten revertir cambios realizados por un módulo o por el agente.

Dónde se monta cada uno, y por qué no es lo que parece:

| Subvolumen | Punto de montaje | Qué lleva |
|---|---|---|
| `system` | `/opt/thalyx` | El store entero: `.staging/`, `modules/`, `state/` y el journal |
| `modules` | `/opt/thalyx/data` | Lo que un módulo escribe — lo que un snapshot tendría que revertir |
| `user` | `/home` | Los archivos de la persona, que ningún rollback nuestro toca |

**`modules` no se monta en `/opt/thalyx/modules`.** El código de un módulo vive dentro de `system`, junto al staging desde el que se publica, porque `rename(2)` devuelve `EXDEV` al cruzar subvolúmenes de Btrfs y la publicación atómica de [[Fase-Commit-Atomico]] es exactamente ese `rename`. Separar el directorio `modules/` en su propio subvolumen se lee más ordenado y rompería todas las instalaciones de la máquina.

Lo que sí tiene sentido separar es lo que un módulo **escribe**: el código instalado es inmutable y versionado, y deshacerlo es `rollback`, no `restore`. Ver [[Rollback-vs-Restore]].

## Dos operaciones distintas, dos comandos distintos

`rollback` y `restore` no son lo mismo y no comparten nombre. Ver [[Rollback-vs-Restore]].

## Auditoría

Cada acción del agente queda registrada en un log inmutable, incluyendo la cadena de origen de los campos del contrato que la produjo (ver [[Marcado-de-Origen]]). Los logs son revisables localmente por el usuario, no subidos automáticamente a ningún lado — ver [[Condiciones-de-Adopcion]].

## Revisiones

### 2026-08-03 — se decreta dónde se monta cada subvolumen
**Antes:** la nota decía que los subvolúmenes se separan entre sistema, módulos
y datos de usuario, sin decir dónde se monta ninguno. Al construir el store por
primera vez, la lectura natural —`modules` en `/opt/thalyx/modules`— resultó ser
la que rompe el sistema.
**Ahora:** la tabla de arriba, con `modules` en `/opt/thalyx/data` y el motivo
escrito al lado.
**Motivo:** el área de staging y su destino tienen que estar en el mismo
subvolumen o `rename` falla con `EXDEV`. Es el mismo fallo que
[[Fase-Commit-Atomico]] registra en su propia revisión, encontrado de nuevo tres
días después por el camino contrario. Un decreto que nombra tres cosas sin decir
dónde van deja que la ubicación la elija quien implemente, y aquí la elección
obvia era la equivocada.

### 2026-08-01 — Btrfs pasa de sugerencia a requisito, y se declara el alcance del journal
**Antes:** la nota mencionaba "Btrfs/ZFS" sin que ninguna nota decretara qué filesystem exige Thalyx, y el journal no declaraba que solo cubre sus propias operaciones.
**Ahora:** Btrfs es obligatorio en Fase 1, y el alcance del journal queda escrito de forma explícita.
**Motivo:** sin snapshots no existe la demostración de adopción de rollback, que es una de las tres decretadas. Y un journal cuyo alcance no está declarado invita a construir operaciones destructivas sobre el supuesto falso de que vio todo lo que pasó.

## Relacionado
- [[Rollback-vs-Restore]]
- [[Coherencia-Doble-Ruta]]
- [[Fase-Commit-Atomico]]
- [[Ramas-de-Fallo]]
- [[Memoria-Persistente]]
