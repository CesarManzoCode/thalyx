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
| Journal append-only con fsync | `crates/thalyx-journal` | Completo salvo el registro de intención |
| Lectura y desempaquetado seguro de bundles | `crates/thalyx-core/bundle.rs` | Completo |
| Verificación de artefacto | `crates/thalyx-core/install.rs` | Completo |
| Commit atómico | `crates/thalyx-core/commit.rs` | Completo |
| Anclaje de clave de publicador (TOFU) | `crates/thalyx-core/keystore.rs` | Completo |
| Registro de permisos con vigencia condicionada | `crates/thalyx-core/permissions.rs` | Completo |
| Camino confiable | `crates/thalyx-core/trusted_path.rs` | Completo para autorización de capacidades |
| Puntos de inyección de fallos | `crates/thalyx-core/fault.rs` | Cuatro puntos sobre la ruta de instalación |
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

## No construido todavía

| Pieza | Bloqueante para |
|---|---|
| Registro de intención en el journal | Cerrar el hueco de [[Fase-Commit-Atomico]] |
| Índice en grafo y parser mecánico | [[FS-en-Grafo]], [[Parser-Mecanico]] |
| `thalyx-lsm` | [[Permisos-JIT]] real; hoy los permisos se registran pero **no se aplican** |
| `thalyx-permd` | Ídem |
| `thalyx-sandbox` | Ejecución de módulos en runtime |
| `thalyx-agent` | Todo el flujo conversacional |
| Contratos y marcado de origen | [[Contrato-Estructurado]], [[Marcado-de-Origen]] |
| Snapshots, `rollback` y `restore` | [[Rollback-vs-Restore]] |
| Memoria persistente | [[Memoria-Persistente]] |
| Imagen ISO | [[Construccion-del-ISO]] |

### La advertencia importante

**Hoy los permisos se registran pero nadie los aplica.** El registro de permisos es contabilidad honesta, no enforcement: hasta que exista `thalyx-lsm`, un módulo instalado no está contenido por nada. Es esperable en esta etapa, pero no debe describirse como si el sistema ya protegiera algo.

## Pruebas

57 pruebas en total, en los tres niveles de [[Estrategia-de-Pruebas]]. Los de nivel 2 matan el binario real con `SIGABRT` en cada punto del commit, incluido el instante entre los dos `rename`, y verifican consistencia **y recuperación**.

## Relacionado
- [[Tareas-Pendientes]]
- [[Estrategia-de-Pruebas]]
- [[Fases-de-Implementacion]]
- [[Criterio-de-Salida-Fase-1]]
