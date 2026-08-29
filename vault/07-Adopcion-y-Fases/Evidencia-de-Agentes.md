---
tipo: medicion
estado: activo
fecha-actualizacion: 2026-08-28
tags: [evidencia, benchmark, agentes, mediciones, dogfooding]
---

# Evidencia de agentes: lo que se ha medido, corrida por corrida

> **Éste es el documento canónico de la evidencia.** Cualquier otra nota que
> hable de cuánto mejora Thalyx el trabajo de un agente enlaza acá en vez de
> repetir las cifras. La prioridad que estas mediciones provocaron está en
> [[Prioridad-Operativa]]; el decreto del puente que las hizo posibles, en
> [[Agentes-Externos]].

## Las reglas de esta nota

Escritas antes de los números, porque son lo que los hace valer algo:

1. **Cada corrida es una observación independiente.** No se promedian, no se
   suman y no se convierten en una distribución. Tres corridas son tres
   corridas.
2. **Los resultados que no favorecen a Thalyx van con exactamente la misma
   visibilidad que los que sí.** CHANGE #1 está escrito igual de completo que
   READ #1.
3. **Hecho y hipótesis se separan.** Un porcentaje observado es un hecho sobre
   *esa* corrida; lo que explica por qué salió así es una hipótesis, y se
   nombra como tal.
4. **Lo que no se midió no se supone.** Un campo que el agente no imprimió está
   **ausente**, nunca en cero.
5. **Prudencia falsa tampoco.** Borrar un resultado observado por miedo a
   sobrevenderlo es la otra manera de mentir.

## El arnés

`dev/bench-external-agent.sh`. Claude Code de verdad, **el mismo modelo, el
mismo prompt y el mismo límite de turnos** en los dos brazos, sobre dos copias
idénticas del mismo proyecto:

- **Brazo A** — Linux normal: `Read`, `Grep`, `Bash`, las herramientas de
  siempre.
- **Brazo B** — **sólo** la superficie de Thalyx por MCP, dentro de la máquina.
  Sus herramientas ordinarias se le quitan a propósito: si se las dejamos, el
  modelo alcanza lo que ha usado mil millones de veces y la corrida no mide
  nada.

El prompt es **una sola cadena** para los dos brazos y no nombra ninguna
herramienta, ni MCP, ni Thalyx. El arnés lo comprueba leyendo su propio archivo
(`dev/bench-external-agent.sh --self-test`, etapa 50 de `verify.sh`).

Las dos ramas se miden en las mismas unidades porque las dos se leen del
`--output-format stream-json` del propio Claude Code: turnos, tiempo de pared,
tiempo de API, costo y tokens que él reporta, cada llamada a herramienta por
nombre, bytes que cada una le devolvió al modelo, archivos leídos y búsquedas de
texto. El lector es `dev/bench-summary.py`, aparte a propósito, con
`--self-test` contra una sesión real capturada en `dev/samples/` — regla 6 de
[[Estrategia-de-Pruebas]].

**El veredicto no lo pone el arnés**: `--expect-file` da una lista de cadenas
que la respuesta final tiene que contener, escrita a mano desde el árbol. Sin
ese archivo el resumen **no reporta veredicto**, nunca uno adivinado.

### Dónde quedan los artefactos

```sh
# una corrida completa, los dos brazos
dev/bench-external-agent.sh --project ~/code/proyecto --symbol AlgunTipo \
    --task read --expect-file dev/bench-expect/<archivo>.txt --out /tmp/bench

# lo que deja:
#   /tmp/bench/summary.json   las métricas de los dos brazos
#   /tmp/bench/a-*.ndjson     el stream completo del brazo A
#   /tmp/bench/b-*.ndjson     el stream completo del brazo B
```

Los streams se conservan enteros. Un número de esta nota que alguien quiera
volver a leer sale de ahí, no de la memoria de nadie.

---

## Corrida histórica — antes del endurecimiento del índice

**Etiquetada como histórica a propósito, y no se mezcla con las de abajo.** Es
anterior al trabajo del 2026-08-28 sobre el índice (aristas `via: symbol`, los
tres falsos positivos del parser, el refresco automático), así que mide una
superficie que ya no existe.

Claude Code (Sonnet), la misma tarea de lectura sobre un proyecto Rust, los dos
brazos correctos:

| | turnos | segundos | costo | qué usó |
|---|---|---|---|---|
| **A** — Linux | 8 | 32.8 | $0.235 | lectura y grep |
| **B** — Thalyx | 7 | 17.3 | $0.089 | 4 llamadas: 1 index, 2 symbol, 1 dependencies |

El brazo B no leyó un solo archivo y no hizo una sola búsqueda de texto.

**Y lo que enseñó en contra**, que es la parte que se conserva porque costó algo:
el brazo A encontró un dependiente que el brazo B **no** —`attempt.rs`, que usa
`Difference` sólo a través de `Plan.difference` y nunca la nombra—. Ese hueco es
lo que motivó el endurecimiento del índice. Ver [[FS-en-Grafo]].

---

## READ #1 — Orux / `TeamRuntime`

**Los dos brazos correctos.**

| métrica | A (Linux) | B (Thalyx) | diferencia |
|---|---|---|---|
| turnos | 6 | 4 | −33 % |
| tiempo de pared | 21.686 s | 17.866 s | −17.6 % |
| tiempo de API | 22.446 s | 15.753 s | −29.8 % |
| costo | $0.1871212 | $0.1007988 | **−46.1 %** |
| tokens de salida | 2 097 | 1 090 | −48 % |
| caché leído (entrada) | 96 441 | 51 544 | |
| caché creado (entrada) | 36 460 | 19 638 | |
| llamadas a herramienta | 5 | 3 (incluyendo `ToolSearch`) | |
| bytes devueltos al modelo | 8 312 | 5 255 | |
| archivos leídos | 0 | 0 | |
| búsquedas de texto | 5 | **0** | |

Qué usó cada uno: A, cinco `Grep`. B, un `thalyx_symbol` y un
`thalyx_dependencies` — **dos preguntas al índice**, cero errores de MCP.

## READ #2 — Orux / `WorkspaceStoragePort`

**Los dos brazos correctos.**

| métrica | A (Linux) | B (Thalyx) | diferencia |
|---|---|---|---|
| turnos | 4 | 4 | = |
| tiempo de pared | 17.490 s | 11.168 s | −36.1 % |
| tiempo de API | 18.165 s | 11.823 s | −34.9 % |
| costo | $0.1743828 | $0.0663174 | **−62.0 %** |
| tokens de salida | 1 456 | 841 | −42 % |
| caché leído (entrada) | 64 339 | 57 212 | |
| caché creado (entrada) | 36 477 | 11 356 | |
| llamadas a herramienta | 3 | 3 (incluyendo `ToolSearch`) | |
| bytes devueltos al modelo | 10 293 | 2 448 | **−76.2 %** |
| archivos leídos | 1 | **0** | |
| búsquedas de texto | 2 | **0** | |

Qué usó cada uno: A, dos `Grep` y un `Read`. B, un `thalyx_symbol` y un
`thalyx_dependencies` — dos preguntas al índice, cero errores de MCP.

## CHANGE #1 — LiquidLauncher / `LauncherError`

**Los dos brazos correctos. Y este resultado no favorece a Thalyx, así que va
escrito con el mismo detalle que los dos de arriba.**

| métrica | A (Linux) | B (Thalyx) | diferencia |
|---|---|---|---|
| turnos | 6 | 6 | = |
| tiempo de pared | 12.214 s | 15.105 s | **+23.7 % (peor)** |
| tiempo de API | 12.815 s | 14.067 s | +9.8 % (peor) |
| costo | $0.0856574 | $0.0820252 | −4.2 % (empate práctico) |
| tokens de salida | 804 | 818 | +1.7 % |
| caché leído (entrada) | 118 292 | 98 671 | |
| caché creado (entrada) | 13 232 | 13 270 | |
| llamadas a herramienta | 5 | 5 | = |
| bytes devueltos al modelo | 2 103 | 3 683 | **+75 % (peor)** |
| archivos leídos | 1 | 1 | = |
| búsquedas de texto | 2 | **0** | |

Qué usó cada uno: A, dos `Edit`, dos `Grep`, un `Read`. B, un `ToolSearch`, un
`thalyx_symbol`, un `thalyx_read` y dos `thalyx_edit` — una pregunta al índice,
cero errores de MCP. **El brazo B nunca abrió un intento.**

### Cómo se lee este resultado

**No es un fallo de la tesis general, y no se maquilla como victoria.**

Lo que dice, entero: en la **primera** corrida de escritura simple —una edición
localizada en un solo archivo— Thalyx todavía **no ofrece una ventaja clara**
frente a herramientas de Linux maduras. Mismos turnos, mismas llamadas, mismos
tokens de salida, empate en costo, más lento, y más bytes devueltos.

Eso es evidencia útil, y es la razón por la que esta nota existe.

Dos lecturas, y las dos son hipótesis, no conclusiones:

- **La tarea no medía la apuesta.** Un archivo cambiado una vez no tiene nada
  que revertir: no hay dependientes que encontrar, no hay partes que dejar
  consistentes, y volver atrás es una edición más. Un agente que no necesita
  deshacer nada no abre un intento porque no le sirve, que es exactamente lo que
  hizo. Lo que la tarea midió fue **el editor**, y el editor no es la apuesta.
- **O el editor es más caro de usar de lo que debería.** `thalyx_edit` trabaja
  por renglón y devolvió más bytes que `Edit`. Puede ser eso, y todavía no hay
  con qué separarlo de lo anterior.

**La decisión que se tomó después de CHANGE #1 fue no optimizar `thalyx_edit`.**
Una corrida no dice dónde está el costo, y una optimización elegida por
intuición se vuelve permanente antes de que exista la medición que la juzgue.
Primero se mide la tarea que sí ejercita la frontera reversible.

---

## El estado de la evidencia, en cuatro renglones

| clase de trabajo | qué se observó |
|---|---|
| **comprensión / navegación semántica** | dos observaciones, las dos favorecen claramente a Thalyx |
| **edición simple de un archivo** | una observación: empate en costo, peor en tiempo |
| **edición multiarchivo reversible** | una observación válida y **mixta**: correcto en los dos brazos, Thalyx más barato y más lento; provocó `sustituir` |
| **bug real de desarrollo (dogfooding)** | **todavía no medido**; el protocolo está más abajo |

Y lo que hay que decir junto a esa tabla, con todas las letras:

- Una única corrida negativa de escritura **no invalida** nada de lo que los
  otros experimentos sí observaron.
- Tampoco demuestra que «Thalyx es igual de bueno escribiendo».
- Lo que hace es **identificar una zona donde todavía no hay ventaja
  observada**, que es distinto de una zona donde se demostró que no la hay.
- Lo que corresponde ahí es **medir más antes de optimizar**, y no implementar
  una solución basándose sólo en intuición.

---

## Banco reversible: CORRIDO, REGRADADO, VÁLIDO

**Se corrió el 2026-08-29, el instrumento estaba mal, se arregló el instrumento
y la corrida se volvió a leer sin gastar nada.** El resultado bruto decía
`reversible.passed: false` en los dos brazos y ese veredicto no valía: dos de
sus partes las decidió un defecto del grader y no el agente. Con el grader
corregido, sobre los mismos artefactos, **los dos brazos salen `VALID`** — los
dos modificaron de verdad, los dos completaron bien, los dos devolvieron el
árbol. Las cifras válidas están en «REVERSIBLE #1, regradado» más abajo.

Lo de arriba se conserva entero, veredicto equivocado incluido, porque borrarlo
sería esconder el incidente. El incidente completo está en «El instrumento
estaba mal», y la regla que sale de él en [[Estrategia-de-Pruebas]].

Existe desde el commit `cb05b05` (merge sobre `d5bee37`):

```sh
dev/bench-external-agent.sh --task reversible
```

### Qué mide

Una tarea que sí ejercita la frontera reversible, en cinco partes:

1. localizar un símbolo;
2. localizar sus referencias;
3. renombrarlo mecánicamente en varios archivos;
4. comprobar qué cambió;
5. **restaurar el árbol exactamente a los bytes iniciales.**

La pregunta, escrita antes de correrla:

> Cuando una tarea exige cambiar varias partes relacionadas y después volver con
> certeza al estado inicial, ¿la frontera reversible de Thalyx le reduce el
> trabajo al mismo agente frente a Linux?

### La trampa que trae adentro

**Un agente que no hace nada restaura el árbol perfecto.** Un veredicto leído
sólo del hash pondría a un agente que se rehusó por encima de todos los que lo
intentaron — y más alto en el brazo B, que es la dirección en la que esta
comparación no se puede equivocar nunca.

Por eso `reversible.passed` es una **conjunción**, y cada parte viene de un
instrumento distinto:

- **cambió de verdad** — el nombre nuevo apareció en alguna llamada, según el
  stream del propio agente;
- **restauró** — el hash final es igual al inicial, según el anfitrión;
- **contestó bien** — la respuesta contiene la verdad conocida de
  `--expect-file`.

Si alguna se desconoce **no hay veredicto**: no es `false`. Es la regla 4 de
[[Estrategia-de-Pruebas]] —línea base y control— en un lugar donde nadie la
había buscado.

También registra `mutating_tool_calls`, `tool_calls_naming_the_new_name` y las
métricas de `attempt` del brazo B.

### Y por qué el brazo B se comprueba en dos pasos

Su espacio de trabajo vive en una imagen Btrfs que QEMU tiene abierta para
escritura, y montarla con la máquina viva es como se corrompe un store. Así que
el hash de después va con la máquina apagada:

```sh
sudo make -C image agent-export INTO=/tmp/armB-after
dev/bench-external-agent.sh --project … --symbol … --task reversible \
    --arms none --restored-b /tmp/armB-after
```

Mientras no se haga, el resumen dice `restore_check: not_proven` y no supone
nada. `THALYX_REQUIRE_RESTORE_CHECK=1` convierte ese salto en falla — regla 3,
una variable por requisito.

### La primera corrida

- **Símbolo:** `UidRegistry`.
- **Verdad conocida:** `dev/bench-expect/reversible-UidRegistry.txt` — seis
  archivos, comprobables en una línea con `grep -rl UidRegistry`.
- **Cambio mecánico:** `UidRegistry` → `UidRegistryRenamed`. Un sufijo, no un
  nombre nuevo: no hay criterio que ejercer, así que no hay diferencia de
  criterio entre los brazos.
- **Sin decidir todavía:** qué hacer con el `CLAUDE.md`. El brazo A trabaja
  dentro de la copia y Claude Code se lo carga; el brazo B trabaja en un
  directorio vacío y no lo ve. Eso le suma tokens al brazo A por algo que no es
  la tarea, o sea **al lado que favorece a Thalyx**, que es justo el sesgo que
  no se vale tener. Se evita apuntando `--project` a una copia sin `CLAUDE.md`.

### Lo que la corrida imprimió, tal cual

Se conserva como se observó, con el veredicto equivocado incluido, porque
borrarlo sería esconder el incidente. **Ninguna de estas cifras es un
resultado**: el veredicto está en disputa y las descriptivas no se han vuelto a
derivar del stream con el lector corregido.

| | brazo A (Linux) | brazo B (Thalyx) |
| --- | --- | --- |
| costo | $0.2597362 | $0.2255152 |
| pared | 47.909 s | 63.805 s |
| API | 48.485 s | 57.801 s |
| `turns` reportados | 17 | 37 |
| llamadas a herramientas | 16 | 36 |
| desglose | `Edit` 6, `Read` 7, `Grep` 1, `Bash` 2 | `thalyx_edit` 29, `thalyx_attempt` 2, `thalyx_changed` 1, `thalyx_find` 2, `thalyx_symbol` 1 |
| bytes devueltos al modelo | 19 774 | 14 365 |
| `mutating_tool_calls` | 6 | 16 |
| `attempt` | — | 1 abierto, 1 abandonado, 0 confirmado |
| `task_success` | true | true |
| árbol final igual | true | **una diferencia:** `image/build/agent.sock` |
| `intermediate_state` | false | true |
| `restored` | true | false |
| `reversible.passed` | false | false |

### El instrumento estaba mal, en dos lugares y de dos maneras

Los dos defectos están arreglados en `dev/bench-summary.py` y
`dev/bench-external-agent.sh`, cada uno con sus pruebas propias. Ninguno de los
dos se descubrió corriendo otra vez: se descubrieron leyendo el grader contra
las cifras que la corrida ya había impreso.

**1. La frontera del espacio de trabajo incluía la maquinaria del banco.**
`image/build/agent.sock` es el socket que QEMU abre para el canal del agente.
Existe en el anfitrión porque el banco está corriendo y no está en la copia del
store porque no existía cuando se empaquetó el proyecto. Ningún agente pudo
crearlo ni borrarlo, y era **la única** diferencia que el brazo B reportó. Ahora
`image/build` es maquinaria declarada, junto con `.git`, `target` y
`node_modules`, en un solo lugar que usan la caminata inicial y la final; y lo
que se deja afuera se **reporta** —cuántas entradas y un digest de sus formas,
de los dos lados— para que una exclusión no pueda ser un escondite.

**2. El testigo del estado intermedio era el único que la tarea correcta
apaga.** Era el `mtime`, y la quinta parte de la tarea es *devolver todo*. Un
agente que restaura desde una copia con `cp -a` devuelve el contenido y la
fecha. El brazo A hizo seis `Edit` y quedó como si no hubiera pasado nada.
Ahora son tres testigos —el `ctime`, que nada en espacio de usuario puede poner
para atrás; la respuesta de la herramienta, que ya está escrita en el stream y
ninguna restauración alcanza; y el contador del adaptador para el brazo B— y
cuatro campos separados donde había dos: lo que el modelo pidió, lo que la
herramienta contestó, lo que vio un instrumento de afuera y cómo quedó el árbol.

**Y una tercera cosa, que no era un defecto sino una ambigüedad.** `turns: 37`
bajo `--max-turns 30` no es una corrida cortada: `turns` cuenta mensajes de
usuario y `--max-turns` acota viajes a la API, y se separan en cuanto el modelo
pide dos herramientas en un mismo mensaje. Está explicado en
[[Estrategia-de-Pruebas]] y fijado con `--self-test` contra la sesión capturada.

### Cómo se cerró REVERSIBLE #1

**Ya se hizo; el resultado está en la sección siguiente.** Queda escrito porque
es el procedimiento, y porque cualquier corrida futura que salga con el
instrumento equivocado se cierra igual. Una sola orden, en la máquina donde
están los artefactos, sin agente y sin gastar nada:

```sh
dev/bench-external-agent.sh --task reversible --symbol UidRegistry \
    --expect-file dev/bench-expect/reversible-UidRegistry.txt \
    --out target/bench-external-agent --regrade
```

Escribe `summary-regraded.json` al lado del `summary.json` original, que **no**
se toca, y el nuevo dice en su cara de dónde salió: de la corrida original, sin
llamar a Claude, con el grader corregido después, qué evidencia se pudo reusar y
cuál falta. Cada brazo sale como `VALID`, `NOT PROVEN` o `INVALID`, y si falta
evidencia **no hay veredicto forzado**.

Antes de eso, la tabla que dice qué hicieron de verdad esas seis `Edit` —cada
llamada mutante con la respuesta que la herramienta le dio— sale de:

```sh
dev/bench-external-agent.sh --task reversible --symbol UidRegistry \
    --out target/bench-external-agent --forensics
```

Lo que se sabe hoy sin correr nada: el brazo A **no** puede regradarse por
`ctime`, porque sus dos caminatas se hicieron cuando el archivo de tiempos
todavía traía una sola columna. Su evidencia retroactiva es la respuesta de la
herramienta, que sí está en `armA.ndjson`. Si esas seis `Edit` vinieron con
`is_error: true`, el `passed: false` del brazo A era correcto por accidente y
seguirá siendo `false` — por la razón de verdad esta vez.

### REVERSIBLE #1, regradado: los dos brazos válidos

Mismos artefactos, grader corregido, **ningún agente corrido y nada gastado**.
Los dos brazos modificaron de verdad, completaron correctamente y restauraron
el árbol; `status: VALID` en los dos.

| | brazo A (Linux) | brazo B (Thalyx) | delta |
| --- | --- | --- | --- |
| costo | $0.2597362 | $0.2255152 | **−13.2 % Thalyx** |
| pared | 47.909 s | 63.805 s | **+33.2 % peor** |
| API | 48.485 s | 57.801 s | +19.2 % peor |
| llamadas a herramientas | 16 | 36 | +125 % |
| mutaciones confirmadas | 6 | 16 | +166.7 % |
| archivos leídos | 7 | 0 | 7 → 0 |
| bytes devueltos al modelo | 19 774 | 14 365 | −27.4 % |
| tokens de salida del modelo | 3 880 | 5 859 | +51 % |
| `attempt` | — | 1 abierto, 1 abandonado, 0 confirmado | |
| restauración | sí | **PROVEN** | |

**Éste es el primer resultado mixto con veredicto válido del proyecto, y hay que
leerlo como es.** La navegación semántica volvió a ganar —cero archivos leídos
contra siete, 27 % menos bytes hacia el modelo, menos dinero— y la frontera
reversible funcionó de punta a punta. Lo que perdió, y por bastante, es el
reloj.

### Lo que la corrida encontró, y el cambio que provocó

Escrito en tres renglones separados a propósito, porque son tres cosas
distintas y sólo la primera es un hecho observado.

**OBSERVACIÓN.** El brazo B necesitó 36 llamadas y 16 mutaciones donde el A
necesitó 16 y 6, y produjo 51 % más tokens de salida. El trace dice de dónde
sale la diferencia y no es un misterio: el editor del brazo A reemplaza todas
las apariciones de un archivo en **una** llamada
(`Edit(replace_all=true, …)`), así que hizo una por archivo; el brazo B sólo
sabía direccionar líneas, así que hizo una por línea —56, 61, 166, 168…— y en
cada una tuvo que **escribir el texto nuevo completo de la línea**.

**HIPÓTESIS.** La granularidad de la superficie de escritura explica buena parte
del peor tiempo de pared y del exceso de tokens de salida. No explica
necesariamente todo: hay latencia por viaje que esta nota no midió por separado.

**CAMBIO.** Se añadió una operación —no una capa— que expresa ese mismo trabajo
en una llamada: `editar <archivo> sustituir <viejo> <nuevo> [más archivos…]`,
expuesta al agente como `thalyx_edit` con `action: "substitute"`. Reemplaza una
cadena exacta en todas partes, en todos los archivos nombrados, con
precomprobación completa antes de escribir un byte, y contesta con cuentas
—cuántos lugares, en cuántas líneas, desde cuál, por archivo— en vez de
devolver el contenido. La regresión determinista que la sostiene,
`crates/thalyx-cli/tests/a_mechanical_rename_costs_one_call.rs`, arma un
proyecto de dos crates con 19 apariciones en 16 líneas de 6 archivos y hace el
mismo renombrado de las dos maneras:

| | llamadas | bytes enviados | bytes de vuelta |
| --- | --- | --- | --- |
| línea por línea | 16 | 1 523 | 2 956 |
| sustitución | **1** | 252 | 742 |

Y **es sustitución, no renombrado**. Nada en Thalyx sabe hoy distinguir el
símbolo de un comentario, de una cadena, de un homónimo en otro ámbito o de un
identificador más largo que lo contiene; llamarle «renombrado semántico» a una
sustitución léxica sería una abstracción falsa. El día que haya un índice que
sí pueda —LSP, SCIP, rust-analyzer— va debajo de esta misma API sin cambiarla.
La descripción MCP manda al agente a `thalyx_symbol` antes de sustituir, que es
lo honesto que se puede hacer hoy.

**LO QUE TODAVÍA NO SE PUEDE AFIRMAR.** Que esto mejore el banco. Nadie lo ha
medido. La próxima corrida de `--task reversible` es **la prueba de esa
hipótesis**, no su confirmación, y el arnés queda **congelado** exactamente como
está: no se adapta la prueba al producto. Repetirla es una sola orden:

```sh
dev/bench-external-agent.sh --task reversible --symbol UidRegistry \
    --expect-file dev/bench-expect/reversible-UidRegistry.txt \
    --out target/bench-external-agent-2
```

Si el tiempo de pared no se mueve, la hipótesis estaba equivocada y la operación
se queda de todos modos —una llamada donde había dieciséis es correcta aunque no
sea más rápida—, pero la causa del reloj habrá que buscarla en otra parte.

---

## El protocolo del bug real (dogfooding)

**Política, decretada el 2026-08-28.** Cuando aparezca un bug **real y no
fabricado** de Thalyx que sea adecuado para ello, **antes de arreglarlo**:

1. **Se congela el commit inicial** (el SHA queda anotado).
2. **Se describe el bug tal como apareció** — el síntoma, no el diagnóstico.
3. **Se hace una única comparación seria**, con los dos brazos de siempre:
   - **A** — el mismo agente sobre Linux normal;
   - **B** — el mismo agente trabajando a través de Thalyx.
4. Con las mismas condiciones en los dos: **mismo modelo y mismo esfuerzo,
   mismo prompt, misma base, y suficiente presupuesto de turnos y de contexto
   para que los dos puedan resolverlo.**
5. **No se optimiza la tarea para favorecer a Thalyx.**
6. **Se conserva todo**: streams completos, costo, tokens, herramientas,
   tiempos, parches y pruebas.
7. **El resultado final se verifica desde afuera**, no preguntándole al agente
   si lo logró.

### El orden de importancia de las métricas

En este orden, y el orden importa más que los números:

1. **el bug quedó realmente resuelto**;
2. **las pruebas y la verificación son correctas**;
3. **calidad y corrección del parche**;
4. **cuánta intervención humana hizo falta**;
5. **costo**;
6. **tiempo**;
7. **contexto, llamadas a herramientas, archivos leídos y búsquedas**.

Un brazo más barato que dejó el bug vivo perdió.

### Qué bug NO sirve para esta comparación

**Un bug cuyo problema central sea precisamente que el puente, el MCP o el
índice que el brazo B necesita esté roto.** Ese bug se estudia —es información
de primera— pero no sirve para esta comparación concreta: el brazo B estaría
compitiendo con una mano rota y el resultado no diría nada sobre las primitivas.

### «Thalyx construyendo Thalyx», y por qué no es marketing

La narrativa es válida **sólo** porque de cada episodio queda guardado: el SHA
inicial, el prompt, los streams, las métricas, los parches, las pruebas y el
resultado. Sin esos siete artefactos es una anécdota contada por la parte
interesada. Con ellos, cualquiera puede volver a leer la corrida y contradecir
la conclusión.

---

## Límites y transparencia

Lo que estos números **no** dicen:

- **Tres corridas no establecen una distribución ni una garantía.** No hay
  varianza medida, no hay repeticiones de la misma tarea, no hay intervalo.
- **Los porcentajes observados son de esas corridas concretas.** No se afirma
  «Thalyx reduce el costo un 50 %» como propiedad universal, y una nota que lo
  diga así está equivocada.
- **El grader actual de `--expect-file` comprueba presencia de verdad conocida**
  —que la respuesta contenga ciertas cadenas—, **no equivalencia semántica
  completa** de toda la respuesta. Una respuesta correcta escrita de otra manera
  puede fallarlo, y una respuesta que nombra los archivos correctos dentro de un
  razonamiento equivocado puede pasarlo.
- **El benchmark de CHANGE simple no ejercitó `attempt`.** Lo que midió de la
  frontera reversible es nada.
- **La corrida reversible es una sola corrida.** Su delta de reloj —+33 %— es de
  esa corrida concreta y no una propiedad medida de Thalyx.
- **`sustituir` todavía no tiene ninguna medición.** Existe porque una
  observación válida identificó su ausencia como causa plausible; que la
  corrija es una hipótesis sin comprobar hasta que el banco se vuelva a correr.
- **Todavía falta medir**: la tarea reversible multiarchivo **repetida**, tareas
  reales, bugs genuinos, otros repositorios, otras clases de cambio, y una
  comparación futura contra herramientas semánticas especializadas.

Y lo que sí es correcto afirmar, porque ocurrió:

> **En las dos primeras corridas controladas de lectura semántica realizadas
> hasta ahora, el brazo Thalyx fue correcto y usó sustancialmente menos costo y
> menos contexto, sin ninguna búsqueda textual.**

## Relacionado

- [[Prioridad-Operativa]] — la prioridad que esta evidencia provocó.
- [[Agentes-Externos]] — el puente, las herramientas y la frontera.
- [[Superficie-para-el-LLM]] — los cinco costos que estas mediciones intentan
  mover.
- [[Estrategia-de-Pruebas]] — las reglas de medición de las que salen las de
  esta nota.
- [[FS-en-Grafo]] — el índice, y el hueco que la corrida histórica encontró.
- [[Punto-Actual]] — dónde quedó el proyecto.
