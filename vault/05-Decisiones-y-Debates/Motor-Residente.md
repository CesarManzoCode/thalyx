---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-28
tags: [agente, modulos, inferencia, motor, rendimiento, decreto]
---

# El motor se queda vivo, con los pesos adentro

## Problema

[[Motor-de-Inferencia-como-Modulo]] puso `llama.cpp` dentro del sistema de
módulos y eso funcionó: el 2026-08-28 la máquina arrancada en QEMU entendió
*«crea una carpeta llamada pruebas»*, la ejecutó y `ls` mostró `pruebas/`.

Lo que quedó mal no fue la arquitectura sino la forma del programa.
`llama-completion` es **de una sola respuesta por construcción**: carga un GGUF,
contesta, y muere. Así que la cadena real era

```
frase → ModuleEngine::complete → thalyx_core::run → confinamiento →
llama-completion → mmap del GGUF → inferencia → el proceso muere
```

y **la siguiente frase volvía a hacerlo todo**. Leer dos gigabytes de disco,
construir el contexto, calentar nada. Eso es la mayor parte de lo que cuesta un
modelo local, gastado otra vez en trabajo que la frase anterior ya había hecho,
en cada frase.

Peor: la llamada ocurría **dentro de la pulsación de Enter** en la sesión
gráfica. Durante esos segundos nada se redibujaba — ni el reloj, ni los paneles,
ni un indicador. Una máquina que parece muerta es una máquina que nadie espera.

## Decisión de Cesar — 2026-08-28

**El motor carga los pesos una vez y se queda vivo mientras la sesión lo esté.**
Y la pantalla no se bloquea nunca: mientras se infiere, el marco se sigue
componiendo y dice si está *cargando el modelo* o *pensando*.

Con cuatro límites que forman parte del decreto:

- **Nada de red.** Ni HTTP, ni TCP, ni el servidor que `llama.cpp` ya trae.
  Conceder `net/outbound` al programa menos confiable de la máquina para que dos
  procesos del mismo anfitrión se hablen es debilitar el aislamiento por
  comodidad.
- **Un solo lanzador.** No hay un segundo mecanismo de sandbox para el módulo que
  se queda. Es `thalyx_core::run`, partido en dos por dentro.
- **Una inferencia a la vez.** No hay cola, ni multiplexado, ni concurrencia.
- **Residencia no es conversación.** Los pesos se reutilizan; el contexto no.

## Qué se construyó

### `engine/thalyx-engine.cpp` — el programa que se queda

El mismo `llama.cpp` en la misma etiqueta fijada (`b10665`), con las mismas
banderas y el mismo enlace estático. Se copia dentro de `tools/` del checkout y
lo compila el mismo `cmake` que compilaba `llama-completion`, para que el enlace
contra los backends de ggml —que se mueve entre etiquetas— no sea algo que este
repositorio tenga que resolver a mano.

Lo que hace: carga el GGUF una vez, anuncia que está listo, y después contesta
peticiones enmarcadas por una tubería hasta que Thalyx cierra el otro extremo.
No reimplementa inferencia: todo lo que hay debajo del protocolo es la librería
`common` de `llama.cpp` — `common_tokenize`, `common_sampler_*`, `llama_decode`.

**Lo que nunca puede salir mal es `stdout`.** Thalyx lee marcos de respuesta de
ahí y `llama.cpp` imprime ahí — la ficha del modelo, el progreso de la carga, los
tiempos. En vez de perseguir cada `printf`, el programa mueve el `stdout` real a
un descriptor privado y apunta el 1 al `stderr`. Después de esa línea, todo lo
que imprima cualquier librería cae donde Thalyx lo drena como salida ordinaria
del módulo.

### El protocolo

Little-endian, con longitud por delante, y sin ningún delimitador de texto: una
completion son bytes arbitrarios elegidos por un modelo que no es confiable, y
cualquier delimitador que pueda teclear es uno que puede falsificar.

```
listo      motor → Thalyx, una vez   "THR1" u64 ms u32 pid u32 hilos u32 ctx
petición   Thalyx → motor            "THQ1" u32 predict u64 seed
                                            u32 largo + ruta del prompt
                                            u32 largo + ruta de la gramática
respuesta  motor → Thalyx            "THA1" u8 estado u64 ms u32 largo + cuerpo
```

**Se mandan rutas, no texto.** Los archivos ya están escritos donde al módulo se
le concedió leerlos, así que la petición pesa menos de un kilobyte y **la
inferencia sigue siendo inspeccionable en disco** — que es lo que `--keep-prompt`
compra y lo que un protocolo que mandara el prompt por el tubo habría perdido.

El cuerpo de una respuesta buena es **el prompt que ese proceso leyó, seguido de
la completion**, que es exactamente lo que `llama-completion` escribía en su
`stdout`. Así nada por encima de la costura cambia: `Prompt::answer_in` busca el
marcador con el que termina el prompt y toma lo que sigue, y un marcador ausente
es como Thalyx distingue *«el modelo contestó mal»* de *«la herramienta nunca
leyó el prompt»*.

### `RunningModule` — el lanzador partido, no duplicado

`thalyx_core::run` era una función que armaba el confinamiento, arrancaba,
esperaba y desarmaba. Eso es exactamente correcto para `correr` y no puede
expresar un motor que sobrevive a la respuesta. En vez de escribir un segundo
lanzador —un segundo lugar donde acertar con el cgroup, la política, el filtro
seccomp, la raíz pivotada y el uid, y un segundo lugar del que se separen— **se
le puso nombre a la mitad**:

```
run::start  →  RunningModule  →  wait / shutdown
```

y `run()` es ahora `start` seguido de `wait`, así que el camino ordinario ejerce
el mismo código que el residente mantiene abierto.

Para poder guardarlo, `Confinement` se partió en `Held` —lo que el confinamiento
posee: el cgroup, la política escrita, el perfil— y el préstamo del almacén de
políticas. El préstamo se suelta; lo poseído no.

`RunRequest::wiring` dice qué pasa con los descriptores del módulo:
`Collected` es lo que todo módulo ha tenido siempre, y `Talks` le da al módulo
una tubería cuyo único escritor es Thalyx. Eso **no** es lo que la nota larga de
`launch::spawn` prohíbe: lo que ahí no puede pasar es que un módulo lea lo que
teclea la persona, porque entonces podría contestar una confirmación en su
nombre. Una tubería desde Thalyx es lo mismo que ya es el canal del descriptor 3.

### La pantalla deja de bloquearse

Un `Flow::Thinking` nuevo: cuando la línea no es un verbo, el dispatch **la
devuelve** en vez de gastar los siguientes segundos dentro de la llamada. La
pantalla pregunta en un hilo, dibuja `⠋ pensando…` con el reloj corriendo cada
120 ms, y cuando la respuesta llega la corre por el **mismo** dispatch en el hilo
que tiene el teclado.

El hilo trabajador recibe una raíz de store, un directorio y una frase, y nada
más: ni pantalla, ni teclado, ni sesión. Lo que produce es una propuesta. Eso no
es orden: [[Modelo-de-Amenaza]] deja al modelo fuera de la TCB, y un trabajador
que pudiera actuar sería el modelo pasando por encima de la confirmación.

Mientras hay una inferencia en vuelo el teclado **no** se lee. Hay un motor y una
inferencia a la vez, así que una segunda frase sólo se formaría en una cola; lo
que se teclee mientras tanto sigue en el buffer de la terminal cuando el bucle
vuelve. Lo que no puede parar es el dibujo, y no para.

### Precalentamiento

Cuando la sesión gráfica arranca, lanza un hilo que carga los pesos. Nada lo
espera: la pantalla ya está dibujada cuando ese hilo regresa, y una petición que
llegue a media carga toma el mismo candado y espera **la misma** carga en vez de
arrancar un segundo motor. Hay exactamente un motor, y eso es una propiedad del
tipo y no algo que los llamadores coordinen.

El panel `modelo` de la derecha dice `sin cargar`, `cargando…`, `listo` con el
pid y el costo de la carga, o la razón por la que no.

## La evidencia, y por qué se cuenta en procesos

Cesar lo dijo en estas palabras: *no me digas «persistent» porque existe un
objeto Rust persistente mientras el proceso sigue muriendo.*

Así que debajo de cada propuesta la sesión imprime `motor <pid> ▪ frío|tibio ▪
<s>`. **El mismo pid dos veces son dos frases contestadas por un proceso**, y
`tibio` al lado es ese proceso sin haber cargado los pesos otra vez.

Y la prueba se escribió igual: `the_engine_stays_alive` empaqueta el binario de
la propia prueba como módulo del motor, y ese motor de mentira **anota su pid en
un archivo al arrancar**. La afirmación es sobre cuántas líneas tiene ese
archivo, no sobre si la segunda frase se contestó.

Eso agarró el defecto que nada más habría agarrado, y está escrito en
[[Estrategia-de-Pruebas]]: `if let (false, Some(stale)) = (usable, held.take())`
evalúa las dos mitades de la tupla antes de comparar con el patrón, así que
`take()` corría siempre y el residente vivo se tiraba en cada llamada. Todas las
frases se contestaban bien. Lo único que cambiaba era el costo, que es justo lo
que esta fase existía para bajar.

## Si el motor se muere

Sin supervisor y sin reintentos infinitos. Si la tubería se rompe, si el marco no
se entiende, o si pasa el plazo: el residente se retira —se mata el proceso y se
saca el cgroup y la política del kernel— y **la siguiente petición lo arranca una
vez más**. Un segundo fallo es el que se reporta. Un bucle de reinicios volvería
un modelo que no puede cargar en una máquina que lo recarga para siempre.

## Lo que sigue sin estar probado

- **Confinado de verdad, residente.** El contenedor no tiene BPF LSM, así que lo
  medido aquí corrió `--unconfined`. La residencia y el confinamiento los
  establece la misma llamada —`run::start`— pero sólo el hierro de Cesar mira la
  segunda mitad de esa frase. §45 y §46 de `verify.sh`.
- **Cuánto baja de verdad.** `frío` contra `tibio` se imprime; nadie ha anotado
  todavía los dos números con un Qwen2.5-3B real y ocho gigas concedidos.
- **Que la pantalla se sienta viva.** Es una afirmación sobre pixeles: hay que
  arrancarla y mirarla. `make -C image run`, que desde hoy abre la interfaz
  gráfica en vez de una consola serie.

## Relacionado

- [[Motor-de-Inferencia-como-Modulo]] — el decreto que esto continúa
- [[Agente-Conversacional]] — qué es el agente, y que está fuera de la TCB
- [[Gamas-de-Modelo]] — las cuatro gamas y lo que cada una cuesta
- [[Sandbox-Ejecucion]] — `module_standard`, que no cambió
- [[Modelo-de-Amenaza]] — por qué el trabajador propone y no actúa
- [[La-Pantalla]] — la superficie que dejó de congelarse
- [[Estrategia-de-Pruebas]] — la regla que salió del defecto de la tupla
- [[Punto-Actual]] — dónde quedó todo
