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

## AVISO — 2026-08-29, revisión posterior: el brazo A no trabajaba en `--project`

**Esto se aplica a todas las corridas de abajo, sin excepción, y hay que leerlo
antes que cualquier número de esta nota.**

Una revisión de la forense de REVERSIBLE #1 encontró al brazo A ejecutando
comandos como

```
cd /home/cesarmanzocode/thalyx
```

cuando el arnés había recibido `--project /tmp/bench-thalyx`. La causa está en
una sola línea de `dev/bench-external-agent.sh`, y estuvo ahí **desde el primer
commit del arnés**: `--out` vale por omisión `$ROOT/target/bench-external-agent`,
`$ROOT` es el checkout donde vive el script, y la copia del brazo A se hacía en
`$OUT/a`. Es decir, `claude` se arrancaba **físicamente dentro del clon de
trabajo de Cesar**, y Claude Code recoge el `CLAUDE.md` de cada ancestro de su
directorio de trabajo — el de este proyecto, que empieza con «lee esto antes que
nada» y nombra `vault/06-Pendientes/Punto-Actual.md`. El brazo A empezaba la
tarea con instrucciones sobre `~/thalyx` en el contexto, y se fue a trabajar a
`~/thalyx`.

**Qué se degrada y qué no.** Los datos de abajo **no se borran** — la regla 5 de
esta nota dice que borrar un resultado observado es la otra manera de mentir.
Lo que se degrada es su **fuerza**:

| lo que sigue valiendo | lo que ya no |
|---|---|
| observación / diagnóstico: el brazo A hizo *algo* y costó *eso* | **comparación controlada** entre dos brazos |
| el brazo B, que sí estaba confinado a su copia dentro de la máquina | que los dos brazos vieran el mismo árbol |
| que el arnés corre de punta a punta | cualquier porcentaje leído como «Thalyx cuesta −X %» |

Los porcentajes de abajo son de un brazo A cuyo árbol de trabajo, contexto y
directorio efectivo no están probados. **No son un resultado de esta comparación
y no deben citarse como tal.**

### Qué corridas están afectadas

`git log` del arnés dice que el escenificado del brazo A —`rm -rf "$OUT/a"`,
`tar` desde `$PROJECT`, `run_arm A "$OUT/a"`— es **idéntico desde `a6feb32`**,
el commit que trajo el arnés. Así que:

| corrida | mecanismo afectado | qué se puede afirmar hoy |
|---|---|---|
| corrida histórica | **sí** | mismo arnés, misma línea |
| READ #1 | **sí** | mismo arnés, misma línea |
| READ #2 | **sí** | mismo arnés, misma línea |
| CHANGE #1 | **sí** | mismo arnés, misma línea |
| REVERSIBLE #1 | **sí, y observado** | es la única donde la forense lo *muestra* |

La distinción importa y no se debe suavizar en ninguna dirección:

- **El mecanismo estaba en todas.** El brazo A de todas ellas arrancó dentro de
  `~/thalyx`, con el `CLAUDE.md` de este repositorio en su contexto. Eso es un
  hecho sobre el arnés, no una sospecha.
- **Que además *saliera* de su copia sólo está observado en REVERSIBLE #1**, que
  es la única cuya forense se leyó con esa pregunta en la mano. De las otras no
  se afirma ni que sí ni que no: **nadie ha mirado**, que es distinto de que no
  haya pasado.

Y se puede mirar, gratis, sin correr ningún agente: los streams de esas corridas
están en sus directorios `--out`, y

```sh
dev/bench-summary.py --scope-check <dir-de-la-corrida> --arm A
```

lee de cada stream el `system init` —que trae el directorio en que arrancó— y
cada ruta de cada llamada, y dice `INTACT` o `VIOLATED`. Mientras eso no se
corra sobre cada `--out` que sobreviva, las cuatro filas de arriba se quedan
como están: mecanismo afectado, salida no observada.

### Qué se cambió para que no pueda repetirse

Está descrito entero en la cabecera de `dev/bench-external-agent.sh`. En corto,
y en el orden en que corre, todo **antes** de llamar a Claude:

1. **el brazo B se prueba vivo** (`thalyx-mcp --preflight`) contra el canal real
   —el hello, un `where` y un `list .` comparado con `--project`—, porque la
   corrida del 2026-08-29 pagó el brazo A entero y después el B dio `0s` y cero
   eventos, con el único control siendo `[ -S "$SOCKET" ]`, que pregunta si
   existe un *archivo*;
2. **los dos brazos se comparan de entrada**: la copia del brazo A y el sello
   que `project-stage` escribió al importar el proyecto se resumen con el mismo
   programa, y `provenance.json` guarda commit de origen, manifiesto lógico de
   entrada, exclusiones y directorio efectivo de cada brazo;
3. **el brazo A se ancla**: su copia se escenifica **fuera de este checkout**,
   se revisa cada ancestro por `CLAUDE.md`, `.claude/`, `.mcp.json` o `.git`, el
   proceso arranca físicamente adentro, y un hook `PreToolUse` rechaza cualquier
   llamada que nombre una ruta de afuera;
4. **se comprueba después que se quedó ahí**, leyendo del stream el `system
   init` y todas las rutas de todas las llamadas. Una sola llamada afuera deja
   la corrida `INVALID`, y se comprueba **entre los dos brazos**, que es el
   último momento en que saberlo cuesta menos que el brazo B.

Los cuatro están aparte a propósito: los tres primeros son cosas que se pueden
hacer verdaderas; el cuarto es el único que es **evidencia**, porque no necesita
que ninguno de los otros haya funcionado.

---

## El estado de la evidencia, en cuatro renglones

> **Léase con el aviso de arriba.** Las cuatro filas describen lo que se
> observó; ninguna de ellas es hoy una comparación controlada, porque el brazo A
> de todas ellas no estaba anclado a `--project`.

| clase de trabajo | qué se observó |
|---|---|
| **comprensión / navegación semántica** | dos observaciones, las dos favorecen claramente a Thalyx — **brazo A no anclado** |
| **edición simple de un archivo** | una observación: empate en costo, peor en tiempo — **brazo A no anclado** |
| **edición multiarchivo reversible** | una observación **mixta** con veredicto válido bajo el grader de entonces: correcto en los dos brazos, Thalyx más barato y más lento; provocó `sustituir` — **brazo A no anclado, y observado saliéndose** |
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

## Banco reversible: CORRIDO, REGRADADO — y después DEGRADADO a observación

> **REVERSIBLE #1 ya no es una comparación controlada.** El veredicto `VALID`
> que se lee abajo lo dio el grader de ese día, y era honesto con lo que ese
> grader sabía: que los dos brazos cambiaron de verdad, contestaron bien y
> devolvieron el árbol. Lo que ese grader no preguntaba —y por eso no lo dijo—
> es **dónde** trabajó el brazo A. Preguntado después, la forense contestó
> `cd /home/cesarmanzocode/thalyx`.
>
> Todo lo que sigue se conserva como **observación y diagnóstico**, con sus
> números tal como se imprimieron. Ninguno de ellos es un resultado de la
> comparación que esta nota existe para hacer. Ver el aviso del 2026-08-29 más
> arriba.
>
> Y hay una cosa más que ese grader no podía decir, encontrada en la misma
> revisión: llamaba `write=False` a un `Bash` cuyo comando era
> `git checkout -- <archivo>`. Contar mutaciones a partir del nombre de la
> herramienta es contar intenciones, no efectos; hoy hay tres clases —`writes`,
> `reads`, `unknown`— y el testigo del sistema de archivos es la autoridad.
> Ninguna de las cuentas de abajo se recalculó con eso.

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
hipótesis**, no su confirmación, y el arnés queda **congelado** en lo que mide:
no se adapta la prueba al producto. Lo único que cambió desde entonces son los
controles que deciden si la corrida vale —anclaje del brazo A, preflight del
brazo B, paridad de entradas—, que no tocan ni el prompt ni las métricas.

**La repetición limpia, en dos órdenes.** La primera importa el proyecto en la
máquina y la arranca; la segunda corre el banco. `--project` es obligatorio y es
lo que los dos brazos reciben:

```sh
make -C image agent PROJECT=/tmp/bench-thalyx

dev/bench-external-agent.sh --task reversible --symbol UidRegistry \
    --project /tmp/bench-thalyx \
    --expect-file dev/bench-expect/reversible-UidRegistry.txt \
    --workspace /tmp/thalyx-bench-arm-a \
    --out target/bench-external-agent-3
```

`--workspace` está escrito aunque sea el valor por omisión, porque es
exactamente lo que salió mal: **la copia del brazo A no puede vivir dentro de
este checkout.** Si se omite, vale `$TMPDIR/thalyx-bench-arm-a`; si se apunta a
algún lugar con un `CLAUDE.md`, un `.claude/`, un `.mcp.json` o un `.git`
encima, la corrida se niega a arrancar y dice cuál de ellos encontró.

Si el brazo B no está vivo, o los dos brazos no vienen del mismo árbol, la
corrida se detiene **antes de llamar a Claude en ningún brazo** y no gasta nada.

Si el tiempo de pared no se mueve, la hipótesis estaba equivocada y la operación
se queda de todos modos —una llamada donde había dieciséis es correcta aunque no
sea más rápida—, pero la causa del reloj habrá que buscarla en otra parte.

---

## REVERSIBLE #2 — la corrida post-lote, y las dos cosas que el instrumento hizo mal

**Los números de abajo son de la corrida del 2026-08-29 en
`target/bench-external-agent-3/`, tal como el arnés los imprimió.** El regrade
con el instrumento corregido **todavía no se ha corrido** —se corre en la
máquina de Cesar, donde están los artefactos, y el comando está más abajo—, así
que el veredicto formal de esta corrida es **PENDIENTE**, ni válido ni inválido.
Lo que sí se sabe de ella, porque lo dijo el arnés en su momento: el restore de
los dos brazos quedó `PROVEN`.

### La secuencia, que es lo que esta sección conserva

**OBSERVACIÓN 1** — REVERSIBLE #1. El brazo B fue correcto y más barato en
lectura y gastó un tercio del reloj haciendo **dieciséis** mutaciones dirigidas
por línea donde el brazo A hizo seis reemplazos de archivo entero. La causa
plausible que se escribió entonces: la granularidad de la superficie de
escritura.

**CAMBIO 1** — `editar … sustituir`: una cadena exacta, en varios archivos, en
una llamada.

**OBSERVACIÓN 2** — esta corrida. El cambio 1 funcionó y se puede leer en los
números:

| | REVERSIBLE #1 (brazo B) | REVERSIBLE #2 (brazo B) |
|---|---|---|
| llamadas a herramienta | 36 | 14 |
| mutaciones | 16 | 5 |
| turnos | 37 | 15 |
| tokens de salida | 5 859 | 3 661 |
| bytes enviados | 4 074 | 1 383 |
| tiempo de pared | 63.8 s | 57.7 s |

Y esta corrida, brazo contra brazo:

| métrica | A (Linux) | B (Thalyx) | diferencia |
|---|---|---|---|
| llamadas a herramienta | 17 | 14 | −17.6 % |
| mutaciones | 6 | 5 | −16.7 % |
| archivos leídos | 8 | **0** | |
| bytes devueltos al modelo | 32 251 | 17 400 | **−46.0 %** |
| bytes enviados | 2 324 | 1 383 | −40.5 % |
| tokens de salida | 4 324 | 3 661 | −15.3 % |
| tiempo de API | 50.392 s | 50.964 s | +1.1 % |
| tiempo de pared | 50.500 s | 57.696 s | **+14.2 %** |
| costo | $0.2070212 | $0.2365656 | **+14.3 %** |
| mensajes del asistente | 8 | 15 | |
| mensajes con herramienta | 7 | 14 | |
| máximo de herramientas en un mensaje | 6 | **1** | |
| `attempt` | — | comenzado 1, abandonado 1, confirmado 0 |
| restore | `PROVEN` | `PROVEN` | |

**El resultado no favorece a Thalyx en costo ni en reloj, y va escrito con el
mismo detalle que los que sí** — regla 2 de esta nota.

Y la traza del brazo B, que es de donde sale el cambio 2:

```
1. ToolSearch
2. attempt begin
3. thalyx_edit substitute   (multiarchivo)
4. thalyx_edit substitute
5. thalyx_edit substitute
6. thalyx_edit substitute
7. thalyx_edit substitute
8. attempt abandon          -> needs confirmation
9. attempt abandon confirm
10. find
…
```

**CAMBIO 2** — `editar … sustituir-lote`: varias sustituciones exactas, con sus
propios conjuntos de archivos, en una llamada. Las cinco de arriba son un solo
plan que la API no podía expresar: llevaba un par `old`/`new` a través de muchos
archivos y no llevaba muchos pares.

**Nada de esto dice que el cambio 2 mejore el costo o el reloj.** Eso se mide
corriendo el banco. Lo único medido hasta ahora es local y es esto, sobre un
fixture con la forma de ese plan y nombres propios, sin Claude y sin API:

| | antes | después |
|---|---|---|
| llamadas lógicas | 5 | **1** |
| llamadas que mutan | 5 | **1** |
| bytes de petición | 569 | 508 |
| bytes de respuesta | 1 499 | 1 452 |

Los bytes bajan poco a propósito: la respuesta del lote dice *más* —qué hizo
cada patrón y cómo quedó cada archivo— en menos espacio del que ocupaban cinco
respuestas. Lo que baja de verdad son los viajes de ida y vuelta.

### Los dos errores del instrumento, y ninguno es del sistema medido

**Primero: `--out` relativo.** La corrida se lanzó con
`--out target/bench-external-agent-3` y el brazo A contestó `Settings file not
found.`. `run_arm` hace `cd` al directorio del agente antes de ejecutar
`claude`, así que `--settings $OUT/armA.settings.json` se resolvía contra ese
directorio y no contra donde estaba parado quien escribió el comando. Arreglado
en un solo lugar (`normalise_paths`), con self-test que corre desde un
directorio que no es ninguno de los dos.

**Segundo, y es el que importa: el scope del brazo B era un falso positivo.** El
grader comparaba

```
/home/bench-thalyx                    el espacio de trabajo, adentro de la máquina
…/bench-external-agent-3/b            el directorio donde arrancó el proceso claude
```

y los declaraba distintos. Lo son: **no están en el mismo espacio de nombres.**
El brazo B corre en el anfitrión con todas sus herramientas de archivo quitadas;
el directorio vacío donde se para no es un espacio de trabajo del que se salió,
es el piso de un cuarto sin nada adentro.

Las dos palabras quedan separadas para siempre, y cada brazo se juzga bajo la
frontera que ese brazo de verdad tiene:

| | `host_control_cwd` | `guest_project_workspace` |
|---|---|---|
| **brazo A** | la copia montada | la misma copia — y que sean la misma es su frontera |
| **brazo B** | infraestructura, un directorio vacío | una ruta adentro de la máquina, alcanzable sólo por el socket |

La frontera del brazo A: arrancó adentro del árbol y no salió. La del brazo B es
el canal, y son cuatro cosas:

1. no tiene ninguna herramienta host que pueda leer o escribir el proyecto —
   `Read`, `Edit`, `Write`, `Grep`, `Glob`, `Bash` se le quitan, y un stream con
   una de ellas adentro es una corrida cuyo confinamiento no ocurrió;
2. toda ruta que sus herramientas de Thalyx **aceptaron** cae bajo el espacio de
   trabajo guest;
3. toda ruta con la que la máquina **contestó**, también;
4. el preflight probó, antes de gastar un centavo, que el canal apuntaba a ese
   árbol.

**Una ruta que la máquina rechazó no es una brecha: es la frontera funcionando**,
que es exactamente lo que el experimento mide. Regla 4: la diferencia entre una
negación y una operación que nunca existió. Un preflight ausente es `NOT PROVEN`;
uno que alcanzó otro espacio de trabajo es evidencia en contra, y eso sí es
`VIOLATED`.

**Y buscando eso apareció un tercer hueco, más viejo y peor.** `paths` en plural
no era un campo que el grader conociera, y su barrido de respaldo sólo miraba
valores que fueran cadenas: **toda ruta nombrada dentro de una lista era una ruta
que nadie revisaba** — que es exactamente cómo `thalyx_edit` nombra sus
archivos. Lo cachó la columna de control de su propia prueba, no la prueba.
Regla 5 otra vez, y la regla nueva que sale de él está en
[[Estrategia-de-Pruebas]].

El regrade ahora incluye el scope como conjunto: un brazo fuera de su frontera
es `INVALID`, uno cuya frontera nadie puede comprobar es `NOT PROVEN`.

### El regrade de esta corrida, que falta correr

Sobre los artefactos que ya existen, sin llamar a Claude y sin gastar nada:

```sh
dev/bench-external-agent.sh --task reversible --symbol UidRegistry \
    --out ~/thalyx/target/bench-external-agent-3 --regrade
```

Escribe `summary-regraded.json` y **no toca** `summary.json`. Imprime una línea
por brazo con `VALID` / `INVALID` / `NOT PROVEN`, la frontera bajo la que lo
decidió y por qué.

Si los dos salen `VALID`, esta corrida es la primera comparación
metodológicamente limpia después del endurecimiento y después del lote: los dos
brazos anclados, la parity comprobada, el preflight comprobado, el restore
comprobado por fuera, y cada brazo juzgado bajo su propia frontera. Hasta que se
corra, es **PENDIENTE**, y esta nota no la cuenta como resultado.

### Una herramienta por mensaje: qué es de Thalyx y qué no

El dato más raro de la corrida es ese renglón: el brazo A llegó a emitir **seis**
llamadas en un mismo mensaje del asistente y el brazo B nunca pasó de **una**.
Lo que se determinó, y de dónde:

**No es una limitación del cliente.** Claude Code (CLI 2.1.251) emite dos
llamadas a herramientas MCP en un solo mensaje del asistente y las dos se
ejecutan; se comprobó en vivo, en una sesión aparte, sin costo de banco. Así que
«el modelo no puede paralelizar MCP» es falso y no explica nada.

**Sí hay una serialización, y es de Thalyx, pero es de ejecución y no de
emisión.** `crates/thalyx-mcp/src/main.rs` sirve así:

```rust
for line in stdin.lock().lines() { … handle(…) … writeln!(stdout, …) }
```

Un mensaje, el viaje completo al socket de la máquina, la respuesta, y hasta
entonces el siguiente. Aunque el cliente mandara N llamadas a la vez, se
ejecutarían en fila. Eso no cambia cuántas emite el modelo; cambia lo que
cuestan.

**Y ahí es donde encaja el reloj.** El brazo B gastó 57.696 s de pared contra
50.964 s de API: **6.7 s fuera de la API**. El brazo A gastó 50.500 contra
50.392: 0.1 s. Esos casi siete segundos son las catorce llamadas cruzando el
adaptador y el socket hacia adentro de la máquina, una tras otra. Es una
**hipótesis sobre la causa del delta de reloj**, no una medición de ella: lo que
está medido es la diferencia entre pared y API en los dos brazos.

**Por qué las cinco `thalyx_edit` no eran paralelizables de todas formas.** El
resto de la traza es causalmente dependiente por diseño y ese diseño es correcto:
el `attempt` tiene que estar abierto antes de que cambie nada, y el `abandon`
tiene que ser después. Las cinco sustituciones sí eran independientes entre sí —
pero cinco llamadas paralelas seguirían siendo cinco viajes por un adaptador que
los hace en fila, y **una sola llamada es mejor que cinco paralelas**: un viaje,
un preflight completo, una escritura por archivo. Por eso el cambio 2 es el lote
y no concurrencia en el adaptador.

**La frontera, dicha de una vez.** La métrica que importa dejó de ser «¿puede el
modelo emitir seis llamadas MCP a la vez?» —puede— y es «¿puede el mismo plan
expresarse en una a tres llamadas?». Hacer el adaptador concurrente exigiría
encauzar peticiones por el puente y una máquina que sirva verbos en paralelo:
es un cambio de protocolo, no una optimización local, y no compraría nada en
esta traza.

### `attempt abandon`, que sigue costando dos llamadas y se queda así

La traza gasta dos: `abandon`, «needs confirmation», `abandon confirm=true`. Se
revisó si esa confirmación es una propiedad de seguridad real o UX humana
reciclada, y **es real**, escrita en `crates/thalyx-cli/src/attempt.rs`:
abandonar **reemplaza el árbol**, y el árbol es **compartido**. La persona pudo
haber escrito en él mientras el intento estaba abierto, y su trabajo no es del
agente para tirarlo porque el agente cambió de opinión. Lo que se destruye no
tiene otro snapshot que lo recupere. Por eso los dos rostros ven primero
exactamente qué se perdería.

No es que la máquina lo imponga —`confirm: true` en la primera llamada pasa
derecho hoy—, sino que la descripción enseña el camino que hace ver el costo
antes. Quitarlo del canal del agente sería debilitar la única protección que hay
sobre el trabajo de alguien más, y eso **no se cambia sin que Cesar lo apruebe**:
es un decreto de [[Camino-Confiable]] y no una decisión de implementación.

### `changed` y las validaciones posteriores

Se buscó en la traza un viaje evitable de este tipo y **no lo hay**: el agente
nunca llamó `thalyx_changed`, porque la respuesta de `sustituir` ya trae por
archivo cuántos lugares, en cuántas líneas, desde cuál y cuántos bytes tiene
ahora. La respuesta del lote conserva eso y agrega el desglose por patrón.

El `find` que sigue al abandon **no se toca**: es el agente comprobando por su
cuenta que el árbol volvió, que es el último paso de la tarea. Ahorrárselo
haciendo que Thalyx afirme su propio éxito es exactamente lo que la regla 2 de
[[Estrategia-de-Pruebas]] prohíbe.

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
- **`sustituir` tiene una medición y `sustituir-lote` no tiene ninguna.**
  REVERSIBLE #2 muestra el brazo B bajando de 36 llamadas a 14 y de 16
  mutaciones a 5 después de `sustituir` — una observación de una corrida, no una
  propiedad medida. `sustituir-lote` existe porque esa misma corrida identificó
  cinco llamadas que eran un solo plan; que eso mueva costo o reloj es una
  hipótesis sin comprobar hasta que el banco se vuelva a correr.
- **REVERSIBLE #2 no tiene veredicto todavía.** Sus artefactos existen y su
  regrade con el instrumento corregido no se ha corrido. Hasta entonces es
  PENDIENTE, y sus cifras son cifras de esa corrida y no un resultado de la
  comparación.
- **El delta de reloj de REVERSIBLE #2 —+14.2 %— tiene una hipótesis y no una
  causa medida.** Lo medido es que el brazo B gastó 6.7 s fuera de la API y el
  brazo A 0.1 s; que eso sean los viajes por el adaptador serializado es la
  explicación más simple y no está comprobada.
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
