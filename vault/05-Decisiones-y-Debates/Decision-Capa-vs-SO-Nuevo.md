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
