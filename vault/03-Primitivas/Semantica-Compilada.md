---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-08-29
fecha-revision: 2026-08-29
tags: [primitiva, rust, semantica, validacion, agentes]
---

# Semántica compilada, y validación de lo afectado

## Función

Contestar **qué es un nombre**, no dónde aparece el texto; y contestar **qué hay
que volver a compilar**, sin que nadie lo decida a mano.

## De dónde salió: una frase que llevaba meses escrita

`crates/thalyx-graph/corpus/05-alias/expected.json` dice, en `known_limits`:

> `Keys` en `src/boot.rs:3` es un uso de `Keystore` y el índice no lo dice.
> Seguir un alias es rastrear una ligadura, y eso es un compilador y no un
> escaneo.

Tiene razón, y no deja de tenerla porque el escáner se esfuerce más. La
resolución de nombres en Rust es `use ... as`, importaciones glob, sombreado del
prelude, higiene de macros, selección de métodos de trait e `impl` repartidos en
varios archivos. Un escáner que contestara todo eso **sería** rust-analyzer, mal
hecho.

**Entonces Thalyx le pregunta a rust-analyzer.** Lo que Thalyx conserva es lo que
nadie más hace: la identidad de la respuesta, su frescura, su presupuesto, la
transacción en la que ocurre, y la autoridad para cambiar algo.

## El reparto, en una línea cada uno

- **Cargo** dice qué es el espacio de trabajo y qué crate depende de cuál.
- **rust-analyzer** dice qué *es* un nombre.
- **[[Conocimiento-con-Testigo]]** dice si alguna de las dos respuestas sigue
  valiendo.
- **Thalyx** decide qué se hace, y es lo único que escribe.

## La frontera, dicha en voz alta

**El proveedor es un lector.** Se arranca con el espacio de trabajo como raíz, se
le pregunta, y se le mata. Nunca escribe: un rename regresa como una
**descripción** de ediciones, y aplicarlas es de Thalyx, por el mismo camino que
cualquier otra mutación, dentro de la misma transacción, contra la misma frontera
del espacio de trabajo. Una URI que apunte fuera se rechaza aquí, no se filtra.

Lo que **todavía no** tiene es confinamiento: hoy es un proceso anfitrión que
Thalyx lanza, no uno que `ejecutar` confina como confina a `cargo`. Es un hueco
real y está en [[Tareas-Pendientes]], no escondido.

## Validación de lo afectado

`hacer` compilaba los crates en los que **estaban** los archivos cambiados. Ésa
es la mitad fácil y está mal en el caso que importa: cambiar un tipo en
`thalyx-core` y compilar `thalyx-core` no prueba nada sobre los doce crates que
lo usan.

Las dos direcciones del grafo de Cargo son preguntas distintas:

- **Dependientes** — *qué hay que volver a compilar.* Hacia arriba del cambio.
- **Clausura** — *de qué depende la respuesta.* Hacia abajo de la selección, y es
  de lo que está hecha la identidad del cache.

Confundirlas produce un cache que o nunca acierta o acierta cuando no debe.

Regla: archivo cambiado → paquete que lo contiene (el manifiesto más cercano) →
más todo lo que depende de él, transitivamente, dentro del espacio de trabajo. Un
`Cargo.lock` o el manifiesto raíz alcanza a todos. Un archivo que no cae en
ningún paquete se **nombra**, nunca se ignora: es una razón por la que la
comprobación podría no cubrir el cambio.

Hay escotilla: un programa que nombra los paquetes él mismo **reemplaza** la
derivación, no la amplía. Quien ya decidió qué compilar, decidió.

## El cache de validación

Identidad = contenido de la clausura de dependencias + el manifiesto y el candado
del espacio de trabajo + el toolchain. El toolchain va adentro por la regla 12 de
[[Estrategia-de-Pruebas]]: los mismos bytes compilados por otro rustc son otra
respuesta.

Sólo se guarda un veredicto **sobre el árbol**. Una comprobación que *no se pudo
correr* — sin cargo, sin kernel que confine — no es un resultado, y guardarla
haría que una máquina que una vez no tuvo toolchain siguiera diciendo
`not_proven` sobre bytes que nunca compiló.

Se guarda el veredicto y una línea, nunca la salida del compilador: son megabytes,
ya están en la evidencia de la corrida que los produjo, y un cache que los
cargara sería una segunda copia de justo lo que este sistema mantiene fuera del
contexto del modelo.

## Se construye fuera del árbol

`CARGO_TARGET_DIR` — para rust-analyzer y para el `cargo check` confinado —
apunta al store. Un `target/` dentro del árbol está dentro del snapshot: la
frontera copiaría un árbol de compilación, la diferencia reportaría miles de
archivos cambiados, y un rollback tiraría el cache de compilación que hace barata
la comprobación siguiente. Lo encontró una prueba que exigía que una corrida que
cambió dos archivos reportara dos.

## Implementación

`crates/thalyx-rust`: `metadata` (Cargo), `analyzer` (LSP), `affected` (las dos
direcciones y la identidad), `edits` (rangos a texto). Los verbos que lo exponen
son `contexto` y `renombrar-simbolo`, y el paso `rename` dentro de `hacer` —
[[Contexto-Progresivo]].
