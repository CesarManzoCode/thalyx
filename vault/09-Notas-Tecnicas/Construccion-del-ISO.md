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

## Qué queda por decidir, y no se hereda

La versión anterior heredó de Alpine cosas que nadie eligió — el login en tty1
fue la más visible, no la única. Lo que sigue abierto se escribe aquí para que
se decida en vez de aparecer:

- **Cómo se construye el kernel:** qué configuración mínima y de dónde salen las
  fuentes.
- **Cómo se arma la imagen:** particionado, `mkfs.btrfs` con los tres
  subvolúmenes de [[Core-Nucleo]], y si el arranque usa el EFI stub del propio
  kernel en vez de un gestor de arranque.
- **Qué hace `thalyx` como PID 1** antes de la sesión: montar `/proc`, `/sys`,
  `devtmpfs` y cgroup2, montar los subvolúmenes, cargar el LSM.
- **Qué pasa al apagar.**

## Cómo se arranca

QEMU, mientras dure la Fase 1. El paso 1 del [[Criterio-de-Salida-Fase-1]] es
arrancar la imagen; el soporte de hardware real llega cuando el sistema
justifique correr fuera de una máquina virtual.

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
