---
tipo: caso-de-uso
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, caso-de-uso, fallo, rollback]
---

# Caso de fallo: hash inválido en verificación

Trazado del camino de fallo sobre el mismo caso base que [[Caso-Instalar-Modulo]], usando la rama de [[Ramas-de-Fallo|Rollback]].

## Trazado

```
... pasos 1-10 iguales al camino feliz (ver Caso-Instalar-Modulo) ...

11. Core: verifica hash → FALLA
12. Core: NO publica (no hay nada que deshacer)
13. Core: registra en Journal (fallo de verificación)
14. Core: notifica al usuario "El módulo no pasó la verificación, no se instaló"
```

## Por qué este caso es simple (y esa simplicidad es la prueba de que build-then-commit funciona)

Con la arquitectura de [[Fase-Commit-Atomico|build-then-commit]], este es el caso de fallo **más simple posible**: el fallo se detecta en la verificación, antes de cualquier intento de commit. No hay archivos parciales en el sistema oficial porque el Sandbox nunca escribió ahí — todo el trabajo fallido queda contenido en `/tmp/build/...`, que puede simplemente descartarse.

Esto es exactamente la consecuencia que se buscaba al decretar build-then-commit: el rollback deja de ser "deshacer una operación a medias" y pasa a ser "no hubo commit".

## Caso no cubierto todavía: fallo durante el commit mismo

Este trazado no cubre el caso donde el fallo ocurre **durante** el commit (ej. la máquina pierde energía a mitad de un `rename`). Con build-then-commit esto es mucho menos probable que sea un problema real, porque `rename` es atómico a nivel de syscall en Linux dentro del mismo filesystem — pero depende de ese supuesto técnico. Ver la nota de atomicidad en [[Fase-Commit-Atomico]].

Si el commit llegara a cruzar filesystems o dispositivos, esa atomicidad ya no está garantizada gratis y este caso necesitaría un trazado propio.

## Relacionado
- [[Caso-Instalar-Modulo]]
- [[Ramas-de-Fallo]]
- [[Fase-Commit-Atomico]]
