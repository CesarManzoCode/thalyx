---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, contrato, json-schema, fase-1]
---

# El contrato estructurado del agente

## Formato

JSON Schema, versión 1.0 para Fase 1. Todos los campos en inglés, según [[Nomenclatura-y-Convenciones]].

## Campos obligatorios (Fase 1)

```json
{
  "version": "1.0",
  "operation": "install_module",
  "targets": ["org.publisher.pyassist"],
  "constraint": "^2.3",
  "permissions": [
    {"resource": "net", "action": "outbound", "type": "persistent"}
  ],
  "requires_confirmation": true,
  "sandbox_profile": "module_standard",
  "rollback": {
    "enabled": true,
    "snapshot_id": null
  },
  "caller": {
    "module_id": "thalyx-agent",
    "request_id": "abc-123"
  },
  "origins": {
    "operation": "user_utterance",
    "targets": "user_utterance",
    "constraint": "system_state",
    "permissions": "system_state"
  }
}
```

## El campo `origins`

Cada campo con efecto sobre el sistema declara de dónde salió: `user_utterance`, `system_state` o `untrusted_content`. El Core **rechaza** el contrato si un campo con efecto tiene origen `untrusted_content`. Es la defensa estructural contra inyección de prompts — ver [[Marcado-de-Origen]].

## Reglas de validación del Core

1. **Sintaxis y schema**, incluida la presencia de `version`.
2. **Origen de los campos** con efecto (ver arriba).
3. **Contención de permisos:** los permisos del contrato deben estar contenidos en los del manifiesto del módulo. Si piden más, se rechaza. Si piden menos, al usuario se le presenta igualmente el conjunto completo del manifiesto. Ver [[Formato-Manifiesto-Thmod]].
4. **Política:** los permisos de tipo `persistent` disparan confirmación explícita, siempre, por el [[Camino-Confiable|camino confiable]].

El Core **no confía en el agente**: revalida todo, sin excepción.

## Campos opcionales (para fases futuras)

- `"priority": "high"` (para el [[Scheduler-Predictivo|scheduler predictivo]], Fase 2).
- `"resource_limits": { "cpu_quota": 10, "memory_max": 512 }`.
- `"audit_trail": [...]` (para [[Interpretabilidad-Mecanicista|interpretabilidad]]).

## Principio de generación: solo con decisión concreta

El contrato **se genera después de resolver la intención**, nunca sobre una búsqueda abierta. Ver [[Resolver-vs-Instalar]] — no tiene sentido firmar un contrato sobre "el mejor módulo" cuando todavía no existe una selección concreta.

## Versiones: restricción, no versión fija

El contrato expresa un `constraint`; el Core resuelve la versión exacta. Ver [[Resolucion-de-Versiones]].

## Revisiones

### 2026-08-01 — Idioma unificado, campo `origins` y reglas de validación explícitas
**Antes:** el ejemplo de esta nota usaba nombres de campo en inglés (`operation`, `targets`) mientras que [[Caso-Instalar-Modulo]] y [[Agente-Conversacional]] los usaban en español (`operacion`, `destino`), y el caso canónico omitía el campo `version` que aquí figuraba como obligatorio. Además no existía ninguna regla que cruzara los permisos del contrato contra los del manifiesto, ni ninguna defensa contra contenido no confiable.
**Ahora:** todos los campos en inglés, `origins` obligatorio, y las cuatro reglas de validación del Core escritas de forma explícita.
**Motivo:** el schema es la interfaz entre el componente no confiable y la TCB. Que existiera en dos idiomas y dos formas distintas según la nota que se leyera hacía imposible implementarlo sin elegir arbitrariamente.

## Relacionado
- [[Marcado-de-Origen]]
- [[Formato-Manifiesto-Thmod]]
- [[Camino-Confiable]]
- [[Flujo-Canonico-Overview]]
- [[Resolver-vs-Instalar]]
- [[Resolucion-de-Versiones]]
