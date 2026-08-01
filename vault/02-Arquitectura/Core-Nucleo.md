---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-07-31
tags: [arquitectura, core, kernel]
---

# El núcleo del sistema

Esta nota describe el **núcleo del sistema operativo**. Para el orquestador del flujo canónico, que también se llama Core, ver [[Core]].

## Especificación base

- **Base:** kernel Linux minimalista (tipo Alpine), con parches opcionales para baja latencia.
- **Tamaño:** hiperligero. Solo lo esencial para arrancar, gestionar procesos, manejar archivos y conectividad básica.
- **Init system:** s6 o busybox-run, no systemd. Decisión tomada por simplicidad y control.
- **Filesystem:** Btrfs obligatorio, con subvolúmenes separados para sistema, módulos y datos de usuario. Ver [[Journal-y-Snapshots]].
- **Módulo propio en el kernel:** `thalyx-lsm`, desde la Fase 1. Ver [[Permisos-JIT]].
- **API interna:** conjunto de interfaces que permiten a los módulos interactuar con el sistema: leer archivos, ejecutar comandos, acceder a hardware, mostrar notificaciones, gestionar permisos.
- **Seguridad base:** el núcleo no confía en ningún módulo por defecto. Sistema de capabilities donde cada módulo declara explícitamente qué recursos necesita, en su [[Formato-Manifiesto-Thmod|manifiesto]].

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
