---
tipo: notas-tecnicas
estado: activo
fecha-decreto: 2026-08-01
tags: [implementacion, estado, fase-1]
---

# Estado de implementación

Qué está construido de lo que está decretado. Esta nota se actualiza con cada avance de código; es lo primero que hay que leer al retomar el proyecto después de tiempo.

**No confundir con [[Tareas-Pendientes]]**, que lista decisiones sin cerrar. Aquí se lista código.

## Construido

| Pieza | Dónde | Estado |
|---|---|---|
| Manifiesto `.thmod`: parseo y validación | `crates/thalyx-manifest` | Completo para el schema v1 |
| Firma ed25519 sobre forma canónica | `crates/thalyx-manifest` | Completo |
| Journal append-only con fsync | `crates/thalyx-journal` | Completo |
| Registro de intención y reconciliación | `crates/thalyx-core/reconcile.rs` | Completo |
| Lectura y desempaquetado seguro de bundles | `crates/thalyx-core/bundle.rs` | Completo |
| Verificación de artefacto | `crates/thalyx-core/install.rs` | Completo |
| Commit atómico | `crates/thalyx-core/commit.rs` | Completo |
| Anclaje de clave de publicador (TOFU) | `crates/thalyx-core/keystore.rs` | Completo |
| Registro de permisos con vigencia condicionada | `crates/thalyx-core/permissions.rs` | Completo |
| Camino confiable | `crates/thalyx-core/trusted_path.rs` | Completo para autorización de capacidades |
| Puntos de inyección de fallos | `crates/thalyx-core/fault.rs` | Cuatro puntos sobre la ruta de instalación |
| Contrato con marcado de origen | `crates/thalyx-contract` | Schema v1, procedencia por campo, contención |
| `thalyx-permd` (política → mapa BPF) | `crates/thalyx-permd` | Traducción, codificación y escritura |
| Manifiesto guardado junto al módulo | `crates/thalyx-core/install.rs` | Verbatim, con su firma, re-verificado en cada lectura |
| Identidad cgroup v2 del módulo | `crates/thalyx-sandbox/cgroup.rs` | Probado contra un montaje real |
| Lanzamiento confinado (re-exec) | `crates/thalyx-sandbox/launch.rs` | Probado: el programa reporta su propio cgroup |
| Orquestación de ejecución | `crates/thalyx-core/run.rs` | `thalyx module run`, ciclo completo |
| Parser mecánico | `crates/thalyx-parser` | Rust, Python, JS/TS, C, Go |
| Índice en grafo (SQLite) | `crates/thalyx-graph` | Nodos, aristas, etiquetas, obsolescencia |
| `thalyx-lsm` (BPF LSM) | `lsm/thalyx_lsm.bpf.c` | **Demostrado denegando en hardware real** |
| `thalyx-watch` (BPF LSM) | `lsm/thalyx_watch.bpf.c` | Enganchado; falta consumir los eventos |
| Entorno de desarrollo (VM) | `dev/` | Preflight, guest reproducible, verificación de enforcement |
| CLI `thalyx` | `crates/thalyx-cli` | `module` (con `run`), `journal`, `permissions`, `enforce`, `store`, `dev` |
| Empaquetado de módulos | `crates/thalyx-cli/dev.rs` | `keygen`, `pack`, `inspect` |

### Decretos que el código ya hace cumplir

- El área de staging vive en el subvolumen del destino, nunca en `/tmp`.
- La publicación es `rename` de directorio + intercambio atómico de symlink.
- Instalar no ejecuta código del módulo.
- El núcleo recalcula el hash; no acepta el que le reporten.
- Las solicitudes de autorización las genera el núcleo desde campos del manifiesto, sin ningún parámetro de texto libre.
- Se presenta el conjunto completo de permisos del manifiesto, no un subconjunto.
- Un cambio de clave para un `id` conocido es error duro.
- Los permisos solo tienen vigencia mientras el módulo sea la versión actual.
- El journal declara su propio alcance al mostrarse.
- Silencio no es consentimiento: sin terminal, la confirmación se rechaza.
- Una operación interrumpida deja una intención sin resolver, no un vacío, y la reconciliación la resuelve contra el disco.
- El filesystem es la verdad: el índice es un caché y **toda consulta devuelve su grado de actualización junto con las filas**, de modo que quien lee no puede olvidarse de la advertencia.
- El índice falla cerrado: lo que no se puede determinar cuenta como obsoleto.
- Una referencia que apunta fuera del árbol se conserva sin destino en vez de inventarse uno.
- Cada campo con efecto del contrato declara su procedencia, y el núcleo rechaza los que vienen de contenido no confiable **antes de abrir nada**.
- Un origen ausente se rechaza, no se asume confiable.
- El journal registra el origen **menos** confiable del contrato, no el más.
- La política está en el kernel **antes** de que el proceso esté en el cgroup, y el proceso está en el cgroup **antes** de la primera instrucción del módulo.
- Al terminar, la política se retira **antes** de borrar el cgroup: el id es un número de inodo y los inodos se reutilizan.
- Un módulo cuyos permisos nada puede aplicar **no se ejecuta**, salvo que se pida explícitamente y el journal lo registre como degradado.
- Un módulo no puede escribir en `.thalyx/` dentro de su propio árbol: ahí vive el registro de lo que tiene permitido hacer.
- El modo de los archivos del artefacto se aplica enmascarado: setuid, setgid y sticky nunca sobreviven a una instalación.

## No construido todavía

| Pieza | Bloqueante para |
|---|---|
| Perfil `module_standard` completo (namespaces, seccomp, límites) | Aislamiento real del módulo |
| `thalyx-agent` | Todo el flujo conversacional |
| Snapshots, `rollback` y `restore` | [[Rollback-vs-Restore]] |
| Memoria persistente | [[Memoria-Persistente]] |
| Imagen ISO | [[Construccion-del-ISO]] |

### La advertencia importante

**El ciclo de enforcement ya se cierra solo, pero el sandbox todavía no aísla.**

`thalyx module run` escribe la política, lanza el módulo dentro del cgroup al que esa política está atada, y retira ambos al terminar. Nadie tiene que invocar `thalyx enforce apply` a mano. Lo que falta es lo otro: el perfil `module_standard` decreta namespaces, un filtro seccomp por lista de permitidos y límites de recursos, y **nada de eso está implementado**.

Un módulo lanzado hoy está contenido por `thalyx-lsm` y por nada más. Eso significa que las operaciones que el LSM engancha —red saliente, apertura de archivos— se deniegan si no fueron concedidas, y todo lo demás no está contenido en absoluto. No es aislamiento; es enforcement de permisos sobre un proceso que por lo demás corre como cualquier otro.

Decirlo así importa más que lo construido. La descripción cómoda —"los módulos corren en un sandbox"— sería falsa hoy, y el día que alguien la crea es el día que un módulo hace algo que nadie esperaba.

## Pruebas

185 pruebas en total, en los tres niveles de [[Estrategia-de-Pruebas]]. Los de nivel 2 matan el binario real con `SIGABRT` en cada punto del commit, incluido el instante entre los dos `rename`, y verifican consistencia **y recuperación**.

Las pruebas de cgroup corren contra un montaje cgroup2 real. Donde no lo hay, **imprimen `NOT PROVEN` y dicen que no probaron nada** en vez de pasar en silencio; con `THALYX_REQUIRE_CGROUP_TESTS=1` el salto se convierte en fallo. Una prueba que pasa sin haber ejercitado lo que nombra es exactamente cómo una herramienta de seguridad llega a leerse como armada estando desarmada.

## Relacionado
- [[Tareas-Pendientes]]
- [[Estrategia-de-Pruebas]]
- [[Fases-de-Implementacion]]
- [[Criterio-de-Salida-Fase-1]]
