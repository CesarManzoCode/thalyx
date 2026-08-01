---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-07-31
tags: [arquitectura, modulos, ecosistema]
---

# El sistema de módulos (el ecosistema)

- Todas las funcionalidades IA son módulos descargables (archivos `.thmod`). El usuario elige qué activar.
- **Distribución descentralizada:** no hay gatekeeper central. Cualquier persona puede crear un módulo y distribuirlo por sus propios medios (web, repositorios personales, foros).
- **Repositorio comunitario** (estilo AUR/Flathub): existe un repositorio opcional donde los usuarios suben módulos, votan, dejan reseñas y reportan problemas. Los más populares destacan por mérito propio.

## Sistema de confianza híbrido

- **Core Modules:** un conjunto pequeño (<20) de módulos esenciales (agente base, gestor de archivos, terminal, etc.) mantenidos por el equipo central, firmados y verificados. Vienen preinstalados o se ofrecen como "recomendados seguros". Ver [[Debate-Core-Modules]].
- **Community Repo:** cualquiera sube módulos. El agente analiza el manifiesto (permisos, código fuente si está disponible), genera un resumen para el usuario ("Este módulo pide acceso a tu carpeta de documentos y a internet. ¿Quieres instalarlo?"), y muestra métricas de la comunidad (descargas, tiempo en el repo, issues abiertos).

## Aislamiento y rollback

- **Aislamiento:** cada módulo se ejecuta en un sandbox con permisos estrictos, usando namespaces de Linux, cgroups y seccomp. Ver [[Sandbox-Ejecucion]].
- **Rollback:** el sistema incluye snapshots (Btrfs/ZFS) que permiten revertir cualquier cambio realizado por un módulo o por el agente. Ver [[Journal-y-Snapshots]].

## Pendiente
- [[Formato-Manifiesto-Osmod]] — formato exacto del manifiesto `.thmod` (pendiente de decreto)
- [[Sistema-Reputacion-Sybil]] — problema de reputación anti-Sybil (pospuesto deliberadamente)

## Relacionado
- [[Core-Nucleo]]
- [[Sandbox-Ejecucion]]
- [[Debate-Core-Modules]]
