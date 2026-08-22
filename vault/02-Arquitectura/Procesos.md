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
  igual que `ps`, porque quien ve `[kworker/0:1]` sabe que no debe matarlo.
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

Diecisiete pruebas del motor —incluidas dos que arrancan un proceso real, lo
matan y le preguntan al kernel con qué señal murió— y once que teclean en el
prompt de verdad.

## Relacionado
- [[Tareas-Pendientes]]
- [[Superficie-para-el-LLM]]
- [[Principio-Doble-Ruta]]
- [[Estrategia-de-Pruebas]]
- [[Busqueda]]
- [[Punto-Actual]]
