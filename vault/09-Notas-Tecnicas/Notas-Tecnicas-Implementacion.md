---
tipo: notas-tecnicas
estado: activo
fecha-decreto: 2026-08-01
tags: [implementacion, tecnico, referencia-rapida]
---

# Notas técnicas adicionales (para implementación)

Referencia rápida de supuestos y decisiones técnicas a tener presentes al escribir código. Cada punto enlaza a su nota completa.

## Commit y filesystem

- **Btrfs es obligatorio**, con subvolúmenes separados para sistema, módulos y datos de usuario.
- **El área de staging va en el mismo subvolumen que el destino**, nunca en `/tmp`. `rename` devuelve `EXDEV` al cruzar filesystems **y también al cruzar subvolúmenes de Btrfs**.
- **La publicación son dos `rename`:** el directorio versionado primero, el enlace simbólico `current` después. `rename` sobre un directorio no vacío da `ENOTEMPTY`, así que una actualización no puede publicarse renombrando el destino. Ver [[Fase-Commit-Atomico]].
- **El invariante `publicado o no publicado, nunca a medias` se demuestra con tests**, no se afirma. Ver [[Estrategia-de-Pruebas]].

## Verificación y módulos

- **Instalar no ejecuta código del módulo.** En Fase 1 los artefactos son prebuildeados y firmados. Ver [[Verificacion-y-Distribucion]].
- **El Core recalcula los hashes.** Nunca acepta el resultado de verificación que reporte un componente fuera de la TCB.
- **Manifiesto en TOML, contrato en JSON.** Firma detached ed25519. Un cambio de clave para un `id` conocido es un error duro. Ver [[Formato-Manifiesto-Thmod]].
- **El resolver es un `max` sobre versiones que satisfacen el constraint**, porque en Fase 1 no hay dependencias entre módulos. Ver [[Resolucion-de-Versiones]].

## Seguridad

- **El agente y el sandbox están fuera de la TCB.** Ver [[Modelo-de-Amenaza]].
- **Cada campo del contrato lleva su origen**, y el Core rechaza campos con efecto originados en contenido no confiable. Ver [[Marcado-de-Origen]].
- **Las confirmaciones las genera y renderiza el Core**, con plantilla fija, por un canal que el agente no controla. Ver [[Camino-Confiable]].
- **Los permisos del manifiesto son los permisos efectivos.** El contrato no puede ampliarlos, y al usuario se le presenta el conjunto del manifiesto.
- **Los permisos confirmados quedan pendientes hasta el commit.** Sin commit, se descartan.
- **El sandbox es de implementación propia**: namespaces, seccomp por lista de permitidos, cgroup v2, sin red por defecto. Ver [[Sandbox-Ejecucion]].

## Estado y coherencia

- **El filesystem es la fuente de verdad; el índice es caché.** Toda consulta declara si está al día.
- **El LSM intercepta mutaciones y encola sin bloquear.** Re-parsear dentro del hook colgaría el filesystem.
- **Si la cola se desborda, se falla cerrado**: los nodos se marcan obsoletos, nunca se descarta un evento en silencio.
- **Ninguna operación destructiva confía en el registro**: se compara contra el disco y se detiene si hay cambios ajenos. Ver [[Coherencia-Doble-Ruta]].
- **`rollback` y `restore` son comandos distintos.** Ver [[Rollback-vs-Restore]].
- **Un contrato a la vez**, bajo lock global del Core. Ver [[Concurrencia]].

## Agente

- **Empieza con reglas escritas a mano y embeddings.** El fine-tuning es de fases posteriores. Ver [[Debate-Agente-Fine-Tuning]].
- **Sin llamadas a modelos remotos en Fase 1.** Ver [[Agente-Conversacional]].

## Estructura del código

- **El Core es un solo binario con módulos internos de fronteras duras** y sin estado mutable compartido. Ver [[Core]].
- **Todo el código en inglés.** Ver [[Nomenclatura-y-Convenciones]].
- **Licencia:** GPLv3 en userspace, GPLv2 en el LSM. Ver [[Decision-Licencia]].

## Relacionado
- [[Tareas-Pendientes]]
- [[Estrategia-de-Pruebas]]
- [[Fase-Commit-Atomico]]
