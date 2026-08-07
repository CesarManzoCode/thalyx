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

Desde el 2026-08-06 cuenta **tres clases** y no dos, porque el archivo lleva
`/dev/console`:

```
13 directories
/dev/console  (character device 5:1, no contents)
/init  (26936 bytes)

1 program(s) in the image.
```

Un nodo de dispositivo no es un programa —no lleva código, es una puerta que el
kernel ya tiene— y clasificarlo con los programas habría hecho que la cuenta
dijera **2**: el decreto roto por una puerta. Pero **excluirlo en silencio es la
respuesta equivocada**, porque lo que no se cuenta es justo por donde entraría
un segundo programa sin que el número se moviera. Se cuenta como su propia
clase, se imprime con sus números —un nodo que apunta al driver equivocado falla
igual que uno ausente— y una prueba exige que las tres clases sumen todas las
entradas del archivo.

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

### El paso 1, cerrado el 2026-08-06

**Un firmware UEFI arrancó Thalyx sin gestor de arranque.** OVMF encontró
`\EFI\BOOT\BOOTX64.EFI`, lo ejecutó, el kernel desempaquetó el initramfs que
lleva adentro y corrió `/init`. El medio llevaba **un archivo**.

Y no arrancó a medias: hizo todo lo que la máquina sabe hacer.

```
  ok  root         moved off the initramfs
  ok  mounted /proc … /sys/fs/cgroup       (los siete)
  ok  sandbox root the root is attached
  ok  controllers  memory, pids handed down
  no  store        no thalyx.store= on the kernel command line
  ok  thalyx-lsm   2 hook(s) live, 3 map(s) pinned under /sys/fs/bpf/thalyx
```

**Lo que ninguna corrida anterior había establecido**: que el `switch_root`, la
delegación de controladores y sobre todo **el enganche del LSM** funcionan
cuando a la máquina la arranca un firmware y no `-kernel`. Eran tres cosas que
sólo se habían visto con QEMU haciendo de gestor de arranque.

El `no store` es correcto y previsto — ver abajo. Y **esto fue OVMF dentro de
QEMU**: discos virtio, teclado emulado, consola serie. El hierro sigue sin
tocarse, y ahí `console=ttyS0` deja de servir.

Costó un arranque, y lo que lo tumbó está en [[Estrategia-de-Pruebas]]: el
archivo tenía un `/dev` vacío, porque hasta entonces `/dev/console` lo ponía el
initramfs predeterminado del kernel, encima del cual se desempaqueta un
`-initrd` externo. Tercera vez que quitar una capa de abajo destapa un trabajo
que el diseño nunca hizo.

### Cómo encuentra su store una máquina instalada — decidido el 2026-08-06

**Por la etiqueta del sistema de archivos.** Decidido por Cesar.

El problema aparece en cuanto la máquina arranca sin la ISO: la línea de
comandos va **compilada dentro del kernel**, así que no puede decir
`thalyx.store=/dev/nvme0n1p2` — es una sola línea y el disco se llama distinto
en cada máquina. Es exactamente lo que la máquina reportó al arrancar el
2026-08-06, y lo reportó bien.

`mkfs.btrfs -L thalyx-store` ya escribe una etiqueta. Thalyx lee el superbloque
de Btrfs de cada dispositivo de bloques y busca **ese nombre exacto**.

**Esto no contradice el decreto de `store_disk.rs`**, y la distinción es la que
sostiene todo: lo prohibido es *«pruebo `/dev/vda`, luego `/dev/sda`, y monto el
primero que conteste»*, porque esa heurística acierta con el disco equivocado
exactamente una vez y ese fallo es Thalyx escribiendo su store encima del
filesystem de otro. Buscar una etiqueta es **pedir un nombre que Thalyx mismo
escribió**, no aceptar al primero que responda.

Y conserva la propiedad que importa, con dos negativas explícitas:

- **Ninguna etiqueta encontrada** → no hay store, se dice, no se fabrica nada.
- **Dos con la misma etiqueta** → se niega en vez de elegir. Elegir sería
  adivinar con una capa de pintura encima.

Queda pendiente cómo se distingue **«es el primer arranque»** de **«no encontré
el tuyo»**, que es lo que el decreto de PID 1 protege. La respuesta probable es
que **PID 1 sigue sin fabricar nada**: quien crea el store es el instalador,
que es un acto humano explícito. Así el decreto se conserva entero en vez de
pedirle una excepción.

### Thalyx escribe el Btrfs él mismo — construido el 2026-08-07

**`crates/thalyx-btrfs`.** Ocho árboles, tres chunks y los superbloques, escritos
byte por byte, sin `mkfs.btrfs` y sin `libbtrfs`. Es el punto 2 del orden de
trabajo y era el poste largo: la máquina arrancaba y no podía guardar nada.

Lo que obliga a que exista es [[Filosofia-Fundacional]] y no una preferencia. La
imagen lleva el kernel de Linux y un programa, así que `mkfs.btrfs` no puede
estar ahí y no se le puede agregar. Misma forma que `bpftool` para el LSM y
`cpio` para el initramfs, y la misma respuesta.

**No es una implementación de Btrfs**, y conviene que la nota lo diga antes que
lo diga alguien creyéndose una cosa distinta. Escribe **un** sistema de archivos,
vacío, con una geometría fija; y lee un superbloque para contestar *cómo se llama
este dispositivo*, que es la otra mitad de la decisión de la etiqueta. No sabe
asignar, no sabe balancear, y **se niega** si un árbol no cabe en una hoja en vez
de intentar partirlo. En cuanto el store existe, todo lo que le pasa se lo hace
el Btrfs del kernel.

Dos decisiones de forma que valen registrarse:

- **Metadata y system en DUP**, que es lo que `mkfs.btrfs` elige por omisión en
  un disco solo. El store es donde vive todo lo que sobrevive a un reinicio, y un
  sector malo en un bloque de árbol con una sola copia se lleva el sistema de
  archivos en vez de un archivo. Cuesta diez líneas: escribir el bloque dos veces.
- **Ninguna chunk cubre un superbloque.** `mkfs.btrfs` produce chunks que sí, y es
  legal, pero entonces el kernel tiene que excluir los sectores del superbloque de
  la asignación dentro de ese grupo de bloques — y una exclusión mal hecha es un
  bloque de árbol escrito encima del superbloque de respaldo. No solaparse es una
  cosa más chica que hay que acertar. Sale de escribir el layout final directo en
  vez de imitar la reubicación en dos fases de `mkfs.btrfs`.
- **El dispositivo mínimo es 128 MiB**, y el límite lo pone el superbloque de
  respaldo de los 64 MiB, no el espacio. Un disco más chico daría un store con
  una sola copia de su propio puntero a la raíz, y eso se niega diciéndolo.

**Y lo crea un humano, no PID 1.** `thalyx disk format <dispositivo>` es el verbo,
y **el decreto se conserva entero**: nada de `thalyx-btrfs` es alcanzable desde
PID 1. La confirmación pide que se **teclee la ruta del dispositivo**, no una `y`:
es la cosa más destructiva que Thalyx sabe hacer, el argumento es una palabra,
`/dev/sda` y `/dev/sdb` se diferencian en una tecla, y una `y` confirma una frase
que el humano ya dejó de leer. Antes de preguntar dice **qué hay ahí ahora**,
leyéndolo.

Dos verbos más: `thalyx disk identify` contesta qué es un dispositivo en los tres
términos que deciden qué hacer —btrfs con su etiqueta, no btrfs, o btrfs con el
superbloque dañado, que es la regla 10 donde más importa— y `thalyx disk layout`
imprime la geometría, que es de donde `verify.sh` saca los offsets de su control
en vez de repetirlos.

### Los tres subvolúmenes, por ioctl — construido el 2026-08-07

Un sistema de archivos recién escrito **no tiene subvolúmenes**, y PID 1 monta
`subvol=system`. Así que lo que salía de `thalyx disk format` era un sistema de
archivos y no un store, y el comando lo decía al terminar en vez de dejar que
pareciera que sí. Ahora los crea.

`btrfs subvolume create` no está disponible para eso: la imagen lleva el kernel de
Linux y un programa. Así que va por **`BTRFS_IOC_SUBVOL_CREATE`**, en
`thalyx-syscall`, que es la tercera vez que este proyecto contesta a un binario
ausente con una llamada al kernel en lugar de un segundo programa — `bpftool`,
`cpio`, y ahora `btrfs`.

**El número del ioctl no se toma de fe.** `_IOW` es un macro de C y en este
workspace no hay C, así que la constante está escrita a mano — y
`tests/ioctl.rs` la **recalcula desde el header capturado**, incluyendo el tamaño
del argumento que el número lleva codificado adentro. Importa porque el fallo no es
limpio: el kernel compara la palabra entera, así que un tamaño equivocado contesta
`ENOTTY` en un sistema de archivos que soporta la llamada perfectamente, y eso se
lee como «este kernel es viejo» o «esto no es btrfs».

Tres decisiones de forma:

- **La verificación es montar, no mirar.** Después de crearlos, cada uno se monta
  con `-o subvol=<nombre>` —exactamente como lo hace PID 1— y se reporta por
  nombre. Preguntar si apareció un directorio con ese nombre daría *sí* para un
  directorio común, que es lo único que PID 1 no puede montar. Y el truco del
  número de inodo (256) se descarta por el motivo que [[Journal-y-Snapshots]] ya
  registra: es cierto de Btrfs hoy y no es una interfaz documentada.
- **Correrlo dos veces es seguro**, y no es comodidad. Un instalador que falla a
  la mitad deja un store con dos de tres subvolúmenes, y si la única forma de
  arreglarlo fuera reformatear, la reparación costaría el disco entero. Un nombre
  que ya estaba se reporta como *ya estaba*, que es un hecho distinto de *lo creé*
  y distinto de un error.
- **Necesita un dispositivo de bloques, y lo dice.** `mount(2)` contesta `ENOTBLK`
  para un archivo común, porque enganchar un archivo a un loop es trabajo que
  util-linux hace en espacio de usuario. Thalyx no tiene por qué reimplementarlo:
  un instalador particiona un disco y escribe en particiones. Para un archivo de
  imagen, el error nombra `losetup`.

**Lo que la etapa 18 hace y la 19 no, a propósito.** La 18 crea los tres
subvolúmenes con btrfs-progs; la 19 los crea con Thalyx. Son dos afirmaciones
distintas —*el sistema de archivos que Thalyx escribió es un Btrfs que funciona* y
*Thalyx sabe crear un subvolumen*— y medir la primera con el código de la segunda
haría que un fallo esconda al otro. Misma razón por la que `make -C image store`
sigue usando `mkfs.btrfs`.

**Y el decreto se conserva entero.** Nada de esto es alcanzable desde PID 1.
`thalyx disk format` lo hace de una vez, y `thalyx disk subvolumes <dispositivo>`
es la mitad separable — que existe porque necesita cosas que escribir los bytes no
necesita: root, un kernel con Btrfs y un dispositivo de bloques.

**Lo que seguía faltando era el instalador**, y está construido — el bloque de
abajo.

Cómo se sabe que algo de esto es correcto está en [[Estrategia-de-Pruebas]], en
las dos reglas nuevas. En una frase: dos headers de Linux capturados verbatim más
`btrfs check`, y **el montaje sólo lo puede establecer la máquina de Cesar** —
etapa 18 de `verify.sh`.

Y `make -C image store` **sigue usando `mkfs.btrfs`**, a propósito. Es la red de
regresión de las etapas 13 y 16, y cambiarla en el mismo commit que introduce lo
que hay que probar dejaría la red y lo probado siendo el mismo código sin
ejercer. Es la misma razón por la que `boot` siguió pasando `-kernel` cuando
apareció `run-uefi`.

### El instalador, el acto que junta las dos piezas — construido el 2026-08-07

`crates/thalyx-install` y `thalyx install <disco> --kernel <archivo>`. Lo que sale:

```
  LBA 0          MBR protector
  LBA 1..34      la tabla de particiones, y su copia al otro extremo
  1 MiB          partición 1, 512 MiB, FAT32, con \EFI\BOOT\BOOTX64.EFI adentro
  513 MiB..      partición 2, el resto, Btrfs etiquetado `thalyx-store`,
                 con los tres subvolúmenes que decreta [[Journal-y-Snapshots]]
```

**Un archivo en la partición de arranque, y es el kernel con Thalyx adentro.** Es
`make -C image count` extendido al disco instalado.

Costó dos escritores de bytes más, por el mismo motivo de siempre. `sgdisk` y
`mkfs.vfat` son lo que usaría una persona, y la imagen lleva el kernel de Linux y
un programa — así que la GPT la escribe `gpt.rs` y el FAT32 lo escribe `fat.rs`.
Van la **cuarta y la quinta** vez que este proyecto contesta a un binario ausente
con el trabajo en vez de con la herramienta: `bpftool`, `cpio`, `btrfs`,
`partprobe`, `mkfs.vfat`.

**Por qué FAT, en un proyecto que eligió Btrfs.** Porque lo eligió el firmware: la
especificación UEFI obliga al firmware a entender FAT y nada más, así que la
partición donde busca `\EFI\BOOT\BOOTX64.EFI` tiene que ser FAT. Es el único
sistema de archivos de Thalyx que existe para satisfacer algo de afuera, y conviene
que quede dicho para que nadie lo lea como una preferencia.

**Y una cuarta llamada al kernel:** `BLKRRPART`. Escribir una tabla en un disco que
el kernel ya tiene abierto no cambia nada de lo que el kernel ve — `/dev/sda1` no
aparece, así que el paso siguiente no tiene dónde escribir. `partprobe` es lo que
correría una persona. En `thalyx-syscall`, con su número recalculado desde
`include/uapi/linux/fs.h` capturado, porque `_IO` es un macro de C y aquí no hay C.

Cuatro decisiones de forma que vale registrar:

- **Los nombres de las particiones se le preguntan al kernel, no se derivan.**
  `/dev/sda` da `/dev/sda1` y `/dev/nvme0n1` da `/dev/nvme0n1p1`, y la regla que
  produce las dos —agregar `p` cuando el nombre termina en dígito— es una convención
  de las herramientas que los imprimen, no una promesa del kernel. Derivarla da un
  instalador que anda en SATA y escribe el store en la nada en NVMe, que es
  justamente la mitad del hierro que no se puede probar aquí. Así que se leen de
  `/sys/dev/block/<mayor>:<menor>/`, que es donde el kernel los publica.
- **La ESP es de 512 MiB y la holgura es el punto.** No se puede agrandar después
  sin mover el store, y lo único que seguro va a pasar es que una actualización de
  kernel tenga que escribir el nuevo **al lado** del que está corriendo antes de
  quitar el viejo. Una máquina que sobrescribe su único archivo arrancable y se
  queda sin corriente no vuelve.
- **Un disco de 4 KiB por sector se rechaza en vez de escribirse.** Cada LBA estaría
  a cuatro veces el byte que el kernel mira, así que la tabla simplemente no se
  encontraría — y eso no se ve como un disco roto, se ve como un disco intacto.
- **Se le da el tipo `Linux filesystem data` al store**, y no un GUID propio de
  Thalyx. Thalyx encuentra su store por la etiqueta del Btrfs y nunca por ese
  número; la única vez que el campo importa es cuando un humano metió el disco en
  otra máquina para ver qué tiene, y un tipo que nada reconoce contesta *desconocido*.

**El kernel se le pasa por `--kernel`, y eso es a propósito por ahora.** Un
instalador corriendo *dentro* de la máquina arrancada desde la ISO tendría que sacar
el bzImage del medio del que arrancó, y eso pide un **lector** de FAT y una forma de
saber cuál de los discos es el medio. Es su propio cambio y no va encima de éste;
está anotado en [[Tareas-Pendientes]].

#### Cómo se sabe que algo de esto es correcto

Tres instrumentos, y sólo el primero es una prueba de `cargo`.

1. **Los offsets contra los headers de Linux capturados verbatim**:
   `block/partitions/efi.h`, `include/uapi/linux/msdos_fs.h`,
   `include/linux/uuid.h` y `include/uapi/linux/fs.h`. El parser que los lee se
   **gradúa antes de medir nada** contra cuatro tamaños que los headers afirman en
   su propio texto — `legacy_mbr` y `fat_boot_fsinfo` contra `SECTOR_SIZE`,
   `msdos_dir_entry` contra `MSDOS_DIR_BITS`, y `guid_t` contra `UUID_SIZE` —,
   que es la regla 5 aplicada al arnés antes de usarlo.
2. **La etapa 20 de `verify.sh`**, donde el **kernel** lee la tabla, monta el FAT32,
   lee el archivo de vuelta y lo compara byte por byte, y monta los tres
   subvolúmenes del store.
3. **`make -C image run-installed`**, donde un firmware UEFI recibe **sólo el disco
   instalado** —sin ISO, sin `-kernel`, sin nada— y tiene que encontrar el archivo y
   arrancarlo. Ésa es la afirmación, y nada menos que eso es la afirmación.

**El punto 1 no alcanza y hay que decir por qué**, porque es lo que distingue a esta
pieza de las anteriores: **una GPT con una suma equivocada no se reporta como rota,
se ignora.** Linux cae al MBR protector y contesta que el disco no tiene
particiones — exactamente lo mismo que contesta un disco que nadie tocó. Un
instalador con ese defecto imprime `ok`. La regla nueva está en
[[Estrategia-de-Pruebas]].

Y el contenedor de desarrollo **no puede establecer el punto 2**: sus dispositivos
`loop` no admiten particiones de ningún tipo. Se comprobó escribiendo un MBR común
—que todo kernel de Linux parsea— y viendo que tampoco producía ninguna; sin ese
discriminador, «no aparecieron particiones» se habría leído como Thalyx escribiendo
mal la tabla. La etapa 20 lleva ese discriminador adentro en vez de una nota.

Lo que sí se pudo hacer aquí, y vale como red: las dos sumas de la GPT recalculadas
con un CRC-32 independiente, y el volumen FAT32 recorrido entero por un lector
escrito aparte —directorio raíz, `EFI`, `BOOT`, la cadena de clusters del archivo—
que devolvió los 3 000 000 de bytes idénticos.

### Lo construido el 2026-08-06

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

**Corrido el 2026-08-06 y en verde**, al segundo intento. El primero murió
antes de su primera línea por el `/dev/console` que faltaba; ninguna opción del
kernel resultó estar mal, que era lo que se esperaba que fallara.

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
