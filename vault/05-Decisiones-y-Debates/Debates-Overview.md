---
tipo: overview
estado: decretado
fecha-decreto: 2026-08-01
tags: [debates, moc, historial]
---

# Decisiones clave: el debate y su resolución

Mapa de contenido de todos los debates que se dieron durante el diseño del proyecto, con su resolución decretada. Sirve como historial de *por qué* se tomó cada decisión, no solo *qué* se decidió.

## Debates de arquitectura

1. [[Decision-Capa-vs-SO-Nuevo]] — ¿es una capa sobre Linux o un SO nuevo?
2. [[Decision-Kernel-vs-Userspace]] — ¿qué vive en el kernel y qué en userspace?
3. [[Debate-Agente-Fine-Tuning]] — ¿qué pasa con el agente y el fine-tuning?
4. [[Decision-Licencia]] — ¿qué licencia, y por qué GPLv3 no alcanza?

## Debates de ecosistema

5. [[Sistema-Reputacion-Sybil]] — ¿qué pasa con el sistema de reputación anti-Sybil?
6. [[Debate-Core-Modules]] — ¿qué es un "Core Module" y quién decide?

## Debates de ejecución

7. [[Debate-Conflicto-Recursos]] — ¿qué pasa cuando dos módulos piden el mismo recurso?
8. [[Que-es-una-Tarea]] — ¿qué es exactamente una "tarea" en la memoria persistente?

## Decisiones del flujo canónico

Viven en `04-Flujo-Canonico/` porque cada una tiene consecuencias directas sobre el trazado:

- [[Fase-Commit-Atomico]] — separar producir de publicar, y cómo se publica de verdad.
- [[Verificacion-y-Distribucion]] — ¿los módulos se buildean localmente o llegan prebuildeados y firmados?
- [[Ramas-de-Fallo]] — rechazo, rollback y degradación son tres cosas distintas.
- [[Rollback-vs-Restore]] — dos operaciones, dos comandos.
- [[Coherencia-Doble-Ruta]] — cómo conviven la doble ruta y el estado del sistema.
- [[Resolucion-de-Versiones]] — por qué el resolver difícil se pospone.
- [[Resolver-vs-Instalar]] — la búsqueda no genera contrato.
- [[Concurrencia]] — un contrato a la vez.

## Decisiones de seguridad

Viven en `11-Seguridad/`:

- [[Modelo-de-Amenaza]] — contra quién defiende Thalyx, y qué está en la TCB.
- [[Camino-Confiable]] — quién le habla al humano cuando hay que autorizar.
- [[Marcado-de-Origen]] — por qué el contenido no confiable no puede originar acciones.

## Relacionado
- [[00-Indice/Indice-Principal|Índice principal]]
