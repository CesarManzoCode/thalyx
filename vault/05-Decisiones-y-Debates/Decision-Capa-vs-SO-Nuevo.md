---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [debate, arquitectura, no-negociable]
---

# ¿Es una capa sobre Linux o un SO nuevo?

## Debate

Un crítico argumentó que casi todo el valor del sistema (agente, FS semántico, permisos JIT, scheduler predictivo) se podía construir como una capa sobre una distro Linux existente, evitando el costo de reinventar el kernel.

## Resolución (decretada, cerrada, no se reabre)

**No puede ser una capa.**

Una capa hereda las limitaciones de diseño del sistema que parchea: permisos pensados para procesos humanos, scheduler pensado para cargas humanas, FS pensado para jerarquía humana. Por más daemons que se añadan encima, el techo de lo que la IA puede hacer con soltura sigue siendo el mismo.

El sistema debe girar en torno a la IA desde el núcleo hacia afuera — las primitivas ([[Permisos-JIT]], [[Scheduler-Predictivo]], [[FS-en-Grafo]]) deben ser ciudadanas del diseño desde el kernel/API, no add-ons.

## Relacionado
- [[Filosofia-Fundacional]]
- [[Arquitectura-Asimetrica]]
- [[Decision-Kernel-vs-Userspace]]
