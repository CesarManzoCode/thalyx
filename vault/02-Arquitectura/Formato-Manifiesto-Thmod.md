---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-01
tags: [modulos, manifiesto, schema, firma, fase-1]
---

# Formato del manifiesto `.thmod`

El manifiesto es la pieza de la que cuelgan los permisos, la firma, la versión y la distribución de un módulo. Hasta el 1 de agosto de 2026 era un enlace roto en la bóveda.

## Formato

**TOML** para el manifiesto. **JSON** para el [[Contrato-Estructurado|contrato]] del agente.

Cada formato para su consumidor: el manifiesto lo escribe una persona, y TOML admite comentarios y no tiene sorpresas de tipado; el contrato lo genera una máquina, y JSON es el formato natural para eso. Es el mismo reparto que hace Cargo, por la misma razón.

Se descartó YAML de forma explícita: sus conversiones implícitas de tipo (`version = 2.30` interpretado como número, perdiendo el patch) son inaceptables en un archivo que gobierna permisos y firma.

## Schema (versión 1)

```toml
format_version = 1

id             = "org.publisher.pyassist"   # DNS inverso, inmutable de por vida
name           = "PyAssist Core"            # visible, puede cambiar
version        = "2.3.1"                    # SemVer estricto
description    = "Python assistance module"
license        = "GPL-3.0-or-later"
publisher_key  = "ed25519:AbC..."           # clave pública que firma
distribution   = "prebuilt"                 # ver Verificacion-y-Distribucion

[artifact]
hash = "sha256:..."
size = 4823910

[requires]
thalyx = "^1.0"        # En Fase 1, un módulo solo depende del runtime.
                       # No se admiten dependencias entre módulos.

[[permissions]]
resource = "net"
action   = "outbound"
type     = "persistent"

[[permissions]]
resource = "/home/user/projects"
action   = "read"
type     = "persistent"

[entrypoints]
run = "bin/pyassist"

[reputation]           # reservado: campo previsto, sin implementar.
                       # Ver Sistema-Reputacion-Sybil.
```

## Reglas

- **`id` es inmutable.** El nombre visible puede cambiar; el identificador no, nunca.
- **Los permisos del manifiesto son los permisos efectivos.** El contrato del agente no puede ampliarlos, y si solicita menos, al usuario se le presenta igualmente el conjunto completo del manifiesto. Ver [[Permisos-JIT]] y [[Contrato-Estructurado]].
- **`format_version` es obligatorio** y se valida antes que cualquier otro campo.

## Firma

- **Detached, ed25519**, calculada sobre la forma canonicalizada del manifiesto.
- **Core Modules:** firmados con la clave del proyecto, fijada en la imagen del sistema. Ver [[Debate-Core-Modules]].
- **Módulos comunitarios:** confianza al primer uso (TOFU), con la clave anclada al `id`. **Un cambio de clave para un `id` ya conocido es un error duro, no una advertencia** — es la forma que toma la suplantación de publicador, que es el adversario 3 del [[Modelo-de-Amenaza]].

## Relacionado
- [[Sistema-de-Modulos]]
- [[Verificacion-y-Distribucion]]
- [[Resolucion-de-Versiones]]
- [[Contrato-Estructurado]]
- [[Modelo-de-Amenaza]]
