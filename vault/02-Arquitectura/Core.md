---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-01
tags: [arquitectura, core, tcb, flujo]
---

# `thalyx-core`

Es la pieza #3 de las [[Flujo-Canonico-Overview|9 piezas fijas]] del flujo canónico, el **único punto de entrada para la ejecución**, y el habitante principal de la TCB — ver [[Modelo-de-Amenaza]].

No confundir con [[Core-Nucleo]], que describe el núcleo del sistema operativo. Esta nota describe el orquestador.

## Responsabilidades

| Responsabilidad | Detalle |
|---|---|
| Validación de contratos | Sintaxis, schema, [[Marcado-de-Origen\|origen de campos]], contención de permisos contra el manifiesto, política |
| Resolución de versiones | Traduce `constraint` a versión exacta — ver [[Resolucion-de-Versiones]] |
| Orquestación de permisos | Solicita a `thalyx-permd`; registra los permisos como pendientes hasta el commit |
| Camino confiable | Genera y renderiza las solicitudes de autorización — ver [[Camino-Confiable]] |
| Verificación | Firma, hash recalculado por él mismo, integridad, dependencias — ver [[Verificacion-y-Distribucion]] |
| Commit atómico | Publicación versionada + intercambio de symlink — ver [[Fase-Commit-Atomico]] |
| Journal | Único escritor — ver [[Journal-y-Snapshots]] |
| Índice en grafo | Único actualizador — ver [[FS-en-Grafo]] |
| Arbitraje | Lock global, serialización de contratos — ver [[Concurrencia]] |

## Validador y orquestador a la vez

El Core es validador y orquestador, pero como **dos fases secuenciales de la misma pieza**, no como dos piezas separadas: primero valida el contrato, después coordina la ejecución llamando a las demás piezas en orden. No hay conflicto de roles porque no ocurren simultáneamente.

## Decreto de estructura interna

El Core se escribe desde el inicio como **módulos internos con fronteras duras**, dentro de un mismo binario:

```
validator · resolver · verifier · committer · journal · graph · arbiter
```

Reglas:
- Interfaces explícitas entre módulos.
- Sin estado mutable compartido.
- Cada módulo testeable de forma aislada.

## Por qué no procesos separados

Se evaluó partir el Core en varios daemons desde ahora. Se descartó para la Fase 1: cada proceso adicional es un canal que serializar, un modo de fallo nuevo y —lo más caro— **un habitante más de la TCB**, es decir, más superficie privilegiada que asegurar, sin ningún beneficio a la escala actual.

## Por qué sí módulos internos desde ahora

Lo que hace costosísimo separar un monolito no es cruzar el límite de proceso: es desenredar las tripas. Si las fronteras internas ya existen y no hay estado compartido, mover un módulo a su propio proceso después es un cambio mecánico.

Es la aplicación literal del [[Criterio-de-Inclusion-de-Primitivas]]: la separación entra ahora porque omitirla implicaría una reescritura dolorosa después; el límite de proceso no entra ahora porque añadirlo después no cuesta nada.

## Riesgo registrado: acumulación de responsabilidades

El Core concentra hoy nueve responsabilidades, y varias de ellas llegaron precisamente por decisiones correctas de seguridad: todo lo que se le quitó al Sandbox y al agente aterrizó aquí.

**Criterio que dispararía partirlo en procesos:**

- Que un módulo interno necesite un ciclo de vida distinto del resto (por ejemplo, reiniciarse sin detener el sistema).
- Que un módulo interno necesite privilegios distintos de los del Core, y por tanto pueda salir de la TCB.
- Que la superficie de código de un módulo crezca lo suficiente como para que auditarlo junto al resto deje de ser viable.

Mientras ninguna de las tres se cumpla, un solo proceso es la decisión correcta.

## Relacionado
- [[Core-Nucleo]]
- [[Flujo-Canonico-Overview]]
- [[Modelo-de-Amenaza]]
- [[Concurrencia]]
- [[Criterio-de-Inclusion-de-Primitivas]]
