---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [arquitectura, kernel, userspace, decision]
---

# Decisión arquitectónica clave: ¿kernel vs userspace?

## El debate

**Postura del crítico:** abogaba por userspace (SQLite para transacciones, FUSE para FS, cgroups para scheduling), argumentando que el kernel es conservador y los context switches no son cuello de botella (<0.5% overhead).

**Postura del dueño del proyecto:** defendía que para que la IA sea realmente ciudadana de primera clase, el FS semántico y el scheduler predictivo deben ser nativos en el kernel, por pureza arquitectónica, rendimiento en el camino crítico y garantías atómicas que el userspace no puede ofrecer sin complejidad.

## Resolución final (decretada)

- El **FS en grafo** comienza como una capa de indexación en userspace, sobre Btrfs. No se toca el VFS del kernel inicialmente.
- El **scheduler predictivo** es un orquestador en userspace que usa cgroups, `nice` y `sched_setattr`. No se toca el scheduler del kernel. Queda pospuesto a Fase 2 — ver [[Fases-de-Implementacion]].
- La **memoria persistente** es una base de datos vectorial local.
- Los **permisos just-in-time** requieren tocar el kernel, mediante `thalyx-lsm`. Es la única primitiva que vive en el kernel en Fase 1.

## Justificación

Mantener tres de las cuatro primitivas en userspace reduce el riesgo de ejecución sin renunciar al valor diferencial: un error en userspace mata un servicio, no el sistema entero. La excepción son los permisos, donde el enforcement solo es real si vive en el kernel.

**Umbral de migración:**
- Overhead medio **<5%** → userspace es suficiente.
- Overhead medio **>15%** → se migra al kernel.
- **Entre 5% y 15%** → decide la **latencia p99 de operaciones interactivas**: si p99 se mantiene por debajo de 50 ms, permanece en userspace; si la supera, migra.

Toda medición de overhead debe reportar media y p99. Reportar solo la media está prohibido.

## Tabla de decisión final

| Primitiva | Ubicación | Justificación |
|---|---|---|
| FS en grafo | Userspace (SQLite) | Seguridad, simplicidad, overhead aceptable en Fase 1 |
| Scheduler predictivo | Userspace (cgroups + nice), Fase 2 | Optimización, no dependencia crítica |
| Memoria persistente | Userspace (BD vectorial) | No requiere kernel |
| Permisos JIT | Kernel (`thalyx-lsm`, programa BPF LSM) + broker en userspace (`thalyx-permd`) | Sin kernel, los permisos son solo cooperativos |

## Revisiones

### 2026-08-01 — Se elimina FUSE de la Fase 1
**Antes:** la tabla situaba el FS en grafo en "Userspace (SQLite + FUSE)", pero [[FS-en-Grafo]] no mencionaba FUSE y [[Tareas-Pendientes]] preguntaba si alcanzaba. Tres estados distintos del mismo componente.
**Ahora:** en Fase 1 el grafo es un índice consultable por API y CLI, sin montaje FUSE.
**Motivo:** FUSE solo paga cuando se quiere que programas no modificados naveguen el grafo como carpetas. En Fase 1 el único consumidor es el agente, que usa la API. A cambio traería ciclo de vida de montaje, deadlocks bajo presión de memoria y una semántica de permisos duplicada.

### 2026-08-01 — Se cierra la zona gris del umbral de migración
**Antes:** el umbral definía <5% y >15%, sin decretar qué ocurre en medio — que es justo donde caen los números reales.
**Ahora:** entre 5% y 15% decide la latencia p99 de operaciones interactivas, con corte en 50 ms.
**Motivo:** el overhead medio es la métrica equivocada para esta decisión. Un 8% repartido de forma uniforme es imperceptible; el mismo 8% concentrado en picos de cientos de milisegundos hace que el sistema se sienta roto. La migración al kernel es la decisión más cara e irreversible del roadmap y merece un criterio que mida lo que el usuario percibe.

### 2026-08-01 — El LSM se adelanta a la Fase 1, y pasa a ser BPF LSM
**Antes:** la tabla lo daba por hecho desde el inicio; [[Fases-de-Implementacion]] lo posponía a Fase 3. Y ambos lo describían como un módulo cargable, que no existe en Linux mainline.
**Ahora:** `thalyx-lsm` se escribe en Fase 1 como programa BPF LSM. Ver el detalle en [[Permisos-JIT]].

## Relacionado
- [[Criterio-de-Inclusion-de-Primitivas]]
- [[FS-en-Grafo]]
- [[Permisos-JIT]]
- [[Fases-de-Implementacion]] — Fase 2 (validación empírica) y Fase 3 (migración al kernel)
