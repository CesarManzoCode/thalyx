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

---

## Revisión 2026-08-30: qué es un nombre cuando es tres nombres

`Provider::ask_about` resolvía un nombre con «el primer símbolo exacto que
rust-analyzer haya listado». En un espacio de trabajo con `crate_a::Config`,
`crate_b::Config` y `crate_c::Config` eso es: tomar uno de los tres por orden de
índice, describirlo como *el* `Config`, y contestar `source: "rust-analyzer"` —
que es cierto, y es exactamente por lo que se le habría creído.

Lo que actúa sobre esa respuesta es `renombrar-simbolo`. Así que la falla era:
uno de tres crates reescrito en todos sus usos, elegido por orden de índice, sin
que nada en ningún lado dijera que se había elegido. **Una respuesta equivocada
con confianza es la peor forma que una respuesta equivocada tiene.**

El decreto ahora es de tres brazos y no de dos:

- **uno** → se contesta normal;
- **varios** → se contesta una ambigüedad estructurada: cada candidato con su
  especie, su crate, su contenedor, su firma y un asa `ruta:línea:columna` —
  deliberadamente la forma que `renombrar` y `contexto` **ya** toman, para que
  resolver una ambigüedad no obligue a aprender nada nuevo;
- **nada** → se contesta que nada lo declara, que no es lo mismo que «varios»
  y tiene otro remedio.

No hay ranking. Un candidato «más probable» arriba de la lista sería la
adivinanza que esta forma existe para quitar, con un descargo puesto encima.

**Una mutación contra una ambigüedad se niega antes de escribir.** `place` corre
antes de `rename_texts`, `rename_texts` no escribe en ningún lado, y el ciclo que
abre archivos va después de los dos. La prueba afirma los bytes de los tres
archivos, no el conteo.

Un programa ([[Transaccion-Programable]]) puede ramificar sobre esto:
`resolution === "ambiguous"` y `thalyx.needModel(candidatos)` devuelve el árbol
intacto y le pregunta al modelo, en lugar de confirmar una adivinanza.

`contexto` carga `resolution` en **toda** respuesta: `one`, `ambiguous`,
`nothing`, `file`, o `matched` cuando vino del índice — que empareja texto y por
lo tanto nunca tiene derecho a afirmar una ambigüedad. Un campo que sólo aparece
el día interesante es un campo que nadie maneja el día interesante.

Dos entradas en una misma posición son una declaración: rust-analyzer contesta
una consulta de símbolos desde varios índices y el mismo ítem puede volver dos
veces. Sin deduplicar por posición, un espacio de trabajo con exactamente un
`Config` recibiría una negativa sobre la que nadie puede actuar.

## Revisión 2026-08-30: el proveedor corre donde corre un huésped

Este documento y el encabezado del módulo decían que el proveedor es **un
lector**: nunca aplica una edición, un rename vuelve como descripción, y Thalyx
es quien escribe. Todo eso es cierto del protocolo LSP y nada de eso es cierto
del árbol de procesos.

**rust-analyzer corre Cargo.** Para contestar cualquier cosa sobre un espacio de
trabajo con un proc-macro o un build script adentro, los **compila y los
ejecuta**: código arbitrario de un registro, ejecutándose en tiempo de análisis,
con el alcance que tuviera el proceso que lo arrancó. Que era el de Thalyx: el
sistema de archivos entero, y la red.

*«No aplica ediciones, por lo tanto es de sólo lectura»* fue la conclusión
equivocada de una premisa cierta, y estuvo escrita una semana.

Ahora arranca por `thalyx_core::start_foreign` — el mismo establecimiento que
usa `ejecutar`, con una sola puerta de aplicación de política y una sola
asignación de uid. El proveedor recibe:

- su propio usuario;
- su propio cgroup con una política en el kernel;
- su propio sistema de archivos raíz, con el espacio de trabajo, el toolchain y
  el registro, y nada más;
- su propio namespace de pid, así que matar el único proceso que Thalyx sostiene
  mata cada `cargo`, `rustc` y build script debajo;
- su propio namespace de red — que es lo que «red denegada por defecto» quiere
  decir aquí;
- el mismo filtro seccomp.

Lo que se le concede: el espacio de trabajo de lectura **y escritura** —lo
primero que rust-analyzer hace sobre un árbol sin `Cargo.lock` es escribir
uno—, el toolchain y el registro de sólo lectura, y un directorio de compilación
**fuera del árbol**.

El perfil es `semantic_provider`: `module_standard` con dos números cambiados —
6 GiB en lugar de 1, porque un proveedor muerto a media indexación se reporta
como «el analizador expiró», que es una oración cierta sobre la cosa
equivocada; y 2048 procesos en lugar de 512, porque esto es un *árbol* de
compiladores. No es un framework de servicios y hay exactamente uno.

**Cae de vuelta al anfitrión, lo dice, y se le puede exigir que no.**
`start_foreign` se niega en una máquina cuyo kernel no deniega — decreto de
[[Programas-Ajenos]], y correcto — y un Thalyx que por eso no pudiera resolver
un símbolo sería una máquina donde la cara de programación no existe. Así que en
esa máquina corre en el anfitrión y **toda** respuesta carga
`analyzer_confined: false` con la razón en `analyzer_how`.
`THALYX_REQUIRE_CONFINED_ANALYZER=1` convierte la caída en una negativa: regla 3,
una variable por requisito, para que una máquina que sí puede confinar exija que
lo hizo en lugar de recibir en silencio un proceso de anfitrión el día que el LSM
no cargó.

Lo que las pruebas exigen es la honestidad, que vale en toda máquina: cada
respuesta semántica dice cuál de las dos ocurrió, los dos campos concuerdan, y
ninguno falta nunca. **Si esta máquina de veras lo confina es pregunta de
`dev/verify.sh`, etapa 59.**

### Y a quién le aplica esa exigencia — corregido el 2026-08-30

`THALYX_REQUIRE_CONFINED_ANALYZER=1` quiere decir *«esta corrida tiene que
contener una prueba de que el proveedor corrió confinado»*. **No** quiere decir
*«todo proceso de esta corrida tiene que ver un kernel que niega»*, y escrita en
la línea de comandos de `dev/verify.sh` era exactamente lo segundo: quedó en el
ambiente de `cargo test --workspace`, que corre contra la línea base observando
del script a propósito, y dos pruebas unitarias sobre lo que hace un rename
reportaron un rename que nunca arrancó.

Así que la variable se lee una vez arriba del script, se saca del ambiente, y la
vuelve a poner **únicamente** la ventana de denegación que las etapas 58 y 59
abren para sí mismas — donde ya está demostrado, leyendo el mapa del kernel con
`bpftool`, que la máquina niega. Ahí un proveedor que arranque como proceso del
anfitrión es un defecto y no una máquina haciendo lo que puede.

Nada de esto la debilita en producción: el valor por omisión no cambió, la caída
sigue siendo una caída que se anuncia, y la variable sigue convirtiéndola en
negativa donde se ponga. Lo que cambió es quién la pone. Ver
[[Estrategia-de-Pruebas]].

### Y una lectura que muta el árbol

Lo encontró la aserción del propio programa de la etapa 59, el 2026-08-30: *«el
árbol muestra 4 cambios y el programa hizo 3»*. El cuarto era `Cargo.lock`.

rust-analyzer resuelve el grafo de dependencias completo, y resolver escribe un
candado. Así que un espacio de trabajo **sin** `Cargo.lock` gana un archivo
*por haber sido interrogado*: una lectura que muta el árbol, adentro de la
transacción, atribuida a nadie.

No se esconde ni se filtra. `changed()` lo reporta —es un cambio real— y un
rollback lo quita, que es correcto. Lo que se arregló son las fixtures: un
espacio de trabajo de Rust de verdad tiene su candado versionado, y una fixture
sin él no es un caso pequeño del mundo real, es otro sistema (regla 8).

Queda escrito porque es la misma familia que el `target/` adentro del snapshot
que se arregló con `CARGO_TARGET_DIR`: **el proveedor semántico tiene efectos en
el sistema de archivos, y "es un lector" no los describe.**
