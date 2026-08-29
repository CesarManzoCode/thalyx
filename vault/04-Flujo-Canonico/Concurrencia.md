---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-01
tags: [flujo, concurrencia, core, fase-1]
---

# Concurrencia y supuesto de usuario

## El hueco que resuelve

[[Debate-Conflicto-Recursos]] cubría el caso de dos módulos pidiendo el mismo recurso, pero nada en la bóveda decía qué pasa cuando **dos contratos se ejecutan a la vez**, ni cómo interactúa el bloqueo de subgrafo de `REFACTOR_SUBGRAPH` con un commit del Core. Tampoco estaba escrito el supuesto —evidente pero no decretado— de que hay un solo usuario y un solo agente.

## Decreto

**Fase 1 asume un usuario y una instancia de agente.**

`thalyx-core` es el **único escritor** y serializa la ejecución de contratos con un lock global: un contrato en ejecución a la vez. Los contratos que llegan durante una ejecución se encolan por orden de llegada.

Esto incluye a `REFACTOR_SUBGRAPH`: se ejecuta bajo el mismo lock, así que no puede solaparse con un commit.

## Por qué un lock global y no locks por recurso

Un lock global elimina de un golpe toda una clase de defectos —contratos que se pisan, refactorizaciones compitiendo con commits, permisos entrelazados entre operaciones— y no cuesta nada en Fase 1, donde no existe carga que serializar ni usuarios que noten la espera.

El paralelismo por recurso, con su detección de deadlocks y su ordenamiento de locks, es la optimización que corresponde **cuando exista contención medida**. Es la aplicación directa del [[Criterio-de-Inclusion-de-Primitivas]]: no se resuelve antes de tiempo un problema que aparece con la escala, y la frontera del lock es fácil de estrechar después, no de introducir después.

## Estado: implementado

Desde el 4 de agosto de 2026 el lock existe: `Store::lock()` toma un `flock(2)`
exclusivo sobre `state/lock`, y lo toman `install`, `remove`, `rollback`,
`restore`, la asignación de uid dentro de `run` y —desde el 28 de agosto de
2026— `intento` en sus tres transiciones.

`flock` se eligió sobre `fcntl` por una razón concreta: un lock de `fcntl` se
suelta cuando **cualquier** descriptor del archivo se cierra en el proceso, lo
que lo vuelve frágil en un programa que abre el almacén desde varios lugares. El
de `flock` pertenece a la descripción de archivo abierta y se libera cuando esa
descripción desaparece — es decir, cuando el proceso termina, incluido cuando lo
matan. Un corte a mitad de una instalación no puede dejar la máquina incapaz de
volver a instalar.

Lo prueban dos tests, y el que importa lanza un **proceso** hijo, no un hilo: un
hilo comparte la descripción de archivo abierta y `flock` lo dejaría pasar
directo, así que la prueba habría pasado sin que la propiedad existiera.

### Lo que el lock no promete

**El orden de llegada.** `flock` despierta a *un* esperante, no al que lleva más
tiempo esperando. Lo que este decreto dice sobre encolar "por orden de llegada"
no se cumple: se cumple la serialización, no el orden.

Se deja así deliberadamente. En Fase 1 hay un usuario y un agente, de modo que
no existen dos contratos en contención cuyo orden alguien pueda observar, y una
cola justa necesitaría un proceso intermediario — una cosa más grande que el
problema. Está escrito aquí en vez de quedar como una diferencia silenciosa
entre lo que el decreto dice y lo que la máquina hace.

## Revisiones

### 2026-08-28 — `intento` no lo tomaba, y comprobar no es excluir

`intento empezar` decía la regla correcta —uno abierto a la vez— y la
**comprobaba** en lugar de **imponerla**. La secuencia era:

```
leer el registro  →  ¿hay alguno abierto?  →  tomar el snapshot  →  escribir el registro
```

sin nada entre la lectura y la escritura. Dos clientes que llegan juntos ven los
dos «no hay ninguno», los dos toman un snapshot, y el segundo pisa el registro
del primero. Lo que queda no falla ruidosamente: queda **un snapshot que nada
nombra** —imposible de abandonar, invisible salvo como disco que no vuelve— y un
`intento abandonar` que devuelve el árbol a un punto que no es el que el primer
cliente creía.

`confirmar` y `abandonar` tenían el espejo: dos clientes que planearon contra el
mismo intento lo llevaban a cabo los dos, y el segundo `restore` caía sobre un
árbol que el primero ya había devuelto —borrando en silencio lo que se hubiera
escrito ahí desde entonces.

**Lo que cambia:** las tres transiciones toman `Store::lock()`, el mismo de
siempre y no un esquema nuevo. Como `flock` se ata a la descripción de archivo
abierta y por lo tanto **no es reentrante**, un `abandonar` que tomara el lock y
llamara a `restore::apply` —que también lo toma— se esperaría a sí mismo para
siempre; por eso existe `restore::apply_holding_the_lock`, cuyo parámetro
`&ContractLock` es la prueba de que quien llama ya lo tiene.

La ventana que el lock **no** puede cerrar es la del plan y la confirmación: la
confirmación es una persona leyendo una pregunta, y sostener el lock global
mientras alguien lee es como una máquina deja de responder. Así que `abandonar`
relee el registro adentro del lock y compara: si el intento en registro ya no es
contra el que se planeó, contesta `superseded` en vez de llevarlo a cabo.

**Y la durabilidad.** `attempt.json` se escribía con `std::fs::write`, que trunca
antes de escribir: un corte a la mitad deja un archivo que es mitad de un estado
y mitad de otro. La regla 2 de `attempt.rs` lo convierte —correctamente— en «hay
un intento y no se puede leer», pero eso es permanente: la máquina ya no puede
empezar un intento ni abandonar el que cree tener. Ahora usa
`keystore::write_durably`, la misma publicación que el resto del estado —
temporal único en el mismo directorio, `fsync`, `rename`, `fsync` del
directorio — y el borrado del registro hace `fsync` del directorio por la misma
razón.

Lo prueban cinco tests en `attempt.rs`. El de la carrera usa hilos, lo que este
decreto advierte que no sirve — y la advertencia es sobre otro caso: un hilo que
*comparte* la descripción de archivo abierta pasa directo, y aquí cada hilo abre
la suya, igual que dos procesos. Se comprobó quitando el lock de `begin`: el
test falla en todas las corridas.

### 2026-08-04 — El lock decretado se implementa, y se corrige lo que el decreto prometía de más
**Antes:** el decreto describía un lock global y ningún código lo tomaba. Cada
escritura individual era atómica —un `rename`— y de ahí se había concluido, sin
que nadie lo escribiera, que el conjunto también lo era.
**Ahora:** existe el lock, y este decreto dice qué garantiza y qué no.
**Motivo:** una instalación escribe cuatro archivos separados —el registro de
permisos, el keystore, el registro de uids y el enlace `current`—. Un `rename`
es atómico; una transacción sobre cuatro archivos no lo es, y ninguna
disposición de renames la vuelve atómica. Dos instalaciones simultáneas podían
entregar el mismo uid a dos módulos distintos, o dejar las concesiones de una
bajo el commit de la otra.
**Cómo se encontró:** una auditoría externa preguntó dónde estaba el lock. No
estaba en ninguna parte, y llevaba tres días decretado.

## Relacionado
- [[Debate-Conflicto-Recursos]]
- [[Core]]
- [[FS-en-Grafo]]
- [[Criterio-de-Inclusion-de-Primitivas]]
