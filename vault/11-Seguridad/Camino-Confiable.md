---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-01
tags: [seguridad, autorizacion, agente, no-negociable]
---

# Camino confiable

## Problema

En el trazado original de [[Caso-Instalar-Modulo]], la confirmación de permisos viajaba así:

```
Core → Agente → Usuario
```

Pero el [[Modelo-de-Amenaza]] decreta que el agente **no pertenece a la TCB**. Delegarle redactar y mostrar la solicitud de permisos significa que un agente manipulado puede solicitar una cosa y mostrarle otra al humano.

Sin resolver esto, el principio de soberanía humana de la [[Filosofia-Fundacional]] queda vacío: el soberano solo puede decidir sobre lo que ve, y lo que ve se lo escribe el componente que no es confiable.

## Decreto

Toda solicitud de autorización se **genera y se renderiza por `thalyx-core`**.

1. **El contenido lo genera el Core**, a partir de los campos del contrato ya validado, usando plantillas fijas. El agente no compone, no reformula y no resume el texto de la solicitud.
2. **El canal lo controla el Core.** La solicitud se presenta por una vía que el agente no puede interceptar, suprimir ni retrasar.
3. **La solicitud está identificada** de forma visualmente inconfundible como emitida por Thalyx, no por el agente.
4. **La prosa del agente se muestra aparte.** El agente puede explicar, recomendar y acompañar, pero su texto va en un área separada y marcada como no confiable. Nunca aparece dentro del bloque sobre el que el humano decide.

## Alcance

Aplica a las tres categorías de [[Tres-Categorias-de-Autorizacion|autorización]]:

- Autorización operacional ("¿ejecuto esto?").
- Autorización de capacidades ("¿aceptás estos permisos?").
- Confirmación de operaciones destructivas, incluido `thalyx restore` — ver [[Rollback-vs-Restore]] y [[Coherencia-Doble-Ruta]].

## Consecuencia de diseño

El camino confiable obliga a que la CLI de Fase 1 tenga un modo de presentación que no pase por el flujo conversacional del agente. Es un requisito de arquitectura, no un detalle de interfaz: define dónde termina el agente y dónde empieza el sistema.

## Relacionado
- [[Modelo-de-Amenaza]]
- [[Tres-Categorias-de-Autorizacion]]
- [[Permisos-JIT]]
- [[Filosofia-Fundacional]]
- [[Caso-Instalar-Modulo]]
