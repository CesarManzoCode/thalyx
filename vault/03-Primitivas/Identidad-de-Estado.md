---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-08-29
fecha-revision: 2026-08-29
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

## El segundo defecto: los timestamps tampoco eran una identidad — revisión 2026-08-29

El testigo que reemplazó a los conteos era un digest sobre cada ruta con su
tamaño, su mtime, su ctime y su inodo. Es estrictamente mejor que un conteo y
**tampoco es una identidad**, y el archivo que lo estrenó lo confesaba sin
darse cuenta: sus propias pruebas dormían veinte milisegundos entre dos
escrituras, porque dos escrituras seguidas del mismo programa pueden caer dentro
del mismo tic de reloj del sistema de archivos.

> **Una identidad de estado que depende de esperar al reloj no es una identidad
> de estado.**

El caso no es exótico. Es exactamente el que este mecanismo existe para cubrir,
a velocidad real: el agente escribe `shared.txt`, toma el estado, y una persona
escribe el mismo archivo en el mismo instante con una línea del mismo largo.
Misma ruta, mismo inodo, mismo tamaño y —dentro de un tic— el mismo mtime y el
mismo ctime. Ningún campo se movió. La declaración vieja coincide y el trabajo de
esa persona vuelve al snapshot.

Una prueba que arregla eso durmiendo esconde precisamente el caso que hay que
proteger. Así que no hay ningún `sleep` en las pruebas de esta primitiva y la
etapa 55 de `dev/verify.sh` escribe sin esperar nada, con el mismo largo, y a
través de un descriptor que ya estaba abierto antes de tomar el estado.

## Qué es el testigo

`thalyx_snapshot::Witness`: un digest sobre **cada ruta** del árbol junto con su
tamaño, su tiempo de modificación, su tiempo de cambio, su número de inodo y
**lo que esa ruta contiene**:

- un archivo regular aporta un digest de sus bytes;
- un enlace simbólico aporta la ruta a la que apunta —seguirlo pesaría el
  archivo de otro árbol, posiblemente uno fuera del espacio de trabajo;
- cualquier otra cosa —un fifo, un socket, un dispositivo— aporta sólo su
  especie, porque abrir un fifo se bloquea esperando a un escritor que puede no
  llegar nunca, y una comprobación de estado que se cuelga es peor que una
  imprecisa.

Cualquier escritura a cualquier archivo lo mueve: haya sido ese archivo ya
modificado o no, cambie el tamaño o no, se haya movido el reloj o no, y haya
entrado la escritura por un descriptor abierto antes o después. Una ruta que
aparece, desaparece o es reemplazada mueve el conjunto o el inodo.

Se calcula con **el mismo recorrido** con el que se planea un restore
(`difference_and_witness`), y eso no es una optimización: un plan de un instante
y un testigo de otro son peor que ningún testigo, porque parecen un par
verificado y no lo son.

El testigo lleva su versión adentro (`w2-…`). Un testigo hecho por otra
construcción de Thalyx se rechaza en cuanto se ve, en vez de compararse bajo
reglas con las que no se hizo — regla 9. `w1` era la versión de tamaño y
timestamps, y por eso una cadena `w1-…` ya no se compara con nada.

### Lo que cuesta, dicho aquí y no descubierto después

**Se leen todos los bytes del espacio de trabajo en cada comprobación de
estado.** Es un precio real y es la razón por la que el diseño anterior no lo
pagaba. Lo que se compra es lo único que este mecanismo vende: una identidad
barata y equivocada autoriza destruir el trabajo de alguien, y no hay precio al
que eso sea un ahorro.

`Witness::bytes` reporta cuánto se pesó, y la respuesta de la máquina lo lleva
como `state_bytes`, para que un llamador que ve una llamada lenta sepa por qué en
vez de tener que medirlo.

### Por qué no se usó el contador de mutaciones del kernel

Thalyx ya tiene uno —`thalyx-watch`— y en la máquina de Cesar `verify.sh` ya
prueba que detecta 5000 escrituras por un descriptor ya abierto, que se puede
acotar a un árbol, y que los cambios fuera de ese árbol no mueven su cuenta.
Convertirlo en la generación del espacio de trabajo sería mucho más barato que
leer un monorepo entero. **Se inspeccionó y se descartó**, y estas son las
razones, en orden de gravedad:

1. **Su gancho de escritura es `lsm/file_permission`**, que sale de
   `rw_verify_area` —la ruta de `read(2)` y `write(2)`—. Una página sucia de un
   mapeo compartido no pasa por ahí. Un archivo reescrito por `mmap` no mueve el
   contador. *Un cambio que no se ve es la única falla que este diseño no puede
   tener.*
2. **Necesita el watcher BPF cargado, `bpftool` y privilegio.** Dentro de la
   imagen de Thalyx no hay `bpftool` que ejecutar, y el requisito de fallar
   cerrado cuando no se puede demostrar la cobertura convertiría cada rollback de
   una sesión ordinaria en un rechazo. Eso no es proteger el vertical, es
   apagarlo.
3. **Acotarlo a un árbol exige registrar la raíz en un mapa BPF** y se rinde ante
   cualquier montaje debajo del árbol, porque el recorrido del kernel sube por
   `d_parent` y nunca cruza un punto de montaje.
4. **La cuenta no tiene época.** Si el watcher se recarga entre capturar y
   comprobar, la cuenta reinicia; hoy eso se detecta sólo porque *bajó*, y una
   detección que depende del signo no es una garantía.

Nada de esto lo descalifica para lo que ya hace —decidir si el índice puede
saltarse un recorrido—, donde equivocarse cuesta un recorrido de más. Aquí
equivocarse cuesta el trabajo de una persona.

### La otra cara: lo que pasa fuera del árbol no cuenta

El testigo recorre el espacio de trabajo y nada más, así que alguien compilando
en otro directorio no invalida un rollback aquí. Eso es por construcción y no por
configuración, que es exactamente lo que el contador tendría que ganarse
registrando raíces.

### Lo que el testigo no afirma

El recorrido no es atómico. Lo que dice es: *en el momento en que se miró cada
una de estas rutas, esto contenía.* Un archivo reescrito mientras el recorrido
va por otro se ve en uno de sus dos estados y no en un tercero —pero cuál de los
dos no queda fijado, y por eso la comparación que autoriza una destrucción se
hace **bajo el candado, en el instante anterior al intercambio**, y por eso el
árbol que se reemplaza se guarda en vez de borrarse.

### Un límite conocido del resumen, que no es el de la identidad

`Difference` —el resumen que se le muestra a una persona antes de contestar—
compara por tamaño y mtime, no por contenido, y por eso una confirmación cuesta
un recorrido de cada árbol en vez de una lectura de los dos. Dos escrituras del
mismo largo dentro de un tic pueden por tanto *contarse* como ningún cambio.

**El resumen puede quedarse corto por un archivo; la autorización no puede
equivocarse de estado.** Hacer exacto también el resumen significa leer cada byte
del snapshot además del árbol vivo, o sea duplicar lo que cuesta cada
confirmación. No se hizo hoy y está anotado en [[Tareas-Pendientes]]; la
aserción que lo sostiene es
`the_count_of_modified_files_can_understate_where_the_witness_cannot`, escrita
para fallar si algún día el resumen se vuelve exacto y esta nota deja de ser
cierta.

## Dónde se comprueba, y por qué ahí y no antes

**Dentro del candado, con el intercambio ya armado y nada más pendiente.**

`thalyx_core::attempt::abandon` toma `Store::lock`, vuelve a leer el registro del
intento, y entrega la última pregunta al restore: el restore escribe su
intención, construye la copia escribible del snapshot —que es la mitad cara— y
*después* calcula el testigo del árbol vivo y lo compara con lo que el llamador
declaró. Lo único que queda entre la respuesta y la destrucción es el
`RENAME_EXCHANGE`.

Una comprobación hecha fuera del candado es una comparación con un momento que ya
pasó — la misma forma de defecto que `canonicalize`-y-después-abrir, que
[[Camino-Confiable]] ya había obligado a quitar de otro lado. Y una comprobación
hecha *dentro* del candado pero antes de armar el restore deja del lado
equivocado todo lo que el restore tarda en prepararse, que en Btrfs son
milisegundos y un milisegundo alcanza para que el editor de otra persona guarde.

### La carrera que queda, nombrada en vez de negada

No es cero y nada que no congele el sistema de archivos la haría cero: queda un
intervalo —un recorrido del árbol, más un `renameat2`— en el que una escritura de
alguien que no toma el candado de Thalyx cae después de la respuesta. Lo que se
garantiza son dos frases y no una:

- **una escritura que terminó antes de la comprobación nunca se pierde**: el
  testigo se movió, la declaración deja de coincidir, y esto se rechaza;
- **una escritura que cae dentro de la ventana no se destruye, se desplaza**: el
  árbol que estaba vivo se conserva, y `replaced_kept_as` dice dónde quedó.
  [[Rollback-vs-Restore]] lo exige de todo restore, y es lo que hace que la
  carrera residual sea sobrevivible en vez de silenciosa.

Los clientes de Thalyx no pueden estar en esa ventana: se forman en `Store::lock`,
que el abandono tiene tomado.

Un rechazo en ese último instante no deja nada a medio construir: la copia
escribible se va con el `Prepared` que la hizo, y la aserción que lo sostiene es
`a_refused_rollback_leaves_nothing_half_built_beside_the_subvolume`.

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

- `thalyx-snapshot`: `Witness`, `witness`, `difference_and_witness`,
  `what_it_holds`, y el par `prepare_restore` / `Prepared::commit` que deja la
  última comprobación pegada al intercambio.
- `thalyx-core`: `attempt::Authorised`, `AttemptError::WorkspaceMoved`,
  `AttemptError::WorkspaceUnreadable`, y la comprobación bajo el candado.
- `thalyx-cli`: `intento abandonar snapshot=… state=…`, `consent`,
  `how_it_is_authorised`, y los campos `state`/`state_bytes` en toda respuesta de
  `intento`.
- `thalyx-mcp`: el argumento `state` de `thalyx_attempt`.

## El defecto de la unión, que la etapa 55 encontró el 2026-08-29

La primera corrida de la etapa 55 en Fedora **no destruyó nada** y aun así falló:
el rechazo no dijo `workspace_moved`, dijo `done: false` y devolvió una línea
`confirm_with` **nueva**.

La causa no era el testigo, que separó los dos árboles correctamente. Era
`consent`, en `thalyx-cli`: comparaba la declaración del llamador contra el
testigo con el que se había hecho *el plan* y, si no coincidían, contestaba con
el objeto de costo. Así que la llamada nunca llegaba a la comprobación bajo el
candado, y la única palabra que este mecanismo existe para decir era inalcanzable
desde la cara que dice palabras.

Y es peor de lo que parece: un agente en un ciclo copia esa línea nueva y en la
llamada siguiente sí destruye el trabajo de la persona. **Un rechazo que se lee
como "volvé a intentar con esto" no es un rechazo.**

Por qué el contenedor no lo vio: la prueba del núcleo que demuestra el rechazo
llama a `abandon` directamente, y lo único que estuvo mal siempre fue el camino
entre las dos. Es la regla 5 —el arnés también es un instrumento— en su forma
estrecha: *una prueba de cada mitad no es una prueba de la unión.* Quedó escrita
en [[Estrategia-de-Pruebas]].

Desde entonces `consent` no compara nada: una llamada que nombra el intento **y**
nombra un estado es una autorización, y si esa declaración es cierta sobre el
árbol se decide en el único lugar donde esa pregunta significa algo.

## Qué falta comprobar en hardware

El contenedor no tiene Btrfs, así que aquí el mecanismo se ejercita contra el
falso de directorios y contra árboles ordinarios. En la máquina de Cesar lo
ejercita `dev/verify.sh`, etapa **55**, con su control negativo, su columna de
trabajo fuera del árbol y su control positivo sobre un subvolumen real —y desde
2026-08-29 la escritura ajena es del mismo largo, por un descriptor ya abierto, y
sin esperar nada.
