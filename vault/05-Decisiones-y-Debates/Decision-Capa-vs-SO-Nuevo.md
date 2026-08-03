---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [debate, arquitectura, no-negociable]
---

# ¿Es una capa sobre Linux o un SO nuevo?

## Debate

Un crítico argumentó que casi todo el valor del sistema (agente, FS semántico, permisos JIT, scheduler predictivo) se podía construir como una capa sobre una distribución de Linux existente, evitando el costo de reinventar el kernel.

## Resolución (decretada, cerrada, no se reabre)

**Thalyx es un sistema operativo nuevo. No es una capa y no es una distribución de Linux.**

Una capa hereda las limitaciones de diseño del sistema que parchea: permisos pensados para procesos humanos, scheduler pensado para cargas humanas, FS pensado para jerarquía humana. Por más daemons que se añadan encima, el techo de lo que la IA puede hacer con soltura sigue siendo el mismo.

## Qué define la diferencia: la autoridad de diseño

Lo que hace que Thalyx no sea una capa **no es dónde corre el código, sino quién tiene autoridad sobre el diseño**.

Una capa no puede cambiar las reglas del sistema que la aloja: las acata. Thalyx es dueño del arranque, del sistema de módulos, de la política de permisos y de los requisitos de filesystem, y modifica el kernel cuando su diseño lo exige — de hecho ya lo hace en la Fase 1, con `thalyx-lsm`.

Que la mayor parte del código de la Fase 1 viva en userspace es una decisión de **orden de construcción y gestión de riesgo**, no una renuncia arquitectónica. Ver [[Decision-Kernel-vs-Userspace]].

## Correr sobre otro Linux es andamio, no destino

Añadido el 2026-08-03, porque faltaba escrito y su ausencia costó un día de
razonamiento torcido.

Hoy Thalyx se ejecuta sobre la Fedora de Cesar: `THALYX_ROOT` apunta a un
directorio, `verify.sh` monta cgroups prestados, el LSM se carga y se desengancha
a mano. **Eso es un banco de pruebas, no una forma de usar Thalyx.** Es
simplemente lo más rápido de validar en una máquina que ya existe.

Lo que el sistema es, lo dice [[Fases-de-Implementacion]] desde la primera
línea de la Fase 1: base Alpine, **Btrfs obligatorio en el volumen raíz**,
subvolúmenes separados para sistema, módulos y datos de usuario. Una imagen que
arranca. El primer usuario no instalará un paquete encima de su distribución —
vivirá la experiencia completa de instalar un sistema operativo.

### Qué se lee mal cuando esto no está escrito

Dos cosas, ambas ocurridas el mismo día:

1. **Se registró como defecto que un módulo sin permisos no corra confinado
   cuando falta el mapa de política**, con el argumento de que "casi ninguna
   máquina tiene `bpf` en el orden de LSM". Cierto para las máquinas de otras
   personas, irrelevante para Thalyx: aquí el LSM se carga en el arranque. Que
   falte es una avería, y negarse es la respuesta correcta. Ver la advertencia 0
   de [[Estado-de-Implementacion]].

2. **Se resumió este decreto como su contrario** — "el proyecto decidió empezar
   como capa sobre Linux" — al descartar una crítica externa que decía que
   Thalyx todavía no es un sistema operativo. El decreto dice literalmente que
   no es una capa, y la revisión de abajo eliminó esa palabra del vocabulario
   precisamente para que nadie volviera a leerlo así.

Sobre esa crítica, dicho bien: **como artefacto de hoy tiene razón** — no hay
imagen que arranque todavía. Lo que confundía era el tipo de afirmación: no es
una posición de diseño pendiente de decidir, es un hecho de **orden de
construcción**. La autoridad de diseño ya está ejercida, y `thalyx-lsm` modifica
el kernel desde la Fase 1.

## Revisiones

### 2026-08-01 — Se añade la formulación de "autoridad de diseño" y se elimina "capa" del vocabulario
**Antes:** este decreto convivía con una Fase 1 titulada "Capa sobre Linux (userspace)", lo que hacía que la bóveda se contradijera a sí misma a la vista de cualquier lector.
**Ahora:** se formula el criterio que distingue capa de sistema propio, y la palabra "capa" desaparece de [[Fases-de-Implementacion]].
**Motivo:** el decreto original era correcto pero incompleto: decía qué no era Thalyx sin decir qué propiedad lo determina. Sin esa propiedad, cualquiera podía leer la Fase 1 y concluir, con razón, que el decreto no se estaba cumpliendo.

## Relacionado
- [[Filosofia-Fundacional]]
- [[Arquitectura-Asimetrica]]
- [[Decision-Kernel-vs-Userspace]]
- [[Fases-de-Implementacion]]
