---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-25
fecha-revision: 2026-08-26
tags: [ritmo, prioridades, cautela, no-negociable]
---

# Ritmo de construcción: agresivos sin ser estúpidos

## Problema

El 2026-08-25, después de una corrida verde —`156 proven · 2 not proven ·
0 failed`—, Cesar cortó la pregunta de qué seguía. Sus palabras, tal cual:

> «llevamos mucho tiempo sin avanzar nada realmente, estamos siendo muy
> cautelosos, el avance significativo paro desde hace mucho, ahora le estamos
> dando demasiada importancia a cosas muy simples y faciles de hacer,
> necesitamos cambiar eso, haz lo que falta, y registra esto que te estoy
> diciendo, tenemos que empezar a ser agresivos sin ser estupidos, la perfeccion
> vendra despues, despues dime que es lo siguiente que debemos de hacer».

Tenía razón medida contra el registro. Del 2026-08-23 al 2026-08-25 se
construyó: un guardia por argumento, dos llamadas de rango de prioridades, tres
arreglos del arnés, y una prueba que preguntaba con la herramienta correcta. Es
trabajo real y ninguno de esos días movió la vara de
[[Filosofia-Fundacional]] —un agente ajeno trabajando aquí— ni un milímetro.

## Decreto

**El costo de preguntar dejó de ser cero.** Una pregunta a Cesar cuesta su
tiempo y detiene la construcción hasta que contesta, así que sólo se le pregunta
lo que **sólo él puede contestar**:

| Se pregunta | No se pregunta |
|---|---|
| Cambiar o revertir un decreto suyo | En qué orden hacer dos cosas ya decididas |
| Escribir donde se puede perder algo suyo | Cómo se llama un archivo o una función |
| Gastar su hierro, su tiempo o su dinero | Si conviene actualizar un párrafo que quedó falso |
| Alcance nuevo que la bóveda no cubre | Qué hacer con un pendiente que ya está escrito |

Lo de la columna derecha **se hace**, y se le dice qué se hizo. Un pendiente ya
escrito en [[Tareas-Pendientes]] ya fue decidido por él: volver a preguntarlo es
pedirle que decida dos veces.

**La perfección viene después.** Un párrafo del README que quedó desactualizado,
un helper duplicado tres veces, un nombre que podría ser mejor — nada de eso
detiene una entrega. Se anota y sigue.

## Lo que este decreto no toca

Escrito aquí porque «agresivos» es la palabra más fácil de estirar del
proyecto, y porque *sin ser estúpidos* es la mitad que aguanta el peso:

- **No baja ninguna regla de [[Estrategia-de-Pruebas]].** Las diez salieron de
  algo que salió mal, y las doce veces que el instrumento se equivocó costaron
  más que cualquier pregunta. Ir rápido y creerle a una prueba que no corrió es
  ir rápido hacia atrás.
- **No autoriza revertir un decreto sin él.** Sigue en pie
  [[Filosofia-Fundacional]] y sigue en pie que Thalyx sólo corre módulos
  firmados. Lo que cambia es que la pregunta se le lleva **una vez, con la
  decisión aislada y el costo medido**, en vez de mezclada con tres cosas que no
  eran decisiones.
- **No permite dejar algo a medias y llamarlo entrega.** Rápido es entregar la
  cosa entera; rápido no es entregar la mitad fácil.
- **No permite que una afirmación sin comprobar se cuente como comprobada.**
  `NOT PROVEN` sigue siendo `NOT PROVEN`.

## Cómo se nota que se está cumpliendo

Una sesión que termina sin haber movido nada de [[Superficie-para-el-LLM]] ni de
la lista de pendientes de [[Tareas-Pendientes]] no cumplió, por muy verde que
esté la corrida. El conteo de `verify.sh` mide que lo construido es cierto; **no
mide que se haya construido algo**.

## Revisión del 2026-08-26: un menú de cosas baratas es la misma falta

El día después del decreto se le preguntó qué construir, con tres opciones. Las
tres estaban ya escritas en [[Tareas-Pendientes]] y una era explícitamente
barata. Sus palabras:

> «creo que ya habiamos dejado claro esto, me pones de opciones cosas sencillas
> de hacer, cuando algo es barato o no requiere de mi, hazlos todos de golpe en
> un sprint y deja listos los tests o herramientas para verificar que quedaron
> bien, registra en tu memoria o como sea que debemos de empezar a ser mas
> agresivos, llevamos mucho tiempo haciendo sprints completos dedicados a algo
> super sencillo, debemos parar eso».

El decreto de arriba ya lo cubría —«qué hacer con un pendiente que ya está
escrito» está en la columna derecha— y aun así se preguntó. Así que la regla se
escribe en la forma en que se rompió, que es la única que sirve:

**Un menú de opciones donde todas las opciones son baratas y ya están
decididas es una pregunta prohibida.** No se salva por venir con una
recomendación: la recomendación es lo que se debía haber hecho en vez de
preguntar. Si al armar las opciones ninguna cae en la columna izquierda de la
tabla, no hay pregunta — hay trabajo.

**Y lo barato no se entrega de a uno.** Todo lo barato que no lo necesita a él
va en **un solo sprint**, junto, no en una entrega por pieza. Un sprint entero
dedicado a una cosa sencilla es exactamente lo que este decreto existe para
impedir, y hacerlo con permiso pedido es peor, no mejor.

Lo que **no** baja, y él lo puso en la misma frase: **cada cosa del sprint sale
con sus pruebas o su herramienta para comprobar que quedó bien.** Ir de golpe
es entregar todo junto y comprobado, no entregar todo junto y averiguado
después.

## Relacionado
- [[Filosofia-Fundacional]]
- [[Superficie-para-el-LLM]]
- [[Tareas-Pendientes]]
- [[Estrategia-de-Pruebas]]
- [[Punto-Actual]]
