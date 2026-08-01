---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-07-31
tags: [arquitectura, core, kernel]
---

# El núcleo (Core)

## Especificación base

- **Base:** Kernel Linux minimalista (tipo Alpine), con parches opcionales para baja latencia y soporte NPU.
- **Tamaño:** Hiperligero. Solo lo esencial para arrancar, gestionar procesos, manejar archivos y conectividad básica.
- **Init system:** s6 o busybox-run (no systemd). Decisión tomada por simplicidad y control.
- **API interna:** Un conjunto de interfaces (syscalls mejoradas o capas de userspace) que permiten a los módulos IA interactuar con el sistema: leer archivos, ejecutar comandos, acceder a hardware, mostrar notificaciones, gestionar permisos.
- **Seguridad base:** El Core no confía en ningún módulo por defecto. Sistema de capabilities (permisos granulares) donde cada módulo declara explícitamente qué recursos necesita.

## El Core como pieza del flujo canónico

Además de ser el núcleo del sistema, "Core" es también una de las [[Flujo-Canonico-Overview|9 piezas fijas del flujo canónico]]: valida contratos y orquesta la ejecución. Ver esa nota para el detalle de su rol operacional.

## Relacionado
- [[Arquitectura-Asimetrica]]
- [[Decision-Kernel-vs-Userspace]]
- [[Sistema-de-Modulos]]
