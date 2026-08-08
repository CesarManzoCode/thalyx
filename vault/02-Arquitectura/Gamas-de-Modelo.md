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

## Relacionado
- [[Agente-Conversacional]] — qué es el agente y qué no puede hacer
- [[Debate-Agente-Fine-Tuning]] — por qué el fine-tuning no es de Fase 1
- [[Marcado-de-Origen]] — la defensa que la gramática no puede dar
- [[Modelo-de-Amenaza]] — por qué el agente está fuera de la TCB
- [[Criterio-de-Salida-Fase-1]] — el criterio que anclar un modelo rompería
- [[Agente-Minimo]] — el primer agente que se construye con esto
