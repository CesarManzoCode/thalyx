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

> **El paso 1 ya tiene máquina detrás.** El 2026-08-03 la imagen arrancó en QEMU
> con un comando. Eso **no cierra el paso**: el decreto exige que lo haga una
> persona ajena, siguiendo solo el README y sin ayuda. Lo que cambió es que
> hasta ese día no existía nada que esa persona pudiera arrancar. Ver
> [[Primer-Arranque]].

## Por qué este criterio y no uno técnico

Un criterio por componentes ("todo implementado y probado") se puede cumplir íntegramente con un sistema que no le sirve a nadie: "implementado" no es "usable", y la diferencia entre ambos solo aparece cuando alguien que no construyó el sistema intenta usarlo.

Un criterio temporal ("12 meses") no es un criterio de calidad, es un plazo.

Este criterio, en cambio, es binario, es demostrable ante terceros, y cubre de un solo golpe las demostraciones de adopción 1 y 2 y el [[Caso-Instalar-Modulo|caso canónico]] completo.

## El efecto secundario buscado

Este criterio **fuerza el contacto externo que hoy no existe**.

[[Por-Que-Elegirian-Este-SO]] marca como la pregunta más importante sin responder si el problema que Thalyx resuelve es un dolor real de otras personas o solo del creador, y [[Riesgo-de-Ejecucion]] identifica que ese razonamiento sigue siendo enteramente a priori. Un criterio de salida que exige a una persona real usar el sistema ataca los dos riesgos a la vez, sin necesidad de un esfuerzo de validación separado.

## Nadie de fuera toca el sistema antes de esto

Decretado el 2026-08-03, después de que una sesión de trabajo derivara justo
hacia lo contrario y hubiera que frenarla.

**El contacto externo no se adelanta.** No hay versión reducida, ni prueba con
un conocido, ni "que alguien lo vea aunque sea por encima" antes de que exista
el ISO y la Fase 1 esté terminada. Los seis pasos de arriba son el momento en
que una persona ajena toca Thalyx **por primera vez**, y el primero de esos
pasos es arrancar la imagen.

### Por qué, y el motivo no es miedo

No es que preocupe lo que esa persona vaya a decir. **Este proyecto nunca
dependió de eso.** Su objetivo no es impresionar a nadie de fuera; es
convencer a Cesar, y eso ocurre —o no ocurre— con independencia de cualquier
opinión ajena.

Lo que la persona ajena determina es **la escala, no la validez**: si esto se
queda como un proyecto excepcional o se convierte en algo mucho más grande. Y
la fase en la que está el proyecto es incompatible con lo segundo. Enseñar
temprano no adelanta esa respuesta, la contamina: mide la reacción a un sistema
a medias y la confunde con la reacción al sistema.

### La deriva concreta que esto previene

La sesión del 2026-08-03 llegó a proponer preparar un README de veinte minutos
y buscar a alguien que supiera abrir una terminal, saltándose el ISO. El
razonamiento sonaba bien —"el contacto externo es el riesgo mayor, y llevamos
cuatro sesiones esquivándolo"— y era un reflejo importado de otro tipo de
proyecto: **validar pronto porque el mercado decide**. Aquí el mercado no
decide. Decide el soberano, y después el mercado dice qué tan lejos llega.

Quien lea [[Riesgo-de-Ejecucion]] o la sección de abajo va a sentir el mismo
tirón. La respuesta ya está dada: **sí, el riesgo es real; se carga a
propósito hasta que exista el ISO.** Cargar un riesgo con los ojos abiertos no
es lo mismo que ignorarlo, y esa distinción es la que evita que esta nota se
convierta en una excusa.

## Relacionado
- [[Fases-de-Implementacion]]
- [[Condiciones-de-Adopcion]]
- [[Construccion-del-ISO]]
- [[Por-Que-Elegirian-Este-SO]]
- [[Riesgo-de-Ejecucion]]
