---
tipo: estrategia
estado: decretado
fecha-decreto: 2026-07-31
tags: [adopcion, gtm, fase-apertura]
---

# Condiciones de adopción (gates de apertura a usuarios)

## Encuadre temporal importante (corrección de secuencia)

**Estas condiciones son para la fase de apertura a usuarios externos, NO para la Fase 1 de construcción.** La Fase 1 construye el Core sin audiencia todavía.

Esto corrige un error de secuencia detectado durante el diseño: inicialmente se plantearon como si fueran requisitos de la Fase 1, cuando en realidad la Fase 1 no tiene audiencia — es la fase de construir primitivas, no de venderlas. Confundir "fase de construcción" con "fase de lanzamiento" fue identificado explícitamente como un error a no repetir.

## 1. Prueba sin fricción

**Requisito:** ISO booteable ejecutable en:
- **Máquina virtual (VM):** con un script de un solo clic que levante el SO en QEMU o VirtualBox, sin tocar el sistema anfitrión.
- **Live USB:** que puedas arrancar desde una USB sin instalar nada, y probar el flujo completo (agente → contrato → ejecución → rollback) en una sesión temporal.
- **Dual-boot:** que pueda instalarse junto a Windows/Linux sin borrar el sistema actual, con un gestor de arranque que permita elegir al inicio.

**Principio:** el costo de *probar* debe ser cero, aunque el costo de *migrar completamente* siga siendo alto.

**Nota sobre hardware real:** no hace falta resolver soporte de hardware real desde el día uno. Se empieza en VM/QEMU porque permite iterar más rápido. El soporte de hardware real llega cuando el sistema ya justifique correr fuera de una VM.

## 2. Demostraciones dramáticas (no incrementales)

**Requisito:** al menos 2-3 demostraciones concretas que cualquier usuario pueda ejecutar en el live USB y que muestren la ventaja de forma innegable — no "10% más rápido", sino "esto era imposible/muy tedioso antes y ahora es trivial".

### Demostraciones decretadas

1. **Rollback instantáneo de una refactorización compleja:** el usuario mueve 10 archivos, actualiza dependencias, y con un solo comando (`rollback last`) el sistema revierte TODO exactamente al estado anterior, en segundos. Esto no existe en ningún OS actual.

2. **Memoria persistente entre sesiones:** el usuario le pide al agente "organiza mi carpeta de descargas", apaga la máquina, la enciende al día siguiente, y el agente recuerda exactamente lo que hizo y puede continuar sin perder contexto.

3. **Orquestación entre módulos:** dos módulos de terceros (ej. "organizador de fotos" y "editor de markdown") coordinados automáticamente por el agente. **Marcada como la más riesgosa de implementar temprano** porque depende de que ya exista un ecosistema mínimo de módulos comunicándose entre sí — cosa que no existe en fases tempranas. Se pospone naturalmente sin urgencia (consecuencia directa de que en Fase 1 no hay audiencia que la necesite ver).

## 3. Confianza ganada en público

**Requisitos:**
- Código abierto desde el día uno (ya decretado en la [[Filosofia-Fundacional]]).
- **Logs de auditoría revisables localmente por el propio usuario.** Se descartó la idea inicial de subirlos automáticamente a un repositorio público anonimizado, por el riesgo no trivial de que anonimizar rutas de archivos y metadatos filtre información sensible sin que sea obvio. En vez de eso, el usuario decide manualmente si comparte un log específico.
- Contratos legibles por humanos con confirmación explícita antes de ejecutar.
- Bug bounty (aunque sea pequeño: reconocimiento público, no dinero al principio) para quien encuentre una vulnerabilidad en el sandbox o en los permisos.

## Relacionado
- [[Fases-de-Implementacion]]
- [[Por-Que-Elegirian-Este-SO]]
- [[Fase-Commit-Atomico]] — refuerza la demo #1
