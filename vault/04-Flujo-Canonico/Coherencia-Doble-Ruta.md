---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-01
tags: [flujo, coherencia, doble-ruta, grafo, journal]
---

# Coherencia de estado frente a la doble ruta

## La tensión entre dos decretos

El [[Principio-Doble-Ruta]] garantiza, como principio no negociable, que el humano puede hacer todo con herramientas POSIX estándar sin pasar por el agente.

Pero entonces, para todo lo que el humano haga por fuera:

- El [[FS-en-Grafo|índice semántico]] queda desactualizado.
- El [[Journal-y-Snapshots|Journal]] no registró nada.
- La [[Memoria-Persistente|memoria persistente]] afirma hechos que ya no son ciertos.
- Y una operación destructiva puede pisar trabajo humano del que el sistema nunca se enteró.

Dos principios decretados que se rozaban, sin ninguna nota que los reconciliara.

## Decreto 1: el filesystem es la única fuente de verdad

El índice en grafo es un **caché derivado**, nunca la verdad. Toda consulta al grafo devuelve, junto al resultado, si el índice está al día o si hay nodos pendientes de reprocesar.

## Decreto 2: el estado se mantiene por interceptación, no por confianza

`thalyx-lsm` intercepta las mutaciones del filesystem —`rename`, `unlink`, `create`, cierre tras escritura— y las **encola sin bloquear**. Un worker consume la cola y re-parsea únicamente los archivos afectados.

El hook no bloquea porque re-parsear dentro de él obligaría a cada escritura del sistema a esperar al parser: un `git checkout` o la descompresión de un paquete se arrastrarían, y un cuelgue del parser colgaría el filesystem entero.

**Lo que esto consigue:** el grafo pasa de poder estar horas desactualizado sin que nadie lo sepa, a estar desactualizado por milisegundos y saber con precisión cuáles nodos.

**Lo que esto no consigue:** que el grafo esté *siempre* exacto. Saber que un archivo cambió es instantáneo; saber que ahora importa otra cosa exige re-parsearlo, y eso no puede correr dentro del hook. Queda una ventana, y el sistema la declara en vez de ocultarla.

### Si la cola se desborda o el worker muere

Se **falla cerrado**: los nodos afectados se marcan como obsoletos. Nunca se descarta un evento en silencio dejando el índice afirmando que está al día.

## Decreto 3: el journal declara su alcance

El Journal registra únicamente operaciones ejecutadas por Thalyx. No es un registro completo del sistema, y lo dice de forma explícita. Ver [[Journal-y-Snapshots]].

## Decreto 4: ninguna operación destructiva confía en el registro

Antes de cualquier operación destructiva —incluida `thalyx restore`— el Core compara el estado real del disco contra el registrado. Si detecta cambios que no originó, **se detiene**, presenta por el [[Camino-Confiable|camino confiable]] el diff de lo que se perdería, y exige confirmación explícita.

Ninguna decisión destructiva depende de creer que Thalyx vio todo lo que pasó.

## Decreto 5: la memoria persistente fecha sus hechos

Cada hecho registrado guarda el estado del índice en el momento de registrarlo. Un hecho cuyo estado ya no se sostiene se marca como **no verificado**; no se borra. Ver [[Memoria-Persistente]].

## Caso extraordinario: cambios con Thalyx apagado

Arrancar otro sistema operativo, montar el disco en otra máquina o extraerlo son situaciones que **ninguna interceptación puede cubrir**: el LSM no está corriendo.

Se decreta:

- Al arrancar, Thalyx **reconcilia** el índice contra el disco mediante un barrido completo, y marca como obsoleto todo lo que no coincida.
- El sistema **documenta explícitamente** esta situación para quien la provoque: modificar el disco desde fuera es una operación legítima de usuario avanzado, y quien la haga debe saber que el índice y el journal no la habrán presenciado, y que el decreto 4 va a detener la siguiente operación destructiva pidiendo confirmación.

No es un flujo de usuario normal, y no se diseña alrededor de él. Se documenta para que no sorprenda.

## Relacionado
- [[Principio-Doble-Ruta]]
- [[FS-en-Grafo]]
- [[Parser-Mecanico]]
- [[Journal-y-Snapshots]]
- [[Rollback-vs-Restore]]
- [[Camino-Confiable]]
