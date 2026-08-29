---
tipo: estrategia
estado: decretado
fecha-decreto: 2026-07-31
tags: [fases, roadmap, implementacion]
---

# Estrategia de implementación (fases)

## Fase 1: El núcleo de Thalyx, sin distribución debajo — primeros 6-12 meses

- **Base:** ninguna. El kernel de Linux y el binario `thalyx`, y nada más — ver [[Construccion-del-ISO]]. **Btrfs obligatorio** en el volumen raíz, con subvolúmenes separados para sistema, módulos y datos de usuario.
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

### 2026-08-28 — Se reordena la prioridad de esta etapa; las fases no cambian
**Antes:** después de la Fase 1 el trabajo avanzaba hacia habitabilidad general
—que Thalyx pudiera ser la máquina completa de alguien— con los benchmarks de
primitivas colgados de la Fase 2 y el ecosistema de la Fase 4.
**Ahora:** la prioridad operativa es **demostrar y medir que Thalyx mejora el
trabajo de agentes reales**, y el trabajo de habitabilidad general que no sirva
a esa meta queda **diferido**. Las fases siguen escritas como estaban y ninguna
se cancela. Ver [[Prioridad-Operativa]].
**Motivo:** el puente de [[Agentes-Externos]] hizo posible por primera vez
comparar el mismo modelo sobre las dos superficies, y las primeras corridas
—[[Evidencia-de-Agentes]]— dieron evidencia cuantitativa de una ventaja fuerte
en comprensión de código. Validar y maximizar esa ventaja es más barato y más
medible que seguir ensanchando la máquina, y **la evidencia caduca**: se mide
con los agentes que existen hoy.
**Lo que esto no dice:** que la prioridad anterior fuera un error. Sin el núcleo
de la Fase 1 no había primitivas que medir, y sin la habitabilidad mínima de
agosto —pantalla, teclado, motor— no había máquina donde medirlas.
**Consecuencia sobre estas fases:** los benchmarks que la Fase 2 pone en los
meses 12-18 **se adelantan en parte** — los de trabajo de agentes ya están
corriendo; los de overhead de las primitivas contra [[Decision-Kernel-vs-Userspace]]
siguen donde estaban. El [[Scheduler-Predictivo]] no se adelanta y la Fase 4
tampoco.

### 2026-08-27 — La pantalla vuelve, porque la razón para posponerla se venció
**Antes:** la interfaz de la Fase 1 es sólo CLI, y la GUI queda pospuesta.
**Ahora:** existe [[La-Pantalla]], decretada por Cesar el 2026-08-27, y se
construye.
**Motivo:** el aplazamiento del 2026-08-01 se justificó en que la GUI *«no
participa del caso canónico ni de las demostraciones, y posponerla no obliga a
reescribir nada»*. Era cierto y era una razón **condicionada a que la Fase 1 no
estuviera terminada**. La Fase 1 cerró el 2026-08-07 y el aplazamiento siguió
vivo veinte días por inercia, que es exactamente cómo un decreto se vuelve una
historia sobre una versión anterior del proyecto. Palabras de Cesar al
reabrirlo: *«actualmente estamos haciendo cosas a ciegas […] no basta con
comandos de terminal para verlo, necesito tenerlo de verdad y usarlo de
verdad»*.


### 2026-08-01 — Se elimina la palabra "capa" del título de la Fase 1
**Antes:** la fase se titulaba "Capa sobre Linux (userspace)", en contradicción directa con [[Decision-Capa-vs-SO-Nuevo]], que decreta que Thalyx no puede ser una capa.
**Ahora:** "Núcleo de Thalyx sobre base Alpine".

### 2026-08-03 — desaparece la base
**Antes:** "Fase 1: Núcleo de Thalyx sobre base Alpine. Base: Alpine Linux minimalista."
**Ahora:** no hay base. La imagen es el kernel de Linux y `thalyx`.
**Motivo:** la revisión de arriba quitó la palabra "capa" del título y dejó la distro en el cuerpo, que es como el problema sobrevivió a la corrección que se suponía lo arreglaba. Ver [[Construccion-del-ISO]].
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
