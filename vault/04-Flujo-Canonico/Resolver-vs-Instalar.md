---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, contrato, agente]
---

# Separación entre "resolver" e "instalar"

## Problema

La sub-tarea de búsqueda/selección de un módulo (cuando el usuario pide "el mejor módulo" sin especificar cuál) no debería mezclarse con el contrato de instalación, porque la búsqueda puede repetirse o refinarse antes de llegar a una decisión concreta.

Ejemplo de por qué importa:

```
Usuario: "Instala el mejor módulo."
  ↓
Agente busca.
  ↓
Usuario: "¿Y cuál tiene menos permisos?"
  ↓
Agente vuelve a consultar.
  ↓
Ahora sí instala.
```

La búsqueda no debería formar parte del contrato de instalación.

## Solución decretada

La búsqueda **no genera contrato**. Es una sub-tarea de consulta (solo lectura). El [[Contrato-Estructurado|contrato]] solo se genera cuando hay una decisión concreta (ej. "instalar pyassist-core"). Esto evita mezclar "explorar" con "ejecutar".

Esta es la opción preferida frente a la alternativa considerada de modelar la búsqueda como un contrato separado tipo `ResolverModulo` — se prefirió no generar contrato en absoluto para la fase de búsqueda, porque no hay acción sobre el sistema todavía, solo lectura.

## Aplicación práctica

Ver el paso 2 de [[Caso-Instalar-Modulo]], donde se aplica exactamente este decreto.

## Relacionado
- [[Contrato-Estructurado]]
- [[Caso-Instalar-Modulo]]
- [[Agente-Conversacional]]
