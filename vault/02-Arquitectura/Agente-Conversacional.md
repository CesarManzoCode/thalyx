---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-07-31
tags: [arquitectura, agente, ia]
---

# El agente general (Módulo 0, preinstalado)

- **Nombre:** `thalyx-agent`.
- **Función:** Traductor de intención. Convierte lenguaje natural en acciones de sistema.
- **Especialización:** No es un LLM genérico. Conoce la documentación del Core, la API de módulos, la estructura de permisos y los comandos del sistema.
- **Modelo:** Local y cuantizado (3B-7B). **En Fase 1 no hay llamadas a modelos remotos** — ver la sección de nube más abajo.
- **Posición en la arquitectura de seguridad:** el agente **está fuera de la TCB**. Ver [[Modelo-de-Amenaza]].

## Flujo de uso típico

1. Usuario: "Quiero instalar un módulo para programar en Python con asistencia de IA."
2. Agente: busca en el repo comunitario, filtra por reputación, muestra opciones.
3. Usuario: "Instala el mejor puntuado."
4. Agente: genera el contrato. El **Core** verifica firma, pide confirmación de permisos por el camino confiable, y publica.
5. El usuario nunca tocó un `.thmod`.

Ver el trazado completo y actualizado en [[Caso-Instalar-Modulo]].

## Arquitectura interna

- **Modelo local (3B-7B):** clasifica intención y genera respuestas simples. Especializado en el sistema, no genérico.
- **Módulos especializados:** la lógica compleja (búsqueda en el repo, gestión de permisos, orquestación) vive en código tradicional, no en el LLM.
- **Fine-tuning:** el modelo local fine-tuneado en la documentación del sistema **es un proyecto de investigación en sí mismo, no una feature de Fase 1.** El agente empieza con reglas escritas a mano y prompting básico. Ver [[Debate-Agente-Fine-Tuning]].

## Seguridad

El agente **no ejecuta comandos directamente**. Genera un contrato estructurado que el Core valida antes de ejecutar. Ver [[Contrato-Estructurado]].

Tres reglas lo acotan:

1. **No compone ni transporta las confirmaciones.** Las solicitudes de autorización las genera y renderiza el Core. Ver [[Camino-Confiable]].
2. **No puede originar acciones desde contenido no confiable.** Cada campo del contrato declara su procedencia. Ver [[Marcado-de-Origen]].
3. **No puede ampliar permisos.** Los permisos efectivos son los del manifiesto del módulo.

## Llamadas a modelos en la nube

**En Fase 1 no existen.** El agente opera exclusivamente con modelo local.

Cuando se habiliten, será bajo estas condiciones: apagadas por defecto, opt-in **por tarea** y no global, con el payload exacto mostrado por el camino confiable antes de cada envío, y sin contenido de archivos salvo confirmación individual.

## Memoria de conversación

El agente guarda el historial de interacciones en la base de datos persistente. Puede retomar conversaciones días después. Ver [[Memoria-Persistente]].

## Revisiones

### 2026-08-01 — Se elimina la nube de la Fase 1
**Antes:** el agente podía llamar a modelos remotos "con consentimiento explícito del usuario".
**Ahora:** en Fase 1 no hay llamadas remotas; las condiciones para habilitarlas quedan escritas para cuando llegue el momento.
**Motivo:** dos razones. La primera es de coherencia: [[Condiciones-de-Adopcion]] descarta subir logs de auditoría anonimizados por el riesgo de que rutas y metadatos filtren información sensible, y una llamada a un modelo remoto envía mucho más que eso, con la misma clase de riesgo. La segunda es de validación: si el agente puede apoyarse en un modelo grande, nunca se descubre dónde falla el local — que es exactamente lo que la Fase 1 existe para averiguar.

### 2026-08-01 — Se explicita la posición del agente respecto a la TCB
**Motivo:** la bóveda ya decía que el Core "no confía en el agente por defecto", pero no derivaba las consecuencias. Ahora están escritas como tres reglas concretas.

## Relacionado
- [[Modelo-de-Amenaza]]
- [[Camino-Confiable]]
- [[Marcado-de-Origen]]
- [[Contrato-Estructurado]]
- [[Resolucion-de-Versiones]]
- [[Resolver-vs-Instalar]]
- [[Debate-Agente-Fine-Tuning]]
