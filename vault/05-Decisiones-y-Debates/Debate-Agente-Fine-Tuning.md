---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [debate, agente, fine-tuning, fase-1]
---

# ¿Qué pasa con el agente y el fine-tuning?

## Debate

El crítico señaló que fine-tunear un modelo 3B-7B para que sea confiable en "traducir intención a contrato estructurado" es un **proyecto de investigación en sí mismo**, no una feature de Fase 1.

## Resolución

Correcto. El agente **no bloquea la Fase 1**.

- Empieza como un router basado en reglas (if-else) + embeddings simples + prompting a modelos genéricos.
- El fine-tuning es una optimización posterior, no un bloqueante.
- El sistema de contratos y validación se construye primero — es correcto hacerlo así, porque el agente (sea cual sea su forma) tiene que producir esos contratos igual.

## Por qué esto importa más de lo que parece

Identificado en evaluación posterior como **el verdadero cuello de botella del proyecto completo**, más que el kernel o el FS en grafo: "traducir lenguaje natural ambiguo a un contrato estructurado correcto, de forma confiable, sin alucinar" es un problema de investigación abierto, no un problema de ingeniería de sistemas ya resuelto (a diferencia de permisos, sandboxing, commits — donde ya existe mucho conocimiento previo tipo Docker/systemd/apt/git del que tomar prestado).

Que se posponga para después de Fase 1 es correcto, pero vale la pena tenerlo presente como el riesgo técnico más grande del proyecto.

## Revisión del 2026-08-03 — el cuello de botella se acota, no desaparece

Esta nota identificó "traducir lenguaje natural ambiguo a un contrato
estructurado correcto, de forma confiable, sin alucinar" como el verdadero
cuello de botella. [[Gamas-de-Modelo]] parte ese problema en dos, y solo una
mitad sigue siendo de investigación:

- **"Contrato estructurado correcto" deja de ser un problema.** Con
  decodificación restringida por gramática, un contrato mal formado no es
  improbable: es inalcanzable, en las cuatro gamas.
- **"Sin alucinar" sigue siéndolo.** Un modelo puede producir un contrato
  perfectamente válido que pide algo que el usuario no pidió. Contra eso no hay
  gramática; hay [[Camino-Confiable|camino confiable]], y el humano sigue siendo
  quien autoriza.

Lo que cambia en la práctica: el riesgo deja de ser "el agente puede corromper
el sistema" y pasa a ser "el agente puede hacerle perder el tiempo al usuario".
Sigue siendo el mayor riesgo del proyecto, pero es un riesgo de **utilidad**, no
de **integridad** — y esa distinción es lo que permite ofrecer una gama de 1.5B
sin que sea una irresponsabilidad.

El fine-tuning sigue fuera de Fase 1. Nada aquí lo adelanta.

## Relacionado
- [[Agente-Conversacional]]
- [[Gamas-de-Modelo]]
- [[Agente-Minimo]]
- [[Criterio-de-Inclusion-de-Primitivas]]
- [[Fases-de-Implementacion]]
