---
tipo: caso-de-uso
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, caso-de-uso, referencia-canonica]
---

# Caso de uso: "Instala el mejor módulo de Python con asistencia de IA"

Este es el **caso de referencia canónico** usado para validar todas las piezas del [[Flujo-Canonico-Overview|flujo canónico]]. Se eligió porque toca prácticamente todas las piezas (agente, contrato, permisos, sandbox, FS en grafo, journal), a diferencia de casos más simples como "borrar temporales" que dejarían el scheduling y la memoria persistente sin ejercitar.

> **Nota de estado:** este trazado ya incorpora las correcciones de [[Fase-Commit-Atomico|build-then-commit]], [[Tres-Tipos-de-Permiso|tipos de permiso]], [[Resolucion-de-Versiones|resolución de versiones]] y [[Resolver-vs-Instalar|separación resolver/instalar]]. Es la versión final y actualizada del trazado.

## Trazado paso a paso

### 1. Usuario
Escribe: *"Quiero instalar un módulo para programar en Python con asistencia de IA. Instala el mejor."*

### 2. Agente — sub-tarea de consulta (sin contrato todavía)
Ver [[Resolver-vs-Instalar]]: la búsqueda no genera contrato, es una sub-tarea de lectura.

- Consulta el repositorio comunitario, filtra por categoría "Python + IA", ordena por reputación/descargas/issues abiertos.
- Presenta opciones: *"Encontré 'pyassist-core' (4.8★, 12k descargas, pide acceso a red y a tu carpeta de proyectos) y 'py-tutor-lite' (4.9★, 3k descargas, solo acceso local, sin red)."*
- Usuario: *"Instala pyassist-core."*

### 3. Agente genera el Contrato (con `constraint`, no versión fija)
Ver [[Resolucion-de-Versiones]] — el agente expresa una restricción, no fija la versión exacta.

```json
{
  "operacion": "instalar_modulo",
  "modulo_id": "pyassist-core",
  "fuente": "repo-comunitario",
  "constraint": "^2.3",
  "permisos_solicitados": [
    {"recurso": "red", "accion": "outbound", "tipo": "persistente"},
    {"recurso": "/home/user/proyectos", "accion": "read", "tipo": "persistente"}
  ],
  "confirmacion_requerida": true,
  "sandbox_profile": "modulo_estandar"
}
```

### 4. Core valida
- Sintaxis correcta.
- Firma del manifiesto coincide con lo descargado (sin tampering).
- Detecta permisos de tipo `persistente` → dispara [[Tres-Categorias-de-Autorizacion|autorización de capacidades]] explícita, sin excepción, sin importar reputación del módulo.

### 5. Core resuelve la versión exacta
Usando el `constraint: "^2.3"`, el resolver interno del Core (similar a npm/apt) determina la versión exacta a instalar, ej. `2.3.1`. Ver [[Resolucion-de-Versiones]].

### 6. Confirmación de permisos
Core → Agente → Usuario: *"Este módulo pide acceso permanente a red y a tu carpeta de proyectos. ¿Confirmas?"*
Usuario: *"Sí, adelante."*

*(Si el usuario dice "no" → rama de [[Ramas-de-Fallo|Rechazo]]. No hay acción física, no hay journal, termina ahí.)*

### 7. Orquestador de permisos JIT otorga el permiso
Tipo `persistente`, sin expiración automática (`duracion: hasta_revocacion_manual`). Registra en el log de auditoría.

### 8. Orquestador de scheduling
No aplica (instalar un módulo no compite por CPU de forma crítica). Se omite sin fallo.

### 9. Core → Sandbox
Arma el paquete de ejecución: contrato validado (con versión exacta ya resuelta) + permisos otorgados + perfil de sandbox (`modulo_estandar`: namespaces, cgroup con límite de memoria, seccomp).

### 10. Sandbox ejecuta
Descomprime el módulo, corre su script de instalación **dentro del área temporal** `/tmp/build/...` (nunca directo a `/opt/modules/`). Registra qué archivos creó. Termina con éxito y devuelve al Core: `{"status": "success", "archivos_creados": [...], "hash_verificacion": "..."}`.

*(Si falla aquí → rama de [[Ramas-de-Fallo|Rollback]]. Ver ejemplo completo en [[Caso-Fallo-Rollback]].)*

### 11. Core verifica
Firma, hash, integridad, dependencias del artefacto producido en el área temporal.

### 12. Core hace commit atómico
Publica el artefacto verificado a `/opt/modules/pyassist-core/` usando `rename` (atómico).

### 13. Core actualiza FS en grafo y Journal
- FS en grafo: agrega nodos para los nuevos archivos, etiqueta `modulo:pyassist-core`, arista de dependencia hacia el intérprete de Python.
- Journal: `"[14:33] instalar_modulo pyassist-core v2.3.1 — éxito — snapshot pre-instalación: btrfs-snap-2847"`.

### 14. Core → Agente → Memoria persistente
Guarda **hechos**: *"Usuario instaló pyassist-core v2.3.1 el 31 de julio de 2026, confirmó permisos persistentes de red y lectura de /home/user/proyectos."*
Guarda **nota de continuidad**: *"Posible siguiente paso: preguntar si quiere configurar el módulo."*

Ver la distinción en [[Memoria-Persistente]].

### 15. Core → Usuario
Notificación: *"pyassist-core instalado correctamente. Tiene acceso a red y a tu carpeta de proyectos, como confirmaste. ¿Quieres que te ayude a configurarlo?"*

## Descubrimientos que salieron de este trazado

Trazar este caso concreto reveló varios huecos que el diseño abstracto no mostraba:
- La necesidad de distinguir [[Tres-Tipos-de-Permiso|tipos de permiso]] (JIT vs. persistente).
- La necesidad de [[Resolver-vs-Instalar|separar búsqueda de instalación]].
- La necesidad de que [[Resolucion-de-Versiones|el Core resuelva versiones]], no el agente.
- La necesidad de [[Fase-Commit-Atomico|build-then-commit]] (identificado en una revisión posterior al primer trazado).

## Relacionado
- [[Flujo-Canonico-Overview]]
- [[Caso-Fallo-Rollback]]
- [[Tres-Tipos-de-Permiso]]
- [[Resolucion-de-Versiones]]
- [[Resolver-vs-Instalar]]
