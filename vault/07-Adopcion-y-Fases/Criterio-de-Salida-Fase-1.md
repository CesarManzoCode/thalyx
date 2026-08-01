---
tipo: estrategia
estado: decretado
fecha-decreto: 2026-08-01
tags: [fases, criterio, validacion, definicion-de-terminado]
---

# Criterio de salida de la Fase 1

## El hueco que resuelve

La Fase 2 tenía un criterio numérico y verificable (overhead <5% / >15%, con p99 en la zona gris). La Fase 1 solo decía "tener un prototipo funcional que pueda demostrar el flujo completo", que no es una definición de terminado: se puede cumplir siempre y nunca.

## Decreto

La Fase 1 se considera terminada cuando **una persona ajena al proyecto**, siguiendo únicamente el README y **sin intervención tuya**, logre:

1. Arrancar la imagen del sistema en QEMU con un solo comando.
2. Instalar un módulo firmado desde un repositorio local.
3. Revisar y confirmar sus permisos por el [[Camino-Confiable|camino confiable]].
4. Revertir la instalación.
5. Apagar la máquina.
6. Reiniciarla y comprobar que el agente conserva el contexto de la tarea.

**Ningún otro criterio lo sustituye.** Que todos los componentes estén implementados y con tests en verde no cierra la Fase 1.

## Por qué este criterio y no uno técnico

Un criterio por componentes ("todo implementado y probado") se puede cumplir íntegramente con un sistema que no le sirve a nadie: "implementado" no es "usable", y la diferencia entre ambos solo aparece cuando alguien que no construyó el sistema intenta usarlo.

Un criterio temporal ("12 meses") no es un criterio de calidad, es un plazo.

Este criterio, en cambio, es binario, es demostrable ante terceros, y cubre de un solo golpe las demostraciones de adopción 1 y 2 y el [[Caso-Instalar-Modulo|caso canónico]] completo.

## El efecto secundario buscado

Este criterio **fuerza el contacto externo que hoy no existe**.

[[Por-Que-Elegirian-Este-SO]] marca como la pregunta más importante sin responder si el problema que Thalyx resuelve es un dolor real de otras personas o solo del creador, y [[Riesgo-de-Ejecucion]] identifica que ese razonamiento sigue siendo enteramente a priori. Un criterio de salida que exige a una persona real usar el sistema ataca los dos riesgos a la vez, sin necesidad de un esfuerzo de validación separado.

## Relacionado
- [[Fases-de-Implementacion]]
- [[Condiciones-de-Adopcion]]
- [[Construccion-del-ISO]]
- [[Por-Que-Elegirian-Este-SO]]
- [[Riesgo-de-Ejecucion]]
