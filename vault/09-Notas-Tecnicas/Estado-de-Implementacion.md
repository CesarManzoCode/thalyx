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
| Parser mecánico | `crates/thalyx-parser` | Rust, Python, JS/TS, C, Go |
| Índice en grafo (SQLite) | `crates/thalyx-graph` | Nodos, aristas, etiquetas, obsolescencia |
| `thalyx-lsm` (BPF LSM) | `lsm/thalyx_lsm.bpf.c` | **Demostrado denegando en hardware real** |
| `thalyx-watch` (BPF LSM) | `lsm/thalyx_watch.bpf.c` | Enganchado; falta consumir los eventos |
| Entorno de desarrollo (VM) | `dev/` | Preflight, guest reproducible, verificación de enforcement |
| CLI `thalyx` | `crates/thalyx-cli` | `module`, `journal`, `permissions`, `store`, `dev` |
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

## No construido todavía

| Pieza | Bloqueante para |
|---|---|
| `thalyx-permd` | Conectar la política al mapa BPF; hoy los permisos se registran pero **no se aplican** |
| `thalyx-sandbox` | Ejecución de módulos en runtime |
| `thalyx-agent` | Todo el flujo conversacional |
| Snapshots, `rollback` y `restore` | [[Rollback-vs-Restore]] |
| Memoria persistente | [[Memoria-Persistente]] |
| Imagen ISO | [[Construccion-del-ISO]] |

### La advertencia importante

**El enforcement funciona, pero todavía no está conectado al resto.** `thalyx-lsm` deniega correctamente cuando alguien escribe una política en su mapa a mano. Lo que falta es `thalyx-permd`: el registro de permisos que llena el núcleo al instalar un módulo no llega al mapa por sí solo.

Es decir: el kernel puede aplicar permisos, y el núcleo sabe cuáles debería aplicar, y esas dos cosas todavía no se hablan. El registro de permisos es contabilidad honesta, no enforcement: hasta que exista `thalyx-lsm`, un módulo instalado no está contenido por nada. Es esperable en esta etapa, pero no debe describirse como si el sistema ya protegiera algo.

## Pruebas

129 pruebas en total, en los tres niveles de [[Estrategia-de-Pruebas]]. Los de nivel 2 matan el binario real con `SIGABRT` en cada punto del commit, incluido el instante entre los dos `rename`, y verifican consistencia **y recuperación**.

## Relacionado
- [[Tareas-Pendientes]]
- [[Estrategia-de-Pruebas]]
- [[Fases-de-Implementacion]]
- [[Criterio-de-Salida-Fase-1]]
