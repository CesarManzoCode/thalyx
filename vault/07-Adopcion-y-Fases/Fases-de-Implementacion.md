---
tipo: estrategia
estado: decretado
fecha-decreto: 2026-07-31
tags: [fases, roadmap, implementacion]
---

# Estrategia de implementación (fases)

## Fase 1: Núcleo de Thalyx sobre base Alpine — primeros 6-12 meses

- **Base:** Alpine Linux minimalista, con **Btrfs obligatorio** en el volumen raíz y subvolúmenes separados para sistema, módulos y datos de usuario.
- **Componentes:**
  - `thalyx-core` — validación de contratos, resolución de versiones, verificación, commit atómico, journal. Módulos internos con fronteras duras — ver [[Core]].
  - `thalyx-lsm` — módulo de seguridad del kernel: aplica permisos e intercepta mutaciones del filesystem.
  - `thalyx-permd` — broker de permisos en userspace.
  - Índice semántico (SQLite) con parser incremental — ver [[FS-en-Grafo]] y [[Parser-Mecanico]].
  - Memoria persistente — ver [[Memoria-Persistente]].
  - Gestor de módulos: instalación, verificación, rollback.
  - `thalyx-sandbox` — aislamiento de ejecución, implementación propia.
  - `thalyx-agent` — reglas + embeddings, **modelo local exclusivamente**, sin fine-tuning y sin llamadas a la nube (ver [[Debate-Agente-Fine-Tuning]]).
  - CLI `thalyx`. **No hay interfaz gráfica ni dashboard web en Fase 1.**
  - Snapshots de Btrfs — ver [[Journal-y-Snapshots]].
- **Objetivo:** demostrar el flujo completo de punta a punta: usuario → agente → contrato → ejecución → commit → rollback. Ver [[Caso-Instalar-Modulo]].
- **Criterio de salida:** ver [[Criterio-de-Salida-Fase-1]]. Ningún otro criterio lo sustituye.

## Fase 2: Validación empírica — meses 12-18

- **Se construye:** el [[Scheduler-Predictivo|scheduler predictivo]], pospuesto desde la Fase 1.
- **Medir:** overhead del índice semántico, latencia de permisos JIT, efectividad del scheduler.
- **Benchmarks:** comparar rendimiento de las primitivas de Thalyx contra hacer las mismas operaciones sin ellas.
- **Decisión:** aplicar el umbral de [[Decision-Kernel-vs-Userspace]] (media <5% / >15%, y p99 en la zona gris).

## Fase 3: Migración al kernel (si aplica) — años 2-3

- **FS en grafo nativo:** módulo VFS, si el overhead lo justifica.
- **Scheduler semántico nativo:** módulo del scheduler del kernel, solo si el overhead es inaceptable.
- **Ampliación de `thalyx-lsm`:** el módulo ya existe desde Fase 1; aquí crece en alcance.

## Fase 4: Ecosistema — continuo

- Repositorio comunitario: crecimiento orgánico de módulos.
- Core Modules: expansión gradual.
- Documentación: publicación de guías, API y tutoriales.
- Comunidad: foros, contribuciones externas.

## Revisiones

### 2026-08-01 — Se elimina la palabra "capa" del título de la Fase 1
**Antes:** la fase se titulaba "Capa sobre Linux (userspace)", en contradicción directa con [[Decision-Capa-vs-SO-Nuevo]], que decreta que Thalyx no puede ser una capa.
**Ahora:** "Núcleo de Thalyx sobre base Alpine".
**Motivo:** Thalyx es un sistema operativo nuevo. Que la mayor parte del código corra en userspace es orden de construcción, no arquitectura. Ver la formulación completa en [[Decision-Capa-vs-SO-Nuevo]].

### 2026-08-01 — El LSM entra en Fase 1, el scheduler y la GUI salen
**Antes:** Fase 1 incluía el scheduler predictivo y una interfaz gráfica mínima con dashboard web, y listaba los permisos JIT como daemon de userspace con el LSM pospuesto a Fase 3.
**Ahora:** el LSM se escribe en Fase 1; el scheduler pasa a Fase 2; la interfaz de Fase 1 es solo CLI.
**Motivo:** el enforcement de permisos sin kernel es cooperativo y no cumple la promesa de la primitiva. El scheduler y la GUI, en cambio, no participan del caso canónico ni de las demostraciones, y posponerlos no obliga a reescribir nada. Con el LSM adentro, la Fase 1 ya es suficientemente grande.

### 2026-08-01 — Se fija Btrfs como requisito y se añade el resolver
**Motivo:** el journal, los snapshots y la demostración de rollback no existen sin snapshots de filesystem. Y el resolver de versiones, que el caso canónico necesita, no figuraba en la lista de componentes. Ver [[Resolucion-de-Versiones]].

## Relacionado
- [[Criterio-de-Salida-Fase-1]]
- [[Condiciones-de-Adopcion]] — aplican a partir de que exista audiencia, no desde Fase 1
- [[Decision-Kernel-vs-Userspace]]
- [[Construccion-del-ISO]]
- [[Tareas-Pendientes]]
