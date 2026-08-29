---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-29
fecha-revision: 2026-08-29
tags: [superficie, contexto, agentes, costos]
---

# Contexto progresivo: una descripción en lugar del archivo

## Función

Bajar el **segundo costo** de [[Superficie-para-el-LLM]] — el contexto — en la
única operación que un agente de programación hace todo el tiempo: enterarse de
qué es algo.

Un agente que quiere saber qué es `Store::lock` abre el archivo donde está y paga
las novecientas líneas, de las cuales cuatro eran la respuesta. Paga en tokens,
en atención, y en el riesgo de que algo en las otras ochocientas noventa y seis
le cambie lo que iba a hacer.

## Qué contesta `contexto`

No el archivo. Esto:

```
Keystore
kind: struct
crate: verify-context
file: src/keystore.rs
line: 1
signature: pub struct Keystore
uses: 4
source: rust-analyzer
fresh: current
handle: ctx-8ca10265b285
```

Doscientos bytes contra un archivo de diez mil, medido y no supuesto: la etapa 57
de `dev/verify.sh` compara los dos números y falla si no hay un orden de magnitud
entre ellos.

Con `contexto <archivo>.rs` contesta el mapa de un archivo: todo lo que declara,
una línea cada cosa.

## Y el detalle, sólo cuando se pide

`contexto expandir=<asa>` devuelve **exactamente las líneas que ocupa esa
declaración** — no el archivo, no una ventana alrededor. El asa se deriva de lo
que señala, así que la misma pregunta devuelve la misma asa hoy y mañana: un
modelo que recibiera un asa distinta cada vez no podría cargarla de un turno al
siguiente.

El mecanismo es el de Aider y el de todo sistema de divulgación progresiva; lo
que es nuevo aquí es que la respuesta compacta es **exacta** en lugar de
heurística, porque sale de un frontend de compilador, y que trae su propia
frescura.

## El presupuesto es explícito

`presupuesto=N` en bytes. Lo que no cupo **se cuenta** y se dice, y lo que no se
devolvió se dice cuánto es. Un presupuesto que nadie puede ver no es un
presupuesto, y una respuesta que se recorta sin decirlo es una pérdida
silenciosa — que es la cosa que esta superficie no hace nunca.

La entrada más valiosa primero, y por reglas deterministas: coincidencia exacta,
luego cuántas veces se usa, luego el archivo. No hay ranking aprendido y no hace
falta.

## La lista de usos se pide, no se supone

La respuesta trae **siempre** cuántas veces se usa el nombre, y trae **dónde**
sólo con `usos=N`. Es deliberado en las dos direcciones: sobre un nombre común la
lista *es* todo el presupuesto, y el número contesta las dos preguntas que a esa
lista se le hacen casi siempre — «¿esto se usa?» y «¿esto se usa mucho?».

Cuando se pide, los lugares salen de la misma respuesta resuelta, no del índice.
Ésa es la diferencia que importa: la importación que renombra el símbolo aparece
porque un compilador sabe que es un uso, no porque el texto coincida.

## Dos fuentes, y la respuesta siempre dice cuál

`source: rust-analyzer` quiere decir que el nombre se **resolvió**.
`source: index` quiere decir que se **coincidió**, con el índice de
[[FS-en-Grafo]], que es lo que hay para un árbol que no es Rust o una máquina sin
rust-analyzer. No son igual de buenas y la superficie no finge que lo son: una
que escondiera la diferencia dejaría a un modelo actuando sobre un escaneo creyendo
que tiene un compilador.

`fresh` dice `current`, `stale` o `unknown`, y viene de
[[Conocimiento-con-Testigo]]. Una respuesta caduca se entrega **marcada**, nunca
disfrazada y nunca negada: negarla le quitaría al que pregunta la posibilidad de
decir «esto es lo que sabía, y se movió».

## `renombrar-simbolo`, y por qué no se llama `renombrar`

`renombrar` quiere decir *mueve este archivo* desde que existen los verbos de
archivo, y quedarse con la palabra habría cambiado en silencio lo que hace la
línea de alguien. Dos cosas que las dos «renombran» son dos verbos, y el nombre
largo es problema del nuevo.

Cambia el nombre en **cada lugar que de veras lo usa**: la importación que lo
renombra tres archivos más allá sí, el comentario que lo menciona no, la cadena
literal que lo contiene tampoco. La etapa 57 corre las dos columnas —el rename
resuelto y la sustitución de texto— sobre el mismo árbol, porque sin la segunda
«cambió dos archivos» es una frase sobre un número.

Escribe por la frontera de la sesión: cada ruta se ancla antes de abrir nada, y
todas antes que cualquiera, porque un rename a medias no compila en ningún lado y
parece que funcionó.

## Dónde encaja

Es la cara caliente. Un agente ordinario trabaja con **`contexto`** para saber,
**`hacer`** para cambiar ([[Ejecucion-Transaccional]]), y **`evidencia`** para el
detalle que la respuesta no trajo. Lo demás sigue ahí, alcanzable, para lo que no
es ordinario.
