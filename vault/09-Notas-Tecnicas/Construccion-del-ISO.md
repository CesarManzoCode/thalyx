---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-01
tags: [iso, build, qemu, fase-1]
---

# Construcción de la imagen del sistema

## Decreto

La imagen se construye con las herramientas nativas de Alpine (`mkimage` y apkovl), con dos objetivos en el Makefile:

- `make iso` — construye la imagen.
- `make run` — la arranca en QEMU.

## Qué trae la imagen

- Base Alpine minimalista.
- **Disco Btrfs con los subvolúmenes ya creados**: sistema, módulos y datos de usuario. Es requisito, no configuración opcional — ver [[Fases-de-Implementacion]] y [[Journal-y-Snapshots]].
- `thalyx-lsm` compilado y cargado en el arranque.
- El resto de los componentes de Fase 1.
- Un repositorio local de módulos con al menos un módulo firmado, para que el [[Criterio-de-Salida-Fase-1|criterio de salida]] sea ejecutable sin conexión a internet.

## Por qué Alpine y no otra base

Coherencia con la base ya decretada en [[Core-Nucleo]], y tamaño: la imagen más chica es la que permite iterar más rápido, que es lo que la Fase 1 necesita.

Se evaluaron dos alternativas. **Debian live-build** es más maduro y trae mucho mejor soporte de hardware, pero implica una base considerablemente más grande que la decretada. **Nix** daría builds reproducibles bit a bit, que es un argumento serio para un proyecto que aspira a publicar resultados, pero suma una curva de aprendizaje entera a un proyecto que ya tiene demasiadas piezas nuevas a la vez.

## Qué se pospone

**Live USB y dual-boot** son requisitos de [[Condiciones-de-Adopcion]], que aplican a la fase de apertura a usuarios externos, no a la Fase 1. En Fase 1 no hay audiencia: se arranca en QEMU porque permite iterar más rápido, y el soporte de hardware real llega cuando el sistema justifique correr fuera de una máquina virtual.

## Revisión del 2026-08-03 — el login que nadie decretó

Al escribir el esqueleto de la imagen apareció que **una base Alpine pone un
getty en tty1 si nadie dice lo contrario**: un login, y detrás un shell. Nadie
lo decretó nunca. Es lo que la base regala por omisión.

Y aceptarlo habría roto [[Decision-Capa-vs-SO-Nuevo]] en el artefacto mientras
lo cumplía en el papel:

> Thalyx es **dueño del arranque**, del sistema de módulos, de la política de
> permisos y de los requisitos de filesystem.

A un sistema al que se llega identificándose ante la sesión de otro y
ejecutando un comando no es dueño de ningún arranque. Así que **la imagen no
lleva getty ni shell**. No escondidos, no detrás de una bandera: no instalados.

Eso además es lo que contesta "¿esto es un SO o es un programa?", y lo contesta
sin argumentar: en Linux siempre estás *en Linux corriendo un programa*, y
siempre hay una salida. Aquí salir no tiene a dónde ir.

`thalyx session` es lo que init arranca, y **solo dice que es la máquina cuando
lo es** — corrido en una máquina de desarrollo reporta que algo más la arrancó,
que es cómo se comprueba que la frase no es decoración. Todo lo que muestra se
lee vivo, y distingue *no está* de *no lo pude comprobar*.

### Lo que sigue heredado y hay que decidir

Mismo error que el login, una capa más abajo:

- **El init system.** [[Core-Nucleo]] decreta "s6 o busybox-run, no systemd" y
  deja la elección abierta. El esqueleto usa OpenRC porque es lo que da un
  perfil de Alpine — una tercera respuesta que nadie eligió.
- **El layout de subvolúmenes lo crea el Makefile, no un instalador.** La Fase 1
  arranca una máquina ya preparada. Quien escriba el instalador hereda esa
  decisión.

## Relacionado
- [[Fases-de-Implementacion]]
- [[Criterio-de-Salida-Fase-1]]
- [[Condiciones-de-Adopcion]]
- [[Core-Nucleo]]
