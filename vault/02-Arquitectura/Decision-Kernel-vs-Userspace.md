---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [arquitectura, kernel, userspace, decision]
---

# Decisión arquitectónica clave: ¿kernel vs userspace?

## El debate

**Postura del crítico:** abogaba por userspace (SQLite para transacciones, FUSE para FS, cgroups para scheduling), argumentando que el kernel es conservador y los context switches no son cuello de botella (<0.5% overhead).

**Postura del usuario (dueño del proyecto):** defendía que para que la IA sea realmente ciudadana de primera clase, el FS semántico y el scheduler predictivo deben ser nativos en el kernel, por pureza arquitectónica, rendimiento en el camino crítico y garantías atómicas que el userspace no puede ofrecer sin complejidad.

## Resolución final (decretada)

- El **FS en grafo** comienza como una capa de indexación en userspace (sobre ext4/btrfs). No se toca el VFS del kernel inicialmente.
- El **scheduler predictivo** es un orquestador en userspace que usa cgroups, nice y `sched_setattr`. No se toca el scheduler del kernel.
- La **memoria persistente** es una base de datos vectorial o un almacén clave-valor serializado en disco.
- Los **permisos just-in-time** sí requieren tocar el kernel (vía módulo LSM), pero es la única primitiva que lo necesita inicialmente.

## Justificación

Esta decisión reduce el riesgo de ejecución en un 90% y mantiene el 100% del valor diferencial. Si los benchmarks demuestran que el overhead de userspace es inaceptable, se migra al kernel progresivamente.

**Umbral de migración:**
- Si el overhead es **<5%** → userspace es suficiente.
- Si el overhead es **>15%** → se migra al kernel.

## Tabla de decisión final

| Primitiva | Ubicación | Justificación |
|---|---|---|
| FS en grafo | Userspace (SQLite + FUSE) | Seguridad, simplicidad, overhead aceptable en Fase 1 |
| Scheduler predictivo | Userspace (cgroups + nice) | Optimización, no dependencia crítica |
| Memoria persistente | Userspace (BD vectorial) | No requiere kernel |
| Permisos JIT | Kernel (módulo LSM) | Seguridad y rendimiento en el camino crítico |

## Por qué userspace primero (seguridad)

Empiezan en userspace por seguridad (un error en userspace mata solo el servicio, no todo el sistema) y simplicidad. Solo se migran al kernel si los benchmarks lo justifican.

## Relacionado
- [[Criterio-de-Inclusion-de-Primitivas]]
- [[FS-en-Grafo]]
- [[Fases-de-Implementacion]] — Fase 2 (validación empírica) y Fase 3 (migración al kernel)
