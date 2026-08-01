---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitiva, permisos, seguridad, kernel, fase-1]
---

# Permisos just-in-time (JIT)

## Función

La IA pide acceso temporal a recursos, el SO otorga y revoca automáticamente.

## Implementación (Fase 1)

- **`thalyx-lsm`**, un módulo de seguridad del kernel (Linux Security Module) propio, escrito desde la Fase 1.
- **`thalyx-permd`**, el broker en userspace que decide qué se otorga y por cuánto tiempo, y le comunica las decisiones al LSM.

El LSM es la única pieza de Thalyx que vive en el kernel en Fase 1. Aplica los permisos y además intercepta las mutaciones del filesystem que alimentan el [[FS-en-Grafo|índice semántico]] — ver [[Coherencia-Doble-Ruta]].

Flujo:

1. El agente solicita: `REQUEST_PERMISSION(resource: "/home/user/docs", action: "write", type: "jit", duration: "30s")`.
2. `thalyx-permd` evalúa la solicitud contra la política y otorga un token temporal.
3. El LSM aplica la restricción sobre el proceso.
4. El token expira y el permiso se revoca automáticamente.

## Tipos de permiso (decretado — reemplaza el campo `duracion` simple)

Se modela con un campo `type` en vez de solo duración, porque representan **políticas de seguridad distintas**, no solo diferencias de tiempo:

```json
{
  "type": "jit | session | persistent",
  "requires_confirmation": true,
  "revocable": true
}
```

- **JIT:** automático, sin confirmación humana, expira solo (ej. 30 segundos), pensado para acciones rutinarias de bajo riesgo.
- **Sesión:** válido mientras dure la sesión activa del agente/usuario, se revoca al cerrar.
- **Persistente:** requiere confirmación explícita del humano **sin excepciones**, no expira solo, permanece activo hasta revocación manual.

### Regla de política decretada
Cualquier permiso persistente sobre red o carpetas de usuario requiere confirmación explícita, **siempre**, sin importar la reputación del módulo que lo solicite. La confirmación se presenta por el [[Camino-Confiable|camino confiable]], nunca a través del agente.

### Regla de alcance
Los permisos efectivos de un módulo son exactamente los declarados en su manifiesto. Ver [[Formato-Manifiesto-Thmod]] y la regla de contención en [[Contrato-Estructurado]].

### Regla de orden
Un permiso confirmado por el usuario se registra en estado **pendiente**, atado al `request_id` del contrato, y se vuelve efectivo únicamente dentro del commit atómico. Si no hay commit, se descarta. Ver [[Fase-Commit-Atomico]].

## Origen de esta distinción

Esta distinción de tres tipos no estaba en el diseño original — surgió al trazar el [[Caso-Instalar-Modulo|caso concreto de instalar un módulo]], cuando se descubrió que un permiso persistente (acceso a red, sin expiración) no encajaba en el modelo original de "JIT de 30 segundos". Es un ejemplo de por qué trazar casos concretos revela huecos que el diseño abstracto no muestra.

## Revisiones

### 2026-08-01 — Se resuelve la contradicción sobre dónde vive el enforcement
**Antes:** [[Decision-Kernel-vs-Userspace]] situaba esta primitiva en el kernel desde el inicio, mientras que [[Fases-de-Implementacion]] la listaba como daemon de userspace en Fase 1 y posponía el LSM a Fase 3. Dos notas decretadas en contradicción directa.
**Ahora:** `thalyx-lsm` se escribe desde la Fase 1. `thalyx-permd` en userspace decide la política; el LSM la aplica.
**Motivo:** un broker de userspace sin enforcement en el kernel deja los permisos en régimen cooperativo — un módulo que ignore al broker no queda contenido, y la primitiva no cumple lo que promete. Se evaluó y se descartó apoyarse en Landlock: Thalyx es un sistema operativo propio, no una distribución de Linux, y el control del enforcement es parte de su arquitectura, no una dependencia delegada.

### 2026-08-01 — Se añaden las reglas de alcance y de orden
**Motivo:** al trazar el caso canónico contra el [[Modelo-de-Amenaza]] se detectó que nada cruzaba los permisos del contrato contra los del manifiesto, y que un permiso persistente podía quedar vivo tras una instalación fallida.

## Relacionado
- [[Caso-Instalar-Modulo]]
- [[Tres-Categorias-de-Autorizacion]]
- [[Camino-Confiable]]
- [[Modelo-de-Amenaza]]
- [[Flujo-Canonico-Overview]]
