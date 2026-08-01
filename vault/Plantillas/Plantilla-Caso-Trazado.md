---
tipo: caso-de-uso
estado: decretado
fecha-decreto: {{date}}
tags: [flujo, caso-de-uso]
---

# Caso: "lo que el usuario pide, en sus palabras"

## Por qué este caso

Qué piezas del [[Flujo-Canonico-Overview|flujo canónico]] ejercita, y por qué se eligió este y no uno más simple.

## Trazado paso a paso

### 1. Usuario
Qué escribe o dice, literal.

### 2. Agente
Qué hace. Si es una consulta, decir explícitamente que no genera contrato — ver [[Resolver-vs-Instalar]].

### 3. Contrato
El JSON completo, con `origins` incluido.

### 4. Core valida
Las cuatro validaciones: schema, origen de campos, contención de permisos, política.

### 5. Confirmación
Por el [[Camino-Confiable|camino confiable]], con el texto que ve el usuario.

### 6. Ejecución
Qué hace el sandbox, dónde escribe.

### 7. Verificación y commit
Qué recalcula el Core, cómo publica.

### 8. Estado
Qué se escribe en el journal, el índice y la memoria persistente.

### 9. Respuesta al usuario

## Descubrimientos que salieron de este trazado

El valor de trazar un caso es encontrar los huecos que el diseño abstracto no muestra. Si el trazado no encontró ninguno, decirlo también — significa que el diseño resistió.

## Ramas de fallo ejercitadas

Qué pasa si falla en cada punto. Ver [[Ramas-de-Fallo]].

## Relacionado
- [[Flujo-Canonico-Overview]]
