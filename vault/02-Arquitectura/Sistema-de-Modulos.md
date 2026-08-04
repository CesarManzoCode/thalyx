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
- **Rollback:** el sistema incluye snapshots de Btrfs que permiten revertir cualquier cambio realizado por un módulo o por el agente. Ver [[Journal-y-Snapshots]] y [[Rollback-vs-Restore]].

## Especificaciones relacionadas
- [[Formato-Manifiesto-Thmod]] — formato exacto del manifiesto `.thmod`, decretado el 2026-08-01
- [[Verificacion-y-Distribucion]] — en Fase 1 los módulos se distribuyen prebuildeados y firmados
- [[Sistema-Reputacion-Sybil]] — problema de reputación anti-Sybil (pospuesto deliberadamente)

## Qué significa "exclusivamente por la API de Thalyx"

[[Filosofia-Fundacional]] dice que los módulos se comunican exclusivamente por la
API de Thalyx, no por POSIX y no por libc. Lo que está construido no es eso, y la
diferencia hay que decirla en vez de administrarla.

**Un módulo es hoy un binario nativo de Linux.** El rootfs del sandbox monta
`/usr`, `/lib`, `/lib64`, `/bin`, `/sbin` y `/etc` de sólo lectura —para que un
programa enlazado dinámicamente pueda siquiera arrancar— y el filtro seccomp
permite alrededor de ciento veinte llamadas al sistema, incluidas `openat`,
`read` y `write`. Un módulo puede usarlas sobre lo que el sandbox le deje ver.

La distinción correcta, y la que este decreto adopta:

> **La API de Thalyx es la única superficie mediada.** No es la única superficie
> alcanzable.

Es una frase más chica que la del decreto fundacional y es la que se puede
defender. Lo que sólo pasa por la API —y no existe por ningún otro camino— es
todo lo que hace que un módulo sea un módulo:

- **Su identidad.** Un módulo no sabe cómo se llama. Lo pregunta.
- **Sus permisos.** No los declara ni los descubre; se los dicen.
- **Los archivos concedidos.** Están detrás de una comprobación que el kernel
  hace durante la resolución, no detrás de una convención.
- **Hablarle al humano.** Un módulo no tiene terminal. Ver [[Camino-Confiable]].

Y lo que queda alcanzable por POSIX está acotado por tres capas que sí existen:
el rootfs no contiene nada que no se haya montado, el filtro mata lo que no está
en la lista, y el LSM deniega en el kernel. Ninguna de las tres convierte al
módulo en un programa que no habla POSIX. Lo que hacen es que hablar POSIX no
lo lleve a ninguna parte que el humano no haya autorizado.

### Lo que haría verdadero el decreto entero

Módulos estáticos sin libc, un rootfs sin `/usr` ni `/lib`, y un filtro mucho
más chico — o un objetivo distinto del binario nativo, como WASM. Es una
decisión sobre **cómo se construyen los módulos**, no sobre cómo se los aísla,
y por eso es de Fase 2: hoy hay un módulo y cambiar la forma de construirlo es
barato; con un ecosistema encima deja de serlo. Está en [[Tareas-Pendientes]].

Mientras tanto no se expande el ecosistema de módulos. Escribir más módulos
contra la forma que se quiere abandonar es exactamente el costo que esta nota
existe para evitar.

## Revisiones

### 2026-08-04 — Se dice qué es un módulo hoy, en vez de citar lo que será
**Antes:** el repositorio afirmaba que los módulos no hablan POSIX ni libc, y
construía binarios de Linux enlazados dinámicamente contra un rootfs con casi
una distribución adentro. El código lo admitía en un comentario; el README no.
**Ahora:** la API es la única superficie **mediada**, y lo que haría verdadera la
frase original es una tarea de Fase 2 con nombre.
**Motivo:** una auditoría externa señaló la contradicción. Es la clase de cosa
que [[Filosofia-Fundacional]] manda resolver a favor del decreto — salvo que
aquí el decreto describe un destino y el código describe un presente, y la
respuesta honesta era escribir la distinción, no fingir que no existe ni
declarar equivocado a un decreto que sigue siendo la dirección.
**Lo que no cambia:** el kernel de Linux sigue siendo un motor que Thalyx
gestiona, no un anfitrión. La imagen sigue llevando el kernel y un programa, y
sigue siendo contable con `make -C image count`.

### 2026-08-01 — Se cierra el pendiente del manifiesto
**Antes:** el formato del manifiesto estaba pendiente de decreto y la nota apuntaba a una nota inexistente — el único enlace roto de la bóveda.
**Ahora:** decretado en [[Formato-Manifiesto-Thmod]].

## Relacionado
- [[Core-Nucleo]]
- [[Core]]
- [[Sandbox-Ejecucion]]
- [[Debate-Core-Modules]]
- [[Filosofia-Fundacional]]
- [[Camino-Confiable]]
