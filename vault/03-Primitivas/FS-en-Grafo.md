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

`thalyx_watch.bpf.c` cuenta cada mutación que ven sus hooks, en un mapa BPF que `bpftool map dump` sabe leer. La pregunta cara pasa a tener respuesta barata: si el contador no se movió desde que se construyó el índice, no hay nada que caminar.

**El atajo está apagado**, y lo interesante de este trabajo es por qué.

Solo es correcto si los hooks atrapan *todas* las formas en que un archivo puede cambiar, y durante meses no lo hicieron. El primer juego era `inode_create`, `inode_unlink` e `inode_rename`: **editar un archivo en su lugar no crea nada, no borra nada y no renombra nada**, así que "el contador no se movió" era compatible con el árbol entero reescrito. Faltaban además los directorios, los enlaces simbólicos, los enlaces duros y los nodos de dispositivo — `inode_create` es solo archivos regulares, así que un `mkdir` era invisible para un watcher cuyo trabajo entero es notar que el árbol cambió de forma.

Un contador con ese hueco no puede encender el atajo jamás, y por lo tanto es decoración.

Ahora son **diez hooks**, y el que decide todo es `lsm/file_permission` enmascarado a `MAY_WRITE`. Se llama desde `rw_verify_area` en cada lectura y cada escritura, así que atrapa las escrituras por un descriptor **que ya estaba abierto cuando el watcher se enganchó** — el editor o la base de datos de vida larga que es precisamente lo que vuelve increíble a un contador. `file_open` no las habría visto. Ese hook es caliente, y por eso el contador pasó a ser por CPU: una sola línea de caché disputada por todos los núcleos en la ruta de escritura no era pagable.

Lo que **sigue** fuera es que el contador es de toda la máquina: dice que algo cambió en esta computadora, no que haya cambiado algo en el árbol indexado. Esa es la dirección segura del error —cuesta recorridos que no hacían falta, nunca esconde un cambio— pero significa que el atajo solo se dispara en una máquina tranquila.

Entonces el atajo no se afirma: **se gana**, y hacen falta dos llaves. La cobertura la responde el kernel: `claims_complete_coverage` dejó de ser la constante `false` y ahora le pregunta a `bpftool` qué programas están cargados, contestando que no por cada motivo por el que debe hacerlo — no cargado, hook ausente, sin `bpftool`, sin permiso. La confianza la decide quien llama, y `thalyx graph verify` pregunta a los dos lados en una máquina real y reporta si coincidieron.

Y una cosa que se estaba haciendo mal sin que doliera: cada hook cuenta **antes** de saber si la operación tuvo éxito, porque un hook LSM corre antes de la operación y su argumento `ret` no es el resultado, es lo que ya decidió otro programa enganchado al mismo hook. Contar de más cuesta un recorrido; contar de menos cuesta corrección. Y `ret` se devuelve intacto: un watcher que devolviera 0 convertiría la denegación de otro programa en un permiso — un programa cuyo propósito entero es no denegar nada empezaría a conceder cosas.

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

Acotar la cuenta al árbol indexado. La vía **no** es consumir el ring buffer: es atribuir cada evento dentro del propio hook, subiendo por los ancestros del dentry hasta encontrar una raíz vigilada. Eso mantiene la lectura en `bpftool map dump` y no necesita ningún consumidor que siga el protocolo del anillo.

Tiene un caso que hay que resolver con cuidado y no adivinar: si la subida llega a la raíz del superbloque sin encontrar coincidencia, el archivo está fuera del árbol vigilado **solo si** ambos están en el mismo superbloque; si no, pudo entrar por un montaje. Se puede comprobar desde userspace al construir el índice —¿hay algún punto de montaje debajo del árbol?— y romper la cobertura cuando lo haya.

Lo que sí queda deliberadamente sin cubrir: los atributos extendidos, porque SELinux reetiqueta archivos todo el día y un contador que nunca deja de moverse no permite ningún atajo; y un filesystem que otra máquina pueda escribir, que cambia sin que ningún hook de esta máquina se entere. Ningún juego de hooks cierra eso.

Nada de esto se puede compilar ni verificar en el contenedor de desarrollo, que no tiene `bpftool`, ni bpffs, ni `vmlinux.h`. Por eso `dev/verify.sh` trae la medición que lo comprueba en hardware.

## Relacionado
- [[Parser-Mecanico]]
- [[Coherencia-Doble-Ruta]]
- [[Decision-Kernel-vs-Userspace]]
- [[Flujo-Canonico-Overview]]
