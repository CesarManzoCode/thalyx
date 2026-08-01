---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitiva, permisos, seguridad, kernel]
---

# Permisos just-in-time (JIT)

## Función

La IA pide acceso temporal a recursos, el SO otorga y revoca automáticamente.

## Implementación

- Módulo LSM (Linux Security Module) en el kernel.
- Flujo:
  1. El agente solicita: `REQUEST_PERMISSION(resource: "/home/user/docs", action: "write", duration: "30s")`.
  2. El SO otorga un token temporal con validez de 30 segundos.
  3. El agente ejecuta la operación.
  4. El token expira y el permiso se revoca automáticamente.

## Tipos de permiso (decretado — reemplaza el campo `duracion` simple)

Se modela con un campo `tipo` en vez de solo duración, porque representan **políticas de seguridad distintas**, no solo diferencias de tiempo:

```json
{
  "tipo": "JIT | sesion | persistente",
  "requiere_confirmacion": true,
  "revocable": true
}
```

- **JIT:** automático, sin confirmación humana, expira solo (ej. 30 segundos), pensado para acciones rutinarias de bajo riesgo.
- **Sesión:** válido mientras dure la sesión activa del agente/usuario, se revoca al cerrar.
- **Persistente:** requiere confirmación explícita del humano **sin excepciones**, no expira solo, permanece activo hasta revocación manual.

### Regla de política decretada
Cualquier permiso persistente sobre red o carpetas de usuario requiere confirmación explícita, **siempre**, sin importar la reputación del módulo que lo solicite.

## Origen de esta distinción

Esta distinción de tres tipos no estaba en el diseño original — surgió al trazar el [[Caso-Instalar-Modulo|caso concreto de instalar un módulo]], cuando se descubrió que un permiso persistente (acceso a red, sin expiración) no encajaba en el modelo original de "JIT de 30 segundos". Es un ejemplo de por qué trazar casos concretos revela huecos que el diseño abstracto no muestra.

## Relacionado
- [[Caso-Instalar-Modulo]]
- [[Tres-Categorias-de-Autorizacion]]
- [[Flujo-Canonico-Overview]]
