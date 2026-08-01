---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, contrato, json-schema, fase-1]
---

# El contrato estructurado del agente

## Formato

JSON Schema, versión 1.0 para Fase 1.

## Campos obligatorios (Fase 1)

```json
{
  "version": "1.0",
  "operation": "delete_files",
  "targets": ["/tmp/*.log", "/var/cache/*"],
  "filters": {
    "max_size_mb": 500,
    "modified_before": "2025-01-01"
  },
  "requires_confirmation": true,
  "rollback": {
    "enabled": true,
    "snapshot_id": null
  },
  "caller": {
    "module_id": "thalyx-agent",
    "request_id": "abc-123"
  }
}
```

## Campos opcionales (para fases futuras)

- `"priority": "high"` (para [[Scheduler-Predictivo|scheduler predictivo]]).
- `"resource_limits": { "cpu_quota": 10, "memory_max": 512 }` (para [[Sandbox-Ejecucion|sandboxing]]).
- `"audit_trail": [...]` (para [[Interpretabilidad-Mecanicista|interpretabilidad]]).

## Principio de generación: solo con decisión concreta

El contrato **se genera después de resolver la intención**, nunca sobre una búsqueda abierta. Ver [[Resolver-vs-Instalar]] — no tiene sentido firmar un contrato sobre "el mejor módulo" cuando todavía no existe una selección concreta.

## Campos relacionados con decretos posteriores

- El campo de permisos debe usar el formato de [[Tres-Tipos-de-Permiso|tres tipos de permiso]] (`tipo: JIT | sesion | persistente`), no solo duración.
- Para instalación de módulos, la versión no debe fijarse en el contrato del agente — ver [[Resolucion-de-Versiones]].

## Relacionado
- [[Flujo-Canonico-Overview]]
- [[Resolver-vs-Instalar]]
- [[Resolucion-de-Versiones]]
