---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-25
tags: [agente-ajeno, ejecutar, sandbox, g1, fase-2]
---

# Programas ajenos: `ejecutar`

Es **G1** de [[Superficie-para-el-LLM]], el punto que bloquea la vara del
proyecto desde que se midió el 2026-08-23 y que [[Que-Necesita-Un-Agente-Ajeno]]
dejó con nombre y sin ambigüedad:

> **G1, ejecutar un proceso arbitrario** — sigue entero. No es una llamada que
> falte ni una ruta: es que hoy `correr` sólo lanza módulos instalados y
> firmados, y un agente ajeno por definición no es ninguna de las dos cosas.

Cesar delegó la forma el 2026-08-25 —*«lo que veas conveniente que sea coherente
con nuestra filosofía»*—, así que esta nota es la forma y **la razón por la que
es coherente**.

## Por qué esto no contradice nada

[[Filosofia-Fundacional]] no lo permite: **lo exige**. La vara está escrita ahí
en las palabras de Cesar —*«un agente ajeno, ya escrito, corriendo sobre Thalyx,
y trabajando mejor que sobre Linux o macOS»*— junto con el estado real de
entonces, que sigue siendo el de hoy: *«hoy Claude Code no podría arrancar en
Thalyx»*. Un sistema que no puede lanzar un programa que no escribió él mismo no
llega a esa vara; ni siquiera llega a la línea de salida.

Lo que sí hay que cuidar es la firma, y se cuida **no tocándola**:

> Un programa ajeno **no es un módulo, y nunca se convierte en uno.**

La firma de [[Sistema-de-Modulos]] significa *alguien respondió por esto*. Si
Thalyx firmara al vuelo lo que se le pide ejecutar, pasaría a significar *esto
pasó por aquí*, que es dejar la palabra sin significado para quien lea la
siguiente — el mismo error que [[Superficie-para-el-LLM]] evitó el 2026-08-24 al
separar un contrato de un plan de verbo. Así que son dos verbos y no uno:

| | `correr <id>` | `ejecutar <ruta>` |
|---|---|---|
| qué lanza | un módulo instalado y firmado | un programa cualquiera |
| quién respondió por él | su publicador, con su llave | **nadie** |
| canal con la API de Thalyx | sí, nace con él | **no, nunca** |
| permisos | los que el store tiene concedidos | los que se nombran en el renglón |
| `sin-confinar` | existe, y queda en el journal como degradado | **no existe** |

## Las cinco decisiones, cada una con lo que evita

### 1. No hay canal, y ésa es la línea

Un módulo **nace sosteniendo** un canal a la API de Thalyx. Un programa ajeno no
recibe ninguno, y no porque no sepa hablarlo: porque la API es la superficie que
Thalyx le da a algo que fue firmado, instalado y al que un humano le concedió
permisos por su nombre. Un invitado corre; no se le da la casa.

Eso es también lo que impide que este verbo sea una puerta trasera al decreto de
firma. Por `ejecutar` no se instala nada, no se concede nada persistente, no se
toca el store y no se pide nada por el canal, porque no hay canal.

### 2. Se confina siempre, y aquí no hay modo degradado

`correr` tiene `sin-confinar`, y existe por una razón buena: un modo malo que se
alcanza a propósito y se nombra en el journal es mejor que uno que se alcanza por
accidente y no se nombra en ningún lado. Esa razón **no aplica aquí**. La
justificación de `sin-confinar` es que un humano leyó el manifiesto de ese módulo
y su publicador respondió por él; de un programa ajeno nadie respondió nada.

Así que `ejecutar` sin confinamiento **no es un modo que exista**. Si la máquina
no puede hacer cumplir nada, el verbo se niega y dice por qué, igual que `correr`
— [[Sandbox-Ejecucion]], falla cerrado.

### 3. Ve lo que se le nombró, y nada más

Dentro del pivote ve tres cosas:

- **su propia carpeta**, montada de sólo lectura en `/module`, que es donde el
  programa está;
- las rutas de sistema de sólo lectura que ya tiene cualquier módulo — `/usr`,
  `/lib`, `/lib64`, `/bin`, `/sbin`, `/etc` — que es lo que deja arrancar a un
  binario enlazado dinámicamente;
- **lo que se nombró en el renglón**, y sólo eso.

`leyendo <ruta>` y `escribiendo <ruta>` van adelante, como manda [[Palabras]], y
el sujeto es el programa y sus argumentos. Cada ruta concedida pasa por el
[[Camino-Confiable]] antes de que el programa exista: se muestran una por una,
dibujadas por Thalyx, y el silencio no es un sí.

**El grano es el que se nombró.** Conceder un archivo concede un archivo, no la
carpeta donde vive — que es la forma obvia de hacerlo funcionar y la que entrega
todo lo demás que hay en esa carpeta. Está probado en `isolation.rs` desde el
2026-08-25, con su control.

### 4. Su usuario es suyo, y es el mismo mañana

Un módulo recibe un uid asignado una vez y para siempre ([[Sandbox-Ejecucion]]).
Un programa ajeno no tiene id, así que la llave es **la ruta canónica del
binario**, con el prefijo `foreign:` para que no pueda chocar con el id de un
módulo.

La consecuencia es la que se quiere: el mismo programa es el mismo usuario entre
corridas, así que lo que escribió ayer sigue siendo suyo hoy, y **dos programas
ajenos distintos no comparten usuario** aunque los lance la misma persona el
mismo día.

### 5. El journal lo distingue de lo demás

La operación se llama `run_foreign` y no `run_module`, y lleva la ruta, las
concesiones y el código de salida. [[Marcado-de-Origen]] pide poder separar lo
que hizo el agente de lo que ya estaba; esto es la mitad más gruesa de esa
pregunta: separar **lo que hizo un programa que nadie firmó** de lo que hizo
Thalyx.

## Lo que este decreto no autoriza

- **No abre la red.** Es `G3` y sigue entera. Un programa ajeno arranca sin red,
  como cualquier módulo sin la concesión.
- **No es E1.** Una tarea con identidad y **concesión que expira** sigue sin
  construirse; lo de aquí son concesiones para una corrida, que terminan cuando
  el proceso termina.
- **No resuelve `G2`.** La imagen sigue llevando el kernel y un programa, y
  dentro de ella no hay libc: `ejecutar` sirve donde hay rutas de sistema que
  montar —la máquina de desarrollo—, y dentro de la imagen instalada sirve para
  lo que esté enlazado estáticamente. La pregunta del ABI sigue abierta en
  [[Tareas-Pendientes]] y esta nota no la contesta.
- **No le quita nada a `correr`.** El decreto de firma sigue rigiendo los
  módulos, entero.
- **No baja el [[Principio-Doble-Ruta]]:** nace con las dos caras, y la humana no
  es la que se agrega después.

## Cómo se comprueba

Etapa 36 de `verify.sh`, y una prueba de integración que **lanza un programa de
verdad** y le pregunta a él qué ve, con la columna de afuera al lado — regla 2 de
[[Estrategia-de-Pruebas]]. Lo que hay que comprobar:

1. un programa que nadie firmó corre, y su código de salida llega;
2. dentro no ve nada del anfitrión que no se le haya nombrado, y sí ve lo
   nombrado;
3. una ruta concedida para leer no se puede escribir;
4. sin confirmación no corre nada, y el silencio no es un sí;
5. el journal lo llama `run_foreign` y no `run_module`;
6. sin nada que haga cumplir la política, el verbo se niega;
7. **con la política cargada pero en modo observación, el verbo también se
   niega** — ver la revisión de abajo.

## Revisiones

### 2026-08-25 — «cargado» y «negando» son dos preguntas

**Qué decía antes.** El decreto decía *«si la máquina no puede hacer cumplir la
política, el verbo se niega»*, y el código preguntaba eso con
`policies.is_available()` — que responde **si el mapa de políticas se abre**.

**Qué pasó.** Cesar corrió `ejecutar /usr/bin/node --version` justo después de
`verify.sh`, que desengancha el LSM al salir, y leyó la negativa correcta. El
remedio que le dio esa negativa es `make -C lsm load`. Y `make -C lsm load`
**aterriza a propósito en modo observación**: los ganchos corren, cada negación
se escribe en el anillo, y ninguna se aplica.

O sea que el remedio del mensaje dejaba la máquina en el estado exacto donde el
verbo *sí* arrancaba al invitado y el kernel no le negaba nada. Nadie en el lado
de Rust había leído nunca el mapa `thalyx_enforcing`; sólo el `Makefile` lo
consultaba, con `bpftool`. `thalyx enforce status` imprimía «kernel policy map:
present» y se callaba.

**Qué dice ahora.** Son dos preguntas y se hacen las dos:

| | módulo firmado | programa ajeno |
|---|---|---|
| mapa sin cargar | se niega, ofrece `sin-confinar` | **se niega**, no hay a qué caer |
| cargado, observando | **corre degradado, y el journal lo dice** | **se niega**: `make -C lsm enforce` |
| no se pudo leer el modo | corre degradado, y el journal lo dice | **se niega**: regla 9 |
| cargado, negando | corre | corre |

La asimetría es la misma del resto de la nota, y por la misma razón. A un módulo
lo firmó alguien y un humano leyó su manifiesto; correr degradado con el journal
diciéndolo es una decisión que alguien puede auditar. Detrás de un invitado no
hay nadie: el confinamiento es *todo* lo que lo respalda, y un confinamiento que
no niega no es un confinamiento.

**Lo que esto abrió.** Cambiar el modo todavía se hace con `bpftool`, que la
imagen no tiene y no va a tener. Queda escrito en [[Tareas-Pendientes]].

### 2026-08-25 — un invitado sin concesiones no podía ni existir

**Qué pasó.** Con el modo de enforcement ya corregido, Cesar corrió
`ejecutar /usr/bin/node --version` — sin `leyendo`, sin `escribiendo`, el caso
ordinario. El confinamiento se armó entero (cgroup 38600, usuario 700000,
pivote, filtro de 130 llamadas) y murió antes de `node`:

```
thalyx: I/O error at /sys/fs/cgroup/thalyx/foreign.node-22.…/cgroup.procs:
Operation not permitted
```

**Por qué.** Sin concesiones, la política sale `allowed=0x0`. El gancho
`lsm/file_open` **no mira rutas**: mira si la operación es lectura o escritura y
consulta el bit. Así que con `0x0` se niega *cualquier* apertura de archivo.

El lanzador escribe su pid en `cgroup.procs` —desde **fuera** del cgroup, así que
esa pasa— y acto seguido **lo vuelve a leer** para comprobar que la entrada tomó
efecto. Esa lectura ya es desde dentro, y es la primera vez que la política del
cgroup contesta algo. Ni siquiera llegaba a `exec`; y si hubiera llegado, abrir
el binario también es una apertura de archivo.

**Ya se había topado con esto, y se rodeó en vez de arreglarse.** La cabecera de
`lsm/demo-enforcement.sh` dice que pone en el mapa *«filesystem allowed, network
denied»*. Tenía que hacerlo: con el sistema de archivos negado, el `python3` que
corre dentro del cgroup no habría arrancado, y el demo habría estado midiendo
`exec` en vez de `connect`. Ese hecho nunca salió del script.

**Qué dice ahora.** El espacio de nombres de montaje decide **qué** ve un
programa confinado; la política decide **leer o escribir** sobre eso. Las dos
sólo componen si el programa puede leer lo que se le montó. Así que la lectura
de lo visible **no es una concesión**: es el piso que hace que el montaje
signifique lo que al humano se le dijo que significa —«su propia carpeta, de
sólo lectura, y las rutas de sistema»—, y `escribiendo` sigue siendo lo único
que abre la escritura.

Vive en `thalyx_permd::CONFINED_FLOOR`, se le da a la **política y nunca al
perfil**: expresado como un permiso sobre `/` habría hecho que `RootFs` montara
el sistema de archivos entero del anfitrión dentro del sandbox, que es lo
contrario de lo que hace. Aplica a módulos igual que a invitados — un módulo sin
permiso de lectura tampoco podía abrir su propio binario.

### 2026-08-25 — una concesión a un invitado dura la corrida

**Lo que el piso dejó ver.** Una entrada de política tiene **una** fecha de
vencimiento, y las concesiones de `ejecutar` eran JIT: treinta segundos. Pasados,
expiraba la entrada entera —el piso de lectura incluido—, así que
`ejecutar leyendo <ruta> …` no podía correr más de medio minuto y moría en su
siguiente apertura de archivo. La vara del proyecto es un agente que corre
minutos.

Lo irónico es que el comentario que estaba encima de esa línea ya decía lo
correcto: *«una concesión hecha en una línea vive lo que vive el proceso»*. El
tipo elegido para lograrlo hacía lo contrario.

**Decidido por Cesar el 2026-08-25: la concesión dura la corrida.** El tipo es
`Session`, que no lleva plazo, y `release()` la retira cuando el proceso sale y
se lleva el cgroup con ella.

**Lo que se cede, dicho y no escondido.** Los treinta segundos eran también el
respaldo del kernel contra un Thalyx que se cuelga y nunca llama a `release()`.
Sin ellos, ese caso deja una política viva sobre un cgroup vacío. Lo acota que
el nombre del cgroup es determinista —la siguiente corrida del mismo programa lo
reutiliza y sobrescribe la entrada— y que `thalyx enforce status` lo muestra.

`Persistent` sigue estando prohibido aquí, por lo de siempre: sería un permiso
que nadie podría encontrar después para retirarlo, porque está pegado a una ruta
y no a nada cuyo nombre conozca el store.

**Cómo se comprueba.** En la etapa 36, con un invitado que duerme **35 segundos**
y luego lee lo que se le concedió. Es la única forma que distingue las dos
respuestas: la corrida tiene que ser más larga que el plazo que ya no debe
existir. Cuesta 35 segundos de reloj en cada `verify.sh` y los vale.

### 2026-08-26 — Thalyx enciende y apaga su propio guardia

**El hueco que dejó abierto el arreglo del 25.** Ese día Thalyx aprendió a
**leer** el modo del kernel —el mapa `thalyx_enforcing`, con `bpf(2)`, sin
`bpftool`— y por eso `ejecutar` se niega mientras el kernel sólo observa.
**Cambiarlo** seguía siendo `make -C lsm enforce`, que es `bpftool`, que la
imagen no lleva y no va a llevar.

O sea: dentro de la máquina, cada negativa cuyo remedio era *«hazlo vinculante»*
nombraba un comando que ahí no existe. Es el mismo hueco que
[[Cargador-BPF-Propio]] cerró para **cargar** y dejó abierto para el **modo**.

**Dos verbos, no uno con argumento.** [[Busqueda]] ya había fijado la forma —un
verbo cuyo significado depende de una palabra de después se puede pedir mal en
silencio— y aquí pesa más que allá, porque las dos direcciones no son
comparables:

| | `negar` | `observar` |
|---|---|---|
| qué hace | el kernel empieza a negar de verdad | el kernel deja de negar |
| a quién afecta | a lo que se salga de lo concedido | a **todo lo confinado en este momento** |
| pide confirmación | no | **sí**, [[Camino-Confiable]] |
| cara estructurada | contesta | **se niega**: `needs_a_human` |

`negar` aprieta: si rompe algo, el algo lo dice, fuerte, en el momento.
`observar` afloja, y una máquina que dejó de negar en silencio se ve idéntica a
una que niega y no tiene qué negar — el fallo sin síntoma que todo este
subsistema existe para no tener. Por eso pregunta el que afloja y no el que
aprieta.

Que el modelo **pueda proponer** `observe` no lo contradice: es el decreto del
2026-08-24 funcionando como está escrito. La línea está en lo que la máquina
**hace** sin un humano en una terminal, no en lo que el modelo puede **decir**.

**Los remedios cambiaron de comando.** `make -C lsm enforce` salió de todos los
mensajes: ahora dicen `negar`, o `thalyx enforce mode enforcing` fuera de una
sesión. Los de «no está cargado» nombran `thalyx enforce attach` antes que el
objetivo de `make`. Un remedio que no se puede correr donde se imprime es prosa.

**Cómo se comprueba, y con qué instrumento.** Etapa **37** de `verify.sh`, y
cada medición la hace **`bpftool map dump`** y no Thalyx. Regla 5: lo que está
bajo prueba es Thalyx escribiendo cuatro bytes con `bpf(2)`, y preguntarle a
Thalyx si llegaron pasaría en una compilación donde la lectura y la escritura
están mal en la misma dirección — que es la forma más probable de equivocarse
en esto. La etapa lleva línea base (la deja observando y lo confirma), el acto,
**el control** (que la vuelve a mover, porque un `set_enforcement` que escribe
`1` siempre pasa todo lo demás), el verbo de sesión, y el `n` con su `y` al
lado. La escritura además se **relee**: `bpf_obj_get` sobre cualquier mapa tiene
éxito, así que sin la relectura apuntar esto a algo con forma de mapa reportaría
que la máquina ya niega.

## Relacionado
- [[Superficie-para-el-LLM]]
- [[Que-Necesita-Un-Agente-Ajeno]]
- [[Sandbox-Ejecucion]]
- [[Sistema-de-Modulos]]
- [[Camino-Confiable]]
- [[Filosofia-Fundacional]]
- [[Palabras]]
