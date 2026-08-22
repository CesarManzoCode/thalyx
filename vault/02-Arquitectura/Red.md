---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-23
tags: [terminal, red, hardware, fase-2, doble-ruta]
---

# Red: verla antes de usarla

Es el punto 8 de la terminal usable de [[Tareas-Pendientes]], y el último de los
nueve. Decidido por Cesar el 2026-08-23: **Thalyx aprende a ver la red y no a
usarla.**

## De qué se partía

De las 110 opciones del kernel en `image/thalyx.config`, **ninguna era una
tarjeta de red.** `CONFIG_NET`, `CONFIG_INET` y `CONFIG_UNIX` estaban ahí, y la
sección lleva su propio comentario diciendo por qué:

```
# --- networking, only so a module can be denied it ---
```

El LSM necesita que exista una pila de red para poder negarle el acceso a un
módulo. Nada más. Una máquina Thalyx instalada no tenía cómo ver una tarjeta,
porque no había driver de ninguna.

## Por qué no se hizo todo de una vez

La diferencia entre ver la red y usarla no es de grado en este sistema, y la
razón es el decreto fundacional: **la imagen lleva el kernel y un programa.**

En Linux, obtener una dirección y salir a internet son programas aparte —un
cliente DHCP, un resolvedor de DNS, una biblioteca de TLS—. Aquí no puede haber
programas aparte. Todo eso tendría que vivir **dentro de `thalyx`**, escrito
desde cero, y probado contra servidores de verdad y no contra fixtures, que es
justamente la regla 6.

Y hay algo antes que eso: **para qué.** Salir a internet sirve si el store puede
traer módulos de algún lado, y de dónde los trae es la pregunta central de la
Fase 2, que no está contestada. Escribir DHCP hoy sería construir la mitad de un
puente hacia un lugar que todavía no se decide.

## Lo que sí se decreta

Un verbo, `red`, que contesta lo mismo que `discos` contesta de los discos: **qué
hay, y qué se sabe de cada uno.** Nombre, tipo, dirección física, si el kernel lo
tiene levantado, si hay cable, a qué velocidad negoció, y qué driver lo maneja.

Con dos caras como todo lo demás, y sin ninguna promesa de que se pueda mandar un
paquete.

## Las tres cosas que se miden y no se citan

Todo lo que sigue salió de leer `/sys/class/net` en una máquina viva, no de un
encabezado ni de una página de manual.

1. **`type` es un número, y dos valores importan.** `1` es Ethernet y `772` es
   loopback. Es la constante `ARPHRD_*` del kernel.

2. **Una interfaz apagada no dice que no tiene cable: no dice nada.** Leer
   `carrier` de una interfaz que el kernel tiene abajo devuelve `EINVAL`, no `0`.
   Son dos hechos distintos —*no hay cable* y *no se puede saber*— y confundirlos
   es exactamente la regla 10. Lo mismo con `speed`.

3. **`speed` tiene tres estados, no dos.** Un número, `-1` cuando el enlace está
   arriba y la velocidad no se conoce, y no legible cuando la interfaz está
   abajo. Imprimir `-1 Mb/s` sería inventar una medición.

Una tarjeta WiFi se reconoce por el enlace `phy80211`, **no por `type`**: en modo
normal una WiFi también dice `1`, porque le presenta al sistema una interfaz
Ethernet. Reconocerla por el tipo la haría pasar por cable.

## Lo que esto no hace, dicho aquí para que nadie lo suponga

- **No hay dirección IP.** Ni estática ni por DHCP.
- **No se manda ni se recibe un paquete.** `red` lee y no escribe.
- **No hay WiFi.** La pila 802.11 necesita firmware binario en la imagen y una
  autenticación WPA que en todos lados es un demonio aparte. Meter firmware
  obliga a revisar qué quiere decir exactamente «el kernel y un programa», y esa
  revisión no se hace de pasada.
- **Nada de esto acerca la instalación de módulos desde otro lugar.** Eso es Fase
  2 y sigue sin decidirse.

## Los drivers que entran, y por qué esos cuatro

Ver una tarjeta necesita un driver de esa tarjeta. Se agregan los que cubren lo
que esta máquina puede encontrarse, y **cada uno se justifica o no entra** — la
configuración tiene 110 opciones porque cada una tuvo que ganarse el lugar.

Ver [[Estado-de-Implementacion]] para la lista tal como quedó.

## Relacionado

- [[Filosofia-Fundacional]] — el kernel y un programa, que es lo que hace cara la
  segunda mitad.
- [[Tareas-Pendientes]] — el punto 8 y lo que queda de Fase 2.
- [[Estrategia-de-Pruebas]] — la regla 10, que es la que da forma a este verbo.
