---
tipo: investigacion
estado: identificado-no-implementado
fecha-decreto: 2026-07-31
tags: [investigacion, interpretabilidad, carrera]
---

# Área de investigación: Interpretabilidad Mecanicista

## Definición

Comprender **por qué** el agente toma las decisiones que toma, no solo *que* las tome correctamente.

## Conexión con el proyecto

El SO permite que el agente opere a nivel de sistema. La interpretabilidad es necesaria para que los usuarios confíen en él — conecta directamente con la primitiva futura [[Criterio-de-Inclusion-de-Primitivas|grafo causal de eventos]].

## Preguntas de investigación

1. ¿Cómo se puede auditar una decisión del agente?
2. ¿Qué información necesita el usuario para confiar en las acciones del agente?
3. ¿Cómo se pueden detectar sesgos o alucinaciones en el agente antes de que causen daño?

## Metodología

- Usar modelos pequeños (3B) y analizar sus representaciones internas (circuitos, atención, atribución).
- Documentar cada fallo y corregir el modelo o las políticas.
- Publicar resultados en arXiv (revistas de interpretabilidad).

## Ventaja estructural

Es un campo joven (fundado hace pocos años), donde el pedigrí importa menos que la contribución técnica concreta.

## Estado actual

Identificada como área de investigación genuina, pero **no implementada en Fase 1**. Se documenta como primitiva futura. El plan es leer papers del campo desde ahora para que el interés sea genuino y no forzado cuando llegue el momento.

## Conexión con la estrategia de carrera

Ver [[Estrategia-Carrera]] — esta área de investigación es parte del plan hacia posgrado en MIT EECS con especialización en interpretabilidad mecanicista / sistemas IA.

## Relacionado
- [[Criterio-de-Inclusion-de-Primitivas]]
- [[Estrategia-Carrera]]
- [[Tareas-Pendientes]]
