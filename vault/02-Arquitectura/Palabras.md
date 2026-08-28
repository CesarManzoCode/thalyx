---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-23
tags: [terminal, palabras, citado, shell, doble-ruta]
---

# Palabras: cómo se parte un renglón

Es el punto 9 de la terminal usable de [[Tareas-Pendientes]]: *decidir si Thalyx
tiene lenguaje de shell*. Decidido por Cesar el 2026-08-23, con estas palabras:

> «lo que sea más fácil de cubrir por ahora, pero en un futuro sí tendremos que
> hacer shell completo, no ahora, pero estemos preparados»

Así que **hay citado y no hay lenguaje**. Y la segunda mitad de la frase es la
que manda sobre el diseño: *estemos preparados* quiere decir que **nada de lo que
se aprenda hoy puede tener que desaprenderse el día del shell completo.**

## El hueco que cerraba, que no era de forma

Antes de esto, un archivo cuyo nombre llevara un espacio se podía listar y nada
más:

```
cp mi archivo.txt copia.txt   ->  Two names: what to take, and where it goes.
rm mi archivo.txt             ->  .../mi is not there
```

Nunca se destruyó nada por eso —los tres verbos se negaban— pero **no había forma
de nombrar el archivo**. Eso no es una molestia: es una capacidad que faltaba, y
por [[Principio-Doble-Ruta]] faltaba en las dos caras por igual.

## Lo que hay

El renglón se parte en palabras, y nada más. **No hay tuberías, no hay
redirección, no hay variables y no hay sustitución.** Las reglas son las de POSIX
hasta donde POSIX llega hoy:

| se escribe | quiere decir |
|---|---|
| `'…'` | literal todo, sin escapes. Adentro no cabe una comilla simple |
| `"…"` | literal salvo `\"` y `\\`, que valen por el carácter de atrás |
| `\x` fuera de comillas | el carácter de atrás, tal cual |
| `""` | una palabra vacía, que es un nombre y llega al verbo |

Una diagonal invertida adentro de comillas dobles que **no** esté frente a algo
que pueda escapar se queda como diagonal invertida — que es lo que dice POSIX, y
es exactamente el espacio que van a necesitar `$` y la comilla invertida el día
que signifiquen algo. Ese día no cambia nada de lo escrito hoy.

## Un renglón sin cerrar se niega

Una shell pediría otro renglón. Una sesión de Thalyx tiene **un** renglón y una
persona esperando respuesta, y adivinar cuál comilla se quiso cerrar es como un
`rm` acaba actuando sobre algo que nadie nombró.

| se escribe | palabra | remedio |
|---|---|---|
| `rm "a b` | `unclosed_quote` | `close_the_quote` |
| `rm a\` | `trailing_backslash` | `finish_the_escape` |

Dos palabras y no una, porque son dos errores distintos: a una comilla le falta
su pareja y una diagonal está esperando un carácter. Quien recibe la misma
respuesta para los dos tiene que averiguar cuál fue.

## La expansión se queda en el verbo, y eso es decreto

Una shell expande `*.log` antes de que el programa corra. **Thalyx no, y no lo
va a hacer.** `encontrar *.rs` busca ese patrón en todo un árbol, y un renglón que
lo expandiera primero le entregaría a `encontrar` los nombres de un solo
directorio — otra pregunta, en silencio.

Así que las palabras salen del renglón **sin expandir** y el verbo que sabe qué
significa un patrón para él hace la comparación. Eso deja las dos costumbres de
Unix intactas, cada una donde estaba:

- **Los verbos que recorren un directorio** —`rm`, `cp`, `mv`, `ls`— se comportan
  como si la shell hubiera expandido: `rm "*.log"` borra el archivo que se llama
  literalmente así, `rm *.log` borra los que terminan en `.log`. Igual que bash.
- **Los verbos cuyo argumento *es* un patrón** —`encontrar`— se comportan como
  `find`: `encontrar "*.rs"` y `encontrar *.rs` son lo mismo, porque ahí las
  comillas existen para que la shell no toque el patrón y el programa sí. Igual
  que `find . -name "*.rs"`.

Por eso una palabra recuerda **cuáles de sus caracteres venían entre comillas**,
carácter por carácter y no una bandera por palabra: `"a"*` es un patrón y `a"*"`
es un nombre, y una sola bandera contestaría lo mismo para los dos.

## Lo que cambió de significado, dicho en voz alta

Una **corrida de espacios se colapsa** donde antes no lo hacía. `contenido fn
main` busca `fn main`; `contenido fn  main` ahora busca `fn main` y antes buscaba
`fn  main`, porque el sujeto era el resto del renglón sin tocar. La forma de pedir
lo otro es `contenido "fn  main"`.

Es un cambio de verdad y se hizo a propósito: colapsar y citar es lo que hace
cualquier terminal, así que es la regla que el shell completo traería de todas
formas. Dejarlo como estaba habría sido guardar una excepción para desaprenderla
después.

## La única excepción, y por qué

**El texto de `editar` se toma del renglón, no de las palabras.**

```
editar "mi nota.txt" poner 2     sangrado
```

El nombre se parte como palabra —puede llevar espacios— y todo lo que va después
de él llega byte por byte. La razón es que ahí el argumento es **contenido**: un
renglón de configuración que empieza con cuatro espacios significa una cosa con
ellos y otra sin ellos, y perderlos no se ve hasta que algo no arranca.

Un `contenido` que colapsa espacios devuelve una búsqueda vacía, que se ve. Un
`editar` que los colapsa escribe un archivo mal, que no. Las apuestas no son las
mismas y la regla tampoco.

## Lo que esto no hace, dicho para que nadie lo busque

- No hay tuberías, ni `>`, ni `<`, ni `&`, ni `;`, ni variables, ni `$(...)`.
- El **completado con tabulador no sabe de comillas** todavía: completa nombres
  sin espacios. Es una deuda anotada, no una decisión.
- Un nombre no se cita solo al salir. Las respuestas imprimen la ruta tal cual,
  así que un nombre con espacios se lee bien y no se puede copiar y pegar de
  vuelta sin ponerle comillas. Cuando llegue el shell completo eso habrá que
  resolverlo del lado de la impresión.

## Cómo se comprueba

Ocho pruebas en el prompt de verdad —`crates/thalyx-cli/tests/`— y la **etapa 33**
de `dev/verify.sh` con archivos reales. Los controles son los que sostienen las
dos afirmaciones que importan:

- que citar agrupa → **un renglón sin comillas se parte exactamente como antes**,
  porque ninguno de los que se han tecleado hasta hoy lleva comillas;
- que un `*` citado es un nombre → hay un archivo que **se llama** `*.log`, y los
  que un patrón de verdad habría atrapado siguen ahí al final. Sin el primero,
  «el `*` citado no encontró nada» y «el `*` citado era un nombre» se ven igual;
  sin los segundos, un `rm` que hubiera dejado de encontrar cualquier cosa
  pasaría la prueba;
- que una comilla sin cerrar se niega → **línea base**: el mismo renglón con la
  comilla cerrada sí borra el archivo. Sin eso, un `rm` descompuesto se vería
  cuidadoso.

## Relacionado
- [[Tareas-Pendientes]]
- [[Superficie-para-el-LLM]]
- [[Principio-Doble-Ruta]]
- [[Busqueda]]
- [[Editor-de-Texto]]
- [[Estrategia-de-Pruebas]]
- [[Punto-Actual]]
