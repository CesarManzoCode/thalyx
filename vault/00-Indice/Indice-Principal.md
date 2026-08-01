---
tipo: indice
estado: activo
fecha-decreto: 2026-07-31
tags: [indice, moc, punto-de-entrada]
---

# Índice principal — SO con IA como ciudadana de primera clase

Punto de entrada de la bóveda. Si sos vos retomando el proyecto después de tiempo, o una IA a la que le compartís esta bóveda como contexto, **empezá por acá**.

## Resumen en una frase

Sistema operativo de código abierto, diseñado desde el núcleo hacia afuera, donde la IA es ciudadana de primera clase — no una aplicación más — y el humano sigue siendo el soberano.

## Orden de lectura sugerido

### 1. Fundamentos (por qué existe esto)
- [[Filosofia-Fundacional]] — la declaración central y los 6 principios rectores
- [[Principio-Doble-Ruta]] — el humano siempre puede operar sin el agente

### 2. Arquitectura general (cómo está construido)
- [[Arquitectura-Asimetrica]] — cara humana vs. cara IA
- [[Core-Nucleo]] — el núcleo del sistema
- [[Sistema-de-Modulos]] — el ecosistema de módulos `.osmod`
- [[Agente-Conversacional]] — el traductor de intención
- [[Decision-Kernel-vs-Userspace]] — qué vive en el kernel y qué en userspace
- [[Criterio-de-Inclusion-de-Primitivas]] — el filtro metodológico para decidir qué se construye ahora

### 3. Las primitivas (el diferencial técnico)
- [[Primitivas-Base-Overview]] — mapa de las 4 primitivas
- [[FS-en-Grafo]] · [[Permisos-JIT]] · [[Scheduler-Predictivo]] · [[Memoria-Persistente]]

### 4. El flujo canónico (la pieza central de diseño)
- [[Flujo-Canonico-Overview]] — las 9 piezas y el flujo completo
- [[Fase-Commit-Atomico]] — **la decisión técnica más importante** (build-then-commit)
- [[Ramas-de-Fallo]] — rechazo / rollback / degradación
- [[Contrato-Estructurado]] · [[Tres-Tipos-de-Permiso]] · [[Tres-Categorias-de-Autorizacion]]
- [[Caso-Instalar-Modulo]] — caso de referencia trazado completo
- [[Caso-Fallo-Rollback]] — caso de fallo trazado completo

### 5. Decisiones y debates (el porqué de cada cosa)
- [[Debates-Overview]] — mapa de todos los debates resueltos

### 6. Pendientes (qué falta)
- [[Tareas-Pendientes]] — lista viva, revisar antes de retomar el proyecto

### 7. Adopción y fases (cuándo y cómo se construye/lanza)
- [[Fases-de-Implementacion]] — las 4 fases del roadmap
- [[Condiciones-de-Adopcion]] — gates para abrir a usuarios (NO son de Fase 1)
- [[Por-Que-Elegirian-Este-SO]] — análisis honesto de propuesta de valor, con huecos de validación reconocidos

### 8. Investigación
- [[Interpretabilidad-Mecanicista]]

### 9. Notas técnicas
- [[Notas-Tecnicas-Implementacion]] — referencia rápida para cuando se escriba código

### 10. Contexto personal y de carrera
- [[Estrategia-Carrera]]
- [[Riesgo-de-Ejecucion]]

## Estado global del proyecto (snapshot al 31 de julio de 2026)

| Área | Estado |
|---|---|
| Filosofía y arquitectura | ✅ Decretado |
| Primitivas base (4) | ✅ Decretadas |
| Flujo canónico (9 piezas) | ✅ Decretado |
| Build-then-commit | ✅ Decretado |
| Tipos de permiso / autorización | ✅ Decretado |
| Ramas de fallo | ✅ Decretado |
| Caso "instalar módulo" trazado | ✅ Completo |
| Caso de fallo trazado | ✅ Completo |
| Resolución de versiones (mecanismo concreto) | ⚠️ Pendiente |
| Formato manifiesto `.osmod` | ⚠️ Pendiente |
| Detalle de sandboxing (namespaces/seccomp) | ⚠️ Pendiente |
| ISO booteable | ⚠️ Pendiente (diseño) |
| Agente: modelo/dataset de fine-tuning | ⚠️ Abierto, no bloqueante para Fase 1 |
| Interpretabilidad: técnicas concretas | ⚠️ Abierto |
| Validación con usuarios reales | ⚠️ No iniciada |

Ver detalle completo en [[Tareas-Pendientes]].

## Cómo mantener esta bóveda

- Cada nota tiene `estado` en su frontmatter (`decretado`, `pendiente`, `primitiva-futura`, `reflexion-abierta`, etc.). Usá eso para filtrar con Dataview si lo instalás.
- Al cerrar un pendiente, actualizá esta tabla y el estado de la nota correspondiente.
- Al abrir un nuevo debate, creá una nota en `05-Decisiones-y-Debates/` y enlazala desde [[Debates-Overview]].
