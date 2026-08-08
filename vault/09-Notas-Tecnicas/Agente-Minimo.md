---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-03
tags: [agente, fase-1, alcance, pruebas]
---

# El agente mínimo

El primer `thalyx-agent` que se construye. **No es un agente general recortado:
es un agente de un solo caso de uso**, y esa restricción es el punto.

## Decreto

- **Un solo caso de uso: instalar un módulo.** Nada más. Ni consultar el grafo,
  ni recordar contexto, ni encadenar acciones.
- **El router de reglas va primero, el modelo después.** Lo que se puede
  resolver de forma determinista no se le pregunta al modelo.
- **El modelo nunca escribe la procedencia.** Ver [[Gamas-de-Modelo]].
- Corre con las gamas y la gramática de [[Gamas-de-Modelo]].

## Por qué instalar un módulo y no algo más barato

Consultar el grafo sería más fácil y más seguro: es de solo lectura, una
alucinación no destruye nada, y se puede iterar rápido. Precisamente por eso no
sirve. **No autoriza nada**, así que no ejercita el camino confiable ni la
procedencia por campo — no toca ninguna de las piezas cuyo diseño el agente
podría invalidar.

Instalar un módulo atraviesa el camino crítico entero: contrato estructurado,
procedencia por campo, confirmación por el [[Camino-Confiable|camino
confiable]], commit atómico. Es el caso ya trazado en [[Caso-Instalar-Modulo]] y
es el paso 2 del [[Criterio-de-Salida-Fase-1|criterio de salida]].

Si la procedencia por campo no sobrevive al pasar por inferencia, se rompe aquí
y se rompe pronto. Es la razón por la que el agente va antes que el ISO: el ISO
integra piezas ya probadas y no puede enseñar nada que no se sepa; este agente
sí puede invalidar un decreto.

## El router antes que el modelo

El modelo es la última opción, no la primera. `thalyx install foo@1.2.3` no
necesita inferencia, y pasarlo por un modelo solo agrega una forma de fallar a
algo que ya funcionaba.

La regla, que es el [[Principio-Doble-Ruta]] visto desde adentro:

> **Todo lo que el router resuelve solo, lo resuelve solo.** El modelo se invoca
> únicamente cuando queda ambigüedad que las reglas no cierran.

Efecto secundario que importa: en la gama ligera el router acierta exactamente
igual que en la máxima, porque no depende del modelo. La gama solo cambia lo que
pasa en la fracción ambigua.

## Dónde se puede probar qué

Esta es la división honesta, y se escribe aquí para que nadie la descubra tarde.
El contenedor de desarrollo **no tiene `llama.cpp` y no alcanza `huggingface.co`**
(la política de red del entorno responde 403 al CONNECT). No es una limitación
de máquina: hay RAM y disco de sobra.

| Pieza | Dónde se prueba |
|---|---|
| Router de reglas | Contenedor — es determinista |
| Ensamblado del contrato y procedencia | Contenedor |
| Que un modelo hostil no logre falsificar el origen | Contenedor, con el falso adversario |
| Enunciado → contrato → resolución → instalación | Contenedor, con bundles firmados de verdad |
| Que la gramática y el parser digan lo mismo | Contenedor — se comparan carácter por carácter |
| Que la respuesta se recorte del proceso correctamente | Contenedor, con procesos sustitutos |
| Que el plazo mate un proceso colgado, y el desborde | Contenedor, con procesos sustitutos |
| Que una herramienta que abre sesión en vez de completar se detecte | Contenedor, con un sustituto del `llama-cli` interactivo |
| Que la respuesta termine donde termina la gramática, y no donde deje de imprimir la herramienta | Contenedor, con **la salida capturada de una corrida real** |
| Que un sondeo distinga una gramática aplicada de una bandera aceptada | Contenedor, con un sustituto que **cambia de respuesta según la bandera** |
| Que `llama-completion` acepte las banderas | **Su máquina** — hecho el 2026-08-08 |
| El flujo completo con inferencia real | **Su máquina** — hecho el 2026-08-08, una gama, un enunciado |
| Que `--grammar-file` restrinja de verdad esa respuesta | **Su máquina** — `thalyx agent model grammar-check` |
| Acierto, latencia y RAM por gama | **Su máquina** — `thalyx agent bench` |

> **Revisión del 2026-08-08.** Las tres filas del medio son nuevas: antes el
> camino real no existía y todo lo que lo rodea caía del lado de «su máquina» por
> no tener con qué separarlo. Ahora sí se separa, y lo que queda ahí es más chico
> y está dicho con nombre — **si `llama.cpp` rechaza una bandera, lo dice y sale
> distinto de cero**, así que esa fila es una afirmación comprobable y no una
> zona gris. Ver [[Gamas-de-Modelo]].

### El gancho que existe por la regla 4

Mientras no haya modelo, todo intento de llegar al sistema por inferencia falla
con "no model is configured". Esa denegación **se ve idéntica** a la de la
comprobación de procedencia y no prueba nada sobre ella: es la regla 4, una
denegación sin control es indistinguible de algo que nunca funcionó.

Por eso existe `thalyx dev agent-probe`, que le pone al agente un modelo que sí
obedece a la página hostil. `verify.sh` lo corre con siete formas de portarse
mal y con el control —el mismo modelo, preguntado sobre algo que el humano sí
tecleó, que **debe** producir un contrato—. Sin ese control, un agente que
rechazara todo pasaría la etapa entera.

## El falso tiene que ser hostil

La regla 8 de `CLAUDE.md` dice que un falso debe modelar la propiedad bajo
prueba, y que uno que no la modela no es un falso sino otro sistema.

La propiedad bajo prueba aquí **no** es "el agente funciona cuando el modelo se
porta bien". Es:

> El agente no puede producir un contrato inválido, ni una procedencia falsa,
> **por mal que se porte el modelo**.

Así que un falso que siempre devuelve JSON correcto no prueba nada: prueba el
camino feliz de una pieza cuyo riesgo entero está en el camino infeliz. El falso
tiene que devolver, al menos:

- Basura que no es JSON.
- JSON válido con la forma equivocada.
- Forma correcta e intención equivocada.
- Un intento explícito de escribir campos de procedencia.
- Texto que trae dentro instrucciones tomadas del contenido no confiable que se
  le dio a resumir.
- Silencio, y una respuesta que nunca termina.

Cada uno de esos tiene que terminar en un rechazo del núcleo o en una petición
de aclaración. Ninguno puede terminar en una acción.

## Lo que este agente no hace

Escrito para que no se lea como una versión coja de algo, sino como su alcance:

- No encadena acciones.
- No busca en un repositorio remoto.
- No llama a modelos remotos, y en Fase 1 no puede.
- No compone las confirmaciones: las genera y las muestra el núcleo.

## Revisión del 2026-08-03 — dos cosas que aparecieron al construirlo

El decreto decía **quién** escribe la procedencia (el ensamblador, no el
modelo). No decía **cómo**. Al implementarlo apareció la regla, y con ella dos
consecuencias que no estaban previstas.

### La regla: un valor hereda la procedencia de donde aparece

El ensamblador no interpreta nada. Busca el valor propuesto en los segmentos que
el agente recibió y le da la procedencia del canal donde lo encuentra. Si
aparece en varios, **gana el más confiable**.

Esto último se escribió al revés primero, y estuvo mal medio día. El
razonamiento era que un id presente en lo tecleado *y* en una página es
ambiguo, así que hay que ser cauteloso. Pero no es ambiguo: el transcript sabe
en qué segmento llegó cada texto. Lo que la versión "cautelosa" hacía era
volver imposible de instalar por nombre cualquier módulo que apareciera en
cualquier página leída. No hay ataque en la otra dirección: para que un valor se
atribuya al humano tiene que aparecer en lo que el humano tecleó, y ese es el
único canal que un atacante no controla. Ver [[Estrategia-de-Pruebas]].

**Consecuencia no prevista: la alucinación deja de ser una cuestión de grado.**
Un valor que no aparece en *nada* de lo que se le dijo al agente no puede
recibir procedencia, y se rechaza. El modelo puede elegir entre las cosas que se
le dijeron; no puede agregar cosas nuevas. Eso no se buscó — salió de la misma
regla que contiene la inyección.

### La segunda: una operación no se puede atribuir buscándola

Un objetivo es un valor copiado del transcript y se puede buscar. Una
*operación* no: es una conclusión sacada de todo el transcript. Así que se
atribuye por lo que la conclusión pudo haber leído.

- Por el router, solo se lee lo que el humano tecleó. La conclusión es suya.
- Por el modelo, todo el transcript estuvo enfrente, y una conclusión sacada
  mientras se leía una página hostil es una conclusión que esa página tuvo
  oportunidad de moldear.

**De ahí sale una propiedad que el decreto no anticipó y que conviene decir en
voz alta: en cuanto hay texto ajeno en el transcript, el modelo ya no puede
originar una acción.** El humano sigue pudiendo instalar lo que quiera
tecleándolo, porque eso toma el camino del router y no se ve afectado.

Esa asimetría es el [[Principio-Doble-Ruta]] haciendo el trabajo para el que
existe: la ruta directa sigue abierta **precisamente** para que la ruta inferida
se pueda cerrar sin dejar a nadie sin salida. Sin doble ruta, esta defensa sería
inaceptable; con ella, no le cuesta nada al usuario.

### Revisión del 2026-08-03 (2) — la regla se abre por tarea

**Cómo se escribió esto la primera vez importa:** la propiedad de arriba se
presentó como un hallazgo elegante, y era una decisión de producto grande
tomada sin consultarla. El caso de uso más natural de un sistema donde la IA es
ciudadana de primera clase es *"lee esto y haz X"*, y esa regla lo volvía
imposible por la ruta del modelo. Segura y potencialmente inservible.

**Decreto:** la regla se mantiene **cerrada por defecto** y se abre con un
opt-in **por tarea, nunca global** — la misma forma que [[Agente-Conversacional]]
ya le da a las llamadas a modelos remotos. En la CLI es `--foreign-may-act`, que
no se recuerda entre invocaciones.

Lo que la concesión **no** concede, y es la mitad que no se mueve:

| | Sin concesión | Con concesión |
|---|---|---|
| El modelo puede actuar tras leer algo ajeno | No | **Sí** |
| El texto ajeno puede decir *qué* instalar | No | **No** |

Los objetivos se atribuyen igual en ambos casos, así que un id que solo aparece
en una página sigue rechazado, por el núcleo, antes de abrir nada.

> *"Lee esta página e instala lo que te diga"* sigue siendo imposible.
> *"Lee esta página y luego instala lo que yo nombré"* se vuelve posible.

Esa separación es lo que hace ofrecible la concesión. Sin ella, conceder "puedes
actuar tras leer" concedería en silencio "la página puede elegir", que es el
ataque entero.

## La memoria, y por qué se lee con un filtro

El agente **lee** su propia memoria, no solo escribe en ella: `thalyx agent
recall <tarea>`, y `agent plan`/`agent do --task <t>` traen ese contexto solos.
Una memoria que nunca se consulta no es memoria, es una bitácora.

Lo que se lee no entra como texto de nadie: **es estado de Thalyx**, así que
llega al transcript por el canal `Thalyx`, que sí puede tener efecto. Eso es lo
que hace posible que el agente actúe sobre algo que solo su propio registro
conoce.

Con un filtro, y aquí está la decisión. `Standing` tiene **tres** valores:

| Standing | Qué es | ¿Sirve de contexto? |
|---|---|---|
| `Unwitnessed` | *"me pediste X"* — un registro de habla | **Sí.** Nada en el disco puede confirmarlo ni desmentirlo, así que no tiene contra qué quedar obsoleto |
| `Verified` | *"X está instalado"*, y el disco sigue de acuerdo | **Sí** |
| `Unverified` | Lo estuvo y ya no | **No.** Se muestra, no se usa |

Un hecho que la memoria ya no puede comprobar **no es falso** —lo que describía
pudo cambiar por fuera de Thalyx— pero dejó de ser una afirmación sobre el
presente. Autorizar algo desde ahí es actuar sobre una creencia que el propio
sistema acaba de decir que no puede confirmar. Regla 9.

Y no deja a nadie atrapado: el humano siempre puede nombrarlo él y tomar la ruta
del router. Otra vez el [[Principio-Doble-Ruta]] siendo lo que hace pagable una
defensa así.

**La primera versión manejaba dos de los tres** y tiraba el tercero en silencio
— justo el que dice de qué iba la tarea. Se encontró corriendo `agent recall` y
notando que al resumen de la tarea le faltaba su propio sujeto. Van dos veces
que la memoria enseña lo mismo: los estados intermedios son los que se pierden.

## Relacionado
- [[Gamas-de-Modelo]] — qué modelo, cómo corre, qué garantiza la gramática
- [[Principio-Doble-Ruta]] — por qué se puede cerrar la ruta inferida
- [[Agente-Conversacional]] — el agente completo al que este apunta
- [[Caso-Instalar-Modulo]] — el caso trazado que implementa
- [[Marcado-de-Origen]] · [[Camino-Confiable]] · [[Contrato-Estructurado]]
- [[Estrategia-de-Pruebas]] — por qué el falso tiene que ser hostil
