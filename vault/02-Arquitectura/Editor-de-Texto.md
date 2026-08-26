---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-22
tags: [terminal, editor, doble-ruta, superficie]
---

# Editor de texto

> **Decretado por Cesar el 2026-08-22**, cuando se le puso enfrente la
> bifurcación: *«las dos caras en una entrega»*.

Es el punto 5 de la terminal usable de [[Tareas-Pendientes]], y la frase que lo
justificaba desde el principio: **sin un editor no se puede corregir un archivo
de configuración desde la máquina.** Hasta el 2026-08-22 Thalyx podía hacer un
archivo, copiarlo, moverlo, borrarlo e imprimirlo, y no podía cambiarle un byte
por dentro.

## El problema que ningún verbo anterior tuvo

[[Principio-Doble-Ruta]] es no negociable: lo que una ruta puede hacer, la otra
también, **sin degradación de capacidad**. Todos los verbos anteriores lo
cumplían gratis, porque *listar una carpeta* y *copiar un archivo* son el mismo
acto se pidan como se pidan.

**Un editor es el primer lugar donde las dos caras difieren de forma.** Lo que
una persona quiere es una pantalla en la que escribe. Una pantalla es
exactamente lo que un programa no puede manejar: se redibuja, no tiene marco, y
pedirle a un modelo que cuente pulsaciones para llegar al renglón 40 es cobrarle
los cinco costos de [[Superficie-para-el-LLM]] de golpe.

Se descartaron dos salidas antes de llegar a la que se construyó:

- **Sólo pantalla.** Deja al agente sin poder editar nada. Es la ruta doble rota
  del lado del programa.
- **Sólo direcciones por renglón.** Deja a la persona con un editor de línea en
  2026, que es la objeción de Cesar sobre el vocabulario: *«parece juguete más
  que sistema operativo serio»*.

## Lo que se decretó

**Un motor, dos caras**, que es la forma que [[Punto-Actual|la cara
estructurada]] ya había probado en `thalyx-files`:

| Pieza | Dónde | Qué decide |
|---|---|---|
| El motor | `crates/thalyx-edit` | qué **es** un cambio: abrir, direccionar, guardar |
| La cara de máquina | `thalyx-edit/src/machine.rs` | un objeto JSON por renglón tecleado |
| La cara humana | `thalyx-cli/src/edit.rs` | la pantalla, dibujada sobre `thalyx-edit/src/screen.rs` |

**Ninguna de las dos caras implementa una edición.** Las dos llaman a las mismas
funciones, que es el único arreglo en el que la pantalla y la respuesta
estructurada no pueden acabar en desacuerdo sobre lo que el archivo dice ahora.

Un solo verbo, `editar`, con la forma que `intento` ya usaba:

- `editar <archivo>` — sin subverbo, **la pantalla**. Es lo que la persona
  quiere y lo más corto de teclear.
- `editar <archivo> ver|poner|cambiar|borrar <renglón> …` — el mismo archivo
  cambiado por algo que no puede ver una pantalla.

Dos verbos separados se descartaron porque es justo el arreglo que el principio
prohíbe: **dos verbos se separan**, uno gana una capacidad y la otra ruta la
pierde sin que nadie lo haya decidido.

## Por qué la cara de máquina no tiene búfer abierto

El contrato de encuadre es **un renglón tecleado, exactamente un objeto**, y una
frontera definida de un solo lado no es una frontera —lo que costó el marcador
del prompt el 2026-08-08—. Un búfer abierto que sobrevive entre varios renglones
tecleados es estado escondido que quien llama tiene que rastrear y que ninguna
respuesta describe; quien lo pierde de vista escribe el renglón 12 de un archivo
que cree que es otro.

Así que **cada edición estructurada es una transacción entera**: lee, cambia,
guarda, contesta. Deshacer más de una edición es [[Journal-y-Snapshots|`intento`]],
y eso es lo que dice el campo `undo` de cada respuesta — en vez de que este
crate crezca una segunda versión, más débil, de una primitiva que ya existe y
está probada en hierro.

## Las teclas, y por qué no son las que uno esperaría

`Ctrl-O` guarda y `Ctrl-X` sale. **No `Ctrl-S`.**

El modo crudo de `thalyx-syscall` deja `ISIG` e `IXON` encendidos a propósito:
`Ctrl-C` tiene que seguir funcionando en una máquina cuya única terminal es
ésta. La consecuencia es que la disciplina de línea del kernel se come `Ctrl-C`,
`Ctrl-Z`, `Ctrl-S` y `Ctrl-Q` **antes de que llegue un byte a Thalyx**. Una tecla
elegida sin preguntarle a la disciplina de línea es una tecla que no hace nada —
o peor, `Ctrl-S` es XOFF y deja la terminal aparentemente muerta.

Hay una prueba en `thalyx-edit::screen` que falla si alguien alguna vez enlaza
una de las teclas que el kernel se come.

**Y no hay pantalla alterna.** Dibujar en la pantalla normal encima escribe sobre
lo que había, que es el menor de los dos costos: con `ISIG` encendido, `Ctrl-C`
termina la sesión de golpe, y un programa que hubiera cambiado a la pantalla
alterna dejaría a la persona mirando una en blanco sin forma de volver.

## Lo que se niega, y por qué negarse es lo correcto

Es el primer verbo cuyo uso ordinario destruye lo que un archivo decía antes, así
que la **regla 9 —fallar cerrado—** manda en todo:

| Qué | Qué pasa | Qué haría la versión acomodaticia |
|---|---|---|
| Bytes que no son UTF-8 | `not_text`, sin abrir | decodificar con pérdida y escribir `U+FFFD` encima del original |
| Un archivo arriba del techo (4 MiB) | `too_large`, **antes** de leer | gastar la memoria que el techo existe para proteger |
| Un renglón que el archivo no tiene | `no_such_line`, con la cuenta | recortar al final, y quien contó mal escribe abajo creyendo que escribió en medio |
| Un rango al revés | `backwards` | adivinar cuál de los dos extremos quiso decir |
| La pantalla sin terminal | `no_screen` | quedarse esperando para siempre una pantalla que no va a llegar |

El guardado es **escribir y renombrar**, igual que [[Fase-Commit-Atomico]]: una
máquina que pierde corriente a medio guardado se queda con el archivo viejo o
con el nuevo, nunca con mitad de cada uno. Un archivo de configuración en ese
estado es una máquina que no arranca.

Y tres cosas que se conservan porque cambiarlas es un diff que nadie pidió: los
finales de renglón del archivo (un archivo escrito en Windows no vuelve
convertido), el salto final o su ausencia, y los permisos —sin lo último un
script deja de ser ejecutable en silencio y la máquina deja de correrlo—.

Un enlace simbólico se edita como **el archivo al que apunta**, y la respuesta
dice a cuál escribió. Renombrar sobre la ruta del enlace —la implementación
obvia— reemplaza el enlace por un archivo regular: el enlace desaparece y el
archivo real sigue diciendo lo viejo. En una máquina donde `/etc` está lleno de
enlaces eso es un cambio de configuración que nadie hizo.

## Lo que construirlo enseñó y el decreto no anticipaba

**Un renglón tecleado no puede contener un salto de renglón.** Por el contrato de
encuadre, cuando el verbo ve su argumento ya no queda renglón. Sin un escape, la
cara estructurada sólo podría meter *una* línea por llamada mientras la persona
en la pantalla presiona Return cuantas veces quiera — o sea, la ruta doble rota
otra vez, y del lado que el decreto acababa de arreglar. Un programa armando un
bloque de cinco renglones tendría que hacer cinco llamadas, dejando el archivo en
cuatro estados que nadie pidió, cada uno guardado.

Así que `\n` y `\t` en el texto se leen como lo que son, y `\\` como una
diagonal. **Tres escapes y no un lenguaje**: un escape desconocido se deja
exactamente como se tecleó, porque un `\d` que se volviera `d` en silencio
corrompería una expresión regular en un archivo de configuración y la persona
nunca vería dónde pasó.

## Relacionado

- [[Principio-Doble-Ruta]]
- [[Superficie-para-el-LLM]]
- [[Journal-y-Snapshots]]
- [[Fase-Commit-Atomico]]
- [[Estrategia-de-Pruebas]]
