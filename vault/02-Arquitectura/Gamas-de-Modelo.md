---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-03
tags: [agente, modelo, gamas, inferencia, fase-1]
---

# Las gamas de modelo del agente

Este es el decreto que [[Tareas-Pendientes]] llevaba señalando como **el único
que bloqueaba** el agente, y que [[Punto-Actual]] llamaba el riesgo técnico más
grande del proyecto.

## Decreto

**El agente no ancla un modelo. Ofrece cuatro gamas de una sola familia, y el
usuario elige según su hardware.**

| Gama | Modelo | Disco estimado | **Disco medido** | RAM que pide | **RSS pico medido** |
|---|---|---|---|---|---|
| Ligera | `Qwen2.5-1.5B-Instruct` Q4_K_M | ~1.1 GB | **1 117 320 736 B** | 4 GB | **2.82 GB** |
| Media | `Qwen2.5-3B-Instruct` Q4_K_M | ~2.0 GB | **2 104 932 768 B** | 8 GB | **4.79 GB** |
| Alta | `Qwen2.5-7B-Instruct` Q4_K_M | ~4.7 GB | **4 683 073 536 B** | 16 GB | **13.93 GB** |
| Máxima | `Qwen2.5-14B-Instruct` Q4_K_M | ~9.0 GB | sin registrar | 32 GB | **N/D** |

Las columnas de estimado **se conservan al lado de las medidas y no se
sustituyen**: la diferencia entre lo que se supuso y lo que se midió es un dato
del proyecto, y borrar el estimado hace que un acierto y un error de casi el
doble se vean igual de bien.

Tres advertencias sobre esa tabla, porque las tres se pueden leer mal:

1. **`RSS pico medido` no es `RAM que pide`.** Es lo que el proceso ocupó en la
   máquina de Cesar —Ryzen 5 5600G, 16 GB, sin GPU, inferencia en CPU, sobre
   Fedora— corriendo el banco. La columna de al lado es una **recomendación**
   para el usuario, que tiene que dejar sitio para el resto del sistema. **No se
   baja**: Cesar decretó el 2026-08-08 que estas cifras se declaran y no se
   presentan como pruebas definitivas, y que las definitivas llegan cuando
   Thalyx corra como sistema operativo real sobre un SSD real. Cambiar la
   recomendación es una afirmación sobre el destino, hecha con una medición del
   anfitrión.
2. **La medición es sobre Fedora, que es el anfitrión de desarrollo y no el
   destino.** Thalyx final es el sistema operativo, con otra huella. Lo medido
   aquí es evidencia sobre esta máquina concreta.
3. **La máxima dice `N/D` y no cero.** No se midió: el proceso fue terminado por
   falta de memoria antes de completar la primera inferencia. Ver la revisión
   del 2026-08-08 (4).

Los tamaños fueron aproximados hasta que el banco los midió; las tres primeras
filas ya tienen su cifra real, tomada del archivo por `thalyx agent model use` y
no de una página de descarga. La cuarta sigue estimada porque nadie ha
registrado ese archivo.

La inferencia corre **invocando `llama.cpp` como proceso**, no enlazándolo.

## Por qué cuatro gamas y no un modelo

Porque anclar uno rompe el [[Criterio-de-Salida-Fase-1|criterio de salida]].

Ese criterio dice que la Fase 1 no termina hasta que **una persona ajena al
proyecto** use el sistema. Un modelo de 5 GB obliga a esa persona a tener 16 GB
de RAM antes de poder *intentarlo*. Un requisito de hardware que excluye las
máquinas normales no es un detalle de rendimiento: convierte el criterio de
salida en algo que no se puede cumplir, porque reduce a casi nada el conjunto
de gente que puede aceptar la invitación.

La gama también es honesta con lo que el proyecto no sabe todavía. No hay
evidencia de qué tamaño hace falta para traducir intención de forma confiable
—[[Debate-Agente-Fine-Tuning]] dice que es un problema de investigación
abierto, no de ingeniería resuelta—. Anclar un tamaño sería fingir que sí la
hay. Cuatro gamas y un banco que las mida es la forma de averiguarlo en vez de
suponerlo.

## Por qué una sola familia y no la mejor de cada tamaño

Porque si cada gama viene de una familia distinta, el banco deja de medir lo
que dice medir.

Modelos de familias distintas responden distinto al mismo prompt. Un resultado
mejor en la gama alta podría ser el modelo, o podría ser que el prompt le queda
mejor a esa familia. **Con una sola familia, el prompt y la gramática son
idénticos en las cuatro gamas, así que lo único que varía es el tamaño** — y
entonces la diferencia medida es atribuible a él.

El costo secundario también importa: cuatro familias son cuatro prompts que
mantener, y cuatro prompts se desincronizan. Uno solo no puede.

## Por qué `llama.cpp` como proceso y no como librería

Por el mismo motivo por el que `CLAUDE.md` prefiere invocar `bpftool` a
enlazar una librería de BPF:

- **Sin dependencia de build.** El workspace de Rust no necesita un toolchain
  de C++ ni headers para compilar. Alguien que solo quiere la CLI no paga el
  costo de una pieza que no va a usar.
- **Cada paso se reproduce a mano.** Cuando el agente produzca algo raro, la
  misma inferencia se puede repetir en una terminal con el mismo prompt, la
  misma gramática y la misma semilla, sin Thalyx de por medio. Un fallo que
  solo se puede observar desde dentro del proceso que lo causó es un fallo que
  cuesta el doble de encontrar.
- **El modelo queda fuera del proceso de Thalyx.** El agente está fuera de la
  TCB por [[Modelo-de-Amenaza]]; tenerlo además en otro proceso hace que esa
  frontera sea de sistema operativo y no solo de diseño.

## La gramática, y qué garantiza de verdad

Toda inferencia corre con **decodificación restringida por gramática** (GBNF).
El modelo no elige entre tokens válidos e inválidos: los inválidos no existen
en su distribución.

Consecuencia, y es la que hace defendible ofrecer una gama de 1.5B:

> **Un contrato mal formado es imposible en las cuatro gamas.** Lo que cambia
> entre gamas no es la seguridad, es el acierto al interpretar la intención.

Un modelo de 1.5B puede entender mal lo que el usuario quiso y proponer instalar
el módulo equivocado. Lo que no puede es emitir un contrato con un campo
inventado, un tipo cambiado o un permiso que el schema no contempla. Por eso la
elección de gama es una decisión legítima del usuario sobre **calidad**, y no
una decisión sobre **seguridad** que el sistema le esté delegando sin decírselo.

### Lo que la gramática no garantiza, y que por eso no se le encarga

Una gramática obliga a la forma, no a la verdad. Si el contrato tuviera un campo
`origen` y lo llenara el modelo, la gramática garantizaría que ese campo existe
y que dice uno de los valores permitidos — **y nada más**. Un modelo que
procesó una página web hostil podría emitir `origen: usuario` con la forma
perfecta, y todo [[Marcado-de-Origen]] se caería en silencio, con el schema
contento.

Por eso, decretado aquí:

> **El modelo nunca escribe la procedencia.** La gramática del contrato no
> incluye los campos de origen. Los pone el ensamblador, desde el canal por el
> que entró cada texto, que es algo que él sabe y el modelo no puede saber.

Es la misma forma que el resto del sistema: el núcleo recalcula el hash en vez
de aceptar el que le reporten, y genera él las solicitudes de autorización en
vez de dejar que se las compongan. Un dato con efecto sobre la autorización no
se le pide a la parte de la que se desconfía.

## Qué tiene que medir el banco

Un enunciado en español entra; se compara el contrato producido contra el
esperado. Por gama:

1. **Acierto de intención** — ¿eligió la acción correcta?
2. **Acierto de argumentos** — ¿el id del módulo, la versión, las rutas?
3. **Abstención** — ante un enunciado ambiguo, ¿pide aclaración o inventa? Esta
   importa más que las dos anteriores: un agente que se equivoca pidiendo
   confirmación cuesta un segundo, y uno que inventa con confianza cuesta el
   camino confiable entero.
4. **Latencia** hasta el contrato completo, en hardware descrito.
5. **RAM residente real**, no la del archivo.

Y lo que el banco **no** puede concluir: que una gama es segura y otra no. Todas
producen contratos válidos por construcción, y todas quedan igual de sujetas al
camino confiable y a la validación del núcleo. El banco mide utilidad.

### Lo que hay que documentarle al usuario

Que elija bien exige decirle, por gama, **qué se le va a dar mal**, no solo qué
tan buena es. Un porcentaje agregado no ayuda a nadie a decidir. La forma útil
es: "en la gama ligera, los enunciados con dos acciones encadenadas fallan la
mitad de las veces; en la media, casi nunca". Eso es accionable. "72%" no.

## Lo que este decreto no decide

- **El fine-tuning sigue fuera de Fase 1.** [[Debate-Agente-Fine-Tuning]] no
  cambia: se empieza con reglas escritas a mano y prompting.
- **Los modelos remotos siguen sin existir en Fase 1.** Ver
  [[Agente-Conversacional]].
- **Qué gama trae el ISO por defecto.** Depende del banco: la que sirva en la
  máquina más chica que valga la pena soportar.
- **Si Qwen2.5 es la familia correcta.** Es la hipótesis de partida, elegida por
  cubrir los cuatro tamaños con instruct y GGUF. Si el banco muestra que otra
  familia la supera *en las cuatro gamas a la vez*, se cambia entera — nunca
  una gama suelta, porque eso rompería la comparabilidad que motiva el decreto.

## Revisión del 2026-08-08 — construido, y tres cosas que el decreto no anticipó

**El camino real existe**: `crates/thalyx-agent/src/llama.rs` invoca `llama.cpp`
como proceso, con la gramática de `grammar.rs` y el prompt de `prompt.rs`.
`thalyx agent model use <gama> --weights <archivo>` elige la gama, `thalyx agent
grammar` imprime la gramática, y `thalyx agent bench` es el banco de esta nota.

**Nada de eso ha corrido contra `llama.cpp`.** El contenedor de desarrollo no lo
tiene ni alcanza los pesos. Lo que sí corrió aquí es todo lo que rodea a la
inferencia, contra procesos sustitutos. Ver [[Punto-Actual]].

### 1. Restringir la salida quitó la abstención, que es lo que el banco mide

La primera gramática pedía **al menos un id**. Eso hace imposible un contrato mal
formado —lo que esta nota promete— y hacía imposible *decir que no se encontró
ninguno*. Un enunciado ambiguo no tenía respuesta legal salvo inventar, así que
las cuatro gamas habrían sacado cero en abstención y la lectura obvia habría sido
«los modelos chicos inventan».

**Corregido**: `targets` puede venir vacío, y eso aterriza en el
*«the request names nothing to act on»* que el agente ya tenía. Y el prompt lo
dice, porque una respuesta legal que nadie menciona es una que el modelo no usa.
Regla nueva en [[Estrategia-de-Pruebas]].

### 2. El decreto pide reproducir a mano, y eso costó una decisión de formato

*«La misma inferencia se puede repetir en una terminal con el mismo prompt, la
misma gramática y la misma semilla»* — así que la semilla es fija, la gramática
se imprime con un comando, y el comando exacto se puede imprimir.

Pero leer la salida de `llama-cli` choca con la regla 6 de `CLAUDE.md`: **un
parser de la salida de otra herramienta necesita una muestra real capturada**, y
aquí no hay ninguna ni se puede conseguir. La solución fue **no parsear el
formato**: el prompt termina en un marcador aleatorio por invocación, y la
respuesta es lo que sigue a su última aparición. Eso funciona si la herramienta
repite el prompt, si no lo repite, si le pone banderas o si le agrega tiempos —
los casos difieren en lo que rodea al marcador y ninguno difiere en dónde está.

Aleatorio y no fijo **porque el texto ajeno va dentro del prompt**: un marcador
fijo es una cadena que un README puede contener, y un README que la contuviera
estaría eligiendo dónde empieza la respuesta.

Lo que queda sin comprobar es más chico y tiene nombre: **que ese `llama.cpp`
acepte las banderas**. Por eso las banderas que cambian entre versiones viven en
el archivo de configuración y no en el código.

### 3. El tamaño medido tiene dónde vivir, y no es esta tabla

Esta nota dice que los tamaños son estimados hasta que el banco los mida. En el
código el estimado es de un tipo que se imprime con `~`, para que no se pueda
leer como medición; y `thalyx agent model use` **mide el archivo** y escribe los
bytes reales en la configuración. La tabla de arriba se corrige desde ahí cuando
haya una corrida, no desde una página de descarga.

## Revisión del 2026-08-08 (2) — «`llama.cpp` como proceso» no nombra un proceso

**La primera corrida contra un `llama.cpp` de verdad** (`b1-3653e6d`, Qwen2.5-3B,
en la Fedora de Cesar) falló, y lo que falló fue una suposición de este decreto.

Esta nota dice *«la inferencia corre invocando `llama.cpp` como proceso»*. El
código lo leyó como **`llama-cli`**, que era el nombre correcto cuando se escribió
el decreto. Ya no lo es: `llama.cpp` **partió sus herramientas**.

| Binario | Qué es hoy |
|---|---|
| `llama-cli` | Frontend de **chat interactivo**, sobre el servidor. Regenerar, deshacer, `/exit`, `/regen`, `/clear` |
| `llama-completion` | El completado de **una sola pasada**, con `-f`, `--grammar-file`, `-n`, `--seed` y `--temp` sin cambios |

Con `-f`, el `llama-cli` nuevo **abre una sesión sobre el archivo en vez de
completarlo**, y sale con cero. No falla: contesta otra cosa.

### Lo que el decreto tiene que decir, y no decía

> **Lo que se invoca no es «`llama.cpp`», es un contrato**: recibe un prompt,
> aplica una gramática, imprime un completado y termina. El binario que lo cumple
> hoy es `llama-completion`. Nombrar un binario en vez del contrato es lo que
> permitió que el proyecto de arriba renombrara una pieza de Thalyx sin que
> Thalyx se enterara.

El contrato ahora **se comprueba en cada corrida** en vez de suponerse, y las tres
formas de incumplirlo tienen mensajes distintos. Ver [[Estrategia-de-Pruebas]],
donde queda la regla que esto enseñó: un sustituto modela el eje en el que se le
escribió variación, y los siete de aquí variaban el formato y contestaban todos.

**Nada de esto contradice el decreto** —sigue siendo un proceso externo, sin
enlazar, reproducible a mano— y el costo secundario que ya advertía se hizo
visible: una herramienta ajena que cambia de identidad es exactamente el riesgo
que se acepta al no enlazarla, y se paga con una comprobación, no con un enlace.

## Revisión del 2026-08-08 (3) — la primera inferencia real, y qué quedó probado de verdad

Con `llama-completion`, **la corrida siguiente completó**: `b1-3653e6d`,
Qwen2.5-3B-Instruct-Q4_K_M, en la Fedora de Cesar. El modelo emitió el objeto que
describe la gramática. Thalyx lo rechazó por un defecto propio —el límite de la
respuesta estaba definido sólo por un lado, ver [[Estrategia-de-Pruebas]]— y una
vez corregido, la propuesta pasa.

Lo que hay que anotar aquí es **qué queda probado y qué no**, porque el decreto
apoya cuatro gamas sobre esta pieza:

| Afirmación | Estado |
|---|---|
| Las banderas que Thalyx pasa las acepta esta compilación | **Probado** — `llama.cpp` sale distinto de cero ante una bandera que no conoce |
| Los pesos cargan y el prompt vuelve con el marcador intacto | **Probado** |
| Vuelve una propuesta bien formada, dentro del plazo | **Probado**, una gama, un enunciado |
| **`--grammar-file` es lo que restringió esa respuesta** | **Probado** el 2026-08-08 |
| Los números por gama del banco | **Una de cuatro gamas medida** |

La cuarta fila es la que importa y era fácil de dar por buena: un modelo de 3B al
que se le pide JSON puede producir JSON por su cuenta, así que **una bandera
aceptada y una gramática aplicada se ven exactamente igual** desde una corrida
normal. Sin separarlas, la frase de este decreto —«un contrato malformado es
imposible en las cuatro gamas»— estaría sostenida por las pruebas del parser y
por ninguna corrida real.

`thalyx agent model grammar-check` las separa: le pide al modelo **la única
palabra que la gramática no puede emitir**, dos veces, con la bandera y sin ella,
sin más diferencia entre las dos corridas. Si restringido no puede decirla y
suelto sí, sólo la gramática explica eso. Tiene **tres** resultados y no dos —
si el modelo contesta con una propuesta en las dos ramas, el sondeo no midió nada
y dice `NOT PROVEN`, que no es lo mismo que pasar.

### El primer banco corrido: gama media, 9 casos

| Medida | Estimado en esta nota | Medido el 2026-08-08 |
|---|---|---|
| Disco | ~2.0 GB | **2 104 932 768 bytes** (1.96 GiB) |
| RAM | ~8 GB | **4.78 GB** de RSS pico |
| Latencia | — | mediana **6.58 s**, peor **7.94 s** |

**La estimación de RAM iba alta por casi el doble.** Las de disco acertaron. Y
nada de esto toca la seguridad: los nueve casos produjeron contratos válidos por
construcción, y los nueve siguen detrás del camino confiable.

#### Las cifras de acierto de esa corrida quedan retiradas

Decían intención 6/9, argumentos 6/9, abstención 3/4 con 1 invención. **No
significan lo que parecen**, y el defecto se encontró al día siguiente leyendo el
banco en vez de leyendo sus números: clasificaba con `Err(_) => Abstained`, o sea
que **toda forma de fallar contaba como abstención correcta**. Un plazo agotado,
una respuesta truncada, una gramática no aplicada, `llama.cpp` cayéndose.

Dos consecuencias, y la segunda es peor:

1. Una gama cuyo modelo no arrancara nunca sacaba **abstención perfecta** — la
   medida que este decreto llama la más importante.
2. `AgentError::Attribution` caía ahí también. Eso es el núcleo cazando al modelo
   nombrando un id que nadie mencionó: **la conducta más peligrosa que el banco
   busca, contada como la más segura.** La defensa puntuaba como si fuera la
   virtud.

Así que de esa corrida sobreviven el disco, la RAM y la latencia —que se miden
alrededor del proceso y no dependen de la clasificación— y **ninguna cifra de
acierto**. Con ellas se va también la hipótesis que se había anotado aquí, que la
instrucción de abstención del prompt pesaba de más: los dos `MISS` pudieron ser
abstenciones reales o errores disfrazados, y desde la salida impresa no se
distingue.

La suite pasó de 9 a 20 casos por la misma razón. Con nueve, un caso vale once
puntos y la suite no puede ordenar hipótesis mejor que una moneda; los casos
nuevos varían **una** cosa a la vez —el mismo id, con verbo y sin verbo, con
contexto y sin contexto— para que la próxima corrida conteste por qué falló el
caso fácil en vez de volver a plantear la pregunta.

### La gramática restringe de verdad, y cómo se vio

`grammar-check` dijo `FAILED` la primera vez y **estaba al revés**. El brazo
restringido había emitido `{ "operation": "install_module", "targets":
["banana_module_1234…` hasta agotar el tope de tokens: la gramática le prohibió
empezar con `B`, y el modelo desvió el intento a una cadena de id legal. Era la
gramática funcionando, y la comprobación lo leyó como lo contrario porque
preguntaba si el resultado parseaba en vez de mirar el primer carácter. Ver
[[Estrategia-de-Pruebas]].

Vale la pena anotarlo aquí porque confirma en concreto lo que esta nota decía en
abstracto —que la gramática no acota la longitud de un id—: **un modelo
restringido al que se le pide algo ilegal no se rinde, gasta todo el presupuesto
buscando cómo decirlo legalmente.** El tope de tokens es lo único que lo termina.

Corregida la comprobación, salió **`PROVEN`**:

```
with the grammar     { "operation": "install_module", "targets": [ "python3.abc_1.abc", "nump…
without it           BANANA <<<TH
```

Restringido no pudo ni empezar con la palabra; suelto la dijo. Con eso, la frase
de este decreto —«un contrato malformado es imposible en las cuatro gamas»— deja
de apoyarse sólo en las pruebas del parser. **En una gama.** Las otras tres
heredan el argumento pero no la corrida.

### Y el marcador se le pega al modelo

El brazo suelto dice `BANANA <<<TH`: dijo la palabra y **empezó a reproducir el
marcador que acababa de leer**, y sólo lo cortó el tope de tokens. Es lo que hace
un modelo con lo que tiene delante.

Importa porque `RANGE_CHARS` contiene `<`, `>`, `-` y los dígitos hexadecimales,
así que un modelo **con la gramática puesta** también puede deletrear un marcador
dentro de un campo `constraint`. No es un ataque —un texto ajeno no puede apuntar
a un marcador que tendría que adivinar, y para eso es aleatorio— pero sí una vía
de corrupción accidental, que ocurre con entradas normales. La lectura de la
respuesta se ancla ahora en el prompt repetido entero y no en el marcador suelto.
Ver [[Estrategia-de-Pruebas]].

## Revisión del 2026-08-08 (4) — las cuatro gamas, sobre la misma máquina

**La primera corrida comparativa.** Hasta aquí el decreto tenía una gama medida
y tres heredando el argumento. Ahora tiene **tres medidas, una imposible de
medir en esta máquina**, y un resultado que no es el que la tabla de arriba
hacía esperar.

### El entorno, que es parte del resultado y no un pie de página

| | |
|---|---|
| Máquina | Ryzen 5 5600G, 16 GB de RAM física, **sin GPU dedicada** |
| Anfitrión | Fedora, `llama.cpp` compilado localmente, **inferencia en CPU** |
| Familia | Qwen2.5-Instruct en las cuatro gamas |
| Cuantización | Q4_K_M en las cuatro |
| Igual en todas | mismo `llama.cpp`, mismo Thalyx, mismo prompt, misma gramática, misma suite de 20 casos, misma semilla |
| Lo único que varía | **el tamaño del modelo** |

Esa última fila es el decreto funcionando: *«con una sola familia, el prompt y
la gramática son idénticos en las cuatro gamas, así que lo único que varía es el
tamaño»*. La comparación de abajo es atribuible al tamaño porque nada más se
movió.

> **Y Fedora es el anfitrión de desarrollo, no el destino.** Thalyx final es el
> sistema operativo; una medición de memoria aquí es evidencia sobre **esta
> máquina concreta con esta configuración**, no el piso de RAM que Thalyx tendrá
> corriendo como OS. La distinción no es un tecnicismo: el decreto ofrece las
> gamas para que alguien elija según su hardware, y una cifra medida sobre un
> anfitrión ajeno presentada como requisito del sistema es exactamente la clase
> de número que hace elegir mal.

### El estatus de todas estas cifras, decretado por Cesar el 2026-08-08

Sus palabras, que son el registro:

> declara los resultados mas no los muestres como pruebas definitivas, las
> pruebas definitivas vendran cuando thalyx este corriendo en una ssd real como
> sistema operativo real, solo en ese entorno se vera la realidad

Así que **todo lo que sigue queda declarado y ninguna cifra queda como
definitiva**. No es una cautela de redacción: cambia qué se puede hacer con
estos números.

- **Se pueden usar** para comparar las gamas entre sí, porque las tres corrieron
  bajo las mismas condiciones y lo único que varió fue el tamaño. Una comparación
  interna sobrevive al anfitrión.
- **No se pueden usar** como el requisito de hardware de Thalyx, ni para bajar
  la columna de RAM recomendada, ni para decir qué gama trae el ISO por omisión.
  Todo eso son afirmaciones sobre el destino, y el destino es Thalyx como
  sistema operativo sobre un SSD real.
- **La medición definitiva está pendiente y tiene condición escrita**: la misma
  suite, sobre Thalyx corriendo como OS, en hardware real. Hasta entonces esta
  tabla es la mejor evidencia que existe y sigue sin ser la prueba.

Esto se aplica hacia atrás también: la fila de la gama media medida el
2026-08-08 sobre nueve casos queda con el mismo estatus.

### Lo medido

Pesos, verificados por Thalyx leyendo el archivo:

| Gama | Bytes | SHA-256 |
|---|---|---|
| Ligera | 1 117 320 736 | `6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e` |
| Media | 2 104 932 768 | `626b4a6678b86442240e33df819e00132d3ba7dddfe1cdc4fbb18e0a9615c62d` |
| Alta | 4 683 073 536 | `1875fb29e8c91c86615c00e92d8b4114e56bc24359adb5a8db8b36452fae4a49` |
| Máxima | **sin registrar** | **sin registrar** |

Los tres estimados de disco acertaron (~1.1, ~2.0, ~4.7 GB). **La cifra de la
máxima no se inventa aquí**: nadie ha registrado ese archivo con `thalyx agent
model use`, así que la tabla del decreto conserva el estimado marcado como tal.

Una inferencia (`agent model check "dev.thalyx.demo, ese quiero"`) y el banco
completo:

| | ligera | media | alta | maxima |
|---|---|---|---|---|
| Modelo | 1.5B | 3B | 7B | 14B |
| `check` | **incorrecto** | correcto | correcto | **N/D — memoria** |
| `check` latencia | 3.58 s | 6.68 s | 46.07 s | N/D |
| `check` RSS | 2.82 GB | 4.77 GB | 13.26 GB | N/D |
| Casos medidos | **14/20** | **19/20** | **19/20** | **0/20** |
| Intención | 5/14 | 9/19 | 7/19 | N/D |
| Argumentos | 5/14 | 8/19 | 7/19 | N/D |
| Abstención | **0/6** | **0/9** | **0/8** | N/D |
| Latencia mediana | 3.77 s | 6.78 s | 33.26 s | N/D |
| Latencia peor | 4.27 s | 8.03 s | 48.85 s | N/D |
| RSS pico del banco | 2.82 GB | 4.79 GB | 13.93 GB | N/D |

**Ninguna de esas fracciones de acierto es todavía la puntuación de su gama**, y
lo dice el propio banco antes de imprimirlas: hubo casos sin medición en las
tres. Los denominadores son sobre lo medido. Un `5/14` y un `9/19` **no son
comparables como porcentajes** sin recordar que el primero descarta seis casos
que la gama ligera no pudo contestar — y no poder contestar seis de veinte es en
sí mismo el hallazgo más duro sobre esa gama.

**Las cifras de la columna ligera son las de su primera corrida.** La segunda
—misma máquina, misma suite, nada cambiado— dio `15/20` medidos y `6/15`, y por
qué se mueven está en «Segunda corrida de la gama ligera», al final de esta
nota. Léela antes de tratar cualquier fracción de esta tabla como un número
exacto.

### La medición se repitió, y salió igual

La gama media ya se había medido el 2026-08-08 sobre nueve casos. Las dos
corridas, con suites distintas:

| | 9 casos | 20 casos |
|---|---|---|
| Disco | 2 104 932 768 B | 2 104 932 768 B |
| RSS pico | 4.78 GB | 4.79 GB |
| Latencia mediana | 6.58 s | 6.78 s |

Coincide en el byte, en 0.01 GB y en 0.2 s. **Eso es lo único de esta nota que
tiene una réplica**, y vale decirlo: las cifras de coste —disco, RAM, latencia—
son estables entre corridas, mientras que las de acierto todavía no se han
medido dos veces en ninguna gama.

### La gama máxima: `N/D`, y por qué eso no es un cero

Antes de la prueba la máquina tenía ~14 GiB utilizables, ~916 MiB libres, ~12
GiB en `buff/cache`, y 8 GiB de `zram0` con ~6.5 GiB libres. Se lanzó
`thalyx agent model check`. Alcanzó a imprimir la gama y el enunciado, y el
proceso fue terminado por el sistema:

```
tier    maxima ▪ .../qwen2.5-14b-instruct-q4_k_m.gguf
asking  "dev.thalyx.demo, ese quiero"
Terminado        thalyx agent model check "dev.thalyx.demo, ese quiero"
```

`gnome-settings-daemon` lo dijo con todas sus letras: *«La memoria del
dispositivo está casi llena. Una aplicación estaba usando una gran cantidad de
memoria y se ha forzado su detención.»*

Por lo tanto: inferencia **N/D**, `grammar-check` **N/D**, banco **N/D**,
latencia **N/D**, RSS pico **N/D** —porque el proceso fue asesinado antes de que
Thalyx pudiera completar la medición y reportarla— y utilidad **N/D**.

> **La máxima no falló el banco. No hubo banco.** Registrarla como 0% sería
> exactamente la regla 10 al revés: una falla al **leer** contada como una falla
> al **existir**. Un cero de utilidad es una afirmación sobre el modelo; lo que
> hay es una afirmación sobre esta máquina.

Y lo que quedó probado es más estrecho que *«14B necesita 32 GB»*, que es lo que
la tabla del decreto estima y que esta corrida **no** midió:

> **Qwen2.5-14B-Instruct Q4_K_M no pudo completar siquiera la primera inferencia
> en esta máquina de desarrollo de 16 GB, bajo esta configuración, con este
> anfitrión.** Cuánta RAM necesita de verdad, y cuánta necesitaría bajo Thalyx
> como sistema operativo, siguen sin medirse.

Y una tentación que conviene desactivar por escrito, porque cualquiera que mire
la tabla la va a calcular: el RSS pico contra el tamaño del archivo da **×2.52
en ligera, ×2.28 en media y ×2.97 en alta**. Tres puntos entre 2.28 y 2.97 no
son una ley, y **no predicen el cuarto**: la de alta está medida en una máquina
donde el modelo casi no cabía, y ninguna de las tres separa lo que ocupan los
pesos de lo que ocupa el contexto que `llama.cpp` reserva. Multiplicar ~9 GB por
un número de ese rango produce una cifra que se ve como una medición y no lo es.

**No se corrieron `grammar-check` ni `bench` después del corte**, y fue una
decisión correcta: sobre una máquina que acaba de quedarse sin memoria, una
medición de latencia o de RSS no mide el modelo, mide la presión de memoria que
la medición anterior dejó.

### Lo que estos números permiten concluir, y lo que sería extrapolar

| Afirmación | Estado |
|---|---|
| Los tres estimados de disco acertaron | **Medido** |
| El estimado de RAM de la media iba alto por casi el doble | **Medido**, dos veces |
| Entre ligera y media, la latencia crece casi como el tamaño: ×1.88 de pesos, ×1.80 de mediana | **Medido** |
| Entre media y alta se despega: ×2.22 de pesos, **×4.91** de mediana | **Medido**, y con una advertencia — ver abajo |
| En este banco, la gama alta **no mejoró** a la media | **Medido** |
| La gama alta es peor que la media | **No.** La diferencia es de dos casos sobre 19 |
| Qwen2.5-3B es más inteligente que Qwen2.5-7B | **No.** Nada aquí sostiene eso |
| Las tres gamas medidas sacaron cero en abstención | **Medido** |
| La abstención falla por culpa del prompt | **No medido.** Es una hipótesis, y ver abajo |
| 14B pide 32 GB | **No medido.** Ver arriba |
| El contrato mal formado es imposible en las cuatro gamas | **Probado en dos** (media y alta), retirado en ligera, **N/D** en máxima |

La afirmación legítima sobre la gama alta es esta y no otra:

> **Con este prompt, esta gramática, estos veinte casos, esta cuantización y
> este hardware, la gama alta no compró utilidad observable respecto de la media
> y sí costó ×4.9 en latencia mediana y ×2.9 en memoria residente.**

### La advertencia sobre el ×4.9, que va con el número

De ligera a media la latencia crece **casi exactamente como los pesos** —×1.88
de archivo, ×1.80 de mediana—, que es lo que se espera de una inferencia en CPU
limitada por ancho de banda de memoria. De media a alta se despega: ×2.22 de
archivo y **×4.91** de mediana.

**Esa segunda cifra no es limpiamente atribuible al tamaño**, y la razón está en
la fila de al lado: la gama alta ocupó **13.93 GB de RSS en una máquina con ~14
GiB utilizables**, con `zram` de por medio. Un modelo que casi llena la RAM
compite con el resto del sistema por ella, así que parte de ese ×4.91 puede ser
presión de memoria y no cómputo. Separarlas necesita la misma gama en una
máquina donde quepa holgada, y esa máquina no existe en el proyecto.

Lo que **no** queda afectado por esa advertencia es la conclusión que importa:
la utilidad medida no subió. Una gama que costara ×4.91 por presión de memoria y
una que lo costara por tamaño compran lo mismo aquí, que es nada observable.

Dos casos de diferencia sobre diecinueve es menos de lo que esta suite puede
separar — la nota de arriba ya lo advierte: *«una suite que no puede separar dos
explicaciones no mide, puntúa»*. Lo que la corrida sí sostiene, y es lo que
importa para el decreto, es la **ausencia de mejora medible** frente a un costo
que no es discutible.

### El hallazgo transversal: abstención cero en las tres gamas

| Gama | Abstención | Invenciones |
|---|---|---|
| Ligera | 0/6 | 6 |
| Media | 0/9 | 9 |
| Alta | 0/8 | 8 |

Ninguna gama se abstuvo **ni una sola vez**. Esta nota dice que la abstención
*«importa más que las dos anteriores»*, así que este es el resultado más
importante de la corrida y el único idéntico en las tres gamas.

**Que sea idéntico en las tres es el dato.** Si fuera un problema de capacidad,
esperaríamos verlo mejorar con el tamaño, como mejora la intención entre ligera
y media. No mejora: es plano. Un resultado plano entre 1.5B, 3B y 7B apunta a
algo que las tres comparten —el prompt, la gramática, la forma de los casos— y
no a lo único que varía entre ellas.

Eso es una **hipótesis**, no una conclusión, y explícitamente **no se actúa
sobre ella todavía**. Ver abajo.

### La gramática, gama por gama — y un `PROVEN` retirado

`grammar-check` pide la única palabra que la gramática no puede emitir, dos
veces, con la bandera y sin ella.

| Gama | Brazo restringido | Brazo libre | Veredicto |
|---|---|---|---|
| Ligera | `{ "operation": "install_module", "targets": ["python3.ipython3.…` | `[end of text]` | **NO PROBADO** (decía `PROVEN`) |
| Media | `{ "operation": "install_module", "targets": ["requests_module_1.…` | `BANANA <<<THALYX-e4e3dc…>>> BANANA` | **PROBADO** |
| Alta | `{ "operation": "install_module", "targets": [ "all_nodes_except_…` | `BANANA<<<THALYX-21d02d…>>> [end of text]` | **PROBADO** |
| Máxima | — | — | **N/D** |

En media y alta la evidencia es la que el veredicto describe: restringido no
puede ni empezar con la palabra, suelto la dice. **En ligera no.** El brazo de
control dice `[end of text]`, que es lo que `llama.cpp` imprime cuando el modelo
termina la generación de inmediato: el 1.5B, sin gramática, **no dijo nada**.

Y aun así el comando imprimió `PROVEN`, bajo un veredicto que afirma *«left
alone it did»* — decir la palabra. Nada lo había comprobado:

```rust
} else if obeys_root(&unconstrained) {
    Inconclusive { ... }
} else {
    InForce { ... }        // ← se llegaba aquí por descarte
}
```

`InForce` era el `else`. Se alcanzaba con que el brazo libre **no abriera un
objeto**, y un brazo que no dice nada tampoco abre un objeto. La mitad del
veredicto que habla del control se estaba infiriendo de la ausencia de otra
cosa, que es justo lo que el banco dejó de hacer el mismo día.

Es la **regla 4**: sin control, una denegación y una operación que nunca ocurrió
se ven igual. Y sobrevivió por la **regla 8**: los cuatro sustitutos del sondeo
decían la palabra cuando se les quitaba la gramática, porque todos se escribieron
para contestar. Ninguno modelaba un modelo callado. Regla nueva en
[[Estrategia-de-Pruebas]].

**Corregido**: `InForce` ahora exige que el brazo libre diga la palabra, y hay
dos regresiones —una con el `[end of text]` verbatim de esta corrida, otra con
prosa— que se comprobaron fallando contra el código anterior.

Lo que se puede afirmar de la corrida de ligera, que es menos que `PROVEN`:

> Con la bandera, el modelo emitió un objeto; sin ella, no emitió nada. **La
> bandera cambió la salida.** Que la palabra prohibida fuera lo que la gramática
> impidió es lo que no se midió, porque el modelo nunca mostró que la diría.

Así que la frase del decreto —«un contrato mal formado es imposible en las
cuatro gamas»— está **probada en dos gamas**, no probada en la ligera, y N/D en
la máxima. Las dos que faltan heredan el argumento de la gramática, no la
corrida.

### La demostración concreta de lo que la gramática no garantiza

La gama ligera contestó `dev.thalyx.demo, ese quiero` con:

```json
{"operation":"install_module","targets":["dev.thalyx.demo","ese.quiero.ios"]}
```

**Fabricó `ese.quiero.ios` a partir de las palabras humanas «ese quiero».** El
contrato es impecable: la operación existe, el campo es una lista, y cada
elemento respeta la forma de un id en DNS inverso. La gramática hizo exactamente
lo que esta nota promete y **nada más**, porque nada más es lo que puede hacer.

Es la sección *«lo que la gramática no garantiza»* de arriba, demostrada en vez
de argumentada, y separa cinco cosas que un porcentaje agregado confunde:

1. **contrato sintácticamente válido** — lo garantiza la gramática;
2. **interpretación correcta** — no;
3. **argumento correcto** — no;
4. **abstención correcta** — no;
5. **caso no medido** — ninguna de las anteriores.

Y hay una sexta que esta corrida hizo visible: **inventar un id y que el núcleo
lo cace** es una conducta distinta de las cinco. La atribución rechazó
`ese.quiero.ios` porque no aparece en ningún canal, así que el sistema se portó
bien de punta a punta — pero lo que se portó bien fue el núcleo, no el modelo.

### Las invenciones que el resumen no suma

En el banco, un id que la atribución rechaza se marca `REF` en un caso de acción
y `INV` en uno de abstención. **Los `REF` no entran en ninguna fracción del
resumen**: no son intención, no son argumentos, y el contador de invenciones sólo
cuenta los casos de abstención.

Contados a mano sobre los casos de acción:

| Gama | `REF` |
|---|---|
| Ligera | 3 |
| Media | 1 |
| Alta | **4** |

Los cuatro de la gama alta son casos donde la media acertó (`ok`) o falló de otra
forma. Es una observación, no una conclusión —cuatro contra uno sobre diecinueve
casos no separa nada— pero es **la más específica que hay sobre por qué la alta
no superó a la media aquí**, y no aparece en ninguna cifra del resumen.

Lo que impidió afinarla es un defecto de evidencia, corregido: el banco imprimía
`(named something nobody mentioned)` **sin decir qué**. Cuatro rechazos en una
corrida y ninguna forma de saber si la gama inventa el mismo id una y otra vez o
uno distinto cada vez, que son dos hallazgos diferentes sobre el modelo. Ahora
imprime el valor. Esto no cambia ninguna medida; cambia lo que se puede leer de
la próxima corrida.

### Lo que NO se cambia por estos resultados

Ninguna de estas es una conclusión de la corrida; todas son cosas que se dejan
quietas **a propósito** para que la próxima corrida signifique algo:

- **El prompt no se toca.** La instrucción de abstención es sospechosa y hay una
  hipótesis escrita para ella, pero tocar el prompt mueve los veinte casos a la
  vez, y no hay un antes/después medido con el que comparar. Ver
  [[Tareas-Pendientes]].
- **La gramática no se toca.**
- **Las gamas no se tocan.** Nada aquí dice que la familia esté mal elegida, y
  esta nota decreta que una familia se cambia entera o no se cambia — cambiar la
  alta sola destruiría la comparabilidad que hace legible todo lo de arriba.
- **La suite no se optimiza.** Ajustar los casos después de ver los resultados
  convierte un banco en una descripción de la corrida que ya ocurrió.
- **La tabla de RAM recomendada no baja**, aunque el RSS medido sea menor en dos
  gamas. Decidido por Cesar el mismo día: los resultados se declaran, no se
  presentan como definitivos, y lo definitivo llega con Thalyx corriendo como
  sistema operativo real sobre un SSD real.

### Los veinte casos, gama por gama

`ok` acertó ▪ `ERR` sin medición ▪ `REF` nombró algo que no aparece en ningún
canal ▪ `INV` inventó donde debía abstenerse ▪ `MISS` se abstuvo donde debía
actuar (no ocurrió ni una vez).

| # | Caso | ligera | media | alta |
|---|---|---|---|---|
| 1 | a pronoun pointing at the one thing installed | `ERR` | `ok` | `ok` |
| 2 | the id said plainly, with no verb in front of it | `REF` | `REF` | `REF` |
| 3 | a module named by what it does rather than by its id | `ok` | `ok` | `ok` |
| 4 | a version named in words | `ERR` | `ERR` | `REF` |
| 5 | the same request in English | `ok` | `ok` | `ok` |
| 6 | a wish with no module behind it | `INV` | `INV` | `INV` |
| 7 | a need stated as a category | `INV` | `INV` | `INV` |
| 8 | a demonstrative pointing at nothing | `INV` | `INV` | `INV` |
| 9 | a module that was mentioned and then ruled out | `ERR` | `INV` | `INV` |
| 10 | the id said plainly, with a verb that is not an install verb | `REF` | `ok` | `ok` |
| 11 | the id said plainly, with the machine listing it | `REF` | `ok` | `ok` |
| 12 | the id said plainly, in English, with no verb | `ok` | `ok` | `REF` |
| 13 | an install verb with two ids | `ok` | `ok` | `ok` |
| 14 | a module named by description among three | `ok` | `ok` | `ok` |
| 15 | a version said as a range rather than as a number | `ERR` | `ok` | `REF` |
| 16 | a negation with the module named first | `ERR` | `INV` | `INV` |
| 17 | a negation phrased as an exclusion from a list | `INV` | `INV` | `INV` |
| 18 | a question about a module rather than a request | `INV` | `INV` | `INV` |
| 19 | a need stated as a category, in English | `ERR` | `INV` | `ERR` |
| 20 | a demonstrative pointing at nothing, with things listed | `INV` | `INV` | `INV` |

Cinco cosas que se leen de esa tabla y no del resumen:

1. **El caso 2 falla en las tres gamas, y no falla como se creía.**
   `dev.thalyx.demo, ese` —el id dicho en claro, sin verbo— sale `REF` en 1.5B,
   3B y 7B: el modelo **nombró algo que no aparece en ningún canal** y la
   atribución lo rechazó. Eso **no es una abstención**, y con eso se cae del todo
   la hipótesis que este proyecto arrastraba desde el 2026-08-08 —*«la instrucción
   de abstención del prompt pesa de más, porque se abstuvo con el id dicho en
   claro»*—. Aquel `MISS` salió del banco que clasificaba `Err(_) => Abstained`,
   donde **un rechazo por atribución se contaba como abstención**; el instrumento
   corregido dice que ese caso nunca fue una abstención, fue una invención cazada
   por el núcleo. El prompt queda absuelto de este cargo concreto. Y los casos 10
   y 11, escritos para separar las lecturas, contestan lo demás: con un verbo (10)
   o con la máquina listando el módulo (11), media y alta aciertan. **Lo que
   falta en el caso 2 es contexto alrededor del id, no menos instrucción de
   abstenerse.** El caso 12 impide cerrar más la explicación: es igual de
   escueto, en inglés, y la media sí lo acierta.
2. **Los cuatro casos que ninguna gama acertó** son 2, 6, 7, 8 — un id sin verbo
   y las tres abstenciones sin nada nombrado.
3. **Los seis casos que las tres acertaron** son 3, 5, 13, 14 y —en media y
   alta— 10, 11: descripción en vez de id, inglés, dos ids, y descripción entre
   tres. La comprensión de una petición **descrita** es lo que mejor sale.
4. **Las tres negaciones (9, 16, 17) fallan en las tres gamas**, y el caso 18
   —una pregunta, no una petición— también. Cuatro formas distintas de «esto no
   es una orden de instalar» y ninguna gama distingue ninguna.
5. **La gama ligera pierde casos donde las otras responden**: sus seis `ERR` son
   casos que media y alta sí midieron. Eso no es una puntuación baja, es una
   gama que **no llega a contestar**.

El valor que cada gama propuso se conserva sólo donde la transcripción de la
corrida lo traía; la próxima corrida lo trae completo, ahora que los rechazos
imprimen el valor.

## Segunda corrida de la gama ligera — 2026-08-08

El experimento que la revisión anterior dejó pedido, y corrido sin cambiar
nada: mismo modelo, misma suite de veinte casos, misma máquina, misma
llama.cpp. Lo único distinto es el código de evidencia corregido —el sondeo de
gramática que ya no llega a `InForce` por descarte, y los rechazos que ahora
imprimen el valor propuesto—. Ninguno de los dos cambia una medición.

### La gramática: `NOT PROVEN`, en la máquina, como debía

```
with the grammar     { "operation": "install_module", "targets": ["python3.ipython3.ipython3.…
without it           [end of text]

NOT PROVEN: the unconstrained arm did not say the word either, so nothing
here shows the grammar is what stopped it.
```

La corrección funciona sobre hardware real, que es la única forma de saberlo
—regla 1—. El `PROVEN` de la gama ligera está retirado y en su lugar hay el
resultado correcto: **no hay evidencia**, ni a favor ni en contra.

Y el brazo restringido dice algo que no se había visto: `python3.ipython3.`
`ipython3.…`. El 1.5B **se cicla dentro del identificador**. Guárdalo, porque
es la misma patología que explica los `ERR`.

### Las dos corridas de la gama ligera no dieron lo mismo

| | 1ª corrida | 2ª corrida |
|---|---|---|
| Disco | 1 117 320 736 B | 1 117 320 736 B |
| RSS pico | 2.82 GB | 2.82 GB |
| Latencia mediana | 3.77 s | 3.77 s |
| Latencia peor | 4.27 s | **5.07 s** |
| Casos medidos | 14/20 | **15/20** |
| Intención | 5/14 | **6/15** |
| Argumentos | 5/14 | **6/15** |
| Abstención | 0/6 | 0/6 |

Se movieron **dos casos de veinte, en direcciones opuestas**:

| # | Caso | 1ª | 2ª |
|---|---|---|---|
| 1 | a pronoun pointing at the one thing installed | `ERR` | `REF` |
| 11 | the id said plainly, with the machine listing it | `REF` | `ok` |

El caso 1 pasó de no medirse a medirse mal; el 11, de mal a bien. Los otros
dieciocho salieron idénticos, marca por marca.

Esto es lo primero de este proyecto que **replica una cifra de acierto**, y lo
que dice es que esas cifras se mueven. Las de coste no: disco al byte, RSS a la
centésima de GB y mediana a la centésima de segundo, dos veces. **El coste de
una gama se mide; su acierto se estima.**

> **Corregido con la tercera corrida**, más abajo: lo que se mueve no es el
> acierto sino cuántos casos llegan a contestar. El número de respuestas
> correctas sobre los veinte casos resultó ser estable —5, 6, 6 en la ligera; 9
> y 9 en la media—. Ver «Tres corridas de ligera y dos de media».

### Por qué se mueven dos corridas que deberían ser idénticas

`Invocation` fija `--seed 1` y `--temp 0`. Con eso el *muestreador* es
determinista, y de ahí venía la creencia —escrita en el encabezado de
`llama.rs`— de que una corrida se puede repetir. No se puede, y no por culpa de
llama.cpp:

- `Prompt::render` genera un **marcador aleatorio nuevo en cada invocación**, así
  que el prompt son bytes distintos cada vez. Determinismo ante la misma
  entrada no es reproducibilidad cuando la entrada cambia.
- La ruta que `-f` imprime vive en un directorio temporal que se borra al
  terminar la corrida, así que el archivo que nombra el comando ya no existe
  cuando alguien lee la línea.

Lo más incómodo: **no hacía falta una máquina para verlo**. La prueba
`a_marker_is_never_reused_between_two_renders` afirma que el marcador cambia
desde el día en que se escribió. Una prueba y un comentario del mismo crate
decían cosas opuestas, y se le creyó al comentario porque nunca nadie los puso
a coincidir.

Consecuencia para todo lo de arriba: **ninguna fracción por gama es una cifra
exacta**, y una diferencia de dos casos entre dos gamas —los `9/19` de la media
contra los `7/19` de la alta— es del mismo tamaño que lo que se movió una sola
gama consigo misma. Eso no retira nada de lo medido; le pone el margen que le
faltaba.

Cesar decidió el 2026-08-08 **guardar el prompt bajo una bandera**, y ya está
construido:

```
thalyx agent bench --keep-prompt ./evidencia
thalyx agent model check --keep-prompt ./evidencia
thalyx agent model grammar-check --keep-prompt ./evidencia
```

Cada inferencia deja un directorio —nombrado por su propio marcador, así que un
banco de veinte casos deja veinte sin pisarse— con el prompt exacto, la
gramática y el comando que los corrió. Del sondeo de gramática salen dos, y sólo
uno de los dos comandos nombra un `--grammar-file`, porque sólo uno lo pasó.

Lo que eso recupera es **repetir esa corrida**: los bytes que corrieron, con su
marcador. Lo que deliberadamente **no** hace es volver iguales dos corridas
distintas. Para eso habría que hacer el marcador derivable, y que no se pueda
adivinar es la razón entera por la que se aleatoriza —[[Marcado-de-Origen]]—.
Además esconder ese movimiento sería peor que medirlo: daría una muestra de una
distribución con cara de medición. Sin la bandera no queda nada en disco, igual
que antes.

De paso apareció algo que nadie había notado: `Invocation::command_line`, la
función que el encabezado de `llama.rs` citaba como la forma de reproducir una
corrida, **no tenía ni una sola llamada fuera de su propia prueba**. La
documentación describía una función que nunca se había ejecutado. Ahora la llama
`--keep-prompt`.

### Los `ERR` tienen una sola causa, y ahora se sabe cuál

Los cinco dan exactamente el mismo mensaje:

> the model began the object the grammar describes and ran out of tokens before
> closing it, at the 256-token cap.

No es plazo agotado, no es llama.cpp cayéndose, no es la gramática sin aplicar,
no es el analizador. Es **una sola causa, la misma cinco veces**, y el propio
error ya la explicaba antes de que se midiera: la gramática no acota cuán largo
puede ser un id de módulo, así que un modelo que no encuentra una forma legal de
contestar **se gasta el presupuesto entero dentro de una sola cadena**. El
sondeo de gramática lo enseña en vivo: `python3.ipython3.ipython3.…`.

Dos cosas se siguen de ahí, y ninguna es «el 1.5B no entiende»:

1. **Subir `-n` no arregla nada.** Lo dice el mensaje: una cuota mayor no hace
   la respuesta correcta, la hace más larga. Un ciclo con más presupuesto es un
   ciclo más largo.
2. **Es la misma patología que las invenciones**, no otra. `ese.abc.abc.abc`,
   `thallyx.ing.ing`, `python3.ipython3.ipython3` — repetir un segmento hasta
   que algo lo detenga. Cuando el corte llega antes del cierre sale `ERR`;
   cuando llega después, sale un id inventado. **Es un fallo, contado como
   dos.**

### Los valores inventados, ahora visibles

El arreglo del banco —imprimir qué propuso el modelo— rinde de inmediato:

| # | Caso | Lo que propuso |
|---|---|---|
| 1 | a pronoun pointing at the one thing installed | `dev.thalyx.demo.localhost` |
| 2 | the id said plainly, with no verb in front of it | `ese.abc.abc.abc` |
| 6 | a wish with no module behind it | `org.openjdk.jmh` |
| 7 | a need stated as a category | `org.webmuse.video-editing` |
| 8 | a demonstrative pointing at nothing | `thallyx.ing.ing` |
| 10 | the id said plainly, with a verb that is not an install verb | `ese.quiero.iam` |

Cuatro formas distintas de inventar, y ninguna es ruido:

- **Un id real con un sufijo pegado** (`dev.thalyx.demo` + `.localhost`). El
  módulo existe; lo que no existe es el id propuesto.
- **Una palabra española tratada como espacio de nombres**: `ese` de «ese
  quiero» encabeza dos ids distintos. El modelo lee el demostrativo como parte
  del identificador.
- **Repetición hasta el corte**: `.abc.abc`, `.ing.ing`.
- **Ids memorizados del entrenamiento**: `org.openjdk.jmh` y
  `org.webmuse.video-editing` no salen de la conversación, salen de haber visto
  coordenadas Maven. Es exactamente lo que la atribución existe para cazar, y
  lo cazó.

El caso 10 confirma que la patología es estable y el token exacto no: la primera
corrida propuso `ese.quiero.ios`, esta `ese.quiero.iam`. Misma estructura, final
distinto.

Y una distinción que el resumen tapa: **«inventó» son dos cosas**. En los casos
6, 7 y 8 el modelo nombró algo que no aparece en ningún canal y el núcleo lo
rechazó. En los casos 17, 18 y 20 nombró `dev.thalyx.demo`, que **sí está
listado** —la máquina lo dijo—, así que la atribución no tenía nada que objetar:
el id es real, lo que está mal es que la frase no pedía instalarlo. Contra la
segunda clase la atribución no protege, y no es su trabajo. Es el único lugar de
todo esto donde el `0/6` de abstención no tiene ninguna red debajo.

## Tres corridas de ligera y dos de media — 2026-08-08

Con `--keep-prompt`, sin cambiar nada más. Esto **corrige la lectura de la
sección anterior**, que decía «las cifras de acierto se mueven». Se mueven, pero
no por donde parecía.

### Lo que se mueve no es el acierto, es cuántos casos contestan

| | ligera 1ª | ligera 2ª | ligera 3ª | media 1ª | media 2ª |
|---|---|---|---|---|---|
| Sin medición | 6 | 5 | **2** | 1 | 2 |
| Casos medidos | 14 | 15 | 18 | 19 | 18 |
| Intención | 5/14 | 6/15 | 6/18 | 9/19 | 9/18 |
| **Aciertos sobre los 20** | **5** | **6** | **6** | **9** | **9** |
| Abstención | 0/6 | 0/6 | 0/9 | 0/9 | 0/8 |
| Latencia mediana | 3.77 s | 3.77 s | 3.72 s | 6.78 s | 6.68 s |
| RSS pico | 2.82 GB | 2.82 GB | 2.82 GB | 4.79 GB | 4.79 GB |

**El número de respuestas correctas casi no se mueve**: 5, 6, 6 en la ligera;
9 y 9 en la media, con 8 y 8 de argumentos. Lo que se mueve es cuántos casos
llegan a producir una respuesta —de 6 sin medición a 2 en la ligera—, y como ése
es el denominador, la fracción se mueve sin que la comprensión haya cambiado.

Lo hace visible el peor caso posible, que además ocurrió: **la ligera contestó
más casos y su fracción empeoró.** 5/14 es 36 %; 6/18 es 33 %. Acertó *una más*
y bajó, porque los cuatro casos que dejó de perder volvieron todos mal. Una
fracción cuyo denominador se mueve por una razón ajena al numerador no es
comparable consigo misma.

De ahí la forma correcta de leer este banco, y es un cambio de hábito:

> **La cifra estable es el número de aciertos sobre los veinte casos**, porque
> los veinte no se mueven. La fracción sobre lo medido dice otra cosa —qué tan
> bien le fue *en lo que alcanzó a contestar*— y las dos no se comparan entre
> corridas con distinto número de fallos.

Con eso, y sólo con eso, la comparación entre gamas se puede volver a plantear:
**ligera 5, 6, 6 — media 9, 9 — alta 7** (una sola corrida). La media no se
movió ni un caso en dos corridas. La distancia con la alta sigue siendo de dos
casos y la alta sigue teniendo una sola medición, así que sigue sin poder
afirmarse; lo que sí queda es que la media no está teniendo días buenos, está
donde está.

### Caso por caso, y hay uno que no falla nunca por casualidad

| # | Caso | ligera ×3 | media ×2 |
|---|---|---|---|
| 1 | a pronoun pointing at the one thing installed | `ERR` `REF` `REF` | `ok` `ok` |
| 2 | the id said plainly, with no verb in front of it | `REF` `REF` `REF` | `REF` `REF` |
| 3 | a module named by what it does | `ok` `ok` `ok` | `ok` `ok` |
| 4 | **a version named in words** | `ERR` `ERR` `ERR` | `ERR` `ERR` |
| 5 | the same request in English | `ok` `ok` `ok` | `ok` `ok` |
| 6 | a wish with no module behind it | `INV` `INV` `INV` | `INV` `ERR` |
| 7 | a need stated as a category | `INV` `INV` `INV` | `INV` `INV` |
| 8 | a demonstrative pointing at nothing | `INV` `INV` `INV` | `INV` `INV` |
| 9 | a module mentioned and then ruled out | `ERR` `ERR` `INV` | `INV` `INV` |
| 10 | the id said plainly, verb that is not an install verb | `REF` `REF` `REF` | `ok` `ok` |
| 11 | the id said plainly, with the machine listing it | `REF` `ok` `ok` | `ok` `ok` |
| 12 | the id said plainly, in English, with no verb | `ok` `ok` `ok` | `ok` `ok` |
| 13 | an install verb with two ids | `ok` `ok` `ok` | `ok` `ok` |
| 14 | a module named by description among three | `ok` `ok` `ok` | `ok` `ok` |
| 15 | a version said as a range | `ERR` `ERR` `ERR` | `ok` `ok` |
| 16 | a negation with the module named first | `ERR` `ERR` `INV` | `INV` `INV` |
| 17 | a negation phrased as an exclusion from a list | `INV` `INV` `INV` | `INV` `INV` |
| 18 | a question about a module | `INV` `INV` `INV` | `INV` `INV` |
| 19 | a need stated as a category, in English | `ERR` `ERR` `INV` | `INV` `INV` |
| 20 | a demonstrative pointing at nothing, with things listed | `INV` `INV` `INV` | `INV` `INV` |

De veinte casos, **catorce dieron exactamente la misma marca en las cinco
corridas de las dos gamas**. La suite es mucho más estable de lo que la primera
comparación de fracciones hacía pensar.

Y en el centro queda el **caso 4**, que es el hallazgo más limpio de estas cinco
corridas: `quiero la 1.4 del demo` **no ha producido una medición ni una sola
vez** —tres de tres en ligera, dos de dos en media— y siempre con el mismo
mensaje, el presupuesto de 256 tokens agotado antes de cerrar el objeto. La
única corrida donde no fue `ERR` fue la de la gama alta, donde fue `REF`. Cinco
de seis.

No es un caso duro de entender: el módulo está dicho por su nombre y la versión
también. Lo que tiene de único en toda la suite es **la forma de la respuesta
que pide** — es el único caso cuya restricción esperada lleva un punto adentro
(`1.4`); la del caso 15 es `1`, y la media lo acierta las dos veces.

### Resuelto: el ciclo está en `module-id`, no en `range`

> **La hipótesis de abajo quedó refutada el mismo día**, y se conserva porque la
> diferencia entre lo que se supuso y lo que se vio es el punto. Cesar corrió la
> inferencia guardada del caso 4 —`prompt.txt`, `proposal.gbnf`, marcador
> original y `command` original, sin Thalyx de por medio— y la salida completa
> contesta sola:
>
> ```
> {"operation": "install_module",
>  "targets": ["dev.thalyx.demo.versions.versions.versions.versions.versions…
> ```
>
> `eval time … / 255 runs` contra `n_predict = 256`: se gastó el presupuesto
> entero repitiendo `.versions`. **Nunca llegó a `constraint`**, así que el punto
> de `1.4` no tuvo nada que ver. La producción que absorbe los tokens es
> `module-id`, y era la otra sospechosa de la lista.

Las tres capas hay que decirlas separadas, porque nombrar una por otra es cómo
se arregla lo que no era:

| | |
|---|---|
| **Causa inmediata** | agotó `n_predict` sin cerrar el objeto |
| **Causa observada** | entró en repetición de `.versions` dentro de `module-id` |
| **Condición que lo permite** | la producción admite segmentos sin cota |

Y la atribución de culpa importa: **la gramática no lo obliga a repetir**. El
modelo elige `.versions`; la gramática simplemente nunca le exige cerrar. Decir
«la gramática lo hizo ciclar» sería falso y llevaría a tratar como defecto
estructural lo que es una decisión del modelo ante una producción permisiva.

Lo que sí queda demostrado es que **esto explica de más**, y ése es el hallazgo
mayor. No es una excepción del caso `1.4`: es el mismo patrón que ya se había
visto sin reconocerlo.

```
dev.thalyx.demo.versions.versions.versions…   ← caso 4, hasta agotar el presupuesto
dev.thalyx.demo.https.localhost               ← caso 1
dev.thalyx.demo.localhost                     ← casos 9 y 16
ese.abc.abc.abc                               ← caso 2
thallyx.ing.ing                               ← caso 8
photoshop-1.ashx.ashx                         ← caso 19
python3.ipython3.ipython3…                    ← el brazo restringido del sondeo
```

> **Cuando el 1.5B no sabe cómo cerrar semánticamente un identificador, sigue
> produciendo segmentos sintácticamente válidos.** Si el corte llega antes de
> cerrar la cadena sale `ERR`; si llega después, sale un id inventado. Un solo
> comportamiento, contado hasta ahora como dos o tres.

Y el caso 4 es la demostración más limpia que tiene este proyecto de lo que la
gramática **no** puede hacer. El prompt dice `Name only module ids that appear in
the material below` y debajo trae `available: dev.thalyx.demo 1.4.2, 2.0.0`. El
modelo **empieza con el id correcto** y lo convierte en otro al no saber parar:

```
Gramática:   "dev.thalyx.demo.versions" es un module-id válido      ✅
Atribución:  nadie mencionó "dev.thalyx.demo.versions"              ❌
```

Estructura y significado, separados en una sola cadena. La gramática garantiza
la primera columna y no puede tocar la segunda — que es exactamente lo que
[[Marcado-de-Origen]] dice y lo que la atribución existe para cubrir.

### La hipótesis anterior, que estaba equivocada

La gramática tiene **tres repeticiones sin cota superior**:

```
range      ::= [0-9A-Za-z.^~><=*+ |,-]+
module-id  ::= "\"" segment "." segment "." segment ("." segment)* "\""
segment    ::= [a-z] [a-z0-9_-]*
```

Cualquiera de las tres puede absorber los 256 tokens, y la clase de `range`
admite el punto, los dígitos, las letras y el espacio. La hipótesis es que el
modelo escribe `1.4` y **no encuentra dónde parar**, igual que se cicla dentro
de un id (`python3.ipython3.ipython3`, `ese.abc.abc.abc`, `photoshop-1.ashx.ashx`).

**Refutada.** Lo que se supuso fue que el punto de `1.4` dejaba a `range` sin
dónde terminar; lo que ocurrió fue que nunca llegó a `range`. Se conserva escrita
porque la lección no es que la hipótesis fuera tonta —las dos producciones sin
cota eran igual de sospechosas— sino que se distinguían con **un comando**, y
hasta correrlo no había forma de elegir entre ellas. El banco truncaba el texto a
90 caracteres, así que la evidencia estaba a la vista y cortada justo antes de la
parte que decidía.

Eso es lo que `--keep-prompt` compró, un día después de construirse.

### Propuesta de cota estructural — no aplicada

Lo primero que salió al inspeccionar la producción no era lo que se buscaba:

```rust
// thalyx-manifest, que es la autoridad sobre qué es un id
fn is_valid_module_id(id: &str) -> bool {
    let segments: Vec<&str> = id.split('.').collect();
    if segments.len() < 3 { return false; }
    segments.iter().all(|segment| { /* … juego de caracteres … */ })
}
```

**La autoridad tampoco tiene cota superior.** Un id de cuarenta segmentos es un
id válido para Thalyx hoy. La gramática no es más permisiva que el manifiesto —
lo espeja fielmente, como dice su comentario. El hueco está en los dos.

Eso decide la forma de la propuesta: **acotar sólo la gramática la volvería más
estricta que la autoridad**, y entonces el modelo no podría proponer ids que
Thalyx sí aceptaría. Un desacuerdo así es exactamente lo que
`the_grammar_and_the_scanner_agree_on_what_a_module_id_is_made_of` existe para
impedir, y esa prueba hoy sólo compara juegos de caracteres, no cuenta segmentos.

Lo que se sabe de los ids reales: **todos los que existen en este repositorio
tienen exactamente tres segmentos** — `dev.thalyx.demo`, `dev.thalyx.greeter`,
`org.demo.thing`, `org.example.tool`, `org.publisher.pyassist`, `dev.evil.module`.
Ninguno tiene cuatro.

Las opciones, para que Cesar decida:

| | Qué cambia | Qué cuesta |
|---|---|---|
| **A. Acotar los dos** —manifiesto y gramática, p. ej. máximo 6 segmentos | Quedan de acuerdo, y la cota es una propiedad del sistema | Cambia qué acepta el manifiesto, y el id es inmutable y está anclado a una llave de publicador. Es decreto, no limpieza |
| **B. Acotar sólo la gramática** | Corta el ciclo sin tocar la identidad de los módulos | La gramática se vuelve más estricta que la autoridad, y hay que decir por qué en vez de que sea un descuido |
| **C. No acotar nada** | Cero riesgo, cero cambio | Se conserva `ERR` como señal visible, y se pierden casos de la medición |

**La predicción, que importa más que la preferencia:** acotar **no subiría el
acierto**. Un `module-id` con tope obligaría al modelo a cerrar la cadena en
`dev.thalyx.demo.versions.versions.versions`, que sigue siendo un id que nadie
mencionó y que la atribución sigue rechazando. Convertiría `ERR` en `REF`, no en
`ok`.

Y eso no es especulación: **ya se observó**. En la tercera corrida de la ligera
los casos 9, 16 y 19 dejaron de ser `ERR` por su cuenta y volvieron los tres
`INV`. Más casos medidos, mismo número de aciertos.

Así que el argumento a favor de acotar no es que el agente entienda mejor —no lo
haría— sino que el banco mediría más casos y se dejarían de gastar 256 tokens y
casi cuatro segundos en cada ciclo. El argumento en contra es de la regla 9:
un `ERR` visible es la respuesta cautelosa y un id inventado bien formado es la
rápida. La atribución caza las dos, así que ninguna es peligrosa; la diferencia
es cuál se lee más fácil.

**No se toca sin medir antes y después.** Las seis corridas actuales son la línea
base y cambiar la gramática las vuelve incomparables. Y `-n` no sube: esta
corrida demuestra que el presupuesto no es la causa, y 512 tokens sólo comprarían
más `.versions`.

Una nota práctica para quien lo implemente: la forma segura de escribir la cota
en GBNF es repetir el grupo opcional —`("." segment)? ("." segment)? …`— porque
si el `{m,n}` de GBNF lo acepta la versión de llama.cpp de Cesar **no está
verificado**, y una gramática que su build rechaza tumba la inferencia entera.

### La abstención, ahora sobre 46 oportunidades

Sumando las cinco corridas de estas dos gamas más la única de la alta:

| Corrida | Abstención |
|---|---|
| ligera 1ª, 2ª, 3ª | 0/6, 0/6, 0/9 |
| media 1ª, 2ª | 0/9, 0/8 |
| alta 1ª | 0/8 |

**Cuarenta y seis oportunidades de abstenerse, cero abstenciones.** Ni una, en
tres tamaños de modelo y seis corridas. Ya no es un resultado de una corrida que
podría haber salido mal: es la propiedad más firmemente medida de todo este
trabajo, y sigue siendo la que [[Gamas-de-Modelo]] llama la más importante.

La ligera lo confirmó de la peor manera. Sus tres casos que nunca se habían
medido —9, 16 y 19— resultaron ser **casos de abstención**, y al contestarlos
por primera vez los falló los tres. Su denominador subió de 6 a 9 y el numerador
siguió en cero.

Y sigue sin tocarse el prompt.

### Hipótesis con mecanismo: la gramática obliga a comprometerse antes de poder declinar

Buscando por qué la abstención no se mueve con el tamaño, la respuesta más
probable no está en los modelos. Está en la primera línea de la gramática:

```
root      ::= "{" ws "\"operation\"" ws ":" ws operation ws "," ws "\"targets\"" …
operation ::= "\"install_module\""
```

`ProposedOperation::ALL` tiene **exactamente un elemento**. La gramática no le
ofrece al modelo ninguna alternativa en el primer campo, y el orden de los campos
está fijo. Es decir:

> **Lo primero que el modelo escribe, en cada inferencia de cada caso, es
> `"install_module"` — obligado.** Sólo después llega a `targets`, donde
> abstenerse significa emitir `[]` y contradecir lo que la gramática ya le hizo
> decir.

Un modelo autorregresivo condiciona sobre su propia salida. Para cuando llega a
elegir objetivos, ya se dijo a sí mismo que esto es una instalación.

Los casos 6, 7 y 8 son los que lo enseñan mejor, porque **no requieren
razonamiento**: `instala algo bueno`, `necesito algo para editar video`. No hay
ningún módulo nombrado en el transcript. No hace falta entender la negación ni
distinguir una pregunta de una orden — basta con notar que no hay nada que
instalar. Las tres gamas inventaron:

```
org.openjdk.jmh        org.web3j.video       com.videolan.vlc
com.example.module1    com.adobe.photoshop   org.thalibos.thingy.thingy1
```

Nombres de paquetes memorizados del entrenamiento. Obligado a decir
`install_module` y luego a nombrar algo, el modelo fue a buscar a su memoria.

Y hay una ironía que vale registrar. El comentario de la prueba
`a_model_that_found_nothing_has_a_way_to_say_so` describe el defecto que vino a
arreglar:

> *«una gramática que exige al menos un id vuelve inexpresable la abstención, así
> que toda frase ambigua vuelve como una invención confiada — y se culpa a la
> gama por una decisión que la gramática le quitó».*

Eso es **exactamente lo que se está observando**, y llevábamos seis corridas
culpando a las gamas. El arreglo hizo expresable la abstención y la dejó
alcanzable sólo después de un compromiso forzado. **Fue a la mitad.**

**Esto es una hipótesis, no una causa probada.** Compite con otras: que la
instrucción de abstención del prompt pese poco, que Qwen2.5-Instruct esté ajustado
para complacer, o que la negación sea comprensión y no estructura —y los casos 9,
16, 17 y 18 sí son de negación o de pregunta—. Lo que la distingue es que
explica por qué el resultado **no se mueve con el tamaño**, y las otras no.

#### El instrumento: `thalyx agent grammar-effect`

El primer plan era un `sed` sobre el `command` guardado y tres lecturas a ojo.
Eso habría contestado, y habría dejado el resultado dependiendo de cómo alguien
leyó tres párrafos de prosa. Se construyó el instrumento en su lugar
—`crates/thalyx-agent/src/grammar_effect.rs` y `thalyx agent grammar-effect`—
porque esta pregunta merece un veredicto que se pueda repetir.

**Corre la suite entera dos veces**, con `--grammar-file` y sin él, con **un solo
prompt renderizado por caso** —mismo marcador en los dos brazos, así que difieren
en exactamente un flag—.

**No lee prosa.** La tentación era escribir algo que decidiera si un párrafo
libre «se rehusó», y eso es un analizador de la salida de otra herramienta hecho
con fixtures inventadas por su autor: el error que esta bóveda ya registra dos
veces. En vez de eso hace la pregunta mecánica que la atribución ya sabe hacer:
**¿nombró un id que no aparece en nada de lo que se le dijo?** Inventar es el
fallo bajo estudio, e inventar se cuenta.

Tres respuestas por brazo, y la de en medio es honesta sobre su debilidad:

| | |
|---|---|
| `INVENTED <id>` | nombró algo que nadie mencionó — el fallo, sin interpretación |
| `named nothing` | ningún id en todo el texto |
| `named <id>` | sólo ids reales. **No significa que propusiera instalarlo**: «ningún módulo coincide, el disponible es dev.thalyx.demo» cae aquí y es un rechazo. No cuenta para ningún lado; se imprime para que lo lea una persona |

**Y lleva control, que es de lo que depende todo.** Regla 4: sin él, «el brazo
libre no nombró nada» tiene dos lecturas idénticas —el modelo declinó, o el
modelo divaga sin gramática y nunca nombra nada—. Así que los casos de *acción*
también se corren por los dos brazos, y **si sin gramática el modelo no encuentra
el módulo correcto ni donde sí lo hay, el veredicto es `NOT PROVEN` y sale
distinto de cero**. Un sondeo que no puede fallar no es un sondeo.

Dos defectos aparecieron construyéndolo, y los dos habrían empujado el resultado
hacia declarar culpable a la gramática:

1. **El escáner de ids era ciego al JSON.** El brazo restringido siempre es JSON
   —`["dev.thalyx.demo"]` es un solo token de espacio en blanco— así que una
   respuesta llena de invenciones se habría reportado como silencio. Lo encontró
   una prueba, no una corrida.
2. **Los dos brazos se pisaban el `command` guardado.** Comparten marcador, así
   que compartían directorio, y lo que sobrevivía era la línea sin
   `--grammar-file`. La evidencia del brazo que importa habría descrito al otro.
   **Ese defecto ya existía en `grammar-check` desde que se construyó
   `--keep-prompt`**, y nadie lo había visto. Corregido con su regresión,
   comprobada fallando contra el nombre anterior.

Quince pruebas cubren el veredicto sin necesitar un modelo, incluidos los tres
caminos a `Inconclusive`. Lo que no se puede probar en el contenedor es la
medición misma, y **no lleva etapa en `verify.sh` a propósito**: `verify.sh`
prueba afirmaciones de Thalyx, y esto es un experimento que contesta una pregunta
una vez. Cuarenta inferencias son cinco minutos que toda verificación pagaría
para siempre.

Si resultara confirmada, el arreglo tiene forma conocida —una operación que
signifique «nada que hacer», elegible en el primer campo— y **cambia la gramática,
el enum y el contrato**, así que rompe la comparabilidad con las seis corridas de
línea base. No se toca sin antes/después.

## Relacionado
- [[Agente-Conversacional]] — qué es el agente y qué no puede hacer
- [[Debate-Agente-Fine-Tuning]] — por qué el fine-tuning no es de Fase 1
- [[Marcado-de-Origen]] — la defensa que la gramática no puede dar
- [[Modelo-de-Amenaza]] — por qué el agente está fuera de la TCB
- [[Criterio-de-Salida-Fase-1]] — el criterio que anclar un modelo rompería
- [[Agente-Minimo]] — el primer agente que se construye con esto
