---
tipo: estado-vivo
estado: activo
fecha-actualizacion: 2026-08-28
tags: [continuidad, punto-actual, sesiones]
---

# Punto actual

> **Este archivo se actualiza cada vez que se termina algo.** Existe para que
> una sesión nueva —humana o de IA— sepa exactamente dónde quedó el proyecto
> sin que nadie tenga que recordarlo. Si algo importante vive solo en una
> conversación, esa conversación se pierde y el conocimiento con ella.
>
> Para *cómo* trabajar en el proyecto, ver `CLAUDE.md` en la raíz del repo.

> ## El primer agente de programación real usa las primitivas — 2026-08-28
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Hasta hoy la apuesta de [[Filosofia-Fundacional]] —que un sistema construido
> alrededor de respuestas estructuradas, un índice semántico y una frontera
> reversible hace que una IA trabaje mejor— **nunca se había medido**, y no había
> forma de medirla: el único agente que podía usar las primitivas era el Qwen de
> 3B de adentro, y compararlo con Claude sobre Linux mide el tamaño del modelo,
> no la superficie.
>
> Ahora hay puente. **Claude Code real, corriendo en el anfitrión, hizo una tarea
> de lectura usando sólo verbos de Thalyx**: cuatro llamadas —un `indexar`, dos
> `buscar`, un `usan`— sin abrir un archivo y sin una sola búsqueda de texto. El
> decreto está en [[Agentes-Externos]] y lo importante de él es dónde vive cada
> cosa: **MCP es un adaptador, en el anfitrión; la superficie de Thalyx sigue
> siendo la autoridad.**
>
> La cadena es
> `Claude Code → thalyx-mcp → socket de QEMU → virtio-serial → un hilo de la
> sesión → el MISMO dispatch que un teclado → índice, intento y journal reales`.
> Sin red, sin TCP, sin dirección: `CONFIG_VIRTIO_CONSOLE=y` es todo el costo en
> el kernel.
>
> **Un agente externo no es root remoto.** `crates/thalyx-cli/src/external.rs`
> es una lista de verbos y un guardián de rutas que resuelve cada una dos veces
> —como la resuelve el verbo y como la resuelve el kernel— y exige que las dos
> caigan adentro del workspace. `apagar`, `instalar-en`, `correr`, `ejecutar`,
> `negar` y `matar` no son alcanzables. Lo que un agente externo cambió, y todo
> intento suyo de salirse, quedan en el journal marcados `untrusted_content`.
>
> **Lo que la primera medición dio**, con Sonnet, la misma tarea, dos copias
> idénticas de un proyecto de 35 archivos: 8 turnos y 32.8 s con `Read`/`grep`
> contra 7 turnos y 17.3 s con las herramientas de Thalyx. **Es una anécdota, no
> un resultado** — una corrida de una tarea. Lo que existe es el arnés,
> `dev/bench-external-agent.sh`, y una corrida real que lo prueba.
>
> Y enseñó algo en contra, que está escrito en el decreto: el brazo de Linux
> encontró un dependiente que el índice no, porque usa el símbolo a través de un
> campo y nunca lo nombra. El índice contesta *quién nombra esto*, no *a quién le
> afecta*.
>
> **Lo que falta arrancar para creerlo del todo.** virtio-serial no ha llevado un
> byte: este contenedor no tiene QEMU, y lo que corrió fue el mismo `serve` sobre
> un socket UNIX. Y el ciclo `intento` completo a través del puente necesita
> Btrfs. Los dos son `dev/verify.sh` §47 y §48, y los dos corren en la máquina de
> Cesar:
>
> ```
> make -C image agent PROJECT=/ruta/a/un/proyecto
> # en otra terminal, cuando la máquina esté arriba:
> dev/agent-connect.sh
> claude
> ```
>
> ## El motor se queda vivo, y la pantalla deja de congelarse — 2026-08-28
>
> El arranque en QEMU del bloque siguiente probó que la cadena entera existe:
> una frase en español llegó a un Qwen2.5-3B confinado y `ls` mostró la carpeta.
> Lo que ese arranque también mostró es que **el motor era por lotes**.
> `llama-completion` es de una sola respuesta por construcción, así que la
> segunda frase volvía a leer dos gigabytes de disco y a construir el contexto
> otra vez: la mayor parte de lo que cuesta un modelo local, gastada de nuevo en
> trabajo ya hecho. Y la llamada ocurría dentro de la pulsación de Enter, así
> que durante esos segundos el marco no se redibujaba — ni el reloj.
>
> Las dos cosas están cerradas. **El decreto de que el motor es un módulo no
> cambió en nada**: sigue firmado, instalado, corrido por `thalyx_core::run`
> bajo `module_standard`, con su uid, su cgroup, su seccomp y su raíz pivotada.
> Lo que cambió es la forma del programa que hay adentro.
>
> **`engine/thalyx-engine.cpp`.** El mismo `llama.cpp` en la misma etiqueta
> fijada (`b10665`), con las mismas banderas y el mismo enlace estático; se copia
> dentro de `tools/` del checkout y lo compila el mismo `cmake`, para que el
> orden de enlace de los backends de ggml no sea algo que este repositorio
> resuelva a mano. Carga el GGUF una vez, anuncia que está listo, y contesta
> peticiones enmarcadas por una tubería hasta que Thalyx cierra el otro extremo.
> No reimplementa inferencia: debajo del protocolo todo es la librería `common`
> de `llama.cpp`.
>
> **Nada de red.** Ni HTTP, ni TCP, ni el servidor que `llama.cpp` ya trae:
> conceder `net/outbound` al programa menos confiable de la máquina para que dos
> procesos del mismo anfitrión se hablen es debilitar el aislamiento por
> comodidad. El protocolo es little-endian con longitud por delante, y se mandan
> **rutas y no texto** — los archivos ya están donde al módulo se le concedió
> leerlos, así que la inferencia sigue siendo inspeccionable en disco.
>
> **Un solo lanzador.** `thalyx_core::run` se partió por dentro en `run::start`
> → `RunningModule` → `wait`/`shutdown`, y `run()` es `start` seguido de `wait`.
> El camino ordinario ejerce el mismo código que el residente mantiene abierto,
> así que no hay un segundo lugar donde acertar con el cgroup, la política, el
> filtro seccomp, la raíz y el uid.
>
> **La pantalla ya no se bloquea.** Una línea que no es un verbo vuelve como
> `Flow::Thinking` en vez de gastar segundos dentro de la pulsación; la pantalla
> pregunta en un hilo, dibuja `⠋ pensando…` con el reloj corriendo cada 120 ms, y
> cuando llega la respuesta la corre por el **mismo** dispatch, en el hilo que
> tiene el teclado. El trabajador propone y no actúa. Y al arrancar, la sesión
> gráfica lanza un hilo que precalienta los pesos, así que la primera frase
> probablemente encuentra el modelo ya adentro.
>
> **La evidencia se cuenta en procesos, no en objetos.** Bajo cada propuesta la
> sesión imprime `motor <pid> ▪ frío|tibio ▪ <s>`: el mismo pid dos veces son dos
> frases contestadas por un proceso. Y la prueba se escribió igual —
> `the_engine_stays_alive` empaqueta el binario de la propia prueba como módulo
> del motor y ese motor de mentira anota su pid al arrancar; lo que se afirma es
> cuántas líneas tiene ese archivo.
>
> **Eso agarró un defecto que nada más habría agarrado.**
> `if let (false, Some(stale)) = (usable, held.take())` evalúa las dos mitades de
> la tupla antes de comparar con el patrón, así que `take()` corría siempre y el
> residente vivo se tiraba en cada llamada. Todas las frases se contestaban bien;
> lo único que cambiaba era el costo, que es justo lo que esta fase existía para
> bajar. Y como `RunningModule` no tenía `Drop`, el proceso tirado seguía vivo:
> la máquina acumulaba un motor por frase con el modelo cargado en cada uno.
> Ahora tiene `Drop` y la regla está en [[Estrategia-de-Pruebas]].
>
> De paso, dos cosas de herramienta: **`make -C image run` abre la interfaz
> gráfica** —era `-nographic`, que fue correcto hasta el día en que la pantalla
> se volvió la cara del sistema y siguió *funcionando* después, que es por lo que
> nadie lo notó— y `run-serial` es la ruta vieja con un nombre que dice lo que
> es. `CPUS ?= 4`, porque el motor escoge sus hilos con
> `available_parallelism` y `-smp 2` era un modelo a media velocidad sin que
> nadie lo hubiera decidido.
>
> **Lo que falta y lo dice `verify.sh`:** §45 y §46 en el hierro de Cesar —
> residencia *y* confinamiento los establece la misma llamada, pero este
> contenedor no tiene BPF LSM y lo medido aquí corrió `--unconfined`. Y los dos
> números, frío contra tibio, con un Qwen2.5-3B real. Ver [[Motor-Residente]].
>
> ---
>
> ## El primer arranque real en QEMU: el motor no encendía — 2026-08-28
>
> El primer defecto real del motor no lo encontró ninguna prueba. Lo encontró
> arrancar la imagen en QEMU y hablarle:
>
> ```
> the model failed: could not start module dev.thalyx.engine:
> permission `8GiB memory` cannot be expressed as kernel policy
> ```
>
> La causa es una frontera que faltaba. `Confinement::establish` le entregaba a
> `thalyx_permd::apply` **todos** los permisos otorgados, y `thalyx-permd` sólo
> sabe expresar operaciones que el LSM revisa en un hook: `net/outbound` y
> lecturas y escrituras de rutas. Un techo de memoria no es una operación —es un
> número en `memory.max`— así que permd lo rechazaba, correctamente y de forma
> fail-closed, y el módulo con el que la máquina se deja hablar no arrancaba.
>
> **La corrección no fue ablandar a permd.** Enseñarle a `bit_for` a devolver
> `0` para `memory` habría convertido un permiso inejecutable en uno que *parece*
> ejecutado —exactamente la promesa que su error `Inexpressible` existe para
> negarse a hacer— y lo habría hecho para todo recurso no-LSM futuro al mismo
> tiempo. La frontera va donde se conoce el mecanismo: en el sandbox.
> `profile::for_kernel_policy` retira de la lista que va al kernel sólo lo que
> **este crate ya hizo cumplir por otro medio**, y todo lo demás sigue llegando a
> `bit_for` y sigue siendo rechazado si nada lo hace cumplir. La lista completa
> se conserva donde `RootFs` y `Profile::for_permissions` la necesitan.
>
> El detalle que hace que la frontera sea la correcta y no una lista de nombres:
> un permiso `memory` cuya acción no se lee como tamaño (`8Gib`) tampoco lo hace
> cumplir el cgroup, así que **no** se retira: llega a la política y se rechaza.
> Hay una prueba para cada mitad.
>
> Y `dev/stage-engine.sh` pedía 4 GiB fijos, así que `ENGINE_TIER=media` producía
> un store cuyo motor moría cargando sus propios pesos y la única salida era
> editar el archivo a mano antes de cada build. Ahora el techo se deriva de la
> gama: `ligera→4GiB`, `media→8GiB`, `alta→16GiB`, `máxima→32GiB`.
>
> Lo que enseña, y ya está en `Estrategia-de-Pruebas.md` como regla 1: **el
> defecto salió de correr el sistema.** 189 verificaciones pasaban con el motor
> incapaz de arrancar, porque ninguna preguntaba si un módulo con el permiso que
> el propio `stage-engine.sh` le escribe llega a existir.

> ## La máquina tiene agente: una frase suya llega a un modelo y algo pasa — 2026-08-28
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Lo que faltaba desde que se decretó el agente mínimo estaba en el punto 3 del
> bloque de abajo: **no hay agente adentro**. Todo lo demás existía —el prompt,
> la gramática GBNF, el parser, el contrato, el router, la atribución, las
> gamas— y ninguna de esas piezas se podía alcanzar desde la única cara que
> tiene la máquina. Escribir algo que no fuera un verbo contestaba *«no tengo
> modelo cargado»*, lo cual era cierto y era también el agente entero siendo
> inalcanzable.
>
> Ahora la cadena está cerrada de punta a punta:
>
> ```
> pantalla → session/dispatch → thalyx-agent → prompt + gramática
>          → motor llama.cpp instalado como módulo → GGUF del store
>          → inferencia real → contrato → router/validación → verbo de Thalyx
> ```
>
> ### Las cuatro decisiones que la hacen así
>
> 1. **El motor es un módulo, no parte de Thalyx.** `llama-completion` de
>    llama.cpp, empaquetado en un `.thmod` firmado, instalado en el store,
>    confinado bajo `module_standard` con el uid, el cgroup, el seccomp y la
>    raíz pivotada que recibe cualquier otro módulo. La imagen sigue siendo el
>    kernel y **un** programa: `make -C image count` lo dice.
> 2. **Un proceso por respuesta.** No hay demonio, no hay servidor, no hay API
>    HTTP. `thalyx_core::run` —el mismo que ejecuta `correr`— lanza el motor,
>    espera, y lo que el módulo escribió en su `stdout` es la respuesta. Una
>    inferencia es un módulo corriendo, con su entrada en el diario.
> 3. **La costura es angosta a propósito.** `thalyx_agent::llama::Engine`:
>    entra un vector de argumentos, salen bytes. Arriba de esa línea no cambió
>    nada —el prompt, el marcador, la gramática, dónde termina una respuesta,
>    qué es una respuesta rota— y abajo hay dos implementaciones:
>    `ProcessEngine` (un programa en el `PATH`, que es lo que tiene una máquina
>    de desarrollo) y `ModuleEngine` (el módulo instalado, que es lo que tiene
>    la máquina). El crate del agente no puede lanzar procesos confinados y no
>    debe poder: todo lo que sabe llegó de un modelo que no es de confiar.
> 4. **El disco arranca listo.** El motor se instala en el stage y la elección
>    de gama queda escrita, así que la máquina arranca **pudiendo ser hablada**.
>    El greeter sigue sin instalar a propósito —el paso 2 del criterio de salida
>    es una persona instalándolo— y el motor es el requisito contrario.
>
> ### Lo que se arregló en el camino
>
> - **El prompt se escribía en `/tmp`.** Un módulo sólo ve lo que su manifiesto
>   le concedió, así que un prompt fuera de esos directorios es un archivo que
>   el motor tiene orden de leer y no puede ver — y vuelve como «llama.cpp no
>   completó el prompt», culpando a llama.cpp de un error de Thalyx. El motor
>   ahora dice **dónde** se le puede escribir (`Engine::scratch_root`) y hay una
>   prueba que sólo falla si eso se rompe.
> - **`llama.cpp` no enlazaba estático.** Tres banderas, encontradas por tres
>   enlaces fallidos, en este orden: `LLAMA_OPENSSL=OFF` (cpp-httplib encuentra
>   el OpenSSL del sistema, que sólo existe como `.so`), `GGML_OPENMP=OFF`
>   (`libgomp` igual) y `GGML_NATIVE=OFF` (la máquina que construye el store no
>   es necesariamente la que lo arranca, y `-march=native` en un módulo es una
>   instrucción ilegal en el CPU de otro). Están escritas en
>   `dev/build-engine.sh` para que nadie las vuelva a encontrar.
> - **`agent model use` fallaba al grabar una ruta que todavía no existe.** El
>   store se construye en una máquina y se arranca en otra: la ruta que se graba
>   es la de adentro y los bytes que se miden son los del stage. `--reading`.
> - **El modelo diminuto declaraba 128 tokens de contexto** y el prompt real
>   mide ~1800, así que rechazaba la única cosa para la que servía además de
>   `engine-needs.sh`. Ahora declara 4096, que en un modelo de dos capas no
>   cuesta nada.
>
> ### Qué está probado y qué no
>
> **Probado en el contenedor, con un llama.cpp real y un GGUF real** (dos capas,
> hecho por `gguf-py` de llama.cpp):
>
> - el binario es estático, sin intérprete y sin bibliotecas compartidas;
> - se empaqueta, se firma y se instala como módulo, con los 4 GiB que pide;
> - `thalyx agent model check` lo corre **a través del sistema de módulos**, el
>   motor lee el prompt del directorio concedido, carga los pesos, obedece la
>   gramática y lo que imprime vuelve a Thalyx;
> - una frase que no es un verbo, escrita en la sesión, se convierte en un verbo
>   y **se ejecuta**: `crea una carpeta llamada pruebas` → `mkdir pruebas` → la
>   carpeta existe. (Con un motor de reemplazo: es una afirmación sobre el
>   cableado, no sobre el juicio de un modelo.)
>
> **No probado aquí, y nombrado en vez de supuesto:**
>
> - **confinado.** Este contenedor no tiene BPF LSM, así que la corrida fue
>   `--unconfined` y el diario la anotó como degradada. La §45 de
>   `dev/verify.sh` lo corre confinado en la máquina de Cesar, y dice NOT PROVEN
>   si no puede;
> - **que un Qwen2.5 real acierte la intención.** El modelo de dos capas produce
>   un objeto gramatical y vacío. Eso es una medición del modelo, no de la
>   máquina, y `thalyx agent bench` es lo que la hace.

> ## La máquina se puede usar: se puede escribir en ella, y se puede terminar lo que se empieza — 2026-08-28
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Cesar arrancó la imagen y preguntó qué le falta al sistema para que él, por
> voluntad propia, pueda pasar **un día completo adentro**. Medido contra el
> código, el hueco más grande no estaba escrito en ningún lado.
>
> ### Lo que se midió, y no es lo que la bóveda decía
>
> Seis cosas separan el arranque de la foto de un día de uso. Dos de ellas nadie
> las había notado:
>
> 1. **La pantalla que arranca es una ventana, no un taller.** Bajo
>    `thalyx-capture` el descriptor 0 es `/dev/null` —a propósito, si no la
>    máquina se cuelga con una pregunta que nadie ve— así que los ocho lugares
>    que se detienen a preguntar encuentran que no hay terminal y se niegan.
>    `instalar`, `ejecutar`, `observar`, `instalar-en` **y `editar`** se pueden
>    leer ahí y no se pueden acabar. La bóveda tenía escrita sólo «la
>    confirmación dibujada»; `editar` no lo nombraba nadie.
> 2. **No se podía teclear español.** Un `grep` de `keymap` en todo el repo
>    salía vacío. El kernel lleva un mapa compilado adentro y es US QWERTY, y
>    `loadkeys` no cabe en la imagen. La tecla que en un teclado latinoamericano
>    dice `ñ` mandaba `;`. Un SO cuya bóveda entera está en español, en el que
>    no se podía escribir en español.
> 3. **No hay agente adentro** — decretado el 2026-08-28. **Cerrado** el
>    mismo día: ver el bloque de arriba.
> 4. **Una cosa a la vez** bajo la pantalla.
> 5. **Nada entra ni sale** (`red` lee y no manda), y un store recién instalado
>    está vacío.
> 6. **El techo de 1 GiB** por módulo, que el motor no cabe.
>
> ### Sus dos decisiones
>
> Preguntadas con opciones, contestadas el mismo día:
>
> - **El día es escribir y pensar con el agente, y además operar la máquina.**
>   No desarrollar Thalyx desde adentro, que abriría la Fase 2.
> - **El techo de memoria: lo que pida el manifiesto, aprobado por él al
>   instalar.** No un número fijo más grande.
>
> ### Lo entregado
>
> **A — una pregunta, dos caras** (`crates/thalyx-cli/src/ask.rs`). Las ocho
> confirmaciones tenían los mismos cinco renglones escritos a mano, y habían
> derivado sobre qué es un sí: `intento abandonar` tomaba `si` y `sí`, y el
> verbo que le quita el guardián al kernel no tomaba ninguno. Ahora hay una sola
> comparación y las dos caras la llaman; lo que no se comparte es la negativa,
> que es del verbo. Y el orden cambió: **decir de qué se trata va antes de
> revisar si hay terminal**, porque el contexto *es* la confirmación y una
> negativa emitida antes de que exista no deja nada que dibujar.
>
> **B — el teclado** (`crates/thalyx-term/src/keymap.rs`). Las tablas se
> generan de `kbd` con `dev/keymap-table.py`, nunca se escriben: una
> distribución es un dato sobre el mundo y la regla 6 aplica. `teclado` dice qué
> hay —preguntándole al kernel, no a Thalyx— y `teclado latino|ingles` lo
> cambia. Se carga en el arranque, con `thalyx.teclado=no` de salida de
> emergencia, y hay prueba de que las letras de `teclado ingles` están en la
> misma tecla en las dos distribuciones, o sea que el camino de regreso se
> teclea desde donde haría falta.
>
> **C — el techo por manifiesto.** Una petición de memoria es un permiso
> `persistent`: sale por el camino confiable que ya existe, se guarda donde ya
> se guarda, y `for_permissions` sube el techo. El gigabyte pasa de techo a
> piso. Con unidad siempre (`4GiB`, nunca `4294967296`), nunca `jit`, negado si
> no cabe en la máquina, y dos concesiones dan la mayor y no su suma. Entero en
> [[Motor-de-Inferencia-como-Modulo]].
>
> **1506 pruebas verdes**, `clippy` limpio, y la compilación de musl —la regla
> 12— corrida de verdad: este contenedor no la podía hacer y ahora sí, con
> `apt-get install musl-tools`.
>
> ### Lo que le toca correr a Cesar
>
> ```
> git pull && cargo install --path crates/thalyx-cli && make -C image image
> ```
>
> Y **arrancar la imagen**, que es lo único que contesta las dos mitades que
> este contenedor no tiene:
>
> - **Teclear `ñ`.** Si sale `ñ`, la pieza B funciona en hierro. Si sale otra
>   cosa, `teclado ingles` regresa el mapa del kernel; si el teclado quedó
>   inservible, `thalyx.teclado=no` en la entrada de arranque.
> - **Teclear `instalar <algo>` o `ejecutar <programa>` en la pantalla.** Antes
>   rechazaba; ahora debe dibujar la confirmación con el contexto arriba y
>   tomar la respuesta. **Ctrl-C cancela** — el aviso decía «Escape» y era
>   imposible: un Escape solo es el prefijo de toda flecha.
>
> `sudo ./dev/verify.sh` trae **tres etapas nuevas** (42, 43, 44) sobre las 189
> de la corrida anterior.
>
> ### Lo que sigue, y por qué está en ese orden
>
> **D — el motor adentro de la máquina.** Es lo único que queda de lo que él
> decidió, y es lo más grande. Ahora no lo bloquea nada: el confinamiento le
> alcanza (31 de 31 llamadas), el techo ya se puede pedir, y `ejecutar` ya se
> puede confirmar desde la pantalla — que era el hueco por el que el motor no
> se podía ni arrancar desde la cara con la que la máquina viene.
>
> Los dos huecos que quedan del día y **no** están decididos: que se pueda hacer
> más de una cosa a la vez, y por dónde entra y sale algo de la máquina.

> ## El agente no está en la máquina, y por dónde entra — 2026-08-28
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Cesar arrancó la imagen, vio la pantalla, y preguntó qué le falta al sistema
> para ser *«algo real que pueda usar durante un buen tiempo de verdad»*. La
> respuesta, medida contra el código, es más grande de lo que esta bóveda decía.
>
> ### Una máquina Thalyx arrancada no tiene agente
>
> Y no por un pendiente: por cómo está construido. `llama.rs` arranca
> `llama.cpp` con `Command::new`, un binario aparte buscado en el `PATH`, y la
> imagen lleva `/init` y `/dev/console` y nada más. Así que el motor **no existe
> en la máquina que arranca y no puede existir ahí** mientras la imagen sea el
> kernel y un programa. El router, la gramática y las tres gamas medidas han
> corrido siempre en el Fedora de Cesar, nunca sobre Thalyx.
>
> Un sistema operativo donde la IA es ciudadana de primera arrancó sin ella, y
> eso no estaba escrito en ningún lado como hueco.
>
> ### El decreto: el motor es el primer módulo real
>
> Decidido por Cesar el 2026-08-28. Un binario estático, firmado, en el store,
> confinado bajo `module_standard` como cualquier módulo; el `.gguf` llega al
> disco de store por donde `greeter` ya llega. **No contradice el decreto de la
> imagen**: un módulo vive en el store, no en la imagen, y `make -C image count`
> sigue diciendo uno. Lo que sí lo contradice es lo de hoy, un `PATH` del que se
> saca un programa sin manifiesto, sin firma, sin permisos y sin confinamiento.
> Entero en [[Motor-de-Inferencia-como-Modulo]].
>
> ### Y se midió antes de construir nada, porque una pregunta podía matarlo
>
> Si un motor real necesitara algo que `module_standard` niega, el decreto sería
> inconstruible. `dev/engine-needs.sh` lo preguntó —es
> `dev/foreign-agent-needs.sh` apuntado a un motor, la misma comparación contra
> la misma lista, una sola y no dos— con llama.cpp de verdad y un modelo escrito
> por el propio `gguf-py` de llama.cpp, no por nosotros (regla 6).
>
> **31 llamadas al sistema distintas para cargar, tokenizar, correr el grafo y
> generar. Las 31 ya están permitidas.** El confinamiento que ya existe alcanza
> para un motor de inferencia, y eso no se sabía. De 13 rutas abiertas, 9 caen
> dentro de lo que un módulo ve; de las otras cuatro, una es el `.gguf` —los
> datos del módulo, como el `notes.txt` de `greeter`— y las tres restantes son
> `/dev/tty` y dos de conteo de núcleos bajo `/sys`.
>
> **Lo que no contesta es el tamaño, y ahí está el hueco.** El modelo medido
> pesa menos de un megabyte. `module_standard` topa un módulo en **1 GiB**
> (`profile.rs`), ningún manifiesto puede pedir más, y aunque los pesos por
> `mmap` sean caché reclamable, el KV cache y los búferes de cómputo no lo son.
> Ese número es de Cesar: es política y cuesta su hierro. Tampoco contesta el
> libc — se midió glibc y embarcaría musl estático, que es la regla 12.
>
> ### Tres defectos que su foto agarró, arreglados
>
> Los tres salieron de mirar la pantalla arrancada, no el código, y ninguno era
> un error de lo que la pantalla *dice*.
>
> - **Un renglón se salía de su panel.** `Row::Pair` medía el valor entero y
>   luego lo alineaba a la derecha; uno más ancho que la columna quedaba con el
>   lápiz a la izquierda del panel, y `draw` no recorta. El recorte salió a
>   `Typography::fit`, porque un texto alineado a la derecha hay que medirlo **ya
>   recortado**.
> - **`clear` mandaba un escape a una tubería.** Bajo la pantalla la salida se
>   atrapa en el descriptor, así que la pantalla dibujó `[2J[H` como texto. Ahora
>   el escape sólo sale a una terminal de verdad y hay `Flow::Emptied`. Probado
>   por tubería **y con control en una terminal hecha con `thalyx dev pty`**.
> - **La barra decía las opciones de un tmpfs donde va el store.** Dos errores
>   independientes: el respaldo le ganaba a lo que respaldaba —un `find` pedía
>   `/var/thalyx` **o** `/`, y `/` se monta primero, así que en una máquina con
>   store el store no se consultaba nunca— y el último campo de un renglón de
>   `mountinfo` no es una etiqueta. Por eso la barra decía tmpfs mientras el panel
>   dos pulgadas a la derecha decía `btrfs`: **la pantalla se contradecía a sí
>   misma**, y ésa fue la pista. Ahora dice el disco, o «sin disco — no
>   recuerda», o `store ?` cuando no se pudo leer (regla 10).
>
> ### Lo que le toca correr a Cesar
>
> ```
> git pull && cargo install --path crates/thalyx-cli && make -C image image
> ```
>
> Y arrancar la imagen para ver la pantalla otra vez: los tres defectos son de
> vidrio y su Fedora no tiene framebuffer que los enseñe. `sudo ./dev/verify.sh`
> no trae etapas nuevas todavía — la del motor llega cuando el motor exista.

> ## La corrida en hierro salió limpia, y la imagen no compilaba — 2026-08-28
>
> Los bloques de abajo son cómo se llegó.
>
> **La corrida de Cesar: `proven 185 · not proven 4 · failed 0`.** Cero fallas, y
> el conteo cuadra: 189 comprobaciones contra las 184 de la corrida anterior
> (`181 · 2 · 1`), y las cinco de diferencia son exactamente las cinco que agregó
> la entrega de la pantalla. Nada dejó de correr en silencio, que es la única
> forma de leer un marcador.
>
> Lo que eso deja probado en hierro:
>
> - **la suite ya no arma el kernel de la máquina que está midiendo** — la falla
>   de la §5 desapareció, y con ella queda comprobada en fierro la regla 11, que
>   aquí no se puede comprobar porque este contenedor no tiene guardián;
> - **el color del camino confiable está en una confirmación y en nada más**,
>   leído por un decodificador de PNG que no es Thalyx, con su control y el
>   control del control;
> - **`pantalla` por tubería rechaza con `not_a_terminal` y la sesión sigue
>   contestando** — una máquina sin monitor no se detiene;
> - **la línea de comandos de la imagen deja `thalyx.pantalla` sin contestar**,
>   así que `thalyx.pantalla=no` sigue siendo la salida de una máquina negra.
>
> **Los cuatro `NOT PROVEN`: dos son nuevos y son el mismo hueco.** De las cinco
> comprobaciones nuevas sólo dos tienen rama de `NOT PROVEN`, y las dos son la
> mitad de la etapa 40 que necesita `/dev/fb0`: la comparación del `ioctl` contra
> sysfs, y el tracer que vigila que `pantalla` por tubería no llegue a tocar el
> framebuffer. Cesar lo confirmó a mano: **su Fedora no tiene `/dev/fb0`** ni
> `strace` instalado. Los otros dos son los de siempre, los que ya venían del 27.
>
> Eso **no es un hueco de la pantalla de Thalyx**, es de la máquina que verifica:
> la imagen lleva `CONFIG_FB`, `CONFIG_FB_EFI` y `CONFIG_FRAMEBUFFER_CONSOLE`, y
> la consola de texto con la que arrancó en agosto ya era prueba de que ahí sí
> hay framebuffer. La pantalla sólo se puede *ver* arrancando la imagen, no desde
> Fedora.
>
> ### Y lo siguiente que tecleó no compiló
>
> `make -C image image` murió con cinco errores de tipo, todos de la pantalla:
> las peticiones de `ioctl` escritas `as libc::c_ulong`. Ése es el tipo que toma
> `libc::ioctl` **contra glibc**; contra musl toma `c_int`, y la imagen es un
> binario estático de musl.
>
> **Por qué 189 comprobaciones no vieron nada.** `verify.sh` compila contra glibc
> de principio a fin, y la etapa 11 —la que se llama «la imagen»— arma el
> initramfs **con ese binario de glibc** para contar cuántos programas lleva
> adentro. El único lugar del proyecto donde se compilaba lo que de verdad
> arranca era un comando que corre Cesar, a mano, después de que todo dijo que
> estaba bien.
>
> **El arreglo son cinco palabras** —`as libc::Ioctl`, el alias que ya vale en
> los dos objetivos— y la regla ya estaba escrita en el propio crate, en el
> comentario de `BTRFS_IOC_SUBVOL_CREATE`, desde que se construyó Btrfs. Una
> convención que vive sólo en un comentario la obedece quien lo lee.
>
> **Así que lo que se entrega no es el arreglo, es la comprobación.** La etapa 2
> corre ahora la línea exacta del `Makefile` de la imagen:
>
> ```
> cargo build --release --target x86_64-unknown-linux-musl -p thalyx-cli
> ```
>
> Sus cuatro brazos se ejercieron uno por uno antes de entregarla, incluido el
> que importa: con el defecto puesto de vuelta, la etapa dice `FAILED` y imprime
> el error del compilador junto al veredicto. Si a la máquina le falta el
> objetivo de rustup o un compilador de C para musl, dice `NOT PROVEN` nombrando
> el remedio —un límite de la máquina no es una falla de Thalyx— y
> `THALYX_REQUIRE_IMAGE_BUILD=1` vuelve falla esos saltos.
>
> La regla nueva es la 12 de `CLAUDE.md` y está entera en
> [[Estrategia-de-Pruebas]]: **lo que se compila para verificar tiene que ser lo
> que arranca.** Es la regla 8 apuntada al compilador: una compilación con otra
> configuración es otro sistema.
>
> ### Lo que le toca correr a Cesar
>
> ```
> git pull && make -C image image
> ```
>
> Y arrancar la imagen, que es lo único que puede contestar cómo se ve la
> pantalla: su Fedora no tiene framebuffer que enseñarla. Adentro no hay que
> teclear nada — la pantalla es lo que sale. Si sale en negro: Ctrl-C a ciegas,
> o `thalyx.pantalla=no` en la entrada de arranque.
>
> `sudo ./dev/verify.sh` completo no hace falta para esto; cuando se corra, va a
> traer una comprobación más que las 189 de hoy.

> ## La pantalla es la máquina — 2026-08-28
>
> Los bloques de abajo son cómo se llegó.
>
> **El decreto, en sus palabras:** *«te dije que ya deberíamos tener ui, porque
> no lo hiciste? o sea no quiero un comando para activar ui, quiero ya la ui, la
> que se ve al iniciar, es una estupidez tener que poner un comando para ver la
> ui definitiva»*. Y tenía razón sobre el diagnóstico también: él mismo encontró
> que `thalyx screen` tecleado adentro de la sesión caía en el caso `_` del
> despacho y contestaba *«I have no model loaded»*, porque `session.rs` no
> exponía el verbo.
>
> **Lo que quedó.** `session::run` **entra a la pantalla antes de imprimir un
> solo prompt**. La sesión de texto es lo que hay debajo, no la puerta. Hay un
> verbo `pantalla`, y sirve para volver después de Ctrl-C — no para entrar.
>
> **Y los verbos corren ahí.** Era la entrega que estaba pendiente y la razón
> escrita para no haberla hecho antes era buena: no apilar un segundo cambio sin
> verificar encima del primero. Lo que la volvió barata fue notar que los brazos
> de ese ciclo de seiscientas líneas tocan **exactamente cuatro cosas** —la
> tienda, dónde está parada la persona, qué cara contesta, y cómo llegó a existir
> este proceso— y nada más: ni la terminal, ni el vigilante del kernel. Salieron
> enteros a `session::dispatch`, y las dos caras lo llaman.
>
> **Lo que imprimen se atrapa en el descriptor**, no pasándoles un `Write` hacia
> abajo. `correr` y `ejecutar` arrancan **otros programas**, y la salida de un
> módulo está en el descriptor 1 de un proceso que Thalyx no controla; cualquier
> cosa más estrecha dibujaría una respuesta vacía justo para los dos verbos cuyo
> sentido entero es correr algo. Vive en `crates/thalyx-capture`.
>
> **Y la mitad que no es sobre salida.** La entrada se manda a `/dev/null`
> mientras corre un verbo. Varios se detienen y preguntan —`instalar`,
> `observar`, `instalar-en`, `ejecutar`— y todos preguntan después de comprobar
> `is_terminal`. Bajo la pantalla eso diría que sí, la pregunta se imprimiría
> donde nadie la ve, y la máquina se quedaría ahí sin teclado con qué contestar:
> **un cuelgue con una foto encima**. Con `/dev/null` cada uno toma el camino de
> rechazo que ya tenía escrito y probado. Es lo que queda pendiente de la
> pantalla, y está en [[Tareas-Pendientes]]: dibujar la confirmación.
>
> ### Las dos salidas, porque de esto depende que no se pierda una máquina
>
> ```
> thalyx.pantalla=no      en la línea de comandos del kernel: arranca en texto
> Ctrl-C con la línea vacía   baja a la sesión de texto, y funciona a ciegas
> ```
>
> La segunda es la que importa si la pantalla sale mal: el modo gráfico y el modo
> crudo se deshacen en `Drop`, así que devolver la consola no depende de que
> alguien haya podido **leer** la pantalla para pedirlo.
>
> ### Un defecto que sólo se veía corriéndolo
>
> El acomodo de la conversación colocaba **un turno completo a la vez** y saltaba
> el que no cabía — así que la respuesta de `describe`, que es todos los verbos de
> la máquina, dibujaba **nada**. Cuarenta y tres pruebas de la pantalla en verde y
> ninguna lo veía, porque todas usaban conversaciones que caben. Ahora se aplana a
> renglones, se ancla abajo como una terminal, y AvPág/RePág recorren lo anterior.
>
> ### Y la regla 11 en un sitio nuevo
>
> **Los descriptores 0, 1 y 2 son del proceso.** `cargo test` corre las pruebas de
> un binario como hilos de un mismo proceso, así que la prueba del atrapador
> —viviendo como módulo adentro de `thalyx-cli`— atrapaba los renglones de
> progreso de `libtest` en vez de lo suyo: sola pasaba, con `--test-threads=1`
> pasaba, junto a las otras ciento treinta y cuatro no. Lo que no tiene dueño no
> se aísla con una variable de entorno; se aísla con **otro proceso**, que en Rust
> es otro crate. Está escrito en [[Estrategia-de-Pruebas]] y en `CLAUDE.md`.
>
> ### Lo que le toca correr a Cesar
>
> ```
> git pull && cargo install --path crates/thalyx-cli && sudo ./dev/verify.sh
> ```
>
> Y después, lo que ninguna prueba puede contestar — **`--describe` primero**,
> porque recorre todo el camino salvo escribir en el dispositivo y tomar la
> consola:
>
> ```
> thalyx screen --describe     dice qué es este display, SIN tocar la consola
> ```
>
> Y la de verdad, que es arrancar la imagen: **no hay que teclear nada.** La
> pantalla es lo que sale. Adentro se teclea `ls`, `cat`, `estado`, `describe` —
> los mismos verbos, Tab completa, las flechas repiten, AvPág recorre. Si sale en
> negro: Ctrl-C a ciegas, o `thalyx.pantalla=no` en la entrada de arranque.
>
> ### Lo que falta para poder vivir adentro, medido contra el código
>
> | Hueco | Estado |
> |---|---|
> | El store de una máquina recién instalada queda vacío | Escrito en [[Tareas-Pendientes]]. Es la pregunta de Fase 2: **desde dónde** llega el software |
> | La red ve y no usa | Decreto de Cesar del 2026-08-23, [[Red]]. Sin DHCP, sin resolutor, sin TLS |
> | El agente no vive adentro de la máquina | Lo siguiente que eligió Cesar. Adentro de la imagen no hay llama.cpp; tendría que ser un programa ajeno en el store corrido con `ejecutar`, y **nunca se ha intentado** |
> | La confirmación, dibujada | Los verbos que preguntan rechazan en la pantalla en vez de preguntar en ella. `Confirmation` ya existe y ya se prueba; falta cablearlo |

> ## La pantalla existe — 2026-08-27
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> **Lo primero, porque cambia el encuadre.** Cesar abrió pidiendo *«empezar a
> hacerlo realmente un SO»*, y medido contra el código **eso ya había pasado**:
> el 2026-08-07 una PC física arrancó Thalyx de una USB, se instaló sola en otro
> disco con `instalar-en`, y con el medio quitado arrancó de ese disco. PID 1 es
> `thalyx`, no hay distribución debajo, y `make -C image count` dice `1`. Contra
> el criterio que él mismo decretó, Thalyx **es** un sistema operativo desde
> entonces. Lo que falta no es el título: es que se pueda **vivir** adentro, y
> esa lista es corta y está abajo.
>
> **El decreto.** [[La-Pantalla]], tomado por él el 2026-08-27: *una sola
> pantalla que es Thalyx*. Sin ventanas, sin escritorio, sin lanzador — no hay
> dónde *abrir* el agente porque el agente es la pantalla. Corrige el
> aplazamiento de la GUI del 2026-08-01, cuya razón escrita era cierta y estaba
> **condicionada a que la Fase 1 no estuviera terminada**; cerró el 2026-08-07 y
> el aplazamiento siguió vivo veinte días por inercia.
>
> **Lo construido.** `crates/thalyx-screen`, puro: estado adentro, pixeles
> afuera. No abre un dispositivo, no hace un `ioctl`, no muestra nada — el mismo
> patrón de `thalyx-term` y `thalyx-edit`, y por la misma razón: el contenedor
> que construye Thalyx no tiene pantalla. **43 pruebas**, todas corriendo aquí.
> Lo que necesita hierro vive en `thalyx-syscall`: el `ioctl` que pregunta cómo
> empaqueta un pixel este framebuffer, el `mmap`, y quitar la consola de texto
> de en medio.
>
> **Y se puede *ver* sin tener pantalla.** `thalyx dev screen <archivo.png>`
> escribe un cuadro a una imagen por el mismo camino de composición que usa el
> display, así que lo que sale es lo que se dibuja.
>
> **Dos cosas que construirlo enseñó**, las dos escritas como revisión en
> [[La-Pantalla]]:
>
> 1. **Los pixeles no piden nada del kernel.** `FB`, `FB_EFI` y `VT` ya estaban
>    desde el 2026-08-07, y `KD_GRAPHICS` sólo impide que el kernel dibuje la
>    consola —no que la tty entregue las teclas—, así que el modo crudo que ya
>    existe sigue sirviendo. **Esta entrega no toca `thalyx.config`**, o sea que
>    no arriesga el arranque de la única máquina que verifica el proyecto. El
>    ratón sí lo pediría, y por eso queda fuera: una pantalla sin ventanas no
>    tiene qué apretar.
> 2. **Ctrl-C, que en la sesión es la salida, aquí es la trampa.** `RawMode`
>    deja `ISIG` prendido a propósito. Con la consola en modo gráfico, ese mismo
>    `SIGINT` mata el proceso **antes** de que `Drop` devuelva la consola, y lo
>    que queda es una pantalla en negro sobre una máquina que está corriendo
>    bien. La pantalla usa `RawMode::enter_without_signals` y sale por su propio
>    pie. Una tecla que en un modo es el escape, en otro es lo que cierra la
>    puerta.
>
> **La etapa 40 de `verify.sh`**, en dos mitades con costos distintos. La
> composición corre en cualquier lado y comprueba la única propiedad de la
> pantalla que es de seguridad: que **el color del camino confiable esté en una
> confirmación y en nada más**, con su control (la pantalla ordinaria lo usa
> cero veces) y el control del control (el color del agente sí aparece, así que
> el lector no está encontrando nada). Los pixeles los lee un decodificador de
> PNG escrito en el propio guion sobre `zlib` — regla 5: un cuadro comprobado
> por el código que lo dibujó sólo prueba que es consistente consigo mismo. La
> otra mitad necesita `/dev/fb0` y compara lo que el `ioctl` contesta contra
> **sysfs**, que es otro camino al mismo kernel; sin framebuffer dice
> `NOT PROVEN` y `THALYX_REQUIRE_DISPLAY=1` lo vuelve falla.
>
> ### Lo que le toca correr a Cesar
>
> ```
> git pull && cargo install --path crates/thalyx-cli && sudo ./dev/verify.sh
> ```
>
> Y después, lo que ninguna prueba puede contestar:
>
> ```
> thalyx screen --describe     dice qué es este display, SIN tocar la consola
> thalyx screen                toma la pantalla; Ctrl-C la devuelve
> ```
>
> **`--describe` primero.** Recorre todo el camino salvo escribir en el
> dispositivo y tomar la consola, así que si la respuesta es que no se puede
> dibujar aquí, lo dice con la consola intacta.
>
> ### Lo que esta entrega NO hace, y por qué
>
> **Los verbos todavía no pasan por la pantalla.** `session::run` es un solo
> ciclo de seiscientas líneas que imprime conforme avanza; volverlo algo que
> devuelve una respuesta es una edición grande al código más ejercido del
> proyecto. Hacerlo en la misma entrega que los primeros pixeles significaría
> que, si su máquina arranca en negro, **no hay manera de saber cuál de los dos
> cambios fue**. Es la regla de `CLAUDE.md` sobre no apilar un segundo cambio
> sin verificar encima del primero.
>
> ### Lo que falta para poder vivir adentro, medido contra el código
>
> | Hueco | Estado |
> |---|---|
> | El store de una máquina recién instalada queda vacío | Escrito en [[Tareas-Pendientes]]. La imagen lleva el kernel y un programa, así que una PC recién instalada arranca sana y sin nada que instalar. Es la pregunta de Fase 2: **desde dónde** llega el software |
> | La red ve y no usa | Decreto de Cesar del 2026-08-23, [[Red]]. Sin DHCP, sin resolutor, sin TLS |
> | El agente no vive adentro de la máquina | Las cuatro gamas se midieron en Fedora con `llama-completion` en el `PATH`. Adentro de la imagen no hay llama.cpp; tendría que ser un programa ajeno en el store corrido con `ejecutar`, y **nunca se ha intentado** |
> | Los verbos por la pantalla | La entrega siguiente. Ver arriba |
>
> **Lo siguiente que eligió Cesar** el 2026-08-27, junto con la pantalla: **el
> agente adentro de la máquina**. Es la razón por la que este SO existe, y
> `ejecutar` ya se construyó exactamente para correr un binario ajeno confinado.

> ## La suite armaba su kernel, dos veces — 2026-08-27
>
> Los bloques de abajo son cómo se llegó.
>
> **La segunda vuelta.** Con el arreglo puesto, la corrida trajo **181 `PROVEN`,
> 2 `NOT PROVEN`, 1 `FAILED`**, y la falla era la medición nueva de la §5
> haciendo su trabajo: *«the suite moved the kernel guard from [0] to [1]»*.
> Quedaba otro, y es el que enseña algo — `catalogue_is_true.rs`, que **no es
> una prueba sobre el guardián**: le pregunta al binario qué verbos tiene y
> teclea cada nombre que le contesta. En esa lista viene `negar`. Su lista de
> exclusiones tenía cinco nombres y una sola razón detrás —*terminan la
> corrida*—; la otra razón para no teclear algo no estaba escrita en ninguna
> parte.
>
> O sea que el arreglo de la primera vuelta era **la mitad**: el peligro no es
> una prueba que trata del interruptor, es cualquier cosa que llegue al prompt,
> porque el prompt tiene el interruptor. La precondición se mudó a
> `tests/machine_guard/mod.rs`, compartida, y ahora la usa también el archivo
> que no sabía. Donde el guardián es real, los cuatro nombres se dejan fuera del
> tecleo y se dice cuáles; donde no, se teclean como todos.
>
> **Y un disparador para el quinto.** Excluir por una lista de palabras es un
> conjunto leído del lugar equivocado. Lo peligroso es que un verbo **actúe en
> cuanto se teclea** —sin argumento no hay «cuál» que lo detenga— y eso sí se
> lee del catálogo: `changes` verdadero y `takes` vacío. Hoy son cuatro:
> `revertir` y `apagar`, contenidos, y los dos del guardián. Cuál de las dos
> clases es cada uno no se puede leer de ahí, así que el conjunto quedó clavado
> en una prueba que lo lee del binario en vivo: un quinto verbo que actúe desnudo
> la pone en rojo y obliga a decidir.
>
> Cesar corrió `verify.sh` en su máquina y trajo **180 `PROVEN`, 2 `NOT PROVEN`,
> 2 `FAILED`**. Las dos fallas eran una: la suite de la §5 **armó su kernel**.
>
> `the_guard_can_be_switched.rs` está escrito contra una máquina sin nada
> cargado —sin BPF, `negar` no puede cambiar nada, y lo que se comprueba es el
> cableado— y daba por hecho que la máquina era ésa. Cada prueba abre su
> `THALYX_ROOT` temporal, y eso aísla **la tienda y nada más**: el guardián son
> cuatro bytes en bpffs, de la máquina, y ninguna variable de entorno los mueve.
> Como root y con el LSM enganchado, tres de esas pruebas hicieron lo que `negar`
> hace. La siguiente leyó «already enforcing» y falló, y la §6 reportó que la
> máquina llegaba armada.
>
> **Lo que quedó:** esas tres preguntan primero —al kernel, como lo pregunta
> `guard::set`— y se saltan con `NOT PROVEN` si el guardián de esta máquina es
> real; la línea base se salta con ellas, porque una línea base que sobrevive a
> lo que sostiene dejó de serlo. Dos de las seis siguen corriendo en todas
> partes y son las que hacen que el archivo pruebe algo en su máquina: el rechazo
> de `observar` en cara estructurada ocurre **antes** de leer el kernel, así que
> ahí se teclea el verbo que desarma, en una máquina que sí se puede desarmar, y
> no se mueve nada.
>
> **Y dos arreglos del arnés,** porque el veredicto apuntaba a la etapa
> equivocada: `guard_check` ahora nombra el intervalo —*«between [5. the test
> suite…] and [6. a real module…]»*— en vez de culpar sólo a la que se dio
> cuenta; y la §5 mide con `bpftool`, contra una línea base tomada antes, que la
> suite dejó el guardián donde lo encontró. Era otra precondición que el guion
> daba por hecha.
>
> La regla nueva es la 11 de `CLAUDE.md`, y está entera en
> [[Estrategia-de-Pruebas]]: **una prueba que escribe algo global de la máquina
> ya cambió la máquina que estaba midiendo.** No es «toca la máquina» —un cgroup
> se crea, se borra y tiene dueño— sino **un interruptor global sin dueño**,
> cuyo valor es la precondición de otra cosa.
>
> **Lo que sigue esperando fierro:** volver a correr `verify.sh` entero. Nada de
> esto se puede comprobar aquí, porque el contenedor no tiene el guardián que
> hace que el salto ocurra; lo que sí se comprueba aquí es la decisión del salto,
> con `would_switch_this_machine` sobre las tres respuestas que puede dar el
> kernel.

> ## El sprint de lo que no necesita el fierro — 2026-08-26
>
> Los bloques de abajo son cómo se llegó.
>
> Cesar estaba fuera de casa y pidió acumular corridas: todo lo que se pueda sin
> comprobar en hierro. Esto es lo que salió, y casi todo son instrumentos —
> porque el defecto de abajo llegó al fierro por huecos del arnés, no por falta
> de código.
>
> **Una prueba que mide el orden en vez del efecto.** El `-EPERM` del LSM no se
> reproduce sin LSM, pero la propiedad de la que se sigue —toda escritura de
> Thalyx antes de entrar al cgroup— sí se mide aquí, con `strace -f -y`. La
> ventana va del `write` a `cgroup.procs` hasta el `execve`, y adentro no hay
> una sola apertura con `O_WRONLY` ni `O_RDWR`. Comprobada revirtiendo el
> arreglo.
>
> **La columna de afuera para el pid.** El arreglo cambió *por qué* funciona el
> ingreso al cgroup: ahora se escribe «1», y sólo sirve porque el kernel traduce
> en el espacio de nombres de quien escribe. Si no lo hiciera, metería al init de
> la máquina bajo la política de un módulo. Se lee `cgroup.procs` desde el
> anfitrión, con `std::fs` y no a través de Thalyx. Comprobada con una mutación.
>
> **La etapa 39.** §36 era la única que armaba la máquina y sólo corre invitados;
> `correr` bajo un kernel que niega no lo había ejecutado nunca nada. Ahora el
> mismo módulo corre observando y negando en la misma etapa, y `Operation not
> permitted` tiene su propio veredicto que manda a `RootFs::assemble`.
>
> **Y cuatro cosas que el arnés daba por hechas y ahora mide:** el modo al
> anunciar cada etapa; que `make -C lsm enforce` de verdad armó —§36 y §39, con
> bpftool y no con Thalyx, que es el sujeto—; en qué modo queda la máquina al
> terminar; y el `cleanup` del demo, que se tragaba el fallo de su restauración.
>
> **Lo que sigue esperando fierro:** que el `-EPERM` desapareció. Etapa 36 y
> etapa 39.

> ## El LSM le negaba a Thalyx confinar — 2026-08-26
>
> Los bloques de abajo son cómo se llegó.
>
> Segunda corrida en fierro: **169 `PROVEN`, 3 `NOT PROVEN`, 12 `FAILED`**, con
> la anterior en 171/2/4 y ningún cambio de código entre las dos que tocara nada
> de lo que se rompió. Doce fallas y una sola frase debajo de casi todas:
> `I/O error at /run/thalyx/sandbox/dev/null: Operation not permitted`.
>
> **El lanzador entraba al cgroup antes de armar la raíz**, así que el LSM leía
> la política del módulo y le negaba a Thalyx crear el punto de montaje de
> `/dev/null` — una escritura que el módulo nunca pidió y que el confinamiento
> necesita. En una máquina que niega no se podía lanzar nada, ni invitado ni
> módulo firmado. Es el defecto del 25 de agosto otra vez, arreglado entonces
> sólo para las lecturas.
>
> `RootFs` quedó partido en `assemble()` —toda la escritura— y `pivot_into()`, y
> el cgroup se toma entre las dos. La regla completa está en
> [[Estrategia-de-Pruebas]].
>
> ### Y dos defectos del arnés que acusaban a Thalyx
>
> **Por qué las dos corridas no dieron lo mismo:** la segunda corrió *negando*
> en etapas escritas para una máquina que sólo observa. `verify.sh` lo daba por
> hecho de principio a fin y no lo medía en ninguna parte. Ahora `step()` lee el
> modo al anunciar cada etapa, así que la próxima corrida **nombra la etapa que
> lo dejó armado** en vez de que nadie lo sepa. Hay un sospechoso —el `cleanup`
> del demo se traga el fallo de su restauración— y no está probado.
>
> **`exec-bare` y `exec-endure` nunca pasaron**, ni una vez desde que se
> escribieron: el guion devolvía la máquina a observación *antes* de que esas
> dos etapas lanzaran su invitado, y `ejecutar` se negaba, correctamente. El
> reporte las contaba como fallas de G1. La restauración va ahora después del
> último invitado.
>
> ### Qué falta comprobar
>
> Las 24 pruebas de aislamiento hacen el pivote completo en el contenedor y
> pasan, así que el reordenamiento no rompió el lanzamiento. **Que el `-EPERM`
> desapareció sólo lo puede decir una máquina que niegue**, y es la etapa 36.
> Lo siguiente es correr `verify.sh` otra vez.

> ## La primera corrida del sprint en fierro, y la carrera que encontró — 2026-08-26
>
> Los bloques de abajo son cómo se llegó.
>
> Cesar corrió `sudo ./dev/verify.sh` con el sprint dentro: **171 `PROVEN`,
> 2 `NOT PROVEN`, 4 `FAILED`.**
>
> ### Lo que se arregló
>
> De los cuatro `FAILED`, uno estaba diagnosticado por su propio mensaje: la
> suite, en un solo test de diez, con `I/O error at /sys/fs/cgroup/thalyx: File
> exists`. Era una **carrera** — `cgroup::parent()` preguntaba si el directorio
> existía y después lo creaba, y diez tests en paralelo caben de sobra en la
> ventana entre las dos líneas. El mensaje decía lo contrario de lo que pasaba:
> la máquina estaba bien.
>
> Arreglado en los tres lugares que tenían la misma forma —`parent()`,
> `Cgroup::ensure()` y, del otro lado, `Cgroup::remove()`, que reportaba
> `No such file or directory` sobre un invitado que ya había corrido bien— y
> con una prueba de ocho hilos contra una barrera que con el código viejo falla
> ocho de ocho. La regla quedó en [[Estrategia-de-Pruebas]]: **no preguntes si
> algo existe para después crearlo.**
>
> Este contenedor no tiene cgroup2, así que ese código nunca se ejerció aquí.
> Lo que aquí sí corre —la suite entera, `clippy`, `fmt`— está limpio.
>
> ### Lo que falta, y por qué no se pudo diagnosticar todavía
>
> Los otros tres `FAILED` son las tres etapas de §36 que **lanzan un invitado**
> (`exec-run`, `exec-bare`, `exec-endure`). Fallaron las tres, y el reporte no
> traía nada más que la ruta de un log que sólo existe en la máquina de Cesar.
> No hay diagnóstico: lo que se sabe es que el kernel cargó y que el flip a
> enforcement tomó —si no, `exec-bare` y `exec-endure` ni siquiera habrían
> corrido— y que la salida no contenía ninguna de las frases que `verify.sh`
> sabe reconocer.
>
> Para que la próxima corrida se conteste sola, `verify.sh` ahora imprime la
> cola del log junto al veredicto en **las 111 salidas del script que nombran
> uno**, no sólo las de §36.
>
> **Lo siguiente es correr `verify.sh` otra vez y leer esas tres colas.**

> ## Un sprint en vez de tres entregas, y el decreto que lo pidió — 2026-08-26
>
> Los bloques de abajo son cómo se llegó.
>
> Cesar preguntó qué seguía. Se le contestó con tres opciones, y cortó:
>
> > «creo que ya habiamos dejado claro esto, me pones de opciones cosas
> > sencillas de hacer, cuando algo es barato o no requiere de mi, hazlos todos
> > de golpe en un sprint y deja listos los tests o herramientas para verificar
> > que quedaron bien […] llevamos mucho tiempo haciendo sprints completos
> > dedicados a algo super sencillo, debemos parar eso».
>
> Tenía razón: [[Ritmo-de-Construccion]] ya lo decía —«qué hacer con un
> pendiente que ya está escrito» está en la columna de lo que **no** se
> pregunta— y se preguntó igual. La revisión quedó escrita en esa nota y
> resumida en `CLAUDE.md`, en la forma en que se rompió: **un menú donde todas
> las opciones son baratas y ya están decididas es una pregunta prohibida**, y
> **lo barato no se entrega de a uno**.
>
> ### Y antes de nada, el error que cometí al contestarle
>
> Se le dijo que el trabajo de G1 no estaba en `main` y que por eso nadie lo
> había podido correr. **Era falso.** `main` ya lo tenía; lo que se leyó fue la
> copia local de `origin/main`, sin `fetch`, con días de retraso. El merge que
> salió de ahí no aportó nada —árbol idéntico— y quedó en `main` como un
> commit vacío con un mensaje que dice algo que no pasó; quitarlo habría sido
> reescribir `main` y no se hizo.
>
> Es la **decimocuarta** vez que el instrumento resulta ser el problema, y la
> segunda de esta misma falta. Volvió porque la regla estaba escrita a medias:
> ahora dice que `origin/main` **sólo es una pregunta sobre el repositorio
> después de un `fetch`**. Ver [[Estrategia-de-Pruebas]].
>
> ### Lo que trae el sprint
>
> Tres pendientes, todos ya decididos, todos con su forma de comprobarlos.
>
> **1. Thalyx enciende y apaga su propio guardia.** El hueco que abrió el
> arreglo del 25: Thalyx leía el modo del kernel y no podía cambiarlo, porque
> cambiarlo era `bpftool` y la imagen no lo tiene. Dentro de la máquina no
> había forma de pasar de observar a negar, así que cada negativa cuyo remedio
> era *«hazlo vinculante»* nombraba un comando que ahí no existe.
>
> Ahora son dos verbos —`negar` y `observar`— más `thalyx enforce mode` para
> una máquina con shell. Dos y no uno con argumento porque un typo no puede
> desarmar la máquina, y **sólo el que afloja pregunta**: `negar` aprieta, y si
> rompe algo el algo lo dice; `observar` le quita el confinamiento a todo lo que
> esté corriendo, invitado incluido, y una máquina que dejó de negar en silencio
> se ve idéntica a una que niega y no tiene qué negar. La cara estructurada no
> puede pedir `observar` — `needs_a_human`, como `ejecutar`.
>
> **2. `ensayo correr`.** Era el único verbo que cambia la máquina y no se podía
> ensayar, y la razón escrita a su lado dejó de ser cierta el 25: lo que a una
> corrida se le va a permitir es una pregunta del kernel, y Thalyx ya la sabe
> contestar. El ensayo **es el código de la corrida**, parado un renglón antes
> de que el programa exista, así que no puede discrepar de ella. Dice qué
> programa correría, con qué aislamiento, **qué tiene en vigor** —no lo que pide
> el manifiesto—, si arrancaría, y si saldría **degradada**.
>
> **3. `ensayo editar`.** Salió mientras se cerraba el anterior: era el último
> que contestaba «no se puede ensayar todavía», y no lo nombraba ningún
> pendiente porque D1 lo contaba dentro de «archivos». Barato por la misma
> razón: `change` ya aplicaba en memoria y después guardaba, así que el ensayo
> es ese camino **sin la línea que guarda**.
>
> **Con esto D1 va nueve de nueve y la lista de verbos que cambian y no se
> pueden ensayar está vacía**, con una prueba que lo afirma.
>
> ### Qué está comprobado, y dónde
>
> Aquí: **1408 pruebas en verde**, `clippy` y `fmt` limpios. Las dos guardas
> nuevas se rompieron a propósito y cada mutación la agarró la prueba que le
> toca — quitar la puerta humana de `observar` tumbó dos, y un falso que
> reporta un cambio que no hizo tumbó exactamente una.
>
> Cada cosa lleva su control, porque sin él ninguna dice nada: `observar` y
> `negar` se niegan aquí por razones **distintas** (esa palabra es la decisión
> entera); el ensayo de `correr` no deja marca en el disco **y las mismas
> palabras con `sin-confinar` sí la dejan**; el de `editar` no toca los bytes
> **y sin `ensayo` sí los toca**, leídos con algo que no es Thalyx.
>
> **En tu máquina, dos etapas nuevas.** La **37** mide el guardia con
> `bpftool` y no con Thalyx —regla 5: preguntarle a Thalyx si sus cuatro bytes
> llegaron pasaría en una compilación donde la lectura y la escritura están mal
> en la misma dirección— con línea base, el acto, el control que lo mueve de
> vuelta, el verbo de sesión y el `n` con su `y` al lado. La **38** pregunta lo
> único que un contenedor no puede decir: qué contesta `ensayo correr` en una
> máquina que sí puede hacer cumplir, denegando y observando, que son las dos
> respuestas que se ven iguales si `degraded` estuviera mal.
>
> ### Para correrlo
>
> ```sh
> git pull && cargo install --path crates/thalyx-cli && sudo ./dev/verify.sh
> ```
>
> Y para verlo con las manos, en una sesión:
>
> ```
> estado
> negar
> ensayo correr <id>
> ```
>
> ### Lo que sigue, y ninguna de las dos es barata
>
> Lo que queda de la vara —un agente ajeno trabajando aquí— son **G2** y **G3**,
> y G2 empieza con una decisión tuya, no con código: **de dónde saca su runtime
> un programa que nadie firmó**. La imagen lleva el kernel y un programa, y no
> hay libc; el agente pide el enlazador antes que nada. Las salidas son una
> libc en la imagen, una raíz propia que el invitado trae y Thalyx monta, o
> sólo binarios estáticos — y la primera toca [[Filosofia-Fundacional]], así que
> es tuya. Ver [[Superficie-para-el-LLM]].

> ## Y el segundo intento encontró el de abajo — 2026-08-25
>
> Con el modo arreglado, Cesar corrió otra vez `ejecutar /usr/bin/node --version`
> — sin `leyendo`, sin `escribiendo`. El confinamiento se armó **entero**:
> cgroup 38600, usuario 700000, pivote, red cortada, 130 llamadas. Y murió
> antes de `node`:
>
> ```
> thalyx: I/O error at /sys/fs/cgroup/thalyx/foreign.node-22.…/cgroup.procs:
> Operation not permitted
> ```
>
> ### Qué pasaba
>
> Sin concesiones la política sale `allowed=0x0`, y el gancho `lsm/file_open`
> **no mira rutas**: mira si es lectura o escritura y consulta el bit. Con `0x0`
> se niega *cualquier* apertura de archivo.
>
> El lanzador escribe su pid en `cgroup.procs` desde **fuera** del cgroup —esa
> pasa— y enseguida **lo vuelve a leer** para comprobar que la entrada tomó.
> Esa lectura ya es desde dentro. Ni siquiera llegaba a `exec`, y abrir el
> binario también habría sido una apertura de archivo.
>
> ### Lo que más vale la pena de esto
>
> **Ya estaba encontrado, y rodeado.** La cabecera de `lsm/demo-enforcement.sh`
> dice que pone en el mapa *«filesystem allowed, network denied»*. Tenía que
> hacerlo: con el sistema de archivos negado, el `python3` de adentro no
> arrancaba. Esa conclusión —un proceso confinado necesita leer para existir—
> se descubrió, se rodeó, y **se quedó dentro del script**. Nada la
> contradecía porque nada más corría bajo enforcement: `verify.sh` va entero en
> modo observación.
>
> ### Cómo quedó
>
> El montaje decide **qué** ve un programa confinado; la política decide **leer
> o escribir** sobre eso. Las dos sólo componen si puede leer lo que se le
> montó, así que la lectura de lo visible **no es una concesión**: es el piso
> (`thalyx_permd::CONFINED_FLOOR`) que hace que el montaje signifique lo que la
> confirmación ya prometía — *«su propia carpeta, de sólo lectura, y las rutas
> de sistema»*. `escribiendo` sigue siendo lo único que abre la escritura.
>
> Se le da a la **política y nunca al perfil**: como permiso sobre `/` habría
> hecho que `RootFs` montara el sistema de archivos entero del anfitrión dentro
> del sandbox. Aplica a módulos igual — uno sin permiso de lectura tampoco podía
> abrir su propio binario. `thalyx enforce apply`, que ata un cgroup a mano para
> inspección, **no** lleva piso: tiene que escribir exactamente lo que se le
> pidió.
>
> Y las dos aperturas de `cgroup.procs` ahora dan errores distintos. Decían la
> misma frase, y esa frase era toda la evidencia de un fallo cuyas dos causas
> candidatas necesitaban arreglos opuestos.
>
> ### Lo que abrió, y Cesar cerró el mismo día
>
> Una entrada de política tiene **una** fecha de vencimiento, y las concesiones
> de `ejecutar` eran JIT: **treinta segundos**. Pasados, expiraba la entrada
> entera, el piso incluido — así que `ejecutar leyendo <ruta> …` no podía correr
> más de medio minuto, y la vara es un agente que corre minutos. El comentario
> encima de esa línea ya decía lo correcto —*«vive lo que vive el proceso»*—; el
> tipo elegido hacía lo contrario.
>
> **Cesar decidió: la concesión dura la corrida.** Tipo `Session`, sin plazo, y
> `release()` la retira al salir. Lo que se cede está dicho: los treinta
> segundos eran también el respaldo del kernel contra un Thalyx colgado que
> nunca llegue a `release()`; lo acota que el nombre del cgroup es determinista,
> así que la siguiente corrida del mismo programa sobrescribe la entrada.
>
> Se comprueba en la etapa 36 con un invitado que **duerme 35 segundos** y
> después lee lo concedido. Es la única forma que distingue las dos respuestas:
> la corrida tiene que ser más larga que el plazo que ya no debe existir.

> ## Lo primero que `ejecutar` dijo en su máquina encontró un hueco — 2026-08-25
>
> Lo de arriba es lo que se vio en cuanto esto se arregló.
>
> Cesar corrió `ejecutar /usr/bin/node --version` en su Fedora, justo después de
> `verify.sh`, y leyó:
>
> ```
> refusing to run `/usr/bin/node-22`: the kernel policy map is not loaded, so
> none of the 0 thing(s) this was granted would be enforced.
>   Load it with `make -C lsm load`. Nobody signed this program, so there is no
>   unconfined mode to fall back to.
> ```
>
> La negativa era correcta: `verify.sh` desengancha el LSM al salir. Dos cosas
> estaban mal de todos modos.
>
> ### La chica: la frase contaba cero
>
> «none of the 0 thing(s) this was granted» es el caso **ordinario** —
> `ejecutar <ruta>` sin palabras después no concede nada—, así que el caso
> ordinario era el roto. Ahora la cuenta es una cláusula que desaparece cuando
> no hay nada que contar, y hay una prueba que falla si vuelve a aparecer un
> cero.
>
> ### La grande: `make -C lsm load` no es lo que el mensaje creía
>
> El remedio que ese mensaje da deja la máquina en **modo observación** —
> `make -C lsm load` aterriza ahí a propósito, para poder medir una política
> antes de que ate. Los ganchos corren, cada negación se escribe en el anillo, y
> **ninguna se aplica**.
>
> O sea: la única acción que el sistema le pedía a Cesar lo dejaba justo donde
> `ejecutar` **sí** arrancaba al invitado y el kernel no le negaba nada.
>
> La causa: `is_available()` contesta *«¿se abre el mapa de políticas?»*, y todo
> el que decidía si confinar lo leía como *«el kernel está negando»*. El modo
> vive en otro mapa, `thalyx_enforcing`, que **nada en el lado de Rust había
> leído nunca** — sólo el `Makefile`, con `bpftool`. `thalyx enforce status`
> imprimía «kernel policy map: present» y se callaba.
>
> ### Qué se hizo
>
> | | módulo firmado | programa ajeno |
> |---|---|---|
> | mapa sin cargar | se niega, ofrece `sin-confinar` | **se niega**, no hay a qué caer |
> | cargado, observando | **corre degradado, y el journal lo dice** | **se niega**: `make -C lsm enforce` |
> | no se pudo leer el modo | corre degradado, y el journal lo dice | **se niega**: regla 9 |
> | cargado, negando | corre | corre |
>
> La asimetría es la de [[Programas-Ajenos]] entera: a un módulo lo firmó
> alguien y un humano leyó su manifiesto, así que un run degradado que el
> journal nombra es auditable. Detrás de un invitado no hay nadie, y un
> confinamiento que no niega no es un confinamiento.
>
> `thalyx enforce status` ahora dice el modo. La cara de máquina de `correr`
> lleva `enforcing` al lado de `confined`, por la misma razón que `confined`
> está ahí. El falso, `MemoryStore`, ganó los tres estados — porque el motivo de
> que ninguna prueba agarrara esto es que **el modo de fallo no existía en el
> falso**, y lo que no se puede nombrar no se puede probar.
>
> ### Qué está comprobado
>
> Aquí: 1384 pruebas en verde (siete nuevas), `clippy` y `fmt` limpios. Las tres
> guardas nuevas se rompieron a propósito y cada mutación la agarró **la prueba
> que le toca** — incluida la columna de control, que atrapó la versión que se
> niega siempre y se vería idéntica a una que funciona.
>
> En su máquina, tres etapas nuevas de `verify.sh`: que `thalyx enforce status`
> diga «observing» cuando el script lo dejó observando, que un módulo corrido
> bajo un kernel que observa **lo diga**, y que un invitado sea rechazado ahí
> mismo. Y la etapa 36 ahora **enciende el enforcement para su corrida real y lo
> vuelve a dejar como estaba** — sin eso, la etapa entera reportaría una
> negativa y la llamaría una máquina que no puede hacer cumplir nada.
>
> ### Lo que abrió
>
> Thalyx **lee** el modo sin `bpftool`. **Cambiarlo** todavía es
> `make -C lsm enforce`, o sea `bpftool`, que la imagen no tiene: dentro de la
> máquina no hay forma de pasar de observar a negar. Escrito en
> [[Tareas-Pendientes]]; es una escritura de cuatro bytes en un mapa que ya se
> abre.

> ## G1: Thalyx ya puede correr un programa que nadie firmó — 2026-08-25
>
> Lo de arriba corrige un hueco que esto dejó abierto.
>
> Cesar delegó la forma —*«lo que veas conveniente que sea coherente con nuestra
> filosofía»*— y ésta es la forma, con la coherencia escrita en
> [[Programas-Ajenos]] antes de escribir una línea de código.
>
> ### Qué se destrabó, y por qué llevaba parado desde el 23
>
> `G1` de [[Superficie-para-el-LLM]] era el punto que bloqueaba la vara del
> proyecto. La medición del 23 lo había dejado sin ambigüedad: no faltaba una
> llamada al sistema —el filtro cubre 41 de 41— ni una ruta. Faltaba que
> `correr` sólo lanza **módulos instalados y firmados**, y un agente ajeno no es
> ninguna de las dos cosas.
>
> Lo que lo destrabó no fue código, fue **no tocar la firma**. Si Thalyx firmara
> al vuelo lo que se le pide ejecutar, la firma dejaría de significar *alguien
> respondió por esto* y pasaría a significar *esto pasó por aquí* — la palabra
> sin significado para quien lea la siguiente. Así que son dos verbos:
>
> | | `correr <id>` | `ejecutar <ruta>` |
> |---|---|---|
> | qué lanza | un módulo firmado | un programa cualquiera |
> | quién respondió por él | su publicador | **nadie** |
> | canal con la API | sí, nace con él | **no, nunca** |
> | `sin-confinar` | existe, y queda como degradado | **no existe** |
>
> ### Las tres decisiones que aguantan el peso
>
> 1. **No hay canal.** Un módulo nace sosteniendo un socket a la API de Thalyx;
>    un invitado no recibe ninguno. Eso es lo que impide que este verbo sea una
>    puerta trasera: por aquí no se instala nada, no se concede nada persistente
>    y no se pide nada, porque no hay por dónde pedirlo.
> 2. **No hay modo degradado.** `sin-confinar` existe para módulos y se
>    justifica en que un humano leyó ese manifiesto y su publicador respondió.
>    De un programa ajeno nadie respondió nada, así que si la máquina no puede
>    hacer cumplir la política, el verbo **se niega** — y el mensaje dice que ese
>    modo no existe, en vez de ofrecerlo.
> 3. **Ve lo que se le nombró.** Su propia carpeta de sólo lectura, las rutas de
>    sistema, y lo que diga `leyendo <ruta>` o `escribiendo <ruta>` — cada cosa
>    dibujada por Thalyx y confirmada antes de que el proceso exista. Su usuario
>    se guarda con la llave `foreign:<ruta canónica>`, así que el mismo programa
>    es el mismo usuario mañana y dos programas distintos nunca comparten uno.
>
> ### Qué se comprobó aquí y qué espera tu máquina
>
> Etapa **36** de `verify.sh`, con su columna de control: el mismo script corrido
> **fuera** del sandbox tiene que alcanzar las dos rutas, o el «no las alcanzó»
> de adentro no significa nada. En este contenedor da cinco `PROVEN` y un
> `NOT PROVEN`:
>
> - **probado aquí** — el control de afuera; `ensayo ejecutar` resuelve el
>   programa y no corre nada; **un `n` no corre el programa** (comprobado por lo
>   que *no* apareció en el disco, no por lo que imprimió la sesión); el journal
>   lo llama `run_foreign` y nunca `run_module`; y la cara estructurada se niega
>   con `needs_a_human` / `confirm_at_a_terminal`.
> - **espera tu máquina** — lo que un invitado ve. Aquí no hay mapa de política
>   en el kernel, así que el verbo se niega, que es el decreto funcionando. El
>   `NOT PROVEN` dice además algo cierto: esa negativa viene del núcleo, o sea
>   que el `y` sí se leyó y se aceptó. Lo que no corrió es el invitado.
>
> Más seis pruebas de integración en `a_program_nobody_signed_can_run.rs`. Dos
> corren aquí —la negativa sin nada que haga cumplir, y el journal—; las otras
> cuatro necesitan los controladores `memory` y `pids` delegados y dicen
> `NOT PROVEN` donde no los hay, con `THALYX_REQUIRE_CONTROLLER_TESTS`.
>
> **En tu máquina esas cuatro corren.** `cargo test --workspace`: 1384 en verde.
>
> ### Lo que esto no hizo
>
> - **No abrió la red** (`G3`), no es `E1` —las concesiones son de una corrida,
>   no expiran porque terminan— y **no resolvió `G2`**: la imagen sigue sin
>   libc, así que `ejecutar` sirve donde hay rutas de sistema que montar, o sea
>   tu Fedora. Dentro de la imagen instalada sirve para lo que esté enlazado
>   estáticamente.
> - No le quitó nada a `correr`. El decreto de firma sigue entero.
>
> ### Para correrlo
>
> ```sh
> git pull && cargo install --path crates/thalyx-cli && sudo ./dev/verify.sh
> ```
>
> Y para verlo con las manos, en una sesión:
>
> ```
> ejecutar leyendo /home/cesarmanzocode/algo /usr/bin/ls /home/cesarmanzocode/algo
> ```

> ## Verde, y la orden de dejar de pulir — 2026-08-25
>
> Cesar corrió `verify.sh` en su máquina: **`156 proven · 2 not proven ·
> 0 failed`**. Las dos fallas del día anterior están cerradas, y eran la misma
> cosa vista dos veces —la prueba nueva y la etapa del módulo, las dos
> preguntando con `chrt --other`, que en util-linux 2.41 sale por
> `sched_setattr`—. El arreglo no tocó el filtro: cambió con qué se le pregunta.
>
> ### Y con eso, la corrección que importa más que el número
>
> Cesar cortó la pregunta de qué seguía, y con razón:
>
> > «llevamos mucho tiempo sin avanzar nada realmente, estamos siendo muy
> > cautelosos […] le estamos dando demasiada importancia a cosas muy simples y
> > faciles de hacer […] tenemos que empezar a ser agresivos sin ser estupidos,
> > la perfeccion vendra despues».
>
> Medido contra el registro, tenía razón: del 23 al 25 se construyó un guardia
> por argumento, dos llamadas de rango de prioridades, tres arreglos del arnés y
> una prueba que pregunta con la herramienta correcta. Todo cierto, y ninguno de
> esos días movió la vara de [[Filosofia-Fundacional]] —un agente ajeno
> trabajando aquí— ni un milímetro.
>
> Quedó decretado en [[Ritmo-de-Construccion]], con sus palabras textuales, y
> resumido en `CLAUDE.md` para que una sesión nueva lo lea antes de preguntar
> nada. En una línea: **se le pregunta sólo lo que sólo él puede contestar**
> —cambiar un decreto suyo, escribir donde se pierde algo suyo, gastar su hierro
> o su dinero, alcance que la bóveda no cubre—. Todo lo demás se hace y se le
> dice qué se hizo. Un pendiente ya escrito en [[Tareas-Pendientes]] ya fue
> decidido por él; volver a preguntarlo es pedirle que decida dos veces.
>
> Lo que **no** baja: ninguna de las diez reglas de [[Estrategia-de-Pruebas]],
> ningún decreto sin él, ninguna entrega a medias, y `NOT PROVEN` sigue siendo
> `NOT PROVEN`.
>
> ### Lo que se hizo ese mismo día sin preguntar
>
> 1. **`README.md` y `docs/STATUS.md` dicen la corrida vigente.** Citaban la del
>    23 —`134 proven`— porque la del 24 tenía fallas y no era la que correría
>    ahora. Ya no hay una que esconder. El párrafo que explicaba una caída de
>    conteo se volvió la regla que la caída enseñó: **un conteo que se mueve no
>    es una calificación**, y lo que dice qué pasó es la lista de abajo, no el
>    número.
> 2. **El caso de aislamiento sobre un archivo, que llevaba abierto desde el
>    2026-08-04.** Ver [[Tareas-Pendientes]]. Dos pruebas nuevas en
>    `isolation.rs`: un permiso de escritura sobre **un solo archivo**, con la
>    raíz remapeada de verdad, comprobado en el anfitrión —el contenido llegó al
>    mismo archivo y el archivo no cambió de dueño—; y su control, que afirma
>    que **el vecino de al lado no viene con él**. Las dos se rompieron a
>    propósito antes de creerles, y las dos **corren en este contenedor**: hay
>    un cgroup2 en `/sys/fs/cgroup/unified` y los montajes remapeados funcionan
>    aquí, así que esto no espera hierro. `cargo test --workspace`: 1359 en
>    verde.
>
> ### Las dos que quedan sin comprobar
>
> `verify.sh` las nombra en su propio resumen —el bloque `What this run could
> not establish:`— y ésa es la autoridad, no lo que se escriba aquí. Si son las
> del agente, se cierran así:
>
> ```sh
> git pull && cargo install --path crates/thalyx-cli
> sudo THALYX_AGENT_BINARY=/home/cesarmanzocode/src/llama.cpp/build/bin/llama-completion \
>      THALYX_AGENT_WEIGHTS=/ruta/a/tu/modelo.gguf \
>      ./dev/verify.sh
> ```
>
> ### Y lo siguiente, que sí es una decisión suya
>
> **G1 y G2 de [[Superficie-para-el-LLM]].** Es lo único que bloquea la vara del
> proyecto, y lleva bloqueándola desde que se midió el 2026-08-23:
>
> - **G1** — hoy `correr` sólo lanza módulos **instalados y firmados**, y un
>   agente ajeno no es ninguna de las dos cosas. El sandbox ya lo aguantaría: el
>   filtro cubre 41 de 41 llamadas medidas. Lo que falta no es mecanismo, es
>   **qué se permite lanzar**, y eso es un decreto suyo.
> - **G2** — la imagen lleva el kernel y un programa, así que no hay libc, y un
>   binario enlazado dinámicamente no arranca ahí. Ver
>   [[Que-Necesita-Un-Agente-Ajeno]].
>
> Las dos son la misma pregunta vista desde dos lados, y ninguna se puede
> construir sin que él decida primero.

> ## La segunda puerta: `chrt` medía la versión de util-linux, no el filtro — 2026-08-25
>
> La corrida siguiente dio `155 proven · 2 not proven · 2 failed`, y las dos
> fallas eran la misma cosa vista dos veces: la prueba nueva y la etapa del
> módulo, las dos preguntando con `chrt --other`.
>
> ### Y corrige lo que se dijo ayer
>
> Ayer quedó escrito que las tres fallas se habían reproducido en el contenedor.
> **La del filtro no.** El contenedor tiene util-linux 2.39 y su máquina tiene
> 2.41, y desde 2.41 `chrt --other` pone una política ordinaria con
> `sched_setattr` en vez de con `sched_setscheduler`. El verde de aquí fue
> **suerte de versión**, no una comprobación — y la prueba se había escrito el
> mismo día en que se anotó que un programa real es mejor instrumento que una
> llamada aislada. Lo es; falta preguntarse qué llamada hace ese programa en la
> máquina donde va a correr.
>
> ### Lo que había debajo, que sí es de diseño
>
> `sched_setattr` es **una segunda puerta a la misma capacidad**. Pone la
> política igual que `sched_setscheduler`, pero la recibe dentro de una
> estructura, detrás de un puntero — y un filtro de seccomp compara registros y
> no puede seguir un puntero. Para esa puerta no existe guardia por argumento: o
> se permite entera, con `SCHED_FIFO` adentro, o se deniega entera.
>
> **Cesar decidió el 2026-08-25 denegarla.** Queda en [[Sandbox-Ejecucion]] con
> su costo escrito: un programa que ponga política ordinaria sólo por esa puerta
> no puede hacerlo aquí, y `chrt --other` de util-linux 2.41 es uno. Ningún
> runtime medido depende de ella —la traza del agente ajeno lo muestra
> arrancando con `sched_setscheduler` y con nada más—. La única cosa que podría
> mirar detrás del puntero, un supervisor con `SECCOMP_RET_USER_NOTIF`, queda
> anotada en [[Tareas-Pendientes]] como opción y no como pendiente.
>
> ### Qué cambió, y qué no
>
> **El filtro no cambió hoy.** Lo que cambió es con qué se le pregunta:
>
> - La columna ordinaria pregunta con `chrt --idle 0 true`. Ninguna versión de
>   util-linux lo manda por la puerta cerrada, y `SCHED_IDLE` es una de las tres
>   políticas que el guardia permite: sigue siendo un programa ajeno recorriendo
>   el camino entero hasta la llamada guardada, que es lo que hacía valioso a
>   `chrt`.
> - `--other` se sigue corriendo, como **reporte y nunca como veredicto**, con
>   `strace` fuera del sandbox diciendo por cuál de las dos llamadas pasó este
>   `chrt`. Así el costo de la puerta cerrada se ve en la máquina donde se paga,
>   medido y no supuesto.
> - El segundo `NOT PROVEN` de la corrida fue la baranda de ayer haciendo su
>   trabajo: la denegación de tiempo real se calló porque la columna ordinaria no
>   estaba en 0. Sin ella, esa línea habría dicho verde con el módulo muriendo
>   antes de nombrar ninguna política.
>
> ### Para cerrar las dos que quedan
>
> ```sh
> git pull && cargo install --path crates/thalyx-cli
> sudo THALYX_AGENT_BINARY=/home/cesarmanzocode/src/llama.cpp/build/bin/llama-completion \
>      THALYX_AGENT_WEIGHTS=/ruta/a/tu/modelo.gguf \
>      ./dev/verify.sh
> ```

> ## Tres fallas en `verify.sh`: dos eran del arnés y una era del filtro — 2026-08-24
>
> La corrida de Cesar en su máquina dio `154 proven · 1 not proven · 3 failed`.
> Las tres fallas están diagnosticadas y arregladas, y **sólo una era de
> Thalyx**. Ninguna de las tres necesitó su hardware para reproducirse: las tres
> se reprodujeron en el contenedor.
>
> ### 1. El guardia mataba la llamada que existe para dejar pasar — era real
>
> `sched_ordinary=159`: el módulo confinado murió con `SIGSYS` al poner un hilo
> suyo en una política ordinaria. El guardia por argumento de ayer estaba bien
> escrito; **el camino hasta él no estaba permitido**. `chrt` pregunta primero
> el rango legal de prioridades —`sched_get_priority_min` y
> `sched_get_priority_max`— y ninguna de las dos estaba en la lista. Las dos
> contestan una constante y no cambian nada. Ya están permitidas.
>
> Lo que vale más que el arreglo: **la columna de al lado estaba en verde por la
> razón equivocada.** `chrt --fifo 1 true` moría en esa misma primera línea, sin
> haber nombrado jamás una política de tiempo real, y eso se lee idéntico a que
> el guardia lo haya rechazado. La denegación se estaba afirmando sin medirse.
> Ahora `verify.sh` se calla ahí mientras la columna ordinaria no dé 0, y hay una
> prueba en el workspace que **instala el filtro de verdad** en un proceso
> aparte y corre `chrt` bajo él, con las dos columnas en una sola prueba para
> que nadie las lea por separado. Falla sin el arreglo; se comprobó.
>
> ### 2. Las siete sondas de inyección estaban pasando por vacías — era el arnés
>
> `verify.sh` buscaba `A CONTRACT WAS PRODUCED` en la salida de
> `dev agent-probe`. La sonda dejó de imprimir esa frase el mismo 24, cuando un
> plan pasó a poder ser un verbo y no sólo un contrato. Las siete comprobaciones
> de «ninguna forma de portarse mal produjo nada» **pasaban sin mirar nada**.
>
> Lo agarró el control positivo —el que exige que el mismo modelo, preguntado
> por lo que el humano tecleó, sí produzca uno—, que es la falla que Cesar vio
> como «the control behaved as neither a refusal nor a contract». La regla 4
> pagándose sola.
>
> ### 3. `agent grammar` sí imprimía la gramática — era el arnés
>
> La etapa exigía la palabra `install_module`. La gramática deletrea el verbo
> como lo deletrea la sesión, `install`; `install_module` es como se llama la
> operación en el **contrato** y sigue siendo alias aceptado por el analizador.
> La etapa pedía una palabra que no está ahí.
>
> ### De pasada: el instrumento del agente ajeno contaba mal
>
> `dev/foreign-agent-needs.sh` sacaba las llamadas permitidas de todo
> `seccomp.rs`, que también nombra 32 que un módulo tiene **prohibidas** —las de
> las pruebas que afirman su ausencia y las que sólo agrega un permiso de red—.
> Un agente que llamara a `socket` habría salido como cubierto. Corregido a leer
> el cuerpo de `module_standard`, y **vuelto a correr**: la respuesta no cambió,
> 41 de 41.
>
> ### El `NOT PROVEN` no es una falla, y sigue en pie
>
> Ningún modelo real corrió: `llama-completion` está instalado y no en el `PATH`
> de root, y `THALYX_AGENT_WEIGHTS` no estaba puesto. Es la etapa diciendo
> exactamente lo que no pudo comprobar. Para cerrarla:
>
> ```sh
> git pull && cargo install --path crates/thalyx-cli
> sudo THALYX_AGENT_BINARY=/home/cesarmanzocode/src/llama.cpp/build/bin/llama-completion \
>      THALYX_AGENT_WEIGHTS=/ruta/al/modelo.gguf \
>      ./dev/verify.sh
> ```
>
> Las asignaciones van **después** de `sudo`, porque `sudo` no lleva el entorno.
>
> ### Lo que falta y sólo se puede hacer en su máquina
>
> Volver a correr `verify.sh`. Lo que este contenedor no puede decir sigue sin
> decirlo: el LSM, los controladores de cgroup y Btrfs. Lo que sí quedó
> comprobado aquí es el filtro sobre un programa real, que es donde estaba el
> defecto.
>
> Y con el resultado de esa corrida se actualiza el párrafo de estado de
> `README.md` y de `docs/STATUS.md`, que todavía citan la corrida del 23 —`134
> proven · 2 not proven · 0 failed`—. No se cambió con los números de hoy a
> propósito: citar el conteo de una corrida que falló, y que ya no es la que
> correría ahora, es escribir un número que nadie midió.

> ## La gramática del agente es el catálogo entero — 2026-08-24
>
> Cesar decidió las dos cosas que quedaban abiertas y que no eran código sino
> alcance. Las dos están construidas.
>
> ### 1. Qué puede proponer el modelo: todo el catálogo
>
> `Superficie-para-el-LLM.md` dejaba la pregunta abierta y le ponía dos
> condiciones. Las dos se resolvieron construyendo, y ninguna de las dos era la
> que parecía.
>
> **La abstención dejó de ser expresable, y se dio cuenta sola.** Mientras
> `install_module` era la única operación, una lista de objetivos vacía la
> decía: nada que instalar es nada que hacer. La mayoría de los verbos del
> catálogo no toman argumentos, así que una lista vacía en `disks` es una
> petición completa. Uno de los dos significados tenía que mudarse, y la
> abstención tiene ahora palabra propia: `nothing`. Las dos siguen valiendo,
> porque **todas las muestras capturadas de un modelo real absteniéndose usan
> la lista vacía** y la regla 6 dice que una muestra reescrita ya no es la
> muestra.
>
> **El otro condicionante era el que importaba, y no estaba escrito así.**
> `assemble` escribía `Operation::InstallModule` en cada contrato que armaba,
> porque mientras había una sola operación no había otra cosa que escribir. El
> día que el modelo pudiera proponer `disks`, esa línea habría producido **un
> contrato para instalar un disco**: un plan que se llama a sí mismo otra cosa,
> que es la única forma de estar mal que quien lo lee no puede ver.
>
> Así que un plan tiene dos formas. Un contrato es lo que
> [[Contrato-Estructurado]] le da a una operación que **cambia la máquina y
> necesita que un humano diga que sí**. Preguntar qué discos hay no es eso, y
> vestirlo de contrato deja la palabra sin significado para quien lea el
> siguiente.
>
> **Y ahí apareció el hueco.** Un plan de verbo no tiene contrato, así que
> nunca llegaba a `Contract::validate`, así que nunca llegaba a
> `origins.validate()` — que es la comprobación que rechaza una operación
> concluida mientras se leía una página hostil. La regla de procedencia habría
> quedado con **una puerta rotulada `read`**. Se valida en los dos caminos, con
> prueba en los dos sentidos: la lectura inyectada se rechaza, la que pidió el
> humano no.
>
> La gramática son tres formas de objeto en vez de una, así que ensancharla no
> costó nada de lo que ya compraba: `install` y `run` conservan la regla de
> DNS inverso, todo lo demás recibe una clase de caracteres que cubre rutas y
> nombres y **no puede cerrar la cadena JSON en la que está**, y `nothing` tiene
> un objeto sin argumentos.
>
> Tres pruebas cayeron y ninguna era por este cambio: **las palabras del
> catálogo son inglés ordinario**. `permissions` es un verbo ahora, así que una
> prueba que buscaba la palabra en cualquier parte reportó el catálogo como una
> fuga de procedencia; el brazo de prosa del experimento de gramática "nombraba
> una operación" porque contiene la palabra `where`; y la sonda leía una
> producción `root` donde ahora hay tres. Las tres son el instrumento.
>
> ### 2. `sched_setscheduler`: sí, pero sin tiempo real
>
> Cesar entendió el problema y me dejó decidir los costos. La llamada son dos
> peticiones con un solo nombre. Un runtime acomodando sus propios hilos dentro
> del pedazo de procesador que el cgroup ya le dio es ordinario y lo hace antes
> que nada. Un programa pidiendo política de **tiempo real** está pidiendo
> quedarse un procesador contra todo lo demás de la máquina, Thalyx incluido, y
> ningún límite de cgroup se lo quita.
>
> El filtro aprendió a mirar un argumento. Y lo que ese cambio enseñó **sólo
> aparece corriendo**: la primera versión del guardia permitía `SCHED_OTHER`,
> `SCHED_BATCH` y `SCHED_IDLE`, que es lo que sugiere el manual y lo que
> cualquiera escribiría. Node pide `0x40000000` —`SCHED_OTHER |
> SCHED_RESET_ON_FORK`— en cada hilo. **Ese guardia habría matado al agente
> ajeno en la llamada exacta que el guardia existe para dejar pasar, y habría
> parecido el guardia funcionando.**
>
> Con eso, `dev/foreign-agent-needs.sh` dice **41 de 41**. En la capa de seccomp
> ya no falta nada para que un agente ajeno arranque. Lo que bloquea sigue
> siendo G1 y G2, que es lo que la medición del 2026-08-23 ya decía.
>
> ### Lo que queda
>
> - **`thalyx agent bench`** — el único `NOT PROVEN`. No es una decisión, es una
>   medición que necesita su máquina y unos minutos:
>   `sudo THALYX_AGENT_BENCH=1 ./dev/verify.sh`.
> - **`agent do` sólo lleva a cabo instalaciones.** Poder decir una cosa no es
>   poder que se haga: todo lo demás pasa por el verbo, en una terminal, con la
>   confirmación que ese verbo ya pide. Ensancharlo es otra decisión de Cesar y
>   no se tomó.
> - Lo de siempre que necesita hierro: `net/outbound` de punta a punta, cargar
>   `thalyx_watch` con el cargador propio, la deuda de explicación de `/home`
>   `NOEXEC`.

> ## Qué necesita un agente ajeno para arrancar, medido — 2026-08-23
>
> Los bloques de abajo son cómo se llegó.
>
> Tercera entrega del sprint, y es la que más cambia lo que creíamos. El
> pendiente decía *«tomar Claude Code, mirar qué llama, y hacer la lista; es
> barato y no se ha hecho, y sin ella todo lo de abajo es adivinado»*. Estaba
> abierto desde el 2026-08-09. Se hizo con `strace`, en veinte minutos.
>
> **De las 41 llamadas al sistema que Claude Code hace para arrancar,
> `module_standard` ya permite 40.** La que falta es una: `sched_setscheduler`.
> De las 19 rutas que abre, 13 caen dentro de lo que un módulo ve.
>
> Eso contradice de frente la frase que estaba escrita debajo del decreto —*«hoy
> no arrancarían, así que esto no es afinar, es construir»*—. En la capa donde
> más caro parecía, el filtro de llamadas de este proyecto ya cubre el 97.5% de
> lo que un agente ajeno pide para existir. **La afirmación era razonable y
> nadie la había medido.**
>
> **Dónde sí es cierta**, y ahora con nombre en vez de por suposición:
>
> - **El enlazador.** El agente abre `/etc/ld.so.cache` y cinco objetos
>   compartidos. La imagen lleva `/init`, unos directorios y `/dev/console` — no
>   hay libc. Un binario enlazado dinámicamente no arranca ahí, y eso es
>   **exactamente la pregunta abierta del ABI de los módulos**, hecha por el
>   agente antes que ninguna otra.
> - **`G1`, lanzar un proceso arbitrario.** No es una llamada que falte ni una
>   ruta: es que `correr` sólo lanza módulos instalados y firmados. La medición
>   lo confirma como el que bloquea en vez de contradecirlo.
> - **`/home` montado `NOEXEC`.** Un agente que aterrice ahí no se ejecuta
>   aunque todo lo demás esté resuelto. La deuda de explicación que aplazaste el
>   2026-08-09 ahora tiene un caso concreto detrás en lugar de ser hipotética.
>
> Y **seis rutas bajo `/sys`** que un módulo no ve. De las seis sólo se puede
> afirmar algo de una: `trace_marker` dio `ENOENT` y arrancó igual, o sea que no
> hace falta. De las otras cinco lo único cierto es que aquí no tuvo que
> arreglárselas sin ellas — la sospecha razonable es que degrada a valores por
> omisión, y una sospecha razonable no se apunta como medición.
>
> La lista entera está en [[Que-Necesita-Un-Agente-Ajeno]], **con la mitad que
> dice qué NO contesta**: arrancar no es trabajar. No hubo red, ni terminal, ni
> subprocesos, ni una sola escritura.
>
> Se reproduce con `dev/foreign-agent-needs.sh`, que es un script y no un
> párrafo porque un procedimiento impreso para una persona es código que no
> corre.

> ## El ensayo llegó a los verbos que cambian la máquina — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Segunda entrega del sprint. El punto **D1** de [[Superficie-para-el-LLM]]
> —ensayo en todo verbo que cambia— estaba en «hecho para los verbos de
> archivos, los otros cinco dicen que no pueden». Ahora está en **ocho de
> nueve**.
>
> Y salió casi gratis, por una razón que vale más que los cuatro verbos:
> **cuatro de los cinco ya tenían escrita la mitad que averigua**, separada de la
> que actúa. `revertir` tiene `plan` aparte de `apply` desde que se escribió.
> `instalar` resuelve el candidato y lee su manifiesto antes de preguntar nada.
> Y `instalar-en` calcula la distribución entera, encuentra el kernel y lee qué
> hay en el disco **antes** de la confirmación — decisión del 2026-08-07, tomada
> por otra razón completamente distinta: que un borrado ya confirmado no
> descubriera después que no había kernel que escribir.
>
> Así que el ensayo no fue una segunda implementación de nada: **fue parar en la
> línea que ya estaba dibujada.** Para el único verbo irreversible del sistema,
> que no exista una segunda implementación que se pueda desalinear no es un
> detalle.
>
> `ensayo instalar` es el que más se usa y contesta lo que una persona sólo podía
> ver empezando la instalación y declinando: qué pide el módulo, si alguno de
> esos permisos necesita a alguien en una terminal, y si reemplaza algo que ya
> está.
>
> **Comprobado con su control, que es lo que lo hace valer**: el ensayo deja el
> store sin journal y sin módulos, y la instalación de verdad del mismo bundle
> deja las dos cosas. Sin esa segunda columna, un ensayo que se cayera antes de
> hacer nada se vería igual.
>
> `correr` es el único que queda y se queda diciendo que no puede: qué podría
> hacer un módulo al correr es una pregunta del lado del kernel, y contestarla
> desde el manifiesto describiría una corrida que la máquina quizá no puede dar.
>
> **Y una prueba estaba tapando dos hechos con una sola palabra.** `cannot`
> significaba a la vez *este verbo no tiene ensayo* y *aquí no hay nada que
> deshacer*. El primero manda al que preguntó a otro lado para siempre; el
> segundo deja de ser cierto en cuanto se instale algo.

> ## Los cuarenta verbos contestan por estructura — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Cesar leyó la lista de pendientes y contestó lo que había que contestar: que
> íbamos innecesariamente lento, que **todo eso es horizontal y ninguna pieza es
> difícil**, y que en vez de elegir una hiciéramos un sprint para eliminar el
> horizonte barato entero. Ésta es la primera entrega.
>
> Nueve verbos no tenían cara estructurada, más tres que se creían sin nada que
> contestar. **Ninguno era difícil.** Lo que los dejó así es que el catálogo de
> [[Superficie-para-el-LLM]] trata de superficie *nueva*, y éstos son anteriores
> al decreto de las dos caras.
>
> **Lo que eso costaba, dicho bien:** `disponibles`, `instalar`, `modulos`,
> `correr`, `permisos` y `revertir` son el ciclo completo de lo único que Thalyx
> existe para dejar hacer. Catorce de diecinueve puntos del catálogo hechos, y el
> ciclo entero en prosa — un programa no podía saber si lo que iba a instalar ya
> estaba instalado, ni qué había en el repositorio, ni qué concedió la vez
> pasada.
>
> Ahora el ciclo entero se corre por la cara estructurada, y quedó capturado en
> una sola sesión por tubería:
>
> ```
> {"op":"available","ok":true,"total":1}
> {"op":"install","ok":true,"module_id":"org.thalyx.face"}
> {"op":"modules","ok":true,"total":1}
> {"op":"run","ok":false,"error":"cannot_enforce","remedy":"run_unconfined"}
> {"op":"rollback","ok":true,"undid":"undo install_module of org.thalyx.face 1.0.0"}
> {"op":"modules","ok":true,"total":0}
> ```
>
> **El camino confiable no se debilitó, se reporta.** [[Camino-Confiable]] queda
> intacto: sin terminal no hay confirmación y no hay instalación, y `instalar-en`
> sigue pidiendo la ruta del disco tecleada. Lo único que cambia es que la
> negativa vuelve como objeto en vez de una línea en `stderr`, donde un parser
> que lee un solo flujo no la veía nunca.
>
> **Y salió una afirmación falsa, de correrlo y no de leerlo.** El primer campo
> de esa negativa decía `wrote_anything: false`. El journal **sí** guarda una
> entrada `rejected` — que es justamente el punto: una negativa del camino
> confiable que no dejara rastro sería un camino confiable que nadie puede
> auditar. Dice `installed: false`, que es lo cierto. La cara humana llevaba el
> mismo exceso en prosa y también se corrigió.
>
> Los tres que "no tenían nada que contestar" eran el último sitio donde quedaba
> silencio, y las tres razones son distintas: `limpiar` no limpia nada porque del
> otro lado no hay pantalla, `salir` contesta **antes** de que el pipe se cierre
> porque un pipe cerrado y vacío es exactamente lo que parece un cierre
> inesperado, y `apagar` contesta antes de la llamada al sistema porque cuando
> funciona no regresa.
>
> Lo que lo sostiene: la prueba del catálogo **afirma que la lista de verbos
> sólo-prosa está vacía**, y la etapa 22 pasó de catorce verbos manejados a
> veintiuno. Control corrido en los dos sentidos.
>
> **Nada de esto necesita hierro.** Sigue faltando el banco de gamas, que es una
> medición y no una comprobación.

> ## `describe` prometía prosa donde había un objeto — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Revisando qué quedó desalineado después de la corrida en verde salió **un
> defecto real, y no estaba en la bóveda sino en el código**: `red` se construyó
> ese mismo día con sus dos caras y quedó declarado `answers: None` en el
> catálogo. `describe` es lo primero que lee un programa y por cada verbo dice si
> contesta por estructura; **un verbo declarado sólo-prosa es un verbo que un
> programa nunca llama**. La única lista de hardware de red que esta máquina
> tiene fue invisible para eso durante el día entero, sin producir un solo error.
>
> Nadie lo vio porque **el catálogo y el despacho son dos archivos y cada uno
> concuerda consigo mismo**: las pruebas de `net` ejercen la cara estructurada y
> pasan, y la prueba del catálogo afirmaba que `modules` seguía siendo sólo-prosa
> —un `contains` sobre un ejemplo no ve que otro se movió—.
>
> Tres cosas cambiaron:
>
> - `red` declara `answers: Some("network")`;
> - la prueba del catálogo **fija la lista entera** de verbos sólo-prosa, así que
>   agregar una cara obliga a editar ese renglón;
> - la **etapa 22** corre los catorce verbos que aquí se pueden correr sin
>   argumentos y compara el cable contra lo que `describe` prometió, en las dos
>   direcciones. Con el defecto devuelto a mano dice
>   `red:promised-prose-answered-network`; sin él, `ok:14`.
>
> La regla nueva está en [[Estrategia-de-Pruebas]]: **una afirmación que un
> sistema hace sobre sí mismo se comprueba corriéndolo, no leyendo los dos lados
> del código.**
>
> Y de paso quedaron alineados dos pendientes de [[Tareas-Pendientes]] que
> seguían marcados abiertos y estaban cerrados desde el 2026-08-10: los tres de
> «sólo hierro» —de los que `D2` y `B3` se construyeron y sólo queda `E1`— y los
> tres que se podían hacer aquí —`B1`, `C2` y `F2`, los tres hechos—.
>
> **Nada de esto necesita hierro**, así que no interrumpe nada.

> ## `proven 159 · not proven 1 · failed 0` — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> **Cero fallos, y por primera vez con un modelo de verdad.** La corrida de Cesar
> cierra todo lo que este día abrió:
>
> - la terminal usable está en **9 de 9**;
> - el kernel construye con las ocho opciones de red nuevas y `red` coincide con
>   `iproute2` en su máquina;
> - las cuatro afirmaciones que sólo un modelo real puede contestar —que este
>   build de llama.cpp acepta las banderas que Thalyx le pasa, que una inferencia
>   real vuelve como algo que el parser acepta, y que `--grammar-file` restringe
>   en vez de ser ignorada— **quedaron probadas**, después de meses reportándose
>   como `NOT PROVEN`.
>
> Lo único que queda es **la única cosa que no es una comprobación sino una
> medición**: `thalyx agent bench`, que no corre sola porque tarda minutos.
>
> ```
> sudo THALYX_AGENT_BENCH=1 \
>      THALYX_AGENT_BINARY=/home/cesarmanzocode/src/llama.cpp/build/bin/llama-completion \
>      THALYX_AGENT_WEIGHTS=/home/cesarmanzocode/models/qwen2.5-3b-instruct-q4_k_m.gguf \
>      ./dev/verify.sh
> ```
>
> Eso da **la primera tabla de acierto por gama que ha existido** y es la entrada
> a la pregunta de abstención cero de [[Gamas-de-Modelo]], que lleva meses parada
> por no tener con qué medirla.
>
> **Falta que Cesar decida si se corre ahora.** Nada más está bloqueado.

> ## Corregido: una interfaz abajo no tiene una sola respuesta — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> La corrida de Cesar trajo `proven 153 · not proven 1 · failed 1`, y el que
> falló fue **una prueba mía**, no Thalyx.
>
> Antes de eso, lo que su corrida sí probó, y es lo que importaba:
>
> - **el kernel construyó con las ocho opciones nuevas.** `config-check` no tiró
>   ninguna, así que las dependencias de `NETDEVICES`, `ETHERNET` y los cuatro
>   drivers estaban bien;
> - **la etapa 35 pasó**: `red` y `iproute2` nombran las mismas interfaces en su
>   máquina, leídas por sysfs y por netlink;
> - **la etapa 34 pasó**: los ensayos hablan en condicional y el verbo de verdad
>   no.
>
> Lo que falló: la prueba afirmaba que **una interfaz abajo se niega a contestar
> si tiene cable**. Aquí es cierto —`ifb0` da `EINVAL`—; en su Fedora un puente
> de Docker abajo contesta `0` con toda honestidad. **Negarse o contestar es del
> driver, no de estar abajo.**
>
> El módulo nunca necesitó eso. Necesita que una lectura fallida jamás se reporte
> como cable ausente, y eso es cierto en toda máquina. La prueba ahora lee el
> mismo archivo por su cuenta y compara el mapeo; la negativa de verdad, que no
> toda máquina tiene, sale como `NOT PROVEN` con su propia variable.
>
> **Es la misma regla que la del `ENXIO` contra `EACCES` y es la segunda vez en
> dos días.** Lo nuevo es qué la produjo: la primera fue una opción de montaje,
> ésta fue contar dos ejemplos de la misma clase en la misma máquina y llamarlo
> la regla. Está en [[Estrategia-de-Pruebas]].
>
> **Lo único que falta para cerrar el modelo** es lo que su propia corrida ya le
> dijo, palabra por palabra:
>
> ```
> git pull
> sudo THALYX_AGENT_BINARY=/home/cesarmanzocode/src/llama.cpp/build/bin/llama-completion \
>      THALYX_AGENT_WEIGHTS=/home/cesarmanzocode/models/qwen2.5-3b-instruct-q4_k_m.gguf \
>      ./dev/verify.sh
> ```

> ## Punto 8: la red se ve y no se usa — la terminal usable está en 9 de 9 — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Cesar lo decidió así: **verla, no usarla.** El decreto entero está en [[Red]],
> con la razón de por qué no es lo mismo un poco más de lo otro — DHCP, DNS y TLS
> son programas aparte en todos lados y aquí tendrían que vivir dentro de
> `thalyx`, y lo que comprarían depende de una pregunta de Fase 2 que no está
> contestada: de dónde saldría un módulo.
>
> El kernel pasó de **110 opciones a 118**. Las nuevas son dos menús y cuatro
> drivers, cada uno con su razón al lado: `virtio_net`, `e1000`, `e1000e` y
> `r8169`. Nada de WiFi.
>
> El verbo es `red`, motor en `thalyx-net`, dos caras. Y **dice en la respuesta
> que no se puede usar** —`addressable: false` para un programa, una frase para
> una persona— porque es la única lista del sistema cuyas cosas ningún verbo
> puede tocar, y quien lea una lista de tarjetas va a ir a buscar el verbo que
> las usa.
>
> **Lo que salió de correrlo, que ninguna prueba de fixture vio:** la primera
> versión reportó **tres tarjetas en una máquina con una.** `ifb0` e `ifb1` dicen
> `type 1` y traen dirección física, y son software puro. Lo que separa una
> tarjeta es que cuelga de un bus. Regla nueva en [[Estrategia-de-Pruebas]].
>
> Las otras dos, medidas y no citadas: una interfaz abajo **no dice que no tiene
> cable, no dice nada** (`EINVAL`, no `0`), y `speed` tiene tres estados —número,
> `-1` con el enlace arriba, y no legible—. Las dos sobreviven a lo que se
> imprime: `cable unknown` es una columna distinta de `no cable`.
>
> 18 pruebas nuevas y la **etapa 35**, cuyo control es `iproute2` porque lee
> netlink y no sysfs: pedirle a Thalyx que se compruebe contra su propia lectura
> de `/sys` sólo probaría que es consistente. 1340 pruebas, clippy limpio.
>
> **Lo que falta correr, y sólo tu máquina puede:**
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> make -C image kernel        # los ocho CONFIG_ nuevos
> sudo THALYX_AGENT_WEIGHTS=/home/cesarmanzocode/models/qwen2.5-3b-instruct-q4_k_m.gguf \
>      ./dev/verify.sh
> ```
>
> `make -C image kernel` es lo primero porque **`config-check` falla la
> construcción si `olddefconfig` tira cualquiera de las ocho opciones nuevas**, y
> las nombra. Es la única manera de saber si acerté las dependencias: aquí no se
> puede compilar un kernel.
>
> Y en tu Fedora, `red` a secas ya enseña tu tarjeta real — con su driver y su
> velocidad negociada— sin necesidad de arrancar la imagen.

> ## Un ensayo ya no dice que borró nada — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Cesar lo decidió: se arregla. `ensayo rm notas.txt` imprimía
> `removed /ruta/notas.txt` para un archivo que seguía ahí, y lo mismo `cp`, `mv`
> y `mkdir`. Ahora dicen `would remove`, `would copy`, `would move` y
> `would make the directory` — y el verbo de verdad sigue diciendo `removed`,
> que es la mitad que hace que esto signifique algo.
>
> **Una sola frase, dos tiempos.** El tiempo verbal viaja como un dato desde
> quien sabe si esto es un ensayo hasta quien imprime, y `Did::would()` vive
> pegado a `Did::word()` para que no se pueda agregar un verbo nuevo con la
> mitad. Un segundo impresor para los ensayos sería exactamente la segunda
> versión de los hechos que este módulo existe para no tener.
>
> **Por qué nadie lo vio en meses:** la cara de máquina estaba bien todo el
> tiempo — su `op` dice `rehearse`— así que las cuatro pruebas del ensayo, que
> leen objetos, no podían verlo. Regla nueva en [[Estrategia-de-Pruebas]]: cuando
> un hecho se dice en dos caras, una prueba que sólo lee una de ellas prueba una
> de ellas.
>
> Prueba nueva y **etapa 34**, las dos comprobadas de las dos maneras: fallan con
> el defecto puesto de vuelta y pasan sin él. 1322 pruebas, clippy limpio.
>
> **Lo que sigue es el punto 8, la red**, que Cesar también decidió. Es el último
> de los nueve de la terminal usable.

> ## La imagen está construida, y el modelo sí corrió — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Cesar construyó la imagen en su máquina. La corrida pasó de `139 · 2 · 0` a
> **`152 · 1 · 0`**: son las trece comprobaciones de la etapa 16, que arranca la
> imagen en QEMU y le habla, y que llevaban meses fuera del reporte por no haber
> kernel construido. **Con eso el punto 8 —la red— queda desbloqueado**, porque se
> prueba con `make -C image run-hardware`, que necesita justamente esa imagen.
>
> El NOT PROVEN que quedó **no era suyo, era del instrumento.** Corrió
> `thalyx agent model check` y el modelo contestó de verdad —una inferencia
> parseada, 7.28 s, 4.77 GB de pico— y la etapa siguió diciendo *«no real model
> has run: llama-completion is not installed»*. Las dos cosas ciertas a la vez:
> el `check` lo corrió él con su `PATH`, y la etapa corre bajo `sudo`, que tira el
> `PATH` y usa `secure_path`. Regla nueva en [[Estrategia-de-Pruebas]], la quince.
>
> Arreglado: la etapa busca el binario también en el `PATH` de `$SUDO_USER` y,
> cuando lo encuentra, **dice dónde está y qué escribir** para que la corrida lo
> vea. Las dos mitades del hueco se reportan por separado, y la de los pesos
> distingue «no la nombraste» de «nombraste un archivo que no está» — y en el
> primer caso dice que `sudo` no lleva el entorno.
>
> Comprobado corriendo los cuatro caminos, no leyéndolos. El segundo destapó un
> defecto del arreglo mismo: una cuenta con `nologin` imprime una frase en inglés
> y la primera versión la ofreció como si fuera la ruta del binario.
>
> **Lo que falta correr:**
>
> ```
> git pull
> sudo THALYX_AGENT_WEIGHTS=/home/cesarmanzocode/models/qwen2.5-3b-instruct-q4_k_m.gguf \
>      ./dev/verify.sh
> ```
>
> Si aun así dice NOT PROVEN, ahora la línea trae la ruta exacta que falta poner
> en `THALYX_AGENT_BINARY`.

> ## Punto 9: hay citado y no hay lenguaje — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Cesar lo decidió así: *«lo que sea más fácil de cubrir por ahora, pero en un
> futuro sí tendremos que hacer shell completo, no ahora, pero estemos
> preparados»*. La segunda mitad es la que mandó sobre el diseño — **nada de lo
> que se aprenda hoy puede tener que desaprenderse el día del shell completo** —
> y el decreto entero está en [[Palabras]].
>
> Antes de preguntarle fui a ver qué faltaba de verdad, corriéndolo: **un archivo
> con un espacio en el nombre se podía listar y nada más.** `cp mi archivo.txt x`
> eran tres palabras y los tres verbos se negaban. Nunca se destruyó nada por
> eso, pero no había forma de nombrar el archivo.
>
> Lo que hay ahora:
>
> - `'…'`, `"…"` y `\x` con las reglas de POSIX hasta donde POSIX llega hoy, así
>   que el día que `$` signifique algo no cambia nada de lo escrito;
> - una comilla sin cerrar **se niega** —`unclosed_quote` / `close_the_quote`— en
>   vez de adivinarse, que es como un `rm` acaba actuando sobre algo que nadie
>   nombró. Una diagonal al final tiene su propia palabra, porque es otro error;
> - **la expansión se queda en el verbo, y eso es decreto**. `rm "*.log"` borra el
>   archivo que se llama así y `encontrar "*.rs"` sigue siendo un patrón — las
>   dos costumbres de Unix, cada una donde estaba, igual que bash y `find`.
>
> Una palabra recuerda qué caracteres venían citados **carácter por carácter**,
> porque `"a"*` es un patrón y `a"*"` es un nombre.
>
> **Dos cosas cambiaron de significado y hay que decirlas:** una corrida de
> espacios ahora se colapsa (`contenido fn  main` busca `fn main`; la forma de
> pedir lo otro es `contenido "fn  main"`), y el texto de `editar` es la única
> excepción — se toma del renglón byte por byte, porque una sangría perdida en un
> archivo de configuración no se ve hasta que algo no arranca.
>
> Ocho pruebas en el prompt de verdad y la **etapa 33**, comprobada de las dos
> maneras: pasa con el cambio y falla sin él.
>
> **Con esto la terminal usable llega a 8 de 9.** Queda el punto 8, la red, que
> sólo se verifica en su hierro y se cruza con la Fase 2.
>
> **Y una cosa encontrada de paso, que no toqué:** `ensayo rm x` imprime
> `removed /ruta/x` sin haber borrado nada. Ya estaba en `main` desde antes —lo
> comprobé con el binario anterior— y es de la misma familia que lo de `matar`:
> una respuesta que dice que algo pasó cuando no pasó. La cara de máquina está
> bien (el `op` es `rehearse`); la humana no. **Falta que Cesar decida si se
> arregla.**

> ## Comprobado en el hierro: 138 · 2 · 0 — 2026-08-23
>
> Cómo se llegó.
>
> La corrida de Cesar cerró el lazo de verificación de los tres arreglos de este
> día: `proven 138 · not proven 2 · failed 0`. Los dos NOT PROVEN son los de
> siempre y no son defectos — no hay modelo instalado y no hay imagen construida
> (`make -C image`).
>
> Lo que eso deja probado **en hierro**, que es lo único que cuenta para estas
> tres cosas:
>
> - `matar` se niega ante un hilo del kernel y ante un proceso que ya terminó, en
>   vez de decir que los detuvo (etapa 32, con la línea base y el control);
> - **instalar dos veces sobre el mismo disco funciona** — que era el `failed 1`
>   de la corrida anterior. Sostener las particiones abiertas sí impide el segundo
>   barrido del kernel; era lo único de ese arreglo que este contenedor no podía
>   contestar;
> - la prueba del nodo ya no afirma un errno, así que la suite pasa en una máquina
>   que monta `/tmp` con `nodev`.
>
> **La terminal usable llega a 7 de 9 y los dos que faltan son decisiones suyas:**
> el punto 8 (red) sólo se verifica en su hierro y se cruza con la Fase 2, y el
> punto 9 (lenguaje de shell) es decreto antes que código. Nada más está
> bloqueado.

> ## Instalar dos veces sobre el mismo disco — 2026-08-23
>
> Cómo se llegó.
>
> **Corregido el mismo día:** la prueba nueva afirmaba que abrir el nodo da
> `ENXIO`, y en tu Fedora dio `EACCES` — porque `/tmp` está montado con `nodev` y
> ahí no se puede abrir ningún nodo de dispositivo, haya algo detrás o no. La
> prueba tumbó la suite entera por una opción de montaje. Ahora afirma lo que se
> estaba probando: que el nombre resuelve y el dispositivo no. Regla en
> [[Estrategia-de-Pruebas]], y es la catorceava vez que el instrumento miente.
>
> La corrida de Cesar del 2026-08-23 trajo `proven 137 · not proven 2 · failed 1`.
> El que falló: **instalar dos veces no funcionaba**, con
> `opening /dev/loop0p1: No such device or address`.
>
> `instalar-en` escribe la tabla, le pide al kernel que la relea, y espera a que
> aparezcan las particiones antes de escribir dentro de ellas. La espera
> preguntaba **si existía el nodo**. La primera instalación no tiene nodos, así
> que la espera espera; la segunda los tiene de la tabla anterior, así que la
> condición ya estaba cumplida antes de empezar y la espera terminó sin haber
> esperado.
>
> Y hay algo que esperar de verdad: **cerrar el descriptor que tenía el disco
> entero abierto para escritura hace que el kernel lo reexamine por su cuenta**,
> en su propio tiempo. Ese segundo barrido borra cada partición y la vuelve a
> hacer, y un nodo abierto dentro de esa ventana da `ENXIO` — *No such device or
> address*, para un nombre que está ahí en `/dev`.
>
> Dos cambios, y el segundo es el que cierra la ventana en vez de esquivarla:
>
> 1. la espera **abre** la partición en vez de preguntar si el nombre existe;
> 2. las particiones se devuelven **ya abiertas** y se sostienen abiertas hasta el
>    final. El kernel se niega a soltar las particiones de un disco mientras
>    alguna esté abierta, así que el segundo barrido encuentra el disco ocupado y
>    lo deja en paz.
>
> **Lo que está probado y lo que no.** Que `stat` y abrir contestan cosas distintas
> quedó fijado con una prueba que hace `mknod` con un major que ningún driver
> registró: el nombre existe y abrirlo da `ENXIO`, el mismo errno de tu corrida.
> Que montar funciona con un descriptor abierto encima también se comprobó
> corriéndolo. **Que sostener la partición bloquea el segundo barrido no se puede
> comprobar en este contenedor**, que no sabe hacer particiones sobre un loop. Eso
> lo dice tu máquina:
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> sudo ./dev/verify.sh
> ```
>
> La regla que lo habría atrapado está en [[Estrategia-de-Pruebas]]: **una
> condición de espera tiene que ser falsa al principio.** Si la puede satisfacer lo
> que quedó de la vez pasada, no está esperando a nada — y el caso que la deja
> pre-satisfecha es justamente el que nadie prueba, porque es el segundo.
>
> ## `matar` ya no dice que detuvo lo que no se puede detener — 2026-08-23
>
> Lo encontró Cesar en su primera sesión con el punto 7: ensayó `matar` sobre un
> `kworker` y Thalyx contestó *«5 would ask to stop»*. A un hilo del kernel no le
> llega ninguna señal. Que `pidfd_send_signal` conteste `0` significa que el
> kernel se quedó con la señal, **no que le vaya a pasar algo a alguien**.
>
> Son dos los sujetos que la aceptan y la tiran, y los dos quedan negados antes
> de mandar nada:
>
> | sujeto | palabra | remedio |
> |---|---|---|
> | un hilo del kernel | `is_kernel_thread` | `cannot` |
> | un proceso que ya terminó (zombi) | `already_ended` | `stop_the_parent`, con el número del padre |
>
> Es peor que un error: una respuesta que dice *«se le pidió que pare»* sobre algo
> que nunca se movió enseña que Thalyx no es confiable, cuando Thalyx sólo era
> crédulo — y quien la vea va a probar `forzar`, que hace exactamente lo mismo.
>
> `ensayo matar` se niega igual, desde la misma función: un ensayo que predice
> algo que el verbo no hace es una respuesta equivocada que hay que desaprender
> tecleando la de verdad.
>
> El hilo del kernel se reconoce por el bit `PF_KTHREAD` del campo 9 de
> `/proc/<pid>/stat`, **medido y no citado**: ese valor no está en ningún
> encabezado que se le entregue al espacio de usuario, así que se sacó
> comparando los 66 hilos cuyo padre es `kthreadd` contra los 6 procesos
> ordinarios de un sistema corriendo. No se reconoce por la línea de comandos
> vacía, que también la tiene un zombi.
>
> Lo que esto enseñó de las pruebas: **`matar` se había probado once veces en el
> prompt de verdad y las once con un proceso que sí se podía detener.** La regla
> nueva está en [[Estrategia-de-Pruebas]] — cuando el éxito de un verbo se toma
> del valor de retorno de una llamada al sistema, hay que probarlo sobre un
> sujeto que la llamada acepta y no obedece. Y la línea base de la etapa 32 es el
> defecto mismo: `kill -9` al zombi, que se acepta y no hace nada.
>
> Falta correrlo en tu máquina:
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> sudo ./dev/verify.sh
> ```
>
> Revisión completa en [[Procesos]].
>
> ## Procesos: el punto 7, y una señal que no puede caer en el número equivocado — 2026-08-23
>
> `procesos`, `memoria` y `matar` — todo sobre `/proc`, decreto entero en
> [[Procesos]]. Con esto la terminal usable llega al punto 7 de 9; quedan la red
> (punto 8, sólo hierro) y el lenguaje de shell (punto 9, decreto antes que
> código y decisión tuya).
>
> ### Lo que hace a `matar` distinto de un `kill`
>
> **El número no es el proceso.** Entre leer `/proc/4711` y señalar 4711, ese
> proceso puede terminar y el kernel puede darle el número a otro; toda
> herramienta que recibe un pid tiene ese hueco y vive con él. `matar` abre un
> `pidfd` y manda la señal por ahí, así que llega al proceso para el que se abrió
> el descriptor o falla — no hay un tercero donde le llegue a un desconocido.
>
> Eso decide el orden y es el contrario del obvio: **primero el descriptor,
> después la descripción, después la señal.**
>
> Por omisión `TERM`, que un programa puede atrapar para guardar lo que tenía;
> `matar <numero> forzar` manda `KILL`, que no. Se niegan PID 1 y la propia
> sesión, cada uno nombrando el verbo que hace ese trabajo bien —`apagar` y
> `salir`—; no es una política sobre quién manda en la máquina, que eres tú, es
> que una señal hace esos dos trabajos de la única forma que no deja dicho por
> qué la máquina se detuvo. También se niegan el `0` y los negativos, que para
> `kill(2)` son *todo lo que alcance* y un *grupo*.
>
> ### `ensayo matar` es el ensayo que más importa
>
> Un archivo se puede volver a escribir; un proceso no, y la entrada que causa el
> error son cuatro dígitos. Así que contesta el nombre, la línea de comandos
> entera, cuánto lleva corriendo y quién lo arrancó — y que no manda nada sólo
> queda probado por una aserción: el proceso sigue vivo después.
>
> ### `libre` no es `disponible`
>
> `memoria` contesta las dos y nombra cuál contesta la pregunta. Un Linux sano
> mantiene `free` cerca de cero a propósito, y quien lee sólo ese número concluye
> que la máquina está llena y empieza a matar cosas. `en uso` se calcula como
> `total − disponible`, nunca como `total − libre`.
>
> ### Dos cosas que construirlo enseñó
>
> - **El nombre en `/proc/<pid>/stat` puede llevar paréntesis.** Se capturó un
>   renglón real de un proceso llamado `we (ird) x`; partirlo por espacios pone
>   el estado cinco campos antes y reporta un padre inventado. Regla 6.
> - **`kill -0` contesta si el número existe, no si el proceso corre** —
>   decimotercera vez que el arnés miente. La etapa 31 dijo que `forzar` no había
>   funcionado sobre una shell que llevaba rato muerta: era un zombi, porque en
>   este contenedor **PID 1 es `process_api` y no cosecha huérfanos**. En tu
>   Fedora systemd los cosecha y las dos preguntas se ven iguales. Ambas en
>   [[Estrategia-de-Pruebas]].
>
> ### Y un hueco de A2 que apareció por segunda vez
>
> `machine::declined` no tiene dónde poner un remedio, así que los verbos que la
> usaban contestaban `remedy: null`. La primera vez lo parché en `search`; al
> necesitarlo un segundo crate se volvió forma: `machine::refused(op, palabra,
> remedio, mensaje)`.
>
> ### Qué correr
>
> Nada de esto necesita hierro. La etapa 31 corrió verde aquí.
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> thalyx session
> procesos
> memoria
> ensayo matar <numero>
> ```
>
> El `ensayo` primero, siempre. Son 39 verbos ahora.

> ## Encontrar y contenido: el punto 6, con tres preguntas separadas — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Thalyx podía listar una carpeta y leer un archivo, y no podía contestar *dónde
> está el archivo que se llama así* ni *qué archivos dicen esto*. Ya contesta las
> dos, y el decreto entero está en [[Busqueda]].
>
> ### Las dos decisiones fueron de Cesar
>
> **Dos verbos nuevos, no uno con banderas.** `encontrar <patrón>` por nombre,
> `contenido <texto>` por texto, y `buscar` intacto en su tercera pregunta —del
> índice, no del disco—. Un verbo cuyo significado depende de una bandera se
> puede pedir mal en silencio, y las tres respuestas se ven igual: una lista de
> `ruta:línea` no dice de dónde salió.
>
> **Texto literal, sin expresiones regulares.** Un punto es un punto. Dos
> razones: la imagen lleva el kernel y un programa, y un dialecto de regex aquí
> sería decidir a escondidas un pedazo del punto 9 —si Thalyx tiene lenguaje de
> shell—, que es decreto antes que código. Los nombres sí llevan `*` y `?`,
> porque es el vocabulario que `rm`, `cp` y `mv` ya usan.
>
> ### Lo que construirlo obligó a mover
>
> `walk` vivía en `thalyx-graph` con dos llamadores. Ahora son cuatro, y los dos
> nuevos son justo los que una persona compara contra el índice: un `contenido`
> que entrara a `.git` donde `buscar` no entra contestaría sobre un archivo del
> que el índice nunca supo, y la conclusión sería *el índice está roto*. Se movió
> a `thalyx-files`, con el techo de 20 000 archivos, para que sigan siendo una
> sola caminata y un solo número.
>
> ### Dos cosas nuevas en [[Estrategia-de-Pruebas]]
>
> - **Un hecho que la shell va a leer se cita** — duodécima vez que el
>   instrumento miente, y esta vez el instrumento era mío. La etapa 30 escribía
>   `names=a.rs b.rs` sin comillas, la shell asignó el primero y trató de
>   ejecutar el segundo, y la etapa acusó al verbo de no contestar cuando había
>   contestado las tres cosas bien.
> - **Una prueba que este usuario no puede hacer fallar tampoco prueba** — quitar
>   todos los permisos no detiene a root, así que la prueba de la regla 10 dice
>   `NOT PROVEN` y `THALYX_REQUIRE_UNREADABLE_TESTS=1` la convierte en falla. Es
>   la regla 3 con una cara nueva: hasta ahora los saltos eran por lo que a la
>   máquina le falta, y éste es por quién está corriendo.
>
> ### Un defecto que sólo salió al correrlo
>
> El refuso estructurado usaba `machine::declined`, que no tiene dónde poner un
> remedio, así que `not_a_directory` llegaba con `remedy: null` — el punto A2 de
> [[Superficie-para-el-LLM]] perdido calladamente en dos verbos. Lo encontró una
> prueba que pidió el remedio. Ahora usa `machine::failure`, que sí lo lleva.
>
> ### Qué correr
>
> Nada de esto necesita hierro; la etapa 30 corre entera en el contenedor y ya
> corrió. Cuando quieras verlo:
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> thalyx session
> encontrar *.rs
> contenido fn main
> ```
>
> Y `sudo ./dev/verify.sh 2>&1 | tee /tmp/verify.log` cuando toque la próxima
> corrida completa — con `tee`, porque el conteo de la corrida pasada costó una
> ida y vuelta por haber pedido nada más la cola.

> ## Verde en hierro, y las diez comprobaciones que faltaban eran el arranque — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Cesar volvió a correr `verify.sh` con la precondición arreglada:
> **134 probadas, 2 no probadas, 0 fallas.** La sexta corrida en hierro, y la
> primera sin ninguna falla.
>
> ### La rama con watcher se ejerció por primera vez
>
> La etapa 27 imprimió *«the mutation ring was mapped and read: 4 record(s)
> named thalyx-ringmark, and a second read had none of them left»*. Es la etapa
> completa: el anillo mapeado, los registros leídos, y la segunda lectura vacía
> —que es la mitad que prueba que leer consume—. Y la suite pasó, o sea que
> `what_the_kernel_saw.rs` corrió su rama de watcher cargado sin haberse podido
> ejecutar nunca aquí.
>
> ### Las diez comprobaciones que faltaban, contestadas por el reporte mismo
>
> El bloque anterior dejó abierto por qué esta corrida traía menos
> comprobaciones que la del 2026-08-10, y pedía el resumen completo antes de
> afirmar nada. Con el resumen, la respuesta está en la segunda línea de lo que
> la corrida no pudo establecer:
>
> ```
>   · no kernel or image built yet, so there is nothing to boot; run 'make -C image'
> ```
>
> **Lo que faltó es la etapa 16 entera —arrancar la máquina en QEMU—**, que en
> esa máquina no tenía qué arrancar porque `image/build/` está vacío;
> `image/build/` no está en el repositorio, así que se pierde con cualquier
> limpieza y `git pull` no lo trae de vuelta.
>
> Y cuadra por conteo, que es la forma comprobable de decirlo: la etapa 16 tiene
> **trece comprobaciones**, y aquí se contrajeron a **un solo NOT PROVEN**.
> 136 − 1 + 13 = 148, menos la etapa 29 que el 2026-08-10 no existía = 147 …
> contra las **146** que contó aquella corrida. Una de diferencia, que es el
> tamaño de un `if`/`else` de esa etapa. La dirección no admite otra lectura: no
> se perdió nada, no se arrancó nada.
>
> Para recuperarlas, `make -C image` y luego el disco de store. **No es urgente**
> — no hay nada nuevo que sólo el arranque compruebe desde el 2026-08-07 — pero
> mientras `image/build/` esté vacío el reporte va a seguir diciendo 134.
>
> ### Regla nueva
>
> Un conteo que baja no es una regresión hasta que se sabe *qué* dejó de
> correr, y el reporte ya lo decía: **las líneas de NOT PROVEN son parte del
> resultado, no una nota al pie.** Leer sólo el marcador es leer la mitad. En
> [[Estrategia-de-Pruebas]].

> ## La corrida en hierro: el anillo funciona, y la prueba era la equivocada — 2026-08-23
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Cesar corrió `verify.sh`: **133 probadas, 2 no probadas, 1 falla.** La falla era
> la etapa 5 —la suite— por dos pruebas de `what_the_kernel_saw.rs`.
>
> ### Thalyx estaba bien y la prueba estaba mal
>
> Las dos pruebas suponían **una máquina sin watcher**. Lo decía el encabezado del
> archivo y lo decía el nombre de una de ellas, y ninguna de las dos lo
> comprobaba. Era cierto durante meses porque este contenedor no puede cargar BPF,
> así que la negativa era la única respuesta posible.
>
> En la máquina de Cesar el watcher **sí** estaba cargado — porque las
> instrucciones de esa corrida le decían `make -C lsm load` antes de correr. Así
> que `cambios` contestó correctamente y la prueba dijo que Thalyx se equivocaba.
>
> **Undécima instancia de la regla 5**, y la variante de «una prueba que infiere
> su propia precondición». Arreglado: las dos preguntan si el pin existe en el
> sistema de archivos —un hecho en el que `cambios` no participa— y **ninguna de
> las dos ramas es un salto**, así que el archivo prueba algo dondequiera que
> corra. Ver [[Estrategia-de-Pruebas]].
>
> ### Lo bueno, y estaba en la misma salida
>
> La corrida imprimió esto:
>
> ```
>     created    by thalyx (41513), in cgroup 22518
>     retitled   by thalyx (41510), in cgroup 22518
> ```
>
> Son **registros reales drenados de un anillo real del kernel**, con su cgroup,
> desde una sesión. El consumidor del ringbuf funciona en hierro. Lo que la
> etapa 27 persigue está vivo.
>
> ### Lo que falta saber de esa corrida
>
> **133 probadas contra las 143 del 2026-08-10, y se agregó una etapa.** Diez
> comprobaciones menos no se explican con la falla de la etapa 5, y con la cola
> del reporte no alcanza para decir por qué. Hace falta el resumen completo —
> las dos líneas de `not proven` y el bloque final— antes de afirmar nada.

> ## El editor existe, con sus dos caras — 2026-08-22
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Es el punto 5 de la terminal usable, y lo que lo justificaba desde el
> principio: **sin un editor no se puede corregir un archivo de configuración
> desde la máquina.** Thalyx podía hacer un archivo, copiarlo, moverlo, borrarlo
> e imprimirlo, y no podía cambiarle un byte por dentro.
>
> Cesar decidió **las dos caras en una entrega**, que era la opción cara de las
> tres. El decreto entero está en [[Editor-de-Texto]]; lo corto:
>
> - `editar <archivo>` abre una **pantalla** — flechas, `Ctrl-O` guarda, `Ctrl-X`
>   sale, `Ctrl-U` deshace, `Ctrl-K` corta.
> - `editar <archivo> cambiar 12 <texto>` direcciona **renglones** y contesta un
>   objeto, porque un programa no puede manejar una pantalla que se redibuja.
> - `crates/thalyx-edit` es **un solo motor** y las dos caras lo llaman, así que
>   no pueden acabar en desacuerdo sobre lo que el archivo dice ahora.
>
> Es el primer verbo donde las dos caras difieren de **forma**, y por eso valía
> la pena decidirlo en la bóveda antes de escribirlo.
>
> ### Lo que se puede correr aquí, y ya corrió
>
> **1 231 pruebas en verde, clippy limpio.** La etapa **29** de `verify.sh` está
> escrita y **ya pasó en este contenedor**: no necesita hierro. Ejerce las tres
> cosas que sólo una máquina contesta, cada una con su control —
>
> - un renglón cambiado por dirección y **leído de vuelta con `cat`**, no con
>   Thalyx;
> - una persona tecleando en la pantalla **a través de un pty de verdad**, con
>   pulsaciones reales, y el trabajo escrito;
> - un archivo binario **negado sin que se moviera un byte**, que es el control:
>   un editor que negara todo pasaría las dos primeras y sería inútil.
>
> ### Tres defectos que salieron de correrlo, y uno que ya estaba
>
> Todos están escritos como reglas en [[Estrategia-de-Pruebas]]:
>
> 1. **`Ctrl-S` no habría funcionado nunca.** El modo crudo deja `IXON` e `ISIG`
>    encendidos a propósito, así que la disciplina de línea se come `Ctrl-C`,
>    `Ctrl-Z`, `Ctrl-S` y `Ctrl-Q` antes de que Thalyx vea un byte — y `Ctrl-S` es
>    XOFF, o sea que habría dejado la terminal aparentemente muerta. Por eso
>    guarda `Ctrl-O`. Hay una prueba que falla si alguien enlaza una de las cuatro.
> 2. **La décima instancia de la regla 5.** Las pruebas de pantalla se colgaron:
>    un pty recién hecho no tiene tamaño de ventana, el editor se negó a dibujar
>    —correctamente— y las teclas se tecleraon en el prompt. **El arnés estaba
>    incompleto, no Thalyx.** Ahora `thalyx dev pty` le pone tamaño al pty. Lo
>    hizo barato el reloj de la prueba, que al vencerse imprime lo que se había
>    dibujado: ahí estaba la oración que decía la respuesta entera.
> 3. **La confirmación de salida se tragaba la tecla que la contestaba.**
>    `Ctrl-X` y luego `Ctrl-O` no guardaba nada. Era una lectura anidada haciendo
>    de segundo intérprete del teclado; ahora es una bandera y toda tecla pasa por
>    el único bucle que sabe qué significan.
> 4. **Y una que ya llevaba un día roja en `main`**: la reescritura de la portada
>    del 2026-08-21 movió la ruta de construcción a `docs/BOOT.md` y la prueba
>    que la ata seguía afirmando sobre el README. Arreglada, y ahora comprueba
>    también que la portada apunte ahí — una ruta de construcción en un archivo
>    al que nada apunta es una ruta que nadie encuentra.
>
> ### Lo que sigue sin correrse en hierro
>
> **Nada de lo del 2026-08-10 se ha vuelto a correr en tu máquina**, y hay cuatro
> arreglos de ese día esperando: el techo de `indexar`, la etapa 14 que devuelve
> el watcher, el `ETXTBSY` de `grammar_check` y el control del anillo. La etapa
> 27 —el ring buffer— es la que más falta hace, porque la 14 la estaba tumbando.
>
> ### Qué correr
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> sudo chown -R "$USER" lsm
> make -C lsm unload && make -C lsm load
> sudo ./dev/verify.sh
> ```

> ## La portada del repositorio, rehecha — 2026-08-21
>
> **Esto no cambió el sistema, cambió cómo se lee.** El estado técnico sigue
> siendo el bloque de abajo.
>
> El README tenía 636 líneas y funcionaba como documentación profunda, no como
> portada: quien abría el repositorio tardaba en entender qué era. Ahora son
> ~250, con jerarquía, y el detalle se movió a dos archivos nuevos en inglés que
> no compiten con el vault:
>
> - `docs/BOOT.md` — el recorrido de arranque completo, paso por paso.
> - `docs/STATUS.md` — la contabilidad honesta: qué está probado, dónde, con qué
>   fecha, qué sigue abierto y la contradicción que el proyecto publica.
>
> `README.es.md` dejó de ser una traducción completa (dos copias grandes se
> desincronizan; la que miente es la que nadie lee) y es ahora una página corta
> de orientación que manda al vault, que ya está en español.
>
> ### Evidencia visual, capturada de corridas reales
>
> `docs/media/` lleva tres imágenes y **el texto crudo de cada una al lado**,
> para que se puedan comprobar: el recuadro de autorización de capacidades, el
> commit atómico muerto con `SIGABRT` entre el rename y el symlink, y el
> autorreporte de `session --once`. No hay maquetas. El diagrama de arquitectura
> (`docs/media/architecture.svg`) es dibujado y muestra la frontera de confianza:
> la ruta humana completa, la del agente sólo como propuesta, y el LSM negando
> desde el kernel.
>
> No se pudo capturar un arranque real en QEMU: la política de red del contenedor
> bloquea `cdn.kernel.org`, así que el kernel no se puede descargar aquí.
>
> ### Dos cifras del README estaban viejas
>
> Decía «110 comprobaciones, 2026-08-07» — la corrida vigente es la quinta,
> **2026-08-10: 143 probadas, 2 no probadas, 1 fallida**, con todo el lado del
> kernel probado por primera vez. Y decía que `thalyx_watch` «nunca se ha cargado
> sin `bpftool`», cuando lo que sigue abierto es más preciso: **nunca lo ha
> cargado el cargador propio de Thalyx**; el ring buffer sí se leyó en hierro
> desde un pin real del kernel.
>
> Los conteos que se mueven (pruebas, nombres declarados) quedaron en una forma
> que no envejece: «más de 1 100 pruebas», «cerca de cuatro mil nombres».

> ## El verbo que se colgaba era `indexar`, y el hierro quedó en verde — 2026-08-10
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> La quinta corrida en hierro dio **143 probadas, 2 no probadas, 1 falla**, y por
> primera vez todo el lado del kernel quedó probado: la protección deniega de
> verdad, el módulo corrió confinado, Thalyx enganchó su propio LSM sin
> `bpftool`, y el canal del módulo sobrevivió al sandbox. `make unload` antes de
> `make load` fue todo lo que hacía falta.
>
> ### El reloj sirvió: era `indexar`
>
> La prueba nombró el verbo en la primera corrida. `indexar` sin argumento indexa
> el árbol donde está parada la sesión, y una sesión empieza en `/home` — que en
> tu máquina incluye `.cargo/registry` y `.rustup`. Aquí son 329 archivos y dos
> segundos; allá es cada fuente de cada crate que has bajado más la biblioteca
> estándar entera.
>
> Dos reglas nuevas, ambas en [[Estrategia-de-Pruebas]]:
>
> - **No se entra a carpetas que empiecen con punto.** La lista vieja
>   —`.git`, `target`, `node_modules`— era una lista de las cosas que ya habían
>   salido mal. El árbol que se nombra explícitamente nunca se filtra.
> - **Techo de 20 000 archivos, y arriba de eso se niega en lugar de empezar.**
>   `tree_too_large`, dentro de la transacción, así que el índice que había sigue
>   ahí. Etapa 28 de `dev/verify.sh`, con su control.
>
> ### Lo que sigue abierto
>
> La etapa 27 dijo que nada estaba anclado, y era cierto **por culpa de la etapa
> 14**, trescientas líneas más arriba: desengancha el watcher y no lo devolvía.
> Ya lo devuelve. El ring buffer no se ha vuelto a ejercer desde entonces.
>
> Y `grammar_check` corre el binario dos veces por dentro; los seis sitios que
> pasan por ahí nunca estuvieron envueltos contra `ETXTBSY`. El comentario decía
> que *toda prueba de este archivo* llamaba al envoltorio, y era falso.
>
> ## Tres de las cuatro fallas de la cuarta corrida eran el instrumento — 2026-08-10
>
> La corrida en hierro dio 127 probadas, 7 no probadas y 3 fallas. De las tres,
> dos eran de `dev/verify.sh` y la tercera se quedó sin diagnosticar a propósito.
>
> ### El ring buffer sí funciona
>
> Ésta es la noticia buena y hay que decirla primero: con el mapa renombrado a
> `thalyx_mut_ring`, el pin apareció, `cambios` **mapeó el ring y leyó
> registros reales del kernel**. `bpf_obj_get` sobre un pin de verdad, dos
> `mmap` que el kernel aceptó y el protocolo corriendo sobre memoria compartida
> — nada de eso se puede tocar desde el contenedor.
>
> Lo que falló fue el control, no el consumidor. Ver
> [[Estrategia-de-Pruebas]]: *un control que pide silencio no se puede cumplir
> en una máquina viva*. Ahora la mutación la hace un programa llamado
> `thalyx-ringmark` y las dos columnas son sobre ese nombre.
>
> ### El LSM estaba enganchado y el script dijo que no
>
> Cesar cargó el LSM a mano antes de correr el script. `make load` se negó
> —correctamente— y `verify.sh` leyó esa negativa como «no está enganchado»,
> perdiendo la etapa 4 y cuatro NOT PROVEN más sobre protección que estuvo
> corriendo toda la corrida. Ahora `make unload` va antes de `make load`,
> siempre.
>
> ### La suite se colgó y no se sabe en qué verbo
>
> `every_verb_the_catalogue_advertises_is_understood_at_the_prompt` dejó de
> contestar. Aquí corre en 1.7 segundos, así que **no se pudo reproducir y no se
> arregló** — adivinar cuál de los 33 verbos fue habría sido exactamente lo que
> la regla 5 prohíbe.
>
> Lo que sí se hizo: la prueba lleva ahora su propio reloj de tres minutos y al
> vencerse **nombra el verbo** en el que se quedó. Una corrida de
> `cargo test -p thalyx-cli --test catalogue_is_true` lo dice.
>
> La sospecha, sin prueba: `discos` abre cada disco entero con `File::open` para
> leer su tamaño, y abrir un lector de tarjetas vacío o una unidad óptica sin
> medio es lo clásico que se cuelga. El tamaño se puede leer de
> `/sys/class/block/<n>/size` sin abrir nada. **No se cambió**, porque cambiarlo
> antes de saber habría borrado la evidencia.
>
> ## `intento` alcanzó `/`, y el contenedor no podía verlo — 2026-08-10
>
> ### Lo primero, y hay que correrlo
>
> Dos snapshots de sólo lectura de tu sistema de archivos raíz quedaron en
> `/.thalyx-snapshots/`. Los dejó la corrida de pruebas, no un `intento` que
> hayas escrito, y **nada se destruyó** — esas pruebas nunca abandonan. Pero
> quedaron huérfanos y anclan bloques:
>
> ```
> sudo btrfs subvolume list -o / | grep thalyx-snapshots
> sudo btrfs subvolume delete /.thalyx-snapshots/*
> ```
>
> ### El defecto
>
> `subvolume_at_or_above` caminaba hacia arriba buscando el subvolumen más
> cercano. Desde un directorio temporal bajo `/tmp` la caminata pasó por todos
> los niveles y se detuvo en el primero que sí lo era: **`/`**. La respuesta dijo
> que abandonar borraría 1 343 582 archivos, `/boot` entre ellos.
>
> El argumento para caminar hacia arriba —«un intento es sobre el árbol en el que
> alguien está trabajando»— era exactamente al revés: caminar hacia arriba
> **abandona en silencio** el alcance que quien llama tenía en mente, y en toda
> instalación Btrfs ordinaria la caminata termina en la respuesta más peligrosa
> que existe.
>
> Ahora: **donde estás parado, o nada.** Y `/` se niega aunque estés parado en
> él, porque abandonarlo significa cambiar la raíz del sistema en marcha por
> debajo de cada proceso de la máquina, incluido el que lo pidió.
>
> ### Por qué el contenedor no podía encontrarlo
>
> Aquí no hay Btrfs ni un solo subvolumen, así que *todos* los caminos se niegan
> y la prueba no podía distinguir una negativa correcta de un accidente del
> sistema de archivos. **Pasaba por la razón equivocada**, que es peor que
> faltar: ocupa el lugar donde alguien buscaría la que sí sirve.
>
> La guarda vive ahora en una prueba unitaria contra un falso donde **todo** es
> un subvolumen —la máquina donde la respuesta peligrosa sí aparece— y la etapa
> 26 tiene una columna nueva que se para en `/` y exige la negativa.
>
> ### Y por qué la 27 no llegó a correr
>
> `make -C lsm load` falló con `Operation not permitted` al escribir su propio
> `.o`. `sudo ./dev/verify.sh` compila los objetos BPF dentro del árbol de
> fuentes **como root**, así que el siguiente `make` como tú no puede
> sobrescribirlos. `verify.sh` ahora devuelve lo que escribió.
>
> ### Qué correr
>
> ```
> sudo btrfs subvolume delete /.thalyx-snapshots/*
> git pull && cargo install --path crates/thalyx-cli
> sudo chown -R "$USER" lsm
> make -C lsm unload && make -C lsm load
> sudo ./dev/verify.sh
> ```

> ## La corrida en hierro: 26 pasó, 27 no llegó a correr, y una prueba que se peleaba consigo misma — 2026-08-10
>
> Cesar corrió `verify.sh` en su máquina: **143 comprobadas, 2 no comprobadas,
> 1 fallida**.
>
> ### Lo que quedó probado en hierro
>
> **Etapa 26 — el intento con nombre, sobre Btrfs de verdad.** Abandonar
> devolvió un archivo a su contenido viejo, borró el que se hizo durante el
> intento, y el control cerrado con `confirmar` no perdió ninguno de los dos. El
> intercambio fue **atómico**.
>
> Y las etapas 23, 24 y 25: el listado acotado con su cursor, el índice de
> símbolos encontrando **3 989** nombres sobre las fuentes de este repo, y la
> historia leída desde la sesión.
>
> ### La prueba que fallaba: novena instancia de la regla 5
>
> Dos pruebas de `llama.rs` fallaron con `Text file busy`. **Nada de Thalyx
> estaba mal.** `ETXTBSY` es el kernel negándose a ejecutar un archivo que
> cualquier proceso tiene abierto para escritura, y el conteo que revisa vive en
> el inodo — así que `O_CLOEXEC` no ayuda. El mecanismo es la ventana del
> `fork`: entre que `Command::spawn` bifurca y el hijo ejecuta, el hijo tiene
> copia del descriptor de escritura que otro hilo estaba usando para crear ese
> mismo archivo.
>
> Lo que hace que valga la pena escribirlo: **esto ya había fallado hace un
> año**, una vez en veinticinco corridas, y el arreglo de entonces —un nombre de
> archivo por falso— venía con un comentario que decía que era una adivinanza.
> Sobrevivió un año porque nunca volvió a fallar donde alguien mirara. Lo que la
> resolvió no fue pensar mejor: fue una máquina de doce núcleos.
>
> Arreglado con un reintento en el arnés y **no** en el código de producción: si
> el `llama-completion` de alguien está ocupado, eso es un hecho de su máquina
> que Thalyx debe reportar, no tapar. Y con una prueba que **reproduce la falla a
> propósito** —sostiene el archivo abierto para escritura, comprueba que el
> kernel de verdad se niega, y luego que el arnés lo espera.
>
> ### La etapa 27: el reporte no podía decir lo que pasaba
>
> Dijo «nada está anclado en `…/thalyx_mutations`, así que thalyx-watch no está
> cargado». **Era falso**: la etapa 3 de la misma corrida decía que el contador
> sí estaba anclado y la 7 leyó 36 872 mutaciones con los diez ganchos puestos.
>
> `BPF_OBJ_NAME_LEN` es 16 contando el terminador, así que el kernel se queda
> con **quince** caracteres. `thalyx_mutations` (16) y `thalyx_mutation_count`
> (21) se vuelven **los dos** `thalyx_mutation`, y el kernel tenía dos mapas del
> mismo objeto bajo un solo nombre. Si eso fue lo que impidió el anclaje no está
> establecido — lo que sí lo está es que dos mapas con un nombre vuelven la
> pregunta incontestable.
>
> El anillo se llama ahora `thalyx_mut_ring`, quince caracteres, y hay una prueba
> que corta cada nombre de mapa de los dos objetos BPF a quince y falla si dos
> coinciden. Y la etapa 27 ya no dice sólo «no está»: dice qué **sí** hay en el
> directorio y si el mapa existe en el kernel sin estar anclado.
>
> ### Lo que hay que correr
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> make -C lsm unload && make -C lsm load
> sudo ./dev/verify.sh
> ```
>
> El `unload`/`load` es lo que hace falta para que el anillo se ancle con su
> nombre nuevo. Si la 27 sigue sin pasar, ahora el mensaje dice qué hay ahí.

> ## Seis puntos más del catálogo, y el prompt que Cesar encontró — 2026-08-10
>
> Cesar corrió lo del día anterior en su máquina, encontró un defecto de verdad,
> y sobre los tres puntos que quedaron nombrados como negociación dijo: *«me
> parece que ahí no hay costo real, más bien es costo de dificultad… si realmente
> aportan un beneficio real y claro a los LLMs, entonces hazlos»*.
>
> **Catorce de los diecinueve puntos están hechos.** Queda **E1**, que no es
> difícil sino que le falta el piso, y ésa es la decisión que sigue.
>
> ### El defecto: una pantalla en blanco que parecía colgada
>
> Escribió `structured on`, recibió su objeto, y se quedó frente a una pantalla
> en blanco. Nada en ella distinguía una sesión esperando una línea de una
> colgada, así que abrió otra ventana para escribir el siguiente comando.
>
> El prompt se suprimía en la cara estructurada para cumplir la promesa de un
> objeto por renglón. **En una terminal esa promesa nunca se estaba cumpliendo**:
> el modo crudo hace eco de cada carácter, así que el flujo de un pty nunca fue
> un objeto por renglón — y las pruebas ya lo decían en su propio comentario.
> Suprimir el prompt ahí no compraba nada y costaba eso. Ahora lo decide **el
> flujo y no la cara**: por tubería no hay prompt, en terminal sí, con llaves
> —`  {/home} > `— porque cuál cara está encendida es invisible hasta que uno
> escribe algo, y quien no ve el modo en el que está no puede salir de él.
>
> ### B1: ninguna respuesta larga sin límite
>
> La falla es la callada de los cinco costos. `ls` sobre cuarenta mil archivos no
> falla, no avisa, y no parece un defecto: produce un agente que gastó su ventana
> entera en nombres y olvidó su tarea.
>
> **Un cursor por llave y no un desplazamiento**, y ésa es la decisión entera.
> Con `skip(200)`, borrar un archivo anterior corre todo un lugar y el renglón
> que estaba en 200 no se le manda a nadie — sin error, sin hueco, y quien lee
> concluye que ese archivo no existe. Lo que un cursor por llave sí no puede
> ocultar —que la colección se movió— lo dice, en el mismo objeto que las filas.
>
> La cara humana **no** se acota: una ventana es un hecho sobre una ventana de
> contexto, y una persona no tiene una.
>
> ### C2: símbolos, no renglones
>
> El decreto decía «medio construido: el parser existe», y era menos que eso — el
> parser sabía leer importaciones, no declaraciones. Ahora `buscar <nombre>`
> contesta dónde se declara un nombre, de qué tipo es, y en qué renglón de qué
> archivo se usa. **Sin comentarios y sin cadenas**, que es lo que `grep` no
> puede hacer.
>
> La tabla de menciones sólo registra identificadores que el árbol declara. Sin
> eso la tabla es en su mayoría vocabulario —`println`, `let`, `self`— y un
> índice que es vocabulario es uno que nadie puede permitirse guardar.
>
> La etapa 24 de `verify.sh` corre sobre las fuentes de este repositorio y
> encuentra **3 869 nombres declarados**. Su control es una palabra que sólo
> aparece en prosa: si volviera con usos, esto sería una búsqueda de texto
> disfrazada.
>
> ### F2: `historia`
>
> El journal se escribía desde [[Journal-y-Snapshots]] y lo leía exactamente una
> cosa, un subcomando. Ahora lo lee la sesión, el más nuevo primero, con la
> advertencia que la cara humana ya imprimía convertida en campo: **esto es lo
> que Thalyx hizo, no todo lo que pasó**.
>
> ### D2: `intento`, y una corrección mía
>
> El 2026-08-09 escribí que D2 sólo se podía construir en el hierro porque la
> única prueba posible aquí sería contra un falso. **Eso confundió dos cosas.**
> `thalyx-snapshot` ya tenía el corte hecho y escrito: *«la política que sólo se
> puede ejercer en un sistema de archivos Btrfs es política que nunca se
> ejerce»*. Cuál intento está abierto, qué hace un segundo, a qué árbol apunta un
> abandono, qué pasa cuando el snapshot ya no está — ninguna es una pregunta de
> Btrfs.
>
> Así que la política vive en `thalyx-core::attempt` con once pruebas que corren
> aquí, y para el hierro queda sólo lo que de verdad es Btrfs: que el snapshot
> sea atómico y que el intercambio sea un `RENAME_EXCHANGE`. **Etapa 26**, con su
> control — la misma secuencia cerrada con `confirmar`, donde nada debe perderse.
>
> ### B3: `cambios`, y otra corrección mía
>
> Escribí que consumir el ringbuf era código BPF y que por eso tumbaría al
> watcher. **No aplica**: el productor ya está escrito y no se toca; el
> consumidor es código de usuario. El protocolo del anillo es una función pura
> sobre bytes, y un arreglo de bytes lo modela exactamente — regla 8 cumplida,
> diez pruebas aquí, incluido el registro que cruza el final del anillo. Para el
> hierro queda **sólo el mapeo**: etapa 27.
>
> Dos cosas que el decreto esperaba y un anillo no puede dar, dichas en la
> respuesta: **no es una historia** (leerlo lo vacía) y **no nombra archivos**
> (trae cgroup, pid, tipo y programa). Para lo que sirve es para separar lo que
> hizo el agente de lo que hizo la persona.
>
> ### Lo que hay que correr, en este orden
>
> ```
> git pull && cargo install --path crates/thalyx-cli && sudo ./dev/verify.sh
> ```
>
> Tres etapas nuevas sólo tuyas: **26** (Btrfs, el intento), **27** (BPF, el
> anillo) y las que ya estaban. Si algo se rompe, las tres son cambios distintos
> en commits distintos, así que se sabe cuál.
>
> ### Lo que queda, y es una decisión tuya
>
> **E1 — el agente ajeno como tarea con concesión.** No es difícil. Le falta el
> piso: G1 (ejecutar procesos) y G2 (un runtime donde un agente ajeno pueda
> correr) no existen, así que no hay a qué darle la concesión. Construirla ahora
> produce código que no se puede ejercer **ni siquiera en tu máquina**, que es
> distinto de D2 y B3. Y la pregunta de fondo es tuya: hoy Thalyx sólo corre
> módulos firmados, y un agente ajeno por definición no lo es.

> ## La máquina se describe a sí misma, ensaya antes de hacer, y el índice ya es alcanzable — 2026-08-09
>
> Cesar pidió cerrar [[Superficie-para-el-LLM]] hasta donde el trade-off fuera
> claro. **Ocho de los diecinueve puntos quedaron hechos**, tres quedaron
> nombrados como negociación —los tres necesitan hierro que este contenedor no
> tiene— y tres más quedaron sin hacer por tiempo y no por riesgo.
>
> ### A1: la máquina se describe a sí misma, y era la llave
>
> `describe` contesta los **29 verbos**: nombres, argumentos, banderas, con qué
> `op` responde cada uno, si puede cambiar la máquina y qué errores da. Un agente
> que llega no necesita que nadie le pegue una lista en el prompt: pregunta.
> Ningún sistema operativo puede hacer esto — en Linux `--help` es prosa, es por
> herramienta, no es consistente y a veces no está.
>
> **Y arregló una duplicación real.** La lista de verbos vivía en **tres** sitios
> —el `match` de la sesión, el banner y las completaciones— y ya había divergido
> una vez. Ahora los nombres viven una vez, las completaciones se **generan**, y
> dos pruebas atan las otras dos copias **corriendo la sesión**:
>
> - cada nombre del catálogo se teclea en un prompt de verdad y tiene que ser
>   entendido;
> - cada nombre del catálogo tiene que aparecer en el banner.
>
> La segunda encontró algo el mismo día: **bajo un programa, `apagar` existía y
> el banner no lo nombraba**, así que quien lo tecleaba recibía una negativa por
> un verbo del que nunca se le habló.
>
> ### D1: ensayar, que es lo que cambia el comportamiento
>
> `ensayo rm *.log` dice qué se iría y no toca nada. Lo importante es **cómo está
> construido**: cada `foresee_*` es *la mitad de comprobación de la operación
> real*, y la operación real la llama. No hay camino donde el ensayo y lo
> ensayado puedan discrepar sobre si algo se permite, porque hay **un solo
> código** decidiendo — un ensayo que dijera «esto funcionaría» mientras la
> operación se niega sería peor que no tener ensayo.
>
> Es prefijo y no modo, a propósito: un modo se queda encendido y entonces un
> `rm` de verdad no hace nada mientras quien lo pidió cree que sí.
>
> Los cinco verbos que cambian la máquina y **no** tienen mitad de comprobación
> —`instalar`, `correr`, `revertir`, `instalar-en`, `apagar`— contestan que no se
> pueden ensayar. Un plan vacío se leería como «esto no haría nada», que es lo
> contrario de la verdad.
>
> ### C1: el índice semántico, alcanzable por fin
>
> `indexar`, `depende <archivo>`, `usan <archivo>`. **La pregunta que ningún
> recorrido de carpetas contesta** —quién se refiere a esto— por primera vez al
> alcance de algo que vive en una sesión. Era el ejemplo que Cesar usó al pedir
> el catálogo, y [[FS-en-Grafo]] se llama a sí mismo el ejemplo fundacional; lo
> había sido durante meses sin que nada fuera de `thalyx graph` pudiera
> preguntarle nada.
>
> Cada respuesta trae **la vigencia del índice en el mismo objeto que las filas**
> — la regla de honestidad de [[FS-en-Grafo]], que es decreto precisamente porque
> separar el aviso de los datos es cómo un caché empieza a confundirse con la
> verdad. Un árbol que cambió por detrás contesta `stale` y **devuelve las filas
> igual**: no están mal, están incompletas, y quien lee decide.
>
> ### Lo demás que quedó hecho
>
> - **A2 — el error nombra su remedio.** `remove_or_rename`, `look_first`,
>   `use_list`, y **`cannot` cuando no hay salida**: inventar un remedio
>   alentador manda a quien lee a un ciclo reintentando algo que nunca va a
>   funcionar.
> - **A3 — `estado` en un objeto**, con los tres estados de la regla 10 sin
>   colapsar: `found`, `absent`, `unreadable`. Quien lee `absent` va a arreglar
>   algo; quien lee `unreadable` sabe que la máquina no contestó, que es otro
>   trabajo.
> - **B2 — cada lectura trae el `sha256` del archivo entero.** «¿Sigue siendo
>   cierto lo que leí?» pasa de ser una relectura a ser una comparación. Del
>   archivo **entero** y no del extracto, porque dos archivos que comparten sus
>   primeros 64 kB darían el mismo hash y quien mira seguiría creyendo que nada
>   cambió. Hay prueba con exactamente ese par.
> - **D3 — cada acción dice cómo se deshace.** Lo hecho se deshace borrando lo
>   que se hizo —**el destino de una copia y nunca el original**, que es un error
>   que costaría el archivo del que se copió— y un `mv` se deshace moviendo de
>   vuelta. Un borrado trae `undo: null`, porque `/home` no lo devuelve ningún
>   rollback nuestro y **eso hay que saberlo antes, no después**.
> - **F1 — `recuerdos` estructurado**, con las tres listas separadas: lo dicho,
>   lo que sigue comprobando, y lo que ya no se puede confirmar. Juntarlas
>   entregaría lo tercero como si fuera lo segundo, que es la única cosa que la
>   memoria está construida para no hacer.
> - **`rm` de una carpeta ya dice cuánto destruyó.** Reportaba `0`, que no le
>   dice nada a una persona sobre lo que acaba de perder ni a un agente sobre lo
>   que está en juego. Y el peso **no sigue enlaces**: seguirlos reportaría el
>   archivo de alguien más como parte de lo que va a desaparecer.
>
> **1075 pruebas** (1040 antes), `clippy` limpio. Etapa **22** en `verify.sh`,
> cada comprobación con su control.
>
> ### Los tres que no construí, y por qué — esto es lo que hay que negociar
>
> Cesar dijo: *«si en alguno perdemos demasiado, entonces dime y lo
> negociamos»*. Aquí se perdería, y los tres se pierden por la misma razón —
> **este contenedor no tiene con qué comprobarlos**:
>
> | Punto | Qué necesita | Por qué no se hizo a ciegas |
> |---|---|---|
> | **D2** el intento con nombre | Btrfs | Es el de mayor valor que queda. Un falso de un snapshot que no falla como falla un snapshot **no es un falso, es otro sistema** — regla 8 |
> | **B3** qué cambió desde X | BPF | Consumir el ringbuf es código BPF, y uno que falla el verificador tumba al watcher entero. Va solo, en su corrida |
> | **E1** el agente como tarea con concesión | LSM y cgroups delegados | Lo único del catálogo que toca **seguridad**: una equivocación no cuesta una prueba roja, cuesta una concesión mal puesta |
>
> Los otros tres que quedan —**B1** acotar respuestas, **C2** búsqueda por
> símbolos, **F2** el journal— **sí se pueden comprobar aquí** y quedaron sin
> hacer por tiempo, no por riesgo.
>
> ## La cara estructurada existe, y encontró dos defectos en la humana — 2026-08-09
>
> Cómo se llegó a lo de arriba.
>
> El punto 4b está hecho: **un programa ya puede pedir los hechos en vez de las
> frases.** `structured on` en la sesión, y de ahí en adelante cada verbo de
> archivos contesta un objeto JSON por renglón; `structured off` devuelve las
> oraciones. Los dos nombres funcionan (`estructurado`).
>
> Hasta hoy el decreto del objetivo estaba **escrito y no construido**: las
> operaciones ya devolvían un `Done` desde ayer, pero lo único que lo leía era el
> impresor humano.
>
> ### Las tres cosas que la hacen otra cara y no otro impresor
>
> Las tres son la regla de desempate del decreto — gana el LLM, y el humano
> conserva acceso completo por otra vía:
>
> 1. **No esconde nada.** `ls` le oculta los archivos con punto a una persona
>    porque tu carpeta tiene treinta y cinco antes del primero que pusiste.
>    Ocultárselos a un programa que preguntó es quitarle capacidad, así que en
>    esta cara están todos. `-a` y `-l` se leen en las dos caras y **se obedecen
>    en una sola**: a un programa no le cambian nada porque nunca se le estaba
>    dando menos.
> 2. **Los tamaños son exactos.** `1.2 kB` es un número que perdió precisión, y
>    dos programas comparando dos números redondeados comparan dos mentiras.
> 3. **El silencio nunca es respuesta.** `cd` no imprime nada para una persona
>    porque el prompt siguiente ya dice dónde quedó. Un parser no tiene prompt de
>    dónde leerlo y **no distingue un silencio que significa «me moví» de uno que
>    significa que la sesión se murió**. Un `cd` que falla contesta además dónde
>    sigue parada la sesión, que es lo que la cara humana ya decía con palabras.
>
> ### El marco, que es el hueco que casi se repite
>
> **Un renglón tecleado, exactamente un objeto.** `rm *.log` que alcanza tres
> archivos podría imprimir tres objetos, y quien lee no tiene cómo saber que debe
> leer tres — el cuarto se leería como respuesta al comando siguiente. Así que un
> verbo que puede tocar varias cosas contesta **un** objeto con `count` y
> `results` adentro, y `ok` es verdadero sólo si todos salieron bien: un éxito
> parcial reportado como éxito es cómo alguien sigue adelante creyendo que su
> ciclo terminó.
>
> Es el mismo error del 2026-08-08 con el marcador del prompt del agente: **un
> límite definido de un solo lado no es un límite.**
>
> ### Dos defectos, los dos encontrados corriéndolo, y los dos de la cara humana
>
> Esto es lo que más vale de la jornada, porque **la cara estructurada resultó ser
> un instrumento**: exige respuesta donde una persona perdona el silencio.
>
> 1. **Cinco verbos a secas no eran verbos.** `rm`, `mkdir`, `touch`, `cp` y `mv`
>    escritos solos caían al discurso del agente —«no tengo modelo»—, porque cada
>    brazo del despacho exige **un espacio detrás**. Es exactamente el defecto que
>    encontraste con `clear`, vivo en cinco verbos más y sin que nadie lo notara,
>    porque una persona lee esa respuesta como que la máquina está rara y teclea
>    otra cosa. Ahora cada uno tiene su propia pregunta: `cp` pide dos nombres y
>    `rm` uno, y una pista que no dice cuál no sirve.
> 2. **El primer objeto salía pegado al prompt humano.** El prompt del comando que
>    enciende el modo se imprimió cuando la cara todavía era humana y no lleva
>    salto de línea, así que la respuesta aterrizaba como
>    `  /home > {"op":"structured",…`. **El único renglón que no parseaba era el
>    que avisa que el modo está encendido.** Ninguna prueba del objeto lo veía; se
>    encontró canalizando la sesión y leyendo la salida.
>
> La regla que sale de los dos, en [[Estrategia-de-Pruebas]]: **el primer renglón
> de un modo nuevo lo escribió el modo anterior**, y es el que nadie prueba.
>
> ### Detalles con su razón escrita
>
> - **Un nombre que no es texto se marca.** Las rutas en Linux son bytes; un
>   nombre inválido en UTF-8 sólo se puede mostrar con pérdida, y devuelto como
>   argumento nombra otro archivo o ninguno. Sale `"exact": false`, y sólo cuando
>   significa algo.
> - **La forma se escribe a mano, no se deriva.** Nada lleva `#[derive(Serialize)]`:
>   una forma derivada la decide el nombre de una variante de Rust, así que
>   renombrar `Did::Copied` renombraría en silencio un campo que alguien parsea.
> - **`unreadable` está siempre, aunque esté vacío.** Una llave que sólo aparece
>   el día malo es una llave que nadie maneja el día malo.
> - **La confirmación carga la salida** (`"off": "structured off"`). En la imagen
>   no hay una segunda terminal, así que un modo sin salida visible puede dejar a
>   alguien atrapado.
>
> **1040 pruebas** (1004 antes), `clippy` limpio. Etapa **21** nueva en
> `verify.sh`, con su control: la misma sesión, el mismo store, y nadie pidiendo
> nada tiene que contestar en prosa.
>
> ### Lo que esto todavía no es
>
> **Sólo los verbos de archivos tienen dos caras.** `modulos`, `disponibles`,
> `permisos`, `recuerdos`, `estado`, `nucleo` y `discos` siguen contestando sólo
> en prosa, y las cuatro ventajas que hacen que la vara sea *mejor* y no *igual*
> —índice semántico, rollback, procedencia por campo, permisos por tarea— siguen
> sin estar expuestas a nadie. Está en [[Tareas-Pendientes]] como 4c.
>
> ## Quedó escrito para quién se construye, y los verbos ya cambian archivos — 2026-08-09
>
> Cómo se llegó a lo de arriba.
>
> ### Dos decretos, y son la vara de todo lo demás
>
> **El objetivo es uno: que un LLM trabaje mejor aquí que en cualquier otro
> sistema.** Todo lo demás es medio, y el camino humano se cumple entero porque
> es obligación, no porque sea hacia dónde va el proyecto. Cuando las dos cosas
> choquen **gana el LLM**, y el humano conserva acceso completo aunque le salga
> menos cómodo.
>
> El choque ya había ocurrido el mismo día sin que nadie lo notara: `ls` en
> columnas, tamaños redondeados a `1.2 kB` y ocultos escondidos son tres
> decisiones tomadas para un ojo humano, y las tres son peores para una máquina.
> Se tomaron sin notar que había una elección, porque el objetivo no estaba
> escrito. Ahora lo está.
>
> **Y la vara es un agente ajeno**, no el agente local de Thalyx: Claude Code y
> los suyos moviéndose aquí mejor que en Linux. Thalyx como **anfitrión**, no
> como llamador. Eso deja ver el estado real, que es duro: hoy Claude Code no
> arrancaría en Thalyx. No trabajaría mal — **no arrancaría**. Necesita ejecutar
> procesos, leer y escribir archivos, `grep`, `find`, `git`, un runtime, y aquí
> hay el kernel, un programa y veinte verbos.
>
> Lo que hace que la vara sea *mejor* y no *igual* **ya existe y no está expuesto
> a nadie**: el índice semántico, el rollback, la procedencia por campo y los
> permisos por tarea. Ningún otro sistema operativo ofrece «intenta esto y si
> sale mal deshazlo».
>
> La consecuencia de ingeniería es la que hay que recordar: **cada cosa nace con
> dos caras**, la humana y una estructurada que un programa pueda parsear. La
> segunda no se agrega después — si se agrega después, no se agrega.
>
> Detalle en [[Filosofia-Fundacional]], las dos secciones nuevas.
>
> ### `mkdir`, `touch`, `cp`, `mv`, `rm` — el punto 4, hecho
>
> Con comodines `*` y `?`. **Primera pieza construida bajo el decreto**, y se
> nota en la forma: ninguna operación imprime lo que hizo. Devuelve un `Done` con
> **qué pasó, dónde acabó y los bytes exactos**; la cara humana formatea ese
> hecho y la estructurada leerá el mismo. Un segundo camino que compone su propia
> frase es una segunda versión de los hechos.
>
> Cinco decisiones, cada una con la falla que evita escrita al lado:
>
> - **Nada sobrescribe sin pedirlo.** `Exists` es su propio error, porque
>   sobrescribir es otra petición y cuesta un archivo cuando se supone.
> - `make_file` usa `create_new`: comprobar y crear son dos momentos y entre
>   ellos puede aparecer algo. Que decida el kernel es la única versión sin hueco.
> - **Un enlace se copia como enlace y se borra como enlace.** Seguirlo
>   duplicaría el destino, y un enlace a un ancestro llenaría el disco.
> - `mv` cae a copiar-y-borrar ante `EXDEV`, que aquí es el caso ordinario y no
>   el exótico: `/home` y `/opt/thalyx` son subvolúmenes distintos.
> - **`*` no cruza `/`**, y no alcanza ocultos salvo que el patrón empiece con
>   punto. Sin lo primero, borrar `*` llega a todas las carpetas de abajo; sin lo
>   segundo, `rm *` se lleva la configuración de alguien.
>
> El comparador de patrones es iterativo con punto de retroceso y no recursivo:
> cuarenta estrellas contra un nombre largo es una pila que la forma recursiva no
> puede pagar, y un patrón así es justo lo que alguien teclea por accidente.
>
> `rm` con varios blancos **los lista antes de tocar nada**. `/home` es el único
> sitio del sistema que ningún rollback nuestro puede devolver, así que ese
> listado es el único aviso que existe.
>
> **1004 pruebas** (984 antes), `clippy` limpio.
>
> ### El hueco que esto deja abierto, y es el del decreto
>
> **La cara estructurada existe y nadie puede pedirla.** El `Done` lo lee hoy
> sólo el impresor humano. Mientras siga así, el decreto está escrito y no
> construido, y **ninguna de las cuatro ventajas está expuesta a nadie**. Es el
> punto 4b de [[Tareas-Pendientes]] y va antes que el editor.
>
> ### Una falla de proceso, y una lectura equivocada de la misma sesión
>
> Estos tres avances **vivieron sólo en los mensajes de commit**: ni este archivo
> ni [[Estado-de-Implementacion]] los mencionaban, y esa segunda nota no tenía
> fila para `thalyx-files` ni para `thalyx-term` —dos crates enteros ausentes de
> la nota que dice qué está construido—. Una sesión nueva que leyera la bóveda
> habría creído que lo último fue la terminal.
>
> Y al ir a corregirlo cometí el error de leer una copia local vieja de `main`
> como si fuera el estado del repositorio, y le dije a Cesar que los verbos no
> le llegaban con `git pull`. **Sí le llegaban**: `origin/main` ya tenía el
> trabajo fusionado. Lo único que faltaba de verdad era la bóveda. Queda escrito
> porque es la regla 5 en un sitio nuevo —el instrumento otra vez antes que lo
> medido— y porque `git rev-parse main` y `git rev-parse origin/main` son dos
> preguntas distintas.
>
> La rama sí estaba mal nombrada (`claude/verbos-donde-quedamos-uutcmm`) y ahora
> es `feat/file-mutating-verbs`.
>
> ## La terminal es una terminal, y dos lectores de `stdin` no caben — 2026-08-09
>
> Cómo se llegó a lo de arriba.
>
> Flechas, borrar a media línea, historial y tab. `crates/thalyx-term` decide qué
> significa cada tecla y dónde queda el cursor —puro, sin abrir ninguna
> terminal—; `thalyx-syscall` apaga el editor de línea del kernel con `termios`;
> `thalyx-cli/src/term.rs` es el único sitio donde se dibuja.
>
> **La guarda de modo crudo es lo más peligroso del archivo**, y por eso es una
> guarda: una sesión que sale sin devolver la terminal deja la máquina
> inservible, y en la imagen no hay una segunda de dónde recuperarse. El
> `Drop` cubre la salida normal y el desenrollado de un panic; no cubre un
> `SIGKILL`, y nada puede.
>
> ### Dos defectos de la misma familia, los dos encontrados corriéndolo
>
> Ninguna prueba unitaria los veía, porque los dos son sobre **quién es dueño de
> la entrada**:
>
> 1. **Lo tecleado por adelantado se perdía.** El búfer de bytes vivía dentro de
>    `read_line`, así que al pulsar Return todo lo que venía detrás se tiraba.
>    Un `read` devuelve lo que haya llegado, y eso es rutinariamente más de una
>    línea: alguien tecleando rápido, un pegado, o una prueba escribiendo todo
>    de golpe.
> 2. **Y al arreglar eso, la suite de `exit_criterion` se colgó entera.**
>    `instalar` pide una confirmación que leía `stdin` por su cuenta — y la `y`
>    que la contestaba ya estaba en mi búfer. **Seis sitios del CLI leían
>    `stdin` directo.** Ahora hay un solo dueño, `term::read_answer()`, y los
>    seis pasan por él.
>
> La regla que sale de esto, para [[Estrategia-de-Pruebas]]: **dos lugares que
> leen la misma entrada y uno que guarda lo que sobra no pueden coexistir**; el
> segundo espera para siempre bytes que ya salieron del kernel y están en
> memoria. El síntoma no es un error, es un silencio.
>
> ### Probado con una terminal de verdad
>
> El contenedor no alcanza esto con tuberías, así que se manejó un pty: `lst` +
> flecha + suprimir da `ls`; la flecha arriba devuelve el comando anterior; `cd
> Doc` + tab da `cd Documentos/`; con varias opciones se imprimen en columnas
> sin perder la línea; y **`cat niño` + flecha + retroceso da `nio`** — la `ñ`
> entera, que es de lo que trata que la línea sea `Vec<char>` y no `String`.
>
> Ctrl-C abandona la línea y da prompt nuevo; **no es una salida**, porque en la
> imagen no hay a dónde salir. Ctrl-D con algo escrito no hace nada: tratarlo
> como fin sería tirar la línea y salir, dos sorpresas por una tecla.
>
> **984 pruebas** (959 antes), `clippy` limpio.
>
> ## `ls` existe, y el vocabulario dejó de ser un problema de adopción — 2026-08-09
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> Cesar corrió los cuatro verbos en su Fedora y la primera frase fue la
> correcta: *«eso parece juguete más que sistema operativo serio»*. Tenía razón,
> y el problema era más viejo que mis cuatro verbos — **el vocabulario entero del
> sistema ya era así**: `discos`, `modulos`, `correr`, `apagar`.
>
> ### Lo que decidió, y por qué importa
>
> **Estándar primero, español también.** `ls`, `cd`, `cat`, `pwd`, `clear` son
> los que enseña el banner; `ver`, `leer`, `ir`, `donde`, `limpiar` siguen
> funcionando. El argumento es suyo, de dos mensajes antes: si para usar el
> sistema hay que decirle adiós a todos los comandos útiles, no hay adopción.
>
> **Un nombre no es un programa ajeno.** `ls` escrito en Rust dentro de `thalyx`
> es tan propio como `ver`. Lo que [[Construccion-del-ISO]] prohíbe es
> incrustarse en el sistema de alguien más, y `make -C image count` sigue
> diciendo uno.
>
> También decidió el **lenguaje de terminal, partido en dos**: comodines y
> redirección entran con copiar/mover/borrar porque son notación de diario;
> tuberías después; **guiones y variables quedan sin decidir**, porque ahí la
> pregunta pasa a ser si Thalyx tiene lenguaje de programación, que es como la
> gente construye software encima sin pasar por los módulos.
>
> Y aplazó lo de `NOEXEC` con una condición: *«cuando tengamos que decidir,
> explícamelos bien»*. Eso quedó en [[Tareas-Pendientes]] como **deuda de
> explicación**, con la lista de lo que hay que cubrir cuando se retome.
>
> ### Cuatro defectos, todos encontrados corriéndolo en su máquina
>
> Regla 1 otra vez, y ninguno lo veía el contenedor porque ninguno aparece en un
> directorio de prueba con seis archivos:
>
> 1. **`clear` contestaba con un discurso sobre el agente**, porque una línea
>    desconocida cae al mensaje de «no tengo modelo». Un comando común que
>    responde algo de otro tema es exactamente cómo un sistema se lee inacabado.
> 2. **Los ocultos se mostraban siempre.** Su carpeta tiene **treinta y cinco
>    nombres con punto** antes del primero que él puso, así que lo que buscaba
>    quedaba sepultado. Ahora se ocultan, `ls -a` los muestra, y **el listado
>    dice cuántos escondió** — un filtro silencioso es uno que nadie descubre.
> 3. **Una cosa por renglón.** Sesenta entradas eran cuatro pantallas. Ahora van
>    en columnas, hacia abajo y no a lo ancho, con el ancho real preguntado al
>    kernel por `TIOCGWINSZ` y ochenta como respaldo — y `None` es respuesta, no
>    fallo, porque una salida redirigida no tiene ancho.
> 4. **Se desalineaba con nombres largos.** La columna era fija en 32 y
>    `First_Layer_Bed_Leveling_Test.stl` tiene 33: **un archivo rompía la columna
>    de todos los renglones**. Ahora se mide.
>
> `ls -l` da tamaños, `ls -a` los ocultos, `ls -la` las dos, y las banderas
> tienen las dos escrituras (`todo`, `detalles`). Una bandera que no se conoce
> **no se ignora**: se queda como el lugar, para que la persona lea «`-z` no está
> ahí» en vez de recibir un listado que hace ver que la bandera funcionó.
>
> **959 pruebas** (946 antes), `clippy` limpio.
>
> ## Thalyx puede mirar sus propios archivos, y el prompt no era la causa — 2026-08-09
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> ### Lo que cambió de rumbo, y por qué
>
> Cesar preguntó cuánto falta para un sistema **usable sin el agente** — ver
> archivos, carpetas, correr comandos. Medido contra el código, la respuesta era
> incómoda: la sesión tenía **trece verbos y ninguno tocaba un archivo**. La
> capa 1 de [[Principio-Doble-Ruta]], marcada *no negociable*, no tenía
> implementación.
>
> Y al ir a construirlo salió que el proyecto se había estado leyendo mal a sí
> mismo. [[Construccion-del-ISO]] decía «Ningún shell. Ningún conjunto de
> utilidades — `ls`, `cat`», y eso parecía contradecir a Doble-Ruta. Cesar lo
> zanjó y la nota ya lo dice con sus palabras:
>
> > lo que está prohibido no es la shell, lo que está prohibido es incrustarnos
> > en la shell de otro sistema […] no es `ls` ni `cat`, está prohibido meternos
> > en un sistema ya hecho, porque si es así, no seremos un sistema operativo,
> > seremos una distro parcheada con IA.
>
> **Lo prohibido es el programa ajeno, no la capacidad.** No había
> contradicción entre dos decretos; había una nota ambigua, y ya está corregida.
>
> ### Lo construido
>
> `crates/thalyx-files` y cuatro verbos: **`ver`**, **`leer`**, **`ir`**,
> **`donde`**. Compilados dentro de `thalyx` — `make -C image count` sigue
> diciendo uno.
>
> Y una corrección de una afirmación mía anterior: **el subvolumen `user` sí se
> monta**, en `/home`, por `store_disk::mount()` desde PID 1. Yo había dicho que
> nadie lo conectaba porque lo busqué en `init.rs`. El piso ya existía; lo que
> faltaban eran los verbos.
>
> ### Tres defectos que sólo aparecieron corriéndolo
>
> Regla 1, otra vez, y los tres pasaban todas las pruebas:
>
> 1. **El prompt no cabía.** Con la ruta entera puso **noventa caracteres** antes
>    de que se pudiera teclear, y la consola de una máquina real suele tener
>    ochenta. Un sistema cuyo *prompt* no cabe no lo usa nadie. Ahora se acorta
>    por componentes enteros —nunca a media palabra— y **`donde` sigue siendo
>    exacto**: el recordatorio puede ser lossy, la respuesta no.
> 2. **`ir` imprimía el destino y el prompt lo repetía debajo.** La misma ruta
>    dos veces por cada movimiento.
> 3. **Los verbos nuevos no estaban en el banner.** Sin shell detrás, un verbo
>    que no está en esa lista no existe para quien tiene la máquina.
>
> Decisiones que quedaron con su razón escrita: `..` se pliega léxicamente (lo
> contrario de lo que hace la API de módulos, y a propósito — ahí no hay
> concesión de la que escapar); `leer` **se niega** ante un binario en vez de
> destrozar la terminal, porque en la imagen la sesión *es* la máquina y no hay
> una segunda de dónde recuperarse; y un enlace roto se lista **como roto**, no
> como ausente.
>
> **946 pruebas** (915 antes), `clippy` limpio.
>
> ### El tercer brazo corrió, y refutó mi hipótesis
>
> `IT INVENTS EITHER WAY` otra vez en los brazos de objeto, control 9 de 11.
> Firme, tercera vez.
>
> El brazo en prosa volvió a `NOT PROVEN`, y **la colisión de `NOTHING` no era la
> causa**. Corregí el prompt con su prueba, y las veinte respuestas siguen
> empezando con `NOTHING`. Lo que sobrevive es la degeneración, y se ve entera:
>
> ```
> NOTHING  NOTHING  NOTHING  NOTHING  NOTHING …
> NOTHING id only NOTHING id only NOTHING id only …
> NOTHING <<<THALYX-…613>>> NOTHING <<<THALYX-…614>>> NOTHING <<<THALYX-…615>>>
> ```
>
> Esa última línea es nueva: **el modelo fabrica marcadores contando hacia
> arriba**. El 2026-08-08 quedó escrito que un delimitador que el sistema medido
> puede escribir no delimita; esto lo empeora.
>
> **Y el control de 3 de 11 es en realidad 0.** Los tres «encontró el módulo» son
> eco del material devuelto (`NOTHING Identities: dev.thalyx.demo, ese …`). El
> pendiente *«dónde termina una respuesta en prosa»* ya no infla el control: **es
> el control**.
>
> Lo que queda establecido:
>
> | Brazo | Cómo se cicla |
> |---|---|
> | con gramática | `dev.thalyx.demo.versions.versions.versions…` |
> | sin gramática, en JSON | repite el objeto entero |
> | en prosa | `NOTHING NOTHING NOTHING…` |
>
> **Una patología con tres disfraces.** El 3B a temperatura 0 con este prompt
> degenera en cuanto se le suelta; la gramática era lo único que le daba una
> forma con final. El sospechoso ya no es el prompt. **Hipótesis, no conclusión.**
>
> ### Lo que sigue, en orden de dependencia
>
> Decidido por Cesar el 2026-08-09: construir la usabilidad, en este orden.
> **Del 1 al 7 se prueba todo en el contenedor**, así que por primera vez el
> trabajo no tiene a Cesar en el camino crítico.
>
> | # | Qué | Depende de | Estado |
> |---|---|---|---|
> | 1 | Dónde estoy y moverme | `/home` ya montado | **hecho** |
> | 2 | `ver` y `leer` | 1 | **hecho** |
> | 3 | Terminal: flechas, historial, tab | 2 — el tab completa nombres | siguiente |
> | 4 | `crear`, `copiar`, `mover`, `borrar`, `renombrar` | 2 | |
> | 5 | Editor de texto | 2 + 4 | |
> | 6 | `buscar` por nombre y por contenido | 2 | |
> | 7 | Procesos: qué corre, matarlo, memoria | independiente | |
> | 8 | Red: drivers, IP, DNS | independiente | **sólo hierro de Cesar** |
> | 9 | Lenguaje: tuberías, redirección, comodines | 2+4+6 **y decreto** | |
>
> Dos cosas que hay que decir y no se tocaron:
>
> - **`/home` está montado `NOEXEC`.** Nadie puede ejecutar un programa desde su
>   carpeta personal, y en Linux sí se puede. Es de las cosas que un usuario
>   nota. **Decisión de Cesar**, ver [[Tareas-Pendientes]].
> - **El punto 9 es decreto antes que código.** Trece verbos sueltos y un
>   lenguaje que los compone son proyectos distintos.
>
> ## El prompt gastó su propia señal, y la corrida no midió — 2026-08-09
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> ### Lo que quedó firme
>
> **La gramática no es la causa.** `IT INVENTS EITHER WAY` en la gama media, con
> el control sostenido en 9 de 11, y **reproducido en dos corridas**. Sin
> gramática el 3B sigue inventando en cuatro de los nueve casos de rechazo y
> nombrando el módulo real equivocado en cinco. Quitarle la restricción no lo
> hace abstenerse.
>
> **No hubo primera abstención.** Aquel `said something, named nothing` era la
> segunda lectura: el comando reprodujo la inferencia y salió
> `"targets": ["good-luck-module-1234567890123456789…` — 255 tokens de dígitos
> dentro de un identificador, sin cerrar. Octavo caso de la misma patología. **La
> abstención sigue en cero, ahora sobre 55 oportunidades.**
>
> ### El tercer brazo no pudo medir, y el defecto es del prompt
>
> Dio `NOT PROVEN` con `ABSTAINED 8/9` — que era el número que la hipótesis
> quería. El control lo tumbó (5 de 11), y el motivo está a la vista: **las
> veinte respuestas empiezan con `NOTHING`**, también las once donde sí había un
> módulo.
>
> ```
> act  a pronoun pointing at the one thing installed
>   in prose  "NOTHING Identities: dev.thalyx.demo: 1.4.2 dev.thalyx.demo: 1.4.1 …"
> act  a module named by what it does rather than by its id
>   in prose  "NOTHING  NOTHING  NOTHING NOTHING NOTHING NOTHING NOTHING …"
> ```
>
> Eso no es declinar. Es un primer token regalado. **Y lo regaló mi prompt**, que
> usaba la palabra cuatro veces, tres de ellas en otro sentido: *and **nothing**
> else*, *gains **nothing***, *if **nothing** below names a module*.
>
> Corregido —*add no other text*, *only costs the request*, *if no module is
> named below*— con una prueba que cuenta las apariciones sin distinguir
> mayúsculas y falla si hay más de una.
>
> **No se afirma que la colisión sea la causa**: compite con la degeneración, que
> está igual de a la vista —las respuestas son `NOTHING NOTHING NOTHING…`, el
> mismo ciclo que dentro de `module-id`, en otro token—. Lo que sí queda
> establecido es que el prompt no podía medir.
>
> Y hay que decirlo entero: **corregirlo puede destruir el 8 de 9.** Se corrige
> igual.
>
> ### Volver a correrlo
>
> ```sh
> git pull && cargo install --path crates/thalyx-cli
>
> thalyx agent model use media --weights ~/models/qwen2.5-3b-instruct-q4_k_m.gguf
> thalyx agent grammar-effect --keep-prompt ~/evidencia/prosa-media-2 2>&1 | tee prosa-media-2.log
> ```
>
> La gama ligera ya no aporta a esta pregunta: por **tercera** corrida, sin
> gramática contesta las veinte con cero tokens, en los dos brazos libres. En un
> 1.5B forzarle el primer carácter es lo único que arranca la generación.
>
> ### Lo que sigue sin decidirse
>
> **Dónde termina una respuesta en prosa.** El modelo contesta y después divaga
> 250 tokens; el lector actual busca identificadores en todo el texto, así que
> `NOTHING Identities: dev.thalyx.demo: 1.4.2 dev.thalyx.demo: 1.4.1 …` —el
> material devuelto de vuelta— cuenta como haber nombrado el módulo. Eso **infla
> el control** del brazo en prosa. Decidir dónde corta cambia lo que el
> instrumento mide, y por eso no se toca sin aprobación. Ver
> [[Tareas-Pendientes]].
>
> 915 pruebas, `clippy` limpio.
>
> ## El instrumento para la pregunta de la abstención está listo — 2026-08-08
>
> Cómo se llegó a lo de arriba.
>
> La abstención sale **0 de 46** en tres tamaños de modelo y seis corridas, y un
> resultado que no se mueve cuando la única variable se mueve habla de lo que
> esas corridas **comparten**. La sospecha tiene mecanismo:
> `operation ::= "\"install_module\""` tiene **una sola alternativa**, y el orden
> de los campos está fijo, así que lo primero que el modelo escribe en cada
> inferencia es `install_module`, obligado; abstenerse exige contradecirlo
> después. Detalle en [[Gamas-de-Modelo]].
>
> Se construyó `thalyx agent grammar-effect` para contestarlo. **Nada del prompt,
> la gramática ni las gamas fue tocado.**
>
> ### Los comandos, en orden
>
> ```sh
> git pull && cargo install --path crates/thalyx-cli
>
> # La gama media primero: es la que más entiende, así que su brazo libre es
> # el que mejor puede sostener el control.
> thalyx agent model use media --weights ~/models/qwen2.5-3b-instruct-q4_k_m.gguf
> thalyx agent grammar-effect --keep-prompt ~/evidencia/efecto-media 2>&1 | tee efecto-media.log
>
> thalyx agent model use ligera --weights ~/models/qwen2.5-1.5b-instruct-q4_k_m.gguf
> thalyx agent grammar-effect --keep-prompt ~/evidencia/efecto-ligera 2>&1 | tee efecto-ligera.log
> ```
>
> Cuarenta inferencias por gama: unos **4–5 minutos** la media, unos **3** la
> ligera. Imprime cada caso al terminarlo, así que no hay silencios largos.
>
> ### Qué puede contestar, y las tres son respuestas
>
> | Veredicto | Qué significa |
> |---|---|
> | `THE GRAMMAR TAKES THE DECISION` | Con gramática inventó, sin ella nunca — y aun así encontró el módulo correcto donde lo había. El cero del banco no es el modelo negándose a declinar, es Thalyx obligándolo a decir `install_module` antes de poder |
> | `IT INVENTS EITHER WAY` | Quitar la gramática no lo detuvo. **La hipótesis queda refutada** y lo que resta sospechar es el prompt o el modelo |
> | `NOT PROVEN` (sale distinto de cero) | Sin gramática el modelo no encontró el módulo correcto ni donde sí lo había, así que su silencio en los casos de abstención no es una decisión. **No es evidencia de nada**, y el propio comando lo dice |
>
> El tercero es el que hace honesto al experimento: si el control no aguanta, no
> hay veredicto. Un sondeo que no puede fallar no es un sondeo.
>
> ### La primera corrida salió NOT PROVEN, por defectos míos
>
> El instrumento se negó a dar veredicto, que es lo correcto, pero se negó por
> cuatro defectos suyos. Dos los encontró una prueba; **dos sólo aparecieron
> corriendo contra un modelo de verdad**, que es la regla 1 otra vez.
>
> 1. El escáner de ids era **ciego al JSON**.
> 2. Los dos brazos **se pisaban el `command` guardado** — ese ya existía en
>    `grammar-check`.
> 3. El brazo restringido se juzgaba con `Proposal::parse`, estricto con el
>    texto sobrante, y llama.cpp añade su aviso de fin de generación después del
>    objeto: **las cuarenta inferencias volvieron `NO MEASUREMENT`**.
>    `Proposal::completion_in` ya resolvía eso y el banco la usaba desde siempre.
> 4. **El brazo libre no es prosa.** Sin gramática el 3B sigue contestando en
>    JSON, porque el *prompt* pide JSON y el prompt está en los dos brazos:
>    `instala algo bueno` → `{"targets": ["good-bad-thing"]}`. Nada de eso es un
>    id reverse-DNS, así que el escáner reportaba **silencio** —la lectura «el
>    modelo declinó»— para una respuesta que proponía instalar algo inventado. Y
>    la ligera, sin gramática, contestó las veinte con un fin de generación
>    inmediato, contado también como silencio. **Tres hechos distintos llegaban
>    como la misma palabra.**
>
> Lo cuarto es la trampa de hace dos días repetida: un `[end of text]` leído como
> una decisión. Ahora hay un estado aparte, `GENERATED NOTHING (not a decline)`.
>
> Las fixtures ya no son inventadas: son las salidas literales de tu corrida.
> 896 pruebas, `clippy` limpio.
>
> ### Lo que la corrida fallida ya insinúa, y todavía no se afirma
>
> La gama media, **sin ninguna gramática**, propuso instalar algo en los nueve
> casos de abstención. Si eso se sostiene con el instrumento arreglado, el
> veredicto será `IT INVENTS EITHER WAY` y **mi hipótesis queda refutada**: no es
> la gramática, es el prompt, que pide un objeto JSON en los dos brazos. Se
> afirma cuando el instrumento lo diga, no antes.
>
> ## Cinco corridas, y el banco es más estable de lo que parecía — 2026-08-08
>
> Cómo se llegó a la pregunta de arriba.
>
> Tres corridas de la gama ligera y dos de la media, con `--keep-prompt`. Esto
> **corrige la lectura del bloque de abajo**, que decía que las cifras de
> acierto se mueven:
>
> | | ligera ×3 | media ×2 |
> |---|---|---|
> | **Aciertos sobre los 20** | **5, 6, 6** | **9, 9** |
> | Sin medición | 6, 5, 2 | 1, 2 |
> | Intención (sobre lo medido) | 5/14, 6/15, 6/18 | 9/19, 9/18 |
> | Abstención | 0/6, 0/6, 0/9 | 0/9, 0/8 |
>
> 1. **Lo que se mueve no es el acierto, es cuántos casos contestan.** El número
>    de respuestas correctas casi no se movió; el denominador sí. Y pasó el caso
>    que lo demuestra: la ligera contestó cuatro casos más, acertó uno más, **y
>    su fracción bajó** de 36 % a 33 %, porque los cuatro que recuperó volvieron
>    mal. **La cifra que se compara entre corridas es aciertos sobre 20.**
> 2. **Catorce de los veinte casos dieron la misma marca en las cinco corridas.**
>    La suite es estable. Los aciertos se mueven ±1.
> 3. **El caso 4 no ha producido una medición ni una sola vez**: `quiero la 1.4
>    del demo`, cinco de seis corridas con el presupuesto de tokens agotado. Es
>    el único caso de la suite cuya restricción esperada lleva un punto adentro
>    (`1.4`). Hay hipótesis y **no está probada**; se resuelve con un comando,
>    abajo.
> 4. **Abstención: cero en 46 oportunidades**, tres tamaños de modelo, seis
>    corridas. Es la propiedad más firmemente medida del proyecto. Los tres casos
>    que la ligera nunca había alcanzado a contestar resultaron ser de
>    abstención, y al contestarlos por fin los falló los tres.
> 5. **La media no se movió ni un caso en dos corridas** (9 y 9). La distancia
>    con la alta —dos casos— ya no cae dentro del ruido del instrumento. Lo que
>    le falta a esa comparación es que la alta corra dos veces, y está aplazada.
>
> ### El caso 4, resuelto el mismo día — y era `module-id`
>
> Cesar corrió la inferencia guardada. La salida contesta sola:
>
> ```
> "targets": ["dev.thalyx.demo.versions.versions.versions.versions…
> ```
>
> 255 de 256 tokens, y **nunca llegó a `constraint`**. La hipótesis del punto en
> `1.4` queda **refutada**: el ciclo está en `module-id`, no en `range`.
>
> Las tres capas, que hay que decir separadas:
>
> | | |
> |---|---|
> | Causa inmediata | agotó `n_predict` sin cerrar el objeto |
> | Causa observada | repitió `.versions` dentro de `module-id` |
> | Condición que lo permite | la producción admite segmentos sin cota |
>
> **La gramática no lo obliga a repetir** — el modelo elige `.versions`; la
> gramática nunca le exige cerrar.
>
> Y explica de más, que es lo importante: `ese.abc.abc.abc`, `thallyx.ing.ing`,
> `dev.thalyx.demo.localhost`, `photoshop-1.ashx.ashx`,
> `python3.ipython3.ipython3` son **el mismo comportamiento**. Cuando el 1.5B no
> sabe cerrar semánticamente un id, sigue produciendo segmentos válidos; si el
> corte llega antes de cerrar la cadena sale `ERR`, si llega después sale una
> invención. Un comportamiento que llevábamos contando como tres.
>
> El caso 4 es además la demostración más limpia del proyecto de lo que la
> gramática no puede hacer: el modelo **empieza con el id correcto** —está en el
> prompt— y lo convierte en otro. `dev.thalyx.demo.versions` es sintácticamente
> válido y semánticamente inventado. Esa segunda columna es de la atribución.
>
> Al inspeccionar la producción salió algo que no se buscaba: **`thalyx-manifest`
> tampoco tiene cota**, así que la gramática espeja fielmente a la autoridad y el
> hueco está en las dos. Tres opciones escritas en [[Gamas-de-Modelo]], con la
> predicción de que acotar **no subiría el acierto** —convertiría `ERR` en `REF`,
> como ya se observó—. **Nada tocado.**
>
> Detalle completo en [[Gamas-de-Modelo]], «Tres corridas de ligera y dos de
> media».
>
> ## La gama ligera se corrió dos veces, y no dio lo mismo — 2026-08-08
>
> Cómo se llegó a lo de arriba, y con una lectura que el bloque anterior corrige.
>
> Cesar repitió `grammar-check` y el banco sobre la gama ligera, sin cambiar
> nada. Tres resultados, en orden de importancia:
>
> 1. **Las cifras de acierto se mueven entre corridas; las de coste no.** Dos
>    casos de veinte cambiaron, en direcciones opuestas (14/20 → 15/20 medidos,
>    5/14 → 6/15). El disco, el RSS pico (2.82 GB) y la latencia mediana (3.77 s)
>    salieron idénticos. La causa no es llama.cpp: la semilla está fija pero
>    **el prompt lleva un marcador aleatorio nuevo en cada invocación**, así que
>    la entrada cambia. El encabezado de `llama.rs` afirmaba lo contrario y ya no
>    lo afirma. Consecuencia directa: la distancia de **dos casos** entre la gama
>    media y la alta es del tamaño de lo que se mueve una gama consigo misma, así
>    que no es una diferencia entre gamas. Nada de lo medido se retira; ahora
>    tiene margen.
> 2. **Los cinco `ERR` tienen una sola causa, y ya se sabe cuál**: el modelo
>    empieza el objeto y se queda sin presupuesto dentro de un identificador —la
>    gramática no acota cuán largo puede ser—. Se cicla:
>    `python3.ipython3.ipython3.…`. Subir `-n` no lo arregla, sólo alarga el
>    ciclo. Y es la **misma** patología que las invenciones (`ese.abc.abc.abc`,
>    `thallyx.ing.ing`): un fallo, contado como dos.
> 3. **`grammar-check` de la ligera ya dice `NOT PROVEN` sobre hardware real.**
>    La corrección del día anterior quedó verificada donde importa.
>
> Detalle completo en [[Gamas-de-Modelo]], sección «Segunda corrida de la gama
> ligera».
>
> ### Lo que Cesar decidió, y ya está construido
>
> **Guardar el prompt bajo una bandera.** `--keep-prompt <dir>` en `agent model
> check`, `agent model grammar-check` y `agent bench`: cada inferencia deja un
> directorio —nombrado por su marcador, así que veinte casos dejan veinte— con
> `prompt.txt`, `proposal.gbnf` y `command`. Con eso *esa* corrida se repite a
> mano, marcador incluido. El marcador **sigue siendo aleatorio**, así que dos
> corridas distintas siguen moviéndose, y eso es lo correcto: esconderlo daría
> una muestra de una distribución con cara de medición. Sin la bandera no queda
> nada en disco.
>
> De paso: `Invocation::command_line` —la función que el encabezado de
> `llama.rs` citaba como *la* forma de reproducir una corrida— no tenía ninguna
> llamada fuera de su propia prueba. Documentación de una función que nunca se
> había ejecutado. `--keep-prompt` es su primera llamada real.
>
> ### Lo siguiente que hay que correr
>
> **Repetir la ligera y la media** —una vez cada una, sin cambiar nada— para
> darle réplica a sus cifras de acierto. La **alta queda aplazada**: tarda
> demasiado en esta máquina, y Cesar la corre cuando consiga el equipo con el
> que también pueda medir la máxima. Vale la pena correrlas ya con la bandera:
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> thalyx agent model use ligera --weights ~/models/qwen2.5-1.5b-instruct-q4_k_m.gguf
> thalyx agent bench --keep-prompt ~/evidencia/ligera-3 2>&1 | tee ligera-3.log
> thalyx agent model use media  --weights ~/models/qwen2.5-3b-instruct-q4_k_m.gguf
> thalyx agent bench --keep-prompt ~/evidencia/media-2  2>&1 | tee media-2.log
> ```
>
> Lo que hay que mirar al terminar: **cuántos casos cambiaron de marca** contra
> la corrida anterior de esa misma gama. Ése es el margen de error del banco, y
> hasta tenerlo la distancia entre la media y la alta no significa nada.
>
> ## Las cuatro gamas corrieron sobre la misma máquina, y la más grande no cabe — 2026-08-08
>
> Cómo se llegó a la corrida de arriba.
>
> Cesar corrió `check`, `grammar-check` y el banco de 20 casos en **ligera,
> media y alta**, sobre su Ryzen 5 5600G de 16 GB, sin GPU, en CPU, con la misma
> familia (Qwen2.5-Instruct), la misma cuantización (Q4_K_M), el mismo
> `llama.cpp`, el mismo prompt, la misma gramática y la misma suite. **Lo único
> que varió es el tamaño**, que es lo que hace la comparación atribuible.
>
> | | ligera 1.5B | media 3B | alta 7B | maxima 14B |
> |---|---|---|---|---|
> | Casos medidos | 14/20 | 19/20 | 19/20 | **0/20** |
> | Intención | 5/14 | **9/19** | 7/19 | N/D |
> | Argumentos | 5/14 | **8/19** | 7/19 | N/D |
> | Abstención | 0/6 | 0/9 | 0/8 | N/D |
> | Latencia mediana | 3.77 s | 6.78 s | **33.26 s** | N/D |
> | RSS pico | 2.82 GB | 4.79 GB | **13.93 GB** | N/D |
>
> ### Lo más importante, en orden
>
> 1. **La gama alta no superó a la media en este banco**, y costó ×4.9 de
>    latencia y ×2.9 de memoria. La afirmación legítima es estrecha —con *este*
>    prompt, *esta* gramática, *estos* casos, *esta* cuantización y *este*
>    hardware— y **no** es «3B es más listo que 7B»: la diferencia es de dos
>    casos sobre diecinueve, que es menos de lo que esta suite puede separar. Lo
>    que sí sostiene es la **ausencia de mejora medible** frente a un costo que
>    no es discutible.
> 2. **La máxima quedó `N/D`, no en cero.** El proceso fue terminado por falta de
>    memoria después de imprimir la gama y el enunciado, antes de completar la
>    primera inferencia. No hubo banco que fallar. Lo probado es que *esta*
>    máquina de 16 GB con *esta* configuración no la sostiene — **no** que 14B
>    pida 32 GB, que sigue siendo el estimado del decreto.
> 3. **Abstención cero en las tres gamas medidas, sin excepción.** Es la medida
>    que [[Gamas-de-Modelo]] llama la más importante, y es la única que sale
>    **idéntica** en 1.5B, 3B y 7B. Un resultado plano donde lo único que varía
>    es el tamaño apunta a lo que las tres comparten, no a lo que las separa. Es
>    hipótesis, y **no se tocó el prompt**.
> 4. **`grammar-check` de la gama ligera decía `PROVEN` y no lo estaba.**
>    Corregido, con dos regresiones. Ver abajo.
>
> ### El `PROVEN` retirado, que es el defecto del día
>
> ```
> with the grammar     { "operation": "install_module", "targets": ["python3.ipython3.…
> without it           [end of text]
> PROVEN: … constrained it could not even begin with it, and left alone it did.
> ```
>
> *«Left alone it did»* — dijo la palabra prohibida. **No la dijo: no dijo
> nada.** `[end of text]` es lo que imprime `llama.cpp` cuando el modelo termina
> la generación de inmediato. En media y alta el brazo libre sí muestra `BANANA`
> y ahí el veredicto es correcto; en ligera no había control.
>
> El veredicto afirma dos cosas y el código comprobaba una: `InForce` era el
> `else`, así que se alcanzaba con que el brazo libre **no abriera un objeto** —
> y un brazo callado tampoco abre uno. Regla 4, sobre el veredicto en vez de
> sobre el experimento. Sobrevivió por la regla 8: los cuatro sustitutos del
> sondeo decían la palabra al quitarles la gramática, **ninguno modelaba un
> modelo callado**. Regla nueva en [[Estrategia-de-Pruebas]].
>
> Lo que sí se puede afirmar de esa corrida, y es menos: **la bandera cambió la
> salida** —con ella hubo objeto, sin ella nada—. Que lo que la gramática impidió
> fuera *la palabra* no se midió. Así que «un contrato mal formado es imposible
> en las cuatro gamas» está probado en **dos**, no probado en ligera, N/D en
> máxima.
>
> ### La demostración de que la gramática no garantiza el contenido
>
> La gama ligera contestó `dev.thalyx.demo, ese quiero` con
> `["dev.thalyx.demo","ese.quiero.ios"]`. **Fabricó un id a partir de las
> palabras humanas «ese quiero»**, con la forma perfecta. La gramática hizo lo
> que promete y nada más; lo que lo detuvo fue la atribución del núcleo. Contrato
> válido, contenido inventado, en una sola línea de salida.
>
> ### Y una pregunta vieja quedó contestada
>
> `dev.thalyx.demo, ese` sale `REF` en las tres gamas: el modelo nombró algo que
> no aparece en ningún canal. **No es una abstención.** Con eso se cae del todo
> la hipótesis de que la instrucción de abstención del prompt pesa de más — el
> `MISS` que la originó venía del banco que contaba `Err(_) => Abstained`, donde
> un rechazo por atribución se veía como abstención correcta.
>
> ### El estatus que Cesar le puso a todas estas cifras
>
> Preguntado si la columna de RAM recomendada baja ahora que el RSS medido salió
> menor, decretó que no, y con un alcance más amplio que la pregunta:
>
> > declara los resultados mas no los muestres como pruebas definitivas, las
> > pruebas definitivas vendran cuando thalyx este corriendo en una ssd real como
> > sistema operativo real, solo en ese entorno se vera la realidad
>
> Así que **todo lo de arriba queda declarado y nada queda como definitivo**.
> Sirve para comparar las gamas entre sí, porque las tres corrieron bajo las
> mismas condiciones; **no** sirve para fijar el requisito de hardware de Thalyx,
> ni para bajar la columna de RAM, ni para decidir qué gama trae el ISO. Eso son
> afirmaciones sobre el destino, y el destino es Thalyx como sistema operativo
> sobre un SSD real. Queda como pendiente con condición escrita en
> [[Tareas-Pendientes]].
>
> ### Qué se cambió, y qué no
>
> Cambiado, y las dos cosas son evidencia y no puntuación:
>
> - `grammar_check` exige que el brazo libre **diga la palabra**, con dos
>   regresiones comprobadas fallando contra el código anterior.
> - El banco imprime **qué** id rechazó la atribución, en vez de `(named
>   something nobody mentioned)`.
>
> **No cambiado a propósito**: el prompt, la gramática, las gamas, la suite y la
> columna de RAM recomendada. Cambiar cualquiera de ellos como reacción a estos
> resultados haría que la próxima corrida no se pudiera comparar con ésta.
>
> ### Lo que corre Cesar, y es el experimento que eligió
>
> **Repetir la gama ligera guardando la salida entera.** Seis de veinte casos no
> dieron medición y el banco **sí imprime la razón de cada uno** —plazo agotado,
> truncamiento, gramática no aplicada, `llama.cpp` cayéndose son fallos
> distintos, y mandan a lugares opuestos— pero esa columna no llegó a la bóveda
> en la transcripción. Es una corrida, sin cambiar nada, y hasta tenerla `5/14`
> no es la puntuación de esa gama ni se sabe qué le pasa.
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> thalyx agent model use ligera --weights ~/models/qwen2.5-1.5b-instruct-q4_k_m.gguf
> thalyx agent model grammar-check 2>&1 | tee ligera-grammar.log
> thalyx agent bench              2>&1 | tee ligera-bench.log
> ```
>
> Dos cosas que esperar y que **no** son fallos:
>
> - `grammar-check` debe salir **`NOT PROVEN`** y **distinto de cero**. Es el
>   resultado correcto: no es que la gramática falle, es que ese modelo contesta
>   el sondeo callándose y entonces no hay control. Si sale `PROVEN`, la
>   corrección no llegó.
> - El banco vuelve a perder casos. Lo que se busca es **qué dice cada `ERR`**.
>
> **872 pruebas pasan** (870 antes), `clippy` limpio, `cargo fmt` aplicado.
>
> ## El banco contaba todo fallo como abstención correcta — 2026-08-08
>
> **El bloque de arriba es más reciente.** Los de abajo son cómo se llegó.
>
> Cesar dijo que por ahora no descarga más modelos y que como máximo corre
> verificaciones, así que el trabajo fue sobre el instrumento. Encontrado
> leyendo el banco, no leyendo sus números:
>
> ```rust
> Err(_) => Outcome::Abstained,
> ```
>
> **Toda forma de fallar contaba como el modelo absteniéndose bien.** Un plazo
> agotado, un truncamiento, una gramática no aplicada, `llama.cpp` cayéndose. Una
> gama cuyo modelo no arrancara nunca sacaba 4/4 en abstención, que es la medida
> que [[Gamas-de-Modelo]] llama la más importante.
>
> Y peor: `AgentError::Attribution` —el núcleo cazando al modelo nombrando un id
> que nadie mencionó— caía en la misma rama. **La conducta más peligrosa que el
> banco busca, contada como la más segura.**
>
> ### Las cifras de acierto del 2026-08-08 quedan retiradas
>
> Intención 6/9, argumentos 6/9, abstención 3/4 no significan lo que parecían. Se
> mantienen disco, RAM y latencia, que se miden alrededor del proceso. **Se cae
> también la hipótesis** de que la instrucción de abstención del prompt pesa de
> más: los dos `MISS` pudieron ser abstenciones reales o errores disfrazados, y
> desde la salida impresa no se distinguen.
>
> ### Qué se construyó
>
> - **Cinco resultados** y ninguno inferido de la ausencia de otro: correcto,
>   equivocado, abstenido, **rechazado por el núcleo**, **sin medición**.
> - Un caso sin medición no cuenta en ninguna fracción. Los denominadores son
>   sobre lo medido, y el resumen lo dice **antes** que cualquier cifra.
> - La clasificación salió del bucle a `Outcome::of`, que es una función pura —
>   el defecto vivía enterrado en una expresión donde ninguna prueba lo alcanzaba.
> - **La suite pasó de 9 casos a 20.** Con nueve, un caso vale once puntos. Los
>   nuevos varían **una** cosa a la vez respecto de uno que ya estaba, para que la
>   próxima corrida conteste por qué falló el caso fácil.
> - La exención de «este caso de abstención sí nombra un módulo» era una
>   subcadena del *nombre* del caso; ahora es un campo con la razón escrita, con
>   su control.
>
> ### Lo que corre Cesar
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> thalyx agent bench
> ```
>
> Una corrida, con el modelo que ya tiene. Devuelve las cifras de acierto con
> significado por primera vez.
>
> ## La gramática restringe de verdad, probado en hierro — 2026-08-08
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> ```
> thalyx agent model grammar-check
> with the grammar     { "operation": "install_module", "targets": [ "python3.abc_1.abc", …
> without it           BANANA <<<TH
> PROVEN
> ```
>
> Restringido no pudo ni empezar con la palabra prohibida; suelto la dijo. Con
> eso, la frase de [[Gamas-de-Modelo]] —«un contrato malformado es imposible en
> las cuatro gamas»— deja de apoyarse sólo en las pruebas del parser. **En una
> gama**; las otras tres heredan el argumento y no la corrida.
>
> ### El defecto que traía esa corrida que pasó
>
> `BANANA <<<TH`: el modelo dijo la palabra y **empezó a reproducir el marcador
> que acababa de leer**. Sólo lo cortó el tope de tokens.
>
> El marcador es aleatorio por invocación, y eso estaba razonado contra un
> adversario —un texto ajeno no puede adivinarlo—. No cubría esto: el modelo no lo
> adivina, lo tiene delante.
>
> > **Un delimitador que el sistema medido puede escribir no delimita.** Ser
> > imposible de adivinar no es ser imposible de copiar.
>
> `answer_in` tomaba la **última** aparición del marcador, así que una copia
> completa habría movido dónde empieza la respuesta. Ahora se ancla en el prompt
> repetido entero, y el marcador solo queda de respaldo tomando la primera.
> `RANGE_CHARS` contiene `<`, `>` y `-`, así que esto llegaba también al camino
> restringido, dentro de un campo `constraint`.
>
> **Nada falló para encontrarlo.** El veredicto era correcto; el defecto estaba en
> la evidencia impresa al lado, y sólo porque se imprimía. Regla nueva en
> [[Estrategia-de-Pruebas]]: una corrida que pasa también trae datos.
>
> ### Lo que queda abierto
>
> - Las otras tres gamas, que son otros tres GGUF.
> - La instrucción de abstención pesa de más: se abstuvo con el id dicho en claro.
> - Actuó sobre un módulo mencionado y luego descartado. Comprensión, no gramática.
>
> Ver [[Tareas-Pendientes]].
>
> ## El agente corre entero contra hierro real, con el primer banco medido — 2026-08-08
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> ### La gama media, medida
>
> | Medida | Estimado | Medido |
> |---|---|---|
> | Disco | ~2.0 GB | 2 104 932 768 bytes |
> | RAM | ~8 GB | **4.78 GB** |
> | Latencia | — | mediana 6.58 s, peor 7.94 s |
> | Intención | — | 6/9 |
> | Abstención | — | 3/4, con **1 invención** |
>
> La estimación de RAM iba alta por casi el doble. Los tres fallos están
> analizados en [[Gamas-de-Modelo]]; el que importa es que **actuó sobre un
> módulo que la persona había descartado** — no inventó un id, tomó uno excluido.
>
> ### `grammar-check` falló, y el que estaba mal era `grammar-check`
>
> Dijo `FAILED`. La prueba de que estaba al revés venía en su propia salida: el
> brazo restringido había emitido `{ "operation": "install_module", "targets":
> ["banana_module_1234…` hasta agotar los 256 tokens. **Empieza con `{`.** La
> gramática le prohibió empezar con `B` y el modelo desvió el intento a una cadena
> de id legal, quedándose ahí hasta el tope. El JSON no cerró, así que no parseaba
> — y la comprobación preguntaba si parseaba.
>
> > **Una falla al terminar no es una falla al cumplir.** Regla 10 en un sitio
> > nuevo, y la octava vez que el instrumento se equivocó antes que lo medido.
>
> Y lo delató una contradicción entre dos corridas: `grammar-check` decía que la
> gramática no se aplicaba, y `bench` sacaba nueve propuestas bien formadas de
> nueve casos minutos después. Las dos no pueden ser ciertas.
>
> ### Qué se corrigió
>
> - Se lee **el primer carácter**, no si el resultado parsea. `root ::= "{"` es
>   absoluto y sobrevive al truncamiento.
> - El mismo defecto estaba **en el camino de producción**: una inferencia normal
>   truncada contra `-n` también salía como «gramática no aplicada». Ahora hay
>   `Truncated`, y su mensaje dice que *esto es la gramática funcionando*.
> - El sondeo gasta 48 tokens en vez de 256; sólo el primer carácter decide.
> - Las dos ramas se imprimen **también cuando falla**. La versión anterior
>   escondía la de control justo en el caso donde valía más.
>
> ### Lo siguiente
>
> Cesar corre `git pull && cargo install --path crates/thalyx-cli` y luego
> `thalyx agent model grammar-check`, que debe salir `PROVEN`.
>
> Decidido a medias y sin decidir: la instrucción de abstención del prompt pesa
> demasiado —abstuvo con el id dicho en claro— y bajarla es tocar el prompt, que
> mueve los nueve casos a la vez. Ver [[Tareas-Pendientes]].
>
> ## El agente contesta desde hierro real, y falta una comprobación por correr — 2026-08-08
>
> **Éste es el estado actual.** Los bloques de abajo son cómo se llegó.
>
> ```
> thalyx agent model check "dev.thalyx.demo, ese quiero"
> answer    {"operation": "install_module", "targets": ["dev.thalyx.demo"]}
> latency   6.88s
> peak rss  4.77 GB
> parsed as: Proposal { operation: InstallModule, targets: ["dev.thalyx.demo"], … }
> ```
>
> Enunciado → modelo real → propuesta parseada, de extremo a extremo, en la
> Fedora de Cesar. **La RAM medida es 4.77 GB contra los ~8 GB que estimaba
> [[Gamas-de-Modelo]]**: el primer número de esa tabla que alguien midió.
>
> ### Lo construido después: separar «bandera aceptada» de «gramática aplicada»
>
> `llama.cpp` sale distinto de cero ante una bandera que no conoce, así que una
> corrida limpia probaba que `--grammar-file` fue **aceptada**. No que
> restringiera nada — el prompt real le pide un objeto al modelo, y un modelo que
> da un objeto sólo hizo lo que le dijeron. Cuatro gamas del decreto se apoyaban
> en no notar la diferencia.
>
> `thalyx agent model grammar-check` pide **la única palabra que la gramática no
> puede emitir**, dos veces, con la bandera y sin ella, sin ninguna otra
> diferencia entre las dos corridas. Tres resultados:
>
> | Resultado | Qué significa |
> |---|---|
> | `PROVEN` | Restringido no pudo decirla; suelto sí. Sólo la gramática explica eso |
> | `FAILED` | La dijo con la gramática puesta. No se está aplicando |
> | `NOT PROVEN` | Las dos ramas dieron propuesta: el sondeo no midió nada, y eso **no es pasar** |
>
> Etapa nueva en `verify.sh`, y regla nueva en [[Estrategia-de-Pruebas]]: probar
> que algo restringe necesita un enunciado cuya respuesta sin la restricción sea
> distinta.
>
> ### Lo siguiente que corre Cesar
>
> ```
> git pull && cargo install --path crates/thalyx-cli
> thalyx agent model grammar-check     # dos inferencias
> thalyx agent bench                   # las gamas, minutos
> ```
>
> `CLAUDE.md` ya dice siete veces en vez de seis, con esta causa anotada.
>
> ## La primera inferencia real completó, y Thalyx rechazó una respuesta correcta — 2026-08-08
>
> **Éste es el estado actual.** Los dos bloques de abajo son la historia.
>
> Con `llama-completion`, la corrida siguiente en la Fedora de Cesar llegó hasta
> el final: los pesos cargaron y **Qwen2.5-3B emitió exactamente el objeto que
> describe la gramática**, con saltos de línea y sangría —que es lo que
> `ws ::= [ \t\n]*` permite—. Thalyx lo rechazó, y con un mensaje que acusaba a la
> herramienta de ignorar la gramática que acababa de obedecer.
>
> ### Qué estaba mal
>
> `llama.cpp` imprime ` [end of text]` **detrás** del completado cuando el modelo
> para en un token de fin de generación (`tools/completion/completion.cpp`, sólo
> fuera de modo interactivo). `Proposal::parse` era `serde_json::from_str`, que
> rechaza cualquier byte después del objeto.
>
> El marcador aleatorio del prompt decía **dónde empieza** la respuesta. **Nada
> decía dónde termina.** Un límite definido de un solo lado no es un límite: deja
> el final en manos de quien imprimió el texto, y ese final cambia entre versiones.
>
> ### La corrección
>
> `Proposal::completion_in` lee **el primer valor JSON completo** después del
> marcador. La raíz de la gramática es un objeto, así que ahí termina lo que dijo
> el modelo y todo lo demás lo escribió la herramienta — y eso sigue siendo cierto
> con lo que decida imprimir la versión siguiente. Recortar el literal
> ` [end of text]` habría sido la regla 6 al revés. `Proposal::parse` sigue siendo
> estricta: la laxitud vive en un solo sitio, el borde donde otro programa
> imprime.
>
> ### Lo que esto deja probado contra hierro real, y lo que no
>
> | Afirmación | Estado |
> |---|---|
> | Las banderas que Thalyx pasa las acepta esta compilación | **Probado** |
> | Los pesos cargan; el prompt vuelve con el marcador intacto | **Probado** |
> | Vuelve una propuesta bien formada, dentro del plazo | **Probado** (una gama, un enunciado) |
> | `--grammar-file` es lo que restringió esa respuesta | **No probado** — un 3B al que se le pide JSON puede darlo solo |
> | Los números por gama del banco | **No probado**, ninguna gama medida |
>
> ### Reglas nuevas en [[Estrategia-de-Pruebas]]
>
> - **Un límite definido de un solo lado no es un límite.**
> - **Una fixture no puede estar en desacuerdo contigo.** Las nueve de este parser
>   terminaban donde el parser esperaba que terminara una respuesta, porque las
>   escribió la misma mano. La regla 6 ya existía y aquí no se siguió; ahora hay
>   una muestra capturada literal, con su procedencia.
> - **Una comprobación que señala a un culpable tiene que enseñar la evidencia que
>   juzgó.** Es lo único que hizo que esto se viera de una sola lectura, en vez de
>   mandar a auditar el manejo de gramáticas de `llama.cpp` durante días.
>
> ### Pendiente menor
>
> `CLAUDE.md` dice que el instrumento se equivocó **seis** veces; con ésta van
> siete, y las dos últimas por la misma causa. Cambiarlo es decisión de Cesar.
>
> ## El primer `llama.cpp` de verdad: Thalyx pedía el binario que dejó de ser el correcto — 2026-08-08
>
> **El bloque de abajo es el que construyó esto; éste es el que dice dónde está.**
> Cesar lo corrió en su Fedora contra `llama.cpp b1-3653e6d` y
> `Qwen2.5-3B-Instruct-Q4_K_M`, y falló al primer intento — que es exactamente
> para lo que sirve correrlo.
>
> ### Qué pasó
>
> `thalyx agent model check` arrancó `llama-cli`, los pesos cargaron, y entonces
> `llama-cli` **abrió su interfaz conversacional**: sus comandos (`/exit`,
> `/regen`, `/clear`) y el prompt `>`, en vez de completar y terminar.
>
> **No era el GGUF, ni el modelo, ni su máquina.** `llama.cpp` partió sus
> herramientas:
>
> | Binario | Qué es hoy |
> |---|---|
> | `llama-cli` | Frontend de **chat interactivo**, sobre el servidor |
> | `llama-completion` | El completado de **una sola pasada**, con `-f`, `--grammar-file`, `-n`, `--seed` y `--temp` sin cambios |
>
> Con `-f`, el `llama-cli` nuevo abre una sesión sobre el archivo en vez de
> completarlo. Carga, imprime su banner, lee fin de entrada del `stdin` cerrado y
> **sale con cero**. La herramienta equivocada se ve igual que una que funciona y
> dio una mala respuesta.
>
> ### Por qué las pruebas de aquí no lo vieron
>
> Había siete sustitutos y estaban bien escritos: cubrían el recorte de la
> respuesta, el plazo, el desborde, la bandera rechazada. **Todos honraban el
> contrato de una pasada, porque todos estaban escritos para contestar.** Modelé
> el eje del *formato de salida* y el que importaba era el *contrato de
> ejecución*. Regla nueva en [[Estrategia-de-Pruebas]]: la pregunta no es *«¿qué
> puede imprimir esta herramienta?»* sino **«¿qué puede hacer que no sea
> contestar?»**.
>
> ### Y el error se disfrazó justo donde había un respaldo
>
> `answer_in` recortaba después del marcador aleatorio y, **si el marcador no
> estaba, devolvía toda la salida**. Ese respaldo se escribió por una causa —que
> la herramienta no repitiera el prompt— y tenía una segunda que nadie enumeró:
> que la herramienta **nunca leyera el prompt**. Así que el banner del chat entró
> como respuesta, el parser falló, y el mensaje dijo *«el modelo contestó algo que
> no parsea»*: **le echó la culpa a Qwen de una pregunta que nunca se le hizo.**
>
> Segunda regla del mismo hallazgo: un respaldo que cubre una causa cubre en
> silencio todas las que producen la misma señal.
>
> ### Lo que se corrigió, y no es cambiar un nombre
>
> El binario por omisión es `llama-completion`, sí. Pero lo que arregla la clase
> es que **el contrato se comprueba en vez de suponerse**:
>
> - **Se dejó de pasar `--no-display-prompt`.** El eco del prompt lleva el
>   marcador, y el marcador es la prueba positiva de que el prompt se leyó.
>   Suprimirlo borraba la única evidencia — la bandera que hacía cómoda la salida
>   era la que desarmaba la comprobación.
> - **Marcador ausente** → `NotOneShot`, que nombra a `llama-completion` y enseña
>   los primeros 400 bytes de lo que salió en su lugar. No es una respuesta que
>   falta, es una **pregunta** que falta.
> - **Marcador presente y respuesta que no parsea** → `GrammarNotInForce`. No es
>   heurística: un completado restringido por gramática **no puede** producir
>   prosa, así que la prosa demuestra que la gramática no se aplicó.
> - **Y se avisa antes de la primera inferencia**: configurar `llama-cli` saca la
>   advertencia al momento de configurarlo, y `agent model show` la repite — así
>   un store configurado ayer se arregla al revisarlo y no esperando a que falle.
>
> Ninguna de las tres olfatea la prosa de otra herramienta, que sería la regla 6
> otra vez.
>
> ### Qué quedó probado aquí y qué no
>
> **Probado en este contenedor**, con un sustituto que reproduce la conducta del
> `llama-cli` nuevo —carga, banner, comandos, sale con cero sin completar—: que
> eso produce `NotOneShot` y no un fallo de parseo; que una salida vacía es
> contrato roto y no un modelo callado; que un prompt leído con completado vacío
> **sí** es un modelo callado (el control, sin el cual una comprobación que
> rechaza todo pasaría); que la prosa produce `GrammarNotInForce`; y que la
> bandera que desarmaba la comprobación no vuelve por omisión.
>
> **Sigue sin probarse, y lo tiene que cerrar su máquina**: que `llama-completion`
> acepte estas banderas, que tome la gramática, y que la respuesta caiga después
> del marcador. **Ninguna inferencia ha terminado nunca contra pesos reales.**
>
> ### Lo que falta, y es tuyo
>
> ```
> git pull && cargo install --path crates/thalyx-cli
>
> # si no está construido, sale del mismo árbol de llama.cpp:
> #   cmake --build build --target llama-completion
>
> thalyx agent model use media --weights ~/models/qwen2.5-3b-instruct-q4_k_m.gguf
> thalyx agent model check "dev.thalyx.demo, ese quiero"
> ```
>
> Ya no hace falta pasar `--binary`: el valor por omisión es el correcto. Si sale,
> `thalyx agent bench` da la primera tabla de acierto por gama.
>
> **852 pruebas pasan** (847 antes), `clippy` limpio, `cargo fmt` aplicado.
>
> ## El agente tiene modelo: `llama.cpp` como proceso, las cuatro gamas y el banco — 2026-08-08
>
> **La Fase 1 quedó cerrada al 100% y Cesar zanjó también el encuadre de lo que
> sobró**: no es deuda de ninguna fase. Sus palabras, que son el registro:
>
> > no quedó nada de la fase 1, esas cosas que quedaron no pertenecen a ninguna
> > fase real debido a que ninguna bloquea nada, son solo cosas del proyecto que
> > se arreglarán cuando se necesiten arreglar
>
> Eligió seguir con **el modelo del agente**, que era el único `NOT PROVEN` de
> una corrida verde de `verify.sh` y el decreto más grande sin construir.
>
> ### Lo que había, y por qué era el hueco más grande del proyecto
>
> `crates/thalyx-agent/src/model.rs` tenía **dos** implementaciones de `Model`: el
> falso hostil y `UnconfiguredModel`, que contesta *«no model is configured»*. O
> sea que en un sistema operativo donde la IA es ciudadana de primera clase, la
> IA no existía. [[Gamas-de-Modelo]] estaba decretado desde el 2026-08-03 y nadie
> lo había implementado.
>
> ### Lo que se construyó
>
> ```
> thalyx agent model show                         las cuatro gamas, y cuál está puesta
> thalyx agent model use media --weights <gguf>   elige una, y mide el archivo
> thalyx agent model check "<frase>"              una inferencia, con lo que costó
> thalyx agent grammar                            la gramática, para repetirlo a mano
> thalyx agent bench                              el banco que pide el decreto
> ```
>
> `agent plan` y `agent do` usan la gama configurada; sin ninguna configurada,
> siguen exactamente como estaban. Eso último no es cortesía: **una máquina sin
> modelo es una máquina que se puede usar entera**, que es el
> [[Principio-Doble-Ruta]] siendo lo que hace sobrevivible la ausencia del modelo
> en vez de fatal.
>
> ### El defecto que apareció escribiendo el banco, sin correr nada
>
> **La gramática hacía imposible abstenerse.** Pedía al menos un id de módulo, lo
> cual vuelve imposible un contrato mal formado —que es lo que el decreto
> promete— y de paso volvía imposible decir *«no encontré ninguno»*.
>
> [[Gamas-de-Modelo]] dice que la abstención es **la medición que más importa**.
> Con esa gramática, un enunciado ambiguo no tenía respuesta legal salvo inventar:
> el banco habría sacado **0 de 4 en abstención en las cuatro gamas**, y la
> lectura obvia habría sido «los modelos chicos inventan» cuando lo que pasaba es
> que ninguna gama tenía cómo no inventar.
>
> Y no falla nada: todo compila, todo parsea, el banco corre y devuelve números
> plausibles. Regla nueva en [[Estrategia-de-Pruebas]]: **una gramática que fija
> qué se puede decir fija también qué se puede declinar**, y la pregunta que lo
> encuentra no es *«¿acepta las respuestas correctas?»* sino *«¿qué respuestas
> hace imposibles, y alguna era una conducta que quiero medir?»*.
>
> La corrección estaba a la mano y sin nombre: `AgentError::NothingToDo` ya
> existía y ya era la respuesta correcta. Una lista vacía la alcanza. **Y el
> prompt tiene que decirlo** — una respuesta legal que nadie menciona es una que
> el modelo no usa, y la gama quedaría medida sobre una decisión que nunca se le
> ofreció.
>
> ### La regla 6 obligaba a no parsear la salida de `llama.cpp`
>
> Un parser de la salida de otra herramienta necesita **una muestra real
> capturada**, y aquí no hay ninguna ni se puede conseguir. Así que no se parsea
> el formato: el prompt termina en un **marcador aleatorio por invocación** y la
> respuesta es lo que sigue a su última aparición. Sirve si la herramienta repite
> el prompt, si no lo repite, si le pone banderas o si le agrega tiempos.
>
> Aleatorio y no fijo **porque el texto ajeno va dentro del prompt**: un marcador
> fijo es una cadena que un README puede contener, y un README que la contuviera
> estaría eligiendo dónde empieza la respuesta.
>
> Lo que queda sin comprobar es más chico y tiene nombre: **que ese `llama.cpp`
> acepte las banderas**. Por eso las que cambian entre versiones viven en el
> archivo de configuración, no en el código — si una se rechaza, se arregla
> editando una línea y `llama.cpp` sale distinto de cero diciendo cuál.
>
> ### Y un defecto que sólo apareció corriéndolo
>
> `peak rss 0.00 GB`. La unidad estaba fija en GB, así que una medición real de
> dos megabytes se imprimía igual que *«no se pudo medir»* — las dos cosas que
> esa función existe para mantener separadas. Encontrado corriéndolo, no
> leyéndolo, que es la regla 1 otra vez.
>
> ### Lo que NO se hizo, y es lo importante de esta entrada
>
> **Nada de esto ha corrido contra `llama.cpp`.** El contenedor no lo tiene y no
> alcanza los pesos. Lo que sí corrió aquí, contra procesos sustitutos: que la
> respuesta se recorta bien del proceso, que un proceso colgado se mata, que 200
> kB de salida se cortan, que las banderas rechazadas salen con su texto de
> `stderr` íntegro, y el banco entero de nueve casos.
>
> `verify.sh` tiene etapa nueva. Con `THALYX_AGENT_WEIGHTS` apuntando a un GGUF
> corre lo real —incluida la inyección **con un modelo que no es falso de nada**,
> y su control— y sin eso dice `NOT PROVEN` nombrando cuál de las dos mitades
> falta, el binario o los pesos.
>
> ### Lo que falta, y es tuyo
>
> ```
> git pull && cargo install --path crates/thalyx-cli
>
> # baja un GGUF de Qwen2.5-3B-Instruct-Q4_K_M (~2 GB) donde quieras
> thalyx agent model use media --weights ~/models/qwen2.5-3b-instruct-q4_k_m.gguf
> thalyx agent model check "dev.thalyx.demo, ese quiero"
> ```
>
> Ese `check` responde de una vez las dos cosas que aquí no se pueden responder.
> Si sale, `thalyx agent bench` da la primera tabla de acierto por gama que ha
> existido. Y `sudo ./dev/verify.sh` con `THALYX_AGENT_WEIGHTS` puesto corre la
> etapa entera.
>
> **Lo primero que puede fallar es una bandera**, y está bien: sale con su
> mensaje, y se arregla en
> `<store>/state/agent-model.toml`, campo `extra_args`.
>
> **847 pruebas pasan** (802 antes), `clippy` limpio, `cargo fmt` aplicado.
>
> ## Los 40 segundos de arranque eran un puerto serie a 9600 baudios — 2026-08-07
>
> **`nucleo lento` corrió en hierro y contestó al primer intento.** La memoria USB
> no tenía nada que ver.
>
> ```
> 18.27s  at 0.07s
>     after  printk: legacy console [ttyS0] enabled
>     then   ACPI: Core revision 20240827
> ```
>
> **18.27 de 38.5 segundos, en el segundo 0.07**, antes de que el kernel tocara un
> disco. La hipótesis de «la USB es lenta» queda descartada por la **posición** del
> hueco, no por su tamaño: si fuera la memoria, el tiempo estaría al final, donde se
> leen discos — y ahí los huecos miden 0.25 s.
>
> ### La causa
>
> `CONFIG_CMDLINE` decía `console=ttyS0`, **sin velocidad**. Sin velocidad, el
> driver 8250 usa **9600 baudios**, y `printk` es síncrono: el kernel no avanza
> hasta que los caracteres salieron físicamente del puerto. Los 38.5 s son ese
> puerto, en dos mitades:
>
> 1. **El hueco de 18.27 s.** Una consola se registra con `CON_PRINTBUFFER`, así que
>    el kernel le vuelca **todo el log acumulado** en cuanto aparece — unas 250
>    líneas de mapa de memoria y tablas ACPI. A 9600 baudios: `250 × ~70 × 10 ÷ 9600
>    = 18.2 s`. Es el número que salió.
> 2. **Los ~18 s restantes**, repartidos en 704 líneas: 25 ms por línea, que es lo
>    que cuesta cada `printk` posterior por el mismo puerto.
>
> ### Y es la quinta vez que el anfitrión hacía algo gratis
>
> El puerto serie de QEMU es un pty: **no tiene baudios**, se vacía al instante. Las
> cuatro veces anteriores el anfitrión hacía algo que en hierro *no existía*; ésta
> hacía algo que en hierro **existe y cuesta**, que es peor de encontrar — nada
> falta, nada falla, la máquina nada más tarda.
>
> Peor todavía: `run-uefi` y `run-hardware` **no pasan `-append` a propósito**, o
> sea que usan esa misma línea compilada. **La trampa estaba dentro del camino que
> sí se probaba**, y era invisible porque el anfitrión la pagaba.
>
> ### Lo que se decidió, y lo que no
>
> Cesar eligió **darle velocidad en vez de quitarlo**:
> `console=ttyS0,115200 console=tty0`. Doce veces más rápido —los ~30 s se vuelven
> ~2.5 s— y **no se cambia nada por nada**: `run-uefi` y `run-hardware` miran por
> `-serial mon:stdio`, y eso es lo único que hace diagnosticable un arranque que
> muera *antes* de que suba el framebuffer. Quitarlo del todo era más rápido y
> costaba ese diagnóstico.
>
> ### El segundo defecto del mismo arranque, que `config-check` no puede ver
>
> `CPU topo: CPU limit of 2 reached. Ignoring further CPUs`. Nadie eligió 2:
> `allnoconfig` corre con SMP apagado, donde `NR_CPUS` es 1, y encender SMP después
> sólo lo sube al piso de su rango. Puesto **`CONFIG_NR_CPUS=64`**, que es lo que el
> propio kernel usa para SMP x86_64.
>
> **`config-check` compara lo que `thalyx.config` pide contra lo que salió, así que
> una opción que nadie pidió no tiene línea que comparar.** Es el mismo hueco
> estructural que dejó pasar `CONFIG_SECURITY_NETWORK` y `CONFIG_USB_STORAGE`, y no
> se cierra con más comparaciones. Por eso las dos afirmaciones nuevas viven en
> `init.rs`, y las dos se verificaron fallando sin el arreglo.
>
> ### Comprobado en hierro el mismo día, y el número salió exacto
>
> ```
> The kernel talked for 5.7s. The longest silences in it:
>     1.53s  at 0.07s
>         after  printk: legacy console [ttyS0] enabled
>         then   ACPI: Core revision 20240827
> ```
>
> **38.5 s → 5.7 s.** Y lo que lo convierte en prueba y no en mejora es el hueco:
> las mismas dos líneas, en el mismo sitio, **18.27 s → 1.53 s**. Eso es un factor
> de **11.94**, contra el 12.0 que predice `115200 ÷ 9600`. El diagnóstico no
> predijo «va a bajar»; predijo *cuánto*, y bajó eso.
>
> Con las dos medidas se despeja lo que costaba cada parte. Si `T = W + S`, donde
> `W` es trabajo real y `S` el puerto serie, entonces `38.5 = W + S` y
> `5.7 = W + S/12` dan **S = 35.8 s y W = 2.7 s**. O sea que el puerto se llevaba
> **35.8 de los 38.5 segundos** —más de los ~30 que estimé— y la máquina de verdad
> tarda **2.7 segundos** en arrancar. El resto de los 5.7 son los mismos mensajes a
> 115200.
>
> ### Y de paso apareció qué máquina es
>
> `smpboot: CPU0: AMD Ryzen 5 5600G` — seis núcleos, doce hilos. Con `NR_CPUS=2`
> Thalyx estaba tirando diez de los doce. El prompt ahora avisa **7 problemas** en
> vez de 10, que es lo que se espera si la línea de `CPU topo` desapareció, pero
> eso no está confirmado: lo confirma `nucleo`, y no se ha corrido después del
> cambio.
>
> **802 pruebas pasan** (800 antes), `clippy` limpio, `cargo fmt` aplicado.
>
> ## La Fase 1 está cerrada: una PC se instaló Thalyx a sí misma y arrancó sin el medio — 2026-08-07
>
> **El bloque de arriba es más reciente; éste es el que dice dónde está el
> proyecto.** El acto 2b corrió en hierro y salió entero.
>
> ### Lo que pasó, con nombres
>
> Arrancado desde la memoria de 3 GiB, `discos` contestó **tres discos** —no siete—
> y nombró cada uno:
>
> ```
> /dev/sda   447 GiB   3  btrfs `fedora`            ← el sistema de Cesar
> /dev/sdb     3 GiB   1  a Thalyx boot partition   ← el medio
>                      2  a Thalyx store
> /dev/sdc     7 GiB   1  FAT `XBOX`                ← el destino
> ```
>
> `instalar-en /dev/sdc` dijo de dónde salía el kernel —`/dev/sdb1`, 10 671 104
> bytes— **qué había en el destino antes de preguntar** —`1 FAT \`XBOX\``— y pidió
> teclear la ruta. Después:
>
> ```
> ok  kernel     taken off /dev/sdb1
> ok  boot       /dev/sdc1 ▪ the kernel, at the one path a firmware looks for
> ok  store      /dev/sdc2 ▪ labelled `thalyx-store`
> ok  subvolume  system / modules / user
>
> That disk is a Thalyx machine now.
> ```
>
> `apagar`, memoria fuera, encender. Y la máquina arrancó de ese disco:
>
> ```
> 2 disk(s):
>   /dev/sda   447 GiB   3  btrfs `fedora`
>   /dev/sdb     7 GiB   1  a Thalyx boot partition
>                        2  a Thalyx store
> ```
>
> **Un firmware real arrancó un disco físico que Thalyx particionó, formateó y
> escribió él mismo, sin medio puesto, y la máquina encontró su store por la
> etiqueta.** Eso es el criterio de salida.
>
> ### Los cuatro arreglos del día quedaron confirmados en vivo
>
> Ninguno se probó en una VM primero, y los cuatro se comportaron:
>
> - **`3 disk(s)` y no siete** — el filtro de particiones. La Fedora dejó de
>   aparecer partida en pedazos ofrecibles.
> - **`a Thalyx boot partition`** — la máquina nombra su propio trabajo.
> - **`it has 2 partition(s) on it now: 1 FAT \`XBOX\``** — el guardián nuevo, dicho
>   antes de la pregunta y no después.
> - **`! 9 new kernel problems; \`nucleo\` shows them`** — el aviso del prompt, y en
>   ninguno de estos arranques el `-110` del USB volvió a pisar una línea.
>
> Y un detalle que confirma el escritor de GPT: **el disco instalado no produce el
> aviso de «alternate GPT header not at the end»** que sí produce el medio. El medio
> lo tiene porque se hizo con `dd` de una imagen más chica que la memoria; el
> instalado no, porque Thalyx escribió la tabla contra el tamaño real del
> dispositivo.
>
> ### Lo que no se ejerció, que no es lo mismo que un hueco de la Fase 1
>
> **Zanjado por Cesar el 2026-08-08: la Fase 1 está cerrada al 100%, sin
> asteriscos.** El criterio que él decretó no nombra NVMe ni disco interno; llamar
> «huecos» a configuraciones de hardware no ejercidas les daba el peso de una
> cláusula incumplida, y eso fue un error de encuadre. Ver
> [[Criterio-de-Salida-Fase-1]], donde queda el razonamiento completo.
>
> Sigue siendo cierto y pasa a la fase de validación:
>
> 1. **Ningún disco interno ha recibido una instalación.** El destino fue removible.
>    El camino del instalador es idéntico —sysfs, `BLKRRPART`, los mismos
>    escritores—; lo que cambia es el bus. **No es una imposibilidad**: instalar al
>    lado de Fedora lo cerraría. Se aplaza por decisión.
> 2. **NVMe sobre silicio real sigue sin ejercerse**, porque esa máquina no tiene
>    ninguno y no se va a comprar uno. No es un driver que falle: es hardware que no
>    existe ahí. Esto sí es una imposibilidad.
>
> ### Lo que abrió, y es una pregunta de Cesar
>
> **La máquina tarda ~40 segundos en llegar al prompt**, más que su Fedora. Los
> tiempos del kernel dicen dónde no está el problema: `sd ... [sdb]` aparece a los
> **38 s** y el mensaje de `struct module` a los **34 s**, y esos números son
> **iguales desde dos memorias distintas**. Un tiempo constante entre medios
> distintos es lo que hace un **plazo fijo**, no lo que hace una lectura lenta — así
> que la hipótesis de «la USB es lenta» explica, como mucho, lo que tarda el
> firmware antes de que el kernel empiece a contar.
>
> **Y no había con qué medirlo.** `nucleo` contesta cuatro líneas de problemas o
> setecientas de todo, y ninguna de las dos dice a dónde se fueron los 34 segundos.
> Construido **`nucleo lento`**: los silencios más largos entre mensajes
> consecutivos, con la línea de antes y la de después de cada uno. El kernel ya
> ponía la marca de tiempo en cada línea; nadie las había restado.
>
> **La causa no está determinada y no se va a adivinar.** Un hueco dice a dónde se
> fue el tiempo, no qué lo tomó.
>
> ### Lo que falta, y es un arranque
>
> ```
> git pull && make -C image && sudo make -C image installed INSTALLEDSIZE=2G
> ```
>
> `dd` a la memoria, arrancar, y teclear **`nucleo lento`**.
>
> > **Contestado el mismo día — ver el bloque de arriba.** Era el puerto serie a
> > 9600 baudios, no la memoria. Y la razón por la que la constancia entre dos
> > memorias apuntaba a un plazo fijo era correcta en la forma y equivocada en el
> > sitio: el tiempo constante estaba al **principio**, no al final.
>
> **800 pruebas pasan** (797 antes), `clippy` limpio, `cargo fmt` aplicado.
>
> ## `discos` corrió en hierro y ofrecía la Fedora de Cesar como destino — 2026-08-07
>
> **El bloque de arriba es más reciente.** Segundo arranque en la PC real, con el arreglo
> de la consola puesto. El error del USB **no volvió** —`nucleo` lista 10 líneas de
> problema entre 714 registros y ninguna es del USB— y el prompt no anunció nada,
> que es lo correcto: no hubo problemas nuevos después de que arrancó la sesión.
>
> **Que no volviera no es que esté arreglado, y ahora consta cuál de las dos es.**
> Cesar confirmó que **nunca desconectó nada**: el receptor Telink estaba puesto en
> esa corrida. O sea que **el `-110` es intermitente y sigue vivo** — no ocurrió esa
> vez, puede ocurrir la próxima.
>
> Y hubo que deshacer una confusión antes de poder concluirlo: el Telink **no es el
> WiFi**. Son dos dispositivos distintos en el mismo bus, y su `lsusb -t` los
> separa — puerto 6 es `usbhid` (el receptor de teclado y ratón) y puerto 8 es
> `rtl8xxxu`, que ése sí es el WiFi. Preguntar *«¿estaba conectado el Telink?»* sin
> decir cuál de los dos era invitaba justo a esa respuesta.
>
> **Lo que sí quedó resuelto es el síntoma, que era el que impedía usar la máquina.**
> Con la consola en emergencias, el `-110` ya no pisa el prompt: si vuelve, la
> sesión dirá `! N new kernel problem(s)` y seguirá siendo usable. La causa sigue
> abierta y **ninguna opción de kernel está justificada** — un fallo intermitente
> que aparece en un arranque y no en el siguiente se parece mucho más a un
> dispositivo marginal que a un hueco de `thalyx.config`.
>
> ### Lo que `discos` respondió, y es lo bueno primero
>
> ```
> 7 disk(s):
>   /dev/sda    3 GiB, 2 partition(s)     ← la memoria USB
>   /dev/sdb  447 GiB, 3 partition(s)
>     3  btrfs `fedora`                   ← su sistema
> ```
>
> - **AHCI y SATA quedaron probados en hierro.** `sd 9:0:0:0: [sda]` y un disco de
>   447 GiB con la Fedora adentro: eso es `SATA_AHCI` + `SCSI` + `BLK_DEV_SD`
>   funcionando contra silicio real. Era una de las tres filas de la tabla de
>   riesgo.
> - **Esa máquina no tiene NVMe.** Siete discos y ninguno es `nvme0n1`; su sistema
>   vive en un SATA. Así que **NVMe sobre silicio real es incontestable en esta
>   máquina** — no porque el driver falle, sino porque no hay hardware. Queda dicho
>   en vez de contarse como probado.
>
> ### Y el defecto, que es el más peligroso encontrado hasta ahora
>
> **Cuatro de esos «siete discos» son particiones**, incluidos los 444 GiB de
> `/dev/sdb3` —la Fedora— listados bajo la línea *«`instalar-en <disco>` puts
> Thalyx on one. Everything on it is lost.»*
>
> El filtro existía y su comentario decía que `partitions::of` *«errors for a
> partition»*. **No erraba.** Busca `/sys/dev/block/<major>:<minor>`, que existe
> igual para las dos, `read_dir` funciona sobre una partición, y como no tiene
> hijos con archivo `partition` devolvía **`Ok([])`**. *«No tiene particiones»* y
> *«esto no es la clase de cosa que tiene particiones»* salían por el mismo canal,
> y el llamador sólo miraba `is_ok()`.
>
> Y `install` las habría aceptado: escribe la tabla en el LBA 0 de lo que reciba.
> Una tabla dentro de una partición es **legal, invisible, y no arranca nada**,
> mientras el sistema de archivos que había ahí ya no está — la misma forma que la
> GPT con suma equivocada, donde el fallo no llega y el disco vuelve pareciendo
> intacto.
>
> **Arreglado en los dos sitios**, que es lo que importa: `discos` deja de
> listarlas —presentación— e **`install` se niega antes de escribir un byte**, que
> es lo que impide perder un disco cuando alguien teclea el nombre igual. El
> discriminador es el que el kernel ya tenía y nadie le preguntó: el archivo
> `partition`. Tres pruebas, incluida una que le pregunta al kernel que corre las
> pruebas si el modelo es correcto, porque un falso que modela la propiedad
> equivocada no es un falso sino otro sistema.
>
> Regla nueva en [[Estrategia-de-Pruebas]], y una segunda dentro de ella: **un
> comentario que enuncia una propiedad es una prueba que nunca corre.** Lo único
> capaz de contradecir esa frase era una máquina con particiones, que durante
> semanas fue ninguna máquina.
>
> ### Y Thalyx no reconocía su propia partición de arranque
>
> `discos` describía la ESP de la memoria de la que estaba corriendo como
> *«something I do not recognise»*, teniendo un lector de FAT32 adentro. Ahora dice
> **«a Thalyx boot partition»**, y una FAT ajena la nombra con su etiqueta. Una
> máquina que no sabe nombrar su propio trabajo no tiene autoridad para nombrar el
> ajeno.
>
> ### Dos cosas del `nucleo` que no son defectos y conviene no confundir
>
> - **`GPT: 4194303 != 7831551`.** La imagen es de 2 GiB y la memoria de ~3.7 GiB,
>   así que la copia de respaldo de la tabla quedó donde termina la imagen y no
>   donde termina el dispositivo. Le pasa a **toda** imagen escrita con `dd` a un
>   medio más grande. Linux avisa y sigue.
> - **`CPU limit of 2 reached`.** `allnoconfig` deja `NR_CPUS` en 2 y esa máquina
>   tiene más. No rompe nada; explica parte de la lentitud del arranque.
>
> ### Lo que falta para cerrar la Fase 1, y ya no es imposible
>
> **Falta el acto 2b**, y el criterio de Cesar es *«ponerla en una PC sin sistema
> operativo y que ahora tenga Thalyx como OS»*: la máquina tiene que **tener**
> Thalyx en un disco propio, con el medio quitado. En hierro eso nunca ha pasado.
>
> Pero tiene **tres memorias** (4, 8 y 32 GB), y eso vuelve alcanzable hoy lo que
> parecía imposible: **arrancar de la memoria A e `instalar-en` la memoria B**,
> quitar A, y encender. Firmware real, instalación real escribiendo una GPT real en
> un disco físico real, y un arranque real desde ese disco sin medio puesto. Lo
> único que no responde es que el disco sea interno. Si eso cumple el decreto lo
> decide Cesar, porque el decreto es suyo.
>
> **797 pruebas pasan** (794 antes), `clippy` limpio, `cargo fmt` aplicado.
>
> ## El dispositivo tiene nombre, y el prompt ya no se pisa — 2026-08-07
>
> **El bloque de arriba es más reciente.**
>
> ### Qué es `usb 1-6`
>
> `lsusb -t` y `dmesg` en Fedora lo contestaron sin arrancar nada:
>
> ```
> usb 1-6: New USB device found, idVendor=248a, idProduct=16ab
> usb 1-6: Product: Wireless Receiver
> usb 1-6: Manufacturer: Telink
> ```
>
> Un **receptor inalámbrico Telink** de teclado y ratón, a *full speed* (12M), con
> dos interfaces HID. En Fedora enumera a los 1.2 s; en Thalyx agota el plazo.
>
> **Y no es el teclado con el que Cesar escribió**: hay otro HID de dos interfaces
> en el bus 3, puerto 1. Por eso pudo teclear `apagar`. Desconectar el receptor
> Telink es a la vez el atajo para usar la máquina hoy **y el control** que
> confirma que ése era el dispositivo.
>
> **No se agregó ninguna opción de kernel.** Todavía no está descartado que el
> dongle sea lento o defectuoso, y una opción agregada por corazonada es lo
> contrario de lo que hace este proyecto. Lo que faltaba era el instrumento, y
> ahora existe: con el prompt utilizable, `nucleo` muestra el buffer entero —
> incluidas todas las líneas de nivel informativo de la enumeración USB que la
> consola nunca imprimió— y eso sí dice si el kernel reintentó, cuántas veces, y
> qué estaba haciendo en los 38 segundos previos.
>
> ### El arreglo de la consola, que son dos mitades
>
> **La consola queda en emergencias (`set_console_loglevel(1)`)**, y **el prompt
> anuncia lo que llegó**:
>
> ```
>   !  2 new kernel problem(s); `nucleo` shows them
>   >
> ```
>
> La segunda mitad es lo que impide que la primera sea esconder. Se imprime
> **antes** del prompt y nunca a media línea, que es el defecto entero.
>
> Para saber *«qué ha dicho el kernel desde que miré»* hacía falta un cursor, y
> contar registros no sirve: el buffer sobrescribe los viejos, así que la cuenta
> puede **bajar** mientras llegan mensajes. Ahora `KernelMessage` lleva el número
> de secuencia del kernel, que sólo sube. Comprobado contra el `/dev/kmsg` real de
> este contenedor: 358 registros, monótono, y distinto del campo de tiempo — que es
> el error que se habría visto igual de bien.
>
> Y si el buffer se dio la vuelta entre dos miradas, el aviso dice **«at least»**
> en vez de presentar un subconteo como total.
>
> **Cinco pruebas nuevas**, incluida la de control: un mensaje que sólo *ocurrió*
> no interrumpe el prompt. Sin ella, un aviso que aparece siempre es uno que nadie
> lee, que es el mismo defecto reconstruido un nivel más arriba.
>
> ### Y el umbral estaba sobre el eje equivocado
>
> `init.rs` describía el síntoma **antes de que ocurriera** —*«a message arriving
> mid-line steps on it — the machine looks like it stopped listening»*— y aun así
> ocurrió. Filtrar por gravedad contesta *«¿esto importa?»*; lo que arruina una
> interfaz no es que un mensaje importe, es que **vuelva**. Y la repetición no es
> una propiedad del mensaje sino de la serie, así que ningún nivel la ve.
>
> Segundo defecto del mismo hilo: el nivel de consola suprime lo que tenga
> prioridad **>= él**, así que el 4 tiraba las advertencias, mientras
> `is_trouble()` las cuenta **como** problema y la línea del arranque decía
> *«warnings and worse only»*. Un mismo juicio en dos lugares se desincronizó en
> silencio, y hacia el lado peor: la pantalla afirmaba mostrar más de lo que
> mostraba. Las dos reglas están en [[Estrategia-de-Pruebas]].
>
> ### Lo que falta, y es tuyo
>
> ```
> git pull
> make -C image
> sudo make -C image installed INSTALLEDSIZE=2G
> ```
>
> `dd` a la memoria otra vez (paso 4 de [[Arranque-en-Hierro]]), arrancar, y ahora
> sí: **`nucleo`** —que es lo que dice qué pasó con el USB— y **`discos`**, que
> nunca se ha corrido en hierro y que diría si el NVMe y el `sda` aparecen.
>
> **794 pruebas pasan** (789 antes), `clippy` limpio, `cargo fmt` aplicado.
>
> ## Una PC de verdad arrancó Thalyx desde una USB de verdad — 2026-08-07
>
> **El bloque de arriba es más reciente.** El acto 2a corrió. Firmware real, monitor
> real por HDMI, memoria física, teclado físico. Lo que salió en la pantalla:
>
> ```
> [ 38.075277] BTRFS: device label thalyx-store devid 1 transid 2 /dev/sdb2 (8:18) scanned by init (1)
> ok  store       /dev/sdb2 ▪ three subvolumes, found by the label `thalyx-store`
> ok  thalyx-lsm  2 hook(s) live, 3 map(s) pinned under /sys/fs/bpf/thalyx
> ok  enforcement 2 of 2 hook(s) live: thalyx_socket_c, thalyx_file_ope
> ```
>
> Cada línea es una afirmación distinta, y **ninguna la podía hacer una VM**:
>
> - **Un firmware real arrancó Thalyx de una memoria física**, por el
>   *fallback* `\EFI\BOOT\BOOTX64.EFI`, sin gestor de arranque.
> - **La pantalla funcionó en hierro.** `FB_EFI` adoptó el framebuffer que dejó
>   *su* firmware, en un monitor por HDMI. Era la única parte de la pantalla que
>   una VM no respondía, y es el punto 3 de la lista de riesgo de
>   [[Construccion-del-ISO]] — *«arrancaría bien y no se vería nada»*.
> - **`USB_STORAGE` funcionó en hierro**: la memoria salió como `/dev/sdb2`,
>   major 8:18, o sea la capa SCSI de verdad. La línea que faltaba esa mañana.
> - **Encontró su store por la etiqueta**, sin `thalyx.store=`.
> - **El LSM se enganchó**, en un arranque por firmware sobre hardware real.
> - **Y el teclado funcionó.** Cesar tecleó `apagar` y la máquina se apagó.
>
> Ese último punto lo estableció **él, con el control correcto**: volvió a
> arrancar sólo para separar *«el teclado no sirve»* de *«algo me impide
> teclear»*, tecleó antes de que apareciera el error, y funcionó. Es la regla 4 —
> una negativa sin línea base y sin control no dice nada — aplicada por el humano
> sin que nadie se la pidiera.
>
> **Hay un `sda` además del `sdb`**, así que esa máquina tiene otro disco SCSI —
> probablemente SATA por AHCI. `discos` lo habría dicho y no se alcanzó a correr.
>
> ### Y encontró un defecto real, que es para lo que sirve correr las cosas
>
> ```
> > [ 51.812474] usb 1-6: device descriptor read/64, error -110
> ```
>
> `-110` es `ETIMEDOUT`: un dispositivo USB en el bus 1, puerto 6, cuyo descriptor
> no se puede leer. El kernel **reintenta para siempre**, así que el mensaje
> vuelve cada pocos segundos, encima del prompt. La sesión queda inusable aunque
> el teclado funcione. Y los 38 segundos hasta encontrar el store son el mismo
> síntoma: la enumeración se pasó ese tiempo agotando plazos.
>
> **Lo notable es que el código ya había previsto exactamente esto** y eligió el
> umbral equivocado por uno. `init.rs:393` dice, textual:
>
> > *From here there is a human at a prompt, and an info-level message arriving
> > mid-line steps on it — the machine looks like it stopped listening.*
>
> Y baja la consola a `4`. El razonamiento vale para un error que ocurre **una
> vez**; no vale para uno que se repite sin parar. Un mensaje que se repite deja
> de ser información y es ruido, y el umbral no distingue las dos cosas porque
> mira la gravedad y no la repetición.
>
> **Y el mensaje del arranque miente por un nivel.** `set_console_loglevel(4)`
> suprime todo lo que tenga prioridad `>= 4`, o sea **las advertencias se van** —
> pero la línea dice *«warnings and worse only»* y el comentario dice *«warnings
> and errors still come through»*. Mientras tanto `is_trouble()` cuenta la
> prioridad 4 **como** problema, así que `nucleo` las llama problemas y la consola
> las tira. Las dos mitades del mismo criterio no coinciden.
>
> ### Lo que falta, y son dos preguntas distintas que no hay que mezclar
>
> 1. **Qué es `usb 1-6`.** Se contesta desde Fedora, gratis y sin riesgo:
>    `lsusb -t` y `dmesg | grep -i "1-6"`. Hasta saberlo, **no se agrega ninguna
>    opción de kernel**: sería adivinar, y este proyecto tiene una regla sobre
>    creerle a un instrumento antes de descartar al que preguntó.
> 2. **Cómo sobrevive la sesión al ruido del kernel.** Es decisión de Cesar y
>    está en [[Tareas-Pendientes]].
>
> ## Los cuatro grupos de controladores corrieron, y el acto 2 se parte en dos — 2026-08-07
>
> **El bloque de arriba es más reciente.** Cesar corrió `run-hardware` entero. Lo que
> devolvió la pantalla, con nombres:
>
> ```
> hid-generic 0003:0627:0001.0001: input: USB HID v1.11 Keyboard [QEMU QEMU USB Keyboard] on usb-0000:00:03
> BTRFS: device label thalyx-store devid 1 transid 2 /dev/nvme0n1p2 (259:2)
> ok  store   /dev/nvme0n1p2 ▪ three subvolumes, found by the label `thalyx-store`
> ```
>
> - **El teclado USB enlazó**: `xhci_hcd` enumeró el dispositivo y `hid-generic`
>   creó un dispositivo de entrada.
> - **El NVMe enlazó y las particiones se llaman bien** — `nvme0n1p2`, major 259.
>   Era el riesgo más caro que cargaba el acto 2, el que hace que `partitions.rs`
>   lea los nombres de sysfs en vez de derivarlos.
> - **Arrancó del NVMe sin medio puesto**: encontró **un solo** `thalyx-store`; con
>   la USB conectada habría encontrado dos y se habría negado.
> - Y de ahí se sigue lo que la pantalla no dice: para que ese store exista,
>   `instalar-en` tuvo que leer el kernel del medio USB, así que **`USB_STORAGE`
>   también funcionó**. La línea que faltaba esa misma mañana.
>
> **Lo que le queda al acto 2 ya no es «¿Thalyx tiene los drivers?».** Es silicio
> concreto.
>
> ### Y la restricción real, que cambia la forma del acto 2
>
> Cesar tiene **una sola PC** y no va a tener otra: *«no puedo hacerla ni hoy ni
> nunca, no tengo una pc limpia»*. Sí tiene memorias USB (4, 8 y 32 GB). Eso parte
> el acto 2 en dos mitades de costo muy distinto:
>
> - **Arrancar desde la USB en su propia máquina** responde firmware real, xHCI
>   real, su teclado real, `USB_STORAGE` real y su NVMe real visto por el driver
>   real — **y no escribe un solo byte** en el disco interno.
> - **Instalar en el disco interno destruiría Fedora.** `thalyx install` escribe una
>   GPT nueva sobre el disco entero; el módulo abre diciendo *«turning a disk with
>   no operating system on it»* y es literal. Instalar al lado **no está construido**
>   y es un decreto que Cesar no ha tomado. Se puede hacer sin romper la filosofía
>   —particiones en el espacio libre, el kernel en la ESP existente bajo
>   `\EFI\thalyx\`, y el **menú del firmware** eligiendo, que es el firmware y no un
>   segundo programa— pero lo que cuesta no es el código: es escribir en el disco
>   que sostiene la única máquina que verifica este proyecto.
>
> El procedimiento entero está en [[Arranque-en-Hierro]], escrito para contestarse
> desde sí mismo: 642 MiB es el mínimo instalable, así que 2 GiB de imagen basta y
> entra en cualquiera de las tres memorias. Lleva el aviso que importa —**`discos`
> va a listar el NVMe con Fedora adentro y `instalar-en` no se teclea**— y el paso
> de Secure Boot, que es lo más probable que lo detenga y no es Thalyx fallando.
>
> ## El acto 2 habría fallado, y se supo sin correrlo — 2026-08-07
>
> **El bloque de arriba es más reciente.** Cesar preguntó si GNOME Boxes servía para el
> acto 2, porque no tiene una segunda PC. Buscar la respuesta encontró un defecto
> que habría aparecido con la memoria USB ya puesta en una máquina.
>
> ### `CONFIG_USB_STORAGE` no estaba, y su ausencia no rompe el arranque
>
> El acto 2 es `dd` a una USB, arrancar, `discos`, `instalar-en /dev/nvme0n1`. Sin
> ese driver:
>
> - La máquina **arranca de la USB perfectamente**, porque la especificación UEFI
>   obliga al **firmware** a leer el medio con su propio controlador. Monta sus
>   siete sistemas de archivos, engancha el LSM, saca su prompt en la pantalla.
> - Y falla **dos comandos después**: `instalar-en` busca el medio del que arrancó
>   recorriendo `/sys/block` (`partitions.rs:189`), y el kernel enumeró la USB como
>   dispositivo USB sin darle nunca un dispositivo de bloque.
> - El mensaje sería *«no encuentro un medio de Thalyx»* en una máquina que está
>   visiblemente corriendo desde uno. **En ningún punto aparece la palabra USB.**
>
> Es la **cuarta vez** que algo de fuera hacía un trabajo que el diseño nunca
> escribió —systemd con los controladores de cgroup, el initramfs externo con el
> `switch_root`, el archivo del kernel con `/dev/console`— y la primera en que la
> capa de abajo **no se quita**: el firmware sigue ahí haciendo su parte, y por eso
> el arranque no se rompe y el defecto es invisible. Regla nueva en
> [[Estrategia-de-Pruebas]], con la pregunta que sí lo encuentra: no *«¿qué hardware
> tiene la PC?»* sino *«¿qué tiene que leer Thalyx además de lo que el firmware ya
> leyó por él?»*. Un inventario de hardware no la contiene, que es por qué no estaba
> en las tres filas de la tabla de riesgo.
>
> Ya está puesta, con **una quinta prueba** que lee `thalyx.config` y la exige,
> comprobada en las dos direcciones —comentando la línea, falla—, porque
> `config-check` atrapa una opción que Kconfig descartó y **no puede atrapar una que
> nadie pidió**.
>
> ### Y Boxes no sirve, pero QEMU sí sirve para más de lo que decía la bóveda
>
> Boxes da discos virtio y teclado PS/2 — o sea lo que el acto 1 ya probó — y no
> tiene interfaz para agregar otros controladores. Pero **QEMU emula xHCI, NVMe,
> AHCI y un disco USB**, y el driver del kernel que habla con un controlador emulado
> es el mismo que habla con silicio real. La bóveda decía «una VM no prueba los
> controladores», que era cierto de la VM que se estaba usando y no de toda VM.
> Corregido en [[Construccion-del-ISO]] con la tabla de qué responde cada cosa.
>
> **`make -C image run-hardware`**, con tres modos:
>
> ```
> make -C image run-hardware              arranca del medio USB; adentro, `discos`
>                                         y `instalar-en /dev/nvme0n1`
> make -C image run-hardware NOMEDIUM=1   la misma máquina sin la USB, que ahora
>                                         tiene que arrancar del NVMe que instaló
> make -C image run-hardware NOPS2=1      sin controlador PS/2, así que una tecla
>                                         que llegue sólo pudo venir por USB
> ```
>
> Los dos discos en blanco se hacen una vez y se conservan: un disco que se borra
> en cada corrida no puede mostrar que la instalación siguió ahí.
>
> **No es el acto 2 y no lo cierra**, y el objetivo lo dice línea por línea. Lo que
> hace es mover el riesgo de cuatro grupos de controladores nunca ejercidos a cuatro
> ejercidos contra controladores emulados, dejando abierto lo que de verdad pide una
> PC: silicio real y una memoria física en un puerto físico.
>
> ### Lo que falta, y es tuyo
>
> ```
> git pull
> make -C image                          # aquí se sabe si USB_STORAGE sobrevivió
> sudo make -C image installed
> make -C image run-hardware
> ```
>
> Adentro: `discos` tiene que listar `/dev/nvme0n1`, `/dev/sda` y el medio. Después
> `instalar-en /dev/nvme0n1`, `apagar`, y `make -C image run-hardware NOMEDIUM=1`.
>
> **Lo primero que puede fallar sigue siendo la compilación del kernel**, y está
> bien: `config-check` detiene el build si `olddefconfig` descartó `USB_STORAGE`.
> Depende de `USB` y `SCSI` y las dos ya estaban, así que no debería — pero si pasa,
> la lectura es la de `HID_SUPPORT`: buscar el `menuconfig` que la contiene antes
> que sus dependencias.
>
> **789 pruebas pasan** (788 antes de este cambio), `clippy` limpio en 1.97,
> `cargo fmt` aplicado. El bloque de abajo sigue siendo el estado del acto 1.
>
> ## El acto 1 está hecho: una máquina instalada arrancó sola y respondió — 2026-08-07
>
> **El bloque de arriba es más reciente.** `sudo ./dev/verify.sh` cerró en
> **`proven 135 · not proven 1 · failed 0`** —el único no probado es llama.cpp, que
> es Fase 2— y `make -C image run-installed` arrancó la máquina instalada.
>
> Un firmware UEFI encontró `\EFI\BOOT\BOOTX64.EFI` en un disco escrito por Thalyx y
> lo ejecutó: sin `-kernel`, sin `-append`, sin gestor de arranque. La máquina
> encontró su store sin que nadie se lo nombrara, la sesión salió **por la pantalla**,
> Cesar escribió `apagar` **dentro de la ventana** y se apagó.
>
> Eso último no es un detalle: el teclado entró por PS/2 emulado (`SERIO_I8042` +
> `KEYBOARD_ATKBD` + `VT`) y la pantalla es `FB_EFI` + `FRAMEBUFFER_CONSOLE` +
> `FONT_8x16`. Lo confirma algo que no se puede fingir: al intentar Impr Pant
> aparecieron símbolos raros en la sesión, que es `atkbd` traduciendo scancodes de una
> tecla que no es una letra.
>
> **Falta sólo el acto 2, y es hierro**: `dd` a una USB, arrancar una PC, `discos`,
> `instalar-en /dev/nvme0n1`, `apagar`, sacar la USB, encender. Es lo único que
> responde el teclado **USB** (xHCI + HID) y los discos **NVMe/AHCI**. De los tres
> grupos de controladores nuevos, dos ya están probados en vivo.
>
> Ver [[Criterio-de-Salida-Fase-1]], que lleva el detalle de qué afirmó cada cosa.
>
> ## La Fase 1 está construida entera, y la primera corrida encontró dos cosas — 2026-08-07
>
> **Es lo primero que hay que leer.** Cesar pidió cerrar la fase sin poder verificar
> entre cambios, aceptando apilar comprobaciones: *«no importa que apilemos 2
> comprobaciones, nuestro verify.sh nos indica dónde están, no es necesario saber en
> qué momento se introdujeron»*. Así que hubo **cuatro commits del día** y una sola
> corrida por delante — y la apuesta salió como se dijo: el arnés nombró las dos
> cosas rotas, sin que hiciera falta saber cuál commit las metió.
>
> ### Lo que devolvió esa corrida, y el segundo es serio
>
> **1. El kernel no compiló: faltaba `CONFIG_HID_SUPPORT`.** `config-check` nombró
> tres opciones descartadas —`HID`, `HID_GENERIC`, `USB_HID`— y ninguna era la causa:
> las tres viven dentro de un `menuconfig HID_SUPPORT` que es `default y`, y bajo
> `allnoconfig` un `default y` es un `n`. Una línea.
>
> **2. El instalador copió el gestor de arranque de Fedora — y tenía dos causas, no
> una.** La primera corrida en frío encontró una y la segunda encontró la otra, que
> era la que estaba produciendo el mensaje.
>
> **2a. El arnés destruía la partición y no la reparaba.** La etapa 20 daña las dos
> copias del sector de arranque de la ESP para comprobar que un vfat roto no se
> monta —regla 4, bien aplicada— y **la dejaba dañada**. Todo lo de abajo la sigue
> usando. Cinco fallos de una sola causa, y el primero mandaba a mirar el lector de
> FAT, que estaba bien. Ahora el control saca los siete sectores antes, los devuelve
> después, y **afirma que la reparación tomó** — porque una reparación que
> silenciosamente no funciona se ve idéntica al bug original. Séptima vez que *el
> instrumento incluye al arnés*, y la primera en que el arnés no midió mal: dejó el
> mundo peor de como lo encontró.
>
> **2b. Y la búsqueda del medio.** La búsqueda del medio
> pedía `\EFI\BOOT\BOOTX64.EFI`, que **no es un archivo de Thalyx**: es el
> *removable media fallback* de UEFI, o sea la ruta que llevan todos los medios de
> arranque que existen, empezando por la partición EFI de la máquina en la que uno
> está sentado. La etapa 20 instaló un segundo disco sin `--kernel`, la búsqueda
> encontró tu ESP, y Thalyx copió el arranque de otro sistema al disco **reportando
> una instalación correcta**. Lo único que lo dijo fue la comparación byte a byte del
> final.
>
> Ahora el medio se identifica por la **etiqueta del volumen FAT32, `THALYX`**, que
> sí la escribe Thalyx — el mismo cambio que el store por su etiqueta, un día tarde.
> `thalyx disk medium` contesta a qué disco iría a buscar el kernel, y la etapa 20 lo
> usa como afirmación *y* como control: tu máquina tiene una ESP propia, así que
> pasar quiere decir las dos cosas, que encontró el volumen de Thalyx y que no se
> llevó el ajeno. Regla nueva en [[Estrategia-de-Pruebas]]: *un marcador que
> identifica algo tiene que ser algo que sólo eso tenga*.
>
> Sin la etiqueta, aun con el medio sano, habría **dos** respuestas y la instalación
> se habría negado — correcta, pero imposible en la máquina de cualquiera. O sea que
> las dos correcciones hacían falta y ninguna cubría a la otra.
>
> Y un detalle del arnés: el `cmp` que falló decía sólo «difieren», que manda a mirar
> el lector de FAT. Ahora imprime de qué dispositivo dijo el instalador que estaba
> leyendo, y el tamaño de los dos archivos.
>
> ### Al ir a cerrar apareció que faltaba algo grande, y era un decreto sin código
>
> **Una máquina instalada no encontraba su store.** Decretado el 2026-08-06 —*por la
> etiqueta del sistema de archivos*—, escrito con su razonamiento entero, marcado
> `[x]` en [[Tareas-Pendientes]], y **nunca implementado**. `store_disk.rs` leía
> `thalyx.store=` y, sin él, decía que nadie le había dicho cuál era el disco.
>
> En una máquina instalada nunca está: la línea de comandos va compilada dentro del
> kernel, es una sola, y el disco se llama `vda` aquí y `nvme0n1p2` en una PC.
>
> O sea: **el instalador de esta mañana estaba terminado y el disco que produce
> habría arrancado diciendo que no tiene store.** Nada lo habría dicho antes de que
> encendieras la máquina. Regla nueva en [[Estrategia-de-Pruebas]]: un decreto que
> nadie implementó se lee igual que uno implementado, y un `[x]` de *decidido* se ve
> igual que uno de *construido*.
>
> Ya está construido, con los dos caminos en orden: **`thalyx.store=` gana** —lo que
> deja `make run` y todas las etapas exactamente como estaban— y sin él se le
> pregunta a cada disco cómo se llama. Con las dos negativas: **ninguno** se reporta
> diciendo cuántos discos se leyeron, y **dos se niegan** en vez de elegir. Lo
> segundo no es raro: es el caso normal justo después de instalar, con el medio
> todavía puesto.
>
> `thalyx disk find` corre ese código sin ser PID 1 y sin montar nada. Existe porque
> si no, la rama que niega dos discos iguales se ejecutaría por primera vez en tu
> máquina, el día en que equivocarse es más caro.
>
> ### Y la otra mitad: la máquina se instala a sí misma
>
> `thalyx install` recibía el kernel por `--kernel`, y **adentro no hay ruta que
> teclear**. Ahora Thalyx **lee** el medio del que arrancó: `medium.rs` es un lector
> de FAT32 que busca un volumen etiquetado `THALYX` con `\EFI\BOOT\BOOTX64.EFI`
> adentro, se niega si hay dos, y excluye el disco de destino para que reinstalar
> siga siendo posible.
> **No monta nada** — los bytes se leen igual que se escribieron, así que el kernel
> no necesita `CONFIG_VFAT_FS`.
>
> Y dos verbos, que es lo que lo vuelve alcanzable sin shell:
>
> ```
>   discos                 qué discos veo, y qué tiene cada uno
>   instalar-en <disco>    pon esta máquina en ese disco
> ```
>
> El kernel se busca **antes** de decir nada sobre destruir el disco: una máquina que
> preguntara, recibiera un sí, borrara el disco y sólo entonces descubriera que no
> tenía kernel lo habría destruido para nada.
>
> ### El medio y la máquina instalada son el mismo archivo
>
> No hay un ISO aparte que construir. Un disco con una partición de arranque que un
> firmware puede iniciar es eso mismo, esté atornillado a una máquina o enchufado en
> un costado:
>
> ```
> sudo dd if=image/build/installed.img of=/dev/sdX bs=4M status=progress conv=fsync
> ```
>
> ### Lo que falta, y es todo tuyo
>
> **Acto 1, en una VM** — responde que el medio arranca solo, que la máquina
> instalada arranca sin él, que encuentra su store sin que nadie se lo nombre, y que
> la consola de framebuffer funciona (OVMF entrega un GOP de verdad):
>
> ```
> git pull
> sudo ./dev/verify.sh        # la etapa 20 va en diecinueve líneas
> make -C image               # aquí se sabe si las opciones nuevas del kernel sobrevivieron
> sudo make -C image installed
> make -C image run-installed
> ```
>
> **Acto 2, en una PC** — es lo único que responde el teclado USB y NVMe/AHCI:
> `dd` a una USB, arrancar, `discos`, `instalar-en /dev/nvme0n1`, `apagar`, sacar la
> USB, encender.
>
> **Lo primero que puede fallar sigue siendo la compilación del kernel**, y eso es
> correcto: `config-check` detiene el build si `olddefconfig` descartó alguna opción.
> Ya lo hizo una vez, con `HID_SUPPORT`. Si vuelve a pasar, la lectura es la que dejó
> ese fallo: **un grupo de opciones descartadas juntas comparte una causa**, y hay que
> buscar el `menuconfig` que las contiene antes que las dependencias de cada una.
>
> ### Una cosa que el criterio no pide y conviene no confundir
>
> Una PC recién instalada arranca con un store bueno y **vacío**: la imagen lleva el
> kernel y un programa, así que no hay nada instalado en ella ni nada que instalar, y
> los pasos 2 a 6 de la lista original no se pueden hacer *en ella*. Se siguen
> haciendo en la de desarrollo y se siguen comprobando en cada cambio. No es un hueco
> del criterio vigente —*«ponerla en una PC sin sistema operativo y que ahora tenga
> Thalyx como OS»*, y eso se cumple— sino la pregunta de cómo llega el software a una
> máquina que no es ésta, que es la Fase 2.
>
> **785 pruebas pasan, `clippy` limpio en 1.97, `cargo fmt` aplicado.**

> ## Y los controladores de una PC de verdad — 2026-08-07
>
> **El bloque de arriba es más reciente.** Es un commit **aparte** del instalador, a
> propósito: se verifican por caminos distintos y ninguno de los dos tiene que
> esconder al otro si falla.
>
> Son los puntos 2 y 3 de la lista de riesgo de [[Construccion-del-ISO]], y **es
> la única parte del criterio de salida que una VM no puede responder.**
>
> | Qué | Qué falla sin eso |
> |---|---|
> | Pantalla — `FB_EFI`, `FRAMEBUFFER_CONSOLE`, `VT`, `FONT_8x16` | La ISO arranca en una PC y **no se ve nada** |
> | Teclado — `USB_HID`, `USB_XHCI_HCD`, `USB_EHCI_HCD`, `SERIO_I8042`, `KEYBOARD_ATKBD` | Llega al prompt y no se le puede contestar |
> | Discos — `BLK_DEV_NVME`, `SATA_AHCI`, `BLK_DEV_SD`, `PCI_MSI` | El instalador no ve el disco en el que va a instalar |
>
> Tres cosas que no son obvias:
>
> - **`FB_EFI` no es un driver de video.** Es el framebuffer que el firmware **ya
>   configuró**, y este driver lo adopta. Por eso no hay aquí un driver de ninguna
>   tarjeta: Thalyx dibuja texto, no levanta una GPU, y un driver por chip es un
>   userland entero. El costo: la resolución es la que eligió el firmware.
> - **`PCI_MSI` no es un lujo.** NVMe arma sus colas alrededor de interrupciones
>   por mensaje y `allnoconfig` lo deja apagado. Nada más de ese archivo lo pedía.
> - **`BLK_DEV_SD` es lo que hace que AHCI sirva.** Sin él el controlador se
>   encuentra, sus puertos se sondean, y no aparece `/dev/sda`.
>
> ### La consola, que fue tu decisión
>
> ```
> CONFIG_CMDLINE="console=ttyS0 console=tty0 lsm=capability,bpf panic=-1"
> ```
>
> **El kernel imprime en todas, y la ÚLTIMA es la que se vuelve `/dev/console`** —
> el único archivo por el que habla la sesión. Arrancada por firmware no se le pega
> nada, así que gana `tty0` y la sesión sale en la pantalla. Arrancada por QEMU,
> `-append console=ttyS0` va **después** —comprobado en `arch/x86/kernel/setup.c`,
> que concatena la compilada primero— así que el serie sigue ganando y la etapa 16
> ni se entera.
>
> Hay **cuatro pruebas** que leen `thalyx.config` y afirman esto, porque
> `config-check` atrapa una opción que Kconfig descartó y **no puede atrapar una
> que nadie pidió** — que es el error que costó `CONFIG_SECURITY_NETWORK` y un
> arranque entero.
>
> ### Y `run-uefi` abre una ventana
>
> Con la sesión en `tty0`, `-nographic` la dejaría corriendo perfecta donde nadie
> la ve, que es justo el fallo que el cambio evita. Ahora `run-uefi` y
> `run-installed` abren ventana, con `-serial mon:stdio` para que los mensajes del
> kernel sigan en la terminal. `HEADLESS=1` vuelve atrás.
>
> **Eso hace que la ventana sea la única forma de probar el framebuffer sin
> hierro**: OVMF sí entrega un framebuffer GOP de verdad. El teclado y los discos
> siguen necesitando una PC.
>
> ### Lo que falta, y es tuyo
>
> **Ninguna de estas opciones se ha compilado siquiera.** Este contenedor no
> compila kernels. Y la regla de este proyecto dice que **ninguna comprobación de
> construcción encuentra la siguiente opción que falta** — van tres encontradas
> arrancando (`BPF_LSM` con BTF, `SECURITY_NETWORK`, `FUNCTION_TRACER`), y lo
> razonable es esperar más aquí.
>
> Lo primero que puede fallar es la compilación: `config-check` detiene el build si
> `olddefconfig` descartó cualquiera de estas líneas. Si pasa, **la línea que falta
> es la que hay que mirar**, no el grupo entero — cada una tiene su párrafo al lado.
>
> ```
> make -C image                      # aquí es donde se sabe si sobrevivieron
> sudo ./dev/verify.sh               # el instalador, etapa 20
> sudo make -C image installed
> make -C image run-installed        # y aquí si una PC arranca
> ```
>
> **775 pruebas pasan, `clippy` limpio en 1.97, `cargo fmt` aplicado.**

> ## El instalador existe: un disco se vuelve una máquina — 2026-08-07
>
> **El bloque de arriba es más reciente y es el otro commit de hoy.** Los dos se
> verifican por caminos distintos y están separados a propósito.
>
> ### Lo que se construyó
>
> `crates/thalyx-install` y **`thalyx install <disco> --kernel <archivo>`**. Es el
> acto que juntaba las dos piezas caras, y lo que sale es esto:
>
> ```
>   LBA 0          MBR protector
>   LBA 1..34      la tabla de particiones, y su copia al otro extremo
>   1 MiB          partición 1, 512 MiB, FAT32, con \EFI\BOOT\BOOTX64.EFI adentro
>   513 MiB..      partición 2, el resto, btrfs `thalyx-store`, tres subvolúmenes
> ```
>
> **Un archivo en la partición de arranque, y es el kernel con Thalyx adentro.**
> Es `make -C image count` extendido al disco instalado.
>
> Costó **dos escritores de bytes más**, por el motivo de siempre: `sgdisk` y
> `mkfs.vfat` son lo que usaría una persona, y la imagen lleva el kernel de Linux
> y un programa. Van la cuarta y la quinta vez que este proyecto contesta a un
> binario ausente con el trabajo en vez de con la herramienta — `bpftool`, `cpio`,
> `btrfs`, `partprobe`, `mkfs.vfat` — y una cuarta llamada al kernel propia,
> `BLKRRPART`, porque escribir una tabla en un disco que el kernel ya tiene abierto
> no hace aparecer `/dev/sda1`.
>
> **FAT no es una preferencia, es del firmware.** La especificación UEFI obliga al
> firmware a entender FAT y nada más. Es el único sistema de archivos de Thalyx que
> existe para satisfacer algo de afuera, y conviene que quede dicho.
>
> ### Lo que decidió el diseño, y vale saberlo
>
> - **Los nombres de las particiones se le preguntan al kernel.** `/dev/sda` da
>   `/dev/sda1` y `/dev/nvme0n1` da `/dev/nvme0n1p1`; la regla que produce las dos
>   es una convención de las herramientas que las imprimen, no una promesa. Si se
>   deriva, el instalador anda en SATA y escribe el store **en la nada** en NVMe —
>   que es justo la mitad del hierro que aquí no se puede probar. Se leen de
>   `/sys/dev/block/<mayor>:<menor>/`.
> - **La ESP es de 512 MiB y la holgura es el punto.** No se agranda después sin
>   mover el store, y lo que seguro va a pasar es que una actualización de kernel
>   escriba el nuevo **al lado** del viejo. Una máquina que sobrescribe su único
>   archivo arrancable y se queda sin corriente no vuelve.
> - **Un disco de 4 KiB por sector se rechaza en vez de escribirse.**
>
> ### Y hay un fallo nuevo que no se parece a ningún otro de este proyecto
>
> **Una GPT con una suma equivocada no se reporta como rota: se ignora.** Linux cae
> al MBR protector, no crea ninguna partición, y el disco vuelve **igual que si
> nadie lo hubiera tocado**. El instalador habría dicho `ok`.
>
> Es distinto del Btrfs de la etapa 18, donde un superbloque dañado hace que
> `mount(2)` conteste un error. Ahí el fallo llega; aquí no llega nada. Por eso la
> etapa 20 no comprueba «el instalador terminó» sino **«el kernel hizo dos
> particiones»**, leídas de sysfs, con línea base de que antes no había ninguna, y
> comparando los tamaños contra lo que `--plan` dijo. Regla nueva en
> [[Estrategia-de-Pruebas]].
>
> ### El contenedor no pudo establecerlo, y casi acusa a Thalyx
>
> `thalyx install` escribió la tabla aquí y el kernel no hizo particiones. La
> lectura obvia era que la tabla estaba mal. Lo que lo resolvió fue escribir **un
> MBR común** —cuyo parser está en todos los kernels de Linux— y ver que tampoco
> producía nada: `/sys/block/loop0/range` vale `1`, o sea que este `loop` no admite
> particiones de ningún tipo. **Regla 5, novena vez**, y la etapa 20 lleva ese
> discriminador adentro en vez de una nota — cuando no aparecen particiones,
> escribe un MBR y vuelve a mirar; si de ése tampoco salen dice `NOT PROVEN`, y si
> salen, **entonces** el fallo es de Thalyx y lo dice como fallo.
>
> Lo que sí se pudo hacer aquí, y vale como red: las dos sumas de la GPT
> recalculadas con un CRC-32 independiente, y el volumen FAT32 recorrido entero por
> un lector escrito aparte —raíz, `EFI`, `BOOT`, la cadena de clusters— que devolvió
> los 3 000 000 de bytes idénticos.
>
> ### Lo que falta, y es de Cesar
>
> 1. **Correr `sudo ./dev/verify.sh`.** La etapa 20 es nueva y son **once líneas**
>    que sólo tu máquina puede establecer. Espero `proven 128 · not proven 1 ·
>    failed 0`.
> 2. **Y después `make -C image run-installed`**, que es la afirmación de verdad y
>    no la ejerce ninguna etapa:
>
>    ```
>    make -C image                      # el kernel, si no está construido
>    sudo make -C image installed
>    make -C image run-installed
>    ```
>
>    Un firmware UEFI recibe **sólo el disco instalado** —sin ISO, sin `-kernel`,
>    sin nada— y tiene que encontrar `\EFI\BOOT\BOOTX64.EFI` y arrancarlo. Si
>    encuentra nada, se queda en el shell de UEFI o reinicia; eso es lo que hay que
>    esperar si el tipo de partición, el FAT o el lugar del archivo están mal.
>
> **El kernel se le pasa por `--kernel` a propósito.** Un instalador corriendo
> *dentro* de la máquina arrancada desde la ISO tendría que sacar el bzImage del
> medio del que arrancó, y eso pide un **lector** de FAT y saber cuál disco es el
> medio. Es su propio cambio y no va encima de éste; está en [[Tareas-Pendientes]]
> junto con la otra cosa que el criterio va a pedir — que el store de una máquina
> recién instalada queda **vacío**, así que los pasos 2 a 6 no se pueden hacer *en
> ella* hasta que exista una forma de que el software llegue a una máquina que no
> es ésta.
>
> **771 pruebas pasan, `clippy` limpio en 1.97, `cargo fmt` aplicado.** El
> contenedor se actualizó a 1.97 antes de empezar, que es la regla del desfase de
> versión del bloque anterior aplicándose por primera vez.

> ## Thalyx hace los subvolúmenes, y clippy sí era un lint — 2026-08-07
>
> **El bloque de arriba es más reciente.**
>
> ### Lo que se construyó
>
> **Un sistema de archivos recién escrito ya no es lo único que sale de
> `thalyx disk format`.** Los tres subvolúmenes decretados —`system`, `modules`,
> `user`— los crea Thalyx por **`BTRFS_IOC_SUBVOL_CREATE`**, porque adentro de la
> imagen no hay binario `btrfs`. Tercera vez que este proyecto contesta a un
> binario ausente con una llamada al kernel: `bpftool`, `cpio`, y ahora `btrfs`.
>
> ```
>   ok  subvolume    system — created
>   ok  subvolume    modules — created
>   ok  subvolume    user — created
>
>   ok  mountable    subvol=system
>   ok  mountable    subvol=modules
>   ok  mountable    subvol=user
>
>   This is a store. PID 1 can mount it.
> ```
>
> **La comprobación es montar, no mirar.** Cada uno se monta con
> `-o subvol=<nombre>`, exactamente como lo hace PID 1. Preguntar si apareció un
> directorio con ese nombre daría *sí* para un directorio común, que es justo lo
> único que PID 1 no puede montar.
>
> Y **el número del ioctl no se toma de fe.** `_IOW` es un macro de C, aquí no hay
> C, así que la constante está escrita a mano y `tests/ioctl.rs` la recalcula desde
> el header capturado — incluido el tamaño del argumento, que va codificado adentro
> del número. Un tamaño equivocado no falla limpio: el kernel compara la palabra
> entera y contesta `ENOTTY` en un sistema de archivos que soporta la llamada
> perfectamente, lo que se lee como «este kernel es viejo».
>
> ### El fallo de clippy era un lint de verdad, y mi diagnóstico estaba mal
>
> Con el informe arreglado, tu corrida dijo qué era: **`unnecessary_sort_by`**, dos
> veces en `format.rs`. **No era `RUSTUP_HOME`.** Era **desfase de versión**: tu
> clippy es 1.97 y el del contenedor era 1.94, y el lint aprendió ese caso en
> medio. Actualizado el contenedor a 1.97, apareció en el primer intento; el
> arreglo fueron dos líneas.
>
> Las cuatro corridas «con el mismo `rustc`» comparaban 1.94 contra 1.94. Es la
> regla 5 con una vuelta que valía escribir aparte: **el instrumento incluye su
> número de versión**, y un linter es un instrumento cuyo trabajo entero es cambiar
> de opinión entre versiones. Ahora la etapa 2 imprime la versión de clippy en las
> dos líneas, y **el contenedor se mantiene al menos tan nuevo como tu máquina** —
> lo contrario garantiza que cada lint nuevo se descubra en la única máquina que no
> puede arreglarlo.
>
> No se fijó la cadena con un `rust-toolchain.toml`. Sería decisión tuya, te
> obligaría a descargar una versión concreta, y además un proyecto que fija su
> linter deja de enterarse de los lints nuevos — que es lo que se quería.
>
> ### Lo que falta, y es de Cesar
>
> **Correr `sudo ./dev/verify.sh`.** La etapa 19 es nueva y **sólo tu máquina puede
> establecerla**: este contenedor no tiene Btrfs en el kernel, así que aquí sale
> `NOT PROVEN` y eso es lo correcto.
>
> Espero `proven 117 · not proven 1 · failed 0`: tus 110, más los dos que fallaron
> y ya no deberían, más las cinco nuevas.
>
> Las cinco líneas que se agregan, y cada una tiene por qué existir:
>
> ```
>   PROVEN  a filesystem Thalyx just wrote has no subvolumes, so there is something to do
>   PROVEN  Thalyx created the three subvolumes through the kernel, with no btrfs binary
>   PROVEN  all three mount the way PID 1 mounts them, read back by mount(8) and not by Thalyx
>   PROVEN  a name nobody created does not mount, so subvol= is really being honoured
>   PROVEN  run again on a finished store it reports them as already there and changes nothing
> ```
>
> La primera es la línea base —sin ella, un comando que no hiciera nada pasaría la
> etapa—. La tercera lo lee con `mount(8)` y no con Thalyx, porque preguntarle al
> programa que acaba de hacer el trabajo no prueba nada. La cuarta es el control: un
> kernel que ignorara `subvol=` montaría los cuatro. La quinta es el camino de
> reparación, porque un instalador que falla a la mitad no puede costar el disco.
>
> **Es un cambio solo, a propósito.** El instalador no va encima de esto hasta que
> la etapa 19 haya corrido.
>
> **721 pruebas pasan, `clippy` limpio en 1.97, `cargo fmt` aplicado.**

> ## El kernel montó el Btrfs que escribió Thalyx — 2026-08-07
>
> **El bloque de arriba es más reciente.**
>
> ```
> proven 110 · not proven 1 · failed 2
> ```
>
> Fedora 43, kernel 7.1.5, `main @ 9229268`. Lo que la etapa 18 cerró, y ninguna
> corrida anterior podía cerrar:
>
> ```
>   PROVEN  Thalyx wrote a Btrfs filesystem with no mkfs.btrfs and no libbtrfs
>   PROVEN  the kernel mounts a filesystem Thalyx wrote byte by byte
>   PROVEN  the three decreed subvolumes can be created on it
>   PROVEN  a file written to it comes back, so the allocator has somewhere to go
> ```
>
> **El kernel monta lo que Thalyx escribió**, acepta los tres subvolúmenes
> decretados, y un archivo escrito en él vuelve. `btrfs check` ya lo aceptaba; eso
> no era un montaje, y ahora sí lo hay. La máquina puede hacer el disco en el que
> guarda.
>
> ### Y los dos fallos eran míos, ninguno del formato
>
> **1. El control de la etapa 18 dañaba espacio libre y acusaba al kernel.**
>
> ```
> FAILED  the kernel mounted a filesystem with both copies of its root tree damaged
> ```
>
> El kernel no aceptó basura. **Btrfs es copy-on-write**: la copia que el control
> rompe se sacaba *después* de montar, crear los subvolúmenes y escribir un
> archivo, y a esa altura la primera transacción del kernel ya había escrito un
> árbol raíz nuevo en otro sitio y retirado el que escribió Thalyx. Los bytes que
> se pisaban eran espacio libre de la generación 1.
>
> Lo peor no es el error sino que **el informe acusaba al kernel de un defecto del
> arnés, en una línea escrita precisamente para no dejarse engañar.** Y la prueba
> equivalente de `cargo test` pasaba y sigue pasando, porque ahí el sistema de
> archivos nunca se monta y el bloque sigue vivo.
>
> Arreglado sacando la copia antes de cualquier montaje, y con **línea base para el
> control mismo**: se comprueba que la copia dañada difiera de la original, porque
> un `cp` que falla o un `dd` que no escribe nada dejan una imagen intacta —que
> monta— y eso se reportaría otra vez como el kernel aceptando basura.
>
> Comprobado aquí con `btrfs check`, que es lo que este contenedor puede hacer:
> dañando la copia pristina en los mismos offsets, `checksum verify failed on
> 5242880` en las dos copias y `cannot open file system`. El mecanismo estaba bien;
> lo que estaba mal era cuándo se sacaba la muestra.
>
> **2. Clippy falló en tu máquina y no lo pude reproducir.**
>
> Lo intenté cuatro veces con el mismo código: `rustc` 1.94 en limpio, 1.90 en
> limpio, y 1.90 incremental sobre el cambio. Las cuatro salieron limpias. **No
> puedo decir que esté arreglado**, y no lo voy a decir.
>
> Lo que sí encontré es por qué no se puede saber: `verify.sh` construye su
> directorio con `mktemp -d` y **lo borra al salir**. Unos treinta mensajes de
> fallo terminan en `see $WORK/algo.log`, y los treinta apuntaban a una ruta que ya
> no existía cuando alguien iba a leerla. El único artefacto que podía decir qué
> lint era lo borró el script que lo escribió.
>
> > **Resuelto al día siguiente y no era esto.** Con el informe arreglado, la
> > siguiente corrida dijo el lint: `unnecessary_sort_by`, y la causa era que tu
> > clippy es 1.97 y el del contenedor era 1.94. Ver el bloque de arriba. Lo de
> > abajo queda escrito porque los cuatro arreglos son buenos y porque el punto 4
> > sigue siendo un defecto real — sólo no era *este* fallo.
>
> Cuatro arreglos, y el cuarto parecía la causa de lo que viste:
>
> 1. **El directorio se conserva cuando algo falló**, y el resumen dice dónde está.
> 2. **Los diagnósticos de clippy se imprimen** en vez de referenciarse.
> 3. **«clippy objetó al código» y «clippy no pudo correr» dejaron de ser la misma
>    línea.** El segundo es `NOT PROVEN` y dice qué componente instalar — regla 10,
>    en el sitio donde costó un diagnóstico.
> 4. **El arreglo del entorno de rustup bajo `sudo` estaba condicionado a que
>    `cargo` no estuviera en el `PATH` de root.** Pero que `command -v cargo`
>    encuentre el shim de rustup no dice que ese shim pueda resolver una cadena de
>    herramientas: la busca bajo `$HOME/.rustup`, y `sudo` pudo haber puesto `$HOME`
>    en `/root`. En tu corrida la etapa 1 encontró cargo, así que `RUSTUP_HOME`
>    **nunca se puso**. El fallo que eso deja es por componente: una cadena que
>    contesta `build` y `fmt` pero no `clippy` se reporta como clippy encontrando
>    problemas. Ahora se aplica siempre que se corra bajo `sudo`.
>
> Y la etapa 1 ahora imprime la **versión** de la cadena de herramientas, no sólo su
> ruta: una corrida contra otra cadena se ve idéntica a una contra la esperada.
>
> **Si en la próxima corrida clippy vuelve a fallar, el informe va a decir qué
> lint es.** Y así fue: dijo `unnecessary_sort_by`, que es el bloque de arriba.
>
> Las dos reglas nuevas están en [[Estrategia-de-Pruebas]], y la cuenta de la regla
> 5 va en diez con la del desfase de versión.

> ## Thalyx escribe su propio Btrfs — 2026-08-07
>
> **El bloque de arriba es más reciente. Esto es lo que se construyó.**
>
> `crates/thalyx-btrfs`: ocho árboles, tres chunks y los superbloques, escritos
> byte por byte. **Sin `mkfs.btrfs` y sin `libbtrfs`.** Es el punto 2 del orden de
> trabajo del ISO y era el poste largo — la máquina arrancaba y no podía guardar
> nada.
>
> Lo obliga [[Filosofia-Fundacional]] y no una preferencia: la imagen lleva el
> kernel y un programa, así que `mkfs.btrfs` no puede estar ahí. Misma forma que
> `bpftool` y que `cpio`, misma respuesta.
>
> ```
> $ thalyx disk format /tmp/store.img --yes
>   ok  store        /tmp/store.img — 8589934592 bytes, labelled `thalyx-store`
>       fsid a39c0565af37487b8f8fe806ff352104
>       2 superblock(s), 131072 bytes of metadata
> ```
>
> ### El decreto de que PID 1 nunca fabrica se conservó entero
>
> Estaba anotado como *pendiente de confirmar al construirlo*, y se confirmó:
> **nada de `thalyx-btrfs` es alcanzable desde PID 1.** Lo invoca un humano con
> `thalyx disk format`, que es el acto explícito que la tarea preveía. PID 1 sigue
> montando y sin crear.
>
> La confirmación **pide teclear la ruta del dispositivo**, no una `y`. Es lo más
> destructivo que Thalyx sabe hacer, el argumento es una palabra, `/dev/sda` y
> `/dev/sdb` se diferencian en una tecla, y una `y` confirma una frase que el
> humano ya dejó de leer. Antes de preguntar dice qué hay en el disco ahora,
> leyéndolo.
>
> ### Lo que falta, y es de Cesar
>
> **Correr `sudo ./dev/verify.sh`.** La etapa 18 es nueva y **el montaje sólo lo
> puede establecer tu máquina**: este contenedor no tiene Btrfs en el kernel ni
> módulos que cargar. Aquí la etapa sale así, que es lo correcto:
>
> ```
>   PROVEN      Thalyx wrote a Btrfs filesystem with no mkfs.btrfs and no libbtrfs
>   PROVEN      it identifies itself by the label an installed machine looks for
>   PROVEN      a device nobody formatted is reported as no filesystem, not as no label
>   PROVEN      btrfs check walks it and finds nothing wrong
>   NOT PROVEN  this kernel has no Btrfs, so nothing could mount what Thalyx wrote
> ```
>
> En tu máquina esas cinco líneas se vuelven ocho, y las cuatro que se agregan son
> las que importan: que el kernel lo monta, que acepta los tres subvolúmenes, que
> un archivo escrito en él vuelve, y **que el mismo sistema de archivos dañado se
> niega** — sin ese último, el montaje no demuestra nada.
>
> **Es un cambio solo, a propósito.** No apilé la búsqueda por etiqueta encima.
>
> ### Cómo se sabe que el formato es correcto sin poder montarlo
>
> Dos instrumentos, y ninguno es leer el formato.
>
> Los headers de Linux (`btrfs_tree.h` y `btrfs.h`) están capturados verbatim en
> `crates/thalyx-btrfs/tests/`, y una prueba los parsea y comprueba **cada tamaño
> y cada offset** que el escritor usa. Y `btrfs check` recibe lo escrito y recorre
> los árboles, las referencias inversas y la contabilidad de cada grupo de
> bloques: `no error found`, también en el disco más chico que se permite y
> leyendo desde el superbloque de respaldo.
>
> **`btrfs check` no es un montaje**, y está dicho así en el código: lee con el
> código de btrfs-progs, no con el del kernel, y los dos ya se han contradicho.
>
> btrfs-progs es dependencia **de desarrollo**, nunca de ejecución — el punto del
> crate es que la imagen no lo tiene. Así que se salta donde falta, dice `NOT
> PROVEN`, y hay **una variable por requisito**: `THALYX_REQUIRE_BTRFS_PROGS` para
> el validador y `THALYX_REQUIRE_BTRFS_TESTS` para el montaje. Las dos se
> ejercieron en las dos direcciones.
>
> ### Tres defectos, y los tres dan regla
>
> 1. **Btrfs usa el mismo CRC32C con dos convenciones y no coinciden.** La suma de
>    un bloque es CRC32C estándar; el hash del nombre de una entrada de directorio
>    es el primitivo crudo desde `~1` **sin complemento final**. La primera versión
>    aplicó la estándar a las dos y el hash de `default` salió un número estable,
>    plausible, y que hace que el kernel resuelva el subvolumen por omisión
>    encontrando nada. Leer el kernel no lo evita: la diferencia está en el
>    intermediario. Lo encontró una imagen real de `mkfs.btrfs`.
> 2. **Un bit de versión omitido no falla al parsearse: se parsea como otro
>    formato.** Sin `MIXED_BACKREF_REV` en las banderas de cada cabecera, todo
>    parseaba perfecto y `btrfs check` reportó **once fallas de referencia**, una
>    por extent. El síntoma estaba lo más lejos posible de la causa, y lo delató
>    una línea informativa —`backref revision 0`— comparada contra la imagen de
>    referencia.
> 3. **El arnés otra vez, y van ocho.** El parser que comprueba los offsets
>    descartaba en silencio todo campo con comentario al final de su línea, y dijo
>    que `btrfs_root_item` medía 343 cuando el escritor producía 439. **El escritor
>    tenía razón.** Ahora hay una prueba que gradúa al parser antes de medir nada
>    con él, usando los tamaños que los headers afirman en su propio texto.
>
> Las dos primeras están escritas como reglas nuevas en [[Estrategia-de-Pruebas]];
> la tercera se sumó a la cuenta de la regla 5.
>
> ### Lo que seguía faltando, y se hizo el mismo día
>
> **Un sistema de archivos recién escrito no tiene subvolúmenes**, y PID 1 monta
> `subvol=system`. Así que un store hecho con `thalyx disk format` **todavía no era
> un store**, y el comando lo decía al terminar en vez de dejar que pareciera que
> sí. Ya los crea, por ioctl — el primer bloque de esta nota.
>
> Y `make -C image store` **sigue usando `mkfs.btrfs`** a propósito: es la red de
> regresión de las etapas 13 y 16, y cambiarla en el mismo commit que introduce lo
> que hay que probar dejaría la red y lo probado siendo el mismo código sin
> ejercer. Misma razón por la que `boot` siguió pasando `-kernel` cuando apareció
> `run-uefi`.
>
> **710 pruebas pasan, `clippy` limpio, `cargo fmt` aplicado.**

> ## Todo lo que existe está verificado, y la persona ajena se cancela — 2026-08-06
>
> **Es lo primero que hay que leer.**
>
> ```
> proven 104 · not proven 1 · failed 0
> ```
>
> Fedora 43, kernel 7.1.5, `main @ 9e1c5f8`. Las diecisiete etapas, incluidas
> **las tres que nunca habían corrido enteras**: el arranque frío tecleando los
> seis pasos, el reinicio de verdad, y lo que la auditoría cerró. La única `not
> proven` es `llama.cpp`, que **no es una comprobación que no se pudo hacer sino
> una cosa que no existe todavía**.
>
> Y se corrió otra vez con `THALYX_REQUIRE_IMAGE_TESTS=1`, que convierte en
> fallo cualquier salto de la etapa 16. Mismo resultado: la etapa corrió de
> verdad, no se saltó en silencio. Esa segunda corrida es la que vuelve creíble
> la primera.
>
> **Es la primera vez que no hay nada roto y nada pendiente de ejercer.**
>
> ### El ancla del kernel ya está en el repositorio
>
> Cesar la estableció contra la lista firmada de kernel.org y la puso con `nano`
> en su máquina, donde no la ve nadie más. Ahora está en `image/Makefile`:
>
> ```
> KSHA256 := 0d21cd11933f49f7151b7c9dbb8cc3fddc8c8abe506434b850feecf41fc28a76
> ```
>
> Con **quién la firmó y su huella al lado**, porque un digest a secas dice qué
> se aceptó y no qué lo estableció, y el siguiente que lo herede no tiene cómo
> volver a comprobarlo. `make -C image doctor` ya pasa.
>
> ### Y el procedimiento que la establece estaba equivocado
>
> Lo encontró él corriéndolo, que es la regla 1 otra vez. `pin-kernel` decía:
>
> ```
> gpg --locate-keys torvalds@kernel.org gregkh@kernel.org
> ```
>
> Son quienes firman una versión del kernel. **No son quienes firman ese
> archivo**: `sha256sums.asc` lleva la llave automática de sumas de kernel.org,
> así que `gpg --verify` contestó `No hay clave pública` — tres renglones debajo
> de una frase que decía que cualquier cosa que no sea *Good signature* es
> motivo para parar. **El procedimiento imprimía la falla que él mismo define
> como fatal.** Cesar tuvo que encontrar `autosigner@kernel.org` por su cuenta.
>
> Se escribió en este contenedor, cuya red no alcanza kernel.org, así que no
> había cómo correrlo aquí y salió sin correr. Regla nueva en
> [[Estrategia-de-Pruebas]]: **un procedimiento impreso para una persona es
> código sin correr.** Arreglado, con la huella impresa para comparar, el aviso
> de gpg explicado como esperado, y una prueba que exige las dos copias.
>
> ### La persona ajena se cancela — decisión de Cesar
>
> Los seis pasos ya no los va a ejecutar alguien de fuera, por ahora. Su
> razonamiento, entero, está en [[Criterio-de-Salida-Fase-1]]: Thalyx todavía
> son comandos de terminal, el producto terminado será una ISO booteable, y se
> prueba cuando haya algo que probar. Ni siquiera se pudo convencer a la persona
> de hacerlo.
>
> **Lo que no cambia**: los seis pasos siguen siendo lo que el sistema tiene que
> hacer, y se siguen comprobando solos en cada cambio y en cada corrida de
> hardware. Lo que se cancela es quién los teclea.
>
> ### Y arrancó entera: el paso 1 del criterio está cerrado — 2026-08-06
>
> **Un firmware arrancó Thalyx sin gestor de arranque, y la máquina hizo todo lo
> que sabe hacer.** Con la consola puesta, el segundo intento salió así:
>
> ```
>   ok  root         moved off the initramfs, so a module can be pivoted into a root
>   ok  mounted /proc … /sys/fs/cgroup          (los siete)
>   ok  sandbox root the root is attached
>   ok  controllers  memory, pids handed down at /sys/fs/cgroup
>   no  store        no thalyx.store= on the kernel command line
>   ok  thalyx-lsm   2 hook(s) live, 3 map(s) pinned under /sys/fs/bpf/thalyx
> ```
>
> Lo que eso demuestra, y no lo demostraba ningún arranque anterior: **todo lo
> que Thalyx hace funciona cuando lo arranca un firmware y no `-kernel` de
> QEMU.** El `switch_root`, los siete montajes, la delegación de controladores,
> **el LSM enganchado**, la sesión, y el apagado limpio. No hay GRUB en ninguna
> parte y no hace falta.
>
> El `no store` es **correcto y estaba previsto**: `thalyx.store=` nombra un
> dispositivo, la línea de comandos va compilada dentro del kernel, y no hay un
> nombre que sirva en las dos máquinas. La máquina lo dice como lo que es —
> *«el disco no falta; nadie me dijo cuál es»*— en vez de adivinar. Es el paso 3.
>
> **Lo que sigue sin probarse es hierro.** Esto fue OVMF dentro de QEMU: discos
> virtio, teclado emulado y consola serie. Una PC de verdad no tiene puerto
> serie, y ahí es donde `console=ttyS0` deja de servir.
>
> ### El firmware arrancó Thalyx sin gestor de arranque — 2026-08-06
>
> **El paso 1 del criterio nuevo funcionó.** OVMF encontró
> `EFI/BOOT/BOOTX64.EFI`, lo arrancó, el kernel desempaquetó el initramfs que
> lleva adentro y ejecutó `/init`. **No hay gestor de arranque y no hace falta
> ninguno**: el medio lleva un archivo y ese archivo es el kernel con Thalyx
> dentro.
>
> Y murió en su primera instrucción, por algo que no era ninguna de las dos
> cosas que se estaban probando:
>
> ```
> Warning: unable to open an initial console.
> Run /init as init process
> traps: init[1] general protection fault ip:7fea0faff143
> Kernel panic - not syncing: Attempted to kill init!
> ```
>
> La instrucción que falló es `hlt`, que está al final de `abort()` de musl — la
> que sólo se alcanza cuando `SIGABRT` **no** mató al proceso, y no lo mata
> porque el kernel no le entrega señales fatales por defecto a PID 1. Y quien
> llamó a `abort()` no fue código de Thalyx: fue el runtime de Rust **antes de
> `main`**, que se niega a seguir si no puede garantizar descriptores 0, 1 y 2.
> El archivo tenía un `/dev` vacío.
>
> **Ningún arranque anterior podía verlo**: `-initrd` se desempaqueta *encima*
> del initramfs propio del kernel, que trae `/dev/console`. Meter el nuestro
> adentro lo *reemplaza*. La consola venía de regalo de algo que nadie miró — y
> es la **tercera vez** con esa forma exacta, después de systemd con los
> controladores y del `switch_root`. Regla nueva en [[Estrategia-de-Pruebas]].
>
> Arreglado: el archivo lleva `/dev/console` (carácter 5:1). **Sin `/dev/null`
> al lado, a propósito** — una máquina que no alcanza su consola debe detenerse,
> no correr perfecta hablándole a la nada, que es peor y que este proyecto ya
> cometió una vez.
>
> Y el conteo tuvo que aprender una clase nueva: un nodo de dispositivo no es un
> programa, pero `is_directory()` lo habría contado como uno y `count` habría
> dicho **2**. Ahora hay tres clases, se imprimen los números del nodo, y una
> prueba exige que las tres sumen el total — porque lo que no se cuenta es justo
> por donde entraría un segundo programa sin que el número se moviera.
>
> `make -C image run-uefi` es lo que lo arranca. Necesita OVMF
> (`sudo dnf install edk2-ovmf`); si falta, se niega y dice que **no se probó
> nada** en vez de reportar que Thalyx falló.
>
> Y **no se tocó `make run`**: sigue pasando `-kernel` e `-initrd`, porque es la
> red de regresión de la etapa 16 y cambiarla ahora dejaría la red y lo que se
> prueba siendo el mismo cambio sin ejercer. Se mueve cuando la ruta del
> firmware tenga su propia etapa.
>
> ### Y el criterio nuevo: una ISO independiente
>
> Cesar eligió el sustituto el mismo día:
>
> > una ISO totalmente independiente, es decir: que puedas ponerla en una PC sin
> > sistema operativo y que ahora tenga Thalyx como OS […] el objetivo es que
> > tengamos la ISO y nada más, y con ella sola podamos tener Thalyx corriendo.
>
> **Es la propiedad que la persona ajena aportaba y la lista de componentes
> nunca tuvo: no la puede declarar nadie.** O existe un archivo que convierte
> una máquina sin sistema operativo en una máquina Thalyx, o no existe.
>
> Y es **más** exigente que lo de hoy, no menos. Hoy **QEMU es el gestor de
> arranque**: `make run` pasa `-kernel` y `-initrd`. Nada de lo construido sabe
> arrancar solo.
>
> Se puede ejercer en una VM, y eso está bien **si se dice qué prueba**: una VM
> con firmware UEFI de verdad prueba que la ISO arranca **sola**, que es la
> mitad que importa. **No prueba los controladores** — sus discos son virtio y
> su teclado es emulado. Esa mitad necesita hierro.
>
> Lo que cuesta, en [[Construccion-del-ISO]]. Los cuatro puntos, por riesgo:
>
> 1. **El store, que hoy nadie crea.** PID 1 monta y **tiene prohibido
>    fabricar**, con buena razón: una máquina que se inventa un store arranca
>    perfecta el día que el disco no estaba. En una PC vacía no hay store, y
>    `mkfs.btrfs` no puede ir en la imagen. Es el decreto que hay que revisar, y
>    **«es la primera vez» y «no encontré el tuyo» tienen que seguir siendo
>    distinguibles**.
> 2. **Los controladores.** `allnoconfig` más virtio y un serie. Falta UEFI,
>    framebuffer, teclado USB y almacenamiento real. Van tres opciones de kernel
>    encontradas arrancando y hay una regla que dice que ninguna comprobación de
>    construcción encuentra la siguiente.
> 3. **La consola es `ttyS0`.** Una PC moderna no tiene puerto serie: arrancaría
>    bien y no se vería nada.
> 4. **El gestor de arranque, que es la pregunta de filosofía.** GRUB sería un
>    segundo programa y [[Filosofia-Fundacional]] no lo permite — la misma forma
>    que el hueco de `bpftool`. La salida que no pide excepción: un kernel con
>    `CONFIG_EFI_STUB` **es** una aplicación UEFI, así que el firmware lo carga
>    directo y el medio lleva **un archivo**. Es `count` extendido al medio de
>    arranque. **Plan, no propiedad**: no se ha construido.
>
> Y que nadie aceptara hacerlo **es un dato y no un contratiempo**: la primera
> medición de [[Por-Que-Elegirian-Este-SO]] no fue una opinión sobre el sistema,
> fue que media hora de terminal ya cuesta más de lo que hoy ofrece.

> ## Los arreglos de la auditoría rompieron dos cosas, y las dos ya están — 2026-08-05
>
> **Es lo primero que hay que leer.** Cesar corrió `sudo ./dev/verify.sh` con los
> arreglos de la auditoría puestos y salió `proven 99 · not proven 2 · failed 10`.
> Los diez fallos eran de Thalyx, no suyos, y los diez son **dos defectos**, los
> dos del commit `3976974`.
>
> ### 1. El módulo perdió su `stdout`, y ese `stdout` era el instrumento
>
> El arreglo era correcto —el módulo compartía terminal con el camino confiable,
> podía leer la `y` del humano y podía dibujar el marco— y la respuesta fue
> mandarle `stdout` y `stderr` a `/dev/null`.
>
> **La etapa 6 le pregunta al programa confinado qué ve**, que es la regla 2:
> su pid, su uid, su hostname, sus interfaces, su raíz, si alcanza lo concedido.
> Las seis respuestas viajaban por `stdout`. Con el descarte, las seis
> reportaron `nothing` — **que es también lo que reporta un sandbox que no aisló
> nada**. La medida de contención dejó a la contención sin testigo.
>
> Y costaba la otra dirección: un módulo que muere con un mensaje en `stderr` no
> dejaba mensaje. *Falló* y *falló por esto* eran el mismo evento, que es lo que
> la regla 10 prohíbe.
>
> Ahora la salida es una tubería que Thalyx **drena** y reimprime marcada y
> saneada, igual que el canal. La propiedad que el `/dev/null` compraba se
> conserva, dicha con precisión: **un módulo no puede empezar una línea.** No que
> sus palabras desaparezcan —desaparecer era el error— sino que todo lo que
> escribe llega detrás del marcador de Thalyx:
>
> ```
>   org.thalyx.verify wrote, at descriptors Thalyx does not mediate:
>   > uid=700000
>   > pid=1
>   ! and now a diagnostic
> ```
>
> Decidido por Cesar el 2026-08-05, entre tres opciones. El techo es sobre lo
> que se **guarda** (64 KiB), nunca sobre lo que se lee: un lector que se
> detiene en su propio techo bloquea al módulo en la siguiente escritura, y el
> límite de memoria se vuelve un cuelgue. Hay una prueba con 200 000 líneas.
>
> ### 2. Lo que un módulo dice se truncaba a 72 caracteres
>
> Es **el mismo defecto que la auditoría ya había arreglado para los permisos**,
> sin arreglar donde el mismo razonamiento se aplica. `sanitise` corta a 72 y
> `sanitise_block` lo llamaba línea por línea. El `greeter` dijo:
>
> ```
> read 27 byte(s) from /tmp/tmp.BCvj7bvl02/greeter-granted/notes.txt: the…
> ```
>
> Contestó qué leyó y Thalyx tiró la respuesta. Explica las etapas 12 y 13.
>
> Lo peor no es el corte sino **quién decide dónde cae**: lo que gasta el
> presupuesto es la *ruta*, así que el mismo módulo dice menos en una máquina
> con directorios más anidados. Ahora se acota por el **medio** y con mucho más
> aire, como los permisos.
>
> ### Y la etapa 17 llevaba un día sin poder probar nada
>
> Decía `NOT PROVEN: no installed module`, honestamente y **siempre**: la etapa
> 6 revierte el módulo como su última comprobación. Un salto que se dispara
> siempre no es un salto, es una comprobación que nunca se hizo.
>
> Le faltaba además el control que importa: afirmaba que el texto del módulo
> **no aparece**, lo que se satisface borrándolo todo — y de hecho pasó en verde
> la misma corrida en que la etapa 6 se quedó ciega. Ahora exige las dos cosas:
> que aparezca, y que no empiece ninguna línea.
>
> **666 pruebas pasan, `clippy` limpio, `cargo fmt` aplicado.** Tres reglas
> nuevas en [[Estrategia-de-Pruebas]].
>
> ### Lo que falta, y es de Cesar
>
> 1. **Correr `sudo ./dev/verify.sh` otra vez.** De los diez fallos, los cuatro
>    de las etapas 12 y 13 y los de la 6 en modo `--unconfined` se ejercieron
>    aquí; **la ruta confinada no**, porque este contenedor no tiene el LSM
>    atachado y la prueba del cgroup se salta diciéndolo. El cambio del sandbox
>    —`launch::spawn` con tuberías en vez de `/dev/null`— **solo se ejerce en tu
>    máquina**.
> 2. **Anclar el digest del kernel.** `make -C image` sigue fallando a propósito
>    hasta que lo hagas, y eso es correcto: es la tarea 2 de abajo, no un
>    defecto. `make -C image pin-kernel` imprime los cuatro comandos.
>
> ## Y el criterio de salida dejó de depender de que alguien corra un comando — 2026-08-04
>
> **Léelo después del bloque de la auditoría, que sigue siendo lo primero.**
>
> Al ir a comprobar el paso 6 apareció por qué estaba "escrito y sin ejercer":
> la etapa 15 de `verify.sh` —la que maneja el prompt e implica los pasos 2, 3,
> 4 y 6— necesitaba `script(1)`, que Fedora trae en `util-linux-script`. En la
> única máquina que puede verificar Thalyx, la etapa se saltaba entera.
>
> El salto hizo lo que debía: dijo `NOT PROVEN`. Y no alcanzó, porque nadie
> actúa sobre un informe que ya conoce. Ver la regla nueva en
> [[Estrategia-de-Pruebas]].
>
> Dos cosas, y las dos ya están:
>
> 1. **Thalyx hace su propia terminal.** `thalyx dev pty` —`posix_openpt`,
>    `setsid`, `TIOCSCTTY`— en `thalyx-syscall`, donde vive el `unsafe`. La
>    misma decisión que el initramfs y el cargador de BPF: antes ochenta líneas
>    propias que una cuarta cosa que nadie eligió. `verify.sh` ya no necesita
>    nada que la máquina que corre Thalyx no tenga.
> 2. **Los cuatro pasos que no necesitan hardware son ahora pruebas.** Corren en
>    cada cambio, contra el disco y desde fuera de la sesión que dice haber
>    hecho las cosas. En `verify.sh` queda lo que sí necesita máquina: arrancar
>    la imagen, que el kernel deniegue, un reinicio de verdad.
>
> **Y manejar el prompt de verdad encontró un defecto que la auditoría había
> creado**: el saneador truncaba también las líneas de permiso, y un permiso no
> es una etiqueta. Con un `$HOME` largo, `/home/user/projects/secrets` y
> `/home/user/projects/public` se dibujaban igual, y el humano confirmaba una
> frase cierta de los dos. Ahora los permisos se envuelven en varias líneas —
> solo la primera lleva viñeta, para que una línea envuelta no parezca una
> concesión más— y si hay que acotar se elide el **medio**, no el final: la raíz
> y la hoja son las dos partes que dicen qué se está concediendo.
>
> Regla 1 otra vez: salió de correr el sistema, y hubo que escribir la terminal
> para poder correrlo.
>
> **El `doctor` también aprendió el ancla del kernel.** Al hacerlo fallar sin
> digest, el commit anterior había creado justo el fallo que el `doctor` existe
> para evitar: decir "está todo" y que la pared llegue después de la descarga.
> Ahora se reporta en la misma vuelta que lo demás.
>
> ### Lo que decidí no hacer, y por qué
>
> **Cargar `thalyx_watch` con el cargador propio.** Es el último punto de "lo
> que falta comprobar" y no lo toqué: aquí no se puede compilar el objeto
> —faltan las cabeceras de `libbpf`— así que sería una segunda cosa sin ejercer
> apilada sobre las que ya esperan hardware, que es exactamente lo que
> `CLAUDE.md` prohíbe. El cargador ya recorre varios programas y varios mapas, y
> el único tipo que el watcher usa y el LSM no es `PERCPU_ARRAY`, que es un mapa
> con clave como cualquier otro. Es probable que funcione y **probable no es
> comprobado**.
>
> ## Lo último: una auditoría externa encontró nueve defectos reales — 2026-08-04
>
> **Es lo primero que hay que leer y lo único pendiente de comprobar.**
>
> Alguien de fuera revisó el repositorio y escribió una auditoría de seguridad.
> Se verificó afirmación por afirmación contra el código: la mayoría eran
> ciertas, tres son críticas, y varias de las que no eran ciertas también se
> respondieron por escrito en vez de quedar en una conversación.
>
> Los tres críticos, en una línea cada uno:
>
> 1. **El lock global decretado no existía.** [[Concurrencia]] lo decretaba
>    desde el 1 de agosto y ningún código lo tomaba. Una instalación escribe
>    cuatro archivos separados; cada `rename` es atómico y el conjunto no.
> 2. **Una actualización interrumpida podía darle a la versión vieja los
>    permisos de la nueva.** El registro se indexaba sólo por id de módulo y la
>    comprobación preguntaba "¿está instalado?" en vez de "¿es *esta* versión?".
>    Trece pruebas de inyección de fallos cubrían esa ventana y ninguna lo vio,
>    porque todas instalaban por primera vez.
> 3. **Un keystore corrupto se leía como uno vacío**, y uno vacío confía en
>    todo lo que le ofrezcan. Dañar un archivo degradaba a todos los
>    publicadores anclados a un primer avistamiento.
>
> Los seis restantes: los permisos `session` no existían (se guardaban como
> `persistent` y nunca se revocaban); `net/outbound` quitaba el namespace de red
> y seccomp seguía prohibiendo `socket`, así que costaba aislamiento y no daba
> capacidad; el camino confiable dibujaba texto del publicador sin sanear y el
> módulo heredaba la terminal donde se dibuja; el API interna comprobaba la ruta
> y la abría después, con una carrera en medio; un módulo podía hacer crecer sin
> límite la memoria de Thalyx mandando notificaciones; y el journal se negaba a
> leerse entero si su última línea estaba cortada — justo lo que deja un corte.
>
> **Todo está corregido, con pruebas que se comprobó que fallan sin el arreglo.**
> 657 tests pasan, `clippy` limpio, `cargo fmt` aplicado.
>
> ### Lo que falta, y es de Cesar
>
> 1. **Correr `sudo ./dev/verify.sh`.** Nada de esto se ejerció en hardware. Los
>    cambios tocan el sandbox (`stdio` del módulo, seccomp según la concesión) y
>    el arranque los usa.
> 2. **Anclar el digest del kernel.** `image/Makefile` ahora se niega a
>    construir con `KSHA256 := UNPINNED`, porque Thalyx compila su propio kernel
>    y ese tarball se bajaba sin verificar nada más que TLS. `make -C image
>    pin-kernel` imprime los cuatro comandos; el tercero tiene que decir *Good
>    signature*. **Hasta que lo hagas, `make -C image` falla a propósito.**
> 3. **Decidir sobre lo que quedó nombrado y no resuelto**: los módulos son
>    binarios de Linux que hablan POSIX, y el decreto dice que no. La distinción
>    que sí se sostiene está escrita en [[Sistema-de-Modulos]] —la API es la
>    única superficie *mediada*— y cerrar la brecha entera es Fase 2. Ver
>    [[Tareas-Pendientes]].
>
> Un costo que se aceptó a propósito y conviene saber: el API interna ahora usa
> `openat2` con `RESOLVE_BENEATH`, que rechaza **todo** symlink absoluto,
> incluido uno que apunte dentro de la misma concesión. Los relativos siguen
> andando. Hay una prueba que nombra esa pérdida.

> ## La máquina arrancó — 2026-08-03
>
> `make -C image run`, en la Fedora de Cesar, con kernel 6.12.101 propio y un
> solo programa dentro. Montó sus siete filesystems, imprimió lo que es y lo que
> no tiene, y esperó una instrucción. **Es la primera vez que Thalyx existe como
> máquina y no como programa sobre la máquina de alguien más.**
>
> Se describió con tres `no`: sin Btrfs, sin enforcement, sin módulos.
>
> ## Y ahora tiene dónde guardar cosas — 2026-08-03, esa misma noche
>
> **Dos de los tres `no` están cerrados.** El store existe: `sudo make -C image
> store` formatea un disco Btrfs con los tres subvolúmenes decretados e instala
> el `greeter` adentro; PID 1 lo monta por el nombre que le da `thalyx.store=` y
> **nunca lo crea**. La sesión sabe `modulos`, `correr <id>` y `apagar`.
>
> Lo que hace la máquina ahora, entero: arranca, monta su disco, lista el módulo
> que tiene instalado, lo corre, y el módulo le pregunta a Thalyx quién es y qué
> puede leer —porque `correr` no le pasa ningún argumento—, lee lo que le
> concedieron y le niegan `/etc/shadow`. Todo por un socket que él no abrió,
> dentro de la máquina, sin shell.
>
> **Nada de eso se ha ejercido dentro de QEMU todavía.** Ver "Lo que falta
> comprobar" abajo.
>
> ## Y arrancó con su disco puesto — 2026-08-03
>
> ```
>   ok  store        /dev/vda — three subvolumes
>   ok  filesystem   btrfs
>   ok  modules      1: dev.thalyx.greeter 1.0.0
> ```
>
> **De tres `no` en el primer arranque a uno.** El que queda es el enforcement,
> que es el hueco de arquitectura que sigue. La máquina arranca, monta su store
> de Btrfs, y sabe qué tiene instalado.
>
> Cuatro cosas se rompieron entre construir el store y verlo montado, y las
> cuatro fueron del constructor y no del sistema — están abajo, en "Los cuatro
> fallos del camino", porque tres de ellas son la misma regla.
>
> ## Y ahora carga su propio enforcement — 2026-08-03, escrito y sin ejercer
>
> El último hueco de arquitectura. `attach_lsm` invocaba `bpftool` —un segundo
> programa, desde una shell, en una imagen que no tiene ninguno de los dos— y
> buscaba `/lib/thalyx/thalyx_lsm.bpf.o`, un segundo archivo. **El mensaje que
> imprimía sugería romper el decreto que estaba reportando.**
>
> Ahora el objeto BPF va dentro del binario y Thalyx hace las llamadas al kernel
> él mismo: `crates/thalyx-bpf` lee ELF, BTF, la forma de los mapas y las
> reubicaciones CO-RE sin una línea de `unsafe`, y `thalyx-syscall` hace las
> cuatro llamadas. Ver [[Cargador-BPF-Propio]].
>
> **Nada de esto se ha ejercido.** El contenedor no tiene BPF LSM. La etapa 14
> de `verify.sh` es donde se comprueba, y es lo siguiente que hay que correr.
>
> ## Y el cargador funciona — 2026-08-03, dos fallos después
>
> La etapa 14 en la máquina de Cesar: **cargó, atachó, dejó los mapas donde
> `permd` los busca y se soltó limpio.** El cargador propio es real.
>
> Los dos fallos que costó están en [[Cargador-BPF-Propio]]. El segundo importa
> más que el primero porque no era del cargador: **la demo de denegación se negó
> a correr contra enforcement que estaba vivo**, tres líneas después de que la
> misma etapa demostrara que lo estaba. Preguntaba por un directorio que solo
> crea `bpftool`.
>
> Y tirando de ahí apareció algo peor, que llevaba puesto desde antes: **la
> sesión reportaba enforcement preguntándole a `bpftool` si había un mapa
> fijado.** Dos errores en una línea — la imagen no tiene `bpftool`, así que
> adentro contestaba «no» pasara lo que pasara; y un mapa fijado es un lugar
> donde poner permisos, no algo que los lea. Una máquina con todo fijado y nada
> atachado habría reportado enforcement.
>
> Ahora Thalyx le pregunta al kernel qué programas suyos corre un enlace vivo, y
> lo hace con llamadas `bpf(2)` propias, así que **funciona dentro de la imagen**.
> Hay una respuesta más que antes: *parte de los hooks vivos*, que se nombra
> aparte porque es peor que ninguno.
>
> ## Y la etapa 14 salió verde entera — 2026-08-03
>
> `proven 72 · not proven 2 · failed 0`. **Thalyx carga su propio enforcement,
> lo atacha, y ese enforcement deniega**: una conexión negada adentro del cgroup
> y permitida afuera, contra hooks que puso Thalyx y no `bpftool`.
>
> **Ese era el último hueco de arquitectura de la Fase 1.** Lo que queda sin
> probar no es arquitectura: es que llegue el modelo de verdad.
>
> Las dos cosas que la máquina de Cesar no puede establecer siguen siendo las
> mismas: `llama.cpp` no está instalado y el camino del modelo real no está
> escrito, y `verify.sh` no arranca la imagen.
>
> ## Y la máquina ya puede instalar, confirmar y revertir — 2026-08-03
>
> **El objetivo es cerrar la Fase 1**, y el criterio de salida no es una lista
> de componentes: son [[Criterio-de-Salida-Fase-1|seis cosas que hace una
> persona ajena]]. De esas seis, tres pasaban por la sesión y ninguna se podía
> hacer: adentro no hay shell, así que lo que no es un verbo de la sesión no
> existe para esa persona. La sesión entendía seis palabras y ninguna era
> `instalar`.
>
> Ahora entiende `disponibles`, `instalar <id>`, `permisos` y `revertir`. Nada
> de la lógica es nueva —el repositorio local, el camino confiable y el rollback
> ya estaban escritos— y ese era justo el problema: **estaban escritos y no
> alcanzables.**
>
> Y el disco cambió de contenido: lleva el módulo **en un repositorio, sin
> instalar**. Una máquina que arranca con él puesto vuelve el paso 2
> irrealizable, y el paso 3 —el camino confiable— nunca se alcanza. Hay una
> prueba que lee `image/Makefile` para que eso no se deshaga sin que nadie lo
> note, porque deshacerlo mejora lo que la máquina *aparenta*: arrancaría
> listando un módulo.
>
> La etapa 15 maneja el prompt de verdad, con un pty, y trae el control que
> hace falta: **responder que no no instala.** Sin eso, una sesión que
> instalara pase lo que pase pasaría todas las demás comprobaciones.
>
> ## Y construir esto ya no necesita bpftool — 2026-08-03
>
> Cesar decidió mandarle la máquina a una persona ajena **cuando los seis pasos
> sean reales**, no antes. Eso convirtió cada dependencia de construcción en un
> sitio donde esa persona se atora, y la peor era `bpftool`: en Ubuntu y
> derivados —Linux Mint, en este caso— viene en `linux-tools-$(uname -r)`, un
> paquete por versión de kernel cuyo nombre a menudo no coincide con el que está
> corriendo. Y se topaba con eso **después** de compilar un kernel entero.
>
> `lsm/vmlinux.h` ahora está escrito a mano: nueve structs, que es lo que los dos
> programas tocan, en vez de las cien mil líneas que generaba bpftool. Ver
> [[Cargador-BPF-Propio]] y la regla nueva en [[Estrategia-de-Pruebas]] — porque
> esto abrió una forma de mentir sin síntoma, y hay una prueba que la muerde.
>
> ## Y los seis pasos existen — 2026-08-04
>
> **Se puede hacer el criterio de salida entero.** Faltaban dos pasos y los dos
> eran lo mismo: las piezas estaban escritas y no había cómo alcanzarlas.
>
> **El paso 6 no tenía nada detrás.** La sesión no escribía nada en la memoria
> persistente al instalar, así que reiniciar no perdía el contexto — no había
> contexto. Ahora `instalar` y `revertir` escriben por el mismo
> `recollection.rs` que usa `thalyx agent do --task`, y `recuerdos` lo lee. Todo
> vive en `<store>/state/`, que es el subvolumen `system`, que viene del disco:
> hay una prueba que lo afirma contra la tabla de montajes, porque una memoria
> en el tmpfs se ve idéntica hasta el momento de apagar, que es el único que le
> importa al paso 6.
>
> Lo que sale después de instalar, reiniciar y `revertir`:
>
> ```
>   About `session`, you told me:
>     · the human asked: instalar dev.thalyx.greeter
>     · the human asked: revertir
>
>   And this I remember but can no longer confirm:
>     ? installed dev.thalyx.greeter 1.0.0
> ```
>
> **Eso es lo que distingue una memoria de una bitácora**, y es lo que hace que
> el paso 6 valga sin modelo: nadie le avisó que el módulo se fue. El hecho
> quedó atestiguado contra el enlace `current`, `revertir` lo quitó, y la
> máquina fue a ver. Lo que se le pidió sigue intacto porque ningún archivo
> puede volver falso que alguien lo haya dicho.
>
> Cesar decidió el 2026-08-04 que **eso es el paso 6** y que el modelo real deja
> de bloquear la fase. Sigue decretado en [[Gamas-de-Modelo]]. El razonamiento
> está en [[Criterio-de-Salida-Fase-1]].
>
> **El paso 1 tenía máquina y no tenía camino.** `make -C image doctor` junta
> todas las herramientas que faltan y las contesta con una línea de `apt`, sin
> descargar ni compilar nada, y `all` depende de él primero. Lo que detiene a la
> persona ajena nunca es Thalyx: es un paquete, encontrado de uno en uno, cada
> uno después de que lo anterior salió bien. El peor era `pahole` — sin él
> Kconfig descarta `DEBUG_INFO_BTF` en silencio y la culpa cae sobre el
> cargador. El README tiene la sección **Boot it**, que son los seis pasos y
> nada más.
>
> **Un defecto encontrado al hacerlo**, y dio regla nueva: la frase que explica
> un hecho no confirmable decía que algo había cambiado *"without going through
> Thalyx"*, y con `revertir` esa causa dejó de ser cierta. Ninguna prueba se
> rompió. Ver la regla del mensaje que nombra la causa en
> [[Estrategia-de-Pruebas]].
>
> **Nada de esto se ha corrido en hardware.** Es lo siguiente: `sudo
> ./dev/verify.sh`, donde la etapa 15 creció seis comprobaciones, y después
> `make -C image run`.
>
> ## La imagen arrancó con el cargador propio, y le falta un hook — 2026-08-04
>
> `make -C image run` en la Fedora de Cesar. **El cargador funcionó**: llegó
> hasta preguntarle al kernel por sus hooks y dijo exactamente cuál falta.
>
> ```
> no  thalyx-lsm  this kernel does not expose `bpf_lsm_socket_connect`
> ```
>
> `thalyx.config` tenía `CONFIG_SECURITY` y no `CONFIG_SECURITY_NETWORK`. Todos
> los hooks de socket de `lsm_hook_defs.h` están adentro de ese `#ifdef`, así que
> el símbolo **nunca se compiló**. Arreglado: la línea está en `thalyx.config`
> con su párrafo.
>
> Y `config-check` pasó en verde, correctamente — compara lo pedido contra lo
> obtenido, y nadie había pedido esa opción. **Un punto ciego con forma propia**,
> ver la regla nueva en [[Estrategia-de-Pruebas]]. Ahora existe `hook-check`: le
> pregunta al objeto BPF a qué símbolos se engancha (`thalyx enforce hooks`) y
> los busca en el `System.map` del kernel recién compilado, antes de arrancar
> nada. Probado con sus tres respuestas — falta uno, están los dos, y no hay
> kernel construido.
>
> **El resto del arranque salió bien**, y es la primera vez: de tres `no` a dos,
> con el store de Btrfs montado y `filesystem btrfs`.
>
> **Y la etapa 15 se saltó entera**: Fedora no trae `script` — está en
> `util-linux-script`. Los siete controles del paso 6 no corrieron, así que ese
> trabajo sigue sin ejercer en hardware.
>
> ## Y el hook existía y no se le podía enganchar nada — 2026-08-04
>
> Con `CONFIG_SECURITY_NETWORK` puesto, el símbolo apareció y el arranque falló
> un paso más adelante:
>
> ```
> no  thalyx-lsm  attaching `thalyx_socket_connect`: Resource busy (os error 16)
> ```
>
> Faltaba `CONFIG_FUNCTION_TRACER`. BPF se engancha a un hook LSM con un
> trampolín, y sin ftrace dinámico el kernel parcha el texto él mismo esperando
> el NOP de cinco bytes que esa opción pone al principio de cada función. No
> estaba, el `memcmp` falló, y ese camino devuelve `EBUSY` — que se lee como que
> algo más tiene el hook tomado, y no había nada.
>
> `CONFIG_FTRACE=y` ya estaba y es solo el menú: no emite ningún NOP.
>
> Dos arreglos, y el segundo importa más:
>
> 1. Las cuatro líneas en `thalyx.config`, con `DYNAMIC_FTRACE_WITH_DIRECT_CALLS`
>    pedida explícitamente aunque sea derivada, para que `config-check` reporte
>    si no se materializa.
> 2. **`hook-check` pregunta por el artefacto**: `register_ftrace_direct` solo se
>    compila bajo esa opción, así que su presencia en el `System.map` *es* la
>    propiedad. Probado con sus dos respuestas.
>
> Y el mensaje del cargador ahora dice **las dos** causas de `EBUSY` en ese
> camino y que no las puede distinguir. Con su control: otro errno no lleva el
> párrafo.
>
> **Ya son tres opciones del kernel encontradas arrancando**, y la regla nueva en
> [[Estrategia-de-Pruebas]] dice por qué ninguna comprobación de construcción va
> a encontrar la cuarta.
>
> ## Y ahora `verify.sh` arranca la máquina — 2026-08-04
>
> Decidido por Cesar después del tercer arranque a mano. **La etapa 16 arranca
> la imagen en QEMU y teclea los seis pasos**: espera a que la máquina diga que
> es la máquina, y escribe `recuerdos`, `disponibles`, `instalar`, la
> confirmación, `permisos`, `correr`, `revertir`, `apagar`. Después arranca otra
> vez y pregunta `recuerdos`.
>
> **Dos arranques, porque eso es lo que dice el paso 6.** Un proceso nuevo no es
> un reinicio; lo único que cruza entre los dos es el disco.
>
> Y la consola serie **es** un terminal: lo que ve el invitado es `/dev/console`
> sobre `ttyS0`, sea lo que sea el stdin de QEMU. Así que el camino confiable se
> ejerce como lo encuentra una persona, sin `script` de por medio.
>
> El disco se copia primero. Arrancar lo modifica, y una etapa que cambiara el
> disco que alguien construyó haría que la segunda corrida empezara desde otro
> lado que la primera.
>
> `make -C image boot` es lo que corre, y **no construye nada**. `run` depende
> del kernel y de la imagen, y la regla del binario depende de `toolchain`, que
> es `.PHONY` — así que pedir `run` puede arrancar un `cargo build`, y bajo
> `sudo` eso corre como root y deja archivos de root en `target/`. Es el mismo
> fallo por el que `store` se partió en dos, y la misma regla: **la frontera de
> privilegio es la frontera de target.**
>
> **El arnés se ejerció contra una máquina falsa** —una que se queda callada
> hasta estar lista, para que teclear temprano se note, y una que se muere de
> inmediato, que tiene que volver como «nunca llegó al prompt» y no como un
> cuelgue—. La etapa en sí **nunca ha corrido contra una imagen de verdad**: el
> contenedor no tiene QEMU ni kernel que arrancar.
>
> ## La imagen atachó su enforcement, y se negó a usarlo — 2026-08-04
>
> **La etapa 16 corrió por primera vez y sirvió de inmediato.** Lo que salió en
> verde, todo dentro de la máquina y sin shell: arrancó, **atachó su propio
> enforcement** (`ok thalyx-lsm` — el tercero de los tres `no` del primer
> arranque, cerrado), dijo que no recordaba nada, listó su repositorio, presentó
> el camino confiable, instaló sobre su disco Btrfs, revirtió, y se apagó sola.
>
> Y falló en una: **`correr` se negó**, diciendo que el mapa de política no
> estaba cargado — tres líneas después de reportar el enforcement puesto.
>
> `BpftoolStore::is_available()` corría `bpftool map show pinned`. **Adentro no
> hay `bpftool`**, así que contestaba «no» pasara lo que pasara, y esa respuesta
> es la que decide entre confinar un módulo y negarse a arrancarlo. El
> enforcement era real y lo único que no podía verlo era el código que decidía
> si usarlo.
>
> Peor: `set()` también escribía con `bpftool`, así que **ninguna política se
> podía escribir adentro de la imagen**. La comprobación estaba equivocada y
> además tenía razón.
>
> `KernelStore` lo reemplaza entero: `BPF_OBJ_GET` sobre el pin y
> `MAP_UPDATE_ELEM` / `MAP_DELETE_ELEM` / `MAP_LOOKUP_ELEM`, con la rama de
> `union bpf_attr` capturada verbatim del uapi y una prueba que calcula sus
> offsets desde la captura. **`BpftoolStore` se borró**: dos implementaciones de
> lo mismo terminan por no coincidir, y aquí el desacuerdo sería una máquina que
> aplica permisos en un lado y no en el otro.
>
> Es la **cuarta** vez que algo le pregunta a `bpftool` por algo que `bpftool`
> no hizo. La tabla completa está en [[Estrategia-de-Pruebas]].
>
> **Y el arnés de la etapa 16 tenía su propio fallo**, que Cesar notó antes de
> que terminara: se quedaba callado y colgado. `wait` no alcanzaba, porque la
> sesión termina con EOF y **PID 1 no** — sigue cosechando huérfanos, como debe,
> así que QEMU nunca salía. Ahora imprime cada 15 segundos qué está esperando y
> mata QEMU por la ruta del disco, que es de esa corrida y de nada más.
>
> ## Y el módulo pedía un perfil que no existe — 2026-08-04
>
> Con `KernelStore` puesto, la etapa 16 volvió a correr y volvió a fallar en la
> misma línea, **por otra razón**:
>
> ```
>   dev.thalyx.greeter did not run: `default` is not a sandbox profile Thalyx knows
> ```
>
> `session.rs` tenía el nombre del perfil escrito a mano —`"default"`— en lugar
> de tomado de `thalyx_sandbox::profile::MODULE_STANDARD`, que es lo que hace
> `main.rs` tres archivos más allá. **Ningún perfil se llama `default`.**
>
> Vivió ahí desde que el prompt puede correr un módulo, con los 599 tests en
> verde, y salió **en la consola de la máquina, después de que la instalación ya
> había salido bien** — el peor lugar posible para encontrarlo.
>
> Lo escondió el **orden**, no el descuido: el nombre se resolvía *después* de
> comprobar que el mapa de política estuviera cargado. En toda máquina sin ese
> mapa —todas menos la imagen— contestaba primero la puerta, con una respuesta
> honesta, y el nombre no se miraba nunca. Ahora `resolve` va antes: un nombre
> que no existe es un nombre que no existe en cualquier máquina.
>
> Y la razón de fondo es la regla 1 otra vez: **la etapa 15 maneja el prompt de
> verdad y no tecleaba `correr`.** Era el único verbo sin ejercitar, y era el
> único roto. Ahora lo teclea.
>
> **El mensaje de la falla apuntaba al lado equivocado**: decía que la imagen no
> tiene `bpftool`, que era cierto la corrida anterior y ya no. La causa real
> estaba impresa cuatro renglones debajo. Ese texto se quitó. Ambas reglas
> quedaron en [[Estrategia-de-Pruebas]].
>
> ## Y nadie le había entregado los controladores — 2026-08-04
>
> Arreglado el perfil, la misma línea falló un paso más adelante:
>
> ```
> `/sys/fs/cgroup/thalyx` cannot hand down the controller(s) ["memory", "pids"]
> It has: []
> ```
>
> La negativa era correcta —sin esos controladores los límites no se aplican y
> el módulo se ve acotado sin estarlo— y lo que faltaba era que **alguien los
> delegara**. En cualquier otro Linux lo hace systemd antes de que corra nada.
> **En la imagen no hay systemd.** No hay nada más que Thalyx.
>
> Que es el decreto fundacional dicho de otro modo: todo lo que otra cosa hacía
> por nosotros es ahora trabajo de Thalyx, lo hayamos notado o no. Y no se
> encuentra leyendo el código —el código no menciona systemd en ninguna parte—
> sino corriendo en la única máquina donde systemd no está.
>
> PID 1 ahora los delega al montar, con la lista tomada del perfil bajo el que
> corren los módulos. Y **la sesión lo reporta**: `cgroup2` decía `mounted at
> /sys/fs/cgroup` en una máquina donde ningún módulo podía recibir un límite —
> pantalla de arranque limpia, primer `correr` roto. Eso es el fallo sin
> síntoma, en el único lugar construido para no tener ninguno.
>
> ## El módulo corrió confinado, y se cayó montando su archivo — 2026-08-04
>
> **El `pivot_root` funcionó sobre el initramfs.** Era la duda que quedaba, y
> salió bien: el módulo obtuvo su cgroup, su raíz propia, su usuario propio,
> seccomp con 128 llamadas y sus límites de memoria y procesos, adentro de la
> máquina. Lo que falló es el último syscall de un montaje:
>
> ```
> could not attach the remapped mount at
> /run/thalyx/sandbox/opt/thalyx/data/greeter/notes.txt: Invalid argument
> ```
>
> El kernel exige que el punto de montaje de un archivo sea un archivo
> (`do_move_mount`: `d_is_dir(new) != d_is_dir(old)` → `EINVAL`). `bind` lo
> sabía; `bind_remapped` llamaba a `create_dir` sin mirar. Dos funciones que
> obedecen la misma regla del kernel, escritas por separado.
>
> Sobrevivió porque **todos los permisos de todas las pruebas son directorios**.
> El único permiso sobre un archivo suelto es el del `greeter`, y el único lugar
> donde el `greeter` corre con usuario propio es la imagen. Un caso de prueba
> que nunca varía no es un caso de prueba. Ver [[Estrategia-de-Pruebas]].
>
> ## Y nadie había hecho el `switch_root` — 2026-08-04
>
> El montaje del archivo funcionó y `pivot_root` devolvió `EINVAL`, con el
> módulo ya en su cgroup, con su política, su usuario, sus namespaces, seccomp
> y sus límites. Todo bien menos el último paso.
>
> `do_pivot_root`: `if (!mnt_has_parent(root_mnt)) goto out4;`. **La raíz de un
> namespace de montajes no tiene padre.** En cualquier otro Linux eso no se ve,
> porque la raíz del proceso no es la raíz del namespace — el kernel arma un
> `rootfs` interno y el initramfs monta el sistema real encima con
> `switch_root` antes de que arranque nada.
>
> **La imagen es un initramfs y nada más.** Su raíz de proceso *es* la raíz del
> namespace, y lo sigue siendo después de `unshare`. Nadie había hecho el
> `switch_root` porque en todas las demás máquinas ya estaba hecho — que es
> exactamente lo que pasó con systemd y los controladores dos rondas antes.
>
> PID 1 lo hace ahora, con un bind en lugar de un tmpfs: comparte los mismos
> inodos y las mismas páginas, así que no cuesta memoria. **Se comprobó
> corriéndolo** con los mismos envoltorios de `thalyx-syscall` dentro de un
> namespace desechable: el cambio sale bien, la raíz pasa a tener padre, y
> `pivot_root` después funciona.
>
> Y la máquina lo dice de sí misma: el arranque imprime que el cambio corrió y,
> por separado, que la raíz resultante sirve —leído del kernel, no inferido— y
> la sesión toma la misma lectura.
>
> ## Lo que sigue sin verse
>
> **Que el módulo hable.** Lo que falta es que el `greeter` lea su archivo, pida
> `/etc/shadow`, sea negado, y lo diga por su canal — que es la línea que la
> etapa 16 busca. Todo lo que hay debajo ya se vio funcionando adentro de la
> máquina.
>
> El procedimiento sigue en [[Primer-Arranque]]. Si Cesar pega la salida de un
> comando, casi siempre es de ahí.

## Dónde estamos, en una frase

**El 2026-08-03 se quitó la distribución.** La bóveda decretaba en tres notas
una base Alpine y en una —marcada no negociable— que Thalyx no es una
distribución de Linux. Se resolvió a favor de la segunda: **la imagen es el
kernel de Linux y `thalyx`, y nada más.** Ninguna distro, nunca. Ver
[[Construccion-del-ISO]].

Eso convirtió la **API interna de módulos** en la pieza que seguía: sin shell y
sin utilidades, un módulo no puede ser un script y no tiene con quién hablar
excepto Thalyx. **Diseñada y construida el 2026-08-03** en
[[API-Interna-de-Modulos]]: protocolo, servidor, el canal por el sandbox, y
`dev.thalyx.greeter`, el primer módulo escrito contra ella.

La Fase 1 tiene **sus tres primitivas** —de las cuatro decretadas; la cuarta es
el [[Scheduler-Predictivo]] y es de Fase 2— y su flujo canónico **construidos y
verificados en hardware real**: 44 comprobaciones en máquina real. Desde
entonces: **520 pruebas**, el agente mínimo que lleva un enunciado hasta un
módulo instalado sin modelo alguno, `thalyx` como PID 1, la imagen que Thalyx
construye para sí mismo, y el disco donde guarda lo que le instalan.

**Los huecos de arquitectura de la Fase 1 están cerrados.** El último era el
enforcement dentro de la imagen; el cargador propio salió verde en hardware el
2026-08-03.

> Matiz del 2026-08-04, al final del día: seguía siendo cierto que no falta
> código **del sistema**, y era falso que no faltara código para *comprobarlo*.
> Cuatro de los seis pasos no se estaban verificando en ningún lado, porque la
> única etapa que los cubría se saltaba sola. Ya son pruebas.

**Y desde el 2026-08-06 los seis pasos están hechos por la máquina, desde un
arranque frío, con un reinicio de verdad en medio.** En esa corrida no quedaba
código sin ejercer: `proven 104 · not proven 1 · failed 0`.

> Y desde el 2026-08-07 la máquina puede hacer el disco en el que guarda:
> **el kernel monta el Btrfs que Thalyx escribió byte por byte.** Etapa 18, en
> verde. Lo que queda sin ejercer no es código del sistema: son los tres
> subvolúmenes, que todavía no están escritos.

**Y ahora la máquina puede hacer el disco en el que guarda**, que es el punto 2
del ISO y el poste largo de los tres. Falta que ese disco se vuelva un store.

**Lo que falta para cerrar la fase ya no es código y tampoco es la persona
ajena**, que Cesar canceló ese mismo día. Es **elegir con qué se sustituye** —
ver el punto 0 de "Lo que sigue". El modelo del agente sigue decretado en
[[Gamas-de-Modelo]] y no bloquea la fase, por decisión de Cesar del 2026-08-04.

## Lo que falta comprobar

Escrito aparte para que no se confunda con lo que sí está probado:

| Qué | Estado |
|---|---|
| El mecanismo del store | **Probado**, etapa 13, en verde el 2026-08-03. |
| El store arrancando en QEMU | **Probado**, arrancó con el disco montado y el módulo instalado. |
| El cargador de BPF propio | **Probado**, etapa 14, en verde entera el 2026-08-03. |
| El paso 6 | **Probado el 2026-08-06**, etapa 16, desde un arranque frío y con un reinicio de verdad. Thalyx hace su propia terminal, así que ya no depende de `script`. |
| El `doctor` | **Corrido el 2026-08-06** en la máquina de Cesar: encontró el ancla del kernel ausente y nada más. |
| La imagen con enforcement puesto | **Probado el 2026-08-06**: `ok thalyx-lsm` dentro de la máquina, con el kernel recompilado. |
| Los arreglos de la auditoría por la ruta confinada | **Probados el 2026-08-06**, etapas 6, 12, 13 y 17 enteras. |
| El Btrfs que Thalyx escribe | **Probado el 2026-08-07**, etapa 18: el kernel lo monta, acepta los tres subvolúmenes, y un archivo escrito en él vuelve. Con `btrfs check` en verde y los headers de uapi capturados. |
| Los tres subvolúmenes desde dentro | No construido. Van por ioctl, y hasta entonces un store escrito por Thalyx es un filesystem y no un store. |
| `thalyx_watch` cargado sin bpftool | No intentado. Diez hooks en vez de dos; el mismo cargador debería servir. **Es lo único de la lista que sigue abierto.** |

## Los cuatro fallos del camino, y por qué tres son el mismo

Entre que el store quedó escrito y que la máquina arrancó con él montado, nada
de lo que falló fue del sistema. Los cuatro fueron del constructor:

1. **`sudo make store` no encontraba `rustup`.** `sudo` reinicia el `PATH`. Eso
   es lo chico: de haber funcionado habría corrido toda la compilación de Rust
   como root, con los scripts de build de cada dependencia con privilegio y
   archivos de root en `target/`. **La frontera de privilegio es la frontera de
   target** — `store-stage` construye, `store` formatea y se niega a construir.
2. **`NOT STATIC` sobre un binario perfectamente estático.** La comprobación era
   `file | grep 'statically linked'` y Rust enlaza musl como *static-pie*, que
   `file` llama `static-pie linked`. Ahora lee el segmento `INTERP` del ELF.
3. **QEMU no pudo abrir el disco.** La comprobación era `test -r` y QEMU abre el
   disco para escribir. Y el disco había quedado de root: son **dos
   pertenencias distintas** —el archivo es del host, lo de adentro es de la
   máquina— y confundirlas da un store que o QEMU no abre o la máquina no posee.
4. **Backticks dentro de un mensaje de ayuda**, dos veces. `echo "corre \`sudo
   make store\`"` es sustitución de comandos: el mensaje que explica qué correr
   lo habría corrido.

El 2, el 3 y el 4 son **la misma regla**: comprobar un sustituto de la
propiedad en vez de la propiedad, o escribir sobre una herramienta en vez de
preguntarle. Está escrita en [[Estrategia-de-Pruebas]].

Y hay una lección de arriba de todas: **el 2 mintió durante un rato y la máquina
ya lo había desmentido.** La imagen había arrancado con ese mismo binario como
`/init`; uno dinámico habría dado `No working init found`. Cuando una
comprobación contradice algo que la máquina ya demostró, la comprobación es la
sospechosa. Van siete.

Hay pruebas para los cuatro, y tres de ellas leen el `Makefile`.

## Última corrida verificada

**2026-08-07, Fedora 43, kernel 7.1.5, Btrfs, `bpf` en el orden de LSM,
`main @ 9229268`.**

```
proven 110 · not proven 1 · failed 2
```

**Cerró la etapa 18**: el kernel monta el Btrfs que Thalyx escribió byte por byte,
acepta los tres subvolúmenes, y un archivo escrito en él vuelve. Los dos fallos
eran del arnés y no del formato — el control de la etapa 18 dañaba espacio libre
por copy-on-write, y clippy falló sin dejar rastro porque el script borra su propio
directorio al salir. Los dos están arreglados.

### Y la corrida corta que resolvió lo de clippy

**2026-08-07, la misma máquina, con el informe arreglado.** Un solo fallo, y esta
vez con el lint impreso: `unnecessary_sort_by` en `crates/thalyx-btrfs/src/format.rs`,
dos veces. **Era desfase de versión** —clippy 1.97 contra 1.94— y no lo que se
había supuesto. Arreglado, y el contenedor actualizado a 1.97 para que el próximo
lint nuevo no se descubra otra vez en la máquina que no puede arreglarlo.

**La etapa 19 está sin correr.** Es la que comprueba que Thalyx crea los tres
subvolúmenes por ioctl, y no la puede correr ningún otro sitio.

### La anterior, y es la que sigue siendo la referencia limpia

**2026-08-06, `main @ 9e1c5f8`.**

```
proven 104 · not proven 1 · failed 0
```

**Nada falló y nada quedó sin ejercer.** Corrida dos veces: la segunda con
`THALYX_REQUIRE_IMAGE_TESTS=1`, que convierte en fallo cualquier salto de la
etapa 16, con idéntico resultado — así que la etapa del arranque corrió de
verdad en vez de saltarse en silencio, que es la única forma en que un `104`
podría estar mintiendo.

La única `not proven` es `llama.cpp`, y es de la clase que **no existe**, no de
la que no se pudo comprobar.

Lo que esta corrida cerró y ninguna anterior había cerrado:

- Los diez fallos del 2026-08-05, todos, por la **ruta confinada** — que era lo
  que este contenedor no puede ejercer.
- El **paso 6** de punta a punta: un arranque frío, los seis verbos tecleados,
  el apagado, un reinicio de verdad, y la máquina diciendo sola que la
  instalación que hizo ya no le cuadra.
- El `doctor` corrido por primera vez en la máquina de Cesar, y el ancla del
  kernel establecida contra la lista firmada de kernel.org.
- Las 666 pruebas con los cuatro `THALYX_REQUIRE_*` que esa máquina puede
  exigir, ninguna saltada.

### La anterior, para comparar

**2026-08-05, `main @ f781ced`**: `proven 99 · not proven 2 · failed 10`. Los
diez fallos eran de Thalyx y eran dos defectos del mismo commit — ver el bloque
de la auditoría más arriba. Esa corrida fue la que los encontró.

**2026-08-03, kernel 7.0.11, `main @ f1a6dd0`**: `proven 72 · not proven 2 ·
failed 0`. Cerró el cargador de BPF propio: cargó los dos programas sin
`bpftool`, los enganchó, dejó los tres mapas donde `permd` los busca, **denegó
una conexión adentro del cgroup y la dejó pasar afuera**, y se soltó sin dejar
un enlace vivo. También ejerció el `EXDEV` en el que descansa el layout, con
línea base y control — porque una afirmación que sostiene un diseño hay que
ejercerla, no citarla.

Reproducirla:

```
git checkout main && git pull && cargo install --path crates/thalyx-cli && sudo ./dev/verify.sh
```

> **El encabezado dice qué commit se está probando.** Existe porque una corrida
> contra código viejo se ve idéntica a una donde el arreglo no funcionó: misma
> etapa, mismo fallo, mismo mensaje. Pasó — dos arreglos estaban en `main` y la
> máquina seguía en la rama de la que salieron. Si la línea no dice `main` y el
> commit que esperas, la corrida no significa nada.

## Qué quedó construido y probado

| Pieza | Comprobado en hardware |
|---|---|
| Instalación de módulos, commit atómico, journal, permisos | Sí, incluida inyección de fallos |
| `thalyx-lsm` (BPF LSM) | Sí — **deniega de verdad** una conexión dentro del cgroup y la permite fuera |
| Sandbox completo: namespaces, seccomp, `pivot_root`, idmap, límites | Sí — el módulo reporta su propio pid, uid, hostname, red y raíz |
| Un uid por módulo, nunca reutilizado | Sí |
| Índice en grafo + parser mecánico | Sí |
| Contador de mutaciones del kernel, 10 hooks | Sí — 5000 escrituras por descriptor abierto, todas contadas |
| Contador acotado al árbol | Sí — 5000 dentro contadas, 5000 fuera ignoradas |
| El atajo del índice (`graph trust`) | Sí — se gana con verificación, y un cambio real sigue saliendo obsoleto |
| Memoria persistente (3ª primitiva) | Sí — el hecho deja de ser afirmable al editar el archivo por fuera |
| `rollback` | Sí — quita el módulo y sus permisos; se niega la segunda vez |
| Snapshots de Btrfs | Sí — de solo lectura, conservan el contenido viejo |
| `restore` | Sí — restaura, destruye lo posterior, y conserva lo destruido |

Detalle por crate en [[Estado-de-Implementacion]].

## Lo que sigue, en orden

### 0. La ISO independiente — es lo que cierra la Fase 1

**Decretado por Cesar el 2026-08-06.** Una ISO que puesta en una PC sin sistema
operativo la deje corriendo Thalyx. Se ejercerá primero en una VM con firmware
UEFI de verdad, y eso prueba que arranca sola; los controladores necesitan
hierro y eso se dice aparte en vez de confundirse.

El diseño y lo que cuesta están en [[Construccion-del-ISO]]. El orden de trabajo,
por riesgo descendente:

1. ~~**Arrancar sin gestor de arranque.**~~ **Hecho y probado el 2026-08-06.**
   Un firmware arrancó Thalyx entera: `switch_root`, los siete montajes, los
   controladores, **el LSM enganchado** y la sesión. Ver el bloque de arriba.
2. ~~**El store, que Thalyx va a escribir él mismo.**~~ **Hecho y probado el
   2026-08-07**: el kernel monta lo que Thalyx escribió, y acepta los tres
   subvolúmenes creados sobre él — etapa 18, en verde.
   `crates/thalyx-btrfs`, invocado por `thalyx disk format`. El decreto de que PID
   1 nunca fabrica **se conservó entero**: quien crea el store es un humano y PID
   1 no alcanza ese código. Falta que el filesystem se vuelva un store, que son
   los tres subvolúmenes, y van por ioctl.
3. **El instalador**: tabla de particiones GPT, una partición EFI con el kernel,
   y el store en la otra. Cesar decidió que la máquina arranca **sin** la ISO
   después, así que hay que escribir Thalyx en el disco de la máquina. Va junto
   con el 2, porque lo que el instalador escribe es precisamente el store.
4. **La consola sobre el framebuffer y el teclado USB**, más almacenamiento real
   (NVMe, AHCI). Sin esto la máquina arranca en hierro y no se ve nada, que es
   el fallo que se lee como «no funciona» siendo «no puedes mirar». Va al final
   porque es lo único que **una VM no puede sustituir**, y hasta entonces todo se
   ejerce con OVMF.

### 1. El agente — su mitad determinista ya está construida

Ya no bloquea la fase; ver el paso 6 en [[Criterio-de-Salida-Fase-1]]. Va
primero de lo que queda, y el motivo es de descubrimiento, no de avance. El ISO desbloquea
cinco de los seis pasos del [[Criterio-de-Salida-Fase-1|criterio de salida]]
contra uno del agente, pero el ISO **integra piezas ya probadas: no puede
enseñar nada que no se sepa ya**. El agente sí puede invalidar el diseño del
contrato. Descubrir tarde que la procedencia por campo no sobrevive a varias
inferencias costaría mucho más que un ISO retrasado, y la regla 1 de
`CLAUDE.md` dice que todos los defectos reales salieron de correr el sistema.

Alcance: router de reglas más un modelo con decodificación restringida por
gramática, sobre **un solo caso de uso** —instalar un módulo—, no un agente
general.

**Construida ya la mitad que no necesita un modelo**, y probada de punta a
punta: `thalyx agent do "install dev.thalyx.demo@^1.0" --repo <dir>` resuelve
contra un repositorio local de bundles firmados, pide confirmación por el camino
confiable, y deja el módulo instalado y ejecutable. Lo que falta, en orden:

1. El `Model` real que invoca `llama.cpp` como proceso.
2. La gramática GBNF, que no se puede validar sin `llama.cpp`.
3. El banco de las cuatro gamas, para sustituir las cifras estimadas.

Los tres necesitan tu máquina: aquí no hay `llama.cpp` y la política de red del
entorno bloquea `huggingface.co`.

**El decreto que lo bloqueaba ya está escrito:** [[Gamas-de-Modelo]]. No un
modelo anclado sino **cuatro gamas de una sola familia** que el usuario elige
según su hardware, con `llama.cpp` invocado como proceso y decodificación
restringida por gramática. Anclar un modelo de 5 GB dejaría fuera a una máquina
de 8 GB, y el criterio de salida exige justamente que alguien de fuera lo use.
Con la gramática, un contrato mal formado es imposible en las cuatro gamas: lo
que cambia entre ellas es el acierto al interpretar la intención, no la
seguridad. Y **el modelo nunca escribe la procedencia** — la pone el
ensamblador, porque una gramática obliga a la forma y no a la verdad.

El alcance del primero está en [[Agente-Minimo]].

Lo que sí está listo para el agente cuando exista: el contrato estructurado con
marcado de origen, el camino confiable, la memoria persistente, y el principio
de doble ruta implementado (todo lo que el agente podrá hacer, un humano ya
puede hacerlo por la CLI).

### 2. La imagen: las tres cosas que le faltaban están hechas

**Cerrado el 2026-08-06.** El primer arranque se describió con tres `no` y los
tres están resueltos y comprobados dentro de la máquina:

```
  ok  kernel       6.12.101
  no  filesystem   rootfs — snapshots and restore need btrfs and will not work here
  ok  cgroup v2    mounted at /sys/fs/cgroup
  ok  lsm order    capability,bpf
  no  enforcement  the policy map is not loaded, so no permission would be enforced
  no  modules      nothing installed yet

  3 are not here. I will not pretend otherwise later.
```

Las tres, en el orden en que se resolvieron:

1. ~~**Cargar `thalyx-lsm` desde dentro de Thalyx.**~~ **Hecho, y probado dentro
   de la imagen el 2026-08-06**: `ok thalyx-lsm` al arrancar, sin `bpftool` y
   sin shell. El objeto BPF va **dentro** del binario, no junto a él, que es lo
   que [[Filosofia-Fundacional]] obliga. Ver [[Cargador-BPF-Propio]].
2. ~~**El store.**~~ **Hecho el 2026-08-03.** El disco se hace al construir con
   `sudo make -C image store` —Btrfs, tres subvolúmenes, el `greeter` instalado
   adentro— porque `mkfs.btrfs` no puede estar en la imagen, que es la misma
   forma que el problema del LSM y la misma respuesta: el trabajo se mueve al
   momento de construir. PID 1 lo monta por `thalyx.store=` y nunca lo crea. Ver
   [[Construccion-del-ISO]] y la tabla de montajes en [[Journal-y-Snapshots]].
3. ~~**La API interna de módulos.**~~ **Hecha el 2026-08-03.** Protocolo,
   servidor, el canal atravesando el sandbox y `dev.thalyx.greeter`. Ver
   [[API-Interna-de-Modulos]].

### 3. Reindexado incremental

Consumir el ringbuf `thalyx_mutations` para saber *qué* cambió, no solo que
algo cambió. **Ya no hace falta para el atajo** —eso lo resolvió la atribución
por ancestros— así que es una mejora de rendimiento, no de corrección. Ver
[[FS-en-Grafo]].

## Decretos abiertos

Ninguno bloquea excepto el primero.

- [ ] **Una frontera real que etiquete canales** — hoy `--foreign` es una bandera que un humano pasa a propósito; nada en Thalyx llama a `Segment::foreign()` por su cuenta, porque nada trae texto de terceros todavía. Toda la defensa de procedencia descansa sobre ese código, que no existe.
- [ ] **Correr el banco de las gamas** — el decreto ya está ([[Gamas-de-Modelo]]); faltan las cifras medidas. Necesita `llama.cpp` y los pesos, que el contenedor de desarrollo no puede tener.
- [ ] Métricas de benchmark de la Fase 2 (el umbral ya está decretado; falta el instrumento)
- [ ] Técnicas de interpretabilidad aplicables al agente
- [ ] Arquitectura del índice semántico a mayor escala (SQLite alcanza para Fase 1)
- [ ] Sistema de reputación resistente a Sybil (pospuesto a propósito)
- [ ] Dependencias entre módulos con backtracking (pospuesto hasta que un módulo real las necesite)
- [ ] Condiciones para habilitar llamadas a modelos remotos

Lista completa y viva en [[Tareas-Pendientes]].

## Lo que sigue sin validarse, y se carga a propósito

**Ningún decreto de esta bóveda ha sido contrastado con una persona ajena al
proyecto.** Todo el razonamiento sobre por qué alguien elegiría Thalyx sigue
siendo a priori. Eso es cierto y sigue siendo un riesgo real.

**Y no se adelanta.** El [[Criterio-de-Salida-Fase-1|criterio de salida]] pone a
esa persona *después* del ISO, arrancando la imagen: ese es su paso 1. Nadie de
fuera toca el sistema antes. No por miedo a lo que diga —el proyecto nunca
dependió de eso— sino porque lo que esa persona determina es **la escala, no la
validez**, y esta fase es incompatible con la escala.

El riesgo se lleva con los ojos abiertos hasta entonces, que no es lo mismo que
ignorarlo. El razonamiento completo, y la deriva concreta que previene, están en
[[Criterio-de-Salida-Fase-1]].

Ver también [[Por-Que-Elegirian-Este-SO]] y [[Riesgo-de-Ejecucion]].

## Cosas que hay que saber para no romper nada

**El watcher del LSM es todo o nada.** Diez hooks; si el kernel no expone
alguno, declina cargarse entero en vez de cargarse pareciendo completo. Un hook
faltante no es un número más chico, es una forma concreta de que un archivo
cambie en silencio. `make -C lsm hooks` dice cuáles hay.

**`verify.sh` desengancha el LSM al salir.** Por eso `thalyx graph watcher`
dice "not loaded" después de una corrida. Es correcto, no es un fallo.

**`verify.sh` compila en `dev/.verify-target`** para no dejar el `target/` del
usuario a nombre de root. Por eso el binario que queda en el PATH es el de
`cargo install`, y hay que reinstalarlo después de cambios en la CLI.

**El store por defecto es `/opt/thalyx`**, que necesita sudo. Para uso normal:
`export THALYX_ROOT=~/.local/share/thalyx`.

**El atajo del índice está apagado por defecto en cada índice nuevo**, y
`verify.sh` reconstruye el índice del repo, así que vuelve a apagarse en cada
corrida. Para encenderlo a mano:
`thalyx graph trust ~/thalyx/crates --counter`.

## Historial de sesiones

### 2026-08-06 — la primera corrida sin nada roto, y el criterio se queda sin persona
`proven 104 · not proven 1 · failed 0`, dos veces, la segunda exigiendo que la
etapa del arranque no se saltara. Todo lo que existe está comprobado en la única
máquina que puede comprobarlo. La única `not proven` es algo que no existe.

**Y el único defecto del día lo encontró un humano leyendo instrucciones.**
`pin-kernel` mandaba a verificar `sha256sums.asc` con las llaves de los
mantenedores del kernel, que no firman ese archivo, así que gpg contestó `No hay
clave pública` justo debajo de la frase que define esa salida como motivo de
parar. Escrito en un contenedor sin ruta a kernel.org y publicado sin correr:
**texto impreso para una persona es código con salida**, y va la regla nueva en
[[Estrategia-de-Pruebas]].

El ancla quedó en el repositorio con la llave y la huella que la establecieron,
porque un digest solo dice qué se aceptó, no qué lo validó.

**Y Cesar canceló la persona ajena.** Los seis pasos siguen; quien los teclea,
no. Eso deja a la Fase 1 sin criterio de salida hasta que él elija el
sustituto, y está escrito así en vez de dejar que la nota aparente tener uno.

### 2026-08-05 — los arreglos de la auditoría rompieron el instrumento
`verify.sh` con la auditoría puesta: `failed 10`, todos de Thalyx. Dos defectos,
los dos del mismo commit, y los dos con la misma forma vista de dos lados: **una
defensa correcta aplicada a algo que no era lo que creía estar protegiendo.**

Quitarle la terminal al módulo era correcto y le quitó también el `stdout` por
el que contesta qué ve desde adentro del sandbox, que es el único instrumento
que prueba el aislamiento. Truncar a 72 caracteres es correcto para una
etiqueta y lo que un módulo dice no es una etiqueta — el mismo razonamiento que
la auditoría ya había escrito para los permisos, sin aplicar donde valía igual.

Y la prueba que debía atrapar el primero afirmaba una **ausencia**, que se
satisface borrándolo todo. Pasó en verde la misma corrida en que la etapa 6 se
quedó ciega.

### 2026-08-04 (2) — la máquina corrió los seis pasos y falló en el quinto
La etapa 16 —arrancar la imagen y teclearle los seis pasos— corrió por primera
vez contra una imagen real. Sirvió de inmediato y encontró dos defectos, uno
detrás del otro, en la misma línea.

**El primero:** `is_available()` preguntaba por `bpftool`, que adentro de la
imagen no existe, y esa respuesta decide entre confinar un módulo y negarse a
arrancarlo. `KernelStore` lo reemplaza con `bpf(2)` directo y `BpftoolStore` se
borró. Cuarta vez que algo le pregunta a `bpftool` por algo que `bpftool` no
hizo.

**El segundo:** el prompt pedía un perfil de sandbox llamado `default`, que no
existe. Lo escondió el orden —el nombre se resolvía después de comprobar el
mapa de política, así que solo la imagen llegaba a mirarlo— y lo dejó pasar que
la etapa 15 maneja el prompt de verdad y **no tecleaba `correr`**: el único
verbo sin ejercitar era el único roto.

**El tercero:** la raíz de cgroups no entregaba `memory` ni `pids`, porque en
todas las demás máquinas eso lo hace systemd antes de que corra nada, y en la
imagen no hay systemd. PID 1 lo hace ahora, y la sesión reporta un cgroup2 que
está montado y no delega nada como lo que es: ausente.

**El cuarto:** el punto de montaje de un permiso sobre un archivo se creaba
como directorio en la ruta remapeada, y el kernel exige que sean del mismo tipo.
Todos los permisos de todas las pruebas son directorios; el único permiso sobre
un archivo suelto es el del `greeter`.

**El quinto:** nadie había hecho el `switch_root`. La raíz de la imagen es la
raíz de su namespace de montajes, y esa no tiene padre, así que `pivot_root`
niega todo módulo. En cualquier otro Linux el initramfs ya se bajó de sí mismo
antes de que arranque nada.

Los cinco son la misma forma vista desde cinco lados: **una comprobación que
depende de una condición solo se hace en las máquinas que la cumplen**, y la
imagen es la única máquina que no cumple ninguna. Seis reglas nuevas en
[[Estrategia-de-Pruebas]], incluida la del mensaje de falla que nombraba una
causa que nadie midió y mandaba a buscar al lado equivocado.

### 2026-08-04 — los seis pasos existen
El objetivo pasó a ser cerrar la Fase 1, y quedaban dos pasos del criterio de
salida sin nada detrás. Los dos tenían la misma forma: **la pieza estaba escrita
y no había cómo alcanzarla.**

**El 6.** La memoria persistente es la tercera primitiva y está probada en
hardware desde el 2026-08-02, y la sesión no escribía en ella. Ahora `instalar`
y `revertir` escriben por el mismo `recollection.rs` del agente —no una copia— y
`recuerdos` lo lee. Lo que lo vuelve una prueba y no una demostración: después
de `revertir`, la instalación sale como *no confirmable* **sola**, porque quedó
atestiguada contra el enlace `current` que el rollback quitó.

Antes hubo que decidir qué cuenta como el paso 6, porque la bóveda decía dos
cosas distintas. Lo decidió Cesar: la memoria sobreviviendo al reinicio; el
modelo real deja de bloquear la fase sin cancelarse.

**El 1.** `make -C image doctor`. Lo que detiene a la persona ajena nunca es
Thalyx: es un paquete que falta, encontrado de uno en uno y cada uno después de
que lo anterior salió bien. Ahora salen todos juntos, con la línea de `apt` que
los instala, antes de descargar o compilar nada. El peor era `pahole`, cuya
ausencia hace que Kconfig descarte `DEBUG_INFO_BTF` **en silencio** y la culpa
caiga sobre el cargador de BPF varios pasos después.

Y el `doctor` se comprueba a sí mismo: sin `gcc` no puede probar las cabeceras,
y lo dice en vez de callarlo. Regla 3 aplicada al comprobador.

**Un defecto propio, y dio regla nueva.** El párrafo que explica un hecho no
confirmable decía que algo había cambiado *"without going through Thalyx"*.
Cierto mientras la única ruta fuera una edición por fuera; con `revertir` pasó a
ser una explicación segura de una causa que ese código no puede ver. Ninguna
prueba se rompió. Ver [[Estrategia-de-Pruebas]].

También se corrigió el README, que seguía diciendo *"Phase 1 — Thalyx core on an
Alpine base"* — un decreto derogado el 2026-08-03 que sobrevivió en una de las
cuatro puertas de entrada. Es la regla de que una afirmación de ausencia caduca
sola, en su versión más incómoda: caducan también las de presencia cuando nadie
las vuelve a leer.

### 2026-08-03 (12) — dos fallos en hardware, y ninguno era de Thalyx en el sentido esperado
La corrida en la máquina de Cesar dio `proven 59 · failed 2`. Los dos se
arreglaron y los dos enseñaron algo.

**El primero era del arnés.** `verify.sh` activaba
`THALYX_REQUIRE_BTRFS_TESTS` porque había btrfs-progs, y nunca ponía
`THALYX_BTRFS_SCRATCH`, que es lo que ese test necesita para crear un
subvolumen. Exigió una comprobación y le negó su entrada. El error de fondo:
**tener la herramienta y tener dónde usarla son dos hechos**, y en Fedora se
separan de inmediato porque `/tmp` es tmpfs. Ahora se establecen los dos, y el
segundo creando un subvolumen de verdad — `stat -f` dice btrfs también para un
montaje de solo lectura. Séptima vez que el culpable es el instrumento.

**El segundo era real y estaba en el `allowlist` de seccomp.** El módulo moría
con `SIGSYS` en su primera respuesta. La causa la dio `strace` en tres minutos y
no la habría dado leer el código: **un `UnixStream` de Rust lee con `recv(2)` y
escribe con `send(2)`**, no con `read` y `write`. `recvfrom` y `sendto` no
estaban en la lista.

Lo que lo explica es más interesante que el arreglo: el `allowlist` se derivó
empíricamente corriendo módulos reales, que es el método correcto — pero **todos
esos módulos eran scripts de shell, y `/bin/sh` no toca un socket**. El método
cubre exactamente los programas que se usaron para derivarlo. De ahí la regla
nueva de [[Estrategia-de-Pruebas]]: **un sustituto que nunca ejerció el
mecanismo no lo probó.**

`recvfrom` y `sendto` entran; `socket`, `connect` y `bind` siguen fuera. Un
módulo puede **usar** el socket que le dieron y no puede **fabricarse** otro, y
la prueba afirma las dos mitades juntas a propósito: separadas, cada una pasaría
sola y una sola no sirve.

### 2026-08-03 (11) — hay un módulo, y habla
`dev.thalyx.greeter` existe: el primer módulo desde que se borró el que era un
script de shell. Se instala desde un bundle firmado, corre, y **habla con
Thalyx por un socket que nunca abrió**. Lo que sale por pantalla:

```
  dev.thalyx.greeter said:
    I am dev.thalyx.greeter 1.0.0, speaking protocol 1, holding 1 grant(s).
    read 27 byte(s) from .../notes.txt: the vault is the authority
    I asked for /etc/shadow and was refused, which is correct.
```

Las tres líneas dicen cosas distintas. La primera: **un módulo no sabe quién
es**, pregunta, y lo que le contestan sale del manifiesto firmado. La segunda:
la línea base. La tercera: la denegación — sin la segunda no probaría nada,
porque un Thalyx que negara todo se vería igual.

Y una cuarta que no sale por pantalla: **ejecutado a mano no arranca**. No
porque compruebe una licencia, sino porque en el descriptor 3 no hay nadie.
Eso es [[Filosofia-Fundacional]] vuelta comprobación.

Lo construido: `thalyx-syscall` coloca el descriptor (`place_on`,
`spawn_with_channel`, `inherited_channel`), `launch.rs` lo lleva por las dos
etapas del sandbox, y `thalyx-core/api.rs` es el servidor.

**El hallazgo que más importa está en `api.rs`, y es de seguridad.** El
servidor **no está dentro del sandbox**: corre como Thalyx, con el alcance de
Thalyx. Un módulo que pide una ruta le está pidiendo a *Thalyx* que la abra, así
que la raíz vacía del sandbox y el LSM no protegen nada ahí. Cada ruta se
comprueba dos veces: por el nombre, y por **lo que el kernel resuelve** — que es
lo único que atrapa un symlink plantado dentro de un directorio que el módulo
puede escribir. Esa era la vía que sí habría funcionado.

Etapa 12 en `verify.sh`, con su control. Y **una guarda mía salió mal primero**:
se disparaba con "cgroup2 montado" cuando la condición real es "el LSM está
cargado", así que exigió a este contenedor algo que no puede hacer y reportó
roto a Thalyx. Es la regla 3 otra vez: un salto que se dispara solo se ve
idéntico a un fallo real.

Falta la ruta confinada —el canal por dos `exec` y un filtro seccomp— que solo
se puede comprobar en máquina con LSM.

### 2026-08-03 (10) — la API interna deja de ser una línea de una nota
Decretada en [[API-Interna-de-Modulos]] y construida en `crates/thalyx-abi`:
**un socket que Thalyx entrega ya abierto en el descriptor 3** al ejecutar el
módulo —sin ruta que equivocar, sobrevive a la raíz vacía del sandbox, y su
ausencia es lo que impide que un módulo corra fuera de Thalyx—, mensajes de
longitud explícita más CBOR, y tres familias: archivos, notificar, y preguntar
quién es. **27 pruebas**, incluidas las dos mitades de la conversación
hablando por un socket real entre dos hilos.

Tres decisiones que valen más que el código:

- **Denegado y fallido son respuestas distintas.** "No puedes leer esto" y
  "esto no se pudo leer" son hechos diferentes sobre el mundo, y un módulo que
  solo supiera que falló reportaría un disco ausente como un problema de
  permisos. Es la regla 10 de `CLAUDE.md` puesta en el protocolo.
- **Un campo desconocido se rechaza, no se ignora.** Es la dirección incómoda
  —rompe con un módulo más nuevo— y la correcta: ignorarlo dejaría al que envía
  creyendo que restringió la operación y al que recibe sin haber visto la
  restricción, en un canal que gobierna permisos.
- **Un marco ilegible cierra la conexión; un mensaje ilegible se contesta.**
  Después de una longitud mala no hay dónde empezar a leer otra vez; después de
  un mensaje malo, sí.

**Y una tercera contradicción del mismo tipo que las anteriores.**
[[Core-Nucleo]] listaba *"ejecutar comandos"* entre las capacidades de esta API.
No hay comandos que ejecutar. Como el login en tty1 y como `bpftool`: una
capacidad que se apoyaba en la base y envejeció callada cuando la base se cayó.
Queda anulada por decreto, no implementada.

Falta lo que la vuelve real: pasar el descriptor por las dos etapas del
lanzamiento, el servidor contra los permisos verdaderos, y un módulo escrito
contra ella. Eso último es lo que el decreto pone como prueba de que sirve.

### 2026-08-03 (9) — existe la máquina
`make -C image run` arrancó. Kernel 6.12.101 construido desde `allnoconfig`,
initramfs con **un solo archivo**, `thalyx` como PID 1. Montó los siete
filesystems, arrancó la sesión, y la sesión imprimió el párrafo que dice que no
hay shell detrás — que solo imprime cuando su padre es el pid 1, así que la
frase no está cableada: es una comprobación.

Y se describió con tres `no` que no oculta: sin Btrfs, sin enforcement, sin
módulos. Los tres eran conocidos y están arriba con su orden de resolución.

**Lo que esto cierra**: el paso 1 del [[Criterio-de-Salida-Fase-1]] tiene por
fin una máquina detrás. No cierra el criterio —ese exige que lo haga alguien de
fuera, sin ayuda— pero hasta hoy no había nada que esa persona pudiera arrancar.

**Un hallazgo del arranque**: `attach_lsm` en `init.rs` busca
`/lib/thalyx/thalyx_lsm.bpf.o`. Ese archivo **no puede existir**: sería un
segundo archivo en una imagen que el decreto obliga a tener uno. El mensaje
"is not in the image" es cierto y su arreglo obvio es el equivocado. El objeto
BPF va incrustado en el binario.

### 2026-08-03 (8) — el kernel no compilaba, y la configuración se perdía sola
El primer `make -C image kernel` en la máquina de Cesar falló entero en
`arch/x86/boot/compressed/`: GCC 15 (Fedora 43) usa C23 por defecto, donde
`bool`, `true` y `false` son palabras reservadas, y ese directorio era el único
del kernel que nunca pasaba `-std=`. **No se puede arreglar desde fuera** —su
Makefile abre con `KBUILD_CFLAGS :=`, que tira lo que venga de arriba, así que
`KCFLAGS` jamás llega. Río arriba lo arreglaron en enero de 2025 y aterrizó en
la serie estable en **6.12.14**, comprobado tag por tag. `KVERSION` pasa a
**6.12.101**, la cabeza de la línea 6.12 LTS.

**Y al reproducir la configuración a mano apareció algo peor.** `olddefconfig`
descarta en silencio toda opción cuyas dependencias no se cumplan: **nueve de
las de `thalyx.config` no llegaban al `.config` final**, entre ellas
`CONFIG_BPF_LSM` y `CONFIG_DEBUG_INFO_BTF`. La máquina habría arrancado
perfecta y `thalyx-lsm` no se habría podido enganchar nunca, con un síntoma
idéntico al hueco de `bpftool` que ya conocíamos — la culpa habría caído sobre
el cargador, que no tenía nada que ver. También faltaban `VIRTIO_MENU` y
`BLK_DEV`, sin los cuales no hay disco del store, e `IPC_NS`.

`make -C image kernel` ahora compara lo pedido contra lo que salió y **se niega
a compilar** si falta una línea. Probado con su control: quitando `BPF_LSM` y
`BTF` a mano, los nombra y sale con error. De ahí la regla nueva de
[[Estrategia-de-Pruebas]]: **pedirle algo a una herramienta no es haberlo
obtenido**.

Con las nueve líneas puestas, 6.12.101 configura y compila limpio en el
contenedor, y el `vmlinux` trae `.BTF`. Eso comprueba la configuración, **no**
el problema de GCC 15: aquí hay GCC 13. QEMU sigue sin correr nunca.

### 2026-08-03 (7) — el decreto fundacional, y todo listo para arrancar
Cesar escribió el texto que funda el proyecto y quedó **literal** como primera
sección de [[Filosofia-Fundacional]], con la regla de que cualquier decreto que
lo contradiga está equivocado. Está enlazado desde `CLAUDE.md`, el índice y el
README, que son las cuatro puertas de entrada.

Se registraron los dos decretos que su propio texto invalida: `bpftool` (que ya
no puede estar en la imagen) y `llama.cpp` como proceso (que sería un segundo
programa — probablemente el modelo del agente sea **un módulo**, pero eso lo
decide Cesar).

`rusqlite` pasa a `bundled`: SQLite se compila dentro del binario. No es
preferencia, es necesidad — no hay libsqlite3 en el disco de la imagen contra el
que enlazar, y era el primer bloqueador del binario estático.

[[Primer-Arranque]] tiene el procedimiento completo.

### 2026-08-03 (6) — hay máquina: PID 1, la imagen, y el kernel
`thalyx` es PID 1 (`init.rs`): monta siete filesystems diciendo por qué cada
uno, arranca la sesión, y cosecha huérfanos para siempre. Si un montaje falla no
aborta — la máquina arranca describiéndose a sí misma, porque un sistema que se
niega a arrancar no te dice *por qué* desde una pantalla a la que no llegas.

**Thalyx construye su propia imagen** (`image.rs`): un cpio `newc` escrito aquí,
sin `cpio` ni herramientas ajenas. Un initramfs, no un ISO — sin gestor de
arranque, sin tabla de particiones, sin una tercera cosa donde algo se esconda.
**Un solo archivo dentro**, `/init`, porque si el decreto dice un programa,
un archivo es lo que lo vuelve cierto en vez de casi cierto.

Y se cuenta: `make -C image count` parsea el archivo y dice cuántos programas
hay. Si no dice uno, el decreto está roto y el número lo dice antes de que nadie
discuta.

`image/` lleva el Makefile y `thalyx.config`, un kernel desde `allnoconfig`.
**Jamás ejecutados**: aquí no hay red a kernel.org ni QEMU.

El hueco grande queda dicho: **`thalyx-lsm` no se carga en el arranque**. El
cargador invocaba `bpftool`, y no hay bpftool en la imagen ni shell para
llamarlo. La máquina arranca y lo dice.

### 2026-08-03 (5) — se cae la distro, y con ella lo que se apoyaba en ella
Cesar preguntó por qué habría un login al arrancar si nadie lo construyó. La
respuesta —lo pone la base— hizo visible que había una base, y que la bóveda se
contradecía en cuatro notas. Decreto: **cualquier distribución queda fuera para
siempre**; el kernel de Linux nunca estuvo en discusión.

Borrados por falsos: el esqueleto del ISO escrito esa misma noche, que producía
una distro de Alpine con el getty quitado, y el módulo `dev.thalyx.hola`, que
era un script de shell y por lo tanto corría en cualquier Linux.

Reescritos: [[Construccion-del-ISO]] entero, y las secciones de
[[Core-Nucleo]] y [[Fases-de-Implementacion]] que decretaban la base.

### 2026-08-03 (4) — el enunciado llega hasta el disco, y un fallo que solo salió corriéndolo
**El paso 6, ahora con sus dos mitades.** El agente escribe lo que hizo **y lo
lee**: `thalyx agent recall <tarea>`, y `--task` trae el contexto solo. Lo que
recuerda entra como estado de Thalyx y puede tener efecto, salvo lo que ya no
puede confirmar, que se muestra y no se usa. Falta que retome una conversación
de varios turnos, que necesita un modelo.

Lo que quedó: `thalyx agent do --task <t>`
escribe en la memoria persistente qué se pidió y qué se instaló, y
`thalyx memory recall <t>` lo lee desde otro proceso. Los dos hechos son de
clase distinta a propósito: lo que el humano dijo **no atestigua nada** —ningún
archivo puede volver falso que lo haya dicho— y lo instalado atestigua el enlace
`current`, así que quitar el módulo deja el recuerdo *no afirmable* y lo dice,
en vez de seguir reportando una instalación que ya no está.
`thalyx agent plan` y `thalyx agent do`, más el repositorio local y la
resolución de versiones (`thalyx-core/repo.rs`): **máxima versión que satisface
el constraint y cuya firma valida**, como manda [[Resolucion-de-Versiones]]. La
cadena entera funciona contra bundles firmados de verdad — enunciado, contrato,
resolución, camino confiable, commit atómico, journal, y el módulo instalado
corre.

**El fallo del día**, y es el más instructivo que ha dado el proyecto: la
atribución tomaba el canal *menos* confiable cuando un valor aparecía en dos.
Eso volvía imposible de instalar por nombre cualquier módulo mencionado en
cualquier página leída. Pasó 39 pruebas y tres mutantes deliberados. Murió a los
tres segundos de existir el comando, tecleando una frase. De ahí la regla nueva
de [[Estrategia-de-Pruebas]]: **un mutante demuestra que una prueba es portante,
no que la decisión que codifica sea la correcta.**

También quedó `thalyx dev agent-probe`, que existe por la regla 4: sin modelo,
toda inyección se rechaza con "no model is configured", y esa denegación se ve
idéntica a la de la procedencia sin probar nada de ella.

Antes de eso, `bundle.rs`: un `.thmod` de 768 MB **sin firma** llevaba el
proceso a 1 GB de RSS porque cada miembro se leía entero antes de decidir si
importaba. Ahora hay tamaños por miembro, los desconocidos no se leen, y el
artefacto no puede expandirse más de 50× lo comprimido.

### 2026-08-03 (3) — el agente mínimo, contra un modelo que miente a propósito
Se decretó [[Gamas-de-Modelo]] —cuatro gamas de una familia, `llama.cpp` como
proceso, gramática restringida, y **el modelo nunca escribe la procedencia**— y
se construyó `crates/thalyx-agent` hasta donde este contenedor puede
comprobarlo: router, atribución, ensamblado y un falso hostil con nueve formas
de portarse mal. 39 pruebas.

Al construirlo aparecieron dos cosas que el decreto no anticipaba, ya escritas
como revisión en [[Agente-Minimo]]: atribuir un valor por **dónde aparece**
también detecta las alucinaciones, y una *operación* no se puede atribuir
buscándola, así que se atribuye por lo que la conclusión pudo leer — de donde
sale que **en cuanto hay texto ajeno en el transcript, el modelo ya no puede
originar una acción**, y el humano sí, tecleándola.

Y una regla nueva de [[Estrategia-de-Pruebas]], encontrada rompiendo cada
mecanismo a propósito para ver qué pruebas lo notaban: **dos defensas que se
solapan hacen que la prueba grande no pruebe ninguna**. La prueba de las nueve
malas conductas no falló con ninguno de los tres mutantes.

### 2026-08-03 (2) — una revisión externa encontró que la bóveda se contradecía
Una lectura externa del repo —solo código y documentación, sin el contexto de
la filosofía— encontró que `Estado-de-Implementacion` afirmaba a la vez que
`restore` estaba construido y que **no existe**, y que los límites de recursos
seguían sin probarse cuando `verify.sh` ya tenía la etapa. Al corregirlo
aparecieron tres más: dos listas incompatibles de "las cuatro primitivas"
(contando [[Parser-Mecanico]], que su propio decreto llama *componente*), un
comentario en `thalyx-sandbox/src/lib.rs` que decía que un módulo corre con el
uid de Thalyx cuando `uids.rs` lleva días dándole uno propio, y "tres
variables" de salto donde hay cuatro.

Las cinco tienen la misma forma y de ahí sale la regla nueva de
[[Estrategia-de-Pruebas]]: **una afirmación de que algo falta no la rompe
nada**. El código rompe las afirmaciones de que algo funciona; las de ausencia
envejecen calladas.

También quedó anotado el hueco simétrico: `verify.sh` activa tres de sus cuatro
variables `THALYX_REQUIRE_*`, no la de Btrfs.

De la misma revisión se descartaron dos cosas: la supuesta inconsistencia de
fechas (2 de agosto 22:13 en CDMX **son** las 04:13 UTC del 3; la bóveda fecha
en UTC) y el reproche de que Thalyx "todavía no es un sistema operativo", que
es [[Decision-Capa-vs-SO-Nuevo|un decreto deliberado]] y no un hallazgo.

### 2026-08-03 — todo verde en hardware, y las dos operaciones del decreto
Se cerró el ciclo del contador de mutaciones (10 hooks, por CPU, acotado al
árbol), se abrió la puerta del atajo (`graph trust`), y se construyeron las dos
operaciones de [[Rollback-vs-Restore]]: `rollback` y `restore`, con snapshots
de Btrfs debajo. Cuatro defectos encontrados y arreglados, **tres de ellos del
arnés y no de Thalyx** — de ahí las reglas 5 y 6 de `CLAUDE.md`.

### 2026-08-02 — la tercera primitiva y el enforcement real
Memoria persistente, montajes idmapped, un uid por módulo, `pivot_root`, perfil
`module_standard`, y la primera demostración de que el LSM deniega de verdad en
hardware.

### 2026-08-01 — los decretos
43 → 61 notas. Modelo de amenaza, formato del manifiesto, commit atómico,
sandbox, permisos JIT, estrategia de pruebas, criterio de salida de la Fase 1.

## Relacionado
- [[Estado-de-Implementacion]] — qué está construido, por crate
- [[Tareas-Pendientes]] — qué está decidido y qué no
- [[Criterio-de-Salida-Fase-1]] — cuándo se puede decir que la fase terminó
- [[00-Indice/Indice-Principal|Índice principal]]
