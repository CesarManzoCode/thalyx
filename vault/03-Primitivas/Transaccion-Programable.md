---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-08-30
tags: [primitiva, agentes, transaccion, programa, control-de-flujo, fase-1]
---

# Transacción programable: el programa que corre aquí

## Función

Que **una sola inferencia del modelo entregue un programa corto cuyo flujo de
control continúa localmente** —variables, ciclos, condiciones, filtrado,
aserciones y operaciones dependientes— y que el modelo sólo vuelva a
participar cuando el programa termina, cuando pide explícitamente un juicio, o
cuando de verdad no puede seguir.

Es la P de TPV. [[Ejecucion-Transaccional]] ya daba la T y la V.

## Lo que faltaba, dicho exactamente

`hacer` tomaba un `Vec<Step>`: **el modelo tenía que saber cada operación y
cada argumento antes de que corriera nada.** Eso es *batching*, y el batching
no puede expresar lo que un agente de verdad gasta sus turnos haciendo:

> preguntar, mirar la respuesta, decidir qué sigue.

Un rename sobre las referencias que una consulta acaba de devolver. Una edición
aplicada sólo a las que tienen cierta cosa alrededor. Una validación cuyo
resultado decide si lo siguiente ocurre. Nada de eso se puede escribir por
adelantado —para escribirlo habría que ya tener la respuesta, que es
precisamente el trabajo—. Así que cada una de esas decisiones era un viaje
redondo al modelo frontera, arrastrando la conversación entera.

## Qué es

```
hacer {"label":"sólo lo que hace falta",
       "run":"const listado = thalyx.list('src');
              const tocados = [];
              for (const entrada of listado.entries) {
                  const fuente = thalyx.read('src/' + entrada.name);
                  if (fuente.text.includes('old_api')) {
                      thalyx.mustWork(
                          thalyx.substitute('src/' + entrada.name, 'old_api', 'new_api'),
                          'la sustitución no ocurrió');
                      tocados.push(entrada.name);
                  }
              }
              const visto = thalyx.changed();
              thalyx.assert(visto.count === tocados.length, 'el árbol dice otra cosa');
              const check = thalyx.validate({check: 'rust'});
              if (check.verdict !== 'passed') { return {rendido: check.summary}; }
              return {cambiados: tocados};"}
```

Una petición externa. Adentro: un listado que nadie conocía, un ciclo sobre él,
una lectura por entrada, una decisión por lectura, mutaciones sólo donde la
lectura lo dice, una observación de lo que el árbol de verdad muestra, una
validación, una rama sobre la validación, y una respuesta chica. Ni una sola
inferencia en medio.

## El lenguaje: JavaScript sobre QuickJS

**El lenguaje no es sagrado; las propiedades sí.** Lo que decidió:

1. **Un modelo frontera ya lo escribe.** Lo único que un lenguaje de scripting
   inventado para Thalyx garantiza es que todo modelo que lo use está
   escribiendo un lenguaje que nunca vio, a partir de una descripción en un
   esquema de herramienta. Ésa es la forma más cara posible de gastar
   exactamente la atención que este mecanismo existe para ahorrar. JavaScript
   cuesta cero prompt.
2. **QuickJS no tiene autoridad ambiente que quitarle.** Su núcleo es un
   lenguaje y nada más: no hay `fs`, no hay `net`, no hay `process`, no hay
   `require`, no hay `fetch`. Un programa arranca pudiendo hacer aritmética, y
   lo único que alcanza es lo que Thalyx le ata. Eso es **lo contrario** de
   incrustar un shell, donde el trabajo sería *restarle* autoridad a algo que
   empieza con toda — y restar es la dirección que falla en silencio.
3. **Es C99 sin dependencias**, así que se compila dentro del binario estático
   de musl. Regla 12: el binario que se verifica tiene que ser el binario que
   se envía, y un runtime que necesitara una biblioteca compartida no podría
   estar en la imagen. Es el mismo argumento que el workspace ya hace para
   compilar SQLite adentro.
4. **Se puede detener.** Un manejador de interrupción, un techo de memoria y un
   techo de pila son parte del motor, así que `while (true) {}` termina por una
   razón y no por suerte.

**No se inventó un DSL, y no se corre un shell.** Ninguna de las dos era una
opción: la primera por (1), la segunda por (2).

## El programa no es la autoridad

Es código no confiable escrito por un modelo de lenguaje. Todo lo que puede
hacer lo hace llamando a `Machine`, que se implementa **arriba** de
`thalyx-program`, en quien es dueño de la transacción:

- **`request`** es `external::one` — la misma función, el mismo chequeo de
  argumentos, la misma frontera del espacio de trabajo por la que pasa una
  petición sola. Un programa no es una manera de alcanzar un verbo que no está
  expuesto, ni una ruta afuera, ni un slot de argumento con el contenido de
  otro.
- **`validate`** es `run_check` — el mismo verificador de la lista declarativa.
- **`changed`** es `thalyx_snapshot::difference` contra el mismo snapshot.

No hay un segundo store, ni un segundo verificador, ni una segunda frontera. Lo
que un programa alcanza es exactamente la unión de lo que sus llamadas habrían
alcanzado una por una.

El motor corre en **su propio hilo** y le habla a la máquina por un canal. Dos
razones, y la segunda es la que decidió: `rquickjs` ata cerraduras `'static` y
la máquina que una transacción real entrega presta un store, una sesión y un
espacio de trabajo — un canal es la manera ordinaria de prestarle un préstamo a
un hilo, y la alternativa era `unsafe`, que en este repositorio vive en
`thalyx-syscall` y en ningún otro lado. Y: el código no confiable tiene su
propia pila.

### Dos verbos que un programa no alcanza

`hacer` y `intento`. La forma estática los rechaza *por nombre* antes de tomar
el snapshot, que es el lugar correcto para una lista: una lista es un valor que
algo puede mirar. **Un programa no.** Alcanza verbos por nombre en tiempo de
ejecución, así que el chequeo tiene que estar en el momento de la llamada.

Encontrado el 2026-08-30 por una prueba que pidió `intento abandonar` desde
adentro de un programa y recibió `ok: true` con una línea `confirm_with` que
cargaba el nombre del snapshot y el testigo de estado exacto que la máquina
acababa de calcular. Dos líneas más y la transacción se habría abandonado a sí
misma, a media corrida, con el runtime todavía sosteniendo una frontera que ya
no existía. La negativa ahora no entrega nada con qué reintentar.

## Detenerse se hace cumplir dos veces

Una aserción que sólo lanzara una excepción de JavaScript puede ser atrapada por
el programa que la falló —`try { thalyx.assert(false) } catch {}`— y la corrida
seguiría más allá de la cosa que debía terminarla. Un programa escrito por un
modelo de lenguaje es exactamente el tipo de programa que envuelve todo en
`try`/`catch`.

Así que una aserción fallida **queda trabada**: se registra del lado de Rust,
lanza, y desde ese momento el manejador de interrupción detiene el motor y toda
llamada al anfitrión se niega. Un programa no puede atrapar su camino más allá
de una detención, porque lo que lo detiene no está en el lenguaje.

`thalyx.needModel(valor)` funciona igual, y **no es un fracaso ni un éxito**: la
transacción devuelve el árbol, así que un programa que se topó con una decisión
que no iba a tomar no deja nada atrás. Es la forma en que se contesta una
ambigüedad de [[Semantica-Compilada]].

## Techos, porque un lenguaje puede dar vueltas

`MOST_STEPS` acotaba la forma vieja por construcción. A un programa no lo acota
nada más que contar, y cada recurso lleva su propio techo porque una máquina que
tiene tiempo de sobra y no memoria debe poder decir cuál:

| techo | qué detiene |
|---|---|
| `wall` | `while (true) {}`, por el manejador de interrupción del motor |
| `ticks` | lo mismo, en unidades que no dependen de qué tan ocupada está la máquina |
| `memory_bytes` | un programa que asigna sin parar |
| `stack_bytes` | recursión sin fin, como negativa y no como este proceso cayéndose |
| `calls` | un ciclo sobre peticiones — el sucesor de `MOST_STEPS` |
| `process_launches` | una explosión de procesos por la vía lenta: validar en un ciclo |
| `answer_bytes` | cuánto puede *entrar* el programa; no es el presupuesto del modelo |
| `returned_bytes` | cuánto puede *salir* — **negado, nunca cortado** |

Lo último es una decisión y no un detalle: una respuesta cortada a la mitad es
una respuesta sobre la que un modelo actúa creyendo que está completa. Toda
está en la evidencia de cualquier manera.

Un techo alcanzado produce evidencia estructurada y **rollback por defecto**.

### La interrupción es atrapable, y eso importó

QuickJS levanta una interrupción como una excepción ordinaria de JavaScript, o
sea que el programa —o el propio `catch` del envoltorio— la puede atrapar. La
primera versión reportaba `while (true) {}` como *«el programa lanzó»*, que es
una oración sobre el programa en lugar de sobre el techo que alcanzó. Ahora el
manejador marca que decidió detener, y esa marca le gana a lo que el motor haya
alcanzado a decir. Está en [[Estrategia-de-Pruebas]] como regla.

## Qué cruza de vuelta

`returned`, y nada más: lo que el programa decidió que importaba. Todo lo demás
—archivos enteros, cada referencia, toda la salida del compilador, la respuesta
completa de cada llamada— se queda adentro bajo el asa de `evidencia`.

Cada llamada del programa queda en la evidencia como un **paso**, con la misma
forma que produce la lista estática, así que `evidencia <id> paso=N` trae la
novena operación de un programa igual que trae el noveno paso de una lista. Una
forma de «qué pasó», no dos.

## Cómo se decide confirmar o devolver

1. El programa tiene que haber **terminado** — `returned`. `needs_model`,
   una aserción, una excepción o un techo, no.
2. Toda validación tiene que haber pasado, contando **el último veredicto por
   check**. Lo último es por el patrón que un programa hace y una lista no
   puede: validar, ver que falla, arreglar, validar otra vez. Una transacción
   que devolviera el árbol porque un intento anterior falló haría ese patrón
   imposible de escribir.
3. `not_proven` nunca cuenta como pasar.

`on_failure: "keep"` deja el árbol fallado en su lugar, y se nombra distinto de
un commit.

## Lo que esto NO es

- **No es aprendizaje de tasklets.** Nada se promueve, nada se recuerda como
  habilidad, no hay ejecución especulativa ni varios rollouts. Primero una
  ejecución correcta y poderosa.
- **No es un framework de plugins.** Hay un runtime y un proveedor semántico.
- **No es una manera de agregar verbos.** La manera de darle más alcance a un
  programa es exponer más verbos a `external::one`, que es una lista en un lugar
  que una persona puede leer.

## Lo que está probado y lo que es hipótesis

**Probado**, con contadores y con bytes: que el flujo de control depende de los
datos (`the_answer_of_one_operation_decides_whether_a_third_runs`, corrido sobre
dos árboles donde el programa es idéntico y la rama es distinta); que un árbol
de cinco módulos donde tres usan `old_api` —y cuáles tres no se ve en los
nombres— se resuelve en una petición; que una validación fallida devuelve el
árbol byte por byte; que un programa que pide el modelo no cambió nada; que los
techos detienen; que una aserción no se puede atrapar.

**Hipótesis**: que todo esto haga que Claude o Codex hagan más trabajo correcto
con menos esfuerzo de modelo. No hay banco pagado corrido contra esto.
