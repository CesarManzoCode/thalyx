---
tipo: notas-tecnicas
estado: activo
fecha-decreto: 2026-08-01
tags: [implementacion, estado, fase-1]
---

# Estado de implementación

Qué está construido de lo que está decretado. Esta nota se actualiza con cada avance de código; es lo primero que hay que leer al retomar el proyecto después de tiempo.

**No confundir con [[Tareas-Pendientes]]**, que lista decisiones sin cerrar. Aquí se lista código.

> Esta nota dice **qué está construido**. Para saber **dónde quedó el proyecto,
> qué fue lo último verificado en hardware y cuál es el siguiente paso**, ver
> [[Punto-Actual]].

## Construido

| Pieza | Dónde | Estado |
|---|---|---|
| Manifiesto `.thmod`: parseo y validación | `crates/thalyx-manifest` | Completo para el schema v1 |
| Firma ed25519 sobre forma canónica | `crates/thalyx-manifest` | Completo |
| Journal append-only con fsync | `crates/thalyx-journal` | Completo |
| Registro de intención y reconciliación | `crates/thalyx-core/reconcile.rs` | Completo |
| Lectura y desempaquetado seguro de bundles | `crates/thalyx-core/bundle.rs` | Completo, con límites de tamaño **antes** de la firma y tope de expansión al desempaquetar |
| Verificación de artefacto | `crates/thalyx-core/install.rs` | Completo |
| Commit atómico | `crates/thalyx-core/commit.rs` | Completo |
| Anclaje de clave de publicador (TOFU) | `crates/thalyx-core/keystore.rs` | Completo |
| Registro de permisos con vigencia condicionada | `crates/thalyx-core/permissions.rs` | Completo |
| Camino confiable | `crates/thalyx-core/trusted_path.rs` | Completo para autorización de capacidades |
| Puntos de inyección de fallos | `crates/thalyx-core/fault.rs` | Cuatro puntos sobre la ruta de instalación |
| Contrato con marcado de origen | `crates/thalyx-contract` | Schema v1, procedencia por campo, contención |
| `thalyx-permd` (política → mapa BPF) | `crates/thalyx-permd` | Traducción, codificación y escritura |
| Manifiesto guardado junto al módulo | `crates/thalyx-core/install.rs` | Verbatim, con su firma, re-verificado en cada lectura |
| Identidad cgroup v2 del módulo | `crates/thalyx-sandbox/cgroup.rs` | Probado contra un montaje real |
| Lanzamiento confinado (re-exec) | `crates/thalyx-sandbox/launch.rs` | Probado: el programa reporta su propio cgroup |
| Orquestación de ejecución | `crates/thalyx-core/run.rs` | `thalyx module run`, ciclo completo |
| Perfil `module_standard` | `crates/thalyx-sandbox/profile.rs` | Namespaces, seccomp y límites; falta user namespace |
| Filtro seccomp (BPF clásico) | `crates/thalyx-sandbox/seccomp.rs` | Lista de permitidos derivada empíricamente. Desde el 2026-08-24 hay **guardias por argumento**: `sched_setscheduler` pasa con política ordinaria y muere con tiempo real. Los saltos se resuelven simbólicamente y una distancia que no cabe en ocho bits se rechaza al compilar. Probado corriendo `chrt` bajo el filtro de verdad, no sólo evaluando el programa compilado. `sched_setattr` —la segunda puerta a la misma capacidad— se deniega de plano: su política vive detrás de un puntero y seccomp no puede leerla. El perfil `semantic_provider` añade seis llamadas sobre `module_standard` y ninguna llega a `module_standard`; la sexta, `fork`, se midió el **2026-09-05** con `strace` alrededor de un proceso que llevaba el filtro real y el rust-analyzer musl de la imagen, porque glibc escribe `fork()` encima de `clone(2)` y ninguna prueba compilada aquí lo había emitido nunca. Ver [[Semantica-Compilada]] |
| Escritura de un Btrfs vacío | `crates/thalyx-btrfs` | Ocho árboles, tres chunks y los superbloques, sin `mkfs.btrfs`; los tres subvolúmenes por ioctl |
| Tabla de particiones GPT | `crates/thalyx-install/gpt.rs` | Una tabla de dos particiones con sus dos copias, sin `sgdisk` |
| Sistema de archivos FAT32 | `crates/thalyx-install/fat.rs` | Un volumen con un archivo en `\EFI\BOOT\BOOTX64.EFI`, sin `mkfs.vfat`. No lee |
| Cómo se parte un renglón | `thalyx-cli/words.rs` | Citado con las reglas de POSIX: `'…'`, `"…"`, `\x`, y una comilla sin cerrar que se niega con su propia palabra. **No hay lenguaje**: ni tuberías, ni redirección, ni variables. La expansión se queda en el verbo, así que `rm "*.log"` es un nombre y `encontrar "*.rs"` sigue siendo un patrón. Una palabra recuerda qué caracteres venían citados, carácter por carácter. Etapa 33. Ver [[Palabras]] |
| La pantalla | `crates/thalyx-screen` | **Pura: estado adentro, pixeles afuera.** Lienzo, cuatro tipos de letra compilados adentro (OFL), acomodo de las regiones, la conversación desde abajo, el prompt, y la confirmación que **toma la pantalla entera**. El empaquetado de pixel del display se convierte aquí y se niega si no se entiende, en vez de aproximarse. 45 pruebas, ninguna necesita un display. Una respuesta más alta que el display **se dibuja**: la conversación se aplana a renglones y se ancla abajo, con AvPág/RePág para lo anterior — antes se saltaba el turno entero y `describe` dibujaba nada. Ver [[La-Pantalla]] |
| La pantalla como cara del arranque | `thalyx-cli/src/session.rs` y `screen.rs` | **Es lo que se ve al arrancar.** `session::run` entra a la pantalla antes de imprimir un prompt, si esta sesión es la de la máquina y la línea de comandos del kernel no dice `thalyx.pantalla=no`; si el display no se puede dibujar, la sesión de texto sigue y dice por qué. Los verbos corren ahí: `session::dispatch` es uno solo para las dos caras, y lo que imprimen se atrapa en el descriptor con `thalyx-capture` — que también atrapa lo que imprime un módulo, porque `correr` y `ejecutar` arrancan otros procesos. **Nadie lo ha corrido sobre hierro.** Ver [[La-Pantalla]] |
| Atrapar lo que un verbo imprime | `crates/thalyx-capture` | Mueve los descriptores 0, 1 y 2 mientras corre un verbo: la salida a un archivo en memoria, la entrada a `/dev/null`. Lo segundo es lo que evita que un verbo que pregunta cuelgue la máquina con una foto encima — sin terminal, cada confirmación toma el camino de rechazo que ya tenía. Crate propio porque los descriptores son del proceso: adentro de `thalyx-cli` la prueba medía a las otras ciento treinta y cuatro |
| El display | `thalyx-syscall` y `thalyx screen` | El `ioctl` que pregunta la geometría, el `mmap`, y `GraphicsMode`, que quita la consola de texto y la devuelve en `Drop`. `thalyx screen --describe` recorre todo el camino **sin tocar la consola**, para que una máquina donde no se pueda dibujar lo diga con la pantalla intacta. **Nadie lo ha corrido sobre hierro**, y la corrida del 2026-08-28 tampoco pudo: la máquina que verifica **no tiene `/dev/fb0`**, así que esa mitad de la etapa 40 dice `NOT PROVEN` y sólo arrancar la imagen puede contestarla |
| Un cuadro como imagen | `thalyx-screen/png.rs` y `thalyx dev screen` | El mismo camino de composición que usa el display, escrito a un PNG. Es cómo se mira la pantalla en una máquina que no tiene ninguna |
| El instalador | `crates/thalyx-install` y `thalyx install` | Particiona, escribe el arranque y hace el store en un acto. Las particiones se reciben **ya abiertas** y se sostienen abiertas hasta el final: cerrar el disco entero hace que el kernel lo reexamine por su cuenta, y ese segundo barrido borra y rehace cada partición — abrir un nodo dentro de esa ventana da `ENXIO`. **El arranque del disco instalado no lo ha ejercido nadie** |
| Controladores de una PC de verdad | `image/thalyx.config` | Framebuffer, teclado USB y PS/2, NVMe y AHCI, y `console=tty0`. **Ninguna de esas opciones se ha compilado ni arrancado** |
| Lectura de FAT32 y hallazgo del medio | `crates/thalyx-install/medium.rs` | Saca el kernel del medio del que se arrancó, sin montar nada. El medio es el volumen FAT32 etiquetado `THALYX`: la ruta `\EFI\BOOT\BOOTX64.EFI` sola la tiene también la ESP de cualquier PC |
| El store por etiqueta | `crates/thalyx-cli/store_disk.rs` | Lo que decretaba el 2026-08-06 y no existía; con `thalyx disk find` para preguntarlo sin ser PID 1 |
| Instalar desde adentro | `discos` y `instalar-en <disco>` | Los dos verbos que vuelven alcanzable el criterio sin shell |
| Raíz propia del módulo (`pivot_root`) | `crates/thalyx-sandbox/rootfs.rs` | Solo el módulo, el sistema en RO y lo concedido |
| Disciplina de cobertura del índice | `crates/thalyx-graph/watch.rs` | Completa y probada; el atajo se gana y se devuelve solo |
| Contador de mutaciones del kernel | `crates/thalyx-watch` | Diez hooks, acotado al árbol; 5000 escrituras dentro, 0 fuera |
| Memoria persistente | `crates/thalyx-memory` | Dos capas, fechado por rutas, base vectorial propia |
| `rollback` | `crates/thalyx-core/rollback.rs` | Deshace un commit; se niega cuando la entrada ya no describe el disco |
| Snapshots de Btrfs | `crates/thalyx-snapshot` | Tomar, listar, olvidar y **restaurar**; intercambio atómico; backend nativo por ioctl, sin el binario `btrfs` |
| `restore` | `crates/thalyx-core/restore.rs` | Diff de lo que se pierde, camino confiable, intención antes de mover |
| Límites de cgroup | `crates/thalyx-sandbox/limits.rs` | `memory.max`, `pids.max`, `cpu.max` |
| Syscalls crudas | `crates/thalyx-syscall` | **El único crate con `unsafe` del workspace** |
| Un uid por módulo | `crates/thalyx-core/uids.rs` | Asignado al instalar, retirado al quitar, nunca reciclado |
| Montajes idmapped para lo concedido | `crates/thalyx-sandbox/idmap.rs` | Verificado: escritura concedida funciona y aterriza a nombre del dueño |
| Motor de edición de texto | `crates/thalyx-edit` | Direcciones por renglón, **sustitución de una cadena exacta en varios archivos en una llamada** y **varias sustituciones distintas en una sola llamada** (`sustituir-lote`, en el orden dado, cada una sobre lo que dejó la anterior, con la composición ambigua —`A -> B` y luego `B -> C`— rechazada por nombre), guardado atómico, deshacer acotado. Conserva finales de renglón, salto final y permisos; un enlace se edita como el archivo al que apunta. La sustitución precomprueba todos los archivos antes de escribir uno —y el lote los abre una vez por inodo y aplica todo en memoria antes de guardar nada, así que un patrón que no encuentra su texto no deja escrito ninguno de los otros—, rechaza un salto de línea en cualquiera de los dos textos —así nunca cambia cuántos renglones tiene un archivo— y dice `wrote: false` en cada rechazo |
| Procesos, memoria y detener uno | `crates/thalyx-proc`, `thalyx-cli/src/proc.rs` | `procesos [patrón]`, `memoria` y `matar <numero> [forzar]` sobre `/proc`. La señal va por un **pidfd**, así que no puede caer en un número reciclado; primero el descriptor, después la descripción, después la señal. `TERM` por omisión y `KILL` sólo con `forzar`, comprobado con una shell que ignora `TERM` de línea base. Se niegan PID 1, la propia sesión, el `0` y los negativos, y también los dos sujetos que aceptan la señal y la tiran —un hilo del kernel y un proceso que ya terminó—, porque contestar que se detuvieron es peor que un error. `memoria` mantiene `libre` y `disponible` separados. Etapas 31 y 32. Ver [[Procesos]] |
| Búsqueda por nombre y por contenido | `crates/thalyx-files/src/search.rs`, `thalyx-cli/src/search.rs` | `encontrar <patrón>` camina el árbol y compara contra el **nombre**, `contenido <texto>` lee y compara **literalmente**. Techo de 20 000 archivos revisado al caminar, binarios y archivos de más de 4 MiB contados aparte de lo ilegible, renglones largos cortados y avisados, y las banderas sólo adelante para que el sujeto sea el resto del renglón sin inventar comillas. Etapa 30 con `find(1)` y `sed(1)` de controles. Ver [[Busqueda]] |
| Editor de pantalla | `crates/thalyx-edit/src/screen.rs` y `thalyx-cli/src/edit.rs` | Aritmética de cursor y viewport pura, dibujado en el CLI. `Ctrl-O` guarda, `Ctrl-X` sale, `Ctrl-U` deshace, `Ctrl-K` corta |
| Parser mecánico | `crates/thalyx-parser` | Rust, Python, JS/TS, C, Go. Importaciones **y declaraciones**; identificadores fuera de comentarios y cadenas. Desde el 2026-08-28 las tres entradas comparten **un solo escaneo con estado**: comentarios de bloque, comentarios al final del renglón y cadenas que siguen en el renglón siguiente. Los tres huecos que cerró se encontraron indexando este repositorio; una comilla simple nunca cruza de renglón, porque en Rust es un lifetime |
| Símbolos en el índice | `crates/thalyx-graph`, verbo `buscar` | Dónde se declara un nombre y dónde se usa. 5 495 nombres sobre `crates/` de este repo |
| Dependencias que ningún import declara | `crates/thalyx-graph`, `Via::Symbol` | Un nombre hace arista donde se use si **exactamente un** archivo lo declara, es **visible desde afuera** (la regla de cada lenguaje, no una heurística), el archivo que lo usa **no lo ata** y **no lo declara él mismo**. Cubre acceso por campo, método, trait en una cota, llamada por ruta y re-export. Las cuatro condiciones salieron de correrlo sobre este repositorio: con sólo la primera, `thalyx-snapshot` tenía 41 dependientes; con las cuatro, 19, de los que 17 son referencias reales entre crates. Cada fila dice `via: import` o `via: symbol`. Ver [[FS-en-Grafo]] |
| Corpus determinista del índice | `crates/thalyx-graph/corpus/` | Doce árboles con la respuesta correcta escrita al lado, 44 igualdades exactas, milisegundos y cero modelo. Cuatro existen para ser contestados angostamente. Los límites se declaran y `THALYX_REQUIRE_FULL_CORPUS=1` los exige. Etapa 49 |
| Respuestas acotadas con cursor | `crates/thalyx-files/window.rs` | Total, cursor por llave y continuidad; en seis verbos |
| El intento con nombre | `crates/thalyx-core/attempt.rs`, verbo `intento` | Política probada contra el falso de directorios; el Btrfs lo ejerce la etapa 26. Desde el 2026-08-29 `abandonar` se puede pedir en **una** llamada nombrando el intento (`snapshot=`) y **el estado exacto del árbol** que se autoriza destruir (`state=`): procede sólo si el nombre es el del registro y el árbol es, en el instante de la destrucción y bajo el candado, exactamente ese estado. Los dos conteos que ocupaban ese lugar durante un día se retiraron —no distinguían una escritura ajena sobre un archivo que el agente ya había editado— y se rechazan nombrando lo que los reemplazó. La decisión entera es una función pura, `consent`, con pruebas que corren sin Btrfs; desde el 2026-08-29 `consent` **no compara el estado** —nombrar el intento y un estado *es* la autorización, y si es cierta se decide bajo el candado, que es el único lugar donde esa pregunta significa algo. Lo contrario fue el defecto que encontró la etapa 55 en Fedora: el rechazo nunca llegaba a decir `workspace_moved`. La forma de dos pasos queda intacta |
| Identidad exacta del espacio de trabajo | `crates/thalyx-snapshot/src/lib.rs` (`Witness`, `w2`) | Un digest sobre cada ruta con su tamaño, su mtime, su ctime, su inodo y **lo que contiene** —los bytes de un archivo regular, el destino de un enlace, la especie de un fifo, que nunca se abre—. Los timestamps solos no bastaban: dos escrituras seguidas caben en un tic, que es por lo que las pruebas dormían veinte milisegundos, y ese es el caso real. Cuesta leer el árbol entero y lo dice: `Witness::bytes` y el campo `state_bytes`. El contador de mutaciones del kernel se inspeccionó como la vía barata y se descartó —su gancho de escritura no ve una página sucia de `mmap`—. Un árbol que no se pudo leer completo, ahora incluidos los bytes, no tiene identidad y nunca autoriza nada. Quince pruebas sobre árboles reales, sin Btrfs y sin esperas; la etapa **55** lo ejerce sobre un subvolumen. Ver [[Identidad-de-Estado]] |
| Ejecución transaccional | `crates/thalyx-cli/src/exec.rs`, verbos `hacer` y `evidencia` | Un programa de varias peticiones dentro de una frontera reversible, con validación determinista y confirmación o rollback automáticos. **Desde el 2026-08-30 `on_success` decide qué pasa con el árbol cuando todo salió bien**: `commit` de omisión, y `rollback` para un refactor exploratorio, un preview, un what-if, medir impacto o una tarea que debe terminar restaurada — el programa corre, muta, consulta `changed()`, valida y devuelve, se conserva su `returned` y toda su evidencia, y **después** se restaura la frontera; la respuesta contesta `succeeded: true` con `tree: "restored"` y `restored_by_request: true`, que nunca es el caso de falla. **Y la respuesta que le llega a un modelo bajó de 38 campos y 1 416 bytes a 8 campos y 469** sobre la misma forma sintética: ids de instantánea, testigos de estado, contadores internos y lo que el programa imprimió viven completos en la evidencia, a una llamada del handle que toda respuesta lleva. Hay una prueba sobre el tamaño en bytes. Cada paso pasa por `external::one`, la misma puerta que una petición suelta, así que componer no amplía nada. Dieciséis pruebas contra el falso de directorios; el Btrfs lo ejerce la etapa **56**. Ver [[Ejecucion-Transaccional]] |
| Comprobación de sintaxis mecánica | `crates/thalyx-parser/src/lib.rs` (`unbalanced`) | Si una edición se comió una llave, una comilla o un comentario, y en qué renglón. No es un compilador. Tiene su propio escáner y el porqué está escrito: `scrub` blanquea el resto del renglón ante un `'` suelto, que en Rust es un lifetime. La prueba es cada `.rs` de este repositorio |
| Consumidor del ringbuf de mutaciones | `crates/thalyx-watch/src/ring.rs`, verbo `cambios` | Protocolo probado sobre bytes; el mapeo lo ejerce la etapa 27 |
| El journal desde la sesión | `crates/thalyx-cli/history.rs`, verbo `historia` | El más nuevo primero, con lo que no cubre dicho en un campo |
| Índice en grafo (SQLite) | `crates/thalyx-graph` | Nodos, aristas, etiquetas, obsolescencia |
| `thalyx-lsm` (BPF LSM) | `lsm/thalyx_lsm.bpf.c` | **Demostrado denegando en hardware real** |
| `thalyx-watch` (BPF LSM) | `lsm/thalyx_watch.bpf.c` | Diez hooks, contador por CPU, atribución por ancestros |
| Entorno de desarrollo (VM) | `dev/` | Preflight, guest reproducible, verificación de enforcement |
| Agente mínimo: router, atribución, ensamblado | `crates/thalyx-agent` | Enunciado → contrato → instalación, probado de punta a punta. Un plan tiene **dos formas** desde el 2026-08-24: contrato para lo que el núcleo lleva a cabo, verbo para el resto del catálogo — y las dos pasan por `origins.validate()`, que el camino de verbo se saltaba |
| Las cuatro gamas | `crates/thalyx-agent/tier.rs` | Los tamaños son de un tipo que se imprime con `~`, para que un estimado no se lea como medición |
| Gramática GBNF de la propuesta | `crates/thalyx-agent/grammar.rs` | Generada desde la forma que tiene que respetar; las clases de caracteres se comparan contra el escáner de ids, carácter por carácter. Desde el 2026-08-24 son **tres formas de objeto** y cubre los 39 `op` del catálogo: `install` y `run` conservan la regla de id, el resto recibe una clase que cubre rutas y nombres, y `nothing` es la abstención con palabra propia |
| El prompt y el marcador | `crates/thalyx-agent/prompt.rs` | Aleatorio por invocación, así que la respuesta se localiza sin parsear el formato de `llama.cpp` |
| `llama.cpp` como proceso | `crates/thalyx-agent/llama.rs` | Invoca `llama-completion`, no `llama-cli`. Plazo, tope de salida, muestreo de `VmHWM`, y **comprobación del contrato de una pasada**. Corrido entero contra hierro real en **tres gamas** (1.5B, 3B, 7B). `grammar-check` probó que la gramática restringe en media y alta; en ligera dijo `PROVEN` **sin control** —el brazo libre no dijo nada— y ahora exige que lo diga. `Truncated` distingue presupuesto agotado de regla violada |
| La gama elegida y sus pesos | `crates/thalyx-agent/config.rs` | Mide el archivo en vez de creerle a nadie; se niega si cambió de tamaño |
| `agent model`, `agent grammar`, `agent bench` | `crates/thalyx-cli/agent_model.rs` | El banco que [[Gamas-de-Modelo]] pide: intención, argumentos, abstención, latencia y RAM. **Tres gamas medidas el 2026-08-08**; la máxima quedó `N/D` porque el proceso murió por falta de memoria antes de la primera inferencia. Un rechazo por atribución imprime **qué** id fue |
| La costura del motor | `crates/thalyx-agent/llama.rs` (`Engine`, `EngineCall`, `ProcessEngine`) | Entra un vector de argumentos, salen bytes. Arriba de la línea no cambió nada de lo que el agente sabe sobre una respuesta; abajo hay dos maneras de arrancar `llama.cpp`. `Engine::scratch_root` es la parte con filo: dice **dónde** se le puede escribir el prompt, porque un módulo sólo ve lo que le concedieron |
| El motor de inferencia, como módulo | `crates/thalyx-cli/src/engine_module.rs`, `dev/build-engine.sh`, `dev/stage-engine.sh` | **2026-08-28.** `llama-completion` estático, empaquetado en un `.thmod` firmado, instalado en el store y corrido por `thalyx_core::run` — el mismo que corre `correr` — bajo `module_standard`, con uid propio, cgroup, seccomp, raíz pivotada y los 4 GiB que pide su manifiesto. **Y desde el mismo día es residente**: el binario es `thalyx-engine` —el mismo `llama.cpp`, misma etiqueta, mismas banderas— que carga el GGUF una vez y contesta peticiones enmarcadas por una tubería, así que la segunda frase no vuelve a leer dos gigabytes de disco. Sigue sin haber demonio, ni servidor, ni red: `run::start` devuelve un `RunningModule` que posee el cgroup, la política y el proceso, y `run()` es ese mismo `start` seguido de `wait`, así que no hay un segundo lanzador. El GGUF es un dato del store en `/opt/thalyx/data/engine/models`, así que cambiar el modelo es copiar un archivo. La imagen sigue siendo el kernel y un programa. Ver [[Motor-de-Inferencia-como-Modulo]] y [[Motor-Residente]] |
| El agente, desde la pantalla | `crates/thalyx-cli/src/session.rs` (`understand`) | Una línea que no es verbo va al agente, y lo que vuelve se convierte en **una línea del vocabulario de la sesión** que el mismo dispatch corre. Así un modelo no puede alcanzar una operación que una persona no pueda, ni saltarse una confirmación, ni inventar un verbo. Se dice en voz alta antes de correrlo, y un salto: lo que el modelo produjo no vuelve a consultar al modelo |
| Memoria de tarea del agente | `crates/thalyx-agent/recollection.rs` | Escribe y **lee**; lo no confirmable se muestra y no se usa |
| Repositorio local y resolución de versiones | `crates/thalyx-core/repo.rs` | Máxima versión que satisface el constraint y cuya firma valida |
| CLI `thalyx` | `crates/thalyx-cli` | `module` (con `run`), `agent` (`plan`, `do`), `graph`, `memory`, `rollback`, `journal`, `permissions`, `enforce`, `store`, `dev` |
| Empaquetado de módulos | `crates/thalyx-cli/dev.rs` | `keygen`, `pack`, `inspect`, `agent-probe` |
| API interna: protocolo | `crates/thalyx-abi` | Marco con longitud + CBOR, las tres familias de la v1, cliente y servidor |
| API interna: el servidor de Thalyx | `crates/thalyx-core/api.rs` | Comprueba cada ruta contra los permisos del manifiesto **y contra lo que el kernel resuelve**; un symlink plantado dentro de lo concedido no sale de ahí |
| El canal por el sandbox | `crates/thalyx-syscall`, `crates/thalyx-sandbox/launch.rs` | Socket entregado en el descriptor 3; sobrevive los dos `exec`. **La ruta confinada solo se comprueba en máquina con LSM** |
| Primer módulo | `modules/dev.thalyx.greeter` | Escrito contra la API. Lee lo concedido, es rechazado en `/etc/shadow`, y **no arranca fuera de Thalyx** |
| Sesión del sistema | `crates/thalyx-cli/session.rs` | Lo que init arranca; solo dice que es la máquina cuando lo es. `ls`, `cat`, `cd`, `pwd`, `clear`, `mkdir`, `touch`, `cp`, `mv`, `rm`, `disponibles`, `instalar <id>`, `permisos`, `revertir`, `recuerdos`, `modulos`, `correr <id>`, `ejecutar <ruta>`, `estado`, `nucleo`, `discos`, `instalar-en <disco>`, `apagar` — adentro no hay shell, así que lo que no es un verbo de aquí no existe. Los nombres estándar son los que enseña el banner y los españoles siguen funcionando; **un nombre no es un programa ajeno**, todos son el mismo Rust y `make -C image count` sigue diciendo uno. **Desde el 2026-08-23 cada uno de esos verbos tiene sus dos caras**, así que el ciclo entero de un módulo —buscar, instalar, listar, correr, deshacer— se corre por la cara estructurada; el camino confiable no se debilitó, se reporta. **Cubre los pasos 2, 3, 4, 5 y 6** del [[Criterio-de-Salida-Fase-1]]: instalar y revertir escriben en la memoria persistente por el mismo `recollection.rs` del agente, y `recuerdos` lo lee después de un reinicio |
| Mirar archivos | `crates/thalyx-files` (`list`, `read`, `resolve`) | `ls`, `cat`, `cd`, `pwd` y sus nombres en español. `ls -a` y `ls -l`, en columnas medidas contra el ancho real del kernel (`TIOCGWINSZ`, 80 de respaldo, y `None` es respuesta y no fallo). Una bandera desconocida **no se ignora**: se queda como el lugar. `cat` se niega ante un binario, porque en la imagen la sesión *es* la máquina. Un enlace roto se lista **como roto**, no como ausente |
| Cambiar archivos | `crates/thalyx-files` (`make_*`, `copy`, `move_to`, `remove`, `expand`) | `mkdir`, `touch`, `cp`, `mv`, `rm` con comodines `*` y `?`. Devuelven un `Done` con **qué pasó, dónde y los bytes exactos** — nada imprime por su cuenta, que es el decreto del objetivo. Nada sobrescribe sin pedirlo; un enlace se copia y se borra como enlace; `*` no cruza `/` ni toca ocultos; `mv` cae a copiar-y-borrar ante `EXDEV`. La cara estructurada los expone desde el 2026-08-09 |
| El catálogo de verbos | `crates/thalyx-cli/src/catalogue.rs` | **A1**: `describe` contesta los 43 verbos con nombres, argumentos, banderas, el `op` de cada uno, si cambia la máquina y qué errores da. **El `op` que promete se comprueba corriendo el verbo** (etapa 22, veintiún verbos): el día que `red` nació con cara y quedó declarado sólo-prosa, el catálogo y el despacho concordaban cada uno consigo mismo y ninguna prueba unitaria podía verlo. Los nombres viven **una vez**: las completaciones se generan de aquí, y dos pruebas atan el banner y el despacho **corriendo la sesión**. Ningún otro sistema operativo puede describirse así |
| Ensayar antes de hacer | `crates/thalyx-files` (`foresee_*`), `thalyx_core::foresee_run`, `crate::edit::foresee`, `ensayo` | **D1, nueve de nueve desde el 2026-08-26.** Cada `foresee_*` es *la mitad de comprobación de la operación real*, y la real la llama — no hay camino donde el ensayo y lo ensayado discrepen. Prefijo y no modo, porque un modo se queda encendido. Los dos últimos se cerraron el mismo día y por la misma forma: `correr` es el código de la corrida parado un renglón antes de que el programa exista, y `editar` es el camino de `editar` **sin la línea que guarda**. La lista de verbos que cambian la máquina y no se pueden ensayar **está vacía**, y hay una prueba que lo afirma |
| El índice, desde la sesión | `crates/thalyx-cli/src/index.rs` | **C1**: `indexar`, `depende`, `usan`. La pregunta que ningún recorrido de carpetas contesta, por primera vez al alcance de algo que no es `thalyx graph`. Cada respuesta trae la **vigencia en el mismo objeto que las filas**. Desde el 2026-08-28 los tres verbos semánticos **reconstruyen el índice antes de contestar** cuando el árbol cabe bajo 2 000 archivos, y cuando no dicen `declined_too_large` con el tamaño y el verbo que hay que llamar; `refrescar=no` devuelve lo que el índice tenía. Nada reporta `current` por haberlo intentado |
| La cara estructurada | `crates/thalyx-files/src/machine.rs` | Lo que el decreto del objetivo pide por nombre: `structured on` y cada verbo de archivos contesta **un objeto JSON por renglón**, desde el mismo hecho que lee el impresor humano. No esconde ocultos, los tamaños son exactos, el silencio nunca es respuesta, y un renglón tecleado produce **exactamente un objeto** — `count` y `results` adentro, `ok` verdadero sólo si todos salieron. La forma se escribe a mano y no se deriva, porque una forma derivada la decide el nombre de una variante de Rust. **Desde el 2026-08-23 la contestan los cuarenta verbos**, y hay una prueba que afirma que la lista de sólo-prosa está vacía |
| La terminal como terminal | `crates/thalyx-term`, `crates/thalyx-cli/term.rs` | Flechas, borrado a media línea, historial de 500 y tab que completa verbos al principio y nombres después. El crate es **puro** y se prueba sin abrir una terminal; el modo crudo es una guarda que la devuelve al soltarse, porque una sesión que sale sin hacerlo deja la máquina inservible. **Un solo lector de `stdin`**, `term::read_answer()`: dos lectores y un búfer que guarda lo que sobra no pueden coexistir |
| PID 1 | `crates/thalyx-cli/init.rs` | Monta siete filesystems, arranca la sesión, cosecha huérfanos. **Corrido como PID 1 el 2026-08-03**: los siete montajes salieron `ok` |
| El cargador de BPF propio | `crates/thalyx-bpf`, `crates/thalyx-syscall` | ELF, BTF, forma de los mapas, CO-RE y las llamadas `bpf(2)`. El objeto va dentro del binario. **Probado entero en hardware**: carga, atacha, y ese enforcement deniega. Ver [[Cargador-BPF-Propio]] |
| Encender y apagar el enforcement | `crates/thalyx-permd/src/store.rs`, `crates/thalyx-cli/src/guard.rs` | **Hecho el 2026-08-26.** `negar` y `observar` en la sesión, `thalyx enforce mode <enforcing\|observing>` fuera de ella: cuatro bytes en `thalyx_enforcing` con `bpf(2)`, **sin `bpftool`**, que la imagen no tiene. Antes de esto una máquina Thalyx podía leer que sólo observaba y no tenía forma de dejar de hacerlo, y toda negativa cuyo remedio era «hazlo vinculante» nombraba un comando inexistente ahí. Dos verbos y no uno con argumento: un typo no puede desarmar la máquina. **`observar` pasa por el [[Camino-Confiable]] y la cara estructurada no puede pedirlo**; `negar` no pregunta, porque aprieta. La escritura se relee, porque `bpf_obj_get` sobre cualquier mapa tiene éxito. Etapa 37, medida con `bpftool` —regla 5— con línea base y control. Ver [[Programas-Ajenos]] |
| Saber si el enforcement está puesto | `crates/thalyx-bpf/src/attached.rs` | Enumera los enlaces del kernel y los sigue hasta el programa que corren, sin `bpftool`, así que **también funciona dentro de la imagen**. `thalyx enforce attached`. Reemplazó tres comprobaciones que contestaban por un pin, un directorio y una cuenta global |
| El disco del store | `crates/thalyx-cli/store_disk.rs`, `image/Makefile` | Tres subvolúmenes Btrfs; PID 1 los monta por `thalyx.store=` y **nunca los crea**. **Arrancó con el disco montado el 2026-08-03.** Lleva el módulo en un repositorio y **sin instalar**, para que instalarlo sea el paso 2 del [[Criterio-de-Salida-Fase-1]] |
| Thalyx escribe el Btrfs | `crates/thalyx-btrfs`, `thalyx disk format` | Ocho árboles, tres chunks y los superbloques, byte por byte, sin `mkfs.btrfs` ni `libbtrfs`. Metadata y system en DUP; ninguna chunk cubre un superbloque. Lo invoca **un humano** y PID 1 no lo alcanza, así que el decreto de no fabricar se conserva. **Validado con `btrfs check`, con los dos headers de uapi capturados, y montado por el kernel de Cesar el 2026-08-07** — etapa 18. Ver [[Construccion-del-ISO]] |
| El orden del confinamiento, medido | `crates/thalyx-cli/tests/isolation.rs`, `RootFs::assemble` / `pivot_into` | **2026-08-26.** Toda escritura de Thalyx ocurre antes de entrar al cgroup; después de esa línea y hasta `execve` no hay una sola apertura con `O_WRONLY` ni `O_RDWR`. El efecto —el `-EPERM` del LSM— no se puede reproducir sin LSM, así que lo que se mide es el **orden**, con `strace -f -y`: la ventana va del `write` a `cgroup.procs` al `execve`, con línea base en dos mitades (nunca lo creó / lo creó del lado malo), falla cerrada ante una llamada partida, y `NOT PROVEN` propio si el `strace` de la máquina no tiene `-y`. Se comprobó revirtiendo el arreglo. Ver [[Estrategia-de-Pruebas]] |
| Programas ajenos | `crates/thalyx-core/src/foreign.rs`, `crates/thalyx-cli/src/foreign.rs` | **G1**, construido el 2026-08-25. `ejecutar <ruta>` corre un programa que nadie firmó, con el mismo confinamiento que un módulo y **sin modo degradado** —`sin-confinar` se justifica en que alguien firmó, y aquí nadie firmó—. No recibe canal con la API, así que por este verbo no se instala, no se concede nada persistente y no se pide nada: un invitado corre, no se le da la casa. Su usuario se guarda con la llave `foreign:<ruta canónica>`, así que el mismo programa es el mismo usuario mañana. El journal lo llama `run_foreign`. **Y se niega también con el LSM cargado pero en modo observación** —corregido el 2026-08-25: `is_available()` contesta si el mapa se abre, no si una negación llega, y `make -C lsm load` aterriza observando a propósito—. Etapa 36 y diez pruebas de integración. Ver [[Programas-Ajenos]] |
| Thalyx hace los subvolúmenes | `crates/thalyx-btrfs/subvolume.rs`, `thalyx disk subvolumes` | `BTRFS_IOC_SUBVOL_CREATE` por `thalyx-syscall`, porque adentro no hay binario `btrfs`. El número del ioctl se recalcula desde el header capturado, porque un tamaño equivocado contesta `ENOTTY` en un sistema de archivos que sí soporta la llamada. Se comprueba **montando** cada uno con `-o subvol=` como hace PID 1, no mirando si hay un directorio; correrlo dos veces es seguro. **Ejercido el 2026-08-07 en la máquina de Cesar**, etapa 19, dentro de la corrida que cerró en `proven 135 · not proven 1 · failed 0`. Ver [[Construccion-del-ISO]] |
| Lo que dijo el kernel | `crates/thalyx-syscall` (`kernel_messages`) | PID 1 baja el volumen de la consola antes de la sesión; `nucleo` lee el ring buffer entero. Sin shell no hay `dmesg`, así que callar sin devolver la vista sería esconder |
| Constructor de la imagen | `crates/thalyx-cli/image.rs` | cpio `newc` escrito por Thalyx; probado, reproducible byte a byte. Lleva `/dev/console`, sin el cual el runtime de Rust aborta antes de `main` cuando el archivo va dentro del kernel |
| Arranque sin gestor de arranque | `image/thalyx.config`, `image/Makefile` | `EFI_STUB` + initramfs dentro del kernel + línea de comandos compilada. **Probado el 2026-08-06**: un firmware UEFI arrancó Thalyx entera —`switch_root`, los siete montajes, los controladores, el LSM enganchado y la sesión— desde un medio con **un archivo**. Y el 2026-08-07, desde un **disco instalado por Thalyx**, sin nada en la línea de comandos: encontró su store por la etiqueta, la sesión salió por el framebuffer y respondió al teclado PS/2. Ver [[Construccion-del-ISO]] |
| Los controladores de una PC | `image/thalyx.config` | Cuatro grupos, cada uno con su párrafo: la pantalla (`FB_EFI` y la consola encima), el teclado (USB por xHCI/EHCI y PS/2), los discos (NVMe, AHCI, la capa SCSI y `PCI_MSI`) y **el medio** (`USB_STORAGE`, agregado el 2026-08-07 al notar que faltaba). Cinco pruebas los afirman opción por opción, porque `config-check` atrapa lo que Kconfig descartó y no lo que nadie pidió. **La pantalla y el teclado PS/2 están probados en vivo; los otros dos no.** Ver [[Construccion-del-ISO]] |
| Los controladores, hasta donde alcanza una máquina | `image/Makefile` (`run-hardware`) | xHCI con teclado USB, un NVMe y un AHCI en blanco, y la imagen instalada colgada como disco USB; `NOMEDIUM=1` saca la memoria y `NOPS2=1` quita el controlador PS/2. El driver que habla con un controlador emulado es el mismo que habla con silicio real, así que responde que los cuatro grupos enlazan y producen los dispositivos bien nombrados. **Corrido el 2026-08-07**: teclado USB por xHCI, store en `/dev/nvme0n1p2`, y la máquina arrancó de ese NVMe sin medio puesto. **No es el acto 2 y no lo cierra**: falta silicio real. Ver [[Criterio-de-Salida-Fase-1]] |
| El header del kernel para BPF | `lsm/vmlinux.h` | Escrito a mano, nueve structs. Quita `bpftool`, `CONFIG_DEBUG_INFO_BTF` y `/sys/kernel/btf/vmlinux` de lo que hace falta para **construir**. Una prueba exige que todo struct esté bajo `preserve_access_index`, porque sin él el programa lee para siempre un offset inventado y no falla nunca |
| Kernel y arranque | `image/` | Makefile y `thalyx.config` desde `allnoconfig`. **Ejecutados: 6.12.101 compila y la imagen arranca en QEMU** — procedimiento en [[Primer-Arranque]] |
| Los prerequisitos de construir | `image/Makefile` (`doctor`) | Junta **todas** las herramientas que faltan antes de descargar o compilar nada, y las contesta con una línea de `apt`. `all` depende de él primero, y hay una prueba que lo lee. Es el paso 1 del [[Criterio-de-Salida-Fase-1]]: lo que detiene a la persona ajena nunca es Thalyx, es un paquete encontrado de uno en uno |

### Decretos que el código ya hace cumplir

- El área de staging vive en el subvolumen del destino, nunca en `/tmp`.
- La publicación es `rename` de directorio + intercambio atómico de symlink.
- Instalar no ejecuta código del módulo.
- El núcleo recalcula el hash; no acepta el que le reporten.
- Las solicitudes de autorización las genera el núcleo desde campos del manifiesto, sin ningún parámetro de texto libre.
- Se presenta el conjunto completo de permisos del manifiesto, no un subconjunto.
- Un cambio de clave para un `id` conocido es error duro.
- Los permisos solo tienen vigencia mientras el módulo sea la versión actual.
- El journal declara su propio alcance al mostrarse.
- Silencio no es consentimiento: sin terminal, la confirmación se rechaza.
- Una operación interrumpida deja una intención sin resolver, no un vacío, y la reconciliación la resuelve contra el disco.
- El filesystem es la verdad: el índice es un caché y **toda consulta devuelve su grado de actualización junto con las filas**, de modo que quien lee no puede olvidarse de la advertencia.
- El índice falla cerrado: lo que no se puede determinar cuenta como obsoleto.
- Una referencia que apunta fuera del árbol se conserva sin destino en vez de inventarse uno.
- Cada campo con efecto del contrato declara su procedencia, y el núcleo rechaza los que vienen de contenido no confiable **antes de abrir nada**.
- Un origen ausente se rechaza, no se asume confiable.
- El journal registra el origen **menos** confiable del contrato, no el más.
- La política está en el kernel **antes** de que el proceso esté en el cgroup, y el proceso está en el cgroup **antes** de la primera instrucción del módulo.
- Al terminar, la política se retira **antes** de borrar el cgroup: el id es un número de inodo y los inodos se reutilizan.
- Un módulo cuyos permisos nada puede aplicar **no se ejecuta**, salvo que se pida explícitamente y el journal lo registre como degradado.
- Un módulo no puede escribir en `.thalyx/` dentro de su propio árbol: ahí vive el registro de lo que tiene permitido hacer.
- El modo de los archivos del artefacto se aplica enmascarado: setuid, setgid y sticky nunca sobreviven a una instalación.

### El techo de memoria lo pide el manifiesto

Decidido por Cesar el 2026-08-28 y construido el mismo día. `module_standard`
topaba en 1 GiB y ningún manifiesto podía pedir más, así que el motor de
inferencia no cabía. Ahora una petición de memoria es un permiso `persistent`
—`resource = "memory"`, `action = "4GiB"`— que sale por el camino confiable y el
registro que ya existían, y `for_permissions` sube el techo. El gigabyte pasa de
techo a **piso**. Entero en [[Motor-de-Inferencia-como-Modulo]].

### Una pregunta, dos caras — la pantalla deja de ser de sólo lectura

`crates/thalyx-cli/src/ask.rs`, del 2026-08-28. Bajo `thalyx-capture` el
descriptor 0 es `/dev/null`, así que los **ocho** lugares que se detienen a
preguntar encontraban que no hay terminal y se negaban: en la cara con la que la
máquina arranca, `instalar`, `ejecutar`, `observar`, `instalar-en` y `editar` se
podían leer y no se podían acabar.

Una sola comparación —`Accepts`— que las dos caras llaman, con cuatro respuestas
(`Yes`, `No`, `NoOneToAsk`, `Unreadable`, y las dos últimas son distintas por la
regla 10). La negativa se queda en cada verbo, porque esa frase es del verbo y no
del preguntar. El cambio que lo hace posible es de orden: **decir de qué se trata
va antes de revisar si hay terminal**, porque el contexto es la confirmación.
`thalyx_capture::said_so_far` es de dónde sale ese contexto en la pantalla.

**Lo dibujado sólo se ve arrancando la imagen.** Etapa 42 de `verify.sh` para lo
que sí se puede medir aquí.

### El teclado, que hasta el 2026-08-28 no podía escribir español

`crates/thalyx-term/src/keymap.rs`. El kernel lleva un mapa compilado adentro y
es US QWERTY; `loadkeys` no cabe en la imagen. La tecla que en un teclado
latinoamericano dice `ñ` mandaba `;`, y `á` no se podía teclear en absoluto.
`thalyx-screen` calentaba los glifos de `áéíóúüñ¿¡` desde antes: se podían
dibujar y no escribir.

- Las tablas se **generan** de `kbd` con `dev/keymap-table.py` y nunca se
  escriben a mano. Una distribución es un dato sobre el mundo, y la regla 6
  aplica: leer `la-latin1.kmap` directo era la trampa, son cuarenta renglones de
  diff contra dos includes.
- Dos distribuciones: `latino` y `ingles`, y la segunda es el mapa propio del
  kernel —el mismo, no algo parecido— para que el regreso sea exacto.
- `teclado` dice qué hay **preguntándole al kernel** con `KDGKBENT`, no
  preguntándole a Thalyx qué mandó. Tres respuestas: una de las mías, otra cosa,
  o no se pudo leer.
- Se carga en el arranque, se reporta como un montaje, y `thalyx.teclado=no` en
  la entrada de arranque no carga nada.
- `ensayo teclado <distribución>` dice qué tecla pasaría de qué a qué sin tocar
  la consola.

**Que la carga de verdad funcione sólo lo contesta su hierro.** Etapa 43, que
lee y nunca escribe: un keymap es un interruptor global sin dueño (regla 11).

#### Y no funcionaba: la representación no era la que pide el ioctl (2026-08-28)

Corriendo la imagen: con `la-latin1` cargado, **hasta las teclas ASCII** —
`qwerty`, `asdfgh`, letras que no tienen nada que ver con el español — dibujaban
cuadros. La misma imagen arrancada con `thalyx.teclado=no` escribía bien. Ese
control deja la falla en la carga del keymap y en ningún otro lado: ni QEMU, ni
el framebuffer, ni la fuente, ni el decodificador de entrada.

La causa: las tablas que emite `loadkeys --mktable` están en la representación
*interna* del kernel (`q` es `0xfb71`, `ñ` es `0xf0f1`), y `KDSKBENT` no recibe
esa forma — `drivers/tty/vt/keyboard.c` pasa lo que le da userspace por `U(x)`,
`x ^ 0xf000`, antes de guardarlo. Se le entregaba `0xfb71` directo, guardaba otra
cosa, y la tecla dibujaba un cuadro.

**Y la verificación lo tapaba.** `KDGKBENT` aplica la misma transformación de
salida, así que el valor mal mandado volvía idéntico y todo lo que le preguntaba
al kernel qué tenía —`loaded()`, la sonda de `teclado`— comparaba igual sobre un
teclado que no servía. Regla 5 otra vez, y esta vez el instrumento equivocado era
la simetría del propio kernel: un round-trip que coincide no prueba que lo
guardado sea lo que se quería.

La conversión quedó en `crates/thalyx-syscall/src/lib.rs`, en `keymap_to_ioctl` y
`keymap_from_ioctl`, con sus pruebas. **No en `keyboard.rs` ni en las tablas
generadas**: la diferencia no es un dato de la distribución, es el ABI de esos
dos ioctls. Todo lo que está arriba de esa frontera —`loaded()`,
`keymap::produces()`, `Layout::plainly()`— sigue hablando una sola
representación, la de las tablas.

### El editor, que en la pantalla no abría (2026-08-28)

Corriendo la imagen: `crear prueba.txt` funcionaba y creaba `/home/prueba.txt`;
`editar prueba.txt` desde la pantalla gráfica contestaba **«there is no terminal
here to draw an editor on; address lines instead»**. En la superficie que es
puro lugar donde dibujar.

La causa no era un chequeo faltante sino **dónde estaba el chequeo**. El editor
de `crates/thalyx-cli/src/edit.rs` pedía el tamaño con
`terminal_size(stdin)`, leía teclas del descriptor 0 y escribía ANSI al 1. Bajo
la pantalla el 0 es `/dev/null` y el 1 es el buffer de `thalyx-capture`, así que
la respuesta honesta de ese chequeo era «no hay terminal» — y fingir una habría
metido las secuencias de escape en la conversación como texto.

El arreglo: `editar <archivo>` sin subverbo ya no abre nada, **contesta una
transición**. `Flow::ToTheEditor(ruta)`, la segunda de esas —`Flow::Emptied` fue
la primera, por la misma razón: hay verbos cuyo significado es una propiedad de
la superficie, y entonces la superficie es la que los termina.

- sesión de texto → `edit::on_this_terminal`, el editor ANSI de siempre, con su
  chequeo de `terminal_size` intacto y su refusal `no_screen` para un pipe;
- pantalla gráfica → `screen::edit_on_the_glass`, que dibuja en el framebuffer y
  lee del teclado que la pantalla ya tiene duplicado, **fuera del capture**.

**Un solo motor.** `thalyx-edit` sigue decidiendo qué es una edición, qué hace
cada tecla, qué cabe en la vista y qué es guardar; lo que se agregó en
`thalyx-screen` es `Screen::editor`, un cuadro de cadenas y dos números que el
motor ya calculó. Un segundo motor es cómo uno de los dos termina guardando un
archivo que el otro habría rechazado.

Falta el hierro: que se vea bien a su resolución sólo lo contesta su máquina.

### La red, que se ve y no se usa

Punto 8 de la terminal usable, decreto en [[Red]]. La configuración del kernel
pasó de 110 opciones a 118: `NETDEVICES` y `ETHERNET`, que son menús y no
drivers, y cuatro drivers con su razón escrita al lado —`virtio_net` para lo que
QEMU entrega, `e1000` para la tarjeta emulada por omisión, `e1000e` para las
Intel PCIe de la mayoría de los equipos, `r8169` para las Realtek del resto.

El verbo es `red`, y el motor es `thalyx-net`. Lo que contesta:

- qué interfaces hay, ordenadas por nombre para que dos corridas se puedan
  comparar;
- de cada una: tipo, dirección física, estado, si hay cable, velocidad, MTU y
  driver;
- **cuántas de ellas son una tarjeta**, que no es lo mismo que cuántas hay.

Tres cosas que se miden y no se citan, cada una una manera de mentir:

| Lo que parece | Lo que es |
|---|---|
| Una interfaz abajo no tiene cable | Contesta `EINVAL`; no dice `0`. Son *no hay cable* y *no se puede saber* |
| `speed` es un número | Tiene tres estados: un número, `-1` con el enlace arriba, y no legible |
| `type 1` es una tarjeta Ethernet | `ifb0` y `ifb1` también dicen `1`. Una tarjeta cuelga de un bus y tiene `device` |

La tercera salió corriéndolo: la primera versión reportó **tres tarjetas en una
máquina con una**, y ninguna prueba de fixture lo vio.

No hay dirección, no hay DHCP, no hay resolvedor y no sale un paquete — y la
respuesta lo dice, en las dos caras, porque es la única lista del sistema cuyas
cosas ningún verbo puede usar.
| Protocolo del puente anfitrión↔máquina | `crates/thalyx-bridge` | Marcado por largo de 4 bytes y JSON UTF-8; `hello`/`request`/`response`/`error`. Lo enlazan los dos extremos, así que hay una definición de frame y no dos |
| Sesión de agente externo | `crates/thalyx-cli/src/external.rs` | Lista de verbos y guardián de rutas. Cada ruta se resuelve como la resuelve el verbo **y** como la resuelve el kernel, y las dos tienen que caer dentro del workspace; un `..` se rechaza de plano. `apagar`, `instalar-en`, `correr`, `ejecutar`, `negar` y `matar` no son alcanzables |
| El extremo dentro de la máquina | `crates/thalyx-cli/src/bridge.rs` | Un hilo de la sesión, no un programa nuevo en la imagen. Encuentra el puerto por su nombre en `/sys/class/virtio-ports`, nunca por su número. Sin puerto: ni error, ni espera, ni una línea en el arranque |
| Adaptador MCP | `crates/thalyx-mcp` | **Desde el 2026-08-30 las instrucciones del `initialize` se derivan de la superficie realmente ofrecida** — una frase por herramienta, viviendo al lado de la herramienta que nombra — porque las escritas a mano decían «prefiere thalyx_symbol y thalyx_dependencies» a un modelo que tenía tres herramientas y ninguna de ésas; hay una prueba mecánica que rechaza cualquier `thalyx_…` que no esté ofrecido, y otra sobre el tamaño en bytes de las tres descripciones calientes (techo 2 048; `thalyx_exec` pesaba 3 649 y pesa 2 037). Doce herramientas sobre stdio — la duodécima es `thalyx_context`, que es la que un agente debería llamar antes de leer un archivo. Desde el 2026-08-29 `thalyx_edit` y `thalyx_file` aceptan `attempt: "begin"`, que manda dos preguntas —el snapshot primero— en un solo viaje, y las instrucciones del `initialize` nombran todas las herramientas, generadas de lo que la máquina ofrece, porque dos de tres corridas reales gastaron su primera búsqueda nombrándolas mal, **servidas una a la vez**: lee un mensaje, hace el viaje completo a la máquina, contesta, y hasta entonces lee el siguiente — así que llamadas concurrentes del cliente se ejecutarían en fila. No abre un archivo del workspace: es adaptador, y la superficie de Thalyx sigue siendo la autoridad. Descarta una herramienta cuyo verbo la máquina no anuncie. Las descripciones dicen **qué no sabe** cada primitiva, no sólo qué contesta: un agente que sobreinterpreta `dependencias` comete el error lejos de aquí |
| Métricas de una sesión de agente | `crates/thalyx-mcp/src/metrics.rs` | Tiempo, llamadas, herramientas, bytes enviados **y** devueltos, errores, archivos leídos, búsquedas, intentos. Desde el 2026-08-29 también `context` —preguntas, bytes devueltos, bytes descritos y no devueltos, y cuántas veces más chico salió— y `semantics` —consultas, aciertos de cache, arranques de rust-analyzer, aciertos y fallos del cache de validación—, leídos de la respuesta de la máquina y no contados aquí, porque el número interesante no es el tamaño del JSON sino el del archivo que nadie tuvo que leer. Sin tokens: este proceso no los ve, y una estimación sería un número que parece medición. Desde el 2026-08-29 también `machine_requests` y `machine_seconds` — preguntas que de verdad llegaron a la máquina, que no son las llamadas, y cuánto tardaron en contestarse: es lo único que separa el puente del tiempo que el modelo pasa pensando, y deja que cada corrida real conteste eso sobre sí misma sin gastar nada. Y `programs`, que cuenta lo que la **máquina** hizo entre dos inferencias —operaciones, procesos, bytes que no salieron— porque dos corridas de una llamada cada una se ven idénticas haya hecho esa llamada una cosa o treinta. Ver [[Trabajo-Entre-Inferencias]] |
| Costo del puente, medido sin modelo | `dev/bridge-cost.sh` | `thalyx-mcp` contra `thalyx bridge serve --listen` sobre un socket, con un guion fijo de llamadas. En este anfitrión: **0.40–0.55 ms por pregunta dentro de la máquina y 0.08–0.10 ms en el adaptador**. Sin QEMU ni virtio, así que es el piso y no el número de una corrida real — y dice que los ~6 s entre reloj total y tiempo de API del banco **no son el puente**. Pide `--surface legacy` por nombre desde el 2026-08-30: lo que se mide es el cable, y los tres verbos que lo aíslan quedaron en esa superficie cuando la de omisión pasó a tres herramientas — pedirlos sin decirlo hizo que la corrida hiciera **cero** peticiones y la etapa reportara `NOT PROVEN` sin un número. Ahora cuenta lo que mandó contra lo que llegó, y una respuesta con `ok: false` no cuenta como llamada medida. `--surface compact` es la otra columna |
| Arnés de comparación de dos brazos | `dev/bench-external-agent.sh`, `dev/bench-summary.py` | Claude Code normal contra Claude Code con sólo herramientas Thalyx, mismo prompt y mismo modelo. **Desde el 2026-08-30 se niega a correr si la hoja de respuestas está dentro del corpus**: hashea lo que se pasó como `--expect-file` y busca esos bytes en cualquier lugar de `--project` que un agente pueda abrir, antes de gastar un centavo, porque en la corrida compacta el brazo B abrió `dev/bench-expect/<nombre>.txt` y una llamada MCP entera se fue en leer la respuesta. Es sobre los bytes y no sobre la ruta, y falla cerrado: una llave que no se pudo leer detiene la corrida. **El `mcp.json` del brazo B lleva `alwaysLoad: true`**, que quita la ronda de `ToolSearch` antes de la primera llamada — medido con línea base y control en `dev/toolsearch-check.sh`. Y `--transcript` imprime cada llamada de una corrida terminada entera, con lo que la contestó. Desde el 2026-08-28 lee `--output-format stream-json`, así que **las dos ramas** se miden igual: herramientas por nombre, bytes devueltos al modelo, archivos leídos, búsquedas, además de turnos, tiempo, tokens y costo. El parser está aparte y trae `--self-test` contra una sesión real capturada — regla 6. `--expect-file` da veredicto de la tarea; sin él, no hay veredicto Desde la misma fecha hay una tercera tarea, `--task reversible`: renombrar un símbolo en su definición y en todos sus dependientes, comprobar qué se tocó, y dejar el árbol byte por byte como estaba. El veredicto es una conjunción de tres instrumentos distintos —cambió de verdad (stream), restauró (hash del árbol en el anfitrión), contestó bien (`--expect-file`)— porque un agente que no hace nada restaura el árbol perfecto. El brazo B se comprueba en dos pasos, con la máquina apagada y `agent-export`; sin eso dice `not_proven`, y `THALYX_REQUIRE_RESTORE_CHECK=1` lo vuelve falla | Desde el 2026-08-29 el brazo A está **anclado**: su copia se escenifica fuera del checkout (`--workspace`), se revisa cada ancestro por `CLAUDE.md`, `.claude/`, `.mcp.json` y `.git`, un hook `PreToolUse` rechaza cualquier ruta de afuera, y después se lee del stream el `system init` y todas las rutas de todas las llamadas — una sola afuera deja la corrida `INVALID`, comprobado **entre los dos brazos**. El brazo B se prueba vivo antes de pagar el A (`thalyx-mcp --preflight`: hello, `where`, `list .` comparado con `--project`, todo de sólo lectura), y `provenance.json` guarda commit de origen, manifiesto de entrada, exclusiones y directorio efectivo de cada brazo, con los dos sellos escritos por el mismo programa; entradas distintas detienen la corrida antes de llamar a nadie. La clasificación de mutación tiene tres valores —`writes`, `reads`, `unknown`— porque el nombre de una herramienta es una intención y no un efecto: `Bash` es siempre `unknown`, y un testigo que no vio nada con llamadas `unknown` en el stream contesta `not_proven`, nunca `false`
| Conocimiento persistente con testigo | `crates/thalyx-know` | Una tabla SQLite por árbol, en el store. Cada dato trae la identidad del estado del que salió, y recordarlo contesta `current`, `stale` o `unknown` — no hay forma de sacar el valor sin la postura. El testigo es **sólo de contenido y acotado**, a diferencia del de `thalyx_snapshot`: un árbol restaurado byte por byte es el mismo árbol, y un cambio en un paquete del que no se depende no invalida nada. Un testigo incompleto no coincide con nada, ni consigo mismo. Ver [[Conocimiento-con-Testigo]] |
| Proveedor semántico de Rust | `crates/thalyx-rust` | `cargo metadata --no-deps` para el espacio de trabajo y el grafo de crates; rust-analyzer por LSP para qué *es* un nombre. Resuelve el caso que el escáner lleva meses declarando imposible: `Keys` en `boot.rs` es `Keystore`. **Nunca escribe**: un rename regresa como el texto que cada archivo debería tener. Arranca un rust-analyzer por árbol y lo conserva —25 s el arranque, 20 ms la pregunta— y todo lo que aprende queda en el conocimiento, así que una sesión nueva no lo paga. Corre **bajo el confinamiento de Thalyx** donde el kernel puede denegar, y cae al anfitrión donde no —diciéndolo en `analyzer_confined` y `analyzer_how` de cada respuesta—. La etapa 59 de `verify.sh` establece cuál de las dos ocurrió, adentro de una ventana de denegación que esa etapa abre y cierra ella misma. Ver [[Semantica-Compilada]] |
| Validación de lo afectado, y su cache | `crates/thalyx-rust/src/affected.rs`, `crates/thalyx-cli/src/exec.rs` | El check `rust` de `hacer` compila los crates que el cambio **alcanza** —los que lo contienen más todo lo que depende de ellos, del grafo de Cargo—, no los que lo contienen. La identidad del cache es la clausura de dependencias más el manifiesto, el candado y el toolchain; sólo se guarda un veredicto sobre el árbol, nunca un `not_proven`, y nunca la salida del compilador. Se construye en el store y no en el árbol, o el snapshot llevaría el `target/` adentro |
| `contexto` y `renombrar-simbolo` | `crates/thalyx-cli/src/semantic.rs` | La cara caliente. `contexto` contesta qué es un nombre en unos cientos de bytes con un asa, `contexto expandir=<asa>` trae exactamente las líneas de esa declaración, `presupuesto=N` acota y dice qué no cupo, y `usos=N` pide los lugares donde se usa — el número siempre viene, la lista sólo si se pide, porque sobre un nombre común la lista es todo el presupuesto. Cada respuesta dice `source` —`rust-analyzer` resolvió, `index` coincidió— y `fresh`. `renombrar-simbolo` escribe en cada lugar que de veras usa el nombre, no donde el texto coincide, y por la frontera de la sesión. **Desde el 2026-08-30 contesta también `edits_by_file`** —cuántos lugares tocó en cada archivo— contado del `WorkspaceEdit` que rust-analyzer ya entregó, mientras se aplica, nunca re-escaneando el árbol: un segundo pase sería un conteo textual de una cadena, que es justo lo que este camino existe para superar. Y `definition` aparece **sólo cuando el lugar se alcanzó a través de la declaración del símbolo**; dado `archivo:línea:columna` el llamador apuntó a algún lado y el campo se omite en vez de inventarse. Ver [[Contexto-Progresivo]] |
| Importar un proyecto a la máquina | `image/Makefile` (`agent`, `run-agent`, `agent-export`) | Una copia descartable, como subvolumen propio para que `intento` tenga qué fotografiar. El checkout del anfitrión no se toca y no es alcanzable desde adentro |

### El runtime Rust del agente, que es de Thalyx y no del anfitrión — 2026-08-31

`dev/build-rust-runtime.sh` construye un artefacto de 644 MB desde tarballs
oficiales con digest fijado —las herramientas musl de Rust, más un `libc.so` de
musl compilado aquí y un `libgcc_s.so.1` enlazado del `libunwind.a` del propio
Rust— y `make -C image agent PROJECT=…` lo pone en el store, jamás en la imagen.
`thalyx-rust::runtime` lo encuentra, `thalyx dev rust-runtime` comprueba que se
lleva consigo todo lo que sus programas nombran, y PID 1 hace que
`/lib/ld-musl-x86_64.so.1` apunte a su cargador. El verbo `toolchain` y
`thalyx-mcp --preflight --needs-rust` hacen que un banco no pueda volver a pagar
por una máquina sin compilador. Entero en [[Runtime-Rust-Agente]].

**Lo que no hace:** compilar. No lleva enlazador, a propósito y explicado ahí.

Y desde el 2026-08-31, más tarde: `toolchain::environment` le entrega a los
hijos del toolchain un `PATH` **construido por Thalyx** con una sola entrada, la
del `bin` del artefacto. rust-analyzer lanza `cargo metadata` y `rustc` por
nombre pelado, y sin `PATH` el workspace no cargaba: la VM podía listar los
símbolos de un archivo y no resolver ninguno. Un toolchain instalado pone su
`bin` adelante del `PATH` heredado en vez de reemplazarlo, que es además lo que
hacía falta bajo `sudo`. Y `dev/verify-agent-rust.sh` ya no puede dar el falso
positivo que dio: para declarar PROVEN exige `resolution == "one"` y la
declaración esperada entre las entradas, no sólo que `source` diga
`rust-analyzer`.

## No construido todavía

| Pieza | Bloqueante para |
|---|---|
| La gama máxima, medida | Cerrar la cuarta fila de [[Gamas-de-Modelo]]. Necesita una máquina con más de 16 GB: en la de desarrollo el proceso muere antes de la primera inferencia |
| Una segunda corrida de acierto en cualquier gama | Saber cuánto se mueven esas cifras entre corridas. Disco, RAM y latencia ya tienen réplica; el acierto no |
| Los casos sin medición del banco | Ninguna fracción de acierto es todavía la puntuación de su gama: 6 casos en ligera y 1 en media y en alta no produjeron respuesta |
| Salir a internet | Que el store pueda traer un módulo de algún lado. DHCP, DNS y TLS tendrían que vivir dentro de `thalyx`, y **de dónde** es una pregunta de Fase 2 sin contestar. Ver [[Red]] |
| WiFi | Necesita firmware binario en la imagen y un suplicante WPA, que en todos lados es un demonio aparte. Obliga a revisar qué quiere decir «el kernel y un programa» |
| Que un Qwen2.5 real acierte la intención desde la pantalla | Es una medición del modelo, no de la máquina: la cadena entera está construida y corrida con un llama.cpp real, y lo que falta es correr `thalyx agent bench` contra las gamas desde adentro |

> **Actualizado el 2026-08-28, más tarde.** Los dos renglones de abajo se
> quitan: los dos están construidos. El tope de memoria lo pide el manifiesto y
> lo aprueba Cesar al instalar, y el motor es un módulo instalado que corre
> confinado — ver el renglón «El motor de inferencia, como módulo» arriba y
> [[Motor-de-Inferencia-como-Modulo]].
>
> **Actualizado el 2026-08-28.** Se agregan dos renglones, y el primero es una
> ausencia que llevaba desde el principio sin estar escrita: **el agente no está
> en la máquina.** Nada mintió — cada nota del agente decía la verdad sobre lo
> que el agente hace — pero nadie había preguntado *dónde corre*, y la respuesta
> es que nunca ha corrido sobre Thalyx. Se encontró preguntando qué le falta al
> sistema para ser usable, no auditando el agente.
>
> **Actualizado el 2026-08-08.** Esta tabla decía *«el `Model` real»*, *«la
> gramática GBNF»* y *«el banco de las cuatro gamas»* como no construidos, y las
> tres estaban construidas y corridas contra hierro desde ese mismo día — es la
> regla de esta bóveda sobre las afirmaciones de ausencia, que nada rompe cuando
> dejan de ser ciertas. Lo que queda no es código sin escribir: es **medición
> que esta máquina no puede producir**.

### Las advertencias que quedan

**0. Un módulo con cero permisos no corre confinado sin el LSM cargado**, y eso
**no es un defecto** — se registró como tal el 2026-08-03 y la lectura estaba
mal. `run.rs:216` se niega si el mapa de política no está disponible, sin mirar
cuántos permisos declara el módulo, y en Thalyx **el LSM se carga en el
arranque** porque Thalyx es dueño del arranque ([[Decision-Capa-vs-SO-Nuevo]]).
Que el mapa falte significa que algo está roto, y negarse es lo correcto.

El argumento con el que se archivó como defecto —"en casi ninguna máquina hay
`bpf` en el orden de LSM"— daba por supuesto un modelo de despliegue que este
proyecto nunca tuvo: instalar Thalyx encima del Linux de alguien más. Ver la
sección de andamio en [[Decision-Capa-vs-SO-Nuevo]].

**1. El perfil no crea un user namespace para el módulo.** Lo que sí hay es un
uid propio por módulo, al que el lanzador desciende con `setresuid` y **relee el
uid efectivo** antes de ejecutar nada, porque un `setuid` que reporta éxito sin
haber cambiado nada se ve igual que uno que funcionó. Un user namespace daría
además un mapa de ids propio; su ausencia no significa que el módulo corra con
el uid de Thalyx. Ver [[Sandbox-Ejecucion]].

**2. El atajo del índice ya se puede encender, y solo ganándoselo.** El contador cubre todo lo que un proceso puede hacerle a un archivo y está acotado al árbol: verificado en hardware con 5000 escrituras dentro contadas y las mismas 5000 fuera ignoradas. `thalyx graph trust --counter` corre la verificación y se niega si no coincide. Se devuelve solo cuando el kernel deja de poder responder. Ver [[FS-en-Grafo]].

**3. `ls -l` se degrada dentro del sandbox.** `socket` está fuera del allowlist a propósito, y NSS quiere un socket unix para resolver nombres de usuario. Es el costo visible de la decisión, no un defecto.

**4. El agente ya vio un modelo, y tres gamas están medidas.** Esta advertencia
decía *«el agente nunca ha visto un modelo»*; dejó de ser cierta el 2026-08-08.
El router, la atribución y el ensamblado siguen probados contra un falso que se
porta mal a propósito —eso cubre lo que el agente hace con lo que el modelo le
entrega— y la otra mitad ya corrió contra `llama.cpp` real: las banderas se
aceptan, la gramática restringe (probado en media y alta), y ligera, media y alta
tienen cifras de coste y de acierto en [[Gamas-de-Modelo]].

Lo que sigue sin probarse, y ahora se puede nombrar con precisión: la **gama
máxima**, que no cabe en la máquina de verificación; que la gramática restrinja
en **ligera**, cuyo modelo no contesta el sondeo; y **cualquier cifra de acierto
repetida**, porque cada gama se ha medido una sola vez. `verify.sh` sigue
reportando `NOT PROVEN` sin `THALYX_AGENT_WEIGHTS`, y
`THALYX_REQUIRE_AGENT_TESTS=1` lo convierte en fallo. Ver [[Agente-Minimo]].

## Pruebas

**1 649 pruebas** en total, en los tres niveles de [[Estrategia-de-Pruebas]]. Las 112
del agente corren además en su propia etapa de `verify.sh`, para que si el crate
desapareciera del workspace el total bajara **y se supiera cuáles faltan**. Los de nivel 2 matan el binario real con `SIGABRT` en cada punto del commit, incluido el instante entre los dos `rename`, y verifican consistencia **y recuperación**.

Las pruebas de aislamiento corren contra el kernel real y **le preguntan al módulo qué ve**, no al sistema si aisló. Las de cgroup corren contra un montaje cgroup2 real. Donde no lo hay, **imprimen `NOT PROVEN` y dicen que no probaron nada** en vez de pasar en silencio; hay **diecinueve** variables distintas y cada una convierte en fallo los saltos de *su* requisito. La séptima es de una clase nueva: no salta por lo que a la máquina le falta sino por **quién está corriendo** — como root, quitarle todos los permisos a un archivo no impide leerlo, así que la prueba de la regla 10 no tendría cómo fallar. Antes había una sola, y entonces la única forma de exigir lo que una máquina sí tiene era exigir lo que no tiene. Una prueba que pasa sin haber ejercitado lo que nombra es exactamente cómo una herramienta de seguridad llega a leerse como armada estando desarmada.

`verify.sh` activa las cuatro primeras cuando la máquina las soporta. La de la
imagen se pide a mano —`THALYX_REQUIRE_IMAGE_TESTS=1`— porque exige un kernel y
un disco ya construidos; **corrida así el 2026-08-06 con idéntico resultado**,
que es lo que demuestra que la etapa del arranque no se estaba saltando. La del
agente es distinta por naturaleza: no hay máquina que la satisfaga todavía,
porque lo que le falta al agente no es hardware sino código.

### Las pruebas que fuerzan un entrelazado — 2026-08-29

Una clase nueva, y la única que puede ver la familia de defectos que cerró los
cuatro P0 del 2026-08-28: **una regla comprobada y no impuesta se comporta
igual que una impuesta mientras haya un solo cliente**, y cada prueba que había
hacía una cosa a la vez.

- **Dos clientes con una barrera**, en `attempt.rs`: dos hilos que abren su
  propia descripción de `state/lock` —que es lo que tienen dos procesos— y
  arrancan a la vez. Sin la barrera el primero termina antes de que el segundo
  empiece.
- **Un adversario en paralelo**, en `external.rs`: cuatro pruebas, una por verbo,
  con un hilo cambiando un directorio por un symlink mientras 4000 peticiones
  entran. De un solo lado —la afirmación es *cero* escapes— y con el conteo de
  rechazos al lado como control, sin el cual «no escapó nada» y «no hubo
  carrera» se leen igual.
- **El estado intermedio sostenido a mano**, en `thalyx-sandbox`: dos
  confinamientos establecidos y nada adentro del cgroup, que una máquina real
  sostiene un instante y un fake sostiene para siempre. Con su mitad de kernel
  en `tests/real_cgroup.rs`, con un proceso vivo adentro.

**Los tres arreglos se comprobaron quitándolos**, y ésa es la parte que no es
opcional: un test de concurrencia que nadie vio fallar pasa igual si mide la
propiedad y si no mide nada.

## Lo que la auditoría del 2026-08-04 cambió

Nueve defectos reales encontrados desde fuera, ninguno de los cuales veía
ninguna de las 612 pruebas de entonces. Corregidos, con pruebas que se comprobó
que fallan sin el arreglo, y **ejercidos en hardware por la ruta confinada el
2026-08-06** — que era lo que faltaba, porque este contenedor no puede atachar
el LSM. Dos de los arreglos rompieron el instrumento que los medía antes de
llegar ahí; ver [[Punto-Actual]].

| Qué estaba mal | Dónde | Cómo quedó |
|---|---|---|
| El lock global de [[Concurrencia]] no existía | `thalyx-core/store.rs` | `flock(2)` sobre `state/lock`, probado entre procesos |
| Una actualización interrumpida daba a la versión vieja los permisos de la nueva | `thalyx-core/permissions.rs` | La concesión graba su versión; sólo vale si `current` la nombra |
| Un keystore corrupto se leía como vacío, y uno vacío confía en todo | `thalyx-core/keystore.rs` | `StateUnreadable`: ausente y corrupto dejan de ser lo mismo |
| Los permisos `session` nunca se revocaban | `thalyx-core/session.rs` | Id de sesión en `state/session`; terminarla es una escritura |
| `net/outbound` quitaba el netns y seccomp seguía prohibiendo `socket` | `thalyx-sandbox/profile.rs` | El allowlist se amplía con la concesión, no antes |
| El camino confiable dibujaba texto del publicador sin sanear | `thalyx-core/trusted_path.rs` | `sanitise` en cada campo; el marco no se puede falsificar |
| El módulo heredaba la terminal donde se dibuja el prompt | `thalyx-sandbox/launch.rs` | `stdin` cerrado; `stdout`/`stderr` a tuberías que Thalyx drena y reimprime marcadas, también con `--unconfined` |
| Comprobar la ruta y abrirla eran dos momentos | `thalyx-core/api.rs` | `openat2` con `RESOLVE_BENEATH` sobre un descriptor de la concesión |
| Un módulo podía hacer crecer sin límite la memoria de Thalyx | `thalyx-core/api.rs` | Techo por cantidad y por bytes; lo descartado se cuenta y se dice |

Menores, del mismo pase: el journal tolera una última línea cortada —lo que deja
un corte— y sigue rechazando una corrupta en el medio; los `request_id` son uuid
y no nanosegundos del reloj; el manifiesto rechaza campos desconocidos; un
paquete con dos `manifest.toml` se rechaza en vez de resolverse; los archivos de
estado se escriben con `fsync` y nombre temporal único; y el tarball del kernel
exige un digest anclado.

## Relacionado
- [[Tareas-Pendientes]]
- [[Estrategia-de-Pruebas]]
- [[Fases-de-Implementacion]]
- [[Criterio-de-Salida-Fase-1]]
