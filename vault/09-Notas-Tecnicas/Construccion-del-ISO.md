---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-03
tags: [iso, imagen, build, qemu, fase-1, no-negociable]
---

# Construcción de la imagen del sistema

> Esta nota se reescribió entera el 2026-08-03. La versión anterior decretaba
> una distribución de Alpine y contradecía a [[Decision-Capa-vs-SO-Nuevo]] sin
> que nadie lo notara durante tres días. Lo que decía, y por qué era falso, está
> al final.

## Decreto

**La imagen contiene el kernel de Linux y un programa: `thalyx`.**

Eso es todo, y está escrito así porque es **contable**: se listan los archivos, y
o hay dos cosas o hay más. Un criterio que se comprueba mirando, no discutiendo.

Lo que no lleva, y no llevará:

- Ninguna distribución. **Ni Alpine ni ninguna otra, nunca.**
- Ningún shell.
- Ningún conjunto de utilidades de sistema — `ls`, `cat`, `id`, busybox.
- Ningún gestor de paquetes. El software llega en `.thmod` o no llega.
- Ningún init de terceros. PID 1 es `thalyx`.
- Ninguna herramienta de construcción de distros: `mkimage.sh`, apkovl,
  live-build.

## Qué es y qué no es "depender del kernel de Linux"

**El kernel de Linux nunca estuvo en discusión.** Es el kernel, `thalyx-lsm` lo
extiende desde la Fase 1, y escribir uno propio es otro proyecto que
[[Fases-de-Implementacion]] pone en años 2-3 y "si aplica".

Una distribución es otra cosa: es el userland, el gestor de paquetes, el init y
las decisiones de arranque que alguien más ya tomó. **Eso es lo que queda
fuera.** El kernel es una pieza que se usa; una distro es un sistema operativo
ajeno dentro del cual el nuestro estaría incrustado — que es exactamente donde
estaba hasta hoy.

`musl` se enlaza **estáticamente**, en tiempo de compilación. No es un programa
en el disco: es código compilado adentro, como cualquier biblioteca. En la
imagen no hay cargador dinámico ni libc que alguien pueda invocar, porque no hay
nadie que pueda invocarla.

## La consecuencia que hace que esto valga la pena

Sin userland, **un módulo no puede ser un script**. No hay `sh` que lo interprete
ni `ls` al que llamar.

Un módulo no tiene con quién hablar excepto Thalyx, y entonces la **API interna**
que [[Core-Nucleo]] decreta —y que hasta hoy no existía ni en una línea— deja de
ser opcional: es la única forma de que un módulo haga algo.

Eso cierra el hueco que [[Decision-Capa-vs-SO-Nuevo]] nombraba y Thalyx no
cumplía. El criterio de ese decreto es que los programas se escriban contra el
contrato de Thalyx; con un userland POSIX debajo siempre había otro contrato
disponible, y ninguno usaba el nuestro. Sin él no hay alternativa.

**Quitar la distro y volverse un sistema operativo son el mismo acto.**

## Cómo se construye, decidido el 2026-08-03

**Un initramfs, no un ISO.** Un ISO necesita gestor de arranque, tabla de
particiones y un filesystem donde ponerlos. Un initramfs no necesita ninguno de
los tres: el kernel desempaca un archivo cpio en un tmpfs y ejecuta `/init`. A
QEMU se le pasan el kernel y el archivo, y nada más.

No es un atajo alrededor del decreto: es el decreto sin nada sobrante. **No hay
una tercera cosa en el camino donde algo se pueda esconder**, que es exactamente
por donde entró la base de Alpine la vez pasada.

**El archivo lo construye Thalyx**, en `crates/thalyx-cli/image.rs`. No por
desconfiar de `cpio` —es un programa perfectamente bueno— sino porque la forma
del error anterior fue *agarrar lo que la base ofrecía*, y un sistema que
embarca un solo programa puede producir su propio sistema de archivos raíz en
doscientas líneas en vez de heredar una cuarta cosa que nadie eligió.

**Un archivo, no dos.** La primera versión ponía el binario en `/thalyx` y en
`/init` por familiaridad, y duplicaba una imagen de 47 MB para decir dos veces lo
mismo. Si el decreto dice "un programa", un solo archivo es lo que lo vuelve
cierto en vez de casi cierto — y es lo que devuelve la cuenta.

**El kernel parte de `allnoconfig`.** La dirección importa: un kernel de
distribución con cosas apagadas sigue conteniendo todo lo que nadie se acordó de
apagar, y nadie puede decir qué hay adentro. Partiendo de cero, lo que está
encendido es lo que alguien decidió encender. La lista está en
`image/thalyx.config`, con un motivo al lado de cada grupo.

**Y se comprueba que sobreviva.** `olddefconfig` descarta en silencio cualquier
opción cuyas dependencias no se cumplan: la línea simplemente no aparece en el
`.config` resultante, sin una advertencia. Nueve de las opciones pedidas se
estaban perdiendo así, entre ellas `CONFIG_BPF_LSM` y `CONFIG_DEBUG_INFO_BTF`
—las dos que `thalyx-lsm` necesita para existir—. Por eso `make -C image kernel`
compara línea por línea lo pedido contra lo que salió y **detiene la compilación**
si falta alguna. Ver la regla de [[Estrategia-de-Pruebas]] sobre pedir y tener.

**El estado persistente va en otro disco.** La raíz es un tmpfs que no conserva
nada entre arranques. PID 1 monta los tres subvolúmenes; **no los crea**, porque
una máquina que se fabrica un store nuevo cada vez que no encuentra el viejo
nunca podría avisarte de que lo perdió.

**El disco se hace una sola vez, al construir.** `sudo make -C image store` lo
formatea con Btrfs, crea los tres subvolúmenes e instala el primer módulo
adentro. Corre en una máquina que tiene btrfs-progs porque **la imagen no lo
tiene y no puede tenerlo**: lleva un programa. Es la misma forma que el problema
de `bpftool`, con la misma respuesta — el trabajo se mueve al momento de
construir, no se mueve un segundo binario a la imagen.

**PID 1 no adivina cuál es el disco.** El parámetro `thalyx.store=` del kernel
lo nombra. La alternativa —probar `/dev/vda`, luego `/dev/sda`, luego lo que
parezca— es una heurística que acierta con el disco equivocado exactamente una
vez, y ese fallo es Thalyx escribiendo su store encima del filesystem de otro.
Cuando el parámetro no está, eso se reporta como su propio hecho y no como un
disco ausente.

**Y el subvolumen `modules` no se monta en `/opt/thalyx/modules`.** Se monta en
`/opt/thalyx/data`. Ponerlo donde su nombre sugiere se lee más ordenado y rompe
todos los commits atómicos de la máquina: `rename(2)` devuelve `EXDEV` al cruzar
subvolúmenes de Btrfs, así que el área de staging y su destino tienen que estar
en el mismo. Es el fallo que [[Fase-Commit-Atomico]] ya registra una vez. Lo que
va en `modules` son los datos que un módulo escribe, que es lo que un snapshot
tendría que poder revertir.

### Cómo se cuenta

El decreto se escribió para ser contable, y esto es lo que cuenta:

```
make -C image count
```

Si dice algo distinto de `1 program(s) in the image`, el decreto está roto y el
número lo dice antes de que nadie tenga que discutirlo. Lo cuenta **parseando el
archivo**, no reportando lo que el constructor creía haber metido.

## Qué queda por decidir, y no se hereda

La versión anterior heredó de Alpine cosas que nadie eligió — el login en tty1
fue la más visible, no la única. Lo que sigue abierto se escribe aquí para que
se decida en vez de aparecer:

- ~~**Cargar `thalyx-lsm` desde dentro de Thalyx.**~~ **Escrito el 2026-08-03**,
  y sin ejercer todavía. El objeto BPF viaja dentro del binario y Thalyx hace
  las llamadas al kernel él mismo. Ver [[Cargador-BPF-Propio]]. La etapa 14 de
  `verify.sh` es donde se comprueba.
- **Qué pasa al apagar.** Ya hay una respuesta parcial: `apagar` en la sesión
  le pide al kernel que corte la corriente. Lo que falta decidir es qué se
  sincroniza antes, y qué hace la máquina cuando alguien corta la corriente sin
  avisar.
- **Que el binario sea estático de verdad.** Hoy enlaza glibc dinámicamente. El
  `Makefile` lo comprueba y se niega si no lo es, pero nunca se ha corrido.

## Cómo se arranca

**Hoy: QEMU, y QEMU es el gestor de arranque.** El `run` del `Makefile` pasa
`-kernel bzImage -initrd initramfs.cpio -append ...`, que es el firmware
haciéndole a la máquina el favor de cargar el kernel y su initramfs en memoria.
No hay medio de arranque, no hay tabla de particiones, no hay nada que un BIOS
o un UEFI pudiera encontrar por su cuenta.

**Y desde el 2026-08-06 eso ya no alcanza**, porque cerrar la Fase 1 es tener
una ISO que arranque sola. Ver abajo.

## La ISO independiente — decretado por Cesar el 2026-08-06

**Es el criterio de salida de la Fase 1.** Ver [[Criterio-de-Salida-Fase-1]] para
el decreto; aquí está lo que implica construirlo.

> Una ISO totalmente independiente: que puedas ponerla en una PC sin sistema
> operativo y que ahora tenga Thalyx como OS. Obviamente lo haremos de alguna
> forma más fácil, por ejemplo una VM, pero el objetivo es que tengamos la ISO
> y nada más, y con ella sola podamos tener Thalyx corriendo.

### Las tres decisiones de Cesar, el 2026-08-06

Se le preguntaron las tres que cambian lo que se construye, y las tres están
decididas:

| Pregunta | Decisión |
|---|---|
| ¿La ISO se queda puesta o la máquina arranca sin ella? | **Arranca sin ella.** La ISO instala Thalyx en el disco y se quita. La PC *tiene* Thalyx, no lo pide prestado. Implica un instalador. |
| ¿Qué firmware? | **Sólo UEFI.** Deja fuera las PC anteriores a ~2012 y a cambio **no hay gestor de arranque**, que es lo que salva el decreto. |
| ¿Quién crea el store? | **Thalyx escribe el Btrfs él mismo.** El mismo patrón que el cpio, el cargador de BPF y la terminal: ochenta líneas propias antes que una cuarta herramienta ajena. |

La segunda y la tercera se refuerzan: sólo-UEFI quita el gestor de arranque, y
un Btrfs escrito por Thalyx quita `mkfs.btrfs`. **La ISO terminada no contiene
ningún programa que no sea Thalyx**, que es el decreto fundacional sostenido en
el sitio donde era más fácil pedirle una excepción.

### El gestor de arranque, que es la pregunta de filosofía

Un medio arrancable necesita que el firmware encuentre algo y lo cargue. La
respuesta obvia —GRUB, syslinux, systemd-boot— es **un segundo programa**, y
[[Filosofia-Fundacional]] no lo permite. Sería exactamente la forma del hueco de
`bpftool`: una herramienta ajena metida en la imagen porque hacía falta.

**La respuesta que no rompe el decreto es que no haya gestor de arranque.** Un
kernel de Linux compilado con `CONFIG_EFI_STUB` **es** una aplicación UEFI
válida: el firmware puede cargar el `bzImage` directamente como
`\EFI\BOOT\BOOTX64.EFI`, sin nada en medio. Con `CONFIG_INITRAMFS_SOURCE` el
initramfs va **dentro** del kernel, y con `CONFIG_CMDLINE` la línea de comandos
también.

Si eso funciona, el medio lleva **un archivo**, y ese archivo es el kernel con
Thalyx adentro. Es `make -C image count` extendido al medio de arranque, y
resuelve el decreto en la dirección fuerte en vez de pedirle una excepción.

**No se ha construido ni ejercido.** Es un plan, no una propiedad, y esta nota
lo dice para que nadie lo cite como si estuviera hecho.

### Lo que cuesta de verdad, en orden de riesgo

1. **El store, que hoy nadie crea.** `store_disk.rs` decreta que PID 1 **monta y
   nunca fabrica**, con una razón buena: una máquina que se inventa un store
   cuando no encuentra el suyo arranca perfecta el día que el disco no estaba, y
   el humano se entera al notar que todo lo que instaló desapareció. En una PC
   sin sistema operativo **no hay store**, y `mkfs.btrfs` no puede ir en la
   imagen. Alguien tiene que crearlo en el primer arranque, y **«lo creo porque
   es la primera vez» y «lo creo porque no encontré el tuyo» tienen que seguir
   siendo distinguibles**, que es lo que el decreto actual protege.
2. **Los controladores.** `thalyx.config` sale de `allnoconfig` más lo que QEMU
   necesita: virtio y un puerto serie. Una PC de verdad necesita UEFI, consola
   sobre el framebuffer, teclado USB (xHCI y HID) y almacenamiento real (NVMe,
   AHCI). **Cada uno es una opción de Kconfig**, y ya hay tres opciones
   encontradas arrancando y una regla que dice que ninguna comprobación de
   construcción encuentra la siguiente. Ésta es la parte que la VM no prueba.
3. **La consola.** Hoy es `console=ttyS0`. Una PC moderna no tiene puerto serie:
   arrancaría bien y **no se vería nada**, que es el fallo que se lee como «no
   funciona» siendo «no puedes mirar».
4. **BIOS o sólo UEFI.** Arrancar por BIOS heredado exige código de arranque en
   el MBR, que es un gestor de arranque otra vez. Sólo-UEFI evita el problema
   entero y deja fuera máquinas de antes de ~2012.

### Lo construido el 2026-08-06, y sin ejercer

**El paso 1: arrancar sin gestor de arranque.** Va primero porque **si falla,
todo lo demás cambia de forma** — haría falta un gestor, que es un segundo
programa, y eso es decisión de Cesar y no un detalle de construcción.

- `thalyx.config` pide `EFI`, `EFI_STUB`, `RELOCATABLE` y `ACPI` (de la que
  depende `EFI` en x86), más una línea de comandos compilada dentro.
- El initramfs va **adentro** del kernel (`CONFIG_INITRAMFS_SOURCE`), así que el
  medio lleva un archivo. La ruta es absoluta y no puede vivir en
  `thalyx.config` —`config-check` compara líneas literales— así que el
  `Makefile` la inyecta al configurar y la comprueba con `initramfs-check`, que
  es su propia regla porque **una línea que nadie comprueba es una línea que
  `olddefconfig` tira en silencio**, y eso ya pasó nueve veces en este archivo.
- `make -C image esp` arma `EFI/BOOT/BOOTX64.EFI`, que es la ruta de respaldo
  que un firmware UEFI busca **sin nada configurado** — que es exactamente lo
  que es una PC sin sistema operativo.
- `make -C image run-uefi` arranca con OVMF y **sin `-kernel`, `-initrd` ni
  `-append`**. Hay una prueba que exige esa ausencia, y es la que más vale de
  las tres: `-kernel` es QEMU haciendo de gestor de arranque, el medio no se lee
  nunca, y el arranque sale precioso sin demostrar nada. Es además el arreglo
  que uno intenta cuando el firmware no arranca.

**Un defecto atrapado antes de llegar a la máquina**, y vale registrarlo:
`CONFIG_CMDLINE_OVERRIDE` es la opción que uno pone aquí —hace que la línea de
comandos sea la del archivo y nada la cambie— y **habría roto la etapa 16**. El
`boot` de hoy pasa `-append ... thalyx.store=/dev/vda`, y `OVERRIDE` descarta la
línea del gestor de arranque entera: la máquina habría arrancado sin store, cada
comprobación del store habría fallado, y la causa habría estado en una opción de
kernel a tres archivos de la falla. Sin `OVERRIDE` las dos líneas se concatenan
y **el mismo kernel arranca por los dos caminos**, que es la propiedad que
importa: la máquina que se prueba tiene que ser la que se entrega.

Por lo mismo, `boot` **sigue** pasando `-kernel` e `-initrd`: es la red de
regresión de todo lo demás, y cambiarla ahora dejaría la red y lo que se está
probando siendo el mismo cambio sin ejercer. Se mueve a `run-uefi` cuando
`run-uefi` haya arrancado, no antes.

**Nada de esto se ha corrido.** Este contenedor no tiene QEMU, ni firmware UEFI,
ni con qué compilar un kernel. Cada arranque anterior de esta imagen encontró
una opción de kernel que ninguna comprobación de aquí podía ver — van tres — y
lo esperable es que éste encuentre más.

### Qué prueba la VM y qué no

Vale la pena separarlo, porque la forma fácil de probarlo mide menos de lo que
parece:

- **La VM con firmware UEFI de verdad (OVMF) prueba lo que más importa**: que la
  ISO arranca **sola**, sin `-kernel` ni `-initrd`, encontrada por un firmware.
  Eso es la mitad del criterio y se puede ejercer en `verify.sh`.
- **La VM no prueba los controladores.** Los discos son virtio y el teclado es
  emulado. Un arranque en hierro real es lo único que responde eso, y es el
  único punto donde este criterio necesita una máquina física.

## Revisiones

### 2026-08-03 — el objeto BPF va dentro del binario, no junto a él
**Antes:** cargar `thalyx-lsm` figuraba como "el hueco grande", y el cargador
buscaba `/lib/thalyx/thalyx_lsm.bpf.o` invocando `bpftool`.
**Ahora:** el objeto se compila al construir, `build.rs` lo incrusta en el
binario, y `thalyx-bpf` hace las cuatro llamadas `bpf(2)`. Ver
[[Cargador-BPF-Propio]].
**Motivo:** un segundo archivo y un segundo programa, los dos prohibidos por
[[Filosofia-Fundacional]] en una imagen que lleva uno. El mensaje que imprimía
el cargador al no encontrar el archivo sugería que alguien lo pusiera ahí — o
sea, sugería romper el decreto que estaba reportando.

### 2026-08-03 — el store se construye, y `modules` no va donde su nombre dice
**Antes:** "Crear los subvolúmenes del store la primera vez. PID 1 los monta y
no los crea; falta decidir quién los hace y cuándo" figuraba entre lo que
quedaba por decidir. El disco `store.qcow2` se creaba vacío y nadie lo tocaba
nunca; PID 1 tampoco lo montaba, aunque su propio comentario decía que sí.
**Ahora:** `sudo make -C image store` formatea el disco, crea los tres
subvolúmenes e instala el primer módulo; PID 1 lo monta por el nombre que le da
`thalyx.store=` y nunca lo crea. El subvolumen `modules` se monta en
`/opt/thalyx/data`.
**Motivo:** sin store, instalar un módulo dentro de la máquina era imposible y
el único lugar donde el `greeter` había corrido era la Fedora de Cesar. Y la
ubicación de `modules` no es cosmética: `/opt/thalyx/modules` habría roto todos
los commits atómicos por `EXDEV`, que es el fallo que [[Fase-Commit-Atomico]] ya
registra. Hay una prueba que lo comprueba y una etapa de `verify.sh` que lo
ejerce en un disco real, con línea base y control.

## Historial: lo que esta nota decretaba antes, y por qué era falso

Del 2026-08-01 al 2026-08-03 decía:

> La imagen se construye con las herramientas nativas de Alpine (`mkimage` y
> apkovl). […] **Base Alpine minimalista.** […] Por qué Alpine y no otra base:
> coherencia con la base ya decretada en [[Core-Nucleo]], y tamaño.

`mkimage.sh` es la herramienta con la que se construyen distribuciones de
Alpine, y lo que produce es una distribución de Alpine. La bóveda decretaba, al
mismo tiempo:

| Nota | Decía |
|---|---|
| [[Decision-Capa-vs-SO-Nuevo]] *(no negociable)* | "No es una capa y **no es una distribución de Linux**" |
| Esta nota | Se construye con las herramientas de Alpine, base Alpine |
| [[Fases-de-Implementacion]] | "Fase 1: Núcleo de Thalyx **sobre base Alpine**" |
| [[Core-Nucleo]] | "Base: kernel Linux minimalista **(tipo Alpine)**" — Alpine no tiene kernel propio; es un error de categoría |

Tres notas decretaban una distro y una decretaba que no lo era. Convivieron tres
días, y las cuatro se leyeron el mismo día sin que saltara.

Lo encontró Cesar preguntando por qué habría un login al arrancar si nadie lo
construyó. La respuesta —que lo pone la base— hizo visible que había una base.

El esqueleto de imagen escrito esa misma noche se borró con esta reescritura,
junto con el módulo `dev.thalyx.hola`, que era un script de shell. Quitarle el
getty a una distro de Alpine hace que no la veas; no hace que no esté. Y eso es
peor que dejarla a la vista, porque esconde justo la pregunta que hay que poder
hacerse.

## Relacionado
- [[Decision-Capa-vs-SO-Nuevo]] — por qué esto no es negociable
- [[Core-Nucleo]] — qué lleva el sistema, y su API interna
- [[Fases-de-Implementacion]]
- [[Criterio-de-Salida-Fase-1]]
- [[Condiciones-de-Adopcion]]
