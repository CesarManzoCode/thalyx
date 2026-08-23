---
tipo: notas-tecnicas
estado: activo
fecha-decreto: 2026-08-23
tags: [agente-ajeno, medicion, sandbox, fase-2]
---

# Qué necesita un agente ajeno para arrancar

> **Medido, no supuesto.** El pendiente decía exactamente eso —*«no por
> suposición: tomar Claude Code, mirar qué llama, y hacer la lista»*— y llevaba
> abierto desde el 2026-08-09 con la nota de que es barato y de que **sin él
> todo lo demás es adivinado**. Esto es la lista.
>
> Se reproduce con `dev/foreign-agent-needs.sh`, que es un script y no un
> párrafo por la regla de [[Estrategia-de-Pruebas]]: un procedimiento impreso
> para una persona es código que no corre.

## La vara

[[Filosofia-Fundacional]] pone como vara que **Claude Code y cualquier otro
agente ya escrito corran sobre Thalyx mejor que sobre Linux o macOS**.
[[Superficie-para-el-LLM]] anota debajo que hoy no arrancarían, y que por lo
tanto esto no es afinar sino construir.

Lo primero sigue en pie. **Lo segundo hay que corregirlo, y ésa es la noticia.**

## Lo medido, 2026-08-23

Claude Code 2.1.241 arrancando —`--version`, que carga el runtime y sale— bajo
`strace -f`, en el contenedor de desarrollo.

### Llamadas al sistema: 41 distintas, y 40 ya están permitidas

`module_standard` permite 159. De las 41 que el agente hace para arrancar,
**falta exactamente una**:

| Falta | Cuántas veces |
|---|---|
| `sched_setscheduler` | 8 |

Eso es todo. No es una lista de trabajo, es un renglón — y contradice de frente
la frase «no arrancarían» en la capa donde más caro parecía: el filtro de
seccomp de este proyecto ya cubre el 97.5% de lo que un agente ajeno pide para
existir.

**Lo que eso no dice**: que agregarlo sea correcto. `sched_setscheduler` cambia
la política de planificación de un hilo, y si un módulo confinado debe poder
hacerlo es una decisión de [[Sandbox-Ejecucion]] y de Cesar, no una consecuencia
de esta medición. Lo que la medición aporta es que **la pregunta es esa una, y
no cuarenta**.

### Rutas: 19 abiertas, 13 dentro de lo que un módulo ve

Lo que un módulo tiene es `SYSTEM_PATHS` —`/usr`, `/lib`, `/lib64`, `/bin`,
`/sbin`, `/etc`, todos de sólo lectura— más cinco nodos de dispositivo y un
`/proc` montado después del pivote.

Cubierto: el enlazador dinámico (`/etc/ld.so.cache` y cinco objetos
compartidos), `/proc/self/{maps,cgroup,statm}`, dos de `/proc/sys/vm/`,
`/etc/localtime` y `/dev/urandom`.

**No cubierto: seis rutas bajo `/sys`,** que un módulo no ve en absoluto.

| Ruta | Qué pasó aquí |
|---|---|
| `/sys/devices/system/cpu/online` | la tuvo |
| `/sys/kernel/mm/transparent_hugepage/enabled` | la tuvo |
| `/sys/fs/cgroup/cpu//cpu.cfs_quota_us` | la tuvo |
| `/sys/fs/cgroup/memory/…/memory.limit_in_bytes` | la tuvo |
| `/sys/fs/cgroup/memory/…/memory.soft_limit_in_bytes` | la tuvo |
| `/sys/kernel/debug/tracing/trace_marker` | `ENOENT`, **y arrancó igual** |

La última fila es la única de las seis sobre la que se puede afirmar algo:
**esa no hace falta**, porque no la tuvo y siguió. De las otras cinco lo único
cierto es que aquí no tuvo que arreglárselas sin ellas. Son todas de forma
—cuántas CPU hay, cuánta memoria le tocan, cómo son las páginas grandes— así
que la sospecha razonable es que un runtime moderno degrada a valores por
omisión; **una sospecha razonable no es una medición** y esta nota no la
apunta como si lo fuera. Se contesta quitándoselas y volviendo a correr.

### El libc, que es donde la frase «no arrancaría» sí es cierta

El agente abre `/etc/ld.so.cache` y cinco objetos compartidos. En una máquina
anfitriona eso está cubierto, porque `SYSTEM_PATHS` monta medio sistema
operativo de sólo lectura.

**En la imagen no.** `make -C image count` dice lo que lleva: `/init`, unos
directorios y `/dev/console`. No hay `/usr`, no hay `/lib`, no hay libc. Un
binario enlazado dinámicamente no arranca ahí, y el agente ajeno es uno.

Eso no es un defecto, es el decreto funcionando: [[Filosofia-Fundacional]] dice
que la imagen lleva el kernel y un programa. Lo que esta medición hace es
ponerle precio a la pregunta abierta de [[Tareas-Pendientes]] —*«decidir el ABI
de los módulos: nativo de Linux o independiente de POSIX»*— porque **es la misma
pregunta**, y ahora se sabe que el agente ajeno la hace con el enlazador antes
que con cualquier otra cosa.

## Qué queda, y ya con nombre

De los cuatro puntos de `G` en [[Superficie-para-el-LLM]], esta medición mueve
uno y no mueve tres:

- **G1, ejecutar un proceso arbitrario** — sigue entero. No es una llamada que
  falte ni una ruta: es que hoy `correr` sólo lanza módulos instalados y
  firmados, y un agente ajeno por definición no es ninguna de las dos cosas.
  **Éste es el que bloquea, y la medición lo confirma en vez de contradecirlo.**
- **G2, un runtime donde pueda correr** — es la pregunta del libc de arriba, y
  ahora tiene una forma concreta: o la imagen deja de llevar un solo programa,
  o los módulos se enlazan estáticamente, o el agente ajeno vive en otra parte.
- **G3, red** — no lo tocó esta medición: `--version` no sale a internet. Lo que
  hace falta para trabajar es otra corrida y otra lista.
- **G4, control de versiones** — igual: no aparece al arrancar.

Y hay un quinto que no está en `G` y sale de aquí: **`/home` está montado
`NOEXEC`**. Un agente ajeno que aterrice ahí no se puede ejecutar aunque todo lo
demás esté resuelto. Es la deuda de explicación que Cesar aplazó el 2026-08-09,
y ahora tiene un caso concreto detrás en vez de ser hipotética.

## Lo que esta medición no contesta

Escrito aquí y no al final por costumbre: es la mitad de la nota.

- **Arrancar no es trabajar.** No hay red, no hay terminal, no hay subprocesos y
  no hay una sola escritura. Todo eso son más llamadas y más rutas.
- **Lo que el agente invoca aparte no está contado**: una shell, `git`, una
  herramienta de búsqueda. Cada una es un binario más y un `execve` más, y `G1`
  otra vez.
- **Node es lo medido.** Un agente escrito en otra cosa da otra lista, y la
  parte que sobreviviría a ese cambio no está separada de la que no.

## Relacionado
- [[Superficie-para-el-LLM]]
- [[Sandbox-Ejecucion]]
- [[Filosofia-Fundacional]]
- [[Tareas-Pendientes]]
- [[Estrategia-de-Pruebas]]
