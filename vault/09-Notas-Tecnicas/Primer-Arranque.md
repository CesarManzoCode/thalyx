---
tipo: procedimiento
estado: activo
fecha-decreto: 2026-08-03
tags: [imagen, arranque, procedimiento, fase-1]
---

# El primer arranque

> **Si eres una sesión nueva y Cesar te está pegando la salida de un comando:
> es de aquí.** Esta nota tiene los comandos en orden, lo que cada uno debe
> imprimir, y qué significa cada fallo. No necesitas más contexto del que hay en
> este archivo para responder — busca el paso, compara la salida, y sigue la
> columna de "si falla".
>
> **Qué se ha ejecutado y qué no.** El paso 3 se corrió una vez, el 2026-08-03,
> en el contenedor de desarrollo y a mano: 6.12.101 configurado y compilado
> desde un espejo de GitHub, con `pahole` real. Salió `bzImage` y el `vmlinux`
> trae `.BTF`. Lo que eso comprueba es **la configuración**, no tu compilador:
> el contenedor tiene GCC 13 y el problema que motivó el cambio de versión solo
> aparece con GCC 15.
>
> **El paso 5 corrió y funcionó**, el 2026-08-03. La máquina arrancó, montó los
> siete filesystems, imprimió lo que es y lo que no tiene, y esperó. Lo que
> queda abajo ya no es un pronóstico: es el procedimiento que sí funciona, con
> lo que efectivamente imprimió.
>
> **El mecanismo del store está probado; el paso 4b no.** La etapa 13 de
> `verify.sh` corrió en verde en la Fedora de Cesar el 2026-08-03: formateó un
> disco Btrfs, le hizo los tres subvolúmenes, instaló el módulo adentro y lo
> volvió a montar para comprobar que sobrevivió. Lo que eso no ejerce es el
> `Makefile` —el paso 4b— ni el disco arrancando dentro de QEMU. Si el paso 4b
> falla y la etapa 13 estaba verde, el problema está en el `Makefile`, no en el
> store.

## Qué se está intentando

Arrancar por primera vez la máquina que decreta [[Construccion-del-ISO]]: **el
kernel de Linux y un programa**. Sin distribución, sin shell, sin gestor de
paquetes. Es el paso 1 del [[Criterio-de-Salida-Fase-1]].

## Paso 0 — que nada se haya roto

```sh
cd ~/thalyx
git pull
cargo install --path crates/thalyx-cli
sudo ./dev/verify.sh
```

**Debe imprimir**, al final:

```
proven      67
not proven  2
failed      0
```

Las **dos** `not proven` esperadas son:

1. El modelo del agente — `llama.cpp` no está instalado y la ruta real no existe.
2. La imagen — nunca ha arrancado.

La etapa 13 es la que prueba el store: formatea un disco Btrfs de verdad, le
hace los tres subvolúmenes, instala el módulo adentro y lo vuelve a montar para
ver si sobrevivió. **Corrió en verde el 2026-08-03**, seis comprobaciones. En un
contenedor sin Btrfs en el kernel se salta diciéndolo, y eso también está bien.

| Si falla | Qué significa |
|---|---|
| `failed` > 0 | Regresión real. La etapa lo dice; el log queda en el directorio temporal que imprime. Esto se arregla antes de seguir. |
| `not proven` > 2 | Algo que antes se comprobaba dejó de poder comprobarse. Mira cuáles se nombran al final: casi siempre es una herramienta que falta (`bpftool`, `clang`, `btrfs`). |
| Falla algo de la etapa 13 | El store. Pégame la etapa completa: cada línea dice cuál de los cuatro pasos —formatear, montar, instalar, sobrevivir al remontaje— fue el que se cayó, y son problemas distintos. |
| `520 tests` sale distinto | Si es menos, algo dejó de compilarse. Si es más, alguien agregó pruebas y está bien. |

## Paso 1 — el target de musl

```sh
rustup target add x86_64-unknown-linux-musl
sudo dnf install -y musl-gcc musl-libc-static
```

`musl-gcc` hace falta porque SQLite se compila desde fuente dentro del binario
(ver `Cargo.toml`), y eso es C.

| Si falla | Qué hacer |
|---|---|
| `musl-gcc` no existe en los repos | En Fedora el paquete puede llamarse `musl-devel` o venir de `musl-gcc`. `dnf search musl` lo dice. |

## Paso 2 — el binario estático

```sh
make -C image binary
```

**Debe imprimir** una línea `static:` con un tamaño. El `Makefile` **se niega**
si el binario no salió estático, porque uno dinámico necesitaría una libc en el
disco de la imagen y eso ya no existe.

| Si falla | Qué significa y qué hacer |
|---|---|
| `NOT STATIC` | El enlazado salió dinámico. Suele arreglarse con `RUSTFLAGS="-C target-feature=+crt-static"`. Pégame el error completo. |
| Errores de `libsqlite3-sys` o `cc` | Falta `musl-gcc`, o `CC_x86_64_unknown_linux_musl` no apunta a él. Prueba `CC_x86_64_unknown_linux_musl=musl-gcc make -C image binary`. |
| Errores de enlazado sobre `getaddrinfo`, `dlopen` o similares | musl no implementa algunas cosas de glibc. Pégame la lista completa de símbolos: es información de diseño, no un error de configuración. |

## Paso 3 — el kernel

```sh
sudo dnf install -y gcc make flex bison bc openssl-devel elfutils-libelf-devel \
    dwarves perl-devel
make -C image kernel
```

`dwarves` trae `pahole`, que hace falta para `CONFIG_DEBUG_INFO_BTF`, que hace
falta para que `thalyx-lsm` pueda cargarse contra este kernel. **Instálalo antes
de configurar, no después:** si `pahole` no está cuando se resuelve el
`.config`, la opción de BTF no se rechaza, desaparece.

Tarda. Descarga ~150 MB y compila con todos los núcleos.

**Antes de compilar debe imprimir**:

```
  config: every option in thalyx.config survived olddefconfig
```

Esa línea existe porque `olddefconfig` descarta en silencio las opciones cuyas
dependencias no se cumplen, y nueve se estaban perdiendo así.

| Si falla | Qué significa y qué hacer |
|---|---|
| `These were asked for and are not in the kernel's .config` | La comprobación hizo su trabajo. Cada opción que nombre le falta una dependencia, o ya no existe con ese nombre en esta versión. Pégame la lista completa: agregar la dependencia es el arreglo correcto, no quitar la opción. |
| `cannot use keyword 'false' as enumeration constant` | Estás compilando una versión anterior a 6.12.14 con GCC 15. `KVERSION` en `image/Makefile` debe decir `6.12.101`. Si dice eso y aun así sale, borra `image/build/linux-*` para que no reutilice el árbol viejo. |
| `pahole: command not found` | Falta `dwarves`. |
| Errores de `flex`/`bison`/`bc` | Falta alguna de las de arriba; el mensaje dice cuál. |
| Descarga falla | Cambia `KVERSION` en `image/Makefile` por una versión que exista en `cdn.kernel.org/pub/linux/kernel/v6.x/`, **6.12.14 o posterior**. |

## Paso 4 — la imagen, y contarla

```sh
make -C image image
make -C image count
```

**Debe imprimir exactamente**:

```
13 directories
/init  (N bytes)

1 program(s) in the image.
```

**Si dice cualquier otro número que no sea `1 program(s)`, el decreto
fundacional está roto.** No es una advertencia: es la comprobación de
[[Filosofia-Fundacional]] hecha número.

Esta parte sí está probada — es `thalyx dev image`, Rust con pruebas.

**El módulo no cuenta y no debe contar.** Vive en el disco del store, no en la
imagen. Esa es exactamente la diferencia entre lo que Thalyx *es* y lo que
alguien le instaló encima.

## Paso 4b — el store

```sh
sudo dnf install -y btrfs-progs
make -C image store-stage
sudo make -C image store
```

**Son dos comandos y el orden importa.** El primero compila y arma lo que va en
el disco; corre como tú. El segundo formatea `image/build/store.img` con Btrfs,
le hace los tres subvolúmenes y copia lo del primero adentro; ese sí necesita
root, porque monta un dispositivo de bucle.

**`store` no compila nada, a propósito.** Era un solo comando y estaba mal dos
veces: falló de inmediato —`sudo` reinicia el `PATH` y `rustup` vive en tu home,
así que no existe— y ese es el problema chico. De haber funcionado, `sudo make
store` habría corrido toda la compilación de Rust como root: los scripts de
build de cada dependencia ejecutándose con privilegio, y archivos de root
quedando en `target/` que tu siguiente `cargo build` normal ya no podría
reemplazar. La frontera de privilegio es la frontera de target.

**El primero debe terminar** con `staged:` y la ruta. **El segundo debe imprimir**,
al final:

```
ID 256 gen 8 top level 5 path system
ID 257 gen 9 top level 5 path modules
ID 258 gen 9 top level 5 path user

  store: .../image/build/store.img — three subvolumes, one module installed
```

Los números de `ID` y `gen` van a ser otros; lo que importa son los tres
`path`. `store-stage` falla solo si el módulo no salió estático o si el bundle
no verifica, que es donde conviene que fallen esas cosas: sin privilegio y con
un mensaje que habla de lo que se está construyendo, no del disco.

| Si falla | Qué significa y qué hacer |
|---|---|
| `nothing staged yet` | Te saltaste `make -C image store-stage`. Ese va primero y sin `sudo`. |
| `rustup: No existe el fichero` | Estás corriendo `store-stage` con `sudo`. No lleva sudo. |
| `make store needs root` | Es `sudo make -C image store`, no `make`. |
| `no mkfs.btrfs` | Falta `btrfs-progs`. |
| `NOT STATIC — the module could not run on the machine` | El `greeter` enlazó dinámico. Mismo arreglo que el paso 2: falta `musl-gcc` o el target de musl. |
| `the module did not install into the stage` | El bundle no verificó o el commit no se publicó. Pégame la salida completa: es un fallo del core, no del disco. |
| `mount: ... unknown filesystem type 'btrfs'` | Tu kernel no trae Btrfs. En Fedora 43 lo trae; si sale esto, algo raro pasa y quiero verlo. |

## Paso 5 — arrancar

```sh
sudo dnf install -y qemu-system-x86 qemu-img
make -C image run
```

Sale por la consola serie. Para salir de QEMU: `Ctrl-a` y luego `x`.

**Lo que debe verse**, en este orden:

```
  Thalyx

  ok  mounted /proc
  ok  mounted /sys
  ok  mounted /dev
  ok  mounted /run
  ok  mounted /sys/kernel/security
  ok  mounted /sys/fs/bpf
  ok  mounted /sys/fs/cgroup
  ok  store        /dev/vda — three subvolumes
  no  thalyx-lsm: /lib/thalyx/thalyx_lsm.bpf.o is not in the image

  Thalyx.

  This is the machine. There is no shell behind this and nothing
  to return to — not because it is hidden, but because it was
  never installed.
```

**Ese párrafo es lo que respondiste que querías ver.** `thalyx session` solo lo
imprime cuando su proceso padre es el pid 1 — corriéndolo en tu Fedora dice lo
contrario, y esa diferencia es cómo se comprueba que la frase no está cableada.

| Si falla | Qué significa y qué hacer |
|---|---|
| `Kernel panic - not syncing: No working init found` | El kernel no encontró o no pudo ejecutar `/init`. Casi siempre el binario no era estático de verdad. |
| Pánico inmediato sin ninguna línea de Thalyx | Falta algo del kernel: `CONFIG_BLK_DEV_INITRD`, `CONFIG_DEVTMPFS` o la consola serie. Pégame las últimas 20 líneas. |
| Arranca pero algún `no  mounted` | **No es fatal, es el diseño.** Falta esa opción en `thalyx.config`. Pégame cuáles. |
| `no  thalyx-lsm` | **Esperado.** Es el hueco conocido: el cargador invocaba `bpftool` y no hay bpftool en la imagen. Ver abajo. |
| `no  store ... neither is any other disk` | No llegó ningún disco. O el paso 4b no se hizo, o QEMU no lo adjuntó. Desde el 2026-08-03 `make run` **se niega a arrancar** sin store, así que esto solo sale con `STORELESS=1`. |
| `no  store ... The disks that are: X` | Sí hay discos y ninguno se llama como dice `thalyx.store=`. Cambia `STOREDEV` en `image/Makefile` por el que aparezca. |
| `no  store        ... is there and did not mount` | El disco está adjunto y no es un store. Casi siempre: se creó con una versión anterior, o el `mkfs` se interrumpió. Rehazlo con el paso 4b. |

## Paso 6 — que haga algo

Con el store montado, la sesión tiene un módulo instalado:

```
  > modulos
  dev.thalyx.greeter 1.0.0

  > correr dev.thalyx.greeter
```

**Esto se va a negar**, y está bien:

```
  dev.thalyx.greeter did not run: refusing to run `dev.thalyx.greeter`:
  the kernel policy map is not loaded, so none of its 1 permission(s)
  would be enforced.
```

Mientras `thalyx-lsm` no cargue, nada en la máquina puede hacer cumplir un
permiso, y Thalyx no finge que sí. Para correrlo de todos modos, sabiendo eso:

```
  > correr dev.thalyx.greeter sin-confinar
```

La palabra existe para que ese estado se alcance **porque alguien lo escribió**,
no porque el sistema se degradó solo y no lo dijo. El journal lo registra como
degradado.

**Debe imprimir** algo así:

```
dev.thalyx.greeter 1.0.0
  ran: /opt/thalyx/modules/dev.thalyx.greeter/1.0.0/bin/greeter
  ...
  dev.thalyx.greeter said:
    I am dev.thalyx.greeter 1.0.0, speaking protocol 1, holding 1 grant(s).
    read 128 byte(s) from /opt/thalyx/data/greeter/notes.txt: Thalyx es el ...
    I asked for /etc/shadow and was refused, which is correct.

  exited cleanly
```

**Eso es el sistema completo en cuatro líneas.** El módulo no sabe cómo se
llama: preguntó. No sabe qué puede leer: preguntó, porque `correr` no le pasa
ningún argumento. Pidió algo que no le concedieron y se lo negaron. Y todo eso
pasó por un socket que él no abrió, dentro de la máquina, sin shell.

Para apagar: `apagar`.

| Si falla | Qué significa y qué hacer |
|---|---|
| `modulos` dice que no hay nada | El store no se montó, o se montó vacío. Las primeras líneas del arranque dicen cuál de las dos. |
| `AND GOT IT` | El módulo leyó `/etc/shadow`. Eso es un fallo de Thalyx y manda sobre todo lo demás. |
| `I was refused /opt/thalyx/data/greeter/notes.txt` | El módulo pidió lo que sí tenía concedido y se lo negaron. Eso es un fallo del núcleo de permisos, no del store: el archivo está en el disco y el manifiesto lo nombra. |
| `nothing to read, and nothing granted` | El módulo preguntó qué podía leer y no le dieron nada. El permiso no sobrevivió a la instalación. |

## Lo que va a estar mal, y ya se sabe

**`thalyx-lsm` no se carga.** La máquina arranca y lo dice. El cargador invocaba
`bpftool`, y la imagen no lleva bpftool ni shell desde donde llamarlo —
[[Filosofia-Fundacional]] lo registra como uno de los dos decretos que su propio
texto invalida. Cargar BPF desde dentro de Thalyx es el trabajo que sigue.

**Por eso `correr` se niega.** El núcleo no arranca un módulo confinado cuando
nada puede hacer cumplir sus permisos, y en esta máquina nada puede todavía. La
salida es `correr <id> sin-confinar`, que existe para que ese estado sea una
decisión escrita y no una degradación silenciosa.

**El store sí existe ya, y el módulo también.** Los dos huecos que esta nota
listaba aquí —"no hay store" y "no hay módulo"— se cerraron el 2026-08-03. Lo
que no se ha ejercido nunca es el disco arrancando dentro de QEMU: la etapa 13
de `verify.sh` prueba el mecanismo fuera de la máquina virtual, y el paso 4b
nunca ha corrido.

Ninguno de esos huecos impide que la máquina arranque, se describa a sí misma y
corra un módulo, que es lo que este arranque tiene que demostrar.

## Lo que salió la primera vez, verbatim

2026-08-03, Fedora 43, QEMU. Después de los siete montajes y del párrafo:

```
  What I can tell you about where I am:

  ok  kernel       6.12.101
  no  filesystem   rootfs — snapshots and restore need btrfs and will not work here
  ok  cgroup v2    mounted at /sys/fs/cgroup
  ok  lsm order    capability,bpf
  no  enforcement  the policy map is not loaded, so no permission would be enforced
  no  modules      nothing installed yet

  3 are not here. I will not pretend otherwise later.

  Say what you want. `salir` to leave.
```

Los tres `no` son los tres huecos conocidos de arriba, en el mismo orden. Que la
máquina los enumere sola —y que el `ok lsm order` conviva con el `no
enforcement`— es la diferencia entre un sistema que sabe lo que le falta y uno
que reporta verde porque nadie preguntó.

**Y hay un cuarto hueco que este arranque hizo visible.** `attach_lsm` busca
`/lib/thalyx/thalyx_lsm.bpf.o`. Ese archivo no puede existir: sería el segundo
de una imagen que tiene que tener uno. El mensaje es cierto y el arreglo que
sugiere es el equivocado — el objeto BPF va incrustado en el binario.

## Qué contestar cuando llegue la salida

Por orden de prioridad:

1. **Si el paso 0 tiene `failed` > 0:** eso primero, y nada más. Una regresión
   invalida cualquier conclusión sobre lo demás.
2. **Si el paso 4 no dice `1 program(s)`:** el decreto está roto y eso manda
   sobre cualquier otra cosa que haya salido bien.
3. **Si el paso 5 imprime "This is the machine":** existe la máquina.
4. **Si el paso 6 imprime "I am dev.thalyx.greeter":** el sistema está completo
   en lo pequeño. Un módulo instalado en su propio disco, corriendo dentro de la
   máquina, preguntándole a Thalyx quién es y qué puede tocar. Lo que sigue es
   cargar el LSM desde dentro, que es lo que convierte ese `sin-confinar` en un
   `correr` a secas.
5. Todo lo demás: seguir la tabla del paso donde falló.

## Relacionado
- [[Filosofia-Fundacional]] — el decreto que esto comprueba
- [[Construccion-del-ISO]] — qué lleva la imagen y por qué
- [[Criterio-de-Salida-Fase-1]] — de qué paso es esto
- [[Punto-Actual]] — dónde quedó todo lo demás
