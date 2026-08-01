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

## Relacionado
- [[Fases-de-Implementacion]]
- [[Criterio-de-Salida-Fase-1]]
- [[Condiciones-de-Adopcion]]
- [[Core-Nucleo]]
