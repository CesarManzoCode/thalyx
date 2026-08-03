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
| Que la gramática GBNF sea válida y `llama.cpp` la acepte | **Su máquina** |
| Acierto, latencia y RAM por gama | **Su máquina** |
| El flujo completo con inferencia real | **Su máquina** |

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
- No recuerda entre sesiones — eso es el paso 6 del criterio de salida y llega
  después, sobre [[Memoria-Persistente]], que ya está construida.
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
aparece en varios, gana el menos confiable.

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

## Relacionado
- [[Gamas-de-Modelo]] — qué modelo, cómo corre, qué garantiza la gramática
- [[Principio-Doble-Ruta]] — por qué se puede cerrar la ruta inferida
- [[Agente-Conversacional]] — el agente completo al que este apunta
- [[Caso-Instalar-Modulo]] — el caso trazado que implementa
- [[Marcado-de-Origen]] · [[Camino-Confiable]] · [[Contrato-Estructurado]]
- [[Estrategia-de-Pruebas]] — por qué el falso tiene que ser hostil
