---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-07-31
tags: [arquitectura, core, kernel]
---

# El núcleo del sistema

Esta nota describe el **núcleo del sistema operativo**. Para el orquestador del flujo canónico, que también se llama Core, ver [[Core]].

## Especificación base

- **Kernel:** Linux, con configuración mínima propia y parches opcionales para baja latencia. **No hay distribución debajo** — ver [[Construccion-del-ISO]].
- **Tamaño:** hiperligero. Solo lo esencial para arrancar, gestionar procesos, manejar archivos y conectividad básica.
- **Init:** `thalyx` es PID 1. No systemd, no OpenRC, no busybox-run, no s6.
- **Filesystem:** Btrfs obligatorio, con subvolúmenes separados para sistema, módulos y datos de usuario. Ver [[Journal-y-Snapshots]].
- **Módulo propio en el kernel:** `thalyx-lsm`, desde la Fase 1. Ver [[Permisos-JIT]].
- **API interna:** conjunto de interfaces que permiten a los módulos interactuar con el sistema. **Diseñada en [[API-Interna-de-Modulos]]**, que es la autoridad. De la lista original de esta línea, *"ejecutar comandos"* queda anulada —no hay comandos que ejecutar— y *"acceder a hardware"* sale de la v1 por no estar decretada. Ver la revisión de abajo.
- **Seguridad base:** el núcleo no confía en ningún módulo por defecto. Sistema de capabilities donde cada módulo declara explícitamente qué recursos necesita, en su [[Formato-Manifiesto-Thmod|manifiesto]].

## Revisión del 2026-08-03 — se corrige el error de categoría y el init deja de heredarse

**Antes:** *"Base: kernel Linux minimalista (tipo Alpine)"* e *"Init system: s6 o
busybox-run, no systemd"*.

**Ahora:** el kernel es Linux con configuración propia, y PID 1 es `thalyx`.

**Motivo, y son dos.** "Kernel tipo Alpine" es un error de categoría: Alpine no
tiene kernel propio, embarca el de Linux como todos. Esa frase es de donde salió
que la bóveda creyera estar decretando un kernel cuando decretaba una distro.

Y sobre el init: las dos opciones que ofrecía eran de terceros, y `busybox-run`
es busybox, que con [[Construccion-del-ISO]] ya no puede estar en la imagen.
Dejar la elección abierta fue lo que permitió que el esqueleto de la imagen
apareciera usando **OpenRC** — una tercera respuesta que nadie eligió, llegada
por omisión desde un perfil de Alpine. Exactamente la misma forma que el login
en tty1: nadie lo decretó, lo puso la base.

## La API interna deja de ser una línea de esta nota

La especificación de arriba nombra una **API interna** desde el 31 de julio y no
existe ni en una línea de código. Podía no existir porque había un userland
POSIX debajo tapando el hueco: un módulo era un script, y hablaba con `sh`.

Ya no. Sin shell y sin utilidades, **un módulo no tiene con quién hablar excepto
Thalyx**, así que esta API pasa de ser una aspiración a ser la única superficie
que un módulo puede tocar. Es la pieza que hace que un programa escrito para
Thalyx no corra en ningún otro lado, que es el criterio de
[[Decision-Capa-vs-SO-Nuevo]].

**Diseñada el 2026-08-03 en [[API-Interna-de-Modulos]]**: un socket que Thalyx
entrega ya abierto en el descriptor 3 al ejecutar el módulo, mensajes de
longitud explícita más CBOR, y tres familias de operaciones en la v1 —archivos,
notificar al humano, y preguntar quién es—. Esa nota es la autoridad sobre la
API; esta solo la nombra.

### 2026-08-03 — "ejecutar comandos" queda anulada
**Antes:** la API interna listaba, entre sus capacidades, *"ejecutar comandos"*.

**Ahora:** no existe y no va a existir.

**Motivo.** No hay comandos que ejecutar. No hay shell, no hay utilidades, y no
hay un segundo programa en la imagen que invocar. La frase es del 31 de julio,
de cuando había un userland POSIX debajo, y sobrevivió intacta al decreto que lo
quitó. Es la tercera de esta forma —el login en tty1 y `bpftool` fueron las
otras dos—: **una capacidad que se apoyaba en la base envejeció callada cuando
la base se cayó.** Lo que un módulo habría hecho con un comando, lo hace con una
operación de esta API o no lo hace.

## Revisiones

### 2026-08-01 — Se separa del orquestador y se retira el soporte NPU de la especificación base
**Antes:** esta nota mezclaba el núcleo del sistema operativo con el rol del Core como pieza del flujo canónico, y listaba "soporte NPU" entre los parches del kernel.
**Ahora:** el orquestador tiene su propia nota ([[Core]]), y el soporte NPU sale de la especificación de Fase 1.
**Motivo:** son dos cosas distintas con el mismo nombre, y la confusión se notaba al enlazar. En cuanto al NPU: no participa de ninguna primitiva, de ningún caso trazado ni de ninguna demostración, y su ausencia no obliga a reescribir nada — es exactamente lo que el [[Criterio-de-Inclusion-de-Primitivas]] manda posponer.

## Relacionado
- [[Core]]
- [[Arquitectura-Asimetrica]]
- [[Decision-Kernel-vs-Userspace]]
- [[Construccion-del-ISO]]
- [[Sistema-de-Modulos]]
