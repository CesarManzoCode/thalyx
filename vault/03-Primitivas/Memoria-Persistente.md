---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitiva, memoria, agente, fase-1]
---

# Memoria persistente de trabajo

## Función

La IA guarda estado de tareas y lo recupera entre sesiones.

## Implementación

- Base de datos vectorial local (ej. sqlite-vss, LanceDB).
- Flujo:
  1. El agente guarda estado: `SAVE_STATE(task: "refactor-auth", state: {"files_moved": [...], "dependencies_updated": [...]})`.
  2. El estado se serializa en la base de datos, indexado por embeddings para búsqueda semántica.
  3. Cuando el usuario vuelve, el agente recupera el estado: `RESTORE_STATE(task: "refactor-auth")` y continúa.

## Distinción entre hechos e inferencias (decretado)

La memoria persistente guarda **dos capas separadas**, etiquetadas de forma distinta:

- **Hechos** (inmutables, verificables): qué se instaló, cuándo, qué versión, qué permisos se confirmaron.
  > Ejemplo: "Usuario instaló pyassist-core v2.3.1 el 31 de julio de 2026, confirmó permisos persistentes de red y lectura de /home/user/proyectos."
- **Notas de continuidad / inferencias** (mutables, descartables, propuestas por el agente): posibles siguientes pasos o contexto interpretativo que el agente había considerado.
  > Ejemplo: "Posible siguiente paso: preguntar si el usuario quiere configurar el módulo."

### Razón de esta distinción
Si la memoria solo guardara hechos crudos, el agente tendría que re-inferir desde cero el contexto de continuidad cada vez que retoma una tarea, gastando cómputo innecesariamente en reconstruir algo que ya había razonado antes.

Esta distinción matiza una sugerencia externa que proponía "guardar solo hechos" — se decidió que ambas capas son valiosas, pero deben quedar etiquetadas de forma distinta para que quede claro qué es verificable y qué es interpretación descartable.

## Fechado de los hechos

Cada hecho guarda el estado del índice en el momento de registrarlo. Si ese estado ya no se sostiene —porque el usuario cambió las cosas por fuera del agente— el hecho se marca como **no verificado**. No se borra: dejar de ser comprobable no es lo mismo que ser falso. Ver [[Coherencia-Doble-Ruta]].

## Distinta de la memoria de conversación

La memoria de conversación del [[Agente-Conversacional]] (historial de interacciones) es un uso relacionado pero no idéntico — permite retomar conversaciones días después.

## Qué NO es esta primitiva

Distinta del [[Journal-y-Snapshots|Journal/sistema de snapshots]]: el Journal existe para *deshacer* operaciones (rollback físico); la Memoria persistente existe para que el agente *recuerde contexto y progreso* (continuidad, no reversión).

## Revisiones

### 2026-08-01 — Se mantiene la base vectorial y se añade el fechado de hechos
**Antes:** la implementación era una base vectorial sin más; los hechos no llevaban ninguna marca de vigencia.
**Ahora:** se conserva la base vectorial, y cada hecho guarda contra qué estado del índice se registró.
**Motivo del fechado:** con el [[Principio-Doble-Ruta]] garantizando que el usuario puede cambiar las cosas sin pasar por el agente, un hecho registrado puede dejar de ser cierto sin que nadie lo note. Un agente que afirma con seguridad algo que ya no se sostiene es peor que uno que dice "esto lo registré, pero ya no puedo verificarlo".
**Nota sobre la base vectorial:** se evaluó reemplazarla en Fase 1 por dos tablas SQLite con acceso por `task_id`, dado que `RESTORE_STATE` es una búsqueda por clave y no semántica. Se decretó conservar la base vectorial.

## Estado: construida

`crates/thalyx-memory`, y `thalyx memory` en la CLI. Las dos separaciones que el decreto pide están **en los tipos**, no en la disciplina de quien las use.

### Las dos capas no se pueden confundir

`Recollection` devuelve hechos y notas en **dos campos distintos**. Escribir una frase que los mezcle exige un acto deliberado, no un descuido. Y las notas se pueden tirar (`forget-notes`); **no existe forma de borrar un hecho**: la inferencia es del agente, el registro de lo que pasó es tuyo.

### El fechado: contra las rutas, no contra el índice entero

La lectura obvia del decreto —tomar la huella del índice y compararla después— deja **todo hecho no verificado en cuanto cambia cualquier cosa en cualquier lado**, o sea a los pocos segundos en una máquina que alguien esté usando. Una memoria donde todo es dudoso es lo mismo que no tener memoria.

Entonces un hecho se atestigua contra **las rutas de las que habla**. Editar un archivo ajeno lo deja en pie; editar el que describe, no — y el reporte **nombra qué ruta se movió**.

Hay un tercer estado además de verificado y no verificable: **`Unwitnessed`**, un hecho registrado sin nada contra qué comprobarlo. Es distinto de "verificado" a propósito: un hecho que nadie puede contradecir no es un hecho que alguien haya confirmado, y juntar los dos es exactamente cómo un agente termina sonando seguro de algo que nunca comprobó.

### La base vectorial, y qué es honestamente

Implementación propia, como se decretó.

Una base vectorial solo es **semántica** si los vectores vienen de un modelo que entiende el texto, y Thalyx todavía no tiene modelo local — cuál correr es un decreto abierto. Así que el embebedor que se entrega hoy es **léxico**: una bolsa de palabras hasheada. Encuentra vocabulario compartido, no significado compartido.

Hay un test que afirma que dos formas de decir lo mismo **no** se encuentran, porque eso es la limitación y no un defecto. Y **cada resultado carga si el emparejamiento fue semántico o no**, con la misma forma que `Answer<T>` en el grafo: no se pueden obtener las filas sin la advertencia. Cambiar a un modelo real toca una implementación de trait y nada más.

La búsqueda es **exacta** sobre todos los vectores. Un índice aproximado cambia recall por velocidad, y un recall que no se ha medido es una memoria que olvida en silencio.

Dos cosas quedan clavadas por tests porque equivocarlas **no fallaría**: la disposición de bytes de un vector guardado, y el bucket en el que cae cada palabra. Las dos harían que todo vector almacenado significara otra cosa. El hash es FNV-1a escrito a mano y no el de la biblioteca estándar, que no promete estabilidad entre versiones.

Una memoria escrita por un embebedor **se niega a ser leída por otro**, en vez de devolver disparates con confianza.

## Relacionado
- [[Coherencia-Doble-Ruta]]
- [[Journal-y-Snapshots]]
- [[Agente-Conversacional]]
- [[Que-es-una-Tarea]]
