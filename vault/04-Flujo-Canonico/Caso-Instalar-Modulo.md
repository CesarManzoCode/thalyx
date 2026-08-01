---
tipo: caso-de-uso
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, caso-de-uso, referencia-canonica]
---

# Caso de uso: "Instala el mejor módulo de Python con asistencia de IA"

Este es el **caso de referencia canónico** usado para validar todas las piezas del [[Flujo-Canonico-Overview|flujo canónico]]. Se eligió porque toca prácticamente todas las piezas, a diferencia de casos más simples como "borrar temporales" que dejarían la memoria persistente sin ejercitar.

> **Nota de estado:** este trazado incorpora los decretos del 1 de agosto de 2026 — [[Verificacion-y-Distribucion|instalar no ejecuta código]], [[Camino-Confiable|camino confiable]], [[Marcado-de-Origen|marcado de origen]], [[Fase-Commit-Atomico|mecanismo real del commit]] y [[Resolucion-de-Versiones|resolución sin transitivas]]. Es la versión final y actualizada.

## Trazado paso a paso

### 1. Usuario
Escribe: *"Quiero instalar un módulo para programar en Python con asistencia de IA. Instala el mejor."*

### 2. Agente — sub-tarea de consulta (sin contrato todavía)
Ver [[Resolver-vs-Instalar]]: la búsqueda no genera contrato, es una sub-tarea de lectura.

- Consulta el repositorio comunitario, filtra por categoría, ordena por reputación y descargas.
- Presenta opciones: *"Encontré 'pyassist-core' (4.8★, 12k descargas, pide acceso a red y a tu carpeta de proyectos) y 'py-tutor-lite' (4.9★, 3k descargas, solo acceso local)."*
- Usuario: *"Instala pyassist-core."*

> Todo el texto que el agente leyó del repositorio queda marcado como `untrusted_content` y **no puede originar campos con efecto** en el contrato. Ver [[Marcado-de-Origen]].

### 3. Agente genera el contrato

```json
{
  "version": "1.0",
  "operation": "install_module",
  "targets": ["org.publisher.pyassist"],
  "constraint": "^2.3",
  "permissions": [
    {"resource": "net", "action": "outbound", "type": "persistent"},
    {"resource": "/home/user/projects", "action": "read", "type": "persistent"}
  ],
  "requires_confirmation": true,
  "sandbox_profile": "module_standard",
  "caller": {"module_id": "thalyx-agent", "request_id": "abc-123"},
  "origins": {
    "operation": "user_utterance",
    "targets": "user_utterance",
    "constraint": "system_state",
    "permissions": "system_state"
  }
}
```

### 4. Core valida
- Sintaxis y schema, incluida la presencia de `version`.
- **Origen de los campos con efecto.** Si alguno viniera de `untrusted_content`, se rechaza aquí.
- Firma del manifiesto, verificada contra la clave anclada al `id`.
- **Contención de permisos:** los del contrato deben estar contenidos en los del manifiesto.
- Detecta permisos de tipo `persistent` → dispara [[Tres-Categorias-de-Autorizacion|autorización de capacidades]], sin excepción, sin importar la reputación del módulo.

### 5. Core resuelve la versión exacta
Con `constraint: "^2.3"`, elige la versión máxima publicada que lo satisface y cuya firma valida: `2.3.1`. Sin backtracking, porque el módulo no depende de otros módulos. Ver [[Resolucion-de-Versiones]].

### 6. Confirmación por el camino confiable
**El Core genera y renderiza la solicitud**, a partir de los campos ya validados y con plantilla fija. El agente no la compone ni la transporta. Ver [[Camino-Confiable]].

El texto presenta **el conjunto completo de permisos del manifiesto**, no solo los que el agente mencionó:

> *Thalyx — Autorización de capacidades*
> *`org.publisher.pyassist` v2.3.1 solicita, de forma permanente:*
> *· salida a red*
> *· lectura de /home/user/projects*
> *¿Confirmás?*

Usuario: *"Sí."*

*(Si el usuario dice "no" → rama de [[Ramas-de-Fallo|Rechazo]]. No hay acción física, no hay journal, termina ahí.)*

### 7. Permisos registrados como pendientes
`thalyx-permd` registra los permisos confirmados en estado **pendiente**, atados al `request_id`. **Todavía no son efectivos.** Ver [[Permisos-JIT]].

### 8. Core → Sandbox
Arma el paquete de ejecución: contrato validado con versión exacta + perfil `module_standard`.

### 9. Sandbox desempaqueta y valida estructura
Descomprime el artefacto **firmado y prebuildeado** en `/opt/thalyx/.staging/<uuid>/`, en el mismo subvolumen Btrfs que el destino final.

**No ejecuta código del módulo.** Verifica que los archivos queden dentro de las rutas declaradas y que no haya enlaces que escapen del árbol. Ver [[Verificacion-y-Distribucion]].

*(Si falla aquí → rama de [[Ramas-de-Fallo|Rollback]]. Ver [[Caso-Fallo-Rollback]].)*

### 10. Core verifica
Recalcula el hash del artefacto **por su cuenta** — no acepta el que reporte el Sandbox — y lo compara contra el declarado en el manifiesto firmado. Verifica integridad y `requires.thalyx`.

### 11. Core hace commit atómico
1. `rename("/opt/thalyx/.staging/<uuid>", "/opt/thalyx/modules/org.publisher.pyassist/2.3.1")`
2. `rename` del symlink `current` → `2.3.1`

En el instante del paso 2, el módulo está instalado. Ver [[Fase-Commit-Atomico]].

### 12. Los permisos pendientes se vuelven efectivos
En el mismo commit. Si no hubiera habido commit, se habrían descartado sin dejar rastro en el registro de permisos activos.

### 13. Core actualiza el índice y el Journal
- Índice: nodos para los archivos nuevos, etiqueta `module:org.publisher.pyassist`, arista de dependencia hacia el intérprete de Python.
- Journal: `"[14:33] install_module org.publisher.pyassist v2.3.1 — éxito — snapshot pre-instalación: btrfs-snap-2847 — cadena de origen: user_utterance"`.

### 14. Core → Memoria persistente
**Hechos:** *"Usuario instaló org.publisher.pyassist v2.3.1 el 31 de julio de 2026, confirmó permisos persistentes de red y lectura de /home/user/projects."* Fechado contra el estado del índice.
**Nota de continuidad:** *"Posible siguiente paso: preguntar si quiere configurar el módulo."*

Ver [[Memoria-Persistente]].

### 15. Core → Usuario
*"pyassist-core instalado correctamente. Tiene acceso a red y a tu carpeta de proyectos, como confirmaste. ¿Querés que te ayude a configurarlo?"*

## Descubrimientos que salieron de este trazado

Trazar este caso concreto reveló huecos que el diseño abstracto no mostraba:

- La necesidad de distinguir [[Tres-Tipos-de-Permiso|tipos de permiso]] (JIT vs. persistente).
- La necesidad de [[Resolver-vs-Instalar|separar búsqueda de instalación]].
- La necesidad de que [[Resolucion-de-Versiones|el Core resuelva versiones]], no el agente.
- La necesidad de [[Fase-Commit-Atomico|build-then-commit]].
- Que la "verificación de hash" del trazado original **no tenía referente** contra el cual comparar. Ver [[Verificacion-y-Distribucion]].
- Que la confirmación humana viajaba a través del componente que no es confiable. Ver [[Camino-Confiable]].

## Revisiones

### 2026-08-01 — Re-trazado completo con los decretos del bloque de seguridad
**Antes:** el paso 10 ejecutaba el script de instalación dentro del sandbox y devolvía un `hash_verificacion` que el Core aceptaba; la confirmación viajaba `Core → Agente → Usuario`; el permiso persistente se otorgaba antes de la verificación; y el commit publicaba de `/tmp/build/` a `/opt/modules/`.
**Ahora:** instalación sin ejecución de código, hash recalculado por el Core, confirmación por camino confiable con el conjunto completo del manifiesto, permisos efectivos dentro del commit, y staging en el mismo subvolumen con publicación por symlink.
**Motivo:** cada uno de esos cuatro puntos era un fallo concreto, no una mejora estilística. El trazado seguía siendo la mejor herramienta de la bóveda para encontrarlos — exactamente como la primera vez.

## Relacionado
- [[Flujo-Canonico-Overview]]
- [[Caso-Fallo-Rollback]]
- [[Verificacion-y-Distribucion]]
- [[Camino-Confiable]]
- [[Marcado-de-Origen]]
- [[Tres-Tipos-de-Permiso]]
- [[Resolucion-de-Versiones]]
- [[Resolver-vs-Instalar]]
