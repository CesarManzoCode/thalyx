---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, commit, seguridad, decision-clave]
---

# Fase de commit atómico (build-then-commit)

> Esta es, según evaluación explícita hecha durante el diseño, **la mejor decisión técnica de toda la sesión de arquitectura** — el tipo de patrón que aparece en bases de datos, deployments y compiladores (separar "producir" de "publicar").

## El problema que resuelve

Originalmente, el [[Sandbox-Ejecucion|Sandbox]] escribía directo al sistema oficial (ej. `/opt/modules`). Esto significa que un fallo a medio camino dejaría archivos parciales en el sistema real — "rollback" se volvía "deshacer lo que alcanzaste a copiar", que es frágil y propenso a estados inconsistentes.

## El decreto

El Sandbox **nunca escribe directo al sistema oficial**. En su lugar:

```
Sandbox → produce artefacto en /tmp/build/...
    ↓
Core → verifica (firma, hash, dependencias, integridad)
    ↓
Core → commit atómico (publica al destino real, ej. /opt/modules/...)
```

Si la verificación falla, **no hay commit** — no hay nada físico que deshacer, simplemente no se publicó.

## El flujo completo con la etapa de commit

```
Usuario
  ↓
Agente (resuelve intención — sub-tarea de consulta/lectura, no genera contrato todavía)
  ↓
Contrato (se genera solo cuando hay una decisión concreta que ejecutar)
  ↓
Core: Validación (sintaxis, firma, permisos solicitados vs. política)
  ↓
Core: Solicita permisos (JIT/sesión/persistente, con confirmación si aplica)
  ↓
Core: Solicita ajuste de scheduling (si aplica; si falla, degrada, no aborta)
  ↓
Sandbox: Produce artefacto (en área temporal, NO en el sistema oficial)
  ↓
Core: Verificación final (firma, hash, dependencias, integridad)
  ↓
Core: Commit atómico (publica el artefacto verificado al destino real;
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

Esto refuerza directamente una de las tres [[Condiciones-de-Adopcion|demostraciones dramáticas de adopción]] ya decretadas (rollback instantáneo).

## Supuesto técnico crítico: atomicidad real

El commit atómico depende de que la operación de publicación sea **atómica a nivel de sistema de archivos** (`rename`, no copy+delete). En Linux, `rename` es atómico dentro del mismo filesystem.

**Si el commit involucra diferentes filesystems o dispositivos**, esa atomicidad ya no está garantizada gratis por el kernel, y habría que resolverlo explícitamente (ej. con un journal adicional de tipo copiar + journal de intención).

Nota para implementación: el Core debe usar `rename` para commits; si se necesitan cruzar filesystems, se implementa una capa de transacción adicional.

## Relacionado
- [[Flujo-Canonico-Overview]]
- [[Ramas-de-Fallo]]
- [[Tres-Categorias-de-Autorizacion]]
- [[Caso-Instalar-Modulo]]
- [[Caso-Fallo-Rollback]]
