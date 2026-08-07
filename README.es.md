# Thalyx

**[🇬🇧 Read it in English →](README.md)**

Un sistema operativo de código abierto diseñado desde el kernel hacia afuera,
donde la inteligencia artificial es ciudadana de primera clase en vez de una
aplicación más, y el ser humano sigue siendo soberano.

> **Thalyx es el sistema operativo.** El kernel de Linux es un componente que
> Thalyx gestiona, no el anfitrión sobre el que descansa. No hay capas
> intermedias, no hay distribuciones — no hay nada que no sea Thalyx. Los
> módulos y el agente se comunican exclusivamente a través de la API de Thalyx,
> no a través de POSIX, no a través de libc, no a través de scripts de shell. Si
> Linux desaparece, Thalyx encuentra otro motor. Si Thalyx desaparece, no hay
> sistema.
>
> La imagen lleva el kernel de Linux y un programa, y eso es **contable** en vez
> de citable: `make -C image count`.

El párrafo anterior es un decreto, y una parte de él es un *destino* más que una
descripción. Lo que es cierto hoy está escrito donde se puede comprobar: ver
[Qué es cierto ahora mismo](#qué-es-cierto-ahora-mismo).

---

## Para qué existe esto

Léelo antes de correr nada, porque los comandos solo tienen sentido cuando ya
sabes qué están tratando de demostrar.

En Windows, Linux y macOS, un agente de IA es un **invitado**. Para hacer
cualquier cosa tiene que fingir que es humano: manejar un teclado y un ratón, o
llamar APIs que son un calco de la interacción humana. Los permisos se diseñaron
para procesos humanos, el planificador para cargas humanas, el sistema de
archivos para jerarquías humanas. Cada uno de ellos es un techo sobre lo que un
agente puede hacer con soltura, y ninguno se puede levantar desde adentro de una
aplicación.

Thalyx invierte la relación: en vez de que la IA se adapte al sistema operativo,
el sistema operativo se construye alrededor de la IA — mientras el humano
conserva un **camino completo y sin degradar** que nunca pasa por el agente.

Esa segunda mitad es la que restringe todo lo demás. Un sistema operativo donde
la IA es la única forma de hacer las cosas es un sistema peor, no uno mejor.

### Las dos reglas a las que responde todo el diseño

**Doble ruta.** Todo lo que el agente puede hacer, un humano lo puede hacer
directamente, sin el agente y sin perder capacidad. El agente es un acelerador,
nunca un intermediario obligatorio. Eso tiene una consecuencia sobre la que está
construido el diseño: Thalyx nunca tiene conocimiento completo de su propio
sistema de archivos —porque eres libre de cambiar cosas a sus espaldas— así que
ninguna operación destructiva puede dar por hecho que sí lo tiene.

**El agente no es confiable.** Vive fuera de la base de cómputo confiable. No
puede ejecutar nada directamente, no puede componer los mensajes con los que se
te pide autorización, y no puede dejar que un texto ajeno que haya leído decida
lo que pasa. El núcleo revalida todo lo que el agente produce.

Si de este repositorio te llevas una sola idea, llévate la segunda. La mayoría
de los sistemas que ponen un modelo de lenguaje cerca de una terminal están a
una inyección de prompt del desastre. Thalyx está construido sobre el supuesto
de que al modelo **sí** lo van a convencer de intentar algo, y se arregla para
que eso sea sobrevivible.

---

## Arráncalo: todo, desde cero

Ésta es la parte que vale la pena hacer. Al final vas a haber arrancado un
sistema operativo que es un kernel de Linux y exactamente un programa —sin
distribución, sin shell, sin `ls`, sin gestor de paquetes—, instalado en él un
módulo firmado, autorizado tú mismo lo que ese módulo pedía, deshecho la
instalación, y reiniciado a una máquina que todavía se acordaba de la
conversación.

Está escrito para **Linux Mint**, y funciona igual en Ubuntu y Debian. También
se ha hecho en Fedora.

### Lo que necesitas

| | |
|---|---|
| **Disco** | ~15 GB libres. El código y la compilación del kernel son casi todo |
| **RAM** | 4 GB. A QEMU se le dan 2 GB |
| **Tiempo** | 20–60 minutos, casi todo compilando el kernel de Linux |
| **Permisos** | `sudo` exactamente una vez, para formatear una imagen de disco |
| **Red** | Para descargar el código del kernel y la cadena de herramientas de Rust |

Tu máquina no se modifica. No se instala nada fuera de este directorio salvo los
paquetes que decidas instalar y la cadena de Rust, y nada toca tu gestor de
arranque, tus particiones ni el sistema que estás usando. Thalyx arranca
**dentro de QEMU**, como máquina virtual.

### Paso 0 — trae el código y pregunta qué falta

```sh
git clone https://github.com/CesarManzoCode/thalyx.git
cd thalyx
make -C image doctor
```

`doctor` no descarga ni compila nada. Existe por una miseria muy concreta: lo
que detiene a la gente aquí nunca es un problema difícil, es un paquete que
falta — encontrado **de uno en uno**, y cada uno solo después de que todo lo
anterior salió bien. Un `bc` ausente te cuesta la descarga y la compilación
enteras del kernel antes de aparecer, y la siguiente herramienta que falte te
las vuelve a costar.

Así que `doctor` los encuentra todos a la vez y te imprime la única línea que
los arregla:

```sh
sudo apt install bc bison build-essential btrfs-progs clang curl dwarves \
                 file flex libbpf-dev libelf-dev libssl-dev qemu-system-x86 \
                 tar xz-utils
```

Vuelve a correr `make -C image doctor` después. Te va a decir si todavía falta
algo, incluido lo que la primera vez no pudo comprobar.

**Rust va aparte**, porque la versión que trae `apt` es más vieja de lo que este
proyecto necesita y no trae el `rustup` con el que se agrega el objetivo
estático:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Después abre una terminal nueva, o haz `source "$HOME/.cargo/env"`.

### Paso 0b — el kernel que vas a compilar ya está anclado

No hay nada que hacer aquí. Está escrito porque vale la pena entenderlo, y
porque es justamente la diferencia entre Thalyx y una distribución.

**Thalyx compila su propio kernel.** Ese archivo no es una dependencia contra la
que se enlaza — *se convierte en la mitad más privilegiada de la máquina que
estás a punto de correr*. HTTPS te dice quién sirvió los bytes. No te dice
cuáles eran, y un CDN que sirviera otra cosa produciría un kernel que nadie
revisó, en una máquina que arrancaría sin decir nada al respecto.

Por eso `image/Makefile` lleva el digest del tarball exacto que esta imagen
construye, y la compilación rechaza cualquier otro. Se estableció el 2026-08-06
contra la lista **firmada** de digests de kernel.org, y la llave que la firmó
está anotada junto al digest — un hash a secas te dice qué se aceptó, no qué lo
estableció.

Si algún `make` dice que el digest no coincide, detente: el archivo que bajaste
no es el que kernel.org firmó.

**Para volver a establecerlo tú mismo**, o después de cambiar `KVERSION`:

```sh
make -C image pin-kernel
```

Imprime cuatro comandos en vez de correrlos, a propósito: un objetivo que bajara
el tarball y anotara su propio hash parecería verificación y no lo sería —
demostraría que el archivo no cambió entre dos lecturas, que es lo que nadie
temía. Lo que establece algo es la firma, y comprobar una firma significa que
**tú** decides de quién es la llave. Compara la huella que imprime.

### Paso 1 — construye y arranca

```sh
make -C image              # kernel, programa, imagen. El kernel es lo largo
make -C image store-stage  # lo que va en el disco de la máquina
sudo make -C image store   # formatearlo. El único comando que necesita root
make -C image run          # arrancar
```

`sudo` aparece exactamente una vez, y solo para formatear una imagen de disco
con Btrfs y copiar archivos adentro. Nada más pide contraseña, y `make run` no
debe pedirla — un arranque que necesitara root pondría a QEMU y a todo lo de
adentro bajo root sin ninguna razón.

Antes de seguir, mira lo que construiste:

```sh
make -C image count
```

Lista lo que hay dentro de la imagen. La respuesta es el kernel de Linux y
**un** programa. Ésa es la afirmación fundacional del proyecto, y es contable en
vez de citable — si alguna vez dice dos, la afirmación está rota.

La máquina arranca, dice lo que tiene y lo que no, y espera. **No hay login**,
porque no hay nadie más que puedas ser. **No hay shell**: lo que no es una
palabra que la sesión conozca, no existe.

### Pasos 2 al 6 — en el prompt de la propia máquina

Escríbelos de uno en uno y lee lo que contesta. Lo importante no son los
comandos; es lo que demuestra cada uno.

```
> disponibles
```

Lo que tiene el repositorio local. El disco viaja con un módulo firmado —el
`greeter`— **deliberadamente sin instalar**. Una máquina que arrancara con él ya
puesto volvería imposible de hacer el paso siguiente.

```
> instalar dev.thalyx.greeter
```

Thalyx verifica la firma contra la llave del publicador, recalcula él mismo el
digest del artefacto en vez de creerle al manifiesto, y entonces **se detiene y
te pregunta**. Lo que ves lo dibuja Thalyx, dentro de un marco, listando todos
los permisos que el módulo va a tener:

```
┌─ Thalyx — capability authorisation ──────────────────
│ Greeter (dev.thalyx.greeter)
│ version 1.0.0
│
│ This module permanently requests:
│   · read access to /opt/thalyx/data/greeter/notes.txt
│
│ These permissions come from the module's signed manifest.
│ They stay in force until you revoke them by hand.
└──────────────────────────────────────────────────────
Confirm? [y/N]
```

**Ese marco es un mecanismo de seguridad, no un adorno.** Lo genera el núcleo a
partir del manifiesto firmado — el agente no lo puede componer, no lo puede
reformular, y no te puede mostrar una parte de lo que se está pidiendo. Prueba
primero contestando `n`: no se instala nada, y tampoco se recuerda nada.

Después instálalo de verdad, y mira qué concediste:

```
> permisos
> modulos
```

Ahora córrelo:

```
> correr dev.thalyx.greeter
```

El módulo le pregunta a Thalyx quién es —no sabe su propio nombre—, lee el único
archivo que le concedieron, y le niegan `/etc/shadow`, que no le concedieron.
Todo lo que te dice llega **a través de Thalyx**, etiquetado, porque un módulo
no tiene terminal propia. No puede escribir en tu pantalla.

Deshaz la instalación y pregúntale a la máquina qué va a seguir sabiendo
después:

```
> revertir
> recuerdos
> apagar
```

Ahora arráncala otra vez y vuelve a preguntar:

```sh
make -C image run
```

```
> recuerdos
```

Te dice qué le pediste, **y que la instalación que hizo ya no le cuadra** — sin
que nadie le haya avisado de que el módulo se fue. Ésa es la diferencia entre
una memoria y una bitácora: fue y miró. Un registro que solo repitiera lo que le
dijeron seguiría afirmando que la instalación sigue en pie.

Esos seis pasos eran el criterio de salida completo de la Fase 1
(`vault/07-Adopcion-y-Fases/Criterio-de-Salida-Fase-1.md`): no una lista de
componentes, sino una persona ajena al proyecto haciendo exactamente esto, desde
este archivo, sin que nadie la ayude. Esa última parte quedó suspendida el
2026-08-06. Los pasos siguen teniendo que funcionar, y se comprueban en cada
cambio y en cada corrida de hardware; lo que ya no se exige por ahora es que los
haga alguien de fuera.

### Si algo falla

Lee lo que imprimió antes de suponer lo peor. La máquina está construida para
distinguir **«no está»** de **«no pude mirar»**, y dice cuál de las dos pasó.
`NOT PROVEN` nunca significa lo mismo que una prueba superada.

- **`make run` dice que no hay nada que arrancar** — `make -C image` no terminó.
- **`make run` dice que no hay disco** — te saltaste `store-stage` o `store`.
- **El prompt dice que todavía nada aplica un permiso** — el kernel arrancó sin
  el LSM de BPF. `correr` se niega en vez de ejecutar un módulo con nada que
  aplique sus permisos, y eso es deliberado: un módulo sin confinar se comporta
  exactamente igual que uno confinado, hasta el momento en que hace algo que no
  debería haber podido hacer.
- **Cualquier otra cosa** — `estado` vuelve a leer la máquina, `nucleo` muestra
  lo que ha estado diciendo el kernel. Ahí adentro no hay `dmesg`; así es como se
  mira.

---

## Qué es cierto ahora mismo

Cada afirmación de esta sección o se puede comprobar con un comando de este
repositorio, o está marcada como todavía no comprobada. Esa distinción es la
regla de trabajo principal del proyecto.

**Construido y cubierto por pruebas: 710.** Entre pruebas unitarias, inyección
de fallos que mata el binario real en cada punto del commit atómico, y corridas
de punta a punta del criterio de salida. `cargo test --workspace` corre todo.

**Verificado en hardware real**: 104 comprobaciones, en una máquina con LSM de
BPF, cgroup v2 y Btrfs — `sudo ./dev/verify.sh`, el 2026-08-06. No falló nada.
Desde entonces hay una cosa construida y **sin ejercer en hardware**: Thalyx
escribe su propio Btrfs sin `mkfs.btrfs`, y nadie lo ha montado todavía. `btrfs
check` lo acepta, y `btrfs check` no es un montaje.
Una sola quedó sin probar, y es algo que **todavía no existe** en vez de una
comprobación que no se pudo hacer: el agente no tiene modelo. El LSM de BPF ha
denegado una conexión de red real a un proceso que no tenía el permiso, y solo a
ese proceso.

**La imagen arranca, y hace los seis pasos sola.** Un kernel construido desde
`allnoconfig` arranca en QEMU con un solo programa adentro, engancha su propio
enforcement sin `bpftool` y sin un segundo archivo, monta su store de Btrfs,
instala un módulo firmado desde su propio repositorio por el camino confiable,
lo corre confinado, lo revierte, se apaga sola — y en el siguiente arranque dice
qué se le pidió y que la instalación que hizo ya no le cuadra. Todo eso es la
etapa 16 de `verify.sh`, tecleada en una máquina real desde un arranque frío.

### Lo que todavía no es cierto, dicho sin rodeos

- **No hay instalador, y eso es lo que ahora cierra la Fase 1.** El criterio,
  fijado el 2026-08-06: un solo archivo que, puesto en una PC sin sistema
  operativo, la deje corriendo Thalyx. La mitad está hecha — el 2026-08-06 un
  firmware UEFI arrancó Thalyx desde un medio con **un archivo**, sin gestor de
  arranque de ninguna clase, y todo lo que la máquina hace funcionó adentro. Lo
  que falta es lo que la hace quedarse: Thalyx todavía no sabe crear su propio
  store, ni escribirse en un disco, y su consola es un puerto serie que una PC
  de verdad no tiene. `make -C image run-uefi` la arranca; no guarda nada.
- **Nadie ajeno al proyecto ha hecho los seis pasos**, y eso ya no es el criterio
  de salida — se suspendió el mismo día, a favor de la ISO. Los pasos siguen
  teniendo que funcionar y se comprueban en cada cambio; lo suspendido es
  **quién los teclea**.
- **El agente conversacional no tiene modelo.** La mitad determinista está
  construida y funciona; no hay un modelo de lenguaje detrás. La sesión dice *"I
  have no model loaded"* en vez de aparentar. La elección de modelo está
  decretada (`vault/03-Primitivas/Gamas-de-Modelo.md`) y no implementada.
- **El planificador predictivo es de Fase 2.** Es diseño, no código.
- **`thalyx_watch` nunca se ha cargado sin `bpftool`.** El cargador de BPF que
  Thalyx lleva adentro está probado sobre el objeto del LSM: lo carga, lo
  engancha, y ese enforcement deniega. El watcher son diez hooks en lugar de dos
  y no se ha intentado. Probable no es comprobado.

### Tres límites del enforcement, dichos en vez de descubiertos

- **El LSM aplica por clase de acción, no por ruta.** Cada permiso de lectura
  sobre una ruta absoluta se vuelve un bit `FS_READ` y cada uno de escritura un
  bit `FS_WRITE`, y el programa BPF comprueba el bit. Lo que confina a un módulo
  a las rutas *particulares* que le concedieron es el sistema de archivos raíz
  —que no contiene nada más—, el uid propio del módulo, y las comprobaciones de
  la API interna de Thalyx, que abren bajo un descriptor del directorio
  concedido con el kernel negándose a salir de él. El LSM es una segunda capa,
  más gruesa. Llamarlo enforcement por ruta sería afirmar más de lo que hace.
- **`net/outbound` no se ha ejercido de punta a punta en hardware.** Que el LSM
  deniegue una conexión a un módulo *sin* la concesión está demostrado y es
  reproducible (`make -C lsm demo`). Que un módulo *con* la concesión abra una
  conexión está implementado y cubierto por pruebas unitarias, y todavía no ha
  corrido en una máquina.
- **Los snapshots necesitan `btrfs-progs`, que la imagen no tiene ni puede
  tener.** `thalyx-snapshot` invoca el comando `btrfs`, así que snapshot y
  restore funcionan en un anfitrión que lo tenga —donde `dev/verify.sh` los
  ejercita— y no dentro de la imagen mínima, que lleva un solo programa.

### Y una contradicción, porque publicarla es lo honesto

El decreto fundacional dice que los módulos se comunican con Thalyx
**exclusivamente** por su API, no por POSIX y no por libc. Hoy un módulo es un
binario de Linux enlazado dinámicamente: el sandbox monta `/usr`, `/lib`, `/bin`
y `/etc` de solo lectura para que pueda siquiera arrancar, y el filtro de
seccomp permite alrededor de 120 llamadas al sistema.

La distinción que sí se sostiene, y que es la que el código implementa de
verdad:

> **La API de Thalyx es la única superficie *mediada*.** No es la única
> superficie alcanzable.

La identidad, los permisos, los archivos concedidos y hablarle al humano existen
a través de ella y de ningún otro lado. Lo que queda alcanzable por POSIX está
acotado por tres capas que sí existen: un sistema de archivos raíz que no
contiene nada que no se haya montado dentro, un filtro que mata lo que no está
en su lista, y el LSM. Nada de eso convierte al módulo en un programa que no
habla POSIX. Lo que hace es que hablar POSIX no lo lleve a ningún lado que el
humano no haya autorizado.

Cerrar la brecha del todo —módulos estáticos, sin libc, un filtro mucho más
chico— es una decisión de Fase 2 registrada en
`vault/02-Arquitectura/Sistema-de-Modulos.md`.

---

## Pruébalo como programa, en vez de como máquina

Todo lo de abajo corre Thalyx encima del Linux que ya tienes. Eso es un **banco
de pruebas y no una forma de usar Thalyx** —la bóveda lo llama andamio y no
destino— y el propio programa te lo va a decir: arrancado desde una terminal, la
sesión dice *this is not the machine*, porque lee a su propio proceso padre para
averiguarlo en vez de que se lo digan.

Está aquí porque es como se verifica el sistema.

```sh
cargo build

# Lado del publicador: una llave y un paquete firmado
./target/debug/thalyx dev keygen --out publisher.key
./target/debug/thalyx dev pack ./payload \
    --manifest manifest.toml --key publisher.key --out demo.thmod

# Lado del usuario
export THALYX_ROOT=/tmp/thalyx-demo
./target/debug/thalyx module install demo.thmod
./target/debug/thalyx module list
./target/debug/thalyx permissions
./target/debug/thalyx journal

# Deshacer la instalación. Estrecho y barato: recupera solo lo que Thalyx publicó.
./target/debug/thalyx rollback --dry-run
./target/debug/thalyx rollback
```

Mira al commit atómico sobrevivir a que lo maten en su instante más peligroso —
entre el renombrado del directorio y el cambio del enlace simbólico:

```sh
THALYX_FAULT_POINT=mid-commit ./target/debug/thalyx module install demo.thmod --yes
# el proceso muere con SIGABRT: sin desenrollado, sin limpieza, sin oportunidad de ordenar

./target/debug/thalyx module list     # no está instalado
./target/debug/thalyx store status    # un huérfano inerte, almacén consistente
./target/debug/thalyx module install demo.thmod   # el reintento funciona
```

El índice semántico, y dejar que el kernel le diga cuándo sigue vigente:

```sh
thalyx graph build ./crates
thalyx graph dependents thalyx-core/src/commit.rs
thalyx graph watcher                    # qué alcanza a ver el vigilante del kernel
thalyx graph trust ./crates --counter   # ganarse el camino rápido, o que se lo nieguen
```

Lo que el agente va a recordar entre sesiones, manejado a mano hasta que exista:

```sh
thalyx memory remember refactor "moved login() to auth.rs" --about src/auth.rs
thalyx memory recall refactor           # vuelto a comprobar contra los archivos, ahora
```

Snapshots, y el comando destructivo que regresa a uno:

```sh
thalyx snapshot take ~/work --label before-upgrade
thalyx snapshot list ~/work
thalyx restore <name> ~/work            # muestra qué destruiría, y luego pregunta
```

---

## Verificarlo en una máquina real

Casi nada de lo que Thalyx afirma se puede comprobar dentro de un contenedor. El
LSM de BPF necesita un kernel con `bpf` en su orden de LSM, los límites de
recursos necesitan controladores de cgroup delegados, y «el enforcement es real»
significa que una conexión se deniega de verdad.

```sh
sudo ./dev/verify.sh
```

Nunca cuenta como aprobada una comprobación que no pudo hacer. Todo lo que la
máquina no puede hacer se reporta como `NOT PROVEN`, con el motivo, y se vuelve
a listar en el resumen — porque una corrida en verde que no ejercitó nada se ve
idéntica a una que lo ejercitó todo.

No deja nada cargado: el LSM se desengancha a la salida, incluso con Ctrl-C.

---

## Las cuatro primitivas base

| Primitiva | Qué hace | Dónde vive |
|---|---|---|
| Sistema de archivos en grafo | Consultar archivos por relaciones semánticas en vez de por rutas | Espacio de usuario (SQLite) |
| Permisos just-in-time | El agente pide acceso temporal; el SO lo otorga y lo revoca | Kernel (`thalyx-lsm`) + broker en espacio de usuario |
| Memoria persistente | El estado de una tarea sobrevive a sesiones y reinicios | Espacio de usuario |
| Planificador predictivo | Ajustar la prioridad de un proceso según el contexto | Espacio de usuario (Fase 2) |

---

## Estructura del repositorio

```
crates/
  thalyx-manifest/  parseo de .thmod, validación, firmas ed25519
  thalyx-contract/  contratos estructurados con procedencia por campo
  thalyx-parser/    parser mecánico: Rust, Python, JS/TS, C, Go
  thalyx-graph/     el índice semántico, y la disciplina sobre su frescura
  thalyx-watch/     lee el contador de mutaciones del kernel
  thalyx-memory/    memoria persistente, con su propio almacén vectorial
  thalyx-permd/     permisos traducidos a política del kernel
  thalyx-sandbox/   namespaces, seccomp, pivot_root, mounts idmapped, cgroups
  thalyx-syscall/   el único crate donde se permite `unsafe`
  thalyx-snapshot/  subvolúmenes y snapshots de Btrfs
  thalyx-journal/   journal de operaciones, solo-append
  thalyx-core/      verificación, staging, commit atómico, permisos, rollback
  thalyx-cli/       el binario `thalyx`
  thalyx-bpf/       el cargador de BPF propio: sin libbpf, sin bpftool

lsm/            programas LSM de BPF: enforcement, y el vigilante del filesystem
image/          la máquina: configuración del kernel, initramfs, disco del store
dev/            verify.sh — un comando que comprueba cada afirmación en hardware
modules/        dev.thalyx.greeter, el primer módulo escrito contra la API

vault/          Bóveda de diseño (Obsidian, en español)
```

La bóveda es la autoridad: el código implementa decretos, no los inventa.
Empieza en `vault/00-Indice/Indice-Principal.md` para el orden de lectura.
`vault/06-Pendientes/Punto-Actual.md` dice dónde está el proyecto ahora mismo y
cuál es el siguiente paso; se actualiza cada vez que se termina algo.

La bóveda está escrita en español. Todo lo demás —código, esquemas,
identificadores, mensajes de commit, salida de la CLI— está en inglés.

---

## Hoja de ruta

- **Fase 1** — La máquina y lo que corre en ella: núcleo, LSM, broker de
  permisos, índice semántico, gestor de módulos, sandbox, agente local, CLI,
  snapshots de Btrfs. Termina cuando alguien de fuera puede arrancarlo,
  instalar, revertir y reiniciar sin ayuda — ver **Arráncalo** arriba, que es
  esa lista de pasos y nada más.
- **Fase 2** — Validación empírica. Los benchmarks deciden si las primitivas se
  mueven al kernel.
- **Fase 3** — Migración al kernel, si los números lo justifican.
- **Fase 4** — Ecosistema.

## Licencia

GPLv3 para los componentes de espacio de usuario. GPLv2 para todo lo que se
enlace contra el kernel de Linux, que es GPLv2 únicamente — ver
`vault/05-Decisiones-y-Debates/Decision-Licencia.md`.
