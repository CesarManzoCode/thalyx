---
tipo: caso-de-uso
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, caso-de-uso, fallo, rollback]
---

# Caso de fallo: hash inválido en verificación

Trazado del camino de fallo sobre el mismo caso base que [[Caso-Instalar-Modulo]], usando la rama de [[Ramas-de-Fallo|Rollback]].

## Trazado

```
... pasos 1-9 iguales al camino feliz (ver Caso-Instalar-Modulo) ...

10. Core: recalcula el hash del artefacto en staging → NO coincide
    con el declarado en el manifiesto firmado → FALLA
11. Core: NO publica. No hay commit, no hay nada que deshacer
12. Core: descarta el área de staging /opt/thalyx/.staging/<uuid>
13. Core: descarta los permisos PENDIENTES atados al request_id.
    No quedan permisos vivos para un módulo que nunca se instaló
14. Core: registra en el Journal el intento fallido, con la cadena de origen
15. Core: notifica al usuario por el camino confiable:
    "El módulo no pasó la verificación. No se instaló y no obtuvo
     ningún permiso."
```

## Por qué este caso es simple, y por qué esa simplicidad es la prueba

Con [[Fase-Commit-Atomico|build-then-commit]], este es el caso de fallo **más simple posible**: se detecta antes de cualquier intento de commit. No hay archivos parciales en el sistema oficial porque el Sandbox nunca escribió ahí — todo el trabajo fallido queda contenido en el área de staging, que se descarta sin más.

Es exactamente la consecuencia que se buscaba: el rollback deja de ser "deshacer una operación a medias" y pasa a ser "no hubo commit".

Los pasos 12 y 13 muestran que la propiedad se extiende más allá de los archivos: **el área de staging y los permisos pendientes se descartan por el mismo motivo y en el mismo momento**. Nada quedó a medio camino porque nada llegó a ser efectivo.

## Caso no cubierto por este trazado: fallo durante el commit mismo

Este trazado no cubre el caso donde el fallo ocurre **durante** el commit: por ejemplo, pérdida de energía entre el `rename` del directorio y el `rename` del enlace simbólico.

Ese caso no se resuelve con un trazado sino con evidencia: es el punto de corte obligatorio de los tests de inyección de fallos de nivel 2. Ver [[Estrategia-de-Pruebas]].

El invariante que esos tests verifican es el que hace que este escenario sea seguro: si el proceso muere entre ambos `rename`, el directorio versionado existe pero `current` sigue apuntando a la versión anterior — el sistema queda en el estado previo, íntegro, con basura recuperable pero sin inconsistencia.

## Revisiones

### 2026-08-01 — Se completa el trazado con permisos y staging, y se reencuadra el caso abierto
**Antes:** el trazado terminaba en "no publica, registra, notifica", sin decir qué pasa con los permisos ya otorgados ni con el área de build. Y el fallo durante el commit se dejaba como caso pendiente de trazar, dependiente del supuesto de atomicidad de `rename`.
**Ahora:** se incluyen el descarte de staging y de permisos pendientes, y el fallo durante el commit queda asignado a la [[Estrategia-de-Pruebas|estrategia de pruebas]], con el invariante concreto que lo hace seguro.
**Motivo:** en el diseño anterior, los permisos persistentes se otorgaban antes de la verificación, así que este mismo caso dejaba un permiso vivo para un módulo inexistente. Y un caso de fallo que depende de un supuesto físico no se cierra escribiendo más prosa: se cierra con un test que lo provoque.

## Relacionado
- [[Caso-Instalar-Modulo]]
- [[Ramas-de-Fallo]]
- [[Fase-Commit-Atomico]]
- [[Estrategia-de-Pruebas]]
