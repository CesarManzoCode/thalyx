---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-08-29
tags: [primitiva, rollback, concurrencia, seguridad, agentes]
---

# Identidad de estado del espacio de trabajo

## Función

Contestar exactamente esta pregunta, y ninguna parecida:

> ¿El árbol que voy a destruir es **el mismo** árbol sobre el que se me dio permiso?

No «¿cambió la misma cantidad de archivos?». No «¿sigue habiendo un intento
abierto?». **El mismo árbol, o no se procede.**

## De dónde salió: el defecto que duró un día

El 2026-08-28 se construyó el abandono en una llamada, y la autorización era una
declaración sobre los **conteos**:

```
intento abandonar snapshot=<snapshot> delete=<N> revert=<M>
```

El argumento era razonable y está escrito: si una persona escribe en el árbol
compartido mientras el intento está abierto, uno de esos dos números se mueve, la
declaración deja de coincidir y no se destruye nada.

**El argumento tiene un hueco por donde se pierde el trabajo de alguien.** Una
persona que edita un archivo que el agente **ya había editado** no mueve ninguno
de los dos números: era un archivo modificado antes y es un archivo modificado
después. La declaración seguía coincidiendo, el abandono procedía, y la edición
de esa persona volvía al snapshot.

> **Un conteo es un resumen, y un resumen no es una identidad.**

El contraejemplo está escrito como aserción en
`crates/thalyx-snapshot/tests/state_identity.rs` —
`writing_to_a_file_that_was_already_modified_moves_the_witness_and_not_the_counts`
— y de punta a punta sobre un árbol real en `thalyx_core::attempt`, con su
control positivo al lado. La regla quedó en [[Estrategia-de-Pruebas]].

## Qué es el testigo

`thalyx_snapshot::Witness`: un digest sobre **cada ruta** del árbol junto con su
tamaño, su tiempo de modificación, su tiempo de cambio y su número de inodo.
Cualquier escritura a cualquier archivo lo mueve — haya sido ese archivo ya
modificado o no, cambie el tamaño o no. Una ruta que aparece, desaparece o es
reemplazada mueve el conjunto o el inodo.

Se calcula con **el mismo recorrido** con el que se planea un restore
(`difference_and_witness`), y eso no es una optimización: un plan de un instante
y un testigo de otro son peor que ningún testigo, porque parecen un par
verificado y no lo son.

### Qué no es, dicho con precisión

**No es un hash del contenido.** Leer cada byte de un subvolumen para contestar
«¿sigue siendo el árbol que vi?» costaría más que el restore que protege, y la
comparación de este crate —la que planea el restore— siempre ha sido por tamaño
y tiempo por esa misma razón.

Lo que se afirma es acotado y exacto: **es el mismo conjunto de inodos, con los
mismos tamaños, escritos y cambiados en los mismos instantes.** Alguien que
produjera un archivo distinto del mismo largo y después falsificara los dos
timestamps al nanosegundo lo derrotaría; alguien que simplemente escriba, no.

El testigo lleva su versión adentro (`w1-…`). Un testigo hecho por otra
construcción de Thalyx se rechaza en cuanto se ve, en vez de compararse bajo
reglas con las que no se hizo — regla 9.

## Dónde se comprueba, y por qué ahí y no antes

**Dentro del candado, en el instante anterior a reemplazar el árbol.**

`thalyx_core::attempt::abandon` toma `Store::lock`, vuelve a leer el registro del
intento, calcula el testigo del árbol vivo **en ese momento** y lo compara con lo
que el llamador declaró. Una comprobación hecha fuera del candado es una
comparación con un momento que ya pasó — la misma forma de defecto que
`canonicalize`-y-después-abrir, que [[Camino-Confiable]] ya había obligado a
quitar de otro lado.

## Las dos formas de autorizar, y por qué son dos

`Authorised::ByAHuman` es [[Camino-Confiable]]: a alguien se le mostró lo que se
perdería y dijo que sí. Lo que vio es el árbol que tiene enfrente, y su respuesta
cubre lo que ese árbol tenga cuando la dé. **Una persona no puede declarar un
digest y no se le debe pedir.**

`Authorised::ByState` es un programa diciendo lo mismo de la única manera en que
un programa puede decirlo en serio: **este estado exacto y ningún otro.** Es más
fuerte que el sí de la persona, no más débil, y por eso se le permite costar una
llamada donde el de la persona cuesta dos.

## Qué se rechaza a propósito

- Un árbol que no se pudo leer completo **no tiene identidad exacta**, así que
  nunca autoriza nada y nunca se entrega la línea de una sola llamada. Reglas 9 y
  10: un directorio que no se pudo abrir no es un directorio vacío.
- `delete=` y `revert=` se **rechazan nombrando lo que los reemplazó**, no se
  ignoran. Un llamador que todavía los escribe está corriendo contra las reglas
  del 2026-08-28; ignorar las palabras lo dejaría creyendo que declaró el costo
  cuando no lo hizo.

## Dónde vive

- `thalyx-snapshot`: `Witness`, `witness`, `difference_and_witness`.
- `thalyx-core`: `attempt::Authorised`, `AttemptError::WorkspaceMoved`,
  `AttemptError::WorkspaceUnreadable`, y la comprobación bajo el candado.
- `thalyx-cli`: `intento abandonar snapshot=… state=…`, y el campo `state` en
  toda respuesta de `intento`.
- `thalyx-mcp`: el argumento `state` de `thalyx_attempt`.

## Qué falta comprobar en hardware

El contenedor no tiene Btrfs, así que aquí el mecanismo se ejercita contra el
falso de directorios y contra árboles ordinarios. En la máquina de Cesar lo
ejercita `dev/verify.sh`, etapa **55**, con su control negativo y su control
positivo sobre un subvolumen real.
