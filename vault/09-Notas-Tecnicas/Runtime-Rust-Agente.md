---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-31
tags: [rust, runtime, agente, store, artefacto, semantica, no-negociable]
---

# El runtime Rust del agente

> Esta nota existe por una corrida pagada. El 2026-08-30, dentro de la máquina,
> Claude eligió **exactamente la primitiva correcta** —preguntar qué es un
> símbolo y luego renombrarlo— y Thalyx le contestó que no había compilador.
> Todo lo demás de esa traza es consecuencia de eso.

## Lo que pasó

En `/var/tmp/thalyx-bench-compact-1/armB.ndjson`, la primera estrategia del
modelo fue la buena:

```js
const def = thalyx.context('…');
const r1  = thalyx.rename('…', '…');
```

y la máquina respondió:

```
context:  source: index      resolution: matched   analyzer_starts: 0
rename:   ok: false          error: unresolved
          message: there is no `cargo` on this machine
```

El preflight había dicho `READY`. Y tenía razón en lo que preguntaba: la máquina
estaba viva y sostenía el árbol correcto. Ninguna de las dos cosas es la
capacidad que la tarea necesitaba, y no había nada entre el dinero y enterarse.

## El decreto

**El anfitrión no aporta ni un archivo del entorno de programación mientras el
agente trabaja.** Ni `cargo`, ni `rustc`, ni `rust-analyzer`, ni el servidor de
proc-macros, ni la biblioteca estándar, ni sus fuentes, ni `librustc_driver`, ni
LLVM, ni el cargador dinámico.

El primer intento de arreglo fue copiar el `~/.rustup` de Fedora al store. César
lo detuvo, y tenía razón: [[Filosofia-Fundacional]] dice que Thalyx **es** el
sistema, así que una cara de programación que sólo funciona porque el anfitrión
tiene rustup instalado es una cara que le pertenece al anfitrión. Apagar Fedora,
mover el disco a otra x86_64 y arrancar no puede hacer desaparecer el proveedor
semántico.

Criterio, en una frase: **el artefacto se lleva consigo todo lo que sus propios
programas nombran.**

## Dónde vive

`/opt/thalyx/toolchains/rust/rust-<versión>-<target>/`, o sea el subvolumen
`system` del store.

**Nunca el initramfs.** [[Construccion-del-ISO]] dice que la imagen es el kernel
de Linux y un programa, y lo dice de forma contable a propósito. Seiscientos
megabytes de compilador son *software instalado sobre Thalyx*, que es justo la
distinción entre lo que Thalyx **es** y lo que le han puesto encima — la misma
razón por la que el motor y los pesos viven en el store y no en la imagen.

Hay una prueba que lo sostiene, no una promesa: `the_rust_runtime_is_not_in_the_image`
arma el archivo y cuenta lo que hay dentro.

## De qué está hecho, y por qué de eso

### Se eligió el toolchain musl, y fue una medición

Los dos caminos, medidos el 2026-08-31 antes de escribir una línea:

| | lo que le falta al artefacto |
|---|---|
| `x86_64-unknown-linux-gnu` | cargador de glibc, `libc`, `libm`, `libdl`, `librt`, `libpthread`, `libgcc_s`, `libz` — y además un `libLLVM.so` aparte de 191 MB |
| `x86_64-unknown-linux-musl` | **dos archivos**: el cargador de musl y `libgcc_s.so.1`. LLVM va dentro de `librustc_driver`, así que no hay `libLLVM` |

Rust publica herramientas de anfitrión para `x86_64-unknown-linux-musl` de forma
oficial, con un sha256 por archivo en su propio manifiesto de canal. Dos
archivos que faltan es un problema que una persona cierra; media distribución no
lo es. Ésa es toda la razón de la elección.

Es además la convención que este repositorio ya tenía: `dev/build-engine.sh` se
niega a terminar si el motor no es un ELF sin `INTERP` y sin `NEEDED`, porque
**dentro de la máquina no hay libc**. El toolchain es el primer programa que no
puede ser estático, así que se trae su propio mundo.

### El cargador se compila, no se copia

`libc.so` de musl, construido desde el tarball de release de musl con su digest
fijado. El mismo arreglo que ya tiene el kernel de Linux en `image/Makefile`: un
tarball fijado, comprobado antes de creerle, compilado por nosotros. Es un
megabyte y tarda segundos.

Bajo musl, `libc.so` **es** también el enlazador dinámico, así que ese único
archivo cubre el `PT_INTERP` y el `DT_NEEDED libc.so` de todos los binarios.

### `libgcc_s.so.1` sale del propio Rust

Se enlaza desde el `libunwind.a` que Rust envía dentro de `rust-std`, en
`self-contained/`. No entra ninguna fuente nueva: viene del mismo artefacto
verificado que el compilador.

Que alcance es una medición, no una esperanza. De los 883 símbolos indefinidos
de `cargo`, `rustc`, `rust-analyzer`, el servidor de proc-macros y
`librustc_driver`, todo resuelve contra musl y `librustc_driver` salvo 29 — y de
esos 29 los únicos que **no** son débiles son los quince `_Unwind_*`, que
`libunwind.a` define todos. El resto son los stubs de memoria transaccional,
`__register_frame_info`, y los dos `pidfd_*` que la std de Rust enlaza débilmente
para musl más nuevos.

Después se corrió en vez de argumentarse: `rustc` desenrolló un error fatal y
salió con 1 limpiamente, que es exactamente el `catch_unwind` que rustc usa para
su propio `FatalError`.

## Lo que deliberadamente **no** lleva

| fuera | por qué |
|---|---|
| `share/`, páginas de manual, documentación | no se ejecuta nunca, y son 13 MB sólo del componente `rustc` |
| `lib/rustlib/<target>/bin/` | 174 MB de **enlazadores** — `rust-lld`, `wasm-component-ld`, `rust-objcopy`. El proveedor semántico nunca enlaza: ni `cargo metadata` ni el análisis de rust-analyzer lo hacen |
| cualquier otro target | un anfitrión, un objetivo |
| `rustdoc`, `rustfmt`, `clippy` | no es para lo que sirve un proveedor semántico |

Dejar fuera los enlazadores es además lo honesto: `rust-lld` pide los builtins
enteros de libgcc (`__popcountdi2` y compañía), que el `libgcc_s` de sólo
unwinder no tiene. **Enviar un enlazador que no arranca es peor que no enviar
ninguno.**

Así que este artefacto **resuelve y renombra; no compila**. El día que algo
dentro de Thalyx necesite compilar una proc-macro o un build script, ésa es la
siguiente causa conocida y le toca su propio cambio con su propia evidencia. Una
causa a la vez.

## `rust-src` es obligatorio, y se supo por medición

Sin `lib/rustlib/src`, rust-analyzer escribe `can't load standard library, try
installing rust-src` y se muere a media primera pasada de análisis. Está en la
lista de archivos requeridos por eso, no por prolijidad.

## El detalle de musl que costó una tarde

Los binarios llevan `RPATH: [$ORIGIN/../lib]`, y **musl resuelve `$ORIGIN` del
programa principal leyendo `/proc/self/exe`**. El mismo binario, mismo artefacto:
con `/proc` montado arranca; sin `/proc` muere con

```
Error loading shared library librustc_driver-<hash>.so: No such file or directory
```

Que es un `execve` que parece un archivo que falta y no lo es.

Hay dos formas de cerrarlo y Thalyx usa las dos: el sandbox monta `/proc` tras el
pivot, y `toolchain::environment` nombra el directorio en `LD_LIBRARY_PATH`. La
segunda es la que no depende de que quien arranque el proceso se acordara de la
primera.

## El `PATH` que Thalyx le arma a sus hijos

> Segunda evidencia física, 2026-08-31, en la VM real y sobre `dev/rust-corpus`.
> El problema «no hay Cargo dentro de Thalyx» ya estaba cerrado: `cargo 1.90.0
> from: thalyx`, `rust-analyzer 1.90.0 from: thalyx`. Y aun así la máquina no
> resolvía un nombre.

Lo que la máquina contestó:

```
context('lantern/src/lib.rs')  → { name: "LanternRegistry", kind: "struct",
                                   crate: "lantern", source: "rust-analyzer" }
context('LanternRegistry')     → { source: "rust-analyzer",
                                   resolution: "nothing", entries: [] }
rename en lantern/src/lib.rs:8:12
                               → "No references found at position"
```

rust-analyzer estaba vivo, había abierto el archivo y lo había parseado, y no
podía resolver un nombre declarado en el archivo que acababa de listar. La
posición no era el problema: se probó la del identificador físico, 8:12, y
falló igual.

**La causa, reproducida y capturada del log del propio servidor** con un entorno
que tenía exactamente lo que Thalyx entregaba y nada más:

```
ERROR FetchWorkspaceError: rust-analyzer failed to load workspace:
  Failed to run `cargo metadata …`: No such file or directory (os error 2)
WARN  failed to get rustc cfgs e=unable to fetch cfgs via `… "rustc" …`
  Caused by: No such file or directory (os error 2)
```

### Por qué esto no lo tapaba nada

Thalyx encuentra sus herramientas por ruta absoluta, y eso está bien y no
cambia. Pero **rust-analyzer no es un programa que Thalyx corre: es un programa
que corre programas.** Lanza `cargo metadata`, `cargo locate-project`,
`cargo --version` y `rustc --print cfg`, y escribe cada uno como nombre pelado.
Un nombre pelado lo resuelve el kernel por `PATH` y por nada más. La máquina
Thalyx, a propósito, no tiene el `PATH` de una distribución.

Sin grafo de crates no hay semántica; la sintaxis sobrevive porque no necesita
subproceso. De ahí la forma exacta de la falla: el *outline* de un archivo
funciona y todos los nombres resuelven a nada.

### El arreglo, y por qué no es el préstamo que el decreto prohíbe

`toolchain::environment` le entrega a los hijos del toolchain administrado
cuatro variables, y esto es lo que hace cada una:

| variable | por qué existe |
|---|---|
| `PATH=<runtime>/bin` | es lo único a través de lo cual rust-analyzer puede encontrar **su** `cargo` y **su** `rustc`. Medido: agregarla —y nada más— vuelve correctas `workspace/symbol`, `definition` y `rename` en el mismo fixture donde las tres estaban vacías |
| `LD_LIBRARY_PATH=<runtime>/lib` | el detalle de musl de la sección anterior |
| `CARGO_HOME=<store>/state/cargo` | el registro es de Thalyx y está en el store, nunca el de quien armó el disco |
| `CARGO_NET_OFFLINE=true` | el proveedor semántico no tiene red por construcción, y un Cargo que no lo sabe se gasta su timeout en enterarse |

**No** se pusieron `CARGO`, `RUSTC` ni `RUST_SRC_PATH`. `PATH` sola alcanzó en la
medición, y una variable agregada porque «Rust normalmente la usa» es una
variable que después nadie se atreve a quitar.

La distinción que importa:

- **mal**: Thalyx busca sus herramientas en el `PATH` que heredó de Fedora;
- **bien**: Thalyx descubre su runtime por ruta absoluta y le arma a sus hijos
  un `PATH` construido por Thalyx que contiene únicamente su propio `bin`.

Ese `PATH` tiene **una** entrada. No hereda nada: ni el `PATH` de quien arrancó,
ni `/usr/bin`, ni `~/.cargo`, ni `~/.rustup`. Nombra un directorio que el
confinamiento ya concede de todos modos, porque está dentro del artefacto. Se
mueve el disco y el valor se mueve con él, que es la propiedad entera.

Un toolchain **instalado** es la otra máquina y recibe la otra respuesta: su
`bin` va **adelante** del `PATH` heredado en vez de reemplazarlo, porque en esa
rama el toolchain es del anfitrión por definición y su Cargo necesita el
enlazador del anfitrión. Tampoco es adorno: `dev/verify.sh` corre bajo `sudo`, y
el `secure_path` de sudo no nombra ningún `~/.rustup/toolchains/*/bin` — así que
en la máquina que verifica todo esto el `cargo` de rust-analyzer estaba tan
inalcanzable como dentro de Thalyx.

### Lo que esto no arregla

El confinamiento. La VM reportó `analyzer_confined=false` porque el kernel está
attached pero observando, y el proveedor cayó al proceso ordinario **dentro de
Thalyx** — y la semántica falló igual. Primero la semántica; la misma propiedad
bajo enforcement es la siguiente causa, con su propia evidencia. Una causa a la
vez.

## El outline apuntaba al comentario

Defecto aparte, encontrado en la misma corrida y arreglado con ella:
`context('lantern/src/lib.rs')` ponía `LanternRegistry` en 3:1 cuando el
identificador está en 8:12.

Thalyx pedía `hierarchicalDocumentSymbolSupport: false`, y en esa forma
rust-analyzer contesta `SymbolInformation`, que trae **un solo rango por
entrada** y lo llena con el ítem entero — para una `struct` documentada eso
arranca en el primer `///`. La forma jerárquica trae además `selectionRange`,
que es el identificador. Se pide la jerárquica y se lee `selectionRange`.

No era la causa del bloqueo —8:12 fallaba igual— pero sí era una posición que
un renombrado por handle habría usado. La respuesta real del servidor quedó
guardada en `crates/thalyx-rust/tests/samples/document-symbol-hierarchical.json`,
que es la regla 6 de [[Estrategia-de-Pruebas]]: un fixture inventado prueba que
el lector coincide con la idea que uno tiene del formato.

## Cómo se descubre, y en qué orden

`thalyx-rust::toolchain`:

1. una variable que nombra un archivo (`THALYX_CARGO`, `THALYX_RUST_ANALYZER`);
2. **el runtime de Thalyx en el store**;
3. `RUSTUP_HOME`;
4. el home de quien invocó (`SUDO_USER`), luego `HOME`;
5. `/usr/local/bin`, `/usr/bin`.

El segundo lugar es el decreto: cuando Thalyx lleva compilador, **ése es el
compilador**, y uno instalado en el anfitrión es el respaldo y no al revés. Sólo
una persona nombrando un archivo lo supera.

Y sigue valiendo la regla que ese archivo tiene desde que existe: **un candidato
es la herramienta después de contestar `--version`**, nunca antes. Un runtime a
medio copiar es un directorio de ELF perfectos que no arrancan; la búsqueda cae
al siguiente lugar y lo dice, en vez de entregar algo que muere después.

En una máquina sin store —cualquier portátil, este contenedor, `dev/verify.sh`—
el paso 2 no existe y nada cambió.

## Qué comprueba Thalyx del artefacto, y por qué `ldd` no sirve

`thalyx dev rust-runtime <artefacto>` lee las cabeceras ELF de los programas
staged y pregunta si **cada** biblioteca que nombran está dentro del artefacto,
y si el cargador que le piden al kernel viaja con él.

`ldd` no puede contestar eso: `ldd` arranca el cargador real contra el `/lib`
real, así que dice lo que resolvería *esta* Fedora. Un artefacto al que le falte
una biblioteca que casualmente está en `/usr/lib` del anfitrión se ve **perfecto**
en el anfitrión y es un directorio de ELF muertos dentro de Thalyx.

## El preflight ya no puede decir READY sobre esto

`thalyx-mcp --preflight --needs-rust` le pregunta a la máquina el verbo
`toolchain`, que corre `cargo --version` y `rust-analyzer --version` **dentro** y
lee los manifiestos del workspace. No escribe nada: la lección del 2026-08-29 fue
una sonda que cambió el estado inicial de la corrida que estaba despejando, y
`cargo metadata --no-deps` no resuelve nada, así que no escribe `Cargo.lock`.

`dev/bench-external-agent.sh` lo pide solo cuando el proyecto tiene `Cargo.toml`
en la raíz — derivado del árbol y no de una bandera, porque una bandera que
alguien tiene que acordarse de pasar es la bandera que faltó en la corrida que la
necesitaba, que es 2026-08-30 exactamente.

## Camino de uso

```sh
make -C image rust-runtime                  # una vez: ~170 MB, y compila musl
make -C image agent PROJECT=/ruta/proyecto  # RUST=auto: se activa si hay Cargo.toml
```

`RUST=1` lo fuerza, `RUST=0` lo apaga, `RUSTRUNTIME=<dir>` reusa un artefacto ya
construido. El stage imprime de dónde salió, cuánto copió y dónde queda dentro de
Thalyx, y **se niega** —borrando lo copiado— si el artefacto no pasa la
comprobación de cierre.

## Lo que queda abierto

- **Compilar dentro de la máquina.** Hace falta un enlazador y un `libgcc_s`
  completo. Es la siguiente causa conocida, no ésta.
- **Cache de Cargo.** El proveedor semántico no tiene red por construcción. Un
  workspace con dependencias de registro va a necesitar un `CARGO_HOME`
  aprovisionado y administrado por Thalyx. **No se implementó porque todavía no
  falló por eso**: el workspace sintético de la prueba física no tiene
  dependencias. Una causa conocida cada vez.
- **musl 1.2.4 y no 1.2.5**, porque 1.2.4 es la que se construyó y corrió de
  punta a punta. Regla 12 de [[Estrategia-de-Pruebas]]: lo que se verifica tiene
  que ser lo que se envía. Subirla es una línea y la misma prueba física otra vez.
