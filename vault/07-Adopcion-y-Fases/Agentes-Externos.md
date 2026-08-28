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
4. **"Restaurado" se comprueba desde afuera**, con `sha256` sobre el árbol
   entero, no preguntándole a la máquina que hizo la afirmación.

### La trampa que esa tarea trae adentro

**Un agente que no hace nada restaura el árbol perfecto.** Un veredicto leído
del hash solo pondría a un agente que se rehusó por encima de todos los que lo
intentaron — y lo pondría más alto en el brazo B, que es la dirección en la que
esta comparación no puede equivocarse nunca.

Por eso `reversible.passed` es una conjunción, y cada parte viene de un
instrumento distinto: **cambió de verdad** (el nombre nuevo apareció en alguna
llamada, según el stream del propio agente), **restauró** (los bytes, según el
anfitrión), y **contestó bien** (nombró los archivos que la verdad conocida
exige, `--expect-file`). Si alguna se desconoce, no hay veredicto: no es `false`.

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
