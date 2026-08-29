---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-08-29
fecha-revision: 2026-08-29
tags: [primitiva, estado, cache, frescura, agentes]
---

# Conocimiento con testigo

## Función

Que una sesión nueva **no vuelva a descubrir el repositorio desde cero**, y que
nada de lo que recuerde se presente como cierto cuando ya no lo es.

Una respuesta guardada aquí trae siempre la **identidad del estado del que
salió**. Recordarla contesta una de tres cosas y nunca una cuarta:

- `current` — las entradas son byte por byte las que eran.
- `stale` — no lo son; aquí está la respuesta de todos modos, marcada.
- `unknown` — nunca se supo nada de esto.

**No hay manera de sacar el valor sin la postura.** Ésa es toda la primitiva.
Un cache cuyo `get` devuelve el valor pelón es un cache que le entrega una
respuesta caduca a cualquiera que se le olvide preguntar, y en este sistema
«cualquiera» es un modelo de frontera que va a actuar sobre ella.

## Por qué no alcanzaba lo que ya había

[[FS-en-Grafo]] ya guardaba un índice con frescura, y ésa es la mitad. Lo que no
guardaba es **la respuesta**: el símbolo que ya se resolvió, los paquetes que
Cargo ya describió, la compilación que ya salió limpia. Cada sesión nueva las
volvía a pagar, y el costo no es el CPU — es la ida y vuelta al modelo, con toda
la conversación encima.

## El testigo: por qué no es el de `thalyx_snapshot`

[[Identidad-de-Estado]] tiene un testigo `w2` que existe para **autorizar una
destrucción**, así que mete mtime, ctime e inodo: ahí una coincidencia falsa
destruye el trabajo de alguien. Éste existe para **autorizar la reutilización de
un resultado**, y tiene dos requisitos distintos:

1. **Es acotado.** Un `cargo check` de un paquete tiene que sobrevivir a un
   cambio en un paquete del que no depende, y una identidad de todo el árbol no
   sabe decir eso.
2. **Es sólo de contenido.** Un árbol restaurado byte por byte **es el mismo
   árbol**, y una validación de él sigue valiendo. Una identidad que dijera lo
   contrario convertiría cada `intento abandonar` en un cache frío — justo en la
   tarea reversible con la que se mide todo esto.

Los dos fallan cerrado igual: una ruta que no se pudo leer deja el testigo
**incompleto**, y un testigo incompleto no coincide con nada, ni consigo mismo.

### La regla que costó el defecto

Una ruta que **no está** no es una ruta que **nadie pudo leer**. Nombrar
`Cargo.lock` entre las entradas de una comprobación es correcto, y un espacio de
trabajo que todavía no tiene uno es un espacio sin candado — no un árbol del que
una parte es un misterio. Contarlo como ilegible hacía que **todos** los testigos
salieran incompletos, y como un testigo incompleto no coincide con nada, el cache
de validación no acertó ni una sola vez, en silencio, y el compilador corrió
siempre. Lo cachó una prueba que exigía un acierto y recibió un compilador.

Es la regla 10 de [[Estrategia-de-Pruebas]] leída al revés.

## Dónde vive

Un archivo SQLite por árbol, en el store y no en el árbol — por lo mismo que la
evidencia de [[Ejecucion-Transaccional]]: el árbol es lo que un rollback
reemplaza, y lo que se guarde ahí lo destruye el rollback que explica.

Una tabla, tres verbos. Nada aquí sabe qué es Rust: las **clases** de dato y el
significado de sus testigos son de quien recuerda. El primer proveedor que lo usa
es [[Semantica-Compilada]].

## La política, en una línea

**Fallo falso = más lento. Acierto falso = mal.** Toda duda se resuelve hacia el
fallo.

## Implementación

`crates/thalyx-know`. `Knowledge::remember` / `recall` / `recall_current`,
`witness::witness` sobre un conjunto acotado de rutas y sufijos, y `woven` para
una respuesta que depende de más de una cosa — el código, más el toolchain que lo
compilaría.
