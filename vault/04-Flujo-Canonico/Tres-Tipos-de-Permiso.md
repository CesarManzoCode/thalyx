---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, permisos, contrato]
---

# Tres tipos de permiso

Ver la especificación completa en [[Permisos-JIT]]. Esta nota existe para indexar el concepto desde el flujo canónico.

## Resumen

```json
{
  "tipo": "JIT | sesion | persistente",
  "requiere_confirmacion": true,
  "revocable": true
}
```

- **JIT** — automático, expira solo, bajo riesgo.
- **Sesión** — dura mientras la sesión esté activa.
- **Persistente** — confirmación explícita obligatoria, no expira solo.

## Por qué se modeló así y no solo como `duracion`

Los tres tipos representan políticas de seguridad distintas, no solo diferencias temporales. Modelarlo como `tipo` en vez de `duracion` es más expresivo y permite añadir reglas de política específicas por tipo (ej. la regla de confirmación obligatoria para permisos persistentes sobre red/carpetas de usuario).

## Relacionado
- [[Permisos-JIT]]
- [[Tres-Categorias-de-Autorizacion]]
- [[Caso-Instalar-Modulo]]
