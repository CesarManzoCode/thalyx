---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-01
tags: [pruebas, ci, atomicidad, evidencia]
---

# Estrategia de pruebas

## Por qué existe esta nota

La atomicidad del commit y la garantía de rollback son **las afirmaciones centrales de Thalyx**. Hasta el 1 de agosto de 2026 nada en la bóveda verificaba ninguna de las dos: eran diseño, no evidencia.

## Los tres niveles (obligatorios)

### Nivel 1 — Unitarios y de propiedades

Sobre el resolver de versiones, el validador de contratos y el parser mecánico. Los tres tienen entrada y salida deterministas, así que se prestan a pruebas basadas en propiedades y no solo a casos de ejemplo.

### Nivel 2 — Inyección de fallos

**Es el nivel que no puede faltar.**

Se mata el proceso en cada punto intermedio del commit y se verifica el invariante:

> **Publicado o no publicado. Nunca a medias.**

Puntos de corte obligatorios, como mínimo:

- Durante la producción del artefacto en el área de staging.
- Entre la verificación y el primer `rename`.
- **Entre el `rename` del directorio y el `rename` del enlace simbólico** — el instante donde la publicación está a mitad de camino.
- Durante la escritura del journal.
- Durante el registro efectivo de los permisos confirmados.

Incluye corte de energía simulado en QEMU, no solo `SIGKILL` al proceso: un `SIGKILL` no ejercita el comportamiento del filesystem ante pérdida de energía.

### Nivel 3 — End-to-end en CI

El [[Caso-Instalar-Modulo|caso canónico]] completo, ejecutado en QEMU dentro de integración continua, en cada cambio.

## Regla de documentación

**Ninguna afirmación sobre atomicidad o rollback se documenta en la bóveda sin un test de nivel 2 que la respalde.**

Si una nota afirma que una operación es atómica y no existe el test que lo demuestra, la nota está describiendo una intención, no una propiedad — y debe decirlo.

## Por qué el nivel 2 importa más allá del código

El sandbox de Thalyx es de [[Sandbox-Ejecucion|implementación propia]], lo que significa que no hereda la auditoría acumulada de una herramienta de terceros. Los tests de inyección de fallos y las pruebas de aislamiento son lo que compensa esa exposición.

Y desde el lado de la investigación: un experimento que mata el proceso en el punto exacto donde la atomicidad podría romperse, y muestra que el invariante se sostiene, es exactamente la clase de resultado reproducible que sostiene un paper. Ver [[Estrategia-Carrera]].

## Relacionado
- [[Fase-Commit-Atomico]]
- [[Sandbox-Ejecucion]]
- [[Caso-Instalar-Modulo]]
- [[Criterio-de-Salida-Fase-1]]
- [[Notas-Tecnicas-Implementacion]]
