---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-01
tags: [seguridad, agente, contrato, inyeccion, no-negociable]
---

# Marcado de origen (defensa contra inyección de prompts)

## Problema

`thalyx-agent` lee contenido que no controla: descripciones de módulos, reseñas y metadatos del repositorio comunitario, archivos del usuario, respuestas de red. Todo eso es texto potencialmente redactado por un atacante.

Ese texto entra al proceso que produce contratos. El Core valida los contratos, pero la validación es **sintáctica y de política**: un contrato malicioso bien formado y dentro de la política la pasa sin problema. El único obstáculo restante era la confirmación humana, que hasta el decreto de [[Camino-Confiable|camino confiable]] también la redactaba el agente.

Es el vector del adversario prioritario del [[Modelo-de-Amenaza]], y no aparecía en ninguna nota de la bóveda.

## Decreto

Todo campo de un contrato lleva un **origen** declarado:

| Origen | Significado |
|---|---|
| `user_utterance` | Proviene directamente de lo que el usuario escribió o dijo |
| `system_state` | Proviene del estado del sistema: el índice, el journal, la memoria persistente, el registro de permisos |
| `untrusted_content` | Proviene de texto que Thalyx no controla: repositorio comunitario, manifiestos de terceros, contenido de red, archivos ajenos |

**`thalyx-core` rechaza todo contrato en el que un campo con efecto sobre el sistema tenga origen `untrusted_content`.** Los campos con efecto son, como mínimo: la operación, los destinos, los permisos solicitados y la restricción de versión.

El contenido no confiable **puede informar lo que el agente le muestra al usuario. Nunca puede determinar lo que el contrato hace.**

La cadena de origen completa se registra en el [[Journal-y-Snapshots|Journal]], de modo que toda acción ejecutada pueda auditarse hasta la fuente que la motivó.

## Por qué este mecanismo y no filtrado

Se consideró sanitizar o filtrar el contenido de terceros antes de que llegue al agente. Se descartó: no existe un filtro confiable para lenguaje natural adversarial, y apostar a uno es una carrera armamentística que se pierde con el tiempo.

El marcado de origen, en cambio, es **mecánicamente verificable**. No requiere entender el texto ni juzgar su intención, y no depende de que el modelo se comporte bien: es una propiedad estructural del contrato que el Core comprueba con una regla fija.

## Nota de implementación

El marcado tiene que existir desde la primera versión del contrato. Retrofitearlo es mucho más caro que construirlo: obliga a rehacer el pipeline entero de generación para que cada campo arrastre su procedencia, y hasta entonces cada campo sin origen conocido es indistinguible de uno comprometido.

## Relacionado
- [[Modelo-de-Amenaza]]
- [[Contrato-Estructurado]]
- [[Camino-Confiable]]
- [[Agente-Conversacional]]
- [[Interpretabilidad-Mecanicista]]
