---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitiva, fs, grafo, fase-1]
---

# Sistema de archivos semántico (FS en grafo)

## Función

Permite a la IA consultar archivos por relaciones, no por rutas jerárquicas.

## Implementación (Fase 1)

- Capa de indexación sobre Btrfs usando SQLite.
- Etiquetas y relaciones manuales (el usuario o el agente etiquetan archivos).
- Consultas semánticas: "dame todos los archivos con etiqueta 'auth-core'".
- No es mágico: necesita un [[Parser-Mecanico|parser mecánico]] que analice imports/includes para construir el grafo automáticamente.
- Sin montaje FUSE en Fase 1: el grafo se consulta por API y CLI. Ver [[Decision-Kernel-vs-Userspace]].

## Origen de esta primitiva

Fue la primera oportunidad clara de mejora identificada para que la IA operara en el mejor terreno posible — el ejemplo fundacional de qué significa que una primitiva "sea nativa para la IA" en vez de heredada del diseño pensado para humanos.

## Coherencia del índice

El índice **no es la fuente de verdad**: lo es el filesystem. El índice es un caché derivado.

- `thalyx-lsm` intercepta las mutaciones del filesystem (`rename`, `unlink`, `create`, cierre tras escritura) y las encola sin bloquear la operación.
- Un worker consume la cola y re-parsea los archivos afectados.
- Toda consulta al grafo devuelve, junto al resultado, si el índice está al día o si hay nodos pendientes de reprocesar.

El detalle completo del mecanismo, sus límites y qué pasa con los cambios hechos con Thalyx apagado está en [[Coherencia-Doble-Ruta]].

## Operación atómica de refactorización

El agente puede ejecutar:

```
REFACTOR_SUBGRAPH(from: "auth-core", to: "auth_v2", tag: "refactor-2026-07-30")
```

Que:
1. Bloquea el subgrafo.
2. Actualiza nodos y aristas en una transacción (usando SQLite WAL).
3. Mueve los archivos físicos.
4. Registra la operación en un journal semántico.
5. Si algo falla, revierte todo (usando snapshots de Btrfs).

Esta operación se ejecuta bajo el lock global del Core — ver [[Concurrencia]].

## Regla de honestidad de las consultas

Toda consulta devuelve las filas **junto con** el grado de actualización del índice. No son dos llamadas: quien quiere los datos recibe la advertencia por obligación.

Hacerlo separable dejaría que se olvidara, que es exactamente cómo un caché empieza a confundirse con la verdad.

Corolario descubierto al implementarlo: **el índice no puede vivir dentro del árbol que indexa**. Un caché que forma parte de su propia entrada nunca puede estar al día, porque escribirlo lo invalida. Está impedido por construcción, no por convención.

## Actualización durante el flujo canónico

**Solo el Core actualiza el FS en grafo**, nunca el [[Sandbox-Ejecucion|Sandbox]] directamente — ver [[Flujo-Canonico-Overview]] y la corrección de separación de responsabilidades ahí documentada.

## Revisiones

### 2026-08-01 — Se reemplaza inotify por interceptación del LSM
**Antes:** esta nota decía que se usaba inotify/fanotify para invalidar el índice, mientras que [[Parser-Mecanico]] decretaba modo batch sin ningún daemon vigilante en Fase 1. Contradicción directa entre dos notas decretadas.
**Ahora:** `thalyx-lsm` intercepta las mutaciones y las encola; un worker re-parsea de forma asíncrona; el índice declara siempre si está al día.
**Motivo:** con el LSM ya decretado para Fase 1 ([[Permisos-JIT]]), la interceptación pasó de ser cara a ser incremental, y es estrictamente superior a inotify: ve todas las mutaciones sin necesitar un watch por directorio y sin perder eventos por overflow. El modo batch sobrevive como reconciliación de arranque y como comando manual.

### 2026-08-01 — Se elimina FUSE y se fija Btrfs
**Motivo:** ver [[Decision-Kernel-vs-Userspace]] y [[Fases-de-Implementacion]].

## El kernel diciendo cuándo el índice sigue vigente

Verificar la frescura **camina el árbol entero, en cada consulta**. Es honesto —el filesystem es la verdad y el índice es un caché— y no escala.

`thalyx_watch.bpf.c` cuenta ahora cada mutación que ven sus hooks, en un mapa BPF de tipo array que `bpftool map dump` sabe leer. La pregunta cara pasa a tener respuesta barata: si el contador no se movió desde que se construyó el índice, no hay nada que caminar.

**El atajo está apagado**, y lo interesante de este trabajo es por qué.

Solo es correcto si los hooks atrapan *todas* las formas en que un archivo puede cambiar, y no lo hacen: una escritura por un descriptor ya abierto no pasa por `inode_create`, ni `inode_unlink`, ni `inode_rename`. Peor: el contador es **de toda la máquina**, así que dice que algo cambió en esta computadora, no que algo haya cambiado en el árbol indexado. Cualquiera de las dos cosas haría que el índice contestara "vigente" sobre un árbol que ya se movió — exactamente lo que [[Coherencia-Doble-Ruta]] declara peor que admitir ignorancia.

Entonces el atajo no se afirma: **se gana**. Por defecto se camina siempre y el contador solo explica el resultado. `thalyx graph verify` pregunta a los dos lado a lado en una máquina real y reporta si coincidieron.

### La asimetría que importa

Las dos formas de discrepar no valen lo mismo:

| El contador dice | El árbol dice | Qué es |
|---|---|---|
| cambió algo | nada cambió | inofensivo: el índice fue más cauto de lo necesario |
| nada cambió | cambió algo | **hueco de cobertura**: el índice mentiría |

Solo la segunda rompe la cobertura, y la rompe de forma permanente.

### Reglas de cobertura

- La cobertura **empieza rota**: nada se ha observado, así que nada se puede avalar.
- Solo una reconstrucción la repara.
- Un contador que va hacia atrás significa que el programa se recargó, y el hueco no se recupera contando de nuevo.
- Un contador ilegible rompe la cobertura; "no lo pude leer" nunca se convierte en "no cambió nada".
- Un baseline persistido que no se puede parsear se trata como **ausente**, no como cero. Cero es un valor que el watcher sí avalaría.

### Lo que falta

Acotar la cuenta al árbol indexado necesita la **ruta** de cada mutación, que está en el ring buffer junto al contador y requiere un consumidor que haga `mmap` del mapa y siga el protocolo del anillo. No se escribió a ciegas: el contenedor de desarrollo no tiene `bpftool`, ni bpffs, ni `vmlinux.h`, así que ni ese consumidor ni el cambio al programa BPF pudieron ejercitarse ahí.

## Relacionado
- [[Parser-Mecanico]]
- [[Coherencia-Doble-Ruta]]
- [[Decision-Kernel-vs-Userspace]]
- [[Flujo-Canonico-Overview]]
