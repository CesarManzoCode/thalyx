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

Entonces el atajo no se afirma: **se gana**, y hacen falta dos llaves. La puerta es `thalyx graph trust --counter`: corre la verificación **en ese momento**, no se fía de una corrida anterior, y se niega si el contador y el árbol no coinciden, si la cobertura está rota o si falta algún hook. Lo que la ganó queda escrito en el índice, así que la respuesta a "¿por qué está encendido el atajo aquí?" está en disco y no en la memoria de alguien.

La propiedad que hace seguro persistirlo: **se devuelve solo**. Nadie tiene que acordarse de apagarlo. Un watcher recargado, un mapa ilegible, un hook que ya no está — cada uno vuelve a caminar el árbol sin que ningún código lo decida, porque la confianza es una de dos condiciones y la otra la responde el kernel en cada consulta. Y un valor corrupto, o escrito por una versión de Thalyx que todavía no existe, se lee como *caminar siempre*: el atajo es la respuesta peligrosa, así que nunca es la que puede producir un campo dañado.

Las dos llaves: La cobertura la responde el kernel: `claims_complete_coverage` dejó de ser la constante `false` y ahora le pregunta a `bpftool` qué programas están cargados, contestando que no por cada motivo por el que debe hacerlo — no cargado, hook ausente, sin `bpftool`, sin permiso. La confianza la decide quien llama, y `thalyx graph verify` pregunta a los dos lados en una máquina real y reporta si coincidieron.

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

### Acotar la cuenta al árbol

Construido, pendiente de ejecutarse en hardware. La vía **no** es consumir el ring buffer: es atribuir cada evento dentro del propio hook, subiendo por los ancestros del dentry hasta encontrar una raíz vigilada. Eso mantiene la lectura en `bpftool map dump` y no necesita ningún consumidor que siga el protocolo del anillo.

El caso delicado se resolvió comprobándolo, no adivinándolo. Si la subida llega a la raíz de su propio filesystem sin coincidir, el archivo está **definitivamente fuera** de todo árbol vigilado en ese filesystem: mismo superbloque, todos los ancestros examinados, ninguno coincide. Nada se cuenta. Ese es el caso que vuelve callada a una máquina ocupada — el caché del navegador, un log, una tubería, una escritura a `/tmp`: cada uno sube a su propia raíz y no aporta a ningún árbol.

Descansa sobre un supuesto, y el supuesto es ahora una **precondición comprobada**: un archivo alcanzado por un *montaje* dentro del árbol vive en otro superbloque, y su subida pararía en la raíz de ese filesystem sin ver nunca el dentry vigilado. Así que userspace se niega a acotar un árbol que tenga algo montado debajo — una lectura de `/proc/mounts` al construir el índice — y cae al conteo de toda la máquina, que es menos útil y nunca está mal.

Lo único que queda genuinamente sin atribuir es una ruta más profunda de lo que la subida sube (64 niveles). Eso va a un contador aparte que userspace **suma a todos los árboles**: cobrarle de más a un árbol cuesta un recorrido, dejarlo fuera dejaría pasar un cambio.

Y la conversión que falla en silencio si se hace mal: el `st_dev` que devuelve un `stat` **no** es el `s_dev` que tiene el kernel. Confundirlos no da error — la búsqueda simplemente nunca coincide, la cuenta del árbol se queda en cero, y cero se lee como "aquí nunca ha cambiado nada". Está pinchado con valores conocidos en un test.

Lo que sí queda deliberadamente sin cubrir: los atributos extendidos, porque SELinux reetiqueta archivos todo el día y un contador que nunca deja de moverse no permite ningún atajo; y un filesystem que otra máquina pueda escribir, que cambia sin que ningún hook de esta máquina se entere. Ningún juego de hooks cierra eso.

Nada de esto se puede compilar ni verificar en el contenedor de desarrollo, que no tiene `bpftool`, ni bpffs, ni `vmlinux.h`. Por eso `dev/verify.sh` trae la medición que lo comprueba en hardware.

## Qué significa "depende" — revisión del 2026-08-28

Hasta hoy una arista del grafo era **un import**: `use`, `mod`, `import`,
`#include`, `require`. Eso es lo que el archivo declara de sí mismo, es cierto
siempre, y es la mitad de la pregunta.

La otra mitad la encontró Claude corriendo el sistema. Preguntado qué depende de
`src/store.rs`, el índice contestó con los dos archivos que escriben
`use crate::store::…` y **se le escapó un tercero** que llega al mismo código
como `server.store.persist()`. `grep persist` lo encontró. La evidencia ya
estaba adentro del índice — la mención estaba registrada — y nada la convertía
en arista.

Así que `dependencias` significaba *imports*, y la palabra que un agente lee ahí
es *todo lo que se rompería*. La distancia entre las dos es exactamente el error
que un agente comete confiando en la respuesta.

**Ahora hay dos clases de arista y cada fila dice cuál es:**

- `via: import` — el archivo la declaró. Evidencia fuerte, cierta sola.
- `via: symbol` — el archivo usa un nombre que **exactamente un** archivo del
  árbol declara. Evidencia más débil a propósito: es un hecho sobre el árbol
  entero, y deja de ser cierto si un segundo archivo empieza a declarar ese
  nombre.

**Cuatro condiciones, y las cuatro se pagaron.** Las tres últimas salieron de
correr el índice sobre este repositorio y leer las filas, no de pensarlo:

1. **Exactamente un archivo declara el nombre.** Dos declaraciones lo vuelven una
   adivinanza, y una adivinanza presentada como hecho es peor que la fila que
   falta. El índice sigue contestando la verdad sobre el nombre — dos
   definiciones y un uso — y se niega a dibujar la arista.
2. **El nombre es visible desde afuera.** La regla de cada lenguaje, no una
   heurística: `pub` en Rust, `export` en JavaScript, mayúscula inicial en Go,
   no-`static` en C, sin guion bajo inicial en Python. `thalyx-snapshot` declara
   `fn place` y `fn relative`, las dos privadas y las dos palabras corrientes, y
   cada archivo del repositorio con un `let relative = …` figuraba como
   dependiente. Un nombre privado **no puede** alcanzarse desde otro archivo: eso
   no es una suposición sobre el código, es el lenguaje diciendo que la arista es
   imposible.
3. **El archivo que lo usa no lo ata.** `thalyx-snapshot` también declara
   `pub fn directory(&self)` y `pub fn subvolume(&self)` — públicas, únicas, y
   palabras del idioma. Con las dos reglas de arriba tenía **41 dependientes**,
   casi todos archivos con un `for directory in …` o un campo llamado
   `subvolume`. Un archivo que ata un nombre habla de su propia atadura, y su
   atadura tapa cualquier cosa de afuera. Quedaron 19, y 17 son referencias
   reales entre crates que ningún import podía resolver.
4. **El archivo no declara ese nombre él mismo**, a cualquier visibilidad. Un
   archivo con su propio `fn validate_name` privado, llamándolo, figuraba como
   dependiente del único crate con uno público — porque sólo las declaraciones
   exportadas son candidatas, así que la privada de al lado no estaba ahí para
   volver ambiguo el nombre.

Y una arista de símbolo que caiga donde ya hay un import no se escribe: sería una
segunda línea sobre el mismo hecho, en una respuesta cuyo propósito entero es
costar menos que leer los archivos.

**Lo que queda mal, contado y no escondido.** Sobre el archivo más difícil del
repo quedan dos filas falsas de diecinueve: un método de la biblioteca estándar
que se llama igual que una función libre (`ops.difference(&otros)`), y un
segmento intermedio de una ruta de otro crate (`thalyx_btrfs::subvolume::create`).
Las dos necesitan saber de qué tipo es el receptor, que es un compilador. Se
dejan porque son dos, y porque el error que evitarían es una fila de más, no una
de menos.

Con un mecanismo se resuelven, medidos en `crates/thalyx-graph/corpus/`: la
llamada directa por ruta sin `use`, el acceso por campo, el método sobre un tipo
que llegó de otro lado, el trait nombrado en una cota, el módulo de directorio, y
el re-export — que es el caso donde el import no resuelve **a nada** porque
`crate::Engine` no es un archivo.

Lo que sigue sin saberse, escrito y no escondido: **un alias**. `use X as Y` da
la dependencia entre archivos por el import, pero `Y` no es `X` para el índice, y
seguir esa ligadura es un compilador y no un escaneo. Está en el corpus como
límite declarado, y `THALYX_REQUIRE_FULL_CORPUS=1` lo convierte en falla.

## Una consulta repara el índice que necesita — revisión del 2026-08-28

La regla de honestidad de arriba no se mueve: la respuesta sigue llevando la
frescura en el mismo objeto que las filas. Lo que cambió es **quién actúa sobre
ella**.

En la primera corrida real de un agente externo, Claude preguntó por un árbol que
acababa de cambiar, recibió una respuesta que no coincidía con lo que había
hecho, dedujo del campo `fresh` que el índice estaba atrasado, llamó a `state`,
llamó a `indexar`, y volvió a preguntar. Cuatro turnos, tres de ellos gastados en
la contabilidad del índice. Nada ahí estaba roto — el índice dijo exactamente lo
que sabía. Lo que estaba mal es que resolverlo quedaba en manos de lo más caro
del circuito.

Ahora `buscar`, `depende` y `usan` reconstruyen el índice antes de contestar,
**cuando es barato**: el techo es 2 000 archivos, sacado de una medición y no del
gusto — indexar `crates/` de este repo (297 archivos, 5 495 nombres, 58 135
menciones) tarda 452 ms, o sea milisegundos por archivo. Por encima del techo no
se reconstruye nada: la respuesta dice `refreshed: declined_too_large`, cuánto
mide el árbol, y que lo que hay que llamar es `indexar`.

Y las cuatro salidas se nombran, nunca un booleano: `not_needed`, `rebuilt`,
`declined_too_large`, `failed`. Las tres últimas piden cosas distintas de quien
pregunta — nada, paciencia, o una llamada — y un booleano habría juntado las dos
últimas en la única respuesta sobre la que no se puede actuar. Nada reporta
`current` por haberlo intentado: una reconstrucción que se rechazó o que falló
devuelve las filas viejas con la etiqueta vieja.

Quien quiera lo contrario — saber qué tenía el índice, que es una pregunta real y
justo la que un refresco automático destruiría — lo pide con `refrescar=no`.

## Relacionado
- [[Parser-Mecanico]]
- [[Coherencia-Doble-Ruta]]
- [[Decision-Kernel-vs-Userspace]]
- [[Flujo-Canonico-Overview]]
