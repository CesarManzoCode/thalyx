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
> Nada de lo que está entre el paso 2 y el paso 6 se ha ejecutado jamás. Se
> escribió en un contenedor sin red a kernel.org, sin QEMU y sin el target de
> musl. **Que algo falle ahí es lo esperado, no una sorpresa**, y cada fallo
> tiene su arreglo escrito abajo.

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
proven      ~53
not proven  2
failed      0
```

Las **dos** `not proven` esperadas son:

1. El modelo del agente — `llama.cpp` no está instalado y la ruta real no existe.
2. La imagen — nunca ha arrancado.

| Si falla | Qué significa |
|---|---|
| `failed` > 0 | Regresión real. La etapa lo dice; el log queda en el directorio temporal que imprime. Esto se arregla antes de seguir. |
| `not proven` > 2 | Algo que antes se comprobaba dejó de poder comprobarse. Mira cuáles se nombran al final: casi siempre es una herramienta que falta (`bpftool`, `clang`, `btrfs`). |
| `463 tests` sale distinto | Si es menos, algo dejó de compilarse. Si es más, alguien agregó pruebas y está bien. |

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
falta para que `thalyx-lsm` pueda cargarse contra este kernel.

Tarda. Descarga ~140 MB y compila con todos los núcleos.

| Si falla | Qué significa y qué hacer |
|---|---|
| Pide opciones que no están en `thalyx.config` | **Esperado.** Ese archivo nunca se compiló. Cada opción que pida es una línea que debió tener. Pégame lo que dice: agregarla es el arreglo correcto, no un parche. |
| `pahole: command not found` | Falta `dwarves`. |
| Errores de `flex`/`bison`/`bc` | Falta alguna de las de arriba; el mensaje dice cuál. |
| Descarga falla | Cambia `KVERSION` en `image/Makefile` por una versión que exista en `cdn.kernel.org/pub/linux/kernel/v6.x/`. |

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

## Lo que va a estar mal, y ya se sabe

**`thalyx-lsm` no se carga.** La máquina arranca y lo dice. El cargador invocaba
`bpftool`, y la imagen no lleva bpftool ni shell desde donde llamarlo —
[[Filosofia-Fundacional]] lo registra como uno de los dos decretos que su propio
texto invalida. Cargar BPF desde dentro de Thalyx es el trabajo que sigue.

**No hay store.** El disco `store.qcow2` se crea vacío y PID 1 monta subvolúmenes
que nadie creó todavía. Instalar un módulo no va a funcionar en este arranque.

**No hay módulo.** `dev.thalyx.hola` se borró porque era un script de shell.
El siguiente se escribe contra la API interna, que tampoco existe.

Ninguna de las tres impide que la máquina arranque y se describa a sí misma, que
es lo que este primer arranque tiene que demostrar.

## Qué contestar cuando llegue la salida

Por orden de prioridad:

1. **Si el paso 0 tiene `failed` > 0:** eso primero, y nada más. Una regresión
   invalida cualquier conclusión sobre lo demás.
2. **Si el paso 4 no dice `1 program(s)`:** el decreto está roto y eso manda
   sobre cualquier otra cosa que haya salido bien.
3. **Si el paso 5 imprime "This is the machine":** existe la máquina. Lo que
   sigue es cargar el LSM desde dentro y la API de módulos.
4. Todo lo demás: seguir la tabla del paso donde falló.

## Relacionado
- [[Filosofia-Fundacional]] — el decreto que esto comprueba
- [[Construccion-del-ISO]] — qué lleva la imagen y por qué
- [[Criterio-de-Salida-Fase-1]] — de qué paso es esto
- [[Punto-Actual]] — dónde quedó todo lo demás
