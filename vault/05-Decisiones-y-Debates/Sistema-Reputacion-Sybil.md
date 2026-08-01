---
tipo: decision
estado: pospuesto-deliberadamente
fecha-decreto: 2026-07-31
tags: [debate, reputacion, comunidad, primitiva-futura]
---

# Sistema de reputación anti-Sybil

## Debate

El crítico señaló que el sistema de reputación para el repositorio comunitario **no tiene solución barata** (proof-of-work, staking, web-of-trust son las opciones típicas, y todas añaden complejidad desproporcionada para un repo comunitario chico).

## Resolución

Correcto. **No se resuelve ahora.**

- Cuando el repositorio tenga ~5 usuarios y ~10 módulos, la reputación se maneja manualmente (revisión por el equipo central).
- El problema solo es real cuando hay cientos de usuarios y módulos.
- Se documenta como "problema futuro".
- Se deja un campo `reputation` en el schema de módulos para migración futura, pero **no se implementa ahora**.

## Por qué se pospone (el criterio aplicado)

Este es el ejemplo original que estableció el patrón de razonamiento usado después para otras primitivas: "es un problema real, pero aparece con la escala, no lo resolvamos antes de tiempo." Ver [[Criterio-de-Inclusion-de-Primitivas]] para la formalización de este criterio aplicado sistemáticamente.

## Relacionado
- [[Sistema-de-Modulos]]
- [[Criterio-de-Inclusion-de-Primitivas]]
- [[Debate-Core-Modules]]
