---
tipo: indice
estado: activo
fecha-decreto: 2026-08-01
tags: [indice, moc, punto-de-entrada]
---

# Índice principal — Thalyx

Punto de entrada de la bóveda. Si eres tú retomando el proyecto después de tiempo, o una IA a la que le compartes esta bóveda como contexto, **empieza por aquí**.

## Resumen en una frase

**Thalyx** es un sistema operativo de código abierto, diseñado desde el núcleo hacia afuera, donde la IA es ciudadana de primera clase — no una aplicación más — y el humano sigue siendo el soberano.

## Orden de lectura sugerido

### 1. Fundamentos (por qué existe esto)
- [[Filosofia-Fundacional]] — la declaración central y los 6 principios rectores
- [[Principio-Doble-Ruta]] — el humano siempre puede operar sin el agente
- [[Nomenclatura-y-Convenciones]] — nombres, extensiones y política de idioma
- [[Decision-Licencia]] — GPLv3 en userspace, GPLv2 en kernel

### 2. Arquitectura general (cómo está construido)
- [[Arquitectura-Asimetrica]] — cara humana vs. cara IA
- [[Core-Nucleo]] — el núcleo del sistema operativo
- [[Core]] — el orquestador del flujo, y por qué es un solo proceso con módulos internos
- [[Sistema-de-Modulos]] — el ecosistema de módulos `.thmod`
- [[Formato-Manifiesto-Thmod]] — el schema del manifiesto
- [[Agente-Conversacional]] — el traductor de intención
- [[Decision-Kernel-vs-Userspace]] — qué vive dónde, y el umbral de migración
- [[Criterio-de-Inclusion-de-Primitivas]] — el filtro metodológico para decidir qué se construye ahora

### 3. Las primitivas (el diferencial técnico)
- [[Primitivas-Base-Overview]] — mapa de las 4 primitivas
- [[FS-en-Grafo]] · [[Permisos-JIT]] · [[Memoria-Persistente]] · [[Scheduler-Predictivo]]
- [[Parser-Mecanico]] — el motor que produce el grafo

### 4. El flujo canónico (la pieza central de diseño)
- [[Flujo-Canonico-Overview]] — las 9 piezas, el flujo completo y las fronteras de confianza
- [[Fase-Commit-Atomico]] — **la decisión técnica más importante** (build-then-commit)
- [[Verificacion-y-Distribucion]] — instalar no ejecuta código
- [[Coherencia-Doble-Ruta]] — cómo conviven la doble ruta y el estado del sistema
- [[Ramas-de-Fallo]] · [[Rollback-vs-Restore]] · [[Concurrencia]]
- [[Contrato-Estructurado]] · [[Tres-Tipos-de-Permiso]] · [[Tres-Categorias-de-Autorizacion]]
- [[Caso-Instalar-Modulo]] — caso de referencia trazado completo
- [[Caso-Fallo-Rollback]] — caso de fallo trazado completo

### 5. Seguridad
- [[Modelo-de-Amenaza]] — contra quién defiende Thalyx, y qué está en la TCB
- [[Camino-Confiable]] — quién le habla al humano cuando hay que autorizar
- [[Marcado-de-Origen]] — defensa estructural contra inyección de prompts

### 6. Decisiones y debates (el porqué de cada cosa)
- [[Debates-Overview]] — mapa de todos los debates resueltos

### 7. Pendientes (qué falta)
- [[Tareas-Pendientes]] — lista viva, revisar antes de retomar el proyecto

### 8. Adopción y fases (cuándo y cómo se construye/lanza)
- [[Fases-de-Implementacion]] — las 4 fases del roadmap
- [[Criterio-de-Salida-Fase-1]] — la definición de terminado de la Fase 1
- [[Condiciones-de-Adopcion]] — gates para abrir a usuarios (NO son de Fase 1)
- [[Por-Que-Elegirian-Este-SO]] — análisis honesto de propuesta de valor, con huecos reconocidos

### 9. Investigación
- [[Interpretabilidad-Mecanicista]]

### 10. Notas técnicas
- [[Estado-de-Implementacion]] — **qué está construido de lo que está decretado**
- [[Notas-Tecnicas-Implementacion]] — referencia rápida para escribir código
- [[Estrategia-de-Pruebas]] — tres niveles, con inyección de fallos como obligatorio
- [[Construccion-del-ISO]] — cómo se construye y se arranca la imagen

### 11. Contexto personal y de carrera
- [[Estrategia-Carrera]] · [[Riesgo-de-Ejecucion]]

## Estado global del proyecto (snapshot al 1 de agosto de 2026)

El proyecto salió de la fase de puro diseño: existe código en `crates/`, y la ruta de instalación de módulos funciona de punta a punta con la atomicidad respaldada por pruebas de inyección de fallos. Ver [[Estado-de-Implementacion]].

| Área | Estado |
|---|---|
| Nombre, nomenclatura y licencia | ✅ Decretado |
| Filosofía y arquitectura | ✅ Decretado |
| Primitivas base (4) | ✅ Decretadas |
| Flujo canónico (9 piezas) | ✅ Decretado |
| Build-then-commit y su mecanismo real | ✅ Decretado |
| Modelo de amenaza y TCB | ✅ Decretado |
| Camino confiable y marcado de origen | ✅ Decretado |
| Formato del manifiesto `.thmod` | ✅ Decretado |
| Verificación y distribución de módulos | ✅ Decretado |
| Resolución de versiones | ✅ Decretado |
| Sandboxing en detalle | ✅ Decretado |
| Coherencia con la doble ruta | ✅ Decretado |
| Concurrencia | ✅ Decretado |
| Alcance y criterio de salida de Fase 1 | ✅ Decretado |
| Estrategia de pruebas | ✅ Decretado |
| Diseño del ISO | ✅ Decretado |
| Casos trazados (feliz y de fallo) | ✅ Completos |
| Ruta de instalación de módulos | ✅ Implementada y probada |
| Atomicidad del commit | ✅ Demostrada con inyección de fallos |
| Registro de intención en el journal | ✅ Implementado |
| Índice en grafo y parser mecánico | ✅ Implementados |
| Contrato con marcado de origen | ✅ Implementado |
| Enforcement de permisos en el kernel | ✅ **Demostrado en hardware real** |
| `thalyx-permd` (política → mapa BPF) | ⚠️ Pendiente: el kernel y el núcleo no se hablan |
| Enforcement real de permisos | ⚠️ Se registran, todavía no se aplican |
| Modelo concreto del agente | ⚠️ Abierto, no bloqueante para Fase 1 |
| Métricas de benchmark de Fase 2 | ⚠️ Abierto |
| Interpretabilidad: técnicas concretas | ⚠️ Abierto |
| Dependencias entre módulos | ⚠️ Pospuesto deliberadamente |
| Reputación anti-Sybil | ⚠️ Pospuesto deliberadamente |
| **Validación con usuarios reales** | ❌ **No iniciada** |

Ver detalle completo en [[Tareas-Pendientes]].

## Cómo mantener esta bóveda

### Vocabulario de `estado` (cerrado)

| Valor | Significado |
|---|---|
| `decretado` | Decisión cerrada |
| `decretado-parcial` | Decidido en lo esencial, con puntos abiertos declarados en la nota |
| `pendiente` | Identificado, sin decidir |
| `pospuesto` | Decidido no resolverlo todavía, con la condición que lo reabriría |
| `reflexion-abierta` | Pensamiento en curso, no es una decisión |
| `activo` | Nota viva que se actualiza (índices, listas, referencias) |

No se usan otros valores. Si hace falta uno nuevo, se agrega aquí primero.

### Reglas

- Al cerrar un pendiente, actualiza esta tabla y el `estado` de la nota correspondiente.
- Al abrir un debate nuevo, crea la nota y enlázala desde [[Debates-Overview]].
- **Al revisar un decreto ya tomado, no borres lo anterior:** añade una sección `## Revisiones` al pie de la nota con qué decía antes, qué dice ahora y por qué cambió. El historial de por qué cambiaste de opinión vale tanto como la decisión.
- La bóveda se escribe en español neutro. Todo lo demás —código, schemas, commits, CLI— en inglés. Ver [[Nomenclatura-y-Convenciones]].
