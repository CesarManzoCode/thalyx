---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-23
tags: [terminal, busqueda, doble-ruta, superficie]
---

# Búsqueda: encontrar y contenido

> **Decretado por Cesar el 2026-08-23**, sobre dos bifurcaciones que se le
> pusieron enfrente: cómo se reparten los nombres —*«dos verbos nuevos»*— y cómo
> se escribe lo que se busca adentro de un archivo —*«texto literal»*.

Es el punto 6 de la terminal usable de [[Tareas-Pendientes]]: *buscar por nombre
y por contenido*. Hasta el 2026-08-23 Thalyx podía listar una carpeta y leer un
archivo, y no podía contestar *dónde está el archivo que se llama así* ni *qué
archivos dicen esto* sin que la persona abriera carpeta por carpeta.

## Tres preguntas, y sólo una estaba contestada

`buscar` ya existía desde el 2026-08-10 y contesta la tercera: **dónde se declara
un nombre y en qué lugares se usa**, sacado del índice semántico
([[Superficie-para-el-LLM]], punto C2). Es la mejor respuesta siempre que
aplique, porque cuesta una fracción de los tokens de una lista de coincidencias y
no tiene falsos positivos por comentarios ni por cadenas.

Aplica a cinco lenguajes y a árboles que alguien indexó. Todo lo demás —un log,
un `Makefile`, un TOML, un lenguaje para el que nadie escribió un parser, un
árbol recién clonado— **no tenía ninguna respuesta**.

| se pregunta | verbo | se contesta leyendo |
|---|---|---|
| dónde está el archivo que se llama así | `encontrar` | el árbol, caminándolo |
| qué archivos dicen esto | `contenido` | el árbol, leyéndolo |
| dónde se declara este nombre y quién lo usa | `buscar` | el índice |

### Por qué tres verbos y no uno con banderas

La alternativa era `encontrar <patrón>` para nombres y `encontrar --texto "…"`
para contenido: una cosa que descubrir en vez de dos. Perdió por el **tercero de
los cinco costos** de [[Superficie-para-el-LLM]] — el costo de ambigüedad. Un
verbo cuyo significado depende de una bandera es un verbo que se puede pedir mal
en silencio, y las tres respuestas no se distinguen al verlas: una lista de
`ruta:línea` se ve igual venga del índice o del disco.

También se consideró renombrar el actual a `simbolo` y dejar `buscar` para lo
general, que es lo que alguien que viene de Linux espera de esa palabra. Cesar lo
descartó: cambia un verbo ya construido, ya documentado y con etapa propia en
`verify.sh`, y el precio de teclear una palabra distinta se paga una vez.

## Texto literal, y por qué no expresiones regulares

Lo que se teclea es lo que se busca. **Un punto es un punto.** Dos razones detrás
de la recomendación que Cesar aceptó:

1. **La imagen lleva el kernel y un programa.** Cada caja que se agrega es peso
   en el arranque, y un motor de expresiones regulares es más peso del que vale
   la pregunta. Es el mismo argumento por el que el comparador de `*` y `?` está
   escrito aquí en vez de traído, y por el que el cpio y el escritor de Btrfs
   también lo están.
2. **Un lenguaje de patrones todavía no está decidido.** El punto 9 de la misma
   lista —si Thalyx tiene lenguaje de shell, con sus comillas y sus comodines— es
   explícitamente *decreto antes que código*. Inventar aquí un dialecto de regex
   sería decidir un pedazo de ese punto adentro de una caja donde nadie lo va a
   ir a buscar.

Los **nombres** sí se comparan con `*` y `?`, porque es el vocabulario que `rm`,
`cp` y `mv` ya usan en esta sesión, y una segunda forma de escribir la misma idea
es un costo de descubrimiento que se paga dos veces.

**Las mayúsculas importan.** Una búsqueda que ignorara la caja en silencio
contestaría sobre `Error` cuando se le preguntó por `error`, y nada en la
respuesta lo diría. El error inverso —ser estricto cuando alguien quería lo
flojo— se ve de inmediato y cuesta una búsqueda más.

## Las banderas van adelante, y es una regla

`contenido en=src TODO` busca `TODO` bajo `src`. `contenido TODO en=src` busca el
texto literal `TODO en=src` en todo el árbol.

Se ve como la opción incómoda hasta que se escribe la otra. Si una bandera se
reconociera en cualquier posición, buscar el texto `en=produccion` se convertiría
calladamente en buscar nada bajo una carpeta llamada `produccion` — una respuesta
que se ve bien, llega rápido y es sobre otra pregunta. La regla que quedó falla
del otro lado: no encuentra nada, la persona ve sus propias palabras en la
respuesta y mueve la bandera. Es la **regla 9** de [[Estrategia-de-Pruebas]]: la
respuesta cautelosa, nunca la plausible.

De ahí sale además que **el sujeto es el resto del renglón, tal cual**, así que
un texto con espacios no necesita comillas. Eso importa más de lo que parece: si
Thalyx tiene comillas es el punto 9, y un verbo de búsqueda que las inventara
estaría decidiéndolo aquí.

## Una sola caminata

`walk` vivía en `thalyx-graph` con dos llamadores —construir el índice y contar
si sigue vigente— y la razón de que fuera una sola función está escrita ahí desde
que once pruebas fallaron por tenerla en dos. Con `encontrar` y `contenido` son
cuatro, y **los dos nuevos son los que una persona compara contra el primero**:
un `contenido` que entrara a `.git` donde `buscar` no entra contestaría sobre un
archivo del que el índice nunca supo, y la conclusión sería *el índice está roto*
en vez de *son dos caminatas distintas*.

Por eso la caminata se movió a `thalyx-files`, que es la caja de más abajo, y
`thalyx-graph` la usa desde ahí. El techo de 20 000 archivos se movió con ella,
por lo mismo: dos techos que tienen que ser el mismo número se separan la primera
vez que alguien ajusta uno.

## Lo que se niega en vez de contestar

| pasa esto | palabra | remedio |
|---|---|---|
| se nombró un archivo donde va una carpeta | `not_a_directory` | `name_a_folder` |
| no se dijo qué buscar | `nothing_asked` | `say_what_to_look_for` |
| el árbol pasa de 20 000 archivos | `tree_too_large` | `name_a_smaller_tree` |
| la carpeta no existe | `absent` | `look_first` |
| un cursor que esta máquina no escribió | `bad_cursor` | — |

`nothing_asked` es el que más se defiende: `contenido` con texto vacío coincide
con **cada renglón de cada archivo**. Es técnicamente correcto, nunca es lo que
nadie quiso, y cuesta la ventana de contexto entera recibirlo.

El techo se revisa **mientras se camina** y no después, así que un árbol de un
millón de archivos cuesta un momento y no un minuto, y el índice o la respuesta
que había siguen exactamente como estaban.

## Lo que no se contesta, y se dice

- **Un binario se salta y se cuenta.** `cat` sobre un binario deja una terminal
  inservible, y en la imagen no hay una segunda terminal para recuperarse. Una
  coincidencia adentro de un ELF son esos mismos bytes con una ruta enfrente. Se
  cuenta en `not_text` y no en `unreadable`, porque el archivo se leyó
  perfectamente bien y simplemente no tiene renglones que ofrecer.
- **Un archivo de más de 4 MiB tampoco se lee**, el mismo techo que [[Editor-de-Texto]],
  y por la misma razón.
- **Lo que no se pudo leer viaja en la respuesta.** Regla 10: un fallo de lectura
  no es una ausencia. Un `contenido` que se saltara callado el archivo que no
  pudo abrir contestaría «aquí no está» sobre el único lugar donde sí está.
- **`looked_at` siempre viene.** Una búsqueda de once mil archivos que no
  encontró nada y una de cuatro que no encontró nada son respuestas distintas a
  la misma pregunta, y sólo la primera significa *el patrón está mal*.
- **Un renglón largo se corta y lo dice.** Un bundle minificado es un renglón de
  doscientos mil caracteres. Quien pegara un renglón cortado de vuelta en un
  archivo estaría escribiendo algo que el archivo nunca dijo, y sólo `cut` se lo
  advierte.

## Cómo se comprueba

Etapa 30 de `dev/verify.sh`, con controles de herramientas que no son Thalyx:
`find(1)` contesta la misma pregunta de nombres y las dos listas se comparan, y
`sed(1)` dice qué hay en el renglón que Thalyx nombró. Los controles son la mitad
que hace que el resultado signifique algo — ver la **regla 4** de
[[Estrategia-de-Pruebas]]: sin ellos, una búsqueda que no encontró nada y una que
corrió contra nada se ven idénticas.

En el taller: dieciocho pruebas del motor sobre árboles reales y once que teclean
en el prompt de verdad del binario de verdad.

## Relacionado
- [[Tareas-Pendientes]]
- [[Superficie-para-el-LLM]]
- [[Principio-Doble-Ruta]]
- [[Editor-de-Texto]]
- [[Estrategia-de-Pruebas]]
- [[Punto-Actual]]
