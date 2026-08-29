---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-28
tags: [adopcion, agentes, mcp, vm, puente, estrategia, decreto]
---

# Los agentes de programación viven en el anfitrión, y Thalyx es la máquina donde trabajan

## El problema que esto resuelve, que no es técnico

Thalyx existe por una apuesta escrita en [[Filosofia-Fundacional]]: que un
sistema operativo construido alrededor de respuestas estructuradas, un índice
semántico y una frontera reversible hace que una IA trabaje **mejor** que un
montón de herramientas POSIX.

Esa apuesta **nunca se ha medido**. Y hasta el 2026-08-28 no había forma de
medirla, porque el único agente que podía usar las primitivas era el agente
local — un Qwen2.5-3B dentro de la máquina, que apenas alcanza para convertir
una frase en un verbo. Comparar *ese* agente con Claude sobre Linux no dice nada
sobre las primitivas: dice que un modelo de 3B es más chico que uno de frontera.

El experimento que sí dice algo es **el mismo modelo, las dos superficies**.

Para eso hace falta que un agente de programación real pueda usar Thalyx. Y ahí
está el problema, que es el que este decreto resuelve:

> Meter Claude Code adentro de Thalyx requiere Node, glibc, un toolchain, git y
> red en el guest. Eso es la etapa **G2** entera, y son semanas. Bloquear la
> primera medición científica del proyecto detrás de G2 significa construir
> durante semanas más sin saber si lo construido sirve.

## Decisión de Cesar — 2026-08-28

**Durante la etapa de adopción, los agentes de programación corren en el
anfitrión. Thalyx es la máquina de trabajo especializada que les ofrece mejores
primitivas, alcanzada por un canal local.**

Con lo que eso implica, dicho entero:

- **Thalyx sigue siendo un sistema operativo completo.** No es una capa, no es un
  servidor, no es un plugin. Lo que cambia es *dónde está parado el agente*, no
  qué es Thalyx. El decreto de [[Filosofia-Fundacional]] sigue intacto: la imagen
  lleva el kernel de Linux y **un** programa, y `make -C image count` lo dice.
- **Se distribuye y se usa, en esta etapa, como una VM para desarrolladores.**
  Que es lo que [[Condiciones-de-Adopcion]] ya decía del costo de probar: cero.
- **Correr agentes y toolchains adentro del guest es una etapa posterior**, y
  tiene nombre: `DEVELOPER RUNTIME / TOOLCHAIN MODULES`. No se empieza hasta
  saber si las primitivas aportan valor.
- **El criterio de producto inicial no es reemplazar la distro de nadie.** Es
  más chico y más honesto: que un desarrollador quiera **conservar** su VM de
  Thalyx y volver a abrirla mañana. Si no la vuelve a abrir, ninguna cantidad de
  arquitectura correcta importa.

### Y el decreto que gobierna todo lo demás

> **MCP es un ADAPTADOR, no la API interna.**

La superficie de Thalyx —`catalogue.rs`, las respuestas estructuradas, los
verbos— sigue siendo la autoridad. MCP es una traducción que vive **enteramente
en el anfitrión**, en un proceso que no sabe nada de sistemas de archivos, ni de
grafos, ni de rollback.

La forma en que este decreto se muere es de a una comodidad por vez: un caché de
contenidos «para ir más rápido», un normalizador de rutas «para que el modelo no
tenga que», un reintento que convierte un rechazo en un éxito. Cada una de esas
convierte al adaptador en un segundo Thalyx, peor, que no coincide con el
primero. Por eso la regla es mecánica: **nada en `thalyx-mcp` abre un archivo
del workspace.** Abre un socket y escribe un resumen de métricas.

## La cadena, entera

```
Claude Code / VS Code / cualquier cliente MCP   ← Fedora, el anfitrión
        ↓ MCP sobre stdio
thalyx-mcp                                      ← Fedora. Sólo adapta.
        ↓ socket UNIX de QEMU
virtio-serial  (org.thalyx.agent)               ← sin red, sin TCP, sin dirección
        ↓
un hilo de la sesión de Thalyx                  ← adentro de la máquina
        ↓ el MISMO dispatch que un teclado
verbos reales → índice / intento / journal reales
```

## Por qué virtio-serial y no HTTP

Decidido explícitamente, y no por gusto:

- Hace falta **un solo canal local** anfitrión↔VM, y nada más.
- Tiene que funcionar **aunque Thalyx no tenga red**. Concederle `net/outbound` a
  la máquina para que dos procesos del mismo anfitrión se hablen es debilitar el
  aislamiento por comodidad — el mismo argumento con el que
  [[Motor-Residente]] rechazó el servidor HTTP de `llama.cpp`.
- **El protocolo de arriba no depende del transporte.** Un frame son bytes sobre
  algo que sabe leer y escribir: hoy un puerto virtio-serial adentro y un socket
  UNIX afuera; mañana podría ser otra cosa.

El costo en el kernel es una línea: `CONFIG_VIRTIO_CONSOLE=y`. Nada de red, nada
de TCP, ninguna familia de sockets nueva.

## El protocolo

Chico y auditable, y **no sabe qué es MCP**:

```
u32 little-endian con el largo, luego exactamente esos bytes de JSON en UTF-8
```

Cuatro mensajes: `hello`, `request`, `response`, `error`. Cada petición lleva un
`id` y cada respuesta lleva el mismo. Una operación a la vez.

**Por qué un largo y no una línea:** porque una respuesta lleva el contenido de
un archivo, y todo archivo tiene saltos de línea. Un protocolo delimitado por
líneas convierte cada payload en un lugar donde se puede falsificar el marco
desde adentro de los datos. El proyecto ya escribió lo que eso cuesta, el
2026-08-08: **un límite definido de un solo lado no es un límite.** Un conteo de
bytes está definido de los dos.

## La frontera: un agente externo no es root remoto

Esto es lo que hace que el canal se pueda tener encendido.

Estar conectado a un puerto **no es autoridad**. Del otro lado de un dispositivo
de caracteres hay exactamente lo que sea que escriba en ese dispositivo, y en la
máquina de un desarrollador eso es un modelo de lenguaje con un prompt adentro.
Así que lo que un agente externo puede hacer está escrito como una **lista**, en
`crates/thalyx-cli/src/external.rs`:

- **Un workspace, y no se sale de él.** Cada ruta se resuelve dos veces: como la
  resuelve el verbo (`thalyx_files::resolve`, léxica) y como la resuelve el
  **kernel** (canonicalizada, con los symlinks seguidos). Las dos tienen que caer
  adentro. Un `..` se rechaza de plano, porque las dos formas de plegarlo dan
  archivos distintos en cuanto hay un enlace de por medio, y una comprobación que
  usara una mientras el verbo usa la otra es una comprobación que pasa
  justamente sobre la fuga.
- **Los verbos están nombrados uno por uno**, y todos son verbos que la máquina
  ya tenía. No hay `instalar-en`, no hay `apagar`, no hay `negar`, no hay
  `observar`, no hay `correr`, no hay `ejecutar`, no hay `matar`.
- **La procedencia sobrevive.** Lo que un agente externo cambió, y todo intento
  suyo de salirse del workspace, quedan en el journal con
  `operation: external_agent` y `origin: untrusted_content` — que es
  [[Marcado-de-Origen]] aplicado al caso para el que fue escrito: lo que la
  máquina hizo por cuenta propia y lo que hizo por cuenta de otro no son del
  mismo color en el registro.

**Y no es un sandbox paralelo.** No confina ningún proceso: el confinamiento de
los módulos es `thalyx-sandbox` y el cumplimiento es el LSM, y ninguno de los dos
se toca. Esto es la cosa más chica y más vieja — **una comprobación de argumentos
delante de los verbos** — y puede ser así de chica porque el agente externo nunca
corre un programa acá. Escribe.

## Las herramientas, y por qué son diez

La lista de herramientas **es un prompt**. Cada una que se le muestra a un agente
es una rama que tiene que considerar en cada turno, y una superficie con una
herramienta por verbo gasta la atención del modelo en elegir en vez de en
trabajar.

Así que la pregunta que se le hizo a cada una no fue *¿esto existe?* sino
**¿esto puede hacer que un agente programe mejor?**. Un verbo que no la pasó es
alcanzable y no está anunciado.

| herramienta | verbo de Thalyx | para qué |
|---|---|---|
| `thalyx_state` | `where` + `state` + `attempt` | qué es esta máquina, en una llamada |
| `thalyx_list` | `list` | orientarse |
| `thalyx_read` | `read` | un archivo, con su sha256 |
| `thalyx_index` | `index_build` | leer el árbol y anotar qué refiere a qué |
| `thalyx_symbol` | `symbol` | dónde se define un nombre y dónde se usa |
| `thalyx_dependencies` | `depends_on` / `depended_on_by` | impacto sin abrir archivos |
| `thalyx_find` | `find` / `grep` | el respaldo, cuando no es un símbolo |
| `thalyx_edit` | `edit` | cambiar por línea, con el deshacer en la respuesta |
| `thalyx_file` | `make_file` / `make_directory` / `remove` / `move` / `copy` | crear, borrar, mover |
| `thalyx_attempt` | `attempt` | la frontera reversible |
| `thalyx_changed` | `attempt` | qué cambió desde el punto de control |

Las **descripciones son parte del producto**. Un agente que nunca vio Thalyx
elige a partir de esas frases y de nada más, así que cada una dice *cuándo*
alcanzar la primitiva y no sólo qué devuelve.

### `changed_since` no hizo falta construirlo

`cambios` existía y **no es lo que un agente necesita**: es un ring buffer del
kernel que se consume al leerlo, no nombra rutas y necesita BPF. Lo que sí
existía era `thalyx_snapshot::difference`, que es exactamente *qué cambió desde
el punto de control* — y ya lo usaba `intento` para decir qué costaría abandonar.

Así que `thalyx_changed` es `intento`, y lo único que se construyó fue la pieza
que faltaba: los archivos **borrados** ahora se nombran, no sólo se cuentan. Dos
de las tres clases estaban nombradas y la tercera era la que un revisor más
necesita ver por nombre.

## Lo que se midió el 2026-08-28

Claude Code real (Sonnet), la misma tarea de lectura, el mismo modelo, el mismo
límite de turnos, dos copias idénticas de un proyecto Rust de 35 archivos:

| | turnos | segundos | costo | qué usó |
|---|---|---|---|---|
| **A** — Linux, `Read`/`Grep`/`Bash` | 8 | 32.8 | $0.235 | lectura y grep |
| **B** — sólo herramientas Thalyx | 7 | 17.3 | $0.089 | 4 llamadas: 1 index, 2 symbol, 1 dependencies |

El brazo B **no leyó un solo archivo** y no hizo una sola búsqueda de texto.

**Esto es una anécdota, no un resultado.** Una corrida de una tarea, y los dos
brazos difieren en cosas que nadie controló. Lo que sí existe es el arnés
—`dev/bench-external-agent.sh`— y la disciplina de anotar cada número como se
observó.

### Y lo que la corrida enseñó en contra

El brazo A encontró un dependiente que el brazo B **no**: `attempt.rs`, que usa
`Difference` sólo a través de `Plan.difference` y nunca la nombra. El índice
contesta *quién nombra este símbolo*, no *a quién le afecta transitivamente*.
Es un límite real del índice y queda escrito acá antes que en ningún otro lado.

## Las tres corridas reales, y la tarea que faltaba — 2026-08-28

Tres comparaciones con Claude Code de verdad, la misma tarea y el mismo modelo
en los dos brazos:

| tarea | correcto | costo | tiempo de pared |
|---|---|---|---|
| **lectura #1** | los dos | Thalyx **−46 %** | Thalyx **−18 %** |
| **lectura #2** | los dos | Thalyx **−62 %** | Thalyx **−36 %** |
| **edición simple, un archivo** | los dos | Thalyx −4 % ($0.0820 contra $0.0856) | Thalyx **+24 %** (15.1 s contra 12.2 s) |

La edición salió **empatada**: los mismos 6 turnos y las mismas 5 llamadas a
herramienta en los dos brazos, y el brazo B **nunca abrió un intento**.

Eso no es una derrota, es una tarea mal elegida, y vale la pena escribir por
qué: **un archivo cambiado una vez no tiene nada que revertir.** No hay
dependientes que encontrar, no hay varias partes que dejar consistentes, y
volver atrás es una edición más. Lo que esa tarea mide es el editor, y el editor
de Thalyx no es la apuesta — la frontera reversible sí. Un agente que no
necesita deshacer nada no abre un intento porque no le sirve de nada, que es
exactamente lo que hizo.

### La tarea `reversible`

`dev/bench-external-agent.sh --task reversible`. Un símbolo se renombra en su
definición y en todo lo que depende de él, el agente comprueba qué tocó, y al
final **deja el árbol exactamente como estaba, byte por byte**. La pregunta,
escrita antes de correrla:

> Cuando una tarea exige cambiar varias partes relacionadas y después volver con
> certeza al estado inicial, ¿la frontera reversible de Thalyx le reduce el
> trabajo al mismo agente frente a Linux?

Cuatro cosas la mantienen honesta:

1. **El prompt es una sola cadena** para los dos brazos, y no nombra ninguna
   herramienta, ni MCP, ni Thalyx. El `--self-test` del arnés lo comprueba
   leyendo el propio archivo: cuenta que haya exactamente un `claude -p` y busca
   las palabras prohibidas en el prompt. Así no se puede pudrir sin que algo
   truene.
2. **El cambio es mecánico.** `UidRegistry` pasa a ser `UidRegistryRenamed` — un
   sufijo, no un nombre nuevo. No hay criterio que ejercer y por lo tanto no hay
   diferencia de criterio entre los brazos.
3. **El brazo A puede restaurar como quiera.** Su copia trae el `.git` del
   proyecto y tiene `Bash`; `git checkout -- .` es una respuesta perfectamente
   válida, y si resulta ser la barata, ése es el resultado. Lo único prohibido
   —en los dos brazos— es compilar y correr pruebas, porque el brazo B no tiene
   shell y no podría: dejarlo habría medido `cargo` en una columna y nada en la
   otra.
4. **"Restaurado" se comprueba desde afuera**, con un digest del árbol entero,
   no preguntándole a la máquina que hizo la afirmación.

### La trampa que esa tarea trae adentro

**Un agente que no hace nada restaura el árbol perfecto.** Un veredicto leído
del hash solo pondría a un agente que se rehusó por encima de todos los que lo
intentaron — y lo pondría más alto en el brazo B, que es la dirección en la que
esta comparación no puede equivocarse nunca.

### Revisión — 2026-08-28: el oráculo daba tres falsos positivos

Lo escrito arriba era la intención. Lo implementado no la cumplía, y una
auditoría lo encontró antes de que se gastara una sola corrida pagada, que es
el único motivo por el que esto es una revisión y no una retractación.

`reversible.passed` era una conjunción de tres cosas, y la segunda —**cambió de
verdad**— se leía como *"el nombre nuevo apareció en alguna llamada"*. Tres
corridas que no cambiaron nada la satisfacían:

- `Grep {"pattern": "UidRegistryRenamed"}` nombra el nombre nuevo y es lectura;
- un `Edit` cuyo `old_string` no coincidió con nada nombra el nombre nuevo,
  vuelve con `is_error` y el espacio de trabajo nunca tuvo el nombre nuevo ni
  por un instante;
- y el `result` del propio agente podía traer `is_error: true` —una corrida
  muerta en su límite de turnos— sin que el veredicto lo mirara.

Además el digest era `find -type f | xargs sha256sum`, o sea **contenido y nada
más**, mientras el prompt promete *byte por byte, ningún archivo agregado ni
quitado*. `-type f` no empareja un symlink: un agente que dejara `src/lib.rs`
como enlace a `/etc/passwd` había *borrado un archivo*, y uno que dejara un
fuente en modo 777, o un directorio donde había un archivo, restauraba el árbol
perfecto según esa tubería.

**Lo que exige ahora**, cinco propiedades y cada una de un instrumento distinto:

| | Propiedad | De dónde sale |
|---|---|---|
| A | `mutation_attempted` — hubo una llamada que sólo puede mutar, que nombra el nombre nuevo, **y cuyo resultado volvió sin error** | el stream, pero de la respuesta de la herramienta, no del pedido del modelo |
| B | `intermediate_state` — el espacio de trabajo realmente tuvo otra cosa un rato | **fuera del agente**: los mtimes del anfitrión antes y después, o el contador `mutations` de `thalyx-mcp --metrics` |
| C | `completed_normally` — el `result` de la corrida no dijo `is_error` | el evento final del agente |
| D | `task_success` — la respuesta nombró los archivos que la verdad conocida exige | `--expect-file` |
| E | `restored` — el digest volvió | el anfitrión |

**Ningún texto del propio agente prueba que modificó algo.** Ésa es la regla, y
B es la que la sostiene: es la única evidencia que no pasa por lo que el agente
dice de sí mismo. Para el brazo A son dos recorridas de mtimes del mismo
directorio —una antes de arrancar, otra al terminar— que no cuestan nada
durante la corrida y por lo tanto no rompen la comparabilidad. Para el brazo B
el espacio de trabajo vive adentro de la VM y el anfitrión no puede recorrerlo
mientras corre, así que el testigo es el adaptador: `thalyx-mcp` cuenta las
llamadas que cambiaron el espacio de trabajo **y volvieron sin error ni
rechazo**, de su lado del cable. Si ninguno de los dos existe, B es
`not_proven` y `THALYX_REQUIRE_MUTATION_WITNESS=1` lo convierte en falla —
regla 3, una variable por requisito, distinta de la del restore porque son
requisitos distintos.

Hay una excepción a A, y es a favor del brazo A: `sed -i` llega como `Bash` y el
stream no lo distingue de `ls`. Exigir una herramienta-que-sólo-muta castigaría
al brazo A por usar la herramienta que se le dio. Así que una llamada que
nombró el nombre nuevo, volvió sin error **y dejó el sistema de archivos
movido** también cuenta — y ninguno de los tres falsos positivos de arriba puede
producir eso, porque ninguno escribe un archivo.

**Y el digest es ahora un manifiesto**: para cada entrada bajo la raíz, su
**tipo**, sus **bits de permiso**, su **contenido** si es archivo regular y su
**destino** si es symlink. Eso es lo que la frase "byte por byte, ningún archivo
agregado ni quitado" promete. Deliberadamente *no* es un diff genérico de
sistema de archivos: dueño, mtime, xattrs e inodos quedan fuera, porque ninguno
es algo que la tarea le pida al agente conservar y cada uno haría fallar al
brazo A una restauración que hizo bien.

Hay **una sola implementación** de qué es un árbol, en `dev/bench-summary.py`;
`dev/bench-external-agent.sh` la llama. Antes había dos —una tubería de `find`
en el shell y el resumen razonando sobre lo que producía— que coincidían por
casualidad y podían dejar de coincidir sin que fallara nada. Regla 5.

Los siete falsos positivos —lectura que menciona el nombre nuevo, edición
fallida que lo menciona, cero mutaciones, mutación sin restore, mutación con
restore, agente terminado en error, verdad conocida incompleta— son casos
nombrados en `dev/bench-summary.py --self-test`, y si alguno vuelve a pasar, el
self-test falla.

### Y el brazo B se comprueba en dos pasos, a propósito

El espacio de trabajo del brazo B vive adentro de la VM, en una imagen Btrfs que
QEMU tiene abierta para escritura. Montarla mientras la máquina corre es como se
corrompe un store. Así que el hash de *después* es necesariamente una segunda
pasada, con la máquina apagada:

```sh
sudo make -C image agent-export INTO=/tmp/armB-after
dev/bench-external-agent.sh --project … --symbol … --task reversible \
    --arms none --restored-b /tmp/armB-after
```

Mientras no se haga, el resumen dice `restore_check: not_proven` y no supone
nada. `THALYX_REQUIRE_RESTORE_CHECK=1` convierte ese salto en falla — regla 3,
una variable por requisito.

### Revisión — 2026-08-28: la frontera del espacio de trabajo era una comparación, no una frontera

La comprobación de contención era:

```
canonicalize(el nombre)  →  ¿empieza con el espacio de trabajo?  →  el verbo abre el nombre original
```

Todos los pasos correctos, y la secuencia mal — porque es una secuencia. Entre
la comparación y la apertura hay un momento, y cualquier cosa que pueda escribir
adentro del espacio de trabajo puede gastar ese momento cambiando un directorio
por un symlink a otro lado. Thalyx —que no está adentro del sandbox de nadie—
abre entonces el destino nuevo con el alcance de Thalyx.

No era teórico. La prueba que lo encontró cambia `src` entre un directorio real
y un enlace a otro árbol mientras un agente lee `src/main.rs` en un ciclo:
**57 de 4000 lecturas devolvieron el contenido de un archivo fuera del espacio de
trabajo.** Con `editar`, `rm`, `cp` y `mv` la misma ventana escribe y borra
afuera.

Y era exactamente la secuencia que `crates/thalyx-core/src/api.rs` fue reescrito
para dejar de usar, en la superficie de módulos, hace semanas. Dos filosofías
distintas adentro del mismo sistema para la misma pregunta.

**Ahora hay una.** `crates/thalyx-cli/src/confine.rs`: `openat2` con
`RESOLVE_BENEATH` contra un descriptor del espacio de trabajo, abierto una vez
cuando la sesión abre. El kernel se niega a resolver fuera de ese directorio
*durante* la resolución, así que no hay respuesta intermedia que nadie pueda
invalidar — la comprobación y la apertura son la misma llamada.

Los verbos siguen recibiendo una ruta, porque reescribirlos todos para tomar
descriptores sería una segunda capa de sistema de archivos al lado de la
primera. Lo que reciben es `/proc/self/fd/N`: el descriptor que el kernel
resolvió, que apunta al inodo y no al nombre. La ruta que la *respuesta* lleva
no cambia — el agente sigue viendo `src/main.rs`. Son dos argumentos en la misma
función, no una forma nueva para cada verbo.

Dos anclajes, y cuál se usa lo decide sobre qué actúa la llamada al sistema:

- **la cosa** — `leer`, `ls`, la lectura de `editar`: un descriptor *es* la cosa;
- **la entrada de directorio** — `crear`, `rm`, `mv`, el destino de `cp`:
  `unlink` y `rename` actúan sobre un nombre adentro de un directorio, y no hay
  nombre adentro de un enlace de procfs. El padre queda fijado y el último
  componente se busca adentro de él.

**Lo que se perdió, dicho claro.** `RESOLVE_BENEATH` rechaza **todo symlink
absoluto**, incluido uno que habría caído adentro del mismo espacio de trabajo.
Un proyecto con `código → /home/proyecto/src` deja de funcionar; con
`código → src` sigue funcionando, porque el kernel contiene un enlace relativo
por construcción. Es la misma pérdida que `api.rs` aceptó y por la misma razón:
decidir si un enlace absoluto cae adentro exige resolverlo en espacio de usuario
primero, que es la comprobación de dos pasos que todo esto existe para eliminar.
Y la dirección de la pérdida es la correcta — a un agente se le niega algo que
debía permitírsele, que alguien nota y reporta, en vez de permitírsele algo que
debía negársele, que no nota nadie.

**Lo que sigue abierto.** Una ruta que se está *creando* no existe, así que no
hay nada que anclar: se fija el padre y el último componente se busca adentro.
Un symlink plantado en ese último componente entre la comprobación y la creación
todavía se seguiría. Es más angosto que lo que reemplaza por cada directorio de
la ruta, y está escrito en `confine.rs` en vez de quedar como diferencia entre lo
que ese módulo afirma y lo que hace.

Lo prueban cuatro tests adversariales en `external.rs`, uno por verbo
—`leer`, `editar`, `rm`, `ls`— que corren el intercambio contra 4000 peticiones.
Son de un solo lado, regla 7: una corrida donde el hilo que intercambia nunca
ganó la carrera no prueba nada, así que la afirmación es sobre la dirección a la
que el ruido no llega —**cero** escapes— y al lado va el control, el conteo de
rechazos, sin el cual una corrida donde no pasó nada se vería igual. Se
comprobaron quitando el confinamiento de la sesión: los cuatro fallan.

### Lo que todavía no está controlado, y hay que decidir

El brazo A trabaja *dentro* de la copia, así que Claude Code le carga el
`CLAUDE.md` del proyecto; el brazo B trabaja en un directorio vacío y no lo ve.
Eso le suma tokens al brazo A por algo que no es la tarea, y **le suma al lado
que favorece a Thalyx**, que es justo el sesgo que no se vale tener. Se evita
apuntando `--project` a una copia sin `CLAUDE.md`; los dos brazos siguen siendo
bytes idénticos porque los dos salen de la misma copia.

## Lo que esta etapa NO hace

Escrito para que nadie lo empiece por accidente:

- no mete Node adentro de Thalyx;
- no mete Claude Code adentro del guest;
- no mete VS Code Server;
- no mete glibc al sistema base;
- no construye toolchains;
- no implementa git;
- no le da internet al guest.

Todo eso es `DEVELOPER RUNTIME / TOOLCHAIN MODULES`, y va después de saber si
esto sirve.

## Lo que hace falta y no está probado

- **virtio-serial no ha llevado un byte todavía.** El contenedor donde se
  construyó esto no tiene QEMU. Todo lo que está encima del transporte —el
  marcado, la frontera, los verbos, las respuestas— sí corrió, sobre un socket
  UNIX, que es el mismo `bridge::serve` sobre otro par de descriptores.
  `dev/verify.sh` §48 comprueba lo único que se puede comprobar sin arrancar:
  que el nombre del puerto que la máquina busca y el que QEMU crea son el mismo.
- **El ciclo `intento` completo a través del puente** necesita Btrfs, que este
  contenedor no tiene. `dev/verify.sh` §47 lo corre entero en la máquina de
  Cesar: abre un intento, crea, edita y borra archivos, abandona, y compara el
  árbol byte por byte contra como estaba.

## Referencias

- [[Filosofia-Fundacional]] — el kernel y un programa; la apuesta que esto mide.
- [[Superficie-para-el-LLM]] — los cinco costos; el puente es cómo se le cobran
  a un agente que no vive acá.
- [[Principio-Doble-Ruta]] — ahora son tres rutas y una sola implementación.
- [[Marcado-de-Origen]] — de dónde viene lo que hizo la máquina.
- [[Camino-Confiable]] — por qué el primer `abandonar` pregunta.
- [[Condiciones-de-Adopcion]] — el costo de probar es cero, y por eso es una VM.
- [[Motor-Residente]] — el mismo argumento sobre red que se usó acá.

## Revisiones

### 2026-08-28 — El registro de evidencia se muda a un documento canónico
**Antes:** las mediciones vivían en esta nota, que es donde se hicieron.
**Ahora:** el registro canónico —cada corrida con sus números completos, los dos
brazos, los límites y el protocolo del bug real— es [[Evidencia-de-Agentes]].
Las tablas de arriba **se quedan donde están**: son el estado en que quedó este
decreto el día que se escribió, y la nota canónica no las contradice, las amplía
(las tres corridas con todas sus métricas, no sólo costo y tiempo).
**Motivo:** una cifra repetida en dos lugares diverge en cuanto se agrega la
cuarta corrida. Lo nuevo se escribe allá; esto queda como historia con fecha.
**Y lo que este decreto conserva entero:** MCP sigue siendo un adaptador, no la
API interna y no la visión final; nada en `thalyx-mcp` abre un archivo del
workspace; y el agente externo sigue siendo software no confiable. Que la
prioridad de la etapa pase a medir agentes —[[Prioridad-Operativa]]— no relaja
ninguna de esas tres.
