---
tipo: primitiva
estado: decretado
fecha-decreto: {{date}}
tags: [primitiva]
---

# Nombre de la primitiva

## Función

Qué permite hacer, en una o dos frases.

## Por qué es una primitiva y no un módulo

Justificación contra el [[Criterio-de-Inclusion-de-Primitivas]]: ¿omitirla hoy implicaría una reescritura dolorosa después? Si la respuesta es no, esta nota debería ser una "primitiva futura", no una primitiva base.

## Implementación

- Ubicación: kernel o userspace, y por qué.
- Fase en la que se construye.
- Tecnología concreta.

Flujo típico de uso:

1.
2.
3.

## Posición respecto a la TCB

¿Este componente es confiable? ¿Qué pasa si lo comprometen? Ver [[Modelo-de-Amenaza]].

## Modo de fallo

Qué ocurre si esta primitiva falla: ¿la operación se aborta, se degrada, o se rechaza? Ver [[Ramas-de-Fallo]].

## Cómo se verifica

Qué test demuestra que hace lo que promete. Ver [[Estrategia-de-Pruebas]].

## Relacionado
- [[Primitivas-Base-Overview]]
- [[Flujo-Canonico-Overview]]
