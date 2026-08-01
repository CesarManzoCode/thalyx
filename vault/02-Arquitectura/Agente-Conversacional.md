---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-07-31
tags: [arquitectura, agente, ia]
---

# El agente general (Módulo 0, preinstalado)

- **Nombre:** `thalyx-agent` (tentativo).
- **Función:** Traductor de intención. Convierte lenguaje natural en acciones de sistema.
- **Especialización:** No es un LLM genérico. Está fine-tuneado en la documentación del Core, la API de módulos, la estructura de permisos y los comandos de sistema.
- **Modelo:** Local, cuantizado (3B-7B) para tareas rutinarias. Para tareas complejas, con consentimiento explícito del usuario, puede llamar a modelos en la nube (Claude/GPT).

## Flujo de uso típico

1. Usuario: "Quiero instalar un módulo para programar en Python con asistencia de IA."
2. Agente: busca en el repo comunitario, filtra por reputación, muestra opciones.
3. Usuario: "Instala el mejor puntuado."
4. Agente: descarga, verifica firma (si aplica), pide confirmación de permisos, instala y configura.
5. El usuario nunca tocó un `.thmod`.

Ver el trazado completo y actualizado en [[Caso-Instalar-Modulo]].

## Arquitectura interna

- **Modelo local (3B-7B):** clasifica intención y genera respuestas simples. No es un LLM genérico; está especializado en el sistema.
- **Módulos especializados:** lógica compleja (búsqueda en repo, gestión de permisos, orquestación) en código tradicional, no en el LLM.
- **API en la nube (opcional):** con permiso del usuario, el agente puede llamar a Claude/GPT para tareas excepcionalmente complejas.
- **Fine-tuning:** el modelo local está fine-tuneado en la documentación del SO, la API de módulos, los permisos y las políticas. **Esto es un proyecto de investigación en sí mismo, no una feature de Fase 1.** El agente empieza con reglas escritas a mano (if-else) y prompting básico a modelos genéricos. Ver [[Debate-Agente-Fine-Tuning]].

## Seguridad

El agente **no ejecuta comandos directamente**. Genera un contrato estructurado que el sistema valida antes de ejecutar. Ver [[Contrato-Estructurado]].

Ejemplo simplificado de contrato:
```json
{"operacion": "eliminar", "destino": ["/tmp/*.log"], "max_size": "500MB", "confirmacion_requerida": true}
```

## Memoria de conversación

El agente guarda el historial de interacciones en la base de datos persistente. Puede retomar conversaciones días después. Ver [[Memoria-Persistente]].

## Relacionado
- [[Contrato-Estructurado]]
- [[Resolucion-de-Versiones]]
- [[Resolver-vs-Instalar]]
- [[Debate-Agente-Fine-Tuning]]
