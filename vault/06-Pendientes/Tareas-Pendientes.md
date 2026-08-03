---
tipo: pendientes
estado: activo
fecha-decreto: 2026-08-01
tags: [pendientes, tareas, roadmap-decisiones]
---

# Tareas pendientes (explícitas)

Lista viva de decisiones y trabajo que todavía falta cerrar. Actualizar el estado en el frontmatter de cada nota enlazada conforme se resuelvan.

> Para saber **qué está construido** en vez de qué está decidido, ver [[Estado-de-Implementacion]].

## Pendientes de implementación

- [ ] **Ejecutar `lsm/` por primera vez.** Escrito sin poder compilarlo. Hasta que `make -C dev check` pase en una máquina real, es una propuesta.
- [ ] **Snapshots y `restore`** — requieren Btrfs, no probables en el entorno actual. `rollback`, que es la operación acotada y no necesita snapshots, ya está construido.
- [ ] **Probar los límites de recursos contra un kernel que delegue controladores** — `THALYX_REQUIRE_CONTROLLER_TESTS=1`.
- [ ] **Consumir el ringbuf `thalyx_mutations`** para saber *qué* cambió, no solo que algo cambió. El atajo ya no lo necesita — lo resolvió la atribución por ancestros — así que esto solo hace falta para reindexar de forma incremental en vez de reconstruir. Ver [[FS-en-Grafo]].


## Pendientes de decreto formal

- [ ] **Modelo concreto del agente** — qué modelo local de 3B-7B, con qué prompting y qué router de reglas. No bloquea la Fase 1, pero es el riesgo técnico más grande del proyecto. Ver [[Debate-Agente-Fine-Tuning]].
- [ ] **Métricas de benchmark concretas** para la Fase 2 — qué se mide exactamente para el índice semántico y los permisos JIT, con qué carga y contra qué línea base. El umbral de decisión ya está decretado, lo que falta es el instrumento. Ver [[Decision-Kernel-vs-Userspace]].
- [ ] **Técnicas de interpretabilidad** aplicables al agente. Ver [[Interpretabilidad-Mecanicista]].
- [ ] **Arquitectura del índice semántico a mayor escala** — SQLite alcanza para Fase 1; falta saber a partir de qué volumen deja de alcanzar.
- [ ] **Sistema de reputación resistente a Sybil** — pospuesto deliberadamente. Ver [[Sistema-Reputacion-Sybil]].
- [ ] **Dependencias entre módulos y resolver con backtracking** — pospuesto hasta que exista un módulo real que las necesite. Ver [[Resolucion-de-Versiones]].
- [ ] **Condiciones para habilitar llamadas a modelos remotos** — las reglas ya están escritas; falta decidir cuándo se activan. Ver [[Agente-Conversacional]].

## Resueltos el 2026-08-01

- [x] **Nombre del sistema y nomenclatura** — Thalyx. Ver [[Nomenclatura-y-Convenciones]].
- [x] **Licencia** — GPLv3 en userspace, GPLv2 en kernel. Ver [[Decision-Licencia]].
- [x] **Modelo de amenaza y definición de la TCB** — ver [[Modelo-de-Amenaza]].
- [x] **Formato exacto del manifiesto `.thmod`** — ver [[Formato-Manifiesto-Thmod]].
- [x] **Mecanismo de resolución de versiones** — ver [[Resolucion-de-Versiones]].
- [x] **Mecanismo de sandboxing en detalle** — ver [[Sandbox-Ejecucion]].
- [x] **Diseño del ISO booteable** — ver [[Construccion-del-ISO]].
- [x] **Mecanismo real del commit atómico** — ver [[Fase-Commit-Atomico]].
- [x] **Defensa contra inyección de prompts** — ver [[Marcado-de-Origen]].
- [x] **Camino confiable para la confirmación humana** — ver [[Camino-Confiable]].
- [x] **Coherencia entre doble ruta y estado del sistema** — ver [[Coherencia-Doble-Ruta]].
- [x] **Semántica de rollback frente a restore** — ver [[Rollback-vs-Restore]].
- [x] **Modelo de concurrencia** — ver [[Concurrencia]].
- [x] **Criterio de salida de la Fase 1** — ver [[Criterio-de-Salida-Fase-1]].
- [x] **Estrategia de pruebas** — ver [[Estrategia-de-Pruebas]].
- [x] **Registro de intención en el journal** — implementado y probado. Ver [[Fase-Commit-Atomico]].
- [x] **Contrato estructurado y marcado de origen** — implementados y probados de punta a punta.
- [x] **`thalyx-permd`** — traducción de permisos a política de kernel, implementada y probada.
- [x] **Índice en grafo y parser mecánico** — implementados. Ver [[Estado-de-Implementacion]].
- [x] **Ubicación de los permisos JIT (kernel vs userspace)** — ver [[Permisos-JIT]].
- [x] **Modo de actualización del índice en grafo** — ver [[Parser-Mecanico]] y [[Coherencia-Doble-Ruta]].
- [x] **FUSE dentro o fuera de Fase 1** — fuera. Ver [[Decision-Kernel-vs-Userspace]].
- [x] **Zona gris del umbral de migración** — ver [[Decision-Kernel-vs-Userspace]].
- [x] **Filesystem requerido** — Btrfs. Ver [[Journal-y-Snapshots]].
- [x] **Alcance de la Fase 1** — ver [[Fases-de-Implementacion]].

## Resueltos el 2026-08-03

- [x] **Acotar la cuenta de mutaciones al árbol** — atribución subiendo por los ancestros del dentry, con la ausencia de montajes debajo como precondición comprobada. Verificado en hardware: 5000 escrituras dentro contadas, las mismas 5000 fuera ignoradas.
- [x] **La puerta del atajo del índice** — `thalyx graph trust`, que corre la verificación en el momento y se niega si no coincide.
- [x] **Escrituras por descriptor abierto en el watcher** — `lsm/file_permission` enmascarado a `MAY_WRITE`, más los siete hooks de forma del árbol que faltaban. El contador ya puede creerse en cuanto a cobertura. Ver [[FS-en-Grafo]].
- [x] **`thalyx rollback`** — deshace un commit de Thalyx, y se niega cuando la entrada del journal ya no describe el disco. Ver [[Rollback-vs-Restore]].

## Resueltos el 2026-08-02

- [x] **Montajes idmapped para las rutas concedidas** — implementados y verificados. Ver [[Sandbox-Ejecucion]].
- [x] **Con qué uid corre un módulo** — uno por módulo, sin reutilizar nunca. Decretado **e implementado**. Ver [[Sandbox-Ejecucion]].
- [x] **Sockets `AF_UNIX` en el sandbox** — se quedan fuera, y queda dicho que la decisión es reversible. Ver [[Sandbox-Ejecucion]].
- [x] **Memoria persistente** — cuarta primitiva, construida y probada. Ver [[Memoria-Persistente]].
- [x] **Raíz propia del módulo (`pivot_root`)** — el módulo ya no ve el árbol del host. Ver [[Sandbox-Ejecucion]].
- [x] **Perfil `module_standard`** — namespaces, seccomp y límites, verificados contra el kernel real. Ver [[Sandbox-Ejecucion]].
- [x] **Dónde vive el `unsafe`** — en `thalyx-syscall` y en ningún otro lado. Ver [[Sandbox-Ejecucion]].
- [x] **Cierre del ciclo de enforcement** — `thalyx module run` establece la contención sola. Ver [[Sandbox-Ejecucion]].
- [x] **Identidad cgroup del módulo y orden de lanzamiento** — probados contra un montaje cgroup2 real.
- [x] **Dónde vive el manifiesto de un módulo instalado** — junto al módulo, publicado por el mismo `rename`, re-verificado en cada lectura.

## Resueltos antes (referencia histórica)

- [x] Re-trazar el caso de "instalar módulo" con build-then-commit — ver [[Caso-Instalar-Modulo]].
- [x] Trazar un caso de fallo/rollback explícito — ver [[Caso-Fallo-Rollback]].
- [x] Decidir si "resolver módulo" es contrato separado o sub-tarea sin contrato — sub-tarea sin contrato. Ver [[Resolver-vs-Instalar]].

## Lo que sigue sin validarse

**Ningún decreto de esta bóveda ha sido contrastado con una persona ajena al proyecto.** Todo el razonamiento sobre por qué alguien elegiría Thalyx sigue siendo a priori. Ver [[Por-Que-Elegirian-Este-SO]] y [[Riesgo-de-Ejecucion]].

El [[Criterio-de-Salida-Fase-1|criterio de salida de la Fase 1]] está diseñado para forzar ese contacto: no se cierra la fase sin que alguien de fuera use el sistema.

## Relacionado
- [[00-Indice/Indice-Principal|Índice principal]]
- [[Notas-Tecnicas-Implementacion]]
