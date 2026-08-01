---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, commit, seguridad, decision-clave]
---

# Fase de commit atómico (build-then-commit)

> Esta es, según evaluación explícita hecha durante el diseño, **la mejor decisión técnica de toda la sesión de arquitectura** — el tipo de patrón que aparece en bases de datos, deployments y compiladores (separar "producir" de "publicar").

## El problema que resuelve

Originalmente, el [[Sandbox-Ejecucion|Sandbox]] escribía directo al sistema oficial. Esto significa que un fallo a medio camino dejaría archivos parciales en el sistema real — "rollback" se volvía "deshacer lo que alcanzaste a copiar", que es frágil y propenso a estados inconsistentes.

## El decreto

El Sandbox **nunca escribe directo al sistema oficial**. En su lugar:

```
Sandbox → produce artefacto en el área de staging
    ↓
Core → verifica (firma, hash recalculado, dependencias, integridad)
    ↓
Core → commit atómico (publica al destino real)
```

Si la verificación falla, **no hay commit** — no hay nada físico que deshacer, simplemente no se publicó.

## Mecanismo concreto del commit

El área de staging **no es `/tmp`**. Es `/opt/thalyx/.staging/<uuid>/`, en el **mismo subvolumen Btrfs que el destino final**.

La publicación son dos operaciones, ambas atómicas:

1. `rename("/opt/thalyx/.staging/<uuid>", "/opt/thalyx/modules/<id>/<version>")`
   El destino no existe todavía, así que no hay `ENOTEMPTY`.
2. Intercambio del enlace simbólico: se crea `<id>/.current.tmp` apuntando a `<version>` y se hace `rename` sobre `<id>/current`.

**El módulo está oficialmente instalado en el instante del paso 2, no antes.** Los permisos confirmados se vuelven efectivos en ese mismo instante, porque su vigencia está condicionada a que el módulo sea la versión actual — ver [[Permisos-JIT]].

### Limpieza de huérfanos antes de publicar

Si ya existe un directorio para esa versión y **no** es el actual, se borra antes del `rename`. Es el residuo de un commit interrumpido: por definición nadie apunta a él, así que nadie puede estar usándolo. Un directorio que sí es el actual nunca llega aquí — instalar sobre una versión viva se rechaza antes.

Sin este paso, reintentar tras un corte a mitad del commit falla con `ENOTEMPTY`: el sistema queda consistente pero irrecuperable sin intervención manual, que en la práctica es igual de inútil que quedar corrupto.

### Por qué así y no de otra forma

`rename` es atómico dentro del mismo filesystem, pero falla con `EXDEV` cuando cruza filesystems — y también **entre subvolúmenes distintos de Btrfs**, no solo entre dispositivos. Un área de staging en `/tmp`, que en Alpine suele ser tmpfs, activa ese fallo siempre.

Y `rename` sobre un directorio no vacío falla con `ENOTEMPTY`, así que publicar una actualización encima de una versión anterior no se puede hacer renombrando el directorio de destino. De ahí la indirección por enlace simbólico: `rename` sobre un symlink existente sí es atómico y sí lo reemplaza.

## El flujo completo con la etapa de commit

```
Usuario
  ↓
Agente (resuelve intención — sub-tarea de consulta/lectura, no genera contrato todavía)
  ↓
Contrato (se genera solo cuando hay una decisión concreta que ejecutar,
          con marcado de origen por campo)
  ↓
Core: Validación (sintaxis, firma, origen de campos, permisos vs. manifiesto)
  ↓
Core: Solicita permisos (JIT/sesión/persistente, confirmación por camino confiable)
  ↓
Sandbox: Produce artefacto (en área de staging, NO en el sistema oficial)
  ↓
Core: Verificación final (firma, hash recalculado, dependencias, integridad)
  ↓
Core: Commit atómico (rename del directorio + rename del symlink;
                       si la verificación falla, no hay commit)
  ↓
Journal (registra la operación y el snapshot pre-commit)
  ↓
Memoria persistente (guarda hechos + notas de continuidad)
  ↓
Respuesta al Usuario
```

## Consecuencia

El rollback deja de significar "deshacer archivos parcialmente copiados" (frágil, propenso a estados inconsistentes) y pasa a significar simplemente **"el commit nunca ocurrió"** (robusto, atómico, predecible).

## Verificación obligatoria de esta propiedad

La atomicidad es la afirmación central de Thalyx, y no se documenta sin evidencia. El invariante *publicado o no publicado, nunca a medias* se comprueba con tests de inyección de fallos que matan el proceso en cada punto intermedio del commit, incluido el instante entre los dos `rename`. Ver [[Estrategia-de-Pruebas]].

## Registro de intención

El journal se escribe **alrededor** del commit, no dentro: una **intención** baja a disco antes de que nada se mueva, y una entrada terminal después.

```
Journal: intención  ("voy a publicar X versión Y")
    ↓
rename del directorio
    ↓
rename del symlink        ← el módulo queda instalado aquí
    ↓
Journal: entrada terminal (éxito)
```

Un proceso que muere en medio deja una intención sin resolver. **Eso no es una operación perdida: es una pregunta, y el disco tiene la respuesta.** La reconciliación la formula: ¿la versión que nombraba la intención es la actual? Si sí, el commit ocurrió y se escribe ahora la entrada que nunca se escribió. Si no, no hubo commit.

Sin esto, un corte justo después del intercambio del symlink dejaba el módulo instalado y funcional **sin ningún registro de que hubiera ocurrido**. El invariante de atomicidad se sostenía, pero el journal mentía por omisión.

La reconciliación es idempotente y se ejecuta sola al principio de cada instalación, así que un usuario que simplemente reintenta nunca necesita enterarse de que existe. También puede invocarse a mano con `thalyx store reconcile`.

## Revisiones

### 2026-08-01 — Se añade el registro de intención
**Antes:** el journal se escribía solo después del commit.
**Ahora:** una intención se registra antes de que nada se mueva, y la reconciliación la resuelve contra el disco.
**Motivo:** un corte entre el commit y la escritura del journal dejaba una instalación real y no registrada. Un journal que a veces omite una operación exitosa es peor que uno que declara cuándo puede fallar.

### 2026-08-01 — Se concreta el mecanismo y se corrige el área de staging
**Antes:** la nota decretaba usar `rename` y advertía, como riesgo hipotético, que la atomicidad no estaría garantizada "si el commit involucra diferentes filesystems". Pero el trazado del caso canónico publicaba de `/tmp/build/` a `/opt/modules/`, que activa ese riesgo en la configuración por defecto de Alpine. Tampoco contemplaba que `rename` no puede publicar encima de un directorio existente.
**Ahora:** staging en el mismo subvolumen que el destino, publicación versionada, e intercambio atómico de enlace simbólico.
**Motivo:** el mecanismo decretado no funcionaba en el propio caso de referencia. Dos fallos concretos: `EXDEV` por cruce de filesystems (incluido el cruce entre subvolúmenes de Btrfs, que no es obvio) y `ENOTEMPTY` al actualizar un módulo ya instalado.

## Relacionado
- [[Verificacion-y-Distribucion]]
- [[Estrategia-de-Pruebas]]
- [[Flujo-Canonico-Overview]]
- [[Ramas-de-Fallo]]
- [[Tres-Categorias-de-Autorizacion]]
- [[Caso-Instalar-Modulo]]
- [[Caso-Fallo-Rollback]]
