---
tipo: procedimiento
estado: activo
fecha-decreto: 2026-08-07
tags: [hierro, arranque, acto-2, procedimiento, fase-1]
---

# El arranque en hierro

> **Si eres una sesión nueva y Cesar te está pegando la salida de un comando con
> una memoria USB de por medio: es de aquí.** Los comandos en orden, lo que cada
> uno debe imprimir, y qué significa cada fallo.
>
> **Corrido el 2026-08-07, y salió.** Un firmware real arrancó Thalyx de una
> memoria física: la pantalla salió por el framebuffer de ese firmware, la memoria
> apareció como `/dev/sdb2`, el store se encontró por la etiqueta, el LSM se
> enganchó y el teclado físico funcionó. Lo que queda abajo ya no es un pronóstico.
>
> **Y encontró un defecto**, que está en el paso 8. La razón por la que esta nota
> existe aparte de [[Primer-Arranque]] sigue en pie: aquí hay una máquina real con
> un sistema operativo real encima, y el paso que se teclea mal no se deshace.

## Qué se está intentando, y qué NO

Cesar tiene **una sola PC**, con Fedora, y no hay una segunda máquina limpia.
Así que el acto 2 se parte en dos mitades con costos muy distintos, y esta nota
sólo cubre la primera:

| | Qué responde | Qué escribe |
|---|---|---|
| **Arrancar desde la USB** — esta nota | Firmware real, xHCI real, teclado real, `USB_STORAGE` real, y el NVMe real visto por el driver real | **Nada.** Ni un byte en el disco interno |
| **Instalar en el disco interno** | Escribir de verdad en un NVMe físico | **Destruye Fedora.** Ver abajo |

**La segunda mitad no se puede hacer con el instalador que existe.**
`thalyx install` escribe una GPT nueva sobre el disco entero — el módulo abre
diciendo *«turning a disk with no operating system on it into a Thalyx
machine»*, y es literal. Instalar al lado de Fedora es una capacidad que **no
está construida** y es un decreto que Cesar no ha tomado.

Lo que sí queda respondido arrancando: todo lo demás. Y no es poco — al
2026-08-07 los cuatro grupos de controladores ya se ejercieron contra
controladores emulados (`make -C image run-hardware`), así que lo único que
queda por descubrir aquí es **silicio concreto**.

## El peligro, dicho antes que los comandos

Dentro de la sesión, con la máquina arrancada de la USB, **`discos` va a listar
el NVMe con Fedora adentro**. `instalar-en /dev/nvme0n1` lo borraría entero y
sin vuelta.

- **Seguros**: `discos`, `estado`, `nucleo`, `modulos`, `permisos`, `recuerdos`,
  `apagar`. Todos leen.
- **El único destructivo es `instalar-en`.** No se teclea en esta máquina.

Thalyx pide teclear la ruta del dispositivo en vez de una `y`, precisamente por
esto. Pero la confirmación protege del error de tipeo, no del comando correcto
tecleado en la máquina equivocada.

## Paso 1 — una imagen del tamaño de una memoria

El disco por omisión son 8 GiB, y escribir 8 GiB a una USB2 es cerca de una
hora. El mínimo que el instalador acepta son **642 MiB** —1 MiB de alineación,
512 MiB de ESP, 128 MiB del store más chico que Thalyx escribe, y 1 MiB al final
para la segunda copia de la tabla—, así que 2 GiB sobra y entra en cualquiera de
las tres memorias.

```sh
cd ~/thalyx
git pull
sudo make -C image installed INSTALLEDSIZE=2G
```

Debe terminar diciendo `installed: .../image/build/installed.img` y repetir la
línea de `dd`. No construye nada: si falta el kernel, lo dice y se detiene.

> **Si dice `is N bytes, and installing needs at least ...`** el tamaño quedó por
> debajo del mínimo. Sube `INSTALLEDSIZE`.

## Paso 2 — saber cuál es la memoria, que es el paso que se teclea mal

**Con la memoria desconectada:**

```sh
lsblk -o NAME,SIZE,TYPE,TRAN,MOUNTPOINTS
```

Anota lo que hay. Ahora conéctala y corre exactamente lo mismo. **El dispositivo
nuevo es el único que puede ir en `of=`**, y la columna `TRAN` debe decir `usb`.

El disco interno es `nvme0n1`. **Nunca es el destino.** Un NVMe no aparece como
`usb` en esa columna, y ésa es la comprobación que separa las dos.

## Paso 3 — desmontar lo que GNOME haya montado solo

```sh
lsblk -o NAME,MOUNTPOINTS /dev/sdX
sudo umount /dev/sdX?*
```

`umount` puede quejarse de que no estaba montado, y eso está bien. Lo que no
puede quedar es una partición de esa memoria con un punto de montaje.

## Paso 4 — escribir

```sh
sudo dd if=image/build/installed.img of=/dev/sdX bs=4M status=progress conv=fsync
```

**`of=/dev/sdX`, el disco entero, no `/dev/sdX1`.** El primer byte de la imagen
es el MBR protector y tiene que caer en el primer byte del dispositivo; escrito
en una partición, el firmware no encuentra nada y el fallo se ve como *«la ISO
no arranca»*.

## Paso 5 — comprobar que lo escrito volvió

Este proyecto ya tiene la regla de que **una reparación que silenciosamente no
funcionó se ve idéntica al bug original**, y aquí aplica igual: una escritura
que se quedó en el caché y no llegó produce una memoria que no arranca, que se
lee como Thalyx fallando.

```sh
sudo cmp -n $(stat -c %s image/build/installed.img) image/build/installed.img /dev/sdX \
  && echo "identico"
```

Tiene que imprimir `identico` y nada más. Si dice `differ: byte N`, la escritura
no llegó entera y **no hay que arrancar nada** hasta repetirla.

## Paso 6 — Secure Boot, que es lo más probable que lo detenga

El kernel de Thalyx **no está firmado**. Una Fedora de fábrica trae Secure Boot
activado, y con él activado el firmware se niega a arrancar la memoria. **Eso no
es Thalyx fallando** y el síntoma no lo dice: según el firmware, es un mensaje de
violación de seguridad, o simplemente que la memoria no aparece como opción.

1. Reiniciar y entrar a la configuración del firmware — `F2`, `Supr`, `F10` o
   `Esc` según el fabricante, apretado apenas enciende.
2. Buscar **Secure Boot** —suele estar en *Security* o en *Boot*— y ponerlo en
   *Disabled*.
3. Guardar y salir.

**Fedora arranca igual con Secure Boot apagado.** Es una restricción que se
quita, no una firma que se rompe, y se puede volver a activar después.

## Paso 7 — arrancar desde la memoria

Reiniciar y apretar la tecla del **menú de arranque de una sola vez** — `F12` en
la mayoría, `F9` en HP, `Esc` en algunas. Elegir la entrada que dice `UEFI:` y el
nombre de la memoria.

**Elegir el menú de una sola vez y no cambiar el orden de arranque permanente.**
Si algo sale mal, apagar y encender devuelve la máquina a Fedora sin haber tocado
nada.

### Lo que debe salir, y qué significa cada línea

Lo mismo que salió en la VM el 2026-08-07, pero con hardware real detrás:

```
xhci_hcd 0000:...: xHCI Host Controller       tu controlador USB real
input: ... Keyboard ... on usb-...            tu teclado real, por USB
usb-storage ...: USB Mass Storage device      la memoria es un disco
ok  store   /dev/sdX2 ▪ three subvolumes, found by the label `thalyx-store`
Thalyx.
```

| Si falla | Qué es |
|---|---|
| El firmware no lista la memoria | Secure Boot sigue activo, o el paso 4 escribió en una partición |
| Arranca y la pantalla queda negra | `FB_EFI` no adoptó el framebuffer de esta máquina. Es lo único de la pantalla que la VM no podía responder |
| Se ve el prompt y el teclado no responde | xHCI o HID contra este chipset. La VM dijo que los drivers enlazan, no que enlacen contra **este** silicio |
| `no store` | La memoria arrancó pero el kernel no la ve como disco: `USB_STORAGE`. La VM ya lo respondió, así que aquí sería nuevo |

## Paso 8 — qué preguntarle a la máquina

```
discos
```

**Es la línea que importa.** Tiene que listar la memoria **y** el NVMe real de la
máquina, con su tamaño verdadero. Que Thalyx vea un NVMe físico es la mitad del
acto 2 que ninguna VM responde.

```
nucleo
estado
apagar
```

Y **no** `instalar-en`. Ver el aviso de arriba.

### Lo que apareció el 2026-08-07, y qué hacer con ello

```
> [ 51.812474] usb 1-6: device descriptor read/64, error -110
```

`-110` es `ETIMEDOUT`: hay un dispositivo USB en el bus 1, puerto 6, cuyo
descriptor el kernel no puede leer. **Reintenta para siempre**, así que el
mensaje vuelve cada pocos segundos encima del prompt y la sesión queda inusable
aunque el teclado funcione. Los 38 segundos que tardó en aparecer el store son el
mismo síntoma: la enumeración se pasó ese rato agotando plazos.

**No es que el teclado no sirva**, y la forma de separarlo es la que Cesar usó:
arrancar otra vez y teclear **antes** de que aparezca el error. Si eso funciona
—y funcionó—, el teclado está bien y lo que estorba es el ruido.

Antes de tocar nada del kernel, **desde Fedora y sin riesgo**:

```sh
lsusb -t
sudo dmesg | grep -i "1-6"
```

Eso dice qué dispositivo es. Hasta saberlo no se agrega ninguna opción: podría
ser un hueco de `thalyx.config` o podría ser un dispositivo defectuoso, y las dos
se ven igual desde adentro de Thalyx. Es la regla 5 — antes de creerle a lo que
el sistema dice, descartar al que preguntó.

Y un atajo mientras tanto: **desconectar todo lo que no haga falta** —hubs,
cámaras, lectores de tarjetas, controles— dejando sólo el teclado y la memoria.
Si el error desaparece, el dispositivo era uno de los que se quitaron y queda
identificado por eliminación.

## Paso 9 — el acto 2b, con dos memorias en vez de un disco interno

**Decidido por Cesar el 2026-08-07**, y es lo que cierra la Fase 1. No hay una
segunda PC ni un disco interno que se pueda sacrificar, pero sí hay tres
memorias, y eso alcanza para lo que el criterio pide: que la máquina **tenga**
Thalyx en un disco propio y arranque de él con el medio quitado.

**Las dos memorias van puestas a la vez.** La A es de la que se arrancó — el
medio, de donde sale el kernel — y la B es el destino.

### El único paso peligroso de todo este documento

Dentro de la sesión, `discos` va a listar **tres** discos enteros, y uno de
ellos es la Fedora:

```
  /dev/sd?      3 GiB     ← la memoria A, con `a Thalyx boot partition`
  /dev/sd?     29 GiB     ← la memoria B, el destino
  /dev/sd?    447 GiB     ← FEDORA. Lleva `btrfs `fedora`` en una partición
```

**El discriminador es el tamaño, y la etiqueta lo confirma.** Ningún disco de
447 GiB es una memoria USB, y ninguna memoria dice `btrfs \`fedora\``. Las letras
(`sda`, `sdb`, `sdc`) **cambian de un arranque a otro** según el orden en que el
kernel enumera, así que la letra de ayer no sirve hoy.

Desde el 2026-08-07 `instalar-en` **dice qué hay en el disco antes de preguntar**,
leyéndolo del disco y no de la lista, y sigue exigiendo teclear la ruta completa
en vez de una `y`. Si en esa pantalla aparece `btrfs \`fedora\``, la respuesta es
no escribir nada.

### Los comandos

```
discos                     mirar los tres, identificar B por su tamaño
instalar-en /dev/sdX       X = la memoria B, NUNCA el disco de 447 GiB
```

Leer lo que imprime. Tiene que decir que el kernel viene de la memoria A, y que
el disco de destino tiene lo que sea que tenga la B — **no** `btrfs \`fedora\``.
Sólo entonces teclear la ruta para confirmar.

```
apagar
```

Quitar la memoria A. Encender. El firmware tiene que arrancar la B.

### Por qué hay que quitar la A y no es opcional

Con las dos puestas hay **dos** volúmenes etiquetados `thalyx-store`, y PID 1 se
niega en vez de elegir — que es correcto y está decretado. El arranque diría que
encontró dos y se detendría. Quitar la A no es higiene, es la condición.

### Qué queda sin responder si esto sale bien

**Que el disco sea interno.** Una memoria USB es un disco físico real, con una
GPT real escrita por Thalyx, particiones creadas por el kernel real y un firmware
real arrancando de ella — pero llega por otro transporte. El camino del
instalador es idéntico (sysfs, `BLKRRPART`, los mismos bytes); lo que cambia es
el bus.

Queda escrito aquí en vez de redondearse, porque es la misma advertencia que esta
nota lleva desde el principio, un nivel más adentro.

## Lo que queda sin responder aunque todo salga bien

**Escribir en un NVMe físico.** Lo ejerció la VM de punta a punta contra un NVMe
emulado —GPT, ESP en FAT32, store Btrfs, y un firmware arrancando de ahí sin
medio puesto— y en hierro no se puede sin destruir la única máquina que verifica
este proyecto.

Así que el acto 2 queda **respondido salvo por esa mitad**, y eso se escribe tal
cual en [[Criterio-de-Salida-Fase-1]] en vez de redondearse hacia arriba. Lo que
no puede pasar es que arrancar desde la USB se reporte como haber instalado.

## Devolver la memoria a lo que era

Nada de esto tocó el disco interno, así que no hay nada que devolver ahí. La
memoria queda con la GPT de Thalyx y aparentando 2 GiB:

```sh
sudo wipefs -a /dev/sdX
```

y después formatearla normal desde GNOME Discos.

## Relacionado
- [[Criterio-de-Salida-Fase-1]]
- [[Construccion-del-ISO]]
- [[Primer-Arranque]]
- [[Estrategia-de-Pruebas]]
