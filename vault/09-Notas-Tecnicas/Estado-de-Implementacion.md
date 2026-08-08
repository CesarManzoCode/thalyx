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
| Filtro seccomp (BPF clásico) | `crates/thalyx-sandbox/seccomp.rs` | Lista de permitidos derivada empíricamente |
| Escritura de un Btrfs vacío | `crates/thalyx-btrfs` | Ocho árboles, tres chunks y los superbloques, sin `mkfs.btrfs`; los tres subvolúmenes por ioctl |
| Tabla de particiones GPT | `crates/thalyx-install/gpt.rs` | Una tabla de dos particiones con sus dos copias, sin `sgdisk` |
| Sistema de archivos FAT32 | `crates/thalyx-install/fat.rs` | Un volumen con un archivo en `\EFI\BOOT\BOOTX64.EFI`, sin `mkfs.vfat`. No lee |
| El instalador | `crates/thalyx-install` y `thalyx install` | Particiona, escribe el arranque y hace el store en un acto. **El arranque del disco instalado no lo ha ejercido nadie** |
| Controladores de una PC de verdad | `image/thalyx.config` | Framebuffer, teclado USB y PS/2, NVMe y AHCI, y `console=tty0`. **Ninguna de esas opciones se ha compilado ni arrancado** |
| Lectura de FAT32 y hallazgo del medio | `crates/thalyx-install/medium.rs` | Saca el kernel del medio del que se arrancó, sin montar nada. El medio es el volumen FAT32 etiquetado `THALYX`: la ruta `\EFI\BOOT\BOOTX64.EFI` sola la tiene también la ESP de cualquier PC |
| El store por etiqueta | `crates/thalyx-cli/store_disk.rs` | Lo que decretaba el 2026-08-06 y no existía; con `thalyx disk find` para preguntarlo sin ser PID 1 |
| Instalar desde adentro | `discos` y `instalar-en <disco>` | Los dos verbos que vuelven alcanzable el criterio sin shell |
| Raíz propia del módulo (`pivot_root`) | `crates/thalyx-sandbox/rootfs.rs` | Solo el módulo, el sistema en RO y lo concedido |
| Disciplina de cobertura del índice | `crates/thalyx-graph/watch.rs` | Completa y probada; el atajo se gana y se devuelve solo |
| Contador de mutaciones del kernel | `crates/thalyx-watch` | Diez hooks, acotado al árbol; 5000 escrituras dentro, 0 fuera |
| Memoria persistente | `crates/thalyx-memory` | Dos capas, fechado por rutas, base vectorial propia |
| `rollback` | `crates/thalyx-core/rollback.rs` | Deshace un commit; se niega cuando la entrada ya no describe el disco |
| Snapshots de Btrfs | `crates/thalyx-snapshot` | Tomar, listar, olvidar y **restaurar**; intercambio atómico |
| `restore` | `crates/thalyx-core/restore.rs` | Diff de lo que se pierde, camino confiable, intención antes de mover |
| Límites de cgroup | `crates/thalyx-sandbox/limits.rs` | `memory.max`, `pids.max`, `cpu.max` |
| Syscalls crudas | `crates/thalyx-syscall` | **El único crate con `unsafe` del workspace** |
| Un uid por módulo | `crates/thalyx-core/uids.rs` | Asignado al instalar, retirado al quitar, nunca reciclado |
| Montajes idmapped para lo concedido | `crates/thalyx-sandbox/idmap.rs` | Verificado: escritura concedida funciona y aterriza a nombre del dueño |
| Parser mecánico | `crates/thalyx-parser` | Rust, Python, JS/TS, C, Go |
| Índice en grafo (SQLite) | `crates/thalyx-graph` | Nodos, aristas, etiquetas, obsolescencia |
| `thalyx-lsm` (BPF LSM) | `lsm/thalyx_lsm.bpf.c` | **Demostrado denegando en hardware real** |
| `thalyx-watch` (BPF LSM) | `lsm/thalyx_watch.bpf.c` | Diez hooks, contador por CPU, atribución por ancestros |
| Entorno de desarrollo (VM) | `dev/` | Preflight, guest reproducible, verificación de enforcement |
| Agente mínimo: router, atribución, ensamblado | `crates/thalyx-agent` | Enunciado → contrato → instalación, probado de punta a punta |
| Las cuatro gamas | `crates/thalyx-agent/tier.rs` | Los tamaños son de un tipo que se imprime con `~`, para que un estimado no se lea como medición |
| Gramática GBNF de la propuesta | `crates/thalyx-agent/grammar.rs` | Generada desde la forma que tiene que respetar; las clases de caracteres se comparan contra el escáner de ids, carácter por carácter |
| El prompt y el marcador | `crates/thalyx-agent/prompt.rs` | Aleatorio por invocación, así que la respuesta se localiza sin parsear el formato de `llama.cpp` |
| `llama.cpp` como proceso | `crates/thalyx-agent/llama.rs` | Invoca `llama-completion`, no `llama-cli`. Plazo, tope de salida, muestreo de `VmHWM`, y **comprobación del contrato de una pasada**. Una inferencia real **sí terminó** y dio una propuesta bien formada; falta probar que la gramática es lo que la restringió, y falta medir toda gama |
| La gama elegida y sus pesos | `crates/thalyx-agent/config.rs` | Mide el archivo en vez de creerle a nadie; se niega si cambió de tamaño |
| `agent model`, `agent grammar`, `agent bench` | `crates/thalyx-cli/agent_model.rs` | El banco que [[Gamas-de-Modelo]] pide: intención, argumentos, abstención, latencia y RAM. **Ninguna gama se ha medido** |
| Memoria de tarea del agente | `crates/thalyx-agent/recollection.rs` | Escribe y **lee**; lo no confirmable se muestra y no se usa |
| Repositorio local y resolución de versiones | `crates/thalyx-core/repo.rs` | Máxima versión que satisface el constraint y cuya firma valida |
| CLI `thalyx` | `crates/thalyx-cli` | `module` (con `run`), `agent` (`plan`, `do`), `graph`, `memory`, `rollback`, `journal`, `permissions`, `enforce`, `store`, `dev` |
| Empaquetado de módulos | `crates/thalyx-cli/dev.rs` | `keygen`, `pack`, `inspect`, `agent-probe` |
| API interna: protocolo | `crates/thalyx-abi` | Marco con longitud + CBOR, las tres familias de la v1, cliente y servidor |
| API interna: el servidor de Thalyx | `crates/thalyx-core/api.rs` | Comprueba cada ruta contra los permisos del manifiesto **y contra lo que el kernel resuelve**; un symlink plantado dentro de lo concedido no sale de ahí |
| El canal por el sandbox | `crates/thalyx-syscall`, `crates/thalyx-sandbox/launch.rs` | Socket entregado en el descriptor 3; sobrevive los dos `exec`. **La ruta confinada solo se comprueba en máquina con LSM** |
| Primer módulo | `modules/dev.thalyx.greeter` | Escrito contra la API. Lee lo concedido, es rechazado en `/etc/shadow`, y **no arranca fuera de Thalyx** |
| Sesión del sistema | `crates/thalyx-cli/session.rs` | Lo que init arranca; solo dice que es la máquina cuando lo es. `disponibles`, `instalar <id>`, `permisos`, `revertir`, `recuerdos`, `modulos`, `correr <id>`, `estado`, `nucleo`, `apagar` — adentro no hay shell, así que lo que no es un verbo de aquí no existe. **Cubre los pasos 2, 3, 4, 5 y 6** del [[Criterio-de-Salida-Fase-1]]: instalar y revertir escriben en la memoria persistente por el mismo `recollection.rs` del agente, y `recuerdos` lo lee después de un reinicio |
| PID 1 | `crates/thalyx-cli/init.rs` | Monta siete filesystems, arranca la sesión, cosecha huérfanos. **Corrido como PID 1 el 2026-08-03**: los siete montajes salieron `ok` |
| El cargador de BPF propio | `crates/thalyx-bpf`, `crates/thalyx-syscall` | ELF, BTF, forma de los mapas, CO-RE y las llamadas `bpf(2)`. El objeto va dentro del binario. **Probado entero en hardware**: carga, atacha, y ese enforcement deniega. Ver [[Cargador-BPF-Propio]] |
| Saber si el enforcement está puesto | `crates/thalyx-bpf/src/attached.rs` | Enumera los enlaces del kernel y los sigue hasta el programa que corren, sin `bpftool`, así que **también funciona dentro de la imagen**. `thalyx enforce attached`. Reemplazó tres comprobaciones que contestaban por un pin, un directorio y una cuenta global |
| El disco del store | `crates/thalyx-cli/store_disk.rs`, `image/Makefile` | Tres subvolúmenes Btrfs; PID 1 los monta por `thalyx.store=` y **nunca los crea**. **Arrancó con el disco montado el 2026-08-03.** Lleva el módulo en un repositorio y **sin instalar**, para que instalarlo sea el paso 2 del [[Criterio-de-Salida-Fase-1]] |
| Thalyx escribe el Btrfs | `crates/thalyx-btrfs`, `thalyx disk format` | Ocho árboles, tres chunks y los superbloques, byte por byte, sin `mkfs.btrfs` ni `libbtrfs`. Metadata y system en DUP; ninguna chunk cubre un superbloque. Lo invoca **un humano** y PID 1 no lo alcanza, así que el decreto de no fabricar se conserva. **Validado con `btrfs check`, con los dos headers de uapi capturados, y montado por el kernel de Cesar el 2026-08-07** — etapa 18. Ver [[Construccion-del-ISO]] |
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

## No construido todavía

| Pieza | Bloqueante para |
|---|---|
| El `Model` real (`llama.cpp` como proceso) | Que el agente sirva de algo |
| La gramática GBNF | Lo mismo, y no se puede validar sin `llama.cpp` |
| Banco de las cuatro gamas | Sustituir las cifras estimadas de [[Gamas-de-Modelo]] |

> Al 2026-08-06 esta tabla tiene **una sola causa**: no hay modelo. Todo lo
> demás que estaba escrito está ejercido en hardware — el cargador de BPF, la
> imagen con su enforcement puesto, y los seis pasos desde un arranque frío.
> `proven 104 · not proven 1 · failed 0`.

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

**4. El agente nunca ha visto un modelo.** El router, la atribución y el
ensamblado están probados contra un falso que se porta mal a propósito, y eso
cubre lo que el agente hace con lo que el modelo le entrega. Lo que no está
probado por nada es la otra mitad: que la gramática GBNF sea una que `llama.cpp`
acepte, y qué acierta cada gama. `verify.sh` lo reporta como `NOT PROVEN` en su
etapa 10, y `THALYX_REQUIRE_AGENT_TESTS=1` lo convierte en fallo. Ver
[[Agente-Minimo]].

## Pruebas

667 pruebas en total, en los tres niveles de [[Estrategia-de-Pruebas]]. Las 39
del agente corren además en su propia etapa de `verify.sh`, para que si el crate
desapareciera del workspace el total bajara **y se supiera cuáles faltan**. Los de nivel 2 matan el binario real con `SIGABRT` en cada punto del commit, incluido el instante entre los dos `rename`, y verifican consistencia **y recuperación**.

Las pruebas de aislamiento corren contra el kernel real y **le preguntan al módulo qué ve**, no al sistema si aisló. Las de cgroup corren contra un montaje cgroup2 real. Donde no lo hay, **imprimen `NOT PROVEN` y dicen que no probaron nada** en vez de pasar en silencio; hay seis variables distintas —`THALYX_REQUIRE_CGROUP_TESTS`, `_LSM_TESTS`, `_CONTROLLER_TESTS`, `_BTRFS_TESTS` `_AGENT_TESTS` y `_IMAGE_TESTS`— y cada una convierte en fallo los saltos de *su* requisito. Antes había una sola, y entonces la única forma de exigir lo que una máquina sí tiene era exigir lo que no tiene. Una prueba que pasa sin haber ejercitado lo que nombra es exactamente cómo una herramienta de seguridad llega a leerse como armada estando desarmada.

`verify.sh` activa las cuatro primeras cuando la máquina las soporta. La de la
imagen se pide a mano —`THALYX_REQUIRE_IMAGE_TESTS=1`— porque exige un kernel y
un disco ya construidos; **corrida así el 2026-08-06 con idéntico resultado**,
que es lo que demuestra que la etapa del arranque no se estaba saltando. La del
agente es distinta por naturaleza: no hay máquina que la satisfaga todavía,
porque lo que le falta al agente no es hardware sino código.

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
