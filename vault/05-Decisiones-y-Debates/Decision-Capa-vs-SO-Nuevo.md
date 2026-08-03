---
tipo: decision
estado: decretado
fecha-decreto: 2026-07-31
tags: [debate, arquitectura, no-negociable]
---

# ¿Es una capa sobre Linux o un SO nuevo?

## Debate

Un crítico argumentó que casi todo el valor del sistema (agente, FS semántico, permisos JIT, scheduler predictivo) se podía construir como una capa sobre una distribución de Linux existente, evitando el costo de reinventar el kernel.

> Desde el 2026-08-03 esta nota **no es la más profunda sobre el tema**. El
> decreto fundacional está en [[Filosofia-Fundacional]], en palabras de Cesar, y
> esta nota se lee a la luz de aquél. Lo que sigue aquí es el debate del que
> salió y el criterio que lo hace comprobable.

## Resolución (decretada, cerrada, no se reabre)

**Thalyx es un sistema operativo nuevo. No es una capa y no es una distribución de Linux.**

Una capa hereda las limitaciones de diseño del sistema que parchea: permisos pensados para procesos humanos, scheduler pensado para cargas humanas, FS pensado para jerarquía humana. Por más daemons que se añadan encima, el techo de lo que la IA puede hacer con soltura sigue siendo el mismo.

## Qué define la diferencia: la autoridad de diseño

Lo que hace que Thalyx no sea una capa **no es dónde corre el código, sino quién tiene autoridad sobre el diseño**.

Una capa no puede cambiar las reglas del sistema que la aloja: las acata. Thalyx es dueño del arranque, del sistema de módulos, de la política de permisos y de los requisitos de filesystem, y modifica el kernel cuando su diseño lo exige — de hecho ya lo hace en la Fase 1, con `thalyx-lsm`.

Que la mayor parte del código de la Fase 1 viva en userspace es una decisión de **orden de construcción y gestión de riesgo**, no una renuncia arquitectónica. Ver [[Decision-Kernel-vs-Userspace]].

## Correr sobre otro Linux es andamio, no destino

Añadido el 2026-08-03, porque faltaba escrito y su ausencia costó un día de
razonamiento torcido.

Hoy Thalyx se ejecuta sobre la Fedora de Cesar: `THALYX_ROOT` apunta a un
directorio, `verify.sh` monta cgroups prestados, el LSM se carga y se desengancha
a mano. **Eso es un banco de pruebas, no una forma de usar Thalyx.** Es
simplemente lo más rápido de validar en una máquina que ya existe.

Lo que el sistema es, lo dice [[Construccion-del-ISO]]: el kernel de Linux y
`thalyx`, sobre Btrfs con subvolúmenes separados para sistema, módulos y datos
de usuario. Una imagen que arranca, sin distribución debajo. El primer usuario
no instalará un paquete encima de su distro — vivirá la experiencia completa de
instalar un sistema operativo.

### Qué se lee mal cuando esto no está escrito

Dos cosas, ambas ocurridas el mismo día:

1. **Se registró como defecto que un módulo sin permisos no corra confinado
   cuando falta el mapa de política**, con el argumento de que "casi ninguna
   máquina tiene `bpf` en el orden de LSM". Cierto para las máquinas de otras
   personas, irrelevante para Thalyx: aquí el LSM se carga en el arranque. Que
   falte es una avería, y negarse es la respuesta correcta. Ver la advertencia 0
   de [[Estado-de-Implementacion]].

2. **Se resumió este decreto como su contrario** — "el proyecto decidió empezar
   como capa sobre Linux" — al descartar una crítica externa que decía que
   Thalyx todavía no es un sistema operativo. El decreto dice literalmente que
   no es una capa, y la revisión de abajo eliminó esa palabra del vocabulario
   precisamente para que nadie volviera a leerlo así.

Sobre esa crítica, dicho bien: **como artefacto de hoy tiene razón** — no hay
imagen que arranque todavía. Lo que confundía era el tipo de afirmación: no es
una posición de diseño pendiente de decidir, es un hecho de **orden de
construcción**. La autoridad de diseño ya está ejercida, y `thalyx-lsm` modifica
el kernel desde la Fase 1.

## Revisión del 2026-08-03 — la bóveda decretaba lo contrario en otras tres notas

Este decreto dice "no es una distribución de Linux" y está marcado no
negociable. Al mismo tiempo, [[Construccion-del-ISO]] decretaba construir con
`mkimage.sh` —la herramienta con la que se hacen distros de Alpine—,
[[Fases-de-Implementacion]] titulaba la Fase 1 *"sobre base Alpine"*, y
[[Core-Nucleo]] decía *"kernel Linux minimalista (tipo Alpine)"*, que además es
un error de categoría porque Alpine no tiene kernel propio.

Tres notas decretaban una distro y esta decretaba que no lo era. **Tres días
conviviendo**, y las cuatro leídas el mismo día sin que saltara.

**Resuelto:** la imagen es el kernel de Linux y `thalyx`, y nada más. Ninguna
distribución, nunca. Ver [[Construccion-del-ISO]].

### Lo que esto le hace al criterio de este decreto

Este decreto dice que lo que distingue un sistema de una capa es **la autoridad
de diseño**, y ese criterio es correcto: Android es Linux y nadie lo llama capa,
porque los programas se escriben contra el contrato de Android.

Pero Thalyx **no lo cumplía**. Un módulo era un script de shell que corre en
cualquier Linux; la API interna que [[Core-Nucleo]] nombra no existía; y todo el
sistema se podía instalar sobre una Fedora — de hecho ahí se desarrolló entero.
Un sistema que se instala con `cargo` sobre la distro de alguien más es una capa,
por más autoridad de diseño que declare tener.

Lo que cambia al quitar la distro es que el criterio **se cumple por
construcción**: sin shell y sin utilidades, no queda otro contrato contra el que
escribir. Quitar la distro y cumplir este decreto son el mismo acto, y esa es la
razón por la que la corrección no es cosmética.

## Revisiones

### 2026-08-01 — Se añade la formulación de "autoridad de diseño" y se elimina "capa" del vocabulario
**Antes:** este decreto convivía con una Fase 1 titulada "Capa sobre Linux (userspace)", lo que hacía que la bóveda se contradijera a sí misma a la vista de cualquier lector.
**Ahora:** se formula el criterio que distingue capa de sistema propio, y la palabra "capa" desaparece de [[Fases-de-Implementacion]].
**Motivo:** el decreto original era correcto pero incompleto: decía qué no era Thalyx sin decir qué propiedad lo determina. Sin esa propiedad, cualquiera podía leer la Fase 1 y concluir, con razón, que el decreto no se estaba cumpliendo.

## Relacionado
- [[Filosofia-Fundacional]]
- [[Arquitectura-Asimetrica]]
- [[Decision-Kernel-vs-Userspace]]
- [[Fases-de-Implementacion]]
