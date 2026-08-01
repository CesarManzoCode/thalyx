---
tipo: componente
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, sandbox, seguridad]
---

# Sandbox de ejecución de módulos

## Rol

Aísla y ejecuta el código del módulo en un entorno controlado. Es una de las [[Flujo-Canonico-Overview|9 piezas fijas]] del flujo canónico. **Está fuera de la TCB**: contiene, pero no se le cree — ver [[Modelo-de-Amenaza]].

## Decreto: el Sandbox es una pieza separada del Core

El Core valida el contrato, pero **no ejecuta el código del módulo**. El módulo se ejecuta en un entorno aislado que es una pieza independiente.

**Motivo:** si el Core ejecutara directamente el código del módulo, un bug o un ataque en el módulo podría comprometer el Core. Separarlos es una decisión de seguridad, no de complejidad.

## Decreto: instalar no ejecuta código del módulo

En Fase 1, la instalación desempaqueta y valida estructura; no corre scripts del módulo. El código del módulo se ejecuta solo en runtime. Ver [[Verificacion-y-Distribucion]].

## Decreto: el Sandbox no toca el estado oficial del sistema

Produce su artefacto en el área de staging, nunca escribe directo al destino oficial. El Core verifica y publica. Ver [[Fase-Commit-Atomico]].

## Decreto: el Sandbox nunca toca el FS en grafo ni el Journal directamente

Solo el Core actualiza el [[FS-en-Grafo]] y escribe en el [[Journal-y-Snapshots]], después de recibir y verificar el resultado del Sandbox. Esto evita que un módulo comprometido dentro del sandbox pueda corromper el índice semántico o falsificar el journal.

## Mecanismo técnico (decretado)

**Implementación propia**, sin depender de un sandbox de terceros.

Perfil `module_standard`:

- **Namespaces:** usuario, montaje, PID, red, IPC y UTS.
- **Red:** denegada por defecto; solo se habilita con permiso explícito otorgado.
- **cgroup v2:** `memory.max`, `pids.max` y `cpu.max`.
- **seccomp:** filtro por lista de permitidos, no por lista de bloqueados.
- **Filesystem:** restricciones aplicadas por `thalyx-lsm` según los permisos otorgados.

El perfil se declara en el contrato y lo aplica el Core, nunca el módulo.

### Por qué implementación propia

Se evaluó apoyarse en bubblewrap, que ya resuelve este montaje y está auditado por su uso en Flatpak. Se decretó implementación propia: Thalyx es un sistema operativo, no una distribución que integra piezas ajenas, y el aislamiento es una pieza de arquitectura, no una dependencia delegada. El costo asumido conscientemente es tiempo de iteración y la necesidad de revisión externa sobre el montaje de user namespaces, donde los errores no se manifiestan como fallos sino como agujeros silenciosos.

Esa exposición se compensa con los tests de nivel 2 de la [[Estrategia-de-Pruebas]].

## Revisiones

### 2026-08-01 — Se detalla el mecanismo y se separa instalación de ejecución
**Antes:** el mecanismo estaba enunciado ("namespaces, cgroups, seccomp") pero pendiente de detalle fino, y la instalación de un módulo implicaba ejecutar su script dentro del sandbox.
**Ahora:** perfil `module_standard` concreto, implementación propia decretada, e instalación sin ejecución de código del módulo.
**Motivo:** el pendiente de sandboxing bloqueaba la Fase 1. Y ejecutar un script de instalación arbitrario hacía imposible cualquier verificación criptográfica del resultado — ver [[Verificacion-y-Distribucion]].

## Relacionado
- [[Verificacion-y-Distribucion]]
- [[Fase-Commit-Atomico]]
- [[Modelo-de-Amenaza]]
- [[Estrategia-de-Pruebas]]
- [[Flujo-Canonico-Overview]]
- [[Sistema-de-Modulos]]
