---
tipo: nota-tecnica
estado: activo
fecha-actualizacion: 2026-09-12
tags: [plataforma, exp13, thalyx-kernel, linux, estado-administrado, hacer]
---

# Frontera de plataforma

> Sprint 1 de 2 del experimento EXP-13: la misma revisión de Thalyx, real y no
> una recreación, sobre dos backends de Linux, para que el Sprint 2 conecte ese
> mismo Thalyx a Thalyx-Kernel implementando **sólo** el lado de la plataforma.
> Rama `feat/exp13-platform-linux`, sobre `0492f72`.

## Por qué existe

Thalyx-Kernel (`vault/integration/thalyx.md`, INT-001) nombra siete propiedades
que un backend le debe a Thalyx: `WorkControl`, `ObjectAuthority`,
`VersionedState`, `ProgramLaunch`, `MessageTransport`, `MonotonicClock` y
`EvidenceSink`. Y en la misma nota advierte que no deben convertirse en un
`Platform::do_anything`.

Hasta esta rama, `hacer` ([[Ejecucion-Transaccional]], [[Transaccion-Programable]])
era Btrfs, y no sólo en el snapshot y el restore: una validación trataba sobre lo
que el árbol vivo tuviera cuando el verificador miraba, un commit era *soltar un
snapshot*, y la evidencia era un archivo renombrado dentro del store. Un segundo
backend que quisiera la misma transacción sobre versiones inmutables habría tenido
que reescribir `exec.rs`, y reescribir lo que se va a comparar es la forma en que
una comparación entre dos máquinas deja de ser una comparación de máquinas.

Así que **la semántica se quedó donde estaba** —qué hace un paso rechazado, qué
hace un chequeo que falla, qué veredicto decide un commit, qué lleva la
respuesta— y todo lo que esa semántica le pide a la máquina pasa ahora por
`thalyx_platform::Platform`.

## La vertical que atraviesa la frontera

`contexto → agente → programa de hacer en QuickJS → herramientas reales →
validación real → congelar → publicar o abandonar → evidencia y estado durables`.

Lo que **no** está detrás de la frontera, a propósito: los verbos, QuickJS, el
parser, el proveedor semántico y la respuesta. Eso es Thalyx, y un backend que
pudiera cambiarlo sería un segundo Thalyx.

## Las siete propiedades, y en qué se convirtió cada una

| Propiedad | Dónde vive | Qué es |
|---|---|---|
| `VersionedState` | `thalyx_platform::state` | Abrir la frontera, observar qué cambió, **nombrar el candidato** del que trata un veredicto, conservar o abandonar. La que carga el peso. |
| `ProgramLaunch` | `thalyx_platform::launch` | Un programa, sus argumentos y concesiones explícitas; un código de salida y la contabilidad que la máquina misma produjo. |
| `ObjectAuthority` | `thalyx_platform::authority` | Tipos, no un trait: concesiones como objetos con derechos, y publicación por un principal con secuencia. Nada en la vertical le pregunta algo a una autoridad; la entrega. |
| `WorkControl` | `thalyx_platform::work` | Admisión de un efecto contra la obra que lo pidió. Una obra cerrada no lanza procesos ni publica. |
| `MessageTransport` | `thalyx_platform::transport` | El cliente administrado y su servicio sólo intercambian mensajes codificados. |
| `MonotonicClock` | `thalyx_platform::clock` | Una sola fuente de «cuánto tardó». |
| `EvidenceSink` | `thalyx_platform::evidence` | Guardar el registro de una corrida antes de responder, y traerlo por su handle. |

El modelo administrado compartido —protocolo y cliente— está en
`thalyx_platform::managed`: es lo que `linux-managed` usa hoy y lo que el backend
de Thalyx-Kernel debe usar sin cambios.

## Los dos backends de Linux

Se eligen con `THALYX_PLATFORM`, leída una sola vez por el verbo (regla 11). Un
valor que no nombra ningún backend se rechaza, no se sustituye por el de omisión.

### `linux-current` (L0)

Exactamente lo que había: `thalyx_core::attempt` sobre un snapshot Btrfs del
subvolumen donde está la sesión, `run_foreign` para lanzar, la evidencia como
archivo en el store, y ninguna obra con nombre (`Ambient`: todo efecto se
admite, que es lo que Thalyx en Linux siempre hizo). Es el de omisión, y existe
para que cortar la frontera no borre la línea base contra la que se compara todo
lo demás.

### `linux-managed` (L1)

La misma transacción sobre el modelo administrado de Thalyx-Kernel
(`vault/architecture/persistence.md`, ADR-003):

- **Un servicio, un escritor.** `thalyx_managed::LinuxStore`: objetos inmutables
  nombrados por digest, un log de registros encadenados por digest con `fsync`
  en cada uno, `PREPARE` y `COMMIT` durables con el recibo que decidió la
  publicación, secuencias por principal (un reintento se contesta desde el log,
  la misma secuencia con otra petición es `conflict`), y una recuperación que
  resuelve como abortado un `PREPARE` sin `COMMIT` antes de admitir nada más.
  Una cola rota se corta; un daño antes del final rechaza el store entero.
- **Tres árboles.** La raíz publicada vive sólo en el store. La **vista** es
  donde está la sesión: una copia de la raíz publicada que se verifica antes de
  abrir y antes de publicar —una vista con algo que el store no publicó se
  **rechaza, no se sobrescribe**—. La **obra privada** es donde actúan las
  peticiones del programa; nada publicado la ve, y abandonar la regresa a su
  generación.
- **El candidato.** La identidad de contenido (`c1-…`,
  `thalyx_platform::content`) del árbol privado congelado, con todos sus objetos
  en el store. Un veredicto nombra uno; una publicación publica uno.
- **La obra tiene cerca** (`Scoped`): cerrada, no lanza ni publica.
- **Evidencia** como objeto que el log nombra; sobrevive al abandono.

El lanzador sigue siendo `run_foreign`, porque lo que confina un proceso en Linux
es Linux; lo administrado es lo que se le entrega y de qué trata su veredicto.

**Lo que su perfil declara, sin redondear:** `managed_local_v1.holds = false`.
Raíces inmutables, CAS durable y cierre definido sí; pero nada en Linux impide que
otro proceso del mismo usuario escriba `objects/`. El store vuelve a calcular el
digest de todo lo que entrega y rechaza lo que no coincide: eso es *detectar*, y
el perfil lo dice (`exclusive_store_writer: detected_by_digest_not_prevented`).
Prevenirlo es para lo que sirve un kernel que es dueño del medio.

## Decisiones que la construcción tomó y que antes no existían

Escritas aquí porque una decisión que no está en el vault no se ha tomado.

1. **Un veredicto trata sobre un candidato.** En L1, el árbol se congela antes y
   después de cada chequeo y el veredicto se ata al candidato sólo si los dos
   coinciden (la política de `thalyx_rust::affected::steady`: un `cargo check`
   sin `Cargo.lock` escribe uno, así que se repite una vez cuando lo que se movió
   lo escribió un proceso lanzado). Al decidir, un veredicto sobre otra versión
   no autoriza publicar la actual. En L0 no hay candidato y la regla es vacía, que
   es lo que mantiene L0 idéntico. **Es una diferencia declarada**: el caso
   `a-verdict-about-an-older-tree` —un chequeo que se sostiene y después una
   edición que deshace lo chequeado— se conserva en L0 y no se publica en L1.
2. **La autoridad de la sesión se traduce al objeto de la obra, nunca se
   ensancha.** En L1 un programa de una sesión confinada queda confinado a su
   copia privada: puede leer `a.rs` y no la ruta absoluta de la vista. En L0 esa
   ruta es el propio árbol y está dentro. Declarado y probado con columna de
   control (`a_confined_program_on_the_managed_model_reaches_its_private_copy_and_not_the_tree`).
3. **Una obra cerrada no conserva nada.** Si la admisión de publicar se rechaza,
   la obra se abandona y la razón lo dice. En L0 nunca ocurre.
4. **Una vista que quedó atrás se pone al día sólo si contiene exactamente una
   raíz que el store publicó.** Es lo que deja un commit durable cuya respuesta se
   perdió; ponerla al día no le quita nada a nadie. Cualquier otra diferencia se
   rechaza.
5. **`intento` sigue siendo sólo L0.** No se portó: está fuera de la vertical
   medida. En L1, `hacer` no ve un intento abierto con `intento`.

## La instrumentación común

`thalyx_platform::trace`, esquema `thalyx-trace-v1`, dentro de la evidencia bajo
`platform.trace` y **nunca** en la respuesta al modelo. Las fases las toma la
transacción en las llamadas que hace a la plataforma, con el reloj del backend, y
un backend nunca escribe un span: `open`, `program`, `request`, `observe`,
`validate`, `candidate`, `launch`, `settle`, `end`, con totales por fase exactos
aunque la lista de spans se acote. Es lo que EXP-13 compara entre L0, L1 y K1
sin inventar un cronómetro por lado.

## El corpus de equivalencia

- `dev/exp13/corpus/*.json`: **sólo entradas** —un árbol, lo que pregunta el
  agente y, para L1, las diferencias que el diseño tiene, cada una con su
  razón—. Ningún caso dice cuál es la respuesta.
- `dev/exp13/baseline/*.json`: lo que contestó el binario de `0492f72` sin
  modificar, grabado dos veces y rechazado si las dos corridas difieren.
- `crates/thalyx-cli/tests/exp13_equivalence.rs`: un proceso `thalyx bridge` por
  caso, una sesión confinada por su socket, y un agente con guion que pregunta
  qué es un nombre, lee la respuesta y manda el programa que esa respuesta
  decide. El digest del árbol lo toma el test recorriendo el directorio (regla 2).

Dos afirmaciones: **L0 contesta exactamente lo que contestó `0492f72`** —cada
respuesta, cada evidencia, cada métrica que no es un reloj, el journal y los
bytes del árbol— sobre un subvolumen real; y **L1 significa lo mismo que L0**,
distinto sólo donde un caso lo declara, y cada declaración tiene que ocurrir.

La etapa 62 de `dev/verify.sh` construye `0492f72` desde git y corre el corpus
con las dos cosas vivas en la máquina de Cesar.

## Sprint 2: K1, `thalyx-kernel-managed`, implementado

El tercer backend existe y se elige con `THALYX_PLATFORM=thalyx-kernel-managed`.
Se implementó **sin tocar la semántica de `exec.rs`**: el único cambio en
`exec.rs` es una rama de despacho —la selección del árbol trata a
`thalyx-kernel-managed` igual que a `linux-managed`, porque los dos necesitan un
directorio y no un subvolumen—. No cambió qué hace un paso rechazado, qué decide
un commit ni qué autoriza un rollback. La frontera estaba bien cortada.

Lo que K1 **es**, propiedad por propiedad:

- `VersionedState` y `EvidenceSink`: **el mismo
  `thalyx_platform::managed::client::Managed`, sin cambios**, sobre un
  `Transport` nuevo —`ConsoleTransport` en `crates/thalyx-cli/src/platform.rs`—
  que enmarca cada mensaje administrado (cuatro bytes de longitud y el cuerpo
  JSON, la gramática de `thalyx-bridge`) y lo escribe en un puerto
  virtio-console del kernel corriendo. Al otro lado está el servicio de estado
  de K4 sin cambios, sobre el driver de bloque de K4, sobre un medio real. El
  cliente no distingue este almacén del de `linux-managed` sobre loopback: esa
  es toda la afirmación de K1 —el mismo Thalyx, la misma frontera, otra máquina—.
- `WorkControl`: **un ámbito del kernel por transacción**. El dominio *link*
  (`thalyx-kernel/user/k5link`) abre un ámbito hijo cuando un trabajo hace
  `fork` y deriva bajo la vida de ese ámbito el grant por el que la publicación
  viaja; cercar el ámbito hace que el kernel rechace ese grant en su siguiente
  uso —comprobado por el kernel, no por un programa—. El coordinador host-side
  (`Scoped`) es idéntico al de L1; la cerca real la impone el kernel y es lo que
  la puerta adversaria de cancelación ejercita.
- `MessageTransport`: un canal de kernel real —una función virtio-console que el
  kernel asignó y que el link conduce desde usuario, `user/k5link/src/virtio.rs`,
  el mismo transporte moderno que K3 levantó para bloques—.
- `MonotonicClock` y `ProgramLaunch`: **host-side, y declarados como tales**. La
  transacción de la revisión real de Thalyx corre en el host —`exec.rs`, el
  reloj del host, `run_foreign` para lanzar la validación—, exactamente como en
  L1. `Check::Rust` compila con `cargo` y QuickJS ejecuta el programa, y ninguno
  corre bajo este kernel; K5 demostró una herramienta nativa sobre un candidato
  sellado, pero no `Check::Rust`, y este brazo **no** reclama ejecución nativa
  para la validación. El perfil de K1 lo dice: `launch =
  linux_confined_process_host_side`.

**La garantía más fuerte, declarada.** El perfil de `linux-managed` dice
`exclusive_store_writer: detected_by_digest_not_prevented` y `holds = false`,
porque nada en Linux impide que otro proceso del usuario escriba `objects/`. El
perfil de K1 dice `exclusive_store_writer: prevented_kernel_owns_the_medium` y
`holds = true`: no hay `objects/` en un sistema de archivos, hay un medio del
que sólo el servicio de estado —el único dominio al que el kernel dio el
dispositivo— puede escribir. Prevenirlo es para lo que sirve un kernel dueño del
medio, y es la única diferencia de garantía que K1 declara sobre L1.

Cómo aterriza el modelo de Thalyx sobre el de K4: Thalyx tiene *líneas*
(secuencias de generaciones con identidad de contenido `c1-…`), K4 publica una
*raíz* (un árbol de a lo más doce nombres). El link guarda el estado de cada
línea dentro de una sola raíz de K4 —un árbol `O` de todos los objetos, un
índice `X`, y por línea su historia `H`, su evidencia `E` y su recibo `R`—, y un
objeto mayor que el techo de K4 viaja como un árbol de trozos. Una publicación de
Thalyx es una publicación de K4 con CAS sobre la generación de la raíz, decidida
además contra la generación de la línea; un corte entre `PREPARE` y `COMMIT` lo
resuelve la recuperación de K4 como en su matriz. Detalle en
`user/k5link/src/managed.rs` y en la evidencia del kernel `exp13-k1-final.md`.

Qué corre dónde, sin redondear: el servicio de estado, el driver, el medio, los
ámbitos de trabajo y el transporte son del kernel; el agente, QuickJS, las
herramientas, la validación y la respuesta son del host, igual que en L1.

Relacionado: [[Principio-Doble-Ruta]], [[Identidad-de-Estado]],
[[Estrategia-de-Pruebas]], [[Punto-Actual]].
