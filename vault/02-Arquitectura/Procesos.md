---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-23
tags: [terminal, procesos, memoria, doble-ruta]
---

# Procesos: procesos, memoria y matar

Es el punto 7 de la terminal usable de [[Tareas-Pendientes]]: *qué corre,
matarlo, cuánta memoria queda*. Todo sale de `/proc`, que es el kernel
contestando sobre sí mismo — en la imagen no hay `ps`, no hay `free` y no hay
`kill`, y no va a haberlos: la imagen lleva el kernel y un programa.

| se pregunta | verbo |
|---|---|
| qué está corriendo | `procesos [patrón]` |
| cuánta memoria queda | `memoria` |
| que esto se detenga | `matar <numero> [forzar]` |

## Lo que esto no es

**No lanza nada.** Lanzar es G1 de [[Superficie-para-el-LLM]] y viene enredado
con G2 y con el decreto de que Thalyx sólo corre módulos firmados. El punto 7 es
la mitad más angosta: la que hace falta para contestar *qué se está comiendo esta
máquina* y para hacer que pare.

## El número no es el proceso

Entre leer `/proc/4711` y mandarle una señal a 4711, ese proceso puede terminar y
el kernel puede darle el número a otro. **Toda herramienta que recibe un pid en
la línea de comandos tiene ese hueco** y vive con él.

`matar` no. Abre un `pidfd` —un descriptor que se refiere al proceso y no al
número— y manda la señal por ahí, así que la señal llega al proceso para el que
se abrió el descriptor o falla con `ESRCH`, y no hay un tercer resultado donde le
llegue a un desconocido.

Eso además decide el orden de las operaciones, **y es el contrario del obvio**:
primero el descriptor, después la descripción, después la señal. Leer
`/proc/<pid>` primero describiría lo que tuviera el número en ese momento, que es
exactamente lo que esto existe para descartar.

## `forzar` es una palabra que alguien tiene que escribir

Por omisión se manda `TERM`, que un programa puede atrapar para guardar lo que
tenía en la mano antes de salir. `matar <numero> forzar` manda `KILL`, que no se
puede atrapar y por lo tanto no escribe nada.

La diferencia se comprueba con una **línea base**, que es lo único que la hace
medible: una shell arrancada con `trap '' TERM` tiene que **sobrevivir** a `matar`
y **no** sobrevivir a `matar forzar`. Sin esa mitad, un `matar` que siempre
mandara `KILL` y uno que respetara la distinción se ven idénticos desde afuera.

## Las dos negativas, que no son restricciones

| número | palabra | remedio |
|---|---|---|
| PID 1 | `is_init` | `use_poweroff` |
| esta misma sesión | `is_self` | `use_exit` |

**No son una política sobre quién es dueño de la máquina**, que es Cesar. Son que
cada uno de esos dos trabajos tiene un verbo que lo hace bien —`apagar` y
`salir`— y una señal los hace de la única forma que no deja dicho por qué la
máquina se detuvo. El remedio viene en la respuesta, que es el punto A2: negarse
sin nombrar la salida sería esconder la respuesta.

Se niegan también el `0` y los negativos. Para `kill(2)` el cero es *todo lo que
alcance* y un negativo es un **grupo** de procesos; nadie escribe ninguno de los
dos a propósito, y los dos son la forma en que un tecleo de más se lleva más de
lo que nombró.

Y **un renglón que nombra dos procesos no detiene ninguno.** Quien escribió ese
renglón esperaba que pararan los dos, y parar uno en silencio es el peor
resultado disponible.

## `ensayo matar` es el ensayo que más importa

D1 de [[Superficie-para-el-LLM]] pide ensayo en todo verbo que cambia algo. Aquí
vale más que en ningún otro lado: un archivo se puede volver a escribir y **un
proceso no**, y la entrada que causa el error son cuatro dígitos.

Así que contesta con todo lo que dejaría notar que se tecleó el número
equivocado: el nombre, la línea de comandos entera, cuánto lleva corriendo y
quién lo arrancó. Y no manda nada — lo cual sólo queda probado por una aserción:
que el proceso sigue vivo después. Es lo único que separa un ensayo del verbo.

## `libre` no es `disponible`

`memoria` contesta las dos y **nombra cuál contesta la pregunta**.

- `available` es la estimación del propio kernel de lo que un programa nuevo
  podría obtener sin irse a swap, contando el caché que soltaría.
- `free` es memoria intacta. Casi siempre alarmante y casi nunca la respuesta:
  un Linux sano la mantiene cerca de cero a propósito, porque memoria que no hace
  nada es memoria desperdiciada.

Quien lee sólo `free` concluye que la máquina está llena y empieza a matar cosas.
Por eso `en uso` se calcula como `total − available` y nunca como `total − free`.

En un kernel sin `MemAvailable` —anteriores a 3.14— se cae a `MemFree`, que se
equivoca **hacia abajo**: reporta menos de lo que un programa podría obtener,
nunca más. Es la dirección que pide la regla 9.

## Leer `/proc` es leer la salida de otro programa

Regla 6 de [[Estrategia-de-Pruebas]]: un parser del formato de otra herramienta
necesita **una muestra real capturada, tal cual**. La trampa está en el segundo
campo de `/proc/<pid>/stat`:

```
4709 (we (ird) x) S 4706 4706 4350 0 -1 4194304 83 0 0 0 0 0 …
```

Eso es un renglón real, capturado el 2026-08-23 de un proceso arrancado a
propósito con ese nombre. El nombre va entre paréntesis y puede llevar espacios y
más paréntesis. Partir el renglón por espacios —que es lo que hace todo primer
intento— pone el estado cinco campos antes y reporta el padre como `4350`. Así
que el nombre se toma entre el **primer** `(` y el **último** `)`.

Lo mismo aplica a los controles: la etapa 31 de `verify.sh` lee el estado de un
proceso con esa misma regla, porque **un control que malinterpreta el formato no
puede comprobar un parser de ese formato**.

## Lo que se dice en vez de callarse

- **Un proceso que terminó mientras se caminaba `/proc` no es un error**: es lo
  que hacen los procesos. Se cuenta aparte (`ended_while_reading`) para que dos
  lecturas que no coinciden tengan una razón dicha, en vez de leerse como un
  defecto.
- **Un hilo del kernel no tiene línea de comandos**, y eso no es una línea de
  comandos vacía. Viaja como `null`, y la cara humana lo muestra entre corchetes
  igual que `ps`. Desde la revisión de más abajo ya no depende de que quien lo
  vea sepa leer los corchetes: `matar` se niega.
- **`uninterruptible` se nombra.** Un proceso en `D` está esperando al kernel y
  **las señales no llegan ahí**, así que un `matar` que parece no hacer nada
  tiene su explicación en esa columna.
- **La memoria residente se cuenta en las páginas del kernel**, preguntadas y no
  supuestas: son 16 KiB en algunos kernels de aarch64, y una cifra de memoria
  cuatro veces menor de lo real es peor que ninguna — es una sobre la que
  alguien actuaría.

## Cómo se comprueba

Etapa 31 de `dev/verify.sh`, con procesos de verdad, señales de verdad y todos
los controles de la regla 4:

- un proceso arrancado por la etapa se lista y se detiene, **y otro que nadie
  nombró sigue corriendo al final** — sin ese control, un `matar` que matara todo
  y uno que funcionara bien producen el mismo proceso muerto;
- la shell que ignora `TERM` como línea base de `forzar`;
- el ensayo, comprobado por el proceso que sigue vivo;
- `memoria` contra `/proc/meminfo` leído por la shell.

Veinte pruebas del motor —incluidas dos que arrancan un proceso real, lo matan y
le preguntan al kernel con qué señal murió— y catorce que teclean en el prompt de
verdad.

## Revisión del 2026-08-23 — dos cosas que aceptan la señal y la tiran

Cesar ensayó `matar` sobre un `kworker` de su máquina y Thalyx contestó *«5 would
ask to stop (kworker/R-sync_wq)»*. Es mentira, y el decreto de arriba no la
anticipó: **que `pidfd_send_signal` conteste `0` significa que el kernel se
quedó con la señal, no que le vaya a pasar algo a alguien.**

Hay dos sujetos que la aceptan y la tiran:

| sujeto | por qué | palabra | remedio |
|---|---|---|---|
| un hilo del kernel | no es un programa: `kthreadd` lo arranca con todas las señales ignoradas | `is_kernel_thread` | `cannot` |
| un proceso que ya terminó (zombi) | ya corrió su última instrucción; sólo es un renglón en la tabla hasta que su padre lo recoja | `already_ended` | `stop_the_parent` |

Los dos quedan **negados antes de mandar nada**, y la negativa se decide después
de tomar el descriptor y leer la descripción — así lo que se niega es lo que se
habría señalado, y no lo que tuviera el número un momento antes.

Medido, no recordado: `kill -9` sobre un hilo del kernel contesta `0` y el hilo
sigue ahí; `pidfd_open` sobre un zombi **funciona**, la señal se acepta y el
zombi sigue igual de muerto. Las dos cosas se comprobaron corriéndolas.

### Por qué esto es peor que un error

Un error se lee y se corrige. Una respuesta que dice *«se le pidió que pare»*
sobre algo que nunca se movió enseña que **Thalyx no es confiable**, cuando
Thalyx sólo era crédulo. Y enseña algo peor todavía: quien lo vea va a probar
`forzar`, que hace exactamente lo mismo, y va a concluir que ni `forzar`
funciona.

### `cannot` es una respuesta

`is_kernel_thread` no manda a ningún lado, y ése es el dato: no hay una segunda
cosa que intentar. Quien recibe un remedio que no existe gasta un ciclo en
descubrirlo.

`already_ended` sí manda a un lado, y es el único caso de este decreto donde el
remedio es *otro proceso*: un zombi se va cuando su padre lo recoge o cuando su
padre se va. Por eso la respuesta lleva el número del padre además de la palabra
— **un remedio que dice «para al padre» sin decir cuál no se puede ejecutar**.
Eso obligó a que la forma de negarse pueda cargar hechos, no sólo palabras.

### El ensayo tiene que llegar al mismo veredicto

`ensayo matar 2` se niega igual que `matar 2`, desde la misma función. Un ensayo
que predice algo que el verbo no hace no es una forma barata de averiguar qué
pasa: es una respuesta equivocada que hay que desaprender tecleando la de verdad,
que es el único costo que este verbo existe para quitar.

### Cómo se distingue un hilo del kernel

Por el bit `PF_KTHREAD` (`0x00200000`) del campo 9 de `/proc/<pid>/stat`. No por
tener la línea de comandos vacía, que también la tiene un zombi.

Ese valor vive en el `include/linux/sched.h` del kernel, que no se le entrega al
espacio de usuario, así que no se puede citar de un encabezado de esta máquina —
se **mide**, que es mejor. Sobre los 72 procesos de un sistema corriendo:

```
AND de los 66 hilos cuyo padre es kthreadd : 0x200040
OR  de los 6 procesos ordinarios           : 0x400100
```

El grupo se escogió por ascendencia —el pid 2 y sus hijos—, que es un hecho de la
tabla de procesos y no de este bit. Dos renglones capturados representan a cada
grupo en las pruebas.

### Cómo se comprueba

Etapa 32 de `dev/verify.sh`, y la **línea base es el defecto mismo**: `kill -9`
manda la señal al zombi, la señal se acepta, y el zombi sigue listado. Sin esa
mitad, negarse a mandarla no se distingue de mandarla.

El control es un proceso ordinario detenido **en la misma sesión**, para que un
`matar` que simplemente hubiera dejado de funcionar no pase por uno cuidadoso. Y
la etapa cuenta aparte las señales que sí salieron hacia algo que no se puede
detener, para que un `matar` crédulo se diagnostique como eso y no como otra cosa.

La etapa **no** le manda `SIGKILL` a `kthreadd` para demostrar que se ignora.
Mandarle señales al kernel de la máquina de una persona para probar un punto es
algo que este script no hace; la negativa se comprueba sin eso.

## Relacionado
- [[Tareas-Pendientes]]
- [[Superficie-para-el-LLM]]
- [[Principio-Doble-Ruta]]
- [[Estrategia-de-Pruebas]]
- [[Busqueda]]
- [[Punto-Actual]]
