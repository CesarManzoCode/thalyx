---
tipo: pendientes
estado: activo
fecha-decreto: 2026-08-01
tags: [pendientes, tareas, roadmap-decisiones]
---

# Tareas pendientes (explícitas)

Lista viva de decisiones y trabajo que todavía falta cerrar. Actualizar el estado en el frontmatter de cada nota enlazada conforme se resuelvan.

> Para saber **qué está construido** en vez de qué está decidido, ver [[Estado-de-Implementacion]].
>
> Para saber **dónde quedó el proyecto y qué sigue ahora mismo**, ver [[Punto-Actual]].

## Pendientes de implementación

- [ ] **Un caso de aislamiento con un permiso sobre un archivo y usuario propio.** El 2026-08-04 un punto de montaje creado como directorio sobre un archivo rompió el `correr` de la máquina, y ninguna prueba lo vio porque **todos los permisos de todas las pruebas son directorios**. Lo cubre ahora una prueba unitaria de `create_target_like` y la etapa 16 en hardware; falta un caso en `isolation.rs` que arme la raíz remapeada de verdad sobre un archivo. Ver [[Estrategia-de-Pruebas]].
- [ ] **Cargar `thalyx_watch` con el cargador propio.** Es lo único que queda de la lista de "lo que falta comprobar" de [[Punto-Actual]]. Diez hooks en lugar de dos, y el único tipo de mapa que el watcher usa y el LSM no es `PERCPU_ARRAY`. Probable no es comprobado, y no se puede intentar en el contenedor: faltan las cabeceras de `libbpf` para compilar el objeto.
- [ ] **Probar `net/outbound` de punta a punta en hardware.** Que el LSM deniegue a un módulo sin la concesión está demostrado y es reproducible; que un módulo **con** la concesión abra una conexión está implementado, cubierto por pruebas unitarias y nunca ejercido en una máquina. Ver [[Permisos-JIT]].
- [ ] **Consumir el ringbuf `thalyx_mutations`** para saber *qué* cambió, no solo que algo cambió. El atajo ya no lo necesita — lo resolvió la atribución por ancestros — así que esto solo hace falta para reindexar de forma incremental en vez de reconstruir. Ver [[FS-en-Grafo]].


## Pendientes de decreto formal

- [x] **Con qué se cierra la Fase 1** — resuelto el 2026-08-06: **una ISO independiente**, que puesta en una PC sin sistema operativo la deje corriendo Thalyx. Sustituye a la persona ajena y conserva lo que ella aportaba: es una condición que el proyecto no se puede declarar a sí mismo. Ver [[Criterio-de-Salida-Fase-1]].
- [ ] **Si el arranque UEFI sin gestor de arranque no funciona, decidir qué se hace.** El plan es que no haya gestor: un kernel con `CONFIG_EFI_STUB` es una aplicación UEFI, así que el firmware carga el `bzImage` directo y el medio lleva **un archivo**. **No se ha construido.** Si resulta que hace falta GRUB o systemd-boot, eso es un segundo programa en la imagen y lo prohíbe [[Filosofia-Fundacional]] — es decisión de Cesar y no un detalle de construcción. Ver [[Construccion-del-ISO]].
- [ ] **Quién crea el store en una máquina que no tiene uno.** `store_disk.rs` decreta que PID 1 **monta y nunca fabrica**, porque una máquina que se inventa un store arranca perfecta el día que el disco no estaba. En una PC sin sistema operativo no hay store y `mkfs.btrfs` no puede ir en la imagen. Hay que revisar el decreto sin perder lo que protege: **«es la primera vez» y «no encontré el tuyo» tienen que seguir siendo hechos distintos**. Ver [[Construccion-del-ISO]].
- [ ] **Confirmar las gamas con el banco** — [[Gamas-de-Modelo]] decreta cuatro gamas de una familia como hipótesis de partida. Falta correrlas y sustituir los tamaños estimados por los medidos. Requiere una máquina con `llama.cpp` y acceso a los pesos; el contenedor de desarrollo no tiene ninguna de las dos.
- [ ] **Métricas de benchmark concretas** para la Fase 2 — qué se mide exactamente para el índice semántico y los permisos JIT, con qué carga y contra qué línea base. El umbral de decisión ya está decretado, lo que falta es el instrumento. Ver [[Decision-Kernel-vs-Userspace]].
- [ ] **Técnicas de interpretabilidad** aplicables al agente. Ver [[Interpretabilidad-Mecanicista]].
- [ ] **Arquitectura del índice semántico a mayor escala** — SQLite alcanza para Fase 1; falta saber a partir de qué volumen deja de alcanzar.
- [ ] **Sistema de reputación resistente a Sybil** — pospuesto deliberadamente. Ver [[Sistema-Reputacion-Sybil]].
- [ ] **Dependencias entre módulos y resolver con backtracking** — pospuesto hasta que exista un módulo real que las necesite. Ver [[Resolucion-de-Versiones]].
- [ ] **Decidir el ABI de los módulos: nativo de Linux o independiente de POSIX.** [[Filosofia-Fundacional]] dice que los módulos no hablan POSIX ni libc; hoy son binarios de Linux enlazados dinámicamente, con `/usr`, `/lib` y `/etc` montados de sólo lectura y unas ciento veinte llamadas al sistema permitidas. La distinción que sí se sostiene está escrita en [[Sistema-de-Modulos]] — la API es la única superficie *mediada*. Hacer verdadera la frase entera significa módulos estáticos sin libc, un rootfs sin `/usr`, un filtro mucho más chico, o un objetivo distinto como WASM. **Es barato ahora, con un módulo, y caro con un ecosistema encima**, así que decidirlo antes de escribir más módulos.
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

- [x] **Modelo concreto del agente** — el decreto que bloqueaba. No es un modelo: son cuatro gamas de una sola familia que elige el usuario según su hardware, con `llama.cpp` como proceso y gramática restringida. Ver [[Gamas-de-Modelo]] y [[Agente-Minimo]].
- [x] **Quién escribe la procedencia en el contrato** — el ensamblador, desde el canal de entrada; nunca el modelo. Ver [[Agente-Conversacional]].
- [x] **Límites de recursos contra un kernel que delegue controladores** — la corrida en Fedora 43 tenía `memory` y `pids` delegados, así que `verify.sh` activó `THALYX_REQUIRE_CONTROLLER_TESTS=1` y los saltos habrían sido fallos. Con `not proven 0`, se ejercitaron.
- [x] **Snapshots y `restore`** — la operación destructiva, con diff de lo que se pierde, confirmación por el camino confiable e intercambio atómico. Ver [[Rollback-vs-Restore]].
- [x] **Acotar la cuenta de mutaciones al árbol** — atribución subiendo por los ancestros del dentry, con la ausencia de montajes debajo como precondición comprobada. Verificado en hardware: 5000 escrituras dentro contadas, las mismas 5000 fuera ignoradas.
- [x] **La puerta del atajo del índice** — `thalyx graph trust`, que corre la verificación en el momento y se niega si no coincide.
- [x] **Escrituras por descriptor abierto en el watcher** — `lsm/file_permission` enmascarado a `MAY_WRITE`, más los siete hooks de forma del árbol que faltaban. El contador ya puede creerse en cuanto a cobertura. Ver [[FS-en-Grafo]].
- [x] **`thalyx rollback`** — deshace un commit de Thalyx, y se niega cuando la entrada del journal ya no describe el disco. Ver [[Rollback-vs-Restore]].

## Resueltos el 2026-08-02

- [x] **Ejecutar `lsm/` por primera vez** — se compiló, se cargó y se demostró denegando una conexión real dentro del cgroup mientras la misma conexión seguía funcionando fuera. Ver [[Permisos-JIT]].
- [x] **Montajes idmapped para las rutas concedidas** — implementados y verificados. Ver [[Sandbox-Ejecucion]].
- [x] **Con qué uid corre un módulo** — uno por módulo, sin reutilizar nunca. Decretado **e implementado**. Ver [[Sandbox-Ejecucion]].
- [x] **Sockets `AF_UNIX` en el sandbox** — se quedan fuera, y queda dicho que la decisión es reversible. Ver [[Sandbox-Ejecucion]].
- [x] **Memoria persistente** — tercera primitiva, construida y probada. Ver [[Memoria-Persistente]].
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

**El repositorio fue auditado desde fuera por primera vez el 2026-08-04**, y esa auditoría encontró nueve defectos reales — tres críticos — que ninguna de las 612 pruebas de entonces veía. Es la evidencia más directa que hay de que las pruebas escritas junto al código comparten sus supuestos, y de que la próxima victoria no es duplicar el tamaño sino que alguien hostil no pueda romper lo que ya se promete. Ver [[Punto-Actual]] y [[Estrategia-de-Pruebas]].



**Ningún decreto de esta bóveda ha sido contrastado con una persona ajena al proyecto.** Todo el razonamiento sobre por qué alguien elegiría Thalyx sigue siendo a priori. Ver [[Por-Que-Elegirian-Este-SO]] y [[Riesgo-de-Ejecucion]].

El [[Criterio-de-Salida-Fase-1|criterio de salida de la Fase 1]] estaba diseñado para forzar ese contacto, y **el 2026-08-06 Cesar lo suspendió**: Thalyx todavía son comandos de terminal y el producto terminado será una ISO booteable, así que se prueba cuando haya algo que probar. El riesgo se sigue cargando a propósito, ahora por más tiempo y con una medición más: **no se pudo convencer a nadie de dedicarle media hora de terminal**, que es una respuesta parcial a [[Por-Que-Elegirian-Este-SO]] y no un contratiempo de calendario.

## Relacionado
- [[00-Indice/Indice-Principal|Índice principal]]
- [[Notas-Tecnicas-Implementacion]]
