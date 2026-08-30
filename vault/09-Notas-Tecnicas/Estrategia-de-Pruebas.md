---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-01
tags: [pruebas, ci, atomicidad, evidencia]
---

# Estrategia de pruebas

## Dos lectores de la misma entrada, y uno que guarda lo que sobra

**Descubierta el 2026-08-09, colgando la suite entera de `exit_criterion`.**

Un `read` sobre la entrada devuelve **todo lo que haya llegado**, no una línea.
El editor de línea nuevo guardaba lo que sobraba para la siguiente vuelta, que es
lo correcto — sin eso, teclear por adelantado pierde todo lo que sigue al primer
Return.

Pero `instalar` pide una confirmación, y esa confirmación leía `stdin` **por su
cuenta**. La `y` que la contestaba ya estaba en el búfer del editor: fuera del
kernel, dentro del proceso, invisible para quien la esperaba. Seis sitios del
CLI leían `stdin` directo.

> **Dos lugares que leen la misma entrada, y uno que guarda lo que sobra, no
> pueden coexistir.** El segundo espera para siempre bytes que ya se leyeron.

Lo que la hace difícil de ver es que **el síntoma no es un error, es un
silencio**: nada falla, nada se imprime, el proceso simplemente no vuelve. Y no
la encuentra ninguna prueba unitaria, porque cada mitad es correcta por separado.

La corrección no es coordinar los dos: es que haya **un solo dueño de la
entrada**. `term::read_answer()`, y los seis sitios pasan por él.

## Por qué existe esta nota

La atomicidad del commit y la garantía de rollback son **las afirmaciones centrales de Thalyx**. Hasta el 1 de agosto de 2026 nada en la bóveda verificaba ninguna de las dos: eran diseño, no evidencia.

## Los tres niveles (obligatorios)

### Nivel 1 — Unitarios y de propiedades

Sobre el resolver de versiones, el validador de contratos y el parser mecánico. Los tres tienen entrada y salida deterministas, así que se prestan a pruebas basadas en propiedades y no solo a casos de ejemplo.

### Nivel 2 — Inyección de fallos

**Es el nivel que no puede faltar.**

Se mata el proceso en cada punto intermedio del commit y se verifica el invariante:

> **Publicado o no publicado. Nunca a medias.**

Puntos de corte obligatorios, como mínimo:

- Durante la producción del artefacto en el área de staging.
- Entre la verificación y el primer `rename`.
- **Entre el `rename` del directorio y el `rename` del enlace simbólico** — el instante donde la publicación está a mitad de camino.
- Durante la escritura del journal.
- Durante el registro efectivo de los permisos confirmados.

Incluye corte de energía simulado en QEMU, no solo `SIGKILL` al proceso: un `SIGKILL` no ejercita el comportamiento del filesystem ante pérdida de energía.

### Nivel 3 — End-to-end en CI

El [[Caso-Instalar-Modulo|caso canónico]] completo, ejecutado en QEMU dentro de integración continua, en cada cambio.

## Qué encontró el nivel 2 la primera vez que se ejecutó

Los tres niveles están implementados desde el 1 de agosto de 2026 para la ruta de instalación. En su primera ejecución, el nivel 2 encontró **dos defectos reales** que ninguna revisión de diseño había detectado, y que ilustran por qué existe:

1. **El reintento tras un corte a mitad del commit fallaba con `ENOTEMPTY`.** El directorio de la versión ya estaba en su lugar definitivo, así que el `rename` siguiente no tenía dónde aterrizar. El invariante de atomicidad se cumplía —nada quedaba a medio publicar— pero el sistema quedaba irrecuperable sin intervención manual. **Consistente no es lo mismo que utilizable**, y solo un test que provoque el corte y después reintente distingue las dos cosas.

2. **La interfaz mostraba permisos vigentes para un módulo no instalado.** El núcleo condicionaba bien la vigencia, pero la capa de presentación leía el registro crudo. El humano veía dos permisos persistentes de red que no existían. Es exactamente el permiso huérfano que [[Permisos-JIT]] prohíbe, agravado: no era un fallo de seguridad interno, era mentirle al soberano sobre lo que había autorizado.

El segundo apareció ejecutando el sistema a mano, no en un test — lo que dice algo sobre no fiarse solo de la suite. El primero sí lo encontró un test, y solo porque el test comprobaba **recuperación** además de consistencia.

De ahí una regla derivada: **un test de nivel 2 no termina al verificar el invariante. Termina al verificar que el sistema todavía sirve.**

## Regla derivada: un test tiene que poder ver lo que mide

Al probar el enforcement del LSM por primera vez, el diagnóstico falló tres veces seguidas por el instrumento, no por lo medido:

1. La sonda usaba `curl -s`, que silencia justo los mensajes de error contra los que comparaba.
2. Aun sin `-s`, curl reporta "permiso denegado" y "conexión rechazada" con el mismo texto — la distinción sobre la que descansaba toda la prueba era invisible.
3. La comprobación de si el programa estaba enganchado corría sin privilegios contra un directorio de modo 700, así que reportaba "no atado" para algo que sí lo estaba.

Los tres se veían como fallos del sistema. Ninguno lo era.

De ahí dos reglas:

**Un test que no distingue el fallo que busca del fallo de fondo no es un test.** Si el resultado esperado y el ruido ambiente producen la misma salida, no hay medición.

**Todo test de denegación necesita una línea base y un control.** La línea base es la misma operación antes de activar la restricción; el control es la misma operación fuera de su alcance. Sin la primera, una denegación y una operación que nunca funcionó se ven igual. Sin el segundo, una política que rompe todo se ve igual que una que funciona.

**Y la misma forma aparece donde el éxito es "nada cambió".** El 2026-08-28, al
escribir la tarea `reversible` del arnés de comparación —cambiar un símbolo en
varios archivos y después dejar el árbol byte por byte como estaba— quedó claro
que **un agente que no hace absolutamente nada pasa la comprobación perfecta**.
El hash de antes y el de después son iguales tanto para el que cambió todo y lo
devolvió como para el que se rehusó. Un veredicto leído de ahí premia la
inacción, y la premia más en el brazo que se está tratando de probar. El arreglo
es el de siempre: el veredicto es una conjunción, y la parte que distingue los
dos casos viene de otro instrumento —el stream del propio agente dice si el
nombre nuevo apareció alguna vez en una llamada. `dev/bench-summary.py
--self-test` tiene ese caso escrito con nombre y apellido.

## Regla derivada: un test que se salta tiene que decir que se saltó

Buena parte de lo que Thalyx hace solo se puede probar contra un kernel real: un cgroup, un mapa BPF, un `rename` sobre el filesystem verdadero. En una máquina donde eso no está, un test así no puede correr.

Lo que **no** puede hacer es pasar en silencio. Un `ok` que no ejercitó nada es indistinguible de un `ok` que ejercitó todo, y esa confusión ya nos costó una vez: una herramienta de seguridad se leyó como desarmada estando armada.

La regla: **el test imprime `NOT PROVEN`, dice explícitamente que no corrió y por qué, y existe una variable de entorno que convierte el salto en fallo.** En la máquina del desarrollador se salta; en CI, donde el entorno sí está, no se le permite.

**Una variable por requisito, no una para todos.** Empezó habiendo una sola (`THALYX_REQUIRE_CGROUP_TESTS`) que cubría cgroup2, namespaces, el LSM de BPF y los controladores delegados. El resultado es que la única forma de exigir lo que una máquina sí tiene era exigir también lo que no tiene — o sea, no exigir nada. Ahora son tres, y el contenedor de desarrollo pasa la suite entera con los saltos de cgroup y namespaces prohibidos.

**Y la guarda se ata a lo que dice el fallo real.** Un test reportó `NOT PROVEN` para una corrida que sí había funcionado, porque su guarda buscaba una palabra que también aparecía en un mensaje inofensivo de un proceso auxiliar. Un salto que se dispara solo es peor que no tener salto: se ve exactamente igual que un entorno incapaz.

## Regla derivada: un test de instalación tiene que intentar usar lo instalado

Al conectar la ejecución de módulos apareció un defecto que la suite completa había dejado pasar: **el desempaquetador nunca aplicaba el modo del archivo**, así que todo módulo se instalaba con su entrypoint no ejecutable. Ningún módulo podía correr jamás.

Había pruebas de que los archivos llegaban, de que la ruta era la correcta, de que el contenido coincidía. Ninguna intentaba ejecutarlo.

Es el mismo patrón que ya había aparecido con el commit atómico —*consistente no es lo mismo que utilizable*— en otra capa. La generalización:

**Una prueba de que algo se produjo correctamente no es una prueba de que sirve.** Todo test que verifique un artefacto tiene que terminar usándolo para lo que existe.

De paso, arreglarlo abrió una decisión que nadie había tomado: si se honra el modo del archivo, un módulo podría enviar un binario setuid y escalar privilegios pasando por encima de todos los permisos que el humano confirmó. El modo se aplica **enmascarado**: setuid, setgid y sticky nunca sobreviven.

## Regla derivada: preguntarle al sistema si funcionó no prueba nada

Al construir el aislamiento del sandbox, la tentación obvia era verificar que Thalyx *dice* haber creado los namespaces. Eso no prueba nada: la clase entera de defecto que este proyecto encuentra es el sistema reportando éxito de trabajo que no hizo.

Cada prueba de aislamiento le pregunta **al programa confinado qué ve**, y lo compara con lo que ve el proceso de prueba:

```
                        adentro          afuera
pid                     1                6878
hostname                thalyx-module    vm
procesos visibles       4                82
interfaces de red       lo:              lo: eth0:
```

La columna de afuera no es decorativa: sin ella, "el módulo ve una sola interfaz" también sería cierto en una máquina que solo tiene loopback. Es la misma regla del control que ya salió con el LSM, aplicada a otra capa.

Y el complemento obligatorio: **una prueba de que todo está denegado necesita una prueba de que algo funciona.** Un allowlist vacío pasaría todas las pruebas de denegación y sería inútil. Por eso existe `ordinary_programs_still_run_under_the_filter`.

## Regla derivada: usar el instrumento del kernel, no uno de afuera

La lista de syscalls permitidas se derivó ejecutando programas reales bajo el filtro y leyendo **la línea del log de auditoría del kernel**, que nombra el syscall exacto y el proceso que lo pidió.

`strace` no habría servido igual: traza desde fuera del sandbox, y lo que hace falta saber es qué llamada mató el filtro adentro. Adivinar la lista de memoria falló en las dos direcciones a la vez — le faltaban `statfs`, `fadvise64` y `copy_file_range`, y sobraban cosas que nadie pidió.

**Cuando el kernel ya te está diciendo qué pasó, léelo antes de instrumentar por fuera.**

## Regla derivada: el arnés también es un instrumento, y también miente

La primera corrida completa en hardware real —Fedora 43, kernel 7.0, con cgroup2, todos los controladores y `bpf` en el orden de LSM— reportó dos fallos. **Ninguno de los dos era de Thalyx.**

1. **`... | tee log | grep -q patrón` bajo `pipefail`.** `grep -q` sale en cuanto encuentra la coincidencia, el extremo que escribe recibe `SIGPIPE`, y el estado de la tubería es el fallo de un comando que ya había hecho su trabajo. Resultado: el script reportó que la demostración de enforcement *no* había probado la denegación, en una corrida cuyo log decía `ENFORCEMENT IS REAL`.

2. **Un arnés de prueba que no montó el escenario que necesitaba.** Los tests creaban una jerarquía de cgroups artificial y no le delegaban controladores. Los límites de recursos fallaban, y el fallo se veía como un defecto de Thalyx en algo que el arnés no había hecho.

3. **Un test que infería su propia precondición.** "El módulo no corrió" se leía como "Thalyx rechazó correr sin enforcement", pero una corrida que fallara por *cualquier otra* razón se veía igual. En una máquina con el LSM cargado se leyó como un fallo. Ahora la precondición se comprueba directamente: ¿existe el mapa de políticas?

La generalización:

**Un fallo del instrumento se ve exactamente igual que un fallo del sistema, y el instrumento incluye al arnés.** Antes de creer que algo que Thalyx afirma es falso, hay que descartar que el que se equivocó fue el que preguntó. Las tres veces anteriores que esto pasó fue con sondas de red y con permisos de bpffs; esta vez fue con una tubería de shell y con un directorio que nadie preparó.

**Y van catorce.** Cada una tiene su regla abajo, porque el mecanismo es distinto
cada vez — las tres del 2026-08-07, las dos del 2026-08-24 y una del 2026-08-25:

- **La octava**, el parser de headers que descartaba campos con comentario al
  final de la línea — está justo debajo.
- **La novena**, el control de la etapa 18 que dañaba espacio libre y acusaba al
  kernel de montar basura. Ahí el arnés no se equivocó al preguntar: se equivocó
  en *cuándo* sacó la muestra.
- **La décima**, clippy fallando en una máquina y pasando en la otra contra el
  mismo código. Ahí el arnés era el correcto y tenía **otra versión**, que es un
  arnés distinto.
- **La undécima**, `verify.sh` buscando en la salida de la sonda una frase que
  el programa había dejado de imprimir ese mismo día. Siete comprobaciones
  pasaron por vacías y el control positivo fue lo único que lo notó.
- **La duodécima**, `foreign-agent-needs.sh` leyendo la lista de llamadas
  permitidas del archivo entero, que también nombra las que un módulo tiene
  prohibidas. Contaba como concedido lo que el archivo niega.
- **La decimotercera**, una prueba que corría `chrt --other` para preguntar si un
  módulo puede acomodar sus hilos. La respuesta dependía de la versión de
  util-linux y no del filtro: hasta 2.40 hace la llamada guardada, desde 2.41
  hace otra que Thalyx deniega. Es la regla de la versión otra vez, abajo.
- **La decimocuarta**, el 2026-08-26, y es la más barata de todas: se leyó
  `origin/main` **sin haber hecho `fetch`**, o sea la copia local del puntero
  remoto, que llevaba días vieja. De ahí salió un diagnóstico completo —«el
  trabajo de G1 no está en `main`, por eso nadie lo ha podido correr»— que era
  falso entero: `main` ya lo tenía. Es la misma falta que ya está escrita abajo
  con las palabras *«`main` y `origin/main` son dos preguntas distintas»*, y
  volvió porque a esa frase le faltaba la mitad operativa: **`origin/main` sólo
  es una pregunta sobre el repositorio después de un `fetch`.** Antes de eso es
  una pregunta sobre la última vez que esta máquina miró.

Y la octava, con detalle: el parser que comprueba los offsets de
`thalyx-btrfs` contra el header capturado **descartaba en silencio todo campo con
un comentario al final de su línea**, porque quitaba el `;` sin quitar el `/* … */`
que venía después. Reportó `btrfs_root_item` de 343 bytes cuando el escritor
producía 439. El escritor tenía razón —la imagen real de `mkfs.btrfs` dice 439— y
sólo se pudo saber porque había una muestra capturada contra la que graduar al
que preguntaba. Por eso `tests/layout.rs` ahora tiene una prueba que **comprueba
al parser antes de medir nada con él**, usando los dos tamaños que los headers
afirman en su propio texto.

Y un corolario que sale de la corrida: **un flag que no hace nada es el mismo fallo que un permiso que no aplica nada.** `--unconfined` solo tenía efecto cuando el kernel no podía aplicar políticas, así que en una máquina donde el enforcement funcionaba el flag se ignoraba en silencio. El sistema hacía algo distinto de lo que se le pidió y no lo decía en ninguna parte.

## Regla derivada: una medición de un solo lado no necesita una máquina en silencio

El contador de mutaciones del kernel cuenta lo de toda la máquina, así que comprobar que un hook nuevo funciona parecía exigir un equipo quieto: cualquier ruido de fondo mueve el número y arruina la comparación.

No lo exige, si la medición se diseña **de un solo lado**. El arnés abre un descriptor, toma la cuenta, escribe cinco mil veces por ese mismo descriptor sin crear, renombrar ni borrar nada, y vuelve a tomarla. Si el hook está enganchado, el delta es de al menos cinco mil, garantizado. Si no lo está, esas escrituras son invisibles y el delta tendría que venir entero del ruido ambiente — cinco mil mutaciones contadas en un segundo, en la máquina que está corriendo el arnés.

El ruido **solo puede sumar**. Esa asimetría es lo que hace la prueba utilizable sin controlar el entorno, y es la misma forma del razonamiento que ya gobierna el contador entero: contar de más cuesta un recorrido que no hacía falta, contar de menos cuesta corrección.

**Cuando el ruido solo puede empujar en una dirección, se elige el umbral para que la dirección contraria sea la que responde la pregunta.**

## Regla derivada: un fixture inventado prueba lo que yo entendí, no lo que la herramienta imprime

El lector del contador del kernel tenía nueve pruebas y todas pasaban. En hardware real falló a la primera.

`bpftool` imprime los mismos números **dos veces**: como arreglos de bytes, y otra vez bajo `formatted`, donde un valor es un entero suelto (`"value":1676`). Ninguno de mis fixtures traía esa sección, porque yo no sabía que existía — los escribí desde lo que creía que la herramienta imprimía. El lector buscaba el siguiente `[` después de cada `"value"`, así que en la sección `formatted` leía corchetes que pertenecían a otras entradas, y en la última no encontraba ninguno.

Resultado: un watcher que estaba contando perfectamente se reportó ilegible.

**Todo parser de la salida de una herramienta externa necesita al menos un caso capturado de esa herramienta, verbatim.** Un fixture escrito a mano prueba que el parser coincide con mi modelo del formato; solo la salida real prueba que coincide con el formato. Ahora la corrida real está pegada en el test, con su suma comprobada contra la vista `formatted` que la propia herramienta imprime.

Y el corolario, que es la quinta vez que este proyecto lo aprende en otra capa: **un fallo al leer se reportó como un fallo al existir.** `thalyx graph watcher` trataba cualquier error como "el watcher no está cargado", así que mandaba al humano a recargar algo que llevaba todo el rato funcionando. Ahora distingue las dos cosas y dice cuál es: una es algo que ir a arreglar afuera, la otra es un defecto de Thalyx.

## Regla derivada: un decreto citado de memoria puede salir invertido

El 2026-08-03, al descartar un punto de una revisión externa, se citó
[[Decision-Capa-vs-SO-Nuevo]] como *"el proyecto decidió empezar como capa sobre
Linux"*. El decreto dice, literalmente y marcado como no negociable, que Thalyx
**no es una capa** — y una revisión anterior había eliminado esa palabra del
vocabulario del proyecto para evitar exactamente esa lectura.

No fue un matiz perdido: fue la afirmación al revés, dicha con confianza, para
rechazar una crítica que en su parte factual tenía razón.

Es la misma forma que el fallo de atribución de ese mismo día, en otra capa. Ahí
una decisión equivocada quedó protegida por una justificación convincente; aquí
un decreto correcto quedó reportado como su contrario por un resumen convincente.
En ambos casos **lo que falló no fue el conocimiento sino la verificación**, y en
ambos el síntoma externo fue el mismo: sonaba bien.

Regla, y aplica a las notas tanto como al código:

> **Un decreto que se va a usar para cerrar una discusión se abre y se cita.**
> No se parafrasea de memoria. Si la conclusión de un argumento es "esto ya está
> decidido", el archivo tiene que estar abierto mientras se escribe la frase.

El costo de abrirlo son diez segundos. El costo de no abrirlo, ese día, fue
archivar como defecto una implementación correcta y descartar como error una
observación válida.

## Regla derivada: la superficie previa a la comprobación se encuentra midiendo, no leyendo

Una revisión externa entregó una lista de una docena de riesgos del desempaquetado
de tar: enlaces duros, symlinks que escapan tras extraer, nodos de dispositivo,
FIFOs, bombas de descompresión, límites de entradas, rutas duplicadas, nombres
que no son UTF-8. Buena lista.

Al comprobarlos uno por uno contra el código, **casi todos ya estaban cerrados**
desde hacía semanas. Y el que sí estaba abierto no aparecía en la lista: los
miembros del bundle que Thalyx *ignora* se leían enteros a memoria antes de
decidir que se ignoraban. 768 MB de archivo sin firma → 1 GB de RSS, antes de
consultar ninguna clave.

Dos reglas de esto:

1. **La pregunta que encuentra estos fallos no es "¿qué comprueba el código?"
   sino "¿qué corre *antes* de la comprobación?"** Todo lo que se ejecuta para
   poder verificar algo se ejecuta sobre un archivo que todavía no se verificó,
   y por lo tanto sobre un archivo de un desconocido.
2. **Una lista de riesgos genéricos es una lista de hipótesis, no de hallazgos.**
   Cuesta un rato comprobarlas y el rendimiento es bajo — aquí, once de doce ya
   estaban resueltas. Lo que rindió fue construir la bomba y medir el proceso.
   Un riesgo enunciado y un riesgo demostrado se parecen mucho en un documento y
   en nada en un arreglo.

Corolario para las revisiones externas, que ya van dos veces: **son valiosas y
hay que verificarlas igual que a Thalyx.** La misma revisión que encontró de
verdad cinco contradicciones de la bóveda falló al restar seis horas de huso
horario y listó once riesgos ya cerrados como si estuvieran abiertos.

## Regla derivada: los mutantes prueban que las pruebas sostienen algo, no que sostengan lo correcto

Esta es la continuación incómoda de la regla de abajo, y llegó el mismo día.

La atribución del agente decidía que un valor presente en **dos** canales
tomaba la procedencia del **menos** confiable. Tenía su prueba
(`a_value_in_two_places_takes_the_less_trusted_one`), tenía un comentario
seguro de sí mismo —"los dos son indistinguibles desde aquí, y la regla 9 dice
tomar la respuesta cautelosa"— y cuando se rompió el mecanismo a propósito, la
prueba falló como debía. Todo en verde y todo coherente.

**Estaba mal.** No son indistinguibles: el transcript registra en qué segmento
llegó cada texto, y el del humano está ahí. No se estaba adivinando nada. Lo
que esa regla hacía de verdad era volver **imposible de instalar por nombre**
cualquier módulo mencionado en cualquier página que el agente hubiera leído: el
humano teclea `install dev.thalyx.demo`, un README lo menciona, y Thalyx rechaza
la instrucción de su propio soberano. Eso no es cautela, es un desconocido
sobrescribiendo al dueño de la máquina.

Se encontró **tecleando una frase en la CLI**, tres segundos después de que
existiera el comando. No lo encontró ninguna de las 39 pruebas, ni los tres
mutantes, ni el ejercicio de la regla de abajo.

> **Un mutante demuestra que una prueba es portante. No demuestra que la
> decisión que codifica sea la correcta.** Las dos cosas se sienten igual desde
> adentro: en ambas la prueba falla cuando se rompe el código. La diferencia
> solo aparece cuando alguien usa el sistema.

Y el corolario que ya es la regla 1 de `CLAUDE.md`, otra vez, en una capa nueva:
**una prueba sabe lo que yo sabía cuando la escribí.** Si lo que yo sabía estaba
equivocado, la prueba lo protege en vez de delatarlo — y cuanto mejor escrita
esté, con mejor justificación en el comentario, más convincente es el error.

El detalle práctico que sale de aquí: el comentario de esa prueba justificaba la
decisión con una cita a la regla 9 ("fallar cerrado"). **Fallar cerrado no
significa rechazar más cosas, significa rechazar lo que no se puede determinar.**
Aquí sí se podía determinar. Invocar una regla correcta sobre un caso que no le
toca es una forma especialmente difícil de detectar de estar equivocado.

## Regla derivada: dos defensas que se solapan hacen que la prueba grande no pruebe ninguna

El agente mínimo tiene una prueba que parece la importante: nueve formas de
portarse mal del modelo falso, contra un transcript con una página hostil
adentro, y ninguna produce un contrato. Pasaba.

Después se rompió cada mecanismo a propósito, para ver cuáles pruebas lo
notaban:

| Lo que se rompió | Pruebas que fallaron | ¿Falló la prueba grande? |
|---|---|---|
| La atribución confía siempre | 8 | **No** |
| La atribución toma la fuente más confiable | 1 | **No** |
| La regla de la ruta desactivada | 1 | **No** |

**La prueba grande no falló ni una sola vez.** Las dos defensas se solapan: con
cualquiera de las dos apagada, la otra sigue deteniendo el ataque. Así que esa
prueba demuestra que el ataque no pasa —que es verdad y vale— pero **no
demuestra que ninguno de los dos mecanismos funcione**, y si mañana alguien
quita uno, seguirá en verde.

Regla:

> **Cuando dos defensas cubren el mismo caso, cada una necesita una prueba que
> desactive la otra.** Si no existe, la que queda sostiene sola una prueba que
> parece cubrir ambas, y su desaparición es invisible.

En la práctica: `attribution_alone_refuses_an_injected_target_without_help_from_the_path_rule`
fuerza el camino determinista para quitar la regla de la ruta de en medio, y
`the_model_cannot_act_once_it_has_read_something_foreign` mira la regla de la
ruta directamente. Cada una falla cuando su mecanismo se rompe; la grande no.

Y el método, que es lo transferible: **romper el mecanismo a propósito y contar
qué pruebas lo notan.** Una prueba que no puede fallar no prueba nada, y la
única forma barata de saber si puede fallar es hacerla fallar. Este ejercicio
costó tres minutos y encontró un hueco que 39 pruebas en verde escondían.

## Regla derivada: una afirmación de ausencia caduca sola, y nadie la revisa

Una revisión externa leyó la bóveda y encontró que `Estado-de-Implementacion`
decía las dos cosas a la vez: la tabla listaba `restore` con su archivo, y una
sección más abajo decía **"`restore` no existe"**. Lo mismo con los límites de
recursos, declarados sin probar en una nota mientras `verify.sh` tenía la etapa
que los probaba. Al ir a corregirlo aparecieron tres más, incluida una en un
comentario de módulo de Rust que afirmaba que un módulo corre con el uid de
Thalyx cuando `thalyx-core/uids.rs` lleva días dándole uno propio.

Las cinco tienen la misma forma. **Una afirmación de que algo funciona la
rompe el código: falla un test. Una afirmación de que algo *falta* no la rompe
nada** — se construye la cosa, pasan todas las pruebas, y la frase que dice que
no existe sigue ahí, verde y falsa. Las secciones "lo que todavía no está" son
las únicas partes de un documento vivo que envejecen sin que nada avise.

Tres consecuencias, en orden de fuerza:

1. **Construir algo incluye buscar quién dijo que no existía.** `grep -rn` del
   nombre de la pieza por `vault/` y por los comentarios de módulo, antes de
   dar por terminado. Es el paso que faltó cinco veces.
2. **Una afirmación de ausencia se escribe con su comprobación al lado.** No
   "`restore` no existe", sino "no existe `crates/thalyx-core/restore.rs`" —
   que es falsable mirando el disco, y por lo tanto automatizable.
3. **Un conteo se escribe una sola vez.** "Las cuatro primitivas" vivía en seis
   notas y en el README; tres decían cuatro construidas contando un componente
   que su propio decreto llama componente. Un número repetido en siete lugares
   es siete oportunidades de divergir y ninguna de detectarlo.

El corolario incómodo: esto no lo encontró ninguna prueba, ni `verify.sh`, ni
yo. Lo encontró alguien de fuera leyendo. **El arnés cubre lo que Thalyx
afirma; no cubre lo que la bóveda afirma sobre Thalyx**, y la bóveda es la
autoridad. Ver la advertencia 4 de [[Estado-de-Implementacion]] para el mismo
agujero en la otra dirección: `verify.sh` exige tres de sus cuatro variables.

## Regla derivada: pedirle algo a una herramienta no es haberlo obtenido

`image/thalyx.config` lista las opciones del kernel que Thalyx necesita, cada
una con su motivo. El `Makefile` las anexaba al `.config` y llamaba a
`olddefconfig`, que las resuelve. Nadie miraba el resultado.

`olddefconfig` **descarta sin decir nada** toda opción cuyas dependencias no se
cumplan. No la rechaza, no advierte, no falla: la línea no aparece en el archivo
de salida y la compilación sigue. Al reproducir la secuencia a mano, **nueve de
las opciones pedidas no estaban en el `.config` final**. Dos de ellas eran
`CONFIG_BPF_LSM` y `CONFIG_DEBUG_INFO_BTF`.

Lo que habría pasado es lo que vuelve grave al caso. El kernel compila. La
máquina arranca. Los siete montajes salen `ok`. Y `thalyx-lsm` no se engancha
—porque el kernel no tiene BPF LSM ni BTF—, con un síntoma idéntico al del
hueco que ya conocíamos (el cargador que invocaba `bpftool`). **La culpa habría
caído sobre el cargador, que no tenía nada que ver**, y el arreglo real estaba a
nueve líneas de distancia en otro archivo.

La forma general: cuando se le pide algo a una herramienta ajena mediante un
archivo de entrada, **el archivo de entrada no es la prueba de nada**. La prueba
es leer lo que la herramienta produjo y compararlo con lo que se pidió. Es la
misma regla que "una prueba de que algo se instaló no es una prueba de que
corre", aplicada un nivel más arriba: aquí ni siquiera hubo instalación, hubo
una petición que se evaporó.

Tres consecuencias:

1. **Toda configuración generada se compara contra la solicitada, y la
   diferencia detiene el proceso.** `make -C image kernel` lo hace ahora y se
   niega a compilar si falta una línea. Fallar la compilación cuesta minutos;
   fallar el arranque costó, en el caso simétrico, tres días de un decreto roto.
2. **La comprobación también cubre el envejecimiento.** Una opción renombrada o
   retirada entre versiones del kernel se ve exactamente igual que una
   dependencia sin cumplir, y es el fallo que un cambio de versión sí produce.
3. **Una opción que está solo porque otra la necesita lo dice en el archivo.**
   Sin eso, la siguiente limpieza borra `CONFIG_FTRACE` por parecer ajena a un
   sistema operativo mínimo, y se lleva `BPF_LSM` con ella sin que nada avise.

El corolario: esto no lo encontró leer el `Makefile`, que era correcto en todo
lo que hacía. Lo encontró correr la secuencia y mirar la salida — la regla 1 de
`CLAUDE.md` otra vez, ahora aplicada a una herramienta de construcción y no al
sistema.

## Regla derivada: un sustituto que nunca ejerció el mecanismo no lo probó

El `allowlist` de seccomp se derivó empíricamente, corriendo módulos reales y
agregando lo que de verdad usaban. Estaba bien hecho y aun así le faltaban dos
syscalls, porque **todos los módulos que se usaron para derivarlo eran scripts
de shell**, y `/bin/sh` no toca un socket en su vida.

El primer módulo escrito contra la API interna murió con `SIGSYS` en su primera
respuesta: un `UnixStream` de Rust lee con `recv(2)` y escribe con `send(2)`,
no con `read` y `write`. Desde el código fuente eso es invisible —dice
`stream.read(...)`— y en una traza es obvio a la primera línea.

La generalización, y es incómoda: **un `allowlist` derivado de correr programas
solo cubre lo que esos programas hacían.** El método es correcto y su alcance
es exactamente el conjunto de programas que se usaron. Un `/bin/sh` que pasa
por el filtro no dice nada sobre un binario de Rust, ni sobre uno de Go, ni
sobre uno que use E/S asíncrona.

Tres consecuencias:

1. **Cuando aparezca una clase nueva de programa, el filtro se vuelve a
   derivar contra ella**, no se asume heredado. El costo de no hacerlo es un
   módulo que muere sin explicación en la primera operación que importa.
2. **La traza antes que la lectura.** Tres minutos de `strace` contestaron lo
   que leer el código no contestaba, porque la syscall real y la función que
   se escribe no se llaman igual.
3. **Permitir usar y permitir crear son permisos distintos.** `recvfrom` y
   `sendto` entran; `socket`, `connect` y `bind` siguen fuera. La prueba que
   lo sostiene afirma las dos mitades juntas a propósito: sin la primera el
   canal está muerto, sin la segunda el canal es un agujero, y separadas cada
   una pasaría sola.

## Regla derivada: una constante de otro proyecto se captura, no se recuerda

**Un número que vive en el header de otro proyecto se copia de ahí y se
comprueba contra esa copia. Escribirlo de memoria es inventarse un fixture, con
la diferencia de que nadie lo va a leer dos veces.**

`BPF_LSM_MAC` es 27. Se escribió 26, que es `BPF_MODIFY_RETURN` — la entrada
inmediatamente anterior en el mismo `enum`. El programa cargó, el kernel aplicó
la comprobación de modify-return a un hook de LSM, y lo rechazó diciendo
`bpf_lsm_socket_connect() is not modifiable`.

Tres cosas de ese fallo valen la pena:

1. **El mensaje era bueno.** Nombraba la función y la comprobación exacta. Aun
   así costó una corrida en hardware real, porque nada del lado de Thalyx podía
   contradecir un número inventado.
2. **Un `enum` sin valores explícitos hace que un error de uno sea silencioso**
   hasta el momento del uso. No hay compilador que lo vea; los dos son `u32`.
3. **La regla de los fixtures inventados ya cubría esto y no se aplicó**, porque
   una constante no parece un fixture. Lo es: es una afirmación sobre el formato
   de otro programa.

El arreglo: `crates/thalyx-bpf/tests/captured/bpf-uapi-enums.h` es el `enum`
copiado verbatim del header de Linux, y la prueba **cuenta las entradas** en vez
de comparar con un número escrito. Si alguien recorta ese archivo "para dejar lo
interesante", las posiciones dejan de ser los números y hay una prueba que se
niega a eso también.

Generaliza a cualquier número que no sea nuestro: valores de `errno`, banderas
de `mount`, `enum`s de uapi, códigos de un protocolo. Si el número viene de otro
lado, la copia viene de otro lado con él.

## Regla derivada: una comprobación sobre la prosa de otra herramienta comprueba la prosa

**Cuando lo que importa es una propiedad, compruébala donde vive, no en la
oración con la que otro programa la describe.**

`image/Makefile` decidía si el binario era estático con `file … | grep
'statically linked'`, y **rechazó un binario perfectamente estático**. Rust
enlaza contra musl como *static-pie*, y `file` a eso le llama `static-pie
linked`: dos frases distintas para la misma ausencia de cargador dinámico. La
construcción se detuvo por la redacción de una herramienta que no prometió nunca
esa redacción.

Lo que de verdad importaba era si el programa pide un intérprete, y eso es una
cabecera del ELF —el segmento `INTERP`— que es exactamente lo que lee el kernel
al hacer `exec`. `readelf -lW | grep INTERP` responde la pregunta real y no
cambia de opinión cuando la herramienta reescribe un mensaje.

Es la misma regla que la de los fixtures inventados, del otro lado: allá se
inventa lo que la herramienta imprime y aquí se inventa lo que va a seguir
imprimiendo. La forma general: **si la comprobación se rompe cuando alguien
reescribe un mensaje, no estaba comprobando la propiedad.**

Y hay una segunda mitad, porque el fallo mintió durante un rato: la imagen ya
había arrancado con ese mismo binario como `/init`. Uno dinámico habría dado
`No working init found` antes de que Thalyx imprimiera una palabra. Cuando una
comprobación contradice algo que la máquina ya demostró, **la comprobación es la
sospechosa** — la quinta regla de esta nota, otra vez, y van siete.

## Regla derivada: una precondición comprueba un artefacto de quien la escribió

**Cuando una prueba empieza con "¿está puesto X?", esa pregunta suele estar
contestada por un rastro que dejó una implementación concreta, y no por X.**

La etapa 14 probó que el cargador propio de Thalyx cargó, que **tres enlaces
estaban vivos** y que los mapas quedaron donde `permd` los busca. Después la
demo de denegación se negó a correr: *«thalyx-lsm is not attached. Run 'make
load' first.»* Sobre una máquina donde acababa de demostrarse, tres líneas más
arriba, que sí lo estaba.

La precondición era `test -d /sys/fs/bpf/thalyx/lsm` — el directorio que crea
`bpftool prog loadall` y **nadie más**. El cargador de Thalyx fija en otra forma,
decretada y con motivo, así que la demo contestaba por el cargador que no estaba
usando.

Tres comprobaciones distintas tenían la misma enfermedad, y todas contestaban
que sí a cosas que no aplican nada:

| Quién | Qué preguntaba | Qué contesta eso |
|---|---|---|
| `thalyx session` | ¿está fijado el **mapa** de política? | que un cargador corrió |
| `make status`, la demo | ¿existe un **directorio** en bpffs? | que corrió *bpftool* |
| `dev/verify.sh` | ¿hay ≥2 enlaces LSM en la máquina? | que alguien tiene enlaces |

La tercera es la más fea porque *pasó*: imprimió **3** para un objeto de dos
programas, y los diez del vigilante de archivos la habrían satisfecho con el
enforcement sin atacharse.

Lo que de verdad importa es lo que esta nota lleva diciendo desde que existe:
**un pin no es un enlace.** Un programa cargado, fijado y en el camino de
decisión de nadie se lista idéntico a uno vivo. La respuesta honesta es
enumerar los enlaces del kernel, seguir cada uno hasta el programa que corre y
comparar los nombres contra el objeto — que es lo que hace `thalyx enforce
attached`, sin bpftool, y por eso contesta igual para cualquiera de los dos
cargadores.

Dos detalles que solo aparecen al hacerlo:

- **El kernel guarda quince caracteres de nombre.** `thalyx_socket_connect` son
  veintiuno. Comparar el nombre completo contra el truncado no encuentra nada, y
  la respuesta habría sido «no está atachado» en una máquina donde sí — el mismo
  fallo, entrando por el arreglo.
- **Un nombre no basta.** Cualquiera puede llamar a su programa
  `thalyx_file_open`; la comparación exige también el tipo de programa.

La forma general: **si la precondición la escribió quien también escribió la
implementación, probablemente comprueba la implementación.** Se nota preguntando
si otra implementación correcta la pasaría.

## Regla derivada: lo escrito a mano en lugar de lo generado descansa en una propiedad invisible

**Cuando algo generado se sustituye por algo escrito a mano, lo que lo vuelve
correcto casi nunca se ve en el archivo. Eso se prueba, no se recuerda.**

`lsm/vmlinux.h` eran cien mil líneas que producía `bpftool` desde el BTF del
kernel corriendo. Escribirlo a mano —nueve structs, que es lo que los dos
programas tocan— quitó `bpftool`, `CONFIG_DEBUG_INFO_BTF` y el permiso de leer
`/sys/kernel/btf/vmlinux` de lo que hace falta para **construir** Thalyx.
Ninguna de las tres hace falta para correrlo, y las tres son sitios donde se
atora la máquina de otra persona.

Los offsets de ese header están mal. Da igual, y ese «da igual» es toda la
cuestión: `preserve_access_index` hace que clang emita una reubicación CO-RE con
el **nombre** del campo en vez de un offset, y el cargador parcha el offset real
del kernel que está corriendo.

Quitar esa línea no rompe nada visible. El programa compila, carga, pasa el
verificador, corre — y lee para siempre el offset que un archivo se inventó.
**Sin síntoma.** Y no hace falta quitarla: basta con declarar un `struct` debajo
del `pop`, que es exactamente la forma de un acomodo inocente.

Así que hay una prueba que lee el header y exige que todo `struct` con campos
esté bajo el `pragma`. Se comprobó moviendo `struct path` debajo del `pop`:
falló, nombrándolo. Y otras dos que impiden que la primera se vuelva vacía —
que los structs que los programas leen estén de verdad ahí, y que el archivo no
sea el generado otra vez.

La forma general: **un sustituto escrito a mano es correcto por un motivo, y ese
motivo hay que poder romperlo en una prueba.** Si no se puede, lo que se tiene
es la esperanza de que nadie ordene el archivo.

## Regla derivada: la limpieza de una prueba puede destruir más que la prueba

**Si una prueba monta algo, su limpieza tiene que desmontarlo antes de borrar, y
tiene que negarse a borrar si no pudo desmontar.**

`verify.sh` termina siempre con `rm -rf "$WORK"`, y eso estuvo bien mientras
`$WORK` fuera un directorio de archivos. La etapa que prueba el store monta un
disco de bucle **adentro** de `$WORK`. Una corrida interrumpida —Ctrl-C, un
error, el `trap` disparando por cualquier motivo— deja el montaje puesto, y en
ese estado `rm -rf` no borra el archivo que contiene el filesystem: **borra el
contenido del filesystem, a través del punto de montaje**.

La forma general: una prueba que cambia el estado de la máquina —montar, cargar
un módulo del kernel, crear un subvolumen— tiene una limpieza que ya no es
simétrica con lo que creó. Y la limpieza corre justo en los casos donde algo
salió mal, que es cuando menos se sabe en qué estado quedó todo.

Tres cosas, en este orden:

1. **Desmontar antes de borrar**, no después ni en paralelo.
2. **Comprobar que se desmontó.** `umount` falla, y falla precisamente cuando
   algo sigue usando el punto de montaje.
3. **Si sigue montado, no borrar y decirlo.** Dejar basura es reparable; borrar
   el disco de alguien no.

Esto se escribió antes de que costara nada, que es la única vez que se puede
escribir una regla de esta clase.

## Regla derivada: un mensaje que nombra la causa envejece cuando aparece la segunda ruta

**Un mensaje puede decir en qué estado está el sistema. No puede decir cómo
llegó ahí, salvo que quien lo imprime lo haya visto.**

Encontrado el 2026-08-04. Cuando la memoria persistente no puede confirmar un
hecho, la sesión imprimía:

> *Something it described changed **without going through Thalyx**.*

Era cierto mientras la única forma de llegar a ese estado fuera que alguien
editara un archivo por fuera. Al darle a la sesión el verbo `revertir`, apareció
una segunda ruta —Thalyx haciendo exactamente lo que le pidieron— y la frase
pasó a ser **una explicación segura de una causa que ese código no puede ver**.
No se rompió ninguna prueba. Lo que se rompió es que manda a la persona a buscar
un intruso que no existe.

La forma general: el mensaje se escribe cuando hay una sola manera de llegar al
estado, y describir *el camino* se siente igual de seguro que describir *el
estado*. La segunda ruta llega meses después, en otro archivo, y nadie vuelve a
leer el texto.

Se distingue así: **si el mensaje se borrara, ¿el código podría volver a
deducirlo?** El estado sí — está ahí, se acaba de leer. La causa no.

Un mensaje que nombra la causa equivocada es peor que uno que no nombra
ninguna, y es la regla 10 otra vez: decir qué pasó, no por qué se supone que
pasó. La versión que quedó nombra las dos rutas y dice que desde ahí no puede
distinguirlas.

## Regla derivada: comprobar que llegó lo que se pidió no comprueba que se pidió lo que hacía falta

**Una comprobación que compara lo solicitado contra lo obtenido no puede ver lo
que nadie solicitó. Hay que preguntarle al artefacto qué necesita.**

Encontrado el 2026-08-04, en el primer arranque de la imagen con el cargador
propio. La máquina montó todo y dijo:

```
no  thalyx-lsm  this kernel does not expose `bpf_lsm_socket_connect`
```

`thalyx.config` tenía `CONFIG_SECURITY` y no tenía `CONFIG_SECURITY_NETWORK`.
Los símbolos `bpf_lsm_<hook>` se generan desde `include/linux/lsm_hook_defs.h`,
y todos los hooks de socket están adentro de un `#ifdef CONFIG_SECURITY_NETWORK`.
El símbolo no fue rechazado: **nunca se compiló.**

Y `config-check` —la comprobación escrita el 2026-08-03 justo para las opciones
que Kconfig descarta en silencio— pasó en verde, correctamente. Compara cada
línea de `thalyx.config` contra el `.config` resultante. **Ninguna línea pedía
`SECURITY_NETWORK`, así que no había nada que comparar.** La comprobación
anterior, "pedirle algo a una herramienta no es haberlo obtenido", cubre un
hueco distinto y no toca este.

Es un punto ciego con forma propia: cada comprobación de esta clase se escribe
mirando la lista que ya existe, y el fallo vive **fuera** de la lista. Nada
dentro del sistema de comprobación puede señalarlo, porque el sistema entero
está definido por esa lista.

La salida es preguntarle a **otra cosa** qué hace falta. Aquí: el objeto BPF
sabe a qué símbolos se engancha —`thalyx enforce hooks` los imprime, sacados de
las secciones `SEC("lsm/...")`— y `hook-check` los busca en el `System.map` del
kernel recién compilado. La lista dejó de estar escrita en ningún lado; se
deriva del artefacto que la va a usar, que es la misma regla que ya rige
[[Cargador-BPF-Propio|`enforce attached`]].

Y el coste de no tenerla: el único que notó la ausencia fue **una máquina sin
shell**, después de compilar un kernel, construir una imagen y arrancar.

## Regla derivada: que el símbolo exista no es que se le pueda enganchar algo

**Comprobar que una capacidad está presente y comprobar que se puede usar son
dos comprobaciones. La segunda es la que importa y casi nunca se escribe.**

El 2026-08-04, un arranque después del anterior. `hook-check` confirmó que el
kernel exponía `bpf_lsm_socket_connect`. La máquina arrancó y dijo:

```
no  thalyx-lsm  attaching `thalyx_socket_connect`: Resource busy (os error 16)
```

BPF se engancha a un hook LSM con un *trampolín*. `register_fentry` le pregunta
a `ftrace_location()` si la función está gestionada por ftrace; sin ftrace
dinámico no lo está, así que el kernel parcha el texto él mismo — y
`__bpf_arch_text_poke` espera encontrar el NOP de cinco bytes que
`CONFIG_FUNCTION_TRACER` pone al principio de cada función. No estaba, el
`memcmp` falló, y ese camino devuelve `-EBUSY`.

**Y `EBUSY` describe otra cosa.** «Recurso ocupado» se lee como que algo más
tiene el hook tomado, y no había nada. El errno era correcto y la única lectura
natural de él era falsa.

Dos cosas salieron de ahí:

1. **La comprobación pregunta por el artefacto, no por la opción.**
   `register_ftrace_direct` solo se compila bajo
   `CONFIG_DYNAMIC_FTRACE_WITH_DIRECT_CALLS`, así que su presencia en el
   `System.map` **es** la propiedad. `thalyx.config` también pide la opción y
   `config-check` la verifica, pero eso solo funciona mientras alguien conserve
   la línea — y una comprobación definida por una lista no puede ver lo que
   falta de la lista, que es la regla de arriba.

2. **Un errno que se lee mal viaja con lo que puede significar.** El mensaje
   nombra **las dos** causas de `EBUSY` en ese camino —sin soporte de
   trampolines, o algo que ya tiene una llamada directa en ese hook— y dice que
   desde ahí no las puede distinguir. Nombrar una sola habría sido inventar la
   causa, que es la regla anterior a esta.

### El patrón, que ya son tres

`allnoconfig` apaga todo lo que nadie nombre, y lo que BPF LSM necesita está
repartido en cuatro menús sin un símbolo que lo anuncie. Tres opciones se han
encontrado **arrancando**: `BPF_LSM`/`DEBUG_INFO_BTF` (2026-08-03),
`SECURITY_NETWORK` y ahora `FUNCTION_TRACER`. Cada una costó recompilar un
kernel y arrancar.

Ninguna comprobación de construcción encuentra la cuarta, porque la lista de lo
que hace falta la tiene el kernel corriendo y nadie más. **Lo único que cerraría
la clase entera es arrancar la imagen dentro de `verify.sh`** en vez de a mano,
que es una de las dos `NOT PROVEN` que la corrida arrastra desde que existe.

## Regla derivada: preguntarle a una herramienta ajena por algo que ella no hizo

**Cuatro veces, la misma forma.** Una comprobación le pregunta a `bpftool` por
un estado que `bpftool` no creó, y contesta por la implementación equivocada —
siempre en la dirección de decir «no hay nada» sobre algo que sí está.

| Cuándo | Qué preguntaba | Qué contestaba de verdad |
|---|---|---|
| 2026-08-03 | `test -d /sys/fs/bpf/thalyx/lsm` | Si el cargador fue `bpftool` |
| 2026-08-03 | La sesión, por un mapa fijado | Si había `bpftool`, y si un mapa estaba puesto — que no es si algo aplica |
| 2026-08-03 | `verify.sh`, contando enlaces LSM | Cuántos enlaces hay en la máquina, de quien sea |
| **2026-08-04** | **`is_available()`, antes de correr un módulo** | **Si `bpftool` existe** |

La cuarta es la peor y salió arrancando la imagen. `bpftool map show pinned` es
lo que decidía entre **confinar un módulo y negarse a arrancarlo**, y adentro de
la máquina no hay `bpftool` — así que una máquina que había atachado su propio
enforcement, con los dos hooks vivos y los tres mapas fijados, se negó a correr
un módulo confinado y ofreció `sin-confinar` como salida. **El enforcement era
real y lo único que no podía verlo era el código que decidía si usarlo.**

Y no era solo la pregunta: `set()` también escribía con `bpftool`, así que
adentro de la imagen **ninguna política se podía escribir**. La comprobación
estaba equivocada y también tenía razón.

La regla general: **una capacidad que se comprueba con una herramienta se está
comprobando en la máquina de quien la instaló, no en la que va a correr.** Si
el programa puede hacer la operación él mismo, la pregunta correcta es
intentarla. `KernelStore` abre el pin con `bpf(2)` y esa apertura *es* la
respuesta.

## Regla derivada: una comprobación que va después de una puerta solo se hace del otro lado

`thalyx session` le pedía al núcleo un perfil de sandbox llamado `default`.
Ningún perfil se llama así. El único perfil que existe para un módulo es
`module_standard`, y el nombre estaba escrito a mano en `session.rs` en lugar de
tomado de la constante que ya estaba a tres líneas de ahí en `main.rs`.

Eso vivió ahí desde que el prompt puede correr un módulo, con la suite entera en
verde, y salió **en la consola de la máquina, después de que la instalación ya
había salido bien**. El peor lugar posible: la persona que estaba probando los
seis pasos ya había confirmado un permiso y visto `installed`, y lo siguiente que
leyó fue un error sobre un perfil.

Lo que lo escondió no fue el descuido, fue el **orden**:

```
if !policies.is_available() { return Err(NothingCanEnforce) }   ← la puerta
let profile = profile::resolve(request.profile)?;               ← el nombre
```

En toda máquina sin el mapa de políticas fijado — es decir, en todas menos la
imagen — la puerta contestaba primero, con una respuesta **honesta y correcta**,
y el nombre no se miraba nunca. La imagen fue la primera máquina que pasó la
puerta, y por eso fue la primera que miró el nombre.

**Un dato que solo se valida después de una condición solo está validado en las
máquinas que cumplen la condición.** Si la validación no tiene efectos —
resolver un nombre no toca nada — va antes de la puerta, y entonces un nombre
que no existe es un nombre que no existe en cualquier máquina. Movido: ahora
`resolve` corre antes de `is_available`, y lo sostiene
`a_profile_no_profile_is_called_is_refused_before_the_kernel_is_asked`, que se
comprobó fallando con el orden viejo.

Y la razón de fondo, que es la regla 1 otra vez: **la etapa 15 maneja el prompt
de verdad y no tecleaba `correr`.** Era el único verbo del prompt sin ejercitar,
y era el único roto. Ahora lo teclea, y lo que exige no es que el módulo corra
—eso lo decide el kernel y lo pregunta la etapa 16— sino que si falla, falle por
una razón que signifique que llegó hasta el kernel.

### Y el mensaje de la falla apuntaba al lado equivocado

La línea que reportó esto decía «the image has no bpftool, so anything that asks
bpftool answers no in there». Se escribió cuando esa era la causa probable y se
quedó ahí después de dejar de serlo. La causa verdadera estaba impresa cuatro
renglones más abajo, en el propio log.

**Un mensaje de falla que nombra una causa que no midió es peor que uno que no
nombra ninguna: dice dónde no hay que buscar.** Un `failed` dice qué no pasó e
imprime lo que la máquina dijo. Por qué es de quien lea.

## Regla derivada: lo que el anfitrión hacía gratis no está en el diseño hasta que alguien lo escribe

Arreglado el nombre del perfil, la misma línea falló otra vez, un paso más
adelante:

```
`/sys/fs/cgroup/thalyx` cannot hand down the controller(s) ["memory", "pids"]
It has: []
```

La negativa era **correcta**: sin esos controladores los límites no se
aplicarían y el módulo se vería acotado sin estarlo. Lo que faltaba era que
alguien los delegara. El `cgroup.controllers` de un cgroup es lo que su padre
puso en `cgroup.subtree_control`, y el kernel arranca con la raíz sin delegar
nada.

En cualquier otro Linux **systemd lo hace antes de que corra nada**, así que el
cgroup `thalyx` los heredaba sin que nadie de este proyecto lo hubiera pedido.
Hasta el arnés de pruebas lo sabía —`confined_run.rs` escribe `+memory +pids` en
su arena y lo explica en un comentario— y aun así nadie lo escribió en el
sistema, porque en las máquinas donde se probaba ya estaba hecho.

**En la imagen no hay systemd.** No hay nada más que Thalyx. Y eso es
exactamente el decreto fundacional: si Linux es un componente que Thalyx
gestiona, entonces todo lo que otra cosa hacía por nosotros es ahora trabajo de
Thalyx, lo hayamos notado o no.

La regla: **cada vez que algo funciona en la máquina de desarrollo y no se sabe
quién lo hizo, hay una dependencia sin escribir.** No se descubre leyendo el
código —el código no menciona a systemd en ninguna parte— sino corriendo en la
única máquina donde no está: la imagen. Es la regla 1 en su forma más literal.

Cerrarla tiene dos mitades, y la segunda importa igual:

1. PID 1 delega los controladores al montar, tomando la lista del perfil bajo el
   que corren los módulos y no de una copia al lado.
2. **La sesión lo reporta.** `cgroup2` decía `mounted at /sys/fs/cgroup` en una
   máquina donde ningún módulo podía recibir un límite. La pantalla de arranque
   salía limpia y el primer `correr` no — el fallo sin síntoma, en el único
   lugar construido para no tener ninguno. Ahora un cgroup2 montado que no
   delega nada se lee como ausente, y dice qué le falta.

Los tests distinguen los dos archivos que se parecen: `cgroup.controllers` es lo
que un cgroup *podría* delegar y `cgroup.subtree_control` es lo que delega. Leer
el primero por el segundo describe exactamente la máquina rota como sana, y se
comprobó cambiándolo.

## Regla derivada: dos códigos que obedecen la misma regla del kernel dejan de obedecerla

Con los controladores entregados, el módulo **corrió confinado** adentro de la
imagen —cgroup propio, raíz propia, usuario propio, seccomp, límites— y se cayó
en el último syscall de un montaje:

```
could not attach the remapped mount at
/run/thalyx/sandbox/opt/thalyx/data/greeter/notes.txt: Invalid argument
```

`fs/namespace.c`, `do_move_mount`, verbatim del 6.12:

```c
if (d_is_dir(new_path->dentry) != d_is_dir(old_path->dentry))
    goto out;   /* -EINVAL */
```

**El punto de montaje de un archivo tiene que ser un archivo.** `bind` lo sabía
—miraba `metadata.is_dir()` y creaba una cosa o la otra— y `bind_remapped`
llamaba a `create_dir` sin mirar. Dos funciones que obedecen la misma regla del
kernel, escritas por separado, y una de las dos dejó de obedecerla.

Sobrevivió porque **todos los permisos de todas las pruebas de este repositorio
son directorios**. El único permiso sobre un archivo suelto que existe es el del
`greeter`, y el único lugar donde el `greeter` corre con usuario propio es la
imagen.

Dos reglas, y la segunda es la que vale más:

1. **Cuando dos lugares tienen que obedecer la misma regla externa, son uno.**
   Ahora hay una `create_target_like` y las dos rutas la llaman. La regla del
   kernel está citada en su documentación, con archivo y función, no de
   memoria.
2. **Un caso de prueba que nunca varía no es un caso de prueba, es una
   constante.** «Un permiso sobre una ruta» se probó decenas de veces y siempre
   sobre un directorio. La dimensión que importaba —archivo o directorio— nunca
   se movió, así que nada dependía de ella y nada la comprobó.

El `EINVAL` del API nuevo de montajes no dice nada; `mount(2)` para el mismo
error dice `ENOTDIR`, que al menos nombra el problema. Por eso la comprobación
está ahora en una prueba que corre siempre, y no en leer un errno.

### Y el extracto de la falla cortaba antes de la línea que importa

La etapa 16 imprimía `grep -A6` del `correr`, y el reporte del run —incluida la
línea del código de salida— es más largo que eso. Un sandbox que no se pudo
armar y un módulo que corrió y no dijo nada se veían igual en la única salida
que alguien lee. Ahora imprime el reporte entero.

## Regla derivada: lo que el anfitrión hacía gratis, otra vez, y ahora era el arranque entero

Cuarta ronda de lo mismo, y la más profunda. `pivot_root` devolvió `EINVAL`
adentro de la imagen, después de que el módulo ya tenía cgroup, política,
usuario, namespaces, seccomp y límites — todo correcto.

`fs/namespace.c`, `do_pivot_root`:

```c
if (!mnt_has_parent(root_mnt))
    goto out4; /* not attached */   -- EINVAL
```

**La raíz de un namespace de montajes no tiene padre.** En cualquier otro Linux
eso es invisible, porque la raíz del proceso *no* es la raíz del namespace: el
kernel arma un `rootfs` interno y todo sistema real monta algo encima, así que
la raíz del proceso es un hijo y tiene padre. Eso lo hace `switch_root`, en el
initramfs, antes de que arranque nada.

**La imagen es un initramfs y nada más.** Su raíz de proceso *es* la raíz del
namespace, y sigue siéndolo después de `unshare(CLONE_NEWNS)` porque la copia
también es raíz de un namespace. Nadie había hecho el `switch_root` porque en
todas las demás máquinas ya estaba hecho — igual que con systemd y los
controladores, dos rondas antes.

Se arregla con lo mismo que hace `switch_root`, pero con un bind en vez de un
tmpfs: montar `/` sobre `/newroot`, `chdir` ahí, mover ese montaje a `/`, y
`chroot(".")`. El bind comparte los mismos inodos y las mismas páginas —copiar
los seis megabytes de `/init` a un tmpfs no cambiaría nada y costaría RAM— y
`__do_loopback` le quita `MNT_LOCKED` al clon, que es lo que hace legal el
movimiento después.

**Se comprobó corriéndolo**, con los mismos envoltorios de `thalyx-syscall`,
dentro de un namespace de montajes desechable: `switch: ok`, la raíz pasa a
tener padre, y `pivot_root` después funciona.

### Y la máquina ahora lo dice de sí misma

El arranque imprime dos hechos distintos: que el cambio de raíz corrió, y que
la raíz resultante **es una de la que se puede sacar un módulo**, leído del
kernel y no inferido del primero. La sesión toma la misma lectura. Es la misma
corrección que le hicimos al cgroup2 dos rondas antes, y por la misma razón: la
pantalla de arranque no puede salir limpia en una máquina donde ningún módulo
puede correr.

## Regla derivada: una prueba que sólo ejerce el primer caso decide el diseño sin que nadie lo note

Encontrada el 2026-08-04, por una auditoría externa y no por una prueba.

El registro de permisos escribía las concesiones **antes** del commit, y el
comentario que lo justificaba era correcto y estaba incompleto. Decía: hasta que
el enlace `current` se mueva, ningún módulo apunta al registro, así que es
inerte. Cierto — **para una instalación nueva**. En una actualización la versión
1 es la actual durante toda la ventana, de modo que un corte ahí dejaba a la
versión 1 corriendo con los permisos confirmados para la versión 2.

Trece pruebas de inyección de fallos cubrían esa ventana. Ninguna la vio, porque
**todas instalaban un módulo por primera vez**. La ventana estaba probada
exhaustivamente en el único caso donde el defecto no aparece.

La regla:

**Una prueba de una transición tiene que ejercerla desde un estado previo que no
sea el vacío.** Instalar sobre nada y instalar sobre algo son transiciones
distintas, y la primera es la que menos se equivoca. Lo mismo vale para
cualquier "actualizar", "reemplazar" o "revocar": el caso inicial es el fácil, y
probar sólo el fácil es cómo un invariante llega a estar escrito, comentado y
roto al mismo tiempo.

El corolario es sobre los comentarios, no sobre las pruebas: **un comentario que
justifica una decisión con "en este caso no puede pasar" tiene que nombrar el
caso.** El de aquí decía "ningún módulo apunta al registro". Si hubiera dicho
"ninguna *versión*", el hueco habría sido visible al leerlo.

## Regla derivada: una comprobación y el uso que la sigue son dos momentos

El API interna resolvía la ruta con `canonicalize`, comparaba el resultado
contra la concesión y después abría la ruta resuelta. Cada paso era correcto y
la secuencia no lo era, porque es una secuencia: entre la comparación y la
apertura hay un instante, y un módulo que puede escribir dentro de su propia
concesión puede usar ese instante para reemplazar el nombre por un symlink a
otro lado. Thalyx abría el nuevo destino, con el alcance de Thalyx — que corre
**fuera** del sandbox a propósito.

**Nada comprobado en espacio de usuario cierra eso.** La comprobación y el uso
tienen que ser la misma llamada al sistema, o no son una comprobación:
`openat2` con `RESOLVE_BENEATH` contra un descriptor del directorio concedido,
abierto una sola vez.

La forma general, que aplica a cualquier recurso y no sólo a rutas:

**Si entre comprobar y usar hay un nombre que alguien más puede redefinir, la
comprobación no vale.** Lo que hay que sostener no es el resultado de la
comprobación: es el objeto. Un descriptor sostiene el objeto; una ruta sostiene
un nombre.

Y el costo se escribe: `RESOLVE_BENEATH` rechaza **todo** symlink absoluto,
incluido uno que habría caído dentro de la concesión. Es una pérdida real y es
la dirección correcta de pérdida — a un módulo se le niega algo que debía poder
hacer, lo cual alguien reporta, en vez de permitírsele algo que no debía, lo
cual no nota nadie. Hay una prueba que nombra ese costo para que el día que
alguien se pregunte por qué su enlace dejó de andar, la respuesta esté en la
suite y no en la memoria de alguien.

## Regla derivada: lo que un componente no confiable acumula en el confiable tiene que tener techo

Cada `Notify` que un módulo manda se guarda hasta que la corrida termina, para
poder mostrárselo al humano. El límite de cuadro acotaba **un** mensaje en un
mebibyte y nada acotaba **cuántos**. El cgroup del módulo no lo veía: la memoria
crecía en Thalyx, que dentro de la imagen es el pid 1.

**Un límite sobre el tamaño de una unidad no es un límite sobre el total.** Cada
vez que un componente que no se confía elige cuántas veces hacer algo, el techo
va sobre la acumulación y no sobre la pieza — y hacen falta los dos, porque
doscientos cincuenta y seis mensajes de un mebibyte es el mismo ataque con menos
pasos.

Y lo que se descarta se **cuenta y se dice**. Una lista que dejó de crecer en
silencio se ve idéntica a un módulo que se calló, y son cosas distintas.

## Regla derivada: un estado de confianza que no se puede leer no es un estado de confianza vacío

`Keystore::load` hacía `unwrap_or_default()`. Un `keys.json` corrupto se volvía
un keystore **vacío**, y un keystore vacío confía en todo lo que le ofrezcan —
eso es lo que significa confianza en primer uso. Así que dañar un archivo
degradaba a todos los publicadores anclados a un primer avistamiento, y el
siguiente paquete ofrecido para cualquier id instalado, firmado por quien fuera,
habría sido aceptado. No hacía falta romper ninguna criptografía: bastaba una
escritura cortada a la mitad.

Ya existía la regla 10 —*una falla de lectura no es una falla de existencia*— y
no se había aplicado aquí. La forma específica que faltaba:

**Ausente y corrupto son respuestas opuestas, no dos formas de "nada".** Para un
almacén de confianza, ausente significa que nunca se ancló nada y confiar en la
primera clave es la política; corrupto significa que **sí** se ancló algo y
nadie sabe qué. El código que las colapsa elige la insegura.

El control importa tanto como el caso: negarse a leer **ambas** dejaría una
máquina nueva incapaz de instalar nada. Hay una prueba para cada una.

## Regla derivada: una comprobación que solo corre a mano, en una máquina, deja de correr

Encontrada el 2026-08-04, y es la forma más cara de la regla 5 hasta ahora.

La etapa 15 de `verify.sh` es la que maneja el prompt de la sesión: instalar,
confirmar por el camino confiable, revertir, y volver a una máquina que todavía
sabe qué se le pidió. **Cuatro de los seis pasos que cierran la Fase 1.**

Necesitaba `script(1)` para darle una terminal al confirmador —que se niega sin
una, correctamente, porque el silencio no es consentimiento—. Fedora trae
`script` en `util-linux-script`, un subpaquete que no se instala solo. Así que
en la única máquina que puede verificar Thalyx, la etapa se saltó entera.

**El salto funcionó exactamente como está diseñado**: imprimió `NOT PROVEN` y
dijo por qué. Eso es la maquinaria haciendo su trabajo, y no alcanzó. La regla:

**Una comprobación que solo puede correr en una máquina, a mano, es una
comprobación que va a dejar de correr — y el `NOT PROVEN` no lo impide, porque
nadie lee un informe que ya conoce.**

De ahí dos consecuencias, y las dos se aplicaron:

1. **La dependencia externa se elimina en vez de documentarse.** Thalyx hace su
   propia pty, como hace su propio initramfs y su propio cargador de BPF. Un
   sistema que embarca un solo programa puede producir ochenta líneas de
   `posix_openpt` antes que heredar una cuarta cosa que nadie eligió.
2. **Lo que no necesita hardware se muda a la suite.** De esos cuatro pasos,
   ninguno necesita BPF, Btrfs ni un cgroup delegado: necesitaban una terminal.
   Ahora corren en cada cambio. En `verify.sh` queda lo que sí necesita una
   máquina — arrancar la imagen, que el kernel deniegue, un reinicio de verdad —
   donde una máquina que no puede contestar lo dice.

El criterio que cierra la fase no puede depender de que una persona se acuerde
de correr un comando.

### Y el corolario sobre el arnés que reemplaza a otro

La pty nueva decide ahora si cuatro pasos del criterio pasan. Una versión que
fallara en silencio haría que todo lo de abajo se leyera como un sistema que no
confirma, en vez de como un instrumento roto. Así que la etapa **comprueba el
arnés antes de confiar en él**, con su control: adentro tiene que haber terminal
y afuera no. Sin el control, una respuesta fija pasaría igual.

## Regla derivada: una medida de contención puede cegar al instrumento que la prueba

Encontrada el 2026-08-05, corriendo `verify.sh` con los arreglos de la auditoría
del 2026-08-04 puestos. Diez `FAILED` en una máquina donde el sistema estaba
bien.

El arreglo era correcto: el módulo heredaba `stdin`, `stdout` y `stderr` de
Thalyx, así que compartía terminal con el camino confiable —podía leer la `y`
del humano y podía dibujar el marco— y se le quitaron los tres, a `/dev/null`.

**Y el `stdout` del módulo era el instrumento.** La regla 2 de este documento
dice que un test de aislamiento le pregunta al programa confinado qué ve; la
etapa 6 le pregunta seis cosas —su pid, su uid, su hostname, sus interfaces, su
raíz, si alcanza lo concedido— y las seis respuestas viajaban por ahí. Con el
descarte, las seis reportaron `nothing`.

**`nothing` es también lo que reporta un sandbox que no aisló nada.** Ese es el
punto entero:

> Una medida de contención que descarta lo que el programa confinado dice deja
> a la contención sin testigo. Un módulo perfectamente aislado y uno sin aislar
> pasan a dar la misma respuesta, y esa respuesta es el silencio.

Y costó la otra dirección también: un módulo que muere con un mensaje en
`stderr` dejó de tener manera de decir por qué. *Falló* y *falló por esto* se
volvieron el mismo evento, que es justo lo que la regla 10 de `CLAUDE.md`
prohíbe.

La salida es una tubería que Thalyx **drena** —el techo es sobre lo que se
guarda, nunca sobre lo que se lee, o el módulo se bloquea en la siguiente
escritura y el techo se vuelve un cuelgue— y reimprime marcada y saneada, igual
que ya hacía con el canal. La propiedad que el `/dev/null` compraba se conserva
entera y dicha con precisión:

> **Un módulo no puede empezar una línea.** No que sus palabras desaparezcan —
> desaparecer era el error—, sino que todo lo que escribe llega detrás del
> marcador de Thalyx. Es la misma afirmación que el saneador del prompt hace
> sobre el nombre de un publicador.

### Y la prueba que lo dejó pasar afirmaba la ausencia

`a_module_never_gets_the_terminal_the_trusted_path_uses` afirmaba que el texto
del módulo **no aparece**. Pasaba, y seguiría pasando con la salida en
`/dev/null` para siempre. La etapa 17 tenía la misma forma.

Una afirmación de ausencia necesita un control que exija la presencia de lo
que sí debe estar — si no, la forma más fácil de satisfacerla es borrar todo.
Las dos tienen ahora ese control: el texto del módulo **tiene** que aparecer,
marcado.

### Y la etapa 17 llevaba un día sin poder probar nada

Decía `NOT PROVEN: no installed module`, honestamente: la etapa 6 revierte el
módulo como su última comprobación, así que cuando la 17 corría no había nada
instalado. Nunca. Es la regla de arriba —nadie lee un informe que ya conoce—
con una vuelta más:

> Un salto que se dispara **siempre** no es un salto, es una comprobación que
> nunca se hizo. Un `NOT PROVEN` que no depende de la máquina depende de un
> defecto.

## Regla derivada: la segunda mitad de un arreglo vive donde el razonamiento se aplica, no donde se escribió

Encontrada la misma tarde, y es la otra mitad de los diez `FAILED`.

La auditoría arregló un truncado: `sanitise` corta a 72 caracteres, y aplicado a
un permiso hacía que `/home/user/projects/secrets` y
`/home/user/projects/public` se dibujaran igual. El arreglo está razonado con
todas las letras en `sanitise_permission` — **un permiso es contenido, no una
etiqueta, y a lo que es contenido se le quita el medio, nunca el final.**

`sanitise_block` —lo que un módulo dice por el canal— quedó llamando a
`sanitise` línea por línea. Es contenido con la misma exactitud, y 72
caracteres es más corto que una frase corriente sobre un archivo:

```
read 27 byte(s) from /tmp/tmp.BCvj7bvl02/greeter-granted/notes.txt: the…
```

Es el `greeter` contestando qué leyó, con lo que leyó cortado. Las etapas 12 y
13 fallaron pidiendo justamente esa parte.

Lo peor no es el corte sino **quién decide dónde cae**: la longitud que gasta el
presupuesto es la de la *ruta*. El mismo módulo diciendo lo mismo dice menos en
una máquina con directorios más anidados. Una prueba escrita con rutas cortas
nunca lo ve, y no había ninguna.

> Cuando un arreglo se justifica con un razonamiento, el arreglo va a **todos**
> los lugares donde ese razonamiento se aplica, en el mismo commit. Buscarlos es
> parte del arreglo, no una limpieza posterior. El razonamiento estaba escrito y
> era correcto; lo que faltó fue preguntarse quién más lo cumple.

## Regla derivada: un procedimiento impreso para una persona es código sin correr

Encontrada el 2026-08-06, la primera vez que alguien siguió `make -C image
pin-kernel` en vez de leerlo.

El objetivo imprime cuatro comandos y **no ejecuta ninguno**, a propósito y con
su párrafo: automatizar la verificación de una firma la volvería teatro, porque
lo que establece algo es que un humano decida de quién es la llave. Lo que no
se pensó es que el texto impreso sigue siendo una salida, y una salida puede
estar equivocada.

El segundo comando decía `gpg --locate-keys torvalds@kernel.org
gregkh@kernel.org`. Son quienes firman una versión del kernel, que es lo que
todo el mundo sabe, y **no son quienes firman ese archivo**: `sha256sums.asc`
lleva la firma de la llave automática de sumas de kernel.org. La corrida real
contestó:

```
gpg: Imposible comprobar la firma: No hay clave pública
```

Tres renglones debajo de una frase que decía que cualquier cosa que no fuera
*Good signature* es motivo para detenerse. **El procedimiento imprimía la falla
que él mismo define como fatal**, y de las dos salidas posibles —seguir con un
digest sin verificar, o parar— ninguna es la que hacía falta.

Y la forma es conocida: se escribió en este contenedor, cuya política de red no
alcanza kernel.org, así que no había manera de correrlo aquí. Es la misma raíz
que [[#Regla derivada un fixture inventado prueba lo que yo entendí, no lo que la herramienta imprime|el fixture inventado]] con otro disfraz — un modelo de cómo se comporta una herramienta ajena, escrito con confianza y sin una sola muestra real.

> Un bloque de texto que le dice a una persona qué teclear es código: tiene una
> salida, y hasta que alguien la corra no se sabe cuál es. Si no se puede
> ejercer donde se escribe, se marca como no ejercido y **lo primero que hace
> quien tenga la máquina es correrlo**, antes de que sea el paso que bloquea
> todo lo demás.

Lo que quedó, además del arreglo:

- La llave correcta (`autosigner@kernel.org`) **y su huella**, impresas. Sin la
  huella, `--locate-keys` le pregunta a la red de quién es una dirección y
  *Good signature* solo demuestra que quien contestó es consistente consigo
  mismo.
- El aviso de gpg de que la llave no está certificada se explica como esperado.
  Sin eso, la persona cuidadosa se detiene en la advertencia correcta.
- La huella y la llave quedaron también **junto a `KSHA256`**, porque un digest
  a secas no se puede volver a comprobar: dice qué se aceptó y no qué lo
  estableció.
- Una prueba que lee el `Makefile` y exige las dos copias, porque dos copias de
  una instrucción es exactamente cómo sobrevive la equivocada.

## Regla derivada: lo que el anfitrión hacía gratis, tercera vez, y ahora era la consola

Encontrada el 2026-08-06, en el primer arranque hecho por un firmware en vez de
por QEMU. La máquina no sobrevivió a su primera instrucción:

```
Warning: unable to open an initial console.
Run /init as init process
traps: init[1] general protection fault ip:7fea0faff143
Kernel panic - not syncing: Attempted to kill init! exitcode=0x0000000b
```

La instrucción que falló es `hlt` —privilegiada, así que en espacio de usuario
es una falta de protección general— y está al final de `abort()` de musl: la que
sólo se alcanza cuando mandarse `SIGABRT` **no** mató al proceso. Y no lo mata,
porque **el kernel no le entrega a PID 1 una señal fatal por acción
predeterminada**. `abort()` se salió por su propio final.

Lo que llamó a `abort()` no fue código de Thalyx: no llegó a correr. El runtime
de Rust, **antes de `main`**, comprueba que los descriptores 0, 1 y 2 estén
abiertos y los apunta a `/dev/null` cuando no lo están — si no, el siguiente
archivo que abras se vuelve tu salida estándar en silencio. Sin consola **y** sin
`/dev/null`, esa garantía no se puede dar y el runtime aborta. Faltaban los dos,
por la misma causa: el archivo tenía un `/dev` vacío.

**Y por qué ningún arranque anterior lo vio**: `-initrd` entrega el cpio aparte,
y un initrd externo se desempaqueta **encima** del initramfs propio del kernel,
que trae `/dev/console`. Meter el nuestro adentro del kernel *reemplaza* ese
predeterminado en lugar de sumarse a él. La consola llegaba de regalo desde algo
que nadie había mirado.

> Es la tercera vez, con la misma forma exacta: **systemd** delegaba los
> controladores de cgroup, **el initramfs** hacía el `switch_root`, y ahora **el
> archivo predeterminado del kernel** ponía la consola. Cada vez, quitar la capa
> de abajo dejó al descubierto un trabajo que el diseño nunca hizo porque nunca
> tuvo que hacerlo. **Cuando se sustituye algo que venía de fuera, lo que hay que
> buscar no es lo que ese algo hacía mal: es lo que hacía y nadie escribió.**

Y un corolario sobre el conteo. `/dev/console` es una entrada más en el archivo,
y `is_directory()` la habría clasificado como programa: `make -C image count`
habría dicho **2** y el decreto se habría roto solo, con un nodo de dispositivo.
La respuesta no es excluirla del conteo —lo no contado es justo por donde
entraría un segundo programa sin que el número se moviera— sino **contarla como
su propia clase**, imprimirla con sus números, y afirmar que las tres clases
suman el total. Una entrada de una clase que nadie previó se nombra en vez de
desaparecer.

## Regla derivada: un escritor de un formato ajeno necesita la misma muestra capturada que un lector

**La regla del fixture inventado y la de la constante capturada valen igual —y
con más urgencia— cuando el código *escribe* el formato en vez de leerlo. Un
lector que se equivoca da una respuesta mala; un escritor que se equivoca produce
algo que nadie puede leer.**

Salió escribiendo `thalyx-btrfs`, el `mkfs.btrfs` propio. Lo obligaba
[[Filosofia-Fundacional]] —la imagen es el kernel y un programa, así que no puede
llevar `mkfs.btrfs`— y es la misma forma que `bpftool` y que `cpio`.

Btrfs firma cada bloque con CRC32C y usa **el mismo primitivo con dos
convenciones distintas**:

- La suma de un bloque o del superbloque es CRC32C estándar: complemento al
  entrar y al salir, porque pasa por el shash del kernel.
- El hash del nombre de una entrada de directorio es el primitivo **crudo**,
  arrancando en `~1`, **sin complemento final**.

La primera versión aplicó la convención estándar a las dos. El hash de `default`
salió 1916812589 en vez de 2378154706: **un número estable, plausible, y que hace
que el kernel resuelva el subvolumen por omisión encontrando nada**. Leer el
código del kernel no lo habría evitado — una llamada va a `crypto_shash` y la
otra a `__crc32c_le`, y la diferencia está en el intermediario, no en la
llamada.

Lo que lo encontró fue una imagen real. `mkfs.btrfs` escribió una, se le calculó
el hash a su entrada `default`, y las dos convenciones quedaron establecidas
midiéndolas. Hay una prueba por cada una y **una tercera que afirma que no
coinciden**, porque «seguro son la misma función» es exactamente el pensamiento
que produjo la versión mala.

Los dos instrumentos, ninguno de los cuales es leer el formato:

1. `crates/thalyx-btrfs/tests/uapi_btrfs_tree.h` y `uapi_btrfs.h` son los
   headers de Linux capturados verbatim, y `tests/layout.rs` los parsea y
   comprueba **cada tamaño y cada offset** que el escritor usa. Los dos archivos,
   porque los structs están en uno y las cotas que los dimensionan en el otro.
2. `tests/against_btrfs_progs.rs` le da lo escrito a `btrfs check`, que recorre
   los árboles, sigue las referencias inversas y cuadra la contabilidad de cada
   grupo de bloques. Se salta donde falta btrfs-progs, dice `NOT PROVEN`, y
   `THALYX_REQUIRE_BTRFS_PROGS=1` convierte el salto en fallo.

Y una frontera dicha en voz alta: **`btrfs check` no es un montaje.** Lee con el
código de btrfs-progs, no con el del kernel, y los dos ya se han contradicho.
Que el montaje funcione sólo lo puede establecer la máquina de Cesar, y es la
etapa 18 de `verify.sh`.

## Regla derivada: un marcador de versión omitido no falla al parsearse, se parsea como otro formato

**El peor error posible en un formato binario no es el que no parsea. Es el bit
que dice en qué versión está escrito lo demás, porque sin él todo parsea — como
otra cosa.**

El primer sistema de archivos que escribió Thalyx tenía los ocho árboles en su
sitio, las tres chunks mapeadas, cada clave donde debía, y `btrfs inspect-internal
dump-tree` lo imprimió entero y correcto. `btrfs check` reportó **once fallas de
referencia**, una por cada extent que existe:

```
ref mismatch on [1048576 16384] extent item 0, found 1
tree extent[1048576, 16384] parent 1048576 has no backref item in extent tree
```

La causa era un bit: `BTRFS_MIXED_BACKREF_REV << 56` en el campo `flags` de la
cabecera de cada bloque. Sin él la revisión es 0, que significa el formato
**viejo** de referencias inversas, así que los extent items se estaban leyendo
con un layout distinto del que se escribieron. El otro bit puesto en ese campo
está en la posición 0, así que un hexdump de la cabecera se ve completamente
normal.

Lo que lo delató no fue ninguna de las once líneas de error: fue una línea
informativa de `dump-tree` que decía `backref revision 0` donde la imagen de
referencia decía `backref revision 1`. **El síntoma estaba lo más lejos posible
de la causa**, y el diagnóstico salió de comparar contra una muestra real, no de
leer los errores.

Generaliza: cuando un formato lleva su propia versión, esa versión se comprueba
aparte y explícitamente. Hay una prueba que sólo afirma ese bit, y dice en su
comentario qué pasó sin él.

## Regla derivada: un control que daña un sitio concreto tiene que sacar su copia antes de que ese sitio pueda moverse

**Cuando el control consiste en romper algo y exigir que se note, la copia que se
rompe se saca antes de que el sistema haya tenido oportunidad de mover lo que se
va a romper. Si no, se daña espacio libre, no se nota nada, y el informe acusa al
sistema de aceptar basura.**

La etapa 18 monta un sistema de archivos que escribió Thalyx, y su control es el
mismo sistema de archivos dañado: si el kernel lo monta igual, el montaje de
arriba no demostró nada. La primera versión sacaba la copia **al final**, después
de montar, crear los tres subvolúmenes y escribir un archivo. Cesar lo corrió el
2026-08-07 y salió:

```
FAILED  the kernel mounted a filesystem with both copies of its root tree damaged,
        so the mount above establishes nothing about the format being right
```

El kernel no aceptó basura. **Btrfs es copy-on-write**: la primera transacción que
el kernel confirma escribe un árbol raíz *nuevo* en otro sitio y retira el que
Thalyx había escrito. Los bytes que el control estaba pisando eran espacio libre
de la generación 1. El sistema de archivos dañado montaba perfecto porque no
estaba dañado en nada que se use.

Dos cosas que valen más que el arreglo:

1. **El informe acusaba al kernel de un defecto del arnés**, y lo decía con
   confianza, en una línea escrita precisamente para no dejarse engañar. Es la
   quinta regla otra vez, y van nueve.
2. **La prueba equivalente en `cargo test` pasaba**, y sigue pasando, porque ahí
   el sistema de archivos nunca se monta: el bloque sigue vivo. Una prueba y una
   etapa que afirman lo mismo pueden diferir en si el sujeto se movió mientras
   nadie miraba, y **la que se mueve es la que corre en la máquina de verdad**.

Arreglado sacando la copia antes de cualquier montaje. Y con una línea base para
el control mismo: **se comprueba que la copia dañada difiere de la original.** Un
`cp` que falló o un `dd` que no escribió nada dejan una imagen intacta, que monta
— y eso se reportaría otra vez como el kernel aceptando basura, que es la
conclusión equivocada a la que esta etapa ya llegó una vez.

## Regla derivada: un arnés que borra la evidencia del fallo que acaba de reportar ha vuelto ese fallo indiagnosticable

**Un informe que dice «ver tal archivo» tiene que dejar ese archivo. Si el que lo
escribió lo borra al salir, la frase es falsa, y se ve idéntica a una verdadera.**

`verify.sh` construye su directorio de trabajo con `mktemp -d` y lo borra en el
`trap` de salida. Unos treinta mensajes de fallo terminan en `see
$WORK/algo.log`. **Los treinta apuntaban a una ruta que ya no existía** en el
momento en que alguien iba a leerla.

Se encontró el 2026-08-07 de la peor manera posible: clippy falló en la máquina de
Cesar, pasó en el contenedor de desarrollo contra el mismo código —comprobado
cuatro veces— y **el único artefacto que podía decir qué lint era lo había borrado
el script que lo escribió.** Cuatro intentos se gastaron buscando un fantasma
porque el informe no dejaba lo que decía dejar.

> **Resuelto el mismo día, y no era lo que se creía.** Cesar volvió a correrlo con
> los arreglos de abajo puestos, y el informe imprimió el lint:
> `unnecessary_sort_by`, dos veces en `format.rs`. Era **desfase de versión** — su
> clippy es 1.97 y el del contenedor era 1.94, y el lint aprendió el caso en
> medio. Nada que ver con `RUSTUP_HOME`. Ver la regla propia más abajo.

Tres arreglos, y los tres son de la misma familia:

1. **El directorio se conserva cuando algo falló**, y el resumen dice dónde está.
   Una corrida limpia sigue sin dejar nada.
2. **Los diagnósticos se imprimen**, no se referencian. Casi todas las demás
   etapas ya hacían `tail` de su log; ésta no.
3. **«clippy objetó al código» y «clippy no pudo correr» son hechos opuestos** y
   se reportaban con la misma línea. Ahora el segundo es `NOT PROVEN` y dice qué
   componente instalar. Regla 10, en el sitio donde costó un diagnóstico.

Y una cuarta cosa, que **no** era la causa de lo de Cesar y sigue siendo un defecto
por su propia cuenta: el arreglo del entorno de rustup bajo `sudo` —poner
`RUSTUP_HOME` y `CARGO_HOME` apuntando a la instalación del usuario— estaba
**condicionado a que `cargo` *no* estuviera en el `PATH` de root**. Pero que
`command -v cargo` encuentre el shim de rustup dice que el archivo está en el
`PATH`; **no dice que ese shim pueda resolver una cadena de herramientas**, porque
la busca bajo `$HOME/.rustup` y `sudo` pudo haber puesto `$HOME` en `/root`. La
reparación estaba condicionada a una prueba que no mide lo que repara — misma
forma que la regla de la precondición que comprueba un artefacto de quien la
escribió — y el fallo que deja es **por componente**: una cadena que contesta
`build` y `fmt` pero no `clippy` se reporta como clippy encontrando problemas.
Ahora se aplica siempre que se corra bajo `sudo`.

Y por eso la etapa 1 imprime ahora la **versión** de la cadena de herramientas y
no sólo su ruta: una corrida contra otra cadena se ve idéntica a una corrida
contra la esperada, que es la misma razón por la que el encabezado nombra el
commit. Fue justo lo que hacía falta — ver la regla siguiente.

## Regla derivada: un instrumento tiene versión, y otra versión es otro instrumento

**Cuando dos máquinas dan respuestas distintas sobre el mismo código, la primera
cosa a comparar es la versión de lo que preguntó, no el código.**

El 2026-08-07 clippy falló en la máquina de Cesar y pasó en el contenedor cuatro
veces seguidas: 1.94 en limpio, 1.90 en limpio, 1.90 incremental sobre el cambio, y
con su invocación exacta en un directorio de compilación vacío. La conclusión que
se sacó fue *«no reproducible, probablemente el entorno de rustup bajo sudo»*, que
era una hipótesis razonable sobre un mecanismo real y **era falsa**.

Su clippy era **1.97**; el del contenedor, **1.94**. `unnecessary_sort_by` aprendió
a ver `sort_by(|a, b| a.0.cmp(&b.0))` en algún punto entre las dos. Actualizado el
contenedor, el fallo apareció en el primer intento, idéntico al suyo, y el arreglo
fueron dos líneas.

Es la regla 5 otra vez —el instrumento incluye al arnés— con una vuelta que valía
escribir aparte: **el instrumento incluye su número de versión**, y un linter es un
instrumento cuyo trabajo entero es cambiar de opinión entre versiones. Cuatro
corridas «con el mismo `rustc`» compararon 1.94 contra 1.94 y se leyeron como
prueba de que el código estaba bien.

Dos consecuencias, y la segunda es la que importa:

1. **La etapa 2 imprime la versión de clippy**, en la línea que pasa y en la que
   falla. Un informe que dice «clippy objetó» sin decir cuál clippy manda a alguien
   a buscar en el sitio equivocado, que es exactamente lo que pasó.
2. **El contenedor de desarrollo se mantiene al menos tan nuevo como su máquina.**
   Lo contrario garantiza que los lints se descubran en la única máquina que no
   puede arreglarlos, y que cada uno cueste una ronda entera de ida y vuelta.

No se fijó la cadena de herramientas con un `rust-toolchain.toml`, que es la otra
respuesta posible. Sería decisión de Cesar y tiene un costo real —lo obligaría a
descargar una versión concreta— y además cambia el problema en vez de resolverlo:
un proyecto que fija su linter deja de enterarse de los lints nuevos hasta que
alguien mueva el archivo, y enterarse es lo que se quería.

## Regla derivada: un formato cuyo lector lo ignora en vez de rechazarlo hace que escribirlo mal se vea igual que no haberlo escrito

**Antes de creerle a un escritor, hay que saber qué hace el lector con lo mal
escrito. Si lo rechaza, el fallo se ve. Si lo ignora, el fallo es invisible y se
parece a no haber hecho nada.**

Salió al escribir el instalador, el 2026-08-07. Una tabla de particiones GPT lleva
dos sumas —una del encabezado y otra del arreglo de entradas— y **Linux no reporta
una GPT que no cuadra: la descarta.** Cae al MBR protector, no crea ninguna
partición, y el disco vuelve igual que si nadie lo hubiera tocado. El instalador
habría dicho `ok` y el disco no arrancaría, sin un solo mensaje sobre por qué.

Es distinto del Btrfs de la etapa 18, donde un superbloque dañado hace que
`mount(2)` conteste un error: ahí el fallo llega. Aquí no llega nada.

Dos consecuencias, y las dos están en la etapa 20:

1. **La comprobación no es «el instalador terminó», es «el kernel hizo dos
   particiones»**, leídas de `/sys/block/…` y no de Thalyx. Y con línea base: se
   afirma primero que el disco **no** tenía particiones, porque «hay dos» también
   es cierto de un disco que ya las tenía.
2. **Los tamaños que el kernel reporta se comparan contra lo que `--plan` dijo.**
   Un plan que describe un disco y un escritor que hace otro no se nota de ninguna
   otra forma — las dos mitades son código distinto y sólo el kernel las junta.

La forma general: **hay que preguntarse qué imprime el lector cuando lo que lee
está mal, antes de decidir qué prueba a la escritura.** Un lector silencioso
convierte cualquier prueba de la forma «no hubo error» en ninguna prueba.

## Regla derivada: cuando el entorno no puede hacer la cosa en absoluto, el salto necesita su propio discriminador

**«No pasó» y «aquí no puede pasar» se ven idénticos, y la diferencia decide si lo
que se reporta es un defecto de Thalyx o un `NOT PROVEN`. Hay que poder separarlos
con una medición, no con una creencia.**

El 2026-08-07, escribiendo el instalador: `thalyx install` escribió la GPT en el
contenedor de desarrollo, el kernel no creó ninguna partición, y **la lectura
obvia era que la tabla estaba mal**. Los bytes se habían comprobado contra
`block/partitions/efi.h` capturado y recalculado las dos sumas con un CRC-32
independiente, así que la tabla estaba bien — y aun así no había forma de *saberlo*
desde adentro.

Lo que lo resolvió fue escribir **un MBR común, con una partición de tipo `0x83`**,
y ver que tampoco producía nada. El parser de MBR está en todos los kernels de
Linux desde siempre; el de GPT es una opción aparte. Que los dos fallaran igual
dice que no es ninguno de los dos parsers: `/sys/block/loop0/range` vale `1`, o
sea que este `loop` no admite particiones de ningún tipo. Es la **regla 5, novena
vez** — el instrumento otra vez, ahora el driver del anfitrión.

Así que la etapa 20 lleva ese discriminador adentro, y no una nota: cuando no
aparecen particiones, escribe un MBR y vuelve a mirar. Si de ése tampoco salen,
dice `NOT PROVEN` con el motivo; si de ése sí salen, **entonces** el fallo es de
Thalyx y lo dice como fallo.

Lo que hay que evitar es lo que casi se hizo: poner el salto por delante —«si el
kernel no soporta X, saltar»— sin nada que compruebe que la razón del salto es la
razón verdadera. Un salto así se dispara también el día que Thalyx se rompe, y
entonces el defecto sale como `NOT PROVEN` para siempre.

## Regla derivada: un decreto que nadie implementó se lee igual que uno implementado, y la bóveda no lo distingue

**Que una nota diga cómo funciona algo no es que funcione. Cuando un decreto se
resuelve escribiendo la decisión y no el código, hay que decirlo en la misma
frase — porque a los tres días nadie puede notar la diferencia leyendo.**

El 2026-08-06 se decretó que una máquina instalada encuentra su store **por la
etiqueta del sistema de archivos**, con su razonamiento entero: por qué no es la
sonda que `store_disk.rs` prohíbe, qué pasa si no hay ninguna, qué pasa si hay dos.
Quedó escrito en [[Construccion-del-ISO]] y en [[Tareas-Pendientes]] marcado con
`[x]`, o sea **resuelto**.

**No se escribió una sola línea de código.** `store_disk.rs` seguía leyendo
`thalyx.store=` y contestando *«nadie me dijo cuál es el disco»* cuando no estaba —
que es exactamente lo que le pasa a una máquina instalada, siempre, porque la línea
de comandos va compilada dentro del kernel.

Se encontró el 2026-08-07, un día después de construir el instalador entero, al ir a
preguntarse qué faltaba para cerrar la fase. **El instalador estaba terminado y el
disco que producía habría arrancado reportando que no tiene store.** Todo lo demás
funcionando no habría alcanzado, y nada lo habría dicho antes de que Cesar arrancara
la máquina.

Lo que lo hizo invisible es la forma del `[x]`: una tarea de *decreto* se marca
resuelta cuando la decisión está tomada, y una de *implementación* cuando el código
existe, y las dos se ven igual en la lista. Peor: la nota técnica describía el
mecanismo en presente —*«Thalyx lee el superbloque de Btrfs de cada dispositivo»*—
que es como se describe algo que existe.

Tres consecuencias:

1. **Un decreto sin código lo dice en su propia línea.** No al final, no en otra
   nota: donde se marca resuelto. `[x]` significa *decidido*, y si además está
   construido, eso se escribe.
2. **Una nota técnica describe en presente sólo lo que existe.** Lo decidido y no
   construido va en futuro o con la palabra «decretado» delante. Es la misma regla
   que ya rige para las afirmaciones de atomicidad, extendida a los mecanismos.
3. **Y la comprobación que lo habría atrapado**: `thalyx disk find` corre el código
   de PID 1 sin ser PID 1. Existe porque la rama que niega dos discos con la misma
   etiqueta, si no, se ejecutaría por primera vez en la máquina de alguien el día en
   que equivocarse es más caro — y porque una función que nadie puede llamar es una
   función que nadie nota que no está.

## Regla derivada: un marcador que identifica algo tiene que ser algo que sólo eso tenga

**Buscar «lo que mi cosa tiene» encuentra todo lo que lo tiene. El marcador que
identifica una cosa no es el que ella cumple, es el que nadie más cumple — y la
diferencia entre los dos no se ve en la máquina de desarrollo hasta que se ve.**

El instalador, cuando nadie le nombra un kernel, busca el medio del que arrancó
la máquina. Lo buscaba así: **el dispositivo de bloques que tenga
`\EFI\BOOT\BOOTX64.EFI`**, y si hay dos, negarse.

El razonamiento estaba escrito y era el correcto en su forma: no es la sonda
prohibida —*probar `/dev/vda`, luego `/dev/sda`, y quedarse con el primero que
conteste*—, es pedir un nombre que Thalyx escribió, y dos respuestas se niegan en
vez de resolverse.

**Salvo que ese archivo no lo escribió Thalyx.** `\EFI\BOOT\BOOTX64.EFI` es el
*removable-media fallback* de la especificación UEFI: es la ruta que una firmware
busca cuando no le han configurado nada, o sea la ruta que está en **todos los
medios de arranque que existen**. La partición EFI de la máquina en la que uno está
sentado la tiene. Una USB de instalación de Windows la tiene. La de Fedora la tiene.

Pedir ese archivo no es preguntar por Thalyx, es preguntar por UEFI.

El 2026-08-07 la etapa 20 instaló un segundo disco sin `--kernel`, la búsqueda
encontró la ESP **de la Fedora de Cesar**, y Thalyx copió el gestor de arranque de
otro sistema operativo al disco y reportó una instalación correcta. Lo único que lo
dijo fue la comparación byte a byte del final:

```
FAILED      the kernel copied off the medium is not the kernel that was installed
```

Y esa línea no dice nada de esto. Mandó a mirar el lector de FAT, que estaba bien.

Tres cosas de aquí:

1. **El marcador es la etiqueta, no la ruta.** El volumen FAT32 que Thalyx escribe
   se llama `THALYX` —en el sector de arranque y en la entrada del directorio raíz—
   y ese nombre lo puso Thalyx. La ruta sigue pidiéndose; lo que cambió es que ya no
   es lo que decide. Es la misma forma que el store por su etiqueta `thalyx-store`,
   y el paralelo debió haberse visto el mismo día en que se construyó el otro.
2. **La máquina de desarrollo es el control, no el estorbo.** Una PC con UEFI tiene
   una ESP propia; una Thalyx recién instalada no tiene ninguna otra. O sea que el
   falso positivo **sólo se puede ver en la máquina de Cesar**, y por eso la
   comprobación nueva de la etapa 20 —`thalyx disk medium` tiene que quedarse con la
   partición que Thalyx escribió— vale doble: afirma que encuentra la suya y que no
   se lleva la ajena.
3. **Un `cmp` que sólo dice «difieren» ha reportado un fallo sin diagnosticarlo.**
   La etapa ahora imprime, cuando falla, de qué dispositivo dijo el instalador que
   estaba leyendo, y el tamaño de los dos archivos. Un gestor de arranque de 950 KB
   al lado de un kernel de 12 MB se explica solo.

## Regla derivada: un `default y` de Kconfig es un `n` bajo `allnoconfig`, y un menú invisible se lleva a sus hijos

**Cuando `config-check` nombra tres opciones descartadas, la causa puede ser una
sola línea que no está — y no es ninguna de las tres.**

`make -C image` se detuvo con:

```
  These were asked for and are not in the kernel's .config:
    CONFIG_HID=y
    CONFIG_HID_GENERIC=y
    CONFIG_USB_HID=y
```

Las tres estaban bien escritas y ninguna era la causa. `drivers/hid/Kconfig` empieza
con `menuconfig HID_SUPPORT`, que es `default y` y `depends on INPUT`, y **todo lo
demás de ese archivo —más `usbhid/Kconfig`— vive dentro de su `if`**. Bajo
`allnoconfig`, que parte de nada, un `default y` es un `n`: el menú queda apagado,
sus hijos quedan invisibles, y `olddefconfig` los descarta sin decir por qué.

Lo que hace difícil verlo es justo lo que hace cómodo el resto del tiempo: nadie que
escriba un `.config` a mano se topa nunca con `HID_SUPPORT`, porque en cualquier
configuración normal ya está encendido. Es un símbolo que sólo existe para quien
parte de cero.

**El chequeo que ya existía hizo su trabajo**: sin `config-check` esto habría sido un
kernel que arranca perfecto y no responde al teclado, en una PC sin puerto serie, o
sea una máquina muda que parece colgada. Lo que faltaba no era una comprobación sino
la lectura: *un grupo de opciones descartadas juntas comparte una causa, y hay que
buscar el `menuconfig` que las contiene antes que las dependencias de cada una*.

## Regla derivada: un control destructivo repara lo que rompió, y la reparación se afirma

**Un control que daña algo y lo deja dañado no terminó cuando pasó: todo lo que
viene después sigue usando esa cosa. El daño se deshace en la misma línea que lo
hizo, y que se deshizo es una afirmación más, no un supuesto.**

Es la séptima vez que *el instrumento incluye al arnés*, y la primera en que el
arnés no se equivocó al medir — se equivocó al dejar el mundo peor de como lo
encontró.

La etapa 20 monta la partición de arranque que el instalador escribió, compara el
kernel byte a byte, y después **daña las dos copias del sector de arranque** para
comprobar que un vfat roto no se monta. Eso es la regla 4 bien aplicada: sin el
control, un kernel que monte cualquier cosa se ve igual que uno que valida el
formato.

Lo que faltaba es la línea siguiente. **Nadie reparaba la partición**, y las
comprobaciones de abajo la siguen usando: la búsqueda del medio, la instalación sin
`--kernel`, el segundo disco, la negativa ante dos medios. El 2026-08-07 eso produjo
**cinco fallos de una sola causa**, y el primero de ellos decía:

```
FAILED   the kernel copied off the medium is not the kernel that was installed
```

que manda a mirar el lector de FAT. El lector estaba bien. Lo que había pasado es
que cuarenta líneas antes el arnés había destruido el único medio Thalyx de la
máquina, así que la búsqueda encontró **la partición EFI de la Fedora del anfitrión**
—que también lleva `\EFI\BOOT\BOOTX64.EFI`, porque esa ruta la lleva todo— y copió
su gestor de arranque al disco.

Dos defectos distintos, uno encima del otro:

- **El del código**, que sigue siendo real y ya está arreglado: pedir esa ruta no es
  pedir Thalyx. Sin la etiqueta, con el medio sano, habría dos respuestas y la
  instalación se habría negado — correcta, pero imposible en la máquina de nadie.
- **El del arnés**, que es éste. Y hay que decirlo aparte: *arreglar el primero no
  arreglaba el segundo*, y el segundo era el que estaba produciendo el mensaje.

Tres cosas:

1. **El control saca su copia antes y la devuelve después.** Ya existía la regla
   hermana —*un control que daña un sitio concreto tiene que sacar su copia antes de
   que ese sitio pueda moverse*— y le faltaba esta mitad: no basta con saber **qué**
   bytes se van a romper, hay que ponerlos de vuelta.
2. **La reparación se afirma con su propia línea.** Una reparación que
   silenciosamente no funcionó se ve **idéntica** al bug original: todo lo de abajo
   falla y nada dice por qué. La afirmación es lo único que separa las dos.
3. **Un control destructivo pertenece al final de lo que usa esa cosa, o repara.**
   Las dos salidas son válidas; lo que no es válido es ninguna de las dos, que es lo
   que había.

## Regla derivada: lo que el anfitrión hacía gratis, cuarta vez, y el anfitrión era el firmware

Encontrada el 2026-08-07 **leyendo la configuración** y no arrancando, que es la
única razón por la que no se encontró con la memoria USB ya puesta en una PC.

Al preparar el acto 2 —`dd` a una USB, arrancar una PC, `discos`,
`instalar-en /dev/nvme0n1`— resultó que `image/thalyx.config` no pedía
**`CONFIG_USB_STORAGE`**. Ni `USB_UAS`. O sea: el kernel de Thalyx no tenía cómo
ver un disco USB.

Lo que hace a este caso de la familia es **por qué no se notaba**. La
especificación UEFI obliga al **firmware** a leer el medio de arranque, con su
propio controlador, antes de que exista kernel alguno. Así que la máquina:

- arranca de la USB perfectamente,
- desempaqueta su initramfs, monta sus siete sistemas de archivos, engancha el
  LSM y saca su prompt en la pantalla,
- y **falla dos comandos después**, cuando `instalar-en` busca el medio del que
  arrancó recorriendo `/sys/block` y no lo encuentra, porque el kernel enumeró la
  USB como dispositivo USB y nunca le dio un dispositivo de bloque.

El mensaje sería *«no encuentro un medio de Thalyx»*, en una máquina que está
visiblemente corriendo desde uno. Y en ningún punto aparece la palabra USB.

> Es la cuarta vez, y la lista completa es: **systemd** delegaba los controladores
> de cgroup, **el initramfs externo** hacía el `switch_root`, **el archivo
> predeterminado del kernel** ponía `/dev/console`, y ahora **el firmware** lee el
> disco de arranque. Cada vez, algo de afuera hacía un trabajo que el diseño nunca
> tuvo que hacer.
>
> Lo nuevo aquí es que **la capa de abajo no se quitó**. El firmware sigue ahí y
> sigue haciendo su trabajo: por eso el arranque no se rompe. La forma es un poco
> peor que las tres anteriores — **cuando algo de fuera hace un trabajo, hay que
> preguntar si *nosotros* también vamos a necesitar hacerlo, porque que ellos lo
> hagan no nos exime y además nos oculta que no sabemos.**

Y la mitad que corresponde al arnés. `config-check` en `image/Makefile` detiene el
build cuando `olddefconfig` **descarta** una opción pedida, y **no puede ver una
que nadie pidió** — que es exactamente lo que costó `CONFIG_SECURITY_NETWORK` y un
arranque entero. Lo que cubre ese hueco son las pruebas que leen `thalyx.config` y
afirman opción por opción, con su motivo escrito al lado; ya había cuatro y ahora
son cinco. La nueva se comprobó en las dos direcciones: comentando la línea, falla.

**El corolario para la lista de controladores.** La tabla de riesgo de
[[Construccion-del-ISO]] tenía tres filas —pantalla, teclado, discos— y las tres
preguntaban *«¿qué hardware tiene la PC?»*. Ésta no estaba en ninguna, porque la
pregunta que la encuentra es distinta: *«¿qué tiene que leer Thalyx, además de lo
que el firmware ya leyó por él?»*. Un inventario de hardware no la contiene.

## Regla derivada: un umbral por gravedad no ve la repetición, y lo que arruina una interfaz es la repetición

Encontrada el 2026-08-07, en el primer arranque de Thalyx sobre una PC de verdad.

`init.rs` bajaba la consola del kernel al nivel 4 y explicaba por qué en una
frase que describe el síntoma **antes de que ocurriera**:

> *From here there is a human at a prompt, and an info-level message arriving
> mid-line steps on it — the machine looks like it stopped listening.*

Y aun así ocurrió. La máquina de Cesar tenía un receptor inalámbrico que no
enumeraba, el kernel lo reintentó para siempre, y
`usb 1-6: device descriptor read/64, error -110` —prioridad 3, un error,
**correctamente clasificado**— cayó sobre el prompt cada pocos segundos hasta
volver la sesión inusable, con un teclado que funcionaba perfectamente.

**El umbral no estaba mal puesto: estaba puesto sobre el eje equivocado.**
Filtrar por gravedad contesta *«¿esto importa?»*, y lo que destruye una interfaz
no es que un mensaje importe, es que vuelva. Un mensaje que se repite sin parar
dejó de ser información — dice lo mismo la centésima vez que la primera — y
ningún nivel de prioridad puede distinguirlo, porque la repetición no es una
propiedad del mensaje sino de la serie.

**El corolario, que es lo que se construyó:** cuando lo que se quiere es un canal
utilizable, la respuesta no es elegir mejor qué dejar pasar sino **dejar de
retransmitir el flujo ajeno y hablar de él**. La consola quedó en emergencias, y
el prompt anuncia con una línea propia que hay problemas nuevos y que `nucleo` los
tiene. Eso no es esconder: el buffer del kernel conserva cada palabra, y bajar el
volumen sin la segunda mitad sí habría sido esconder.

### Y las dos mitades del mismo criterio no coincidían

Leyendo eso apareció un segundo defecto, más chico y del mismo día. El nivel de
consola suprime todo lo que tenga prioridad **mayor o igual** que él, así que un 4
tiraba las advertencias — mientras `KernelMessage::is_trouble()` cuenta la
prioridad 4 **como** problema. `nucleo` las llamaba problemas y la consola las
tiraba, y la línea impresa en el arranque decía *«warnings and worse only»*, que
es exactamente lo que no hacía.

**Un mismo juicio implementado en dos lugares se desincroniza en silencio**, y la
dirección en que se rompió es la peor: la pantalla afirmaba mostrar más de lo que
mostraba. La regla hermana ya existe para las precondiciones —*una comprobación y
el uso que la sigue son dos momentos*—; ésta es su versión para un criterio
partido en dos módulos, y la señal de alarma es la misma: si dos lugares deciden
lo mismo, uno de los dos va a cambiar solo.

## Regla derivada: «vacío» y «esa pregunta no aplica» son la misma respuesta cuando el llamador sólo mira si hubo error

Encontrada el 2026-08-07, cuando `discos` corrió por primera vez en una máquina
con particiones de verdad y contestó **siete discos**, de los cuales cuatro eran
particiones — incluidos los 444 GiB de la Fedora de Cesar, listados bajo una
línea que dice *«everything on it is lost»*.

El filtro estaba escrito y explicado:

```rust
// A whole disk is one sysfs knows as a disk rather than as somebody's
// partition, and `partitions::of` answers for the first and errors for
// the second.
thalyx_install::partitions::of(device).is_ok()
```

**Y `of` no erraba para una partición.** Busca
`/sys/dev/block/<major>:<minor>`, que existe igual para las dos; `read_dir` sobre
una partición funciona; y como una partición no tiene hijos con archivo
`partition`, la respuesta era `Ok([])`. *«Este disco no tiene particiones»* y
*«esto no es la clase de cosa que tiene particiones»* salían por el mismo canal,
y el llamador sólo miraba `is_ok()`.

**La regla:** una función que contesta *«nada»* con un `Ok` vacío hace
indistinguible el caso vacío del caso inaplicable, y cualquier llamador que use
`is_ok()` como predicado está preguntando otra cosa de la que cree. El arreglo no
es un `if` en el llamador: es que la función **se niegue** cuando la pregunta no
aplica, para que la propiedad que el comentario afirmaba exista de verdad.

Es la familia de la regla 10 —*una falla al leer no es una falla al existir*—
girada un cuarto de vuelta: allí eran ausencia y error los que se confundían,
aquí son ausencia e **inaplicabilidad**. El discriminador correcto ya estaba en
el kernel y nadie se lo preguntó: **el archivo `partition`**, que existe sólo
dentro del directorio de una partición y lleva su número.

### Y el comentario era la única prueba que existía

Nada afirmaba esa propiedad. La frase *«answers for the first and errors for the
second»* iba en un comentario, que es donde una afirmación no puede fallar. **Un
comentario que enuncia una propiedad es una prueba que nunca corre**, y lo único
capaz de contradecirlo era una máquina con particiones — que durante semanas fue
ninguna máquina.

### La mitad que importaba de verdad

`discos` **ofrecía** esas particiones como destino, y `install` las habría
aceptado: escribe la tabla en el LBA 0 de lo que reciba. Una tabla escrita dentro
de una partición es **legal, invisible para toda herramienta que busque una, y no
arranca nada** — mientras el sistema de archivos que había ahí ya no está. Es la
misma forma que la GPT con suma equivocada: el fallo no llega, y el disco vuelve
pareciendo que nadie lo tocó.

Por eso el arreglo va en dos sitios y no en uno: `discos` deja de listarlas, e
`install` se niega **antes de escribir un byte**. Lo primero es presentación; lo
segundo es lo que impide perder un disco cuando alguien teclea el nombre de todas
formas.

## Regla derivada: lo que el anfitrión hacía gratis, quinta vez, y ahora tenía un precio en vez de no existir

Las cuatro veces anteriores el anfitrión hacía algo que en hierro **no existía**:
un directorio ya montado, un controlador ya delegado, un disco que el kernel veía
sin driver, un medio que el firmware leía sin `USB_STORAGE`. Esta quinta vez el
anfitrión hacía algo que en hierro **sí existe y cuesta**, que es peor de
encontrar: nada falta, nada falla, y la máquina simplemente tarda.

`CONFIG_CMDLINE` decía `console=ttyS0`, sin velocidad. Sin velocidad, el driver
8250 usa **9600 baudios**, y `printk` es síncrono — el kernel no avanza hasta que
los caracteres salieron por el puerto. El puerto serie de QEMU es un pty: **no
tiene baudios**, se vacía instantáneamente. Así que el precio era exactamente cero
en todas las máquinas donde esto se había probado.

En un PC real, el 7 de agosto de 2026, eran unos 30 de los 38.5 segundos que
tardaba en arrancar. El chipset todavía trae su UART aunque el gabinete no traiga
el conector, así que el driver lo registra y el kernel le escribe con toda
formalidad a un puerto que no tiene nada del otro lado.

### Y estaba dentro del camino que sí se probaba

`run-uefi` y `run-hardware` **no pasan `-append` a propósito** — ésa es su razón
de existir, arrancar como arrancaría una máquina sin sistema operativo. O sea que
usaban esta misma línea compilada. No es que el defecto viviera en un camino sin
cubrir: vivía en el camino cubierto, y el anfitrión lo pagaba.

**Un objetivo que reproduce el arranque de hierro reproduce sus decisiones, no sus
costos.** Lo que ese objetivo no puede ver es todo lo que en QEMU es gratis y en
silicio se cobra: velocidad de puerto, latencia de disco, tiempos de firmware.

### Lo que lo encontró fue un instrumento, no una lectura

Nadie iba a encontrar esto leyendo el código. La línea es correcta, arranca, y
todo funciona. Lo encontró `nucleo lento`, que resta las marcas de tiempo entre
mensajes consecutivos del kernel y ordena por silencio — y salió un hueco de
18.27s en el segundo 0.07, justo después de `printk: legacy console [ttyS0]
enabled`.

**La posición del hueco descartó la hipótesis, no su tamaño.** La sospecha era que
la memoria USB fuera lenta; un hueco antes de que el kernel toque un disco no
puede ser eso.

De ahí sale la regla, que es sobre instrumentos y no sobre consolas:

> **«Tarda mucho» no es un síntoma hasta que algo dice dónde.** Un total no se
> puede diagnosticar, y una bitácora de 712 líneas tampoco: las dos son la misma
> falta de instrumento. Lo que convierte una queja en un defecto localizado es
> restar dos marcas de tiempo que ya estaban ahí.

Y el corolario, que es el que evita atribuir mal:

> **Un hueco dice a dónde se fue el tiempo, no qué lo tomó.** La línea *después*
> de un silencio es la que terminó de esperar, no la que fue lenta. El verbo lo
> imprime así y no señala culpables.

### Y `config-check` no podía ver ninguno de los dos defectos de ese arranque

El mismo arranque reportó `CPU topo: CPU limit of 2 reached. Ignoring further
CPUs`. Nadie eligió 2: `allnoconfig` corre con SMP apagado, donde `NR_CPUS` es 1,
y encender SMP después sólo lo sube al piso de su rango.

`config-check` compara lo que `thalyx.config` **pide** contra lo que salió. Un
opción que nadie pidió no tiene línea que comparar, así que su valor puede ser
cualquiera y la comprobación pasa limpia. Es el mismo hueco estructural que dejó
pasar `CONFIG_SECURITY_NETWORK` y `CONFIG_USB_STORAGE`, y no se cierra con más
comparaciones:

> **Una comprobación que verifica que se obtuvo lo pedido es ciega a lo que no se
> pidió.** El único lugar donde esa clase de defecto aparece es corriendo la
> máquina, y el único lugar donde se puede fijar después es una prueba que afirme
> el valor — no la comprobación que compara listas.

Por eso las dos afirmaciones nuevas viven en `init.rs` y no en el `Makefile`.

## Regla derivada: una restricción sobre la salida también restringe lo que se puede medir

Encontrada el 2026-08-08 escribiendo el banco de las gamas, **antes de que
ninguna de las dos cosas hubiera corrido**.

La gramática GBNF que restringe al modelo pedía **al menos un id de módulo** en
`targets`. Eso hace imposible un contrato mal formado, que es lo que
[[Gamas-de-Modelo]] promete. Y hacía imposible otra cosa que nadie buscó:
**abstenerse.**

`Gamas-de-Modelo` dice que la abstención es la medición que más importa —
*«un agente que se equivoca pidiendo confirmación cuesta un segundo, y uno que
inventa con confianza cuesta el camino confiable entero»*. Con esa gramática, un
enunciado ambiguo no tenía ninguna respuesta válida que dijera *«no encontré
ninguno»*, así que la única salida gramatical era inventar. El banco habría
reportado **0 de 4 en abstención en las cuatro gamas**, y la lectura obvia habría
sido «los modelos chicos inventan» — cuando lo que pasaba es que **ninguna gama
tenía cómo no inventar**.

> **Una gramática que fija qué se puede decir fija también qué se puede
> declinar.** El repertorio de respuestas legales es el repertorio de conductas
> observables, así que una conducta que la gramática no contempla no se mide como
> ausente: se mide como su contraria, y la culpa cae sobre el modelo.

Dos cosas que hacen esto peor que un error normal:

1. **No falla nada.** Todo compila, todo parsea, el banco corre y devuelve una
   tabla de números plausibles. Es de la familia de *lo que el anfitrión hacía
   gratis*: nada falta, nada se rompe, el número está mal.
2. **La corrección estaba a la mano y sin nombre.** `AgentError::NothingToDo`
   —*«the request names nothing to act on»*— ya existía y ya era la respuesta
   correcta; lo que faltaba era una forma de llegar a ella desde el modelo. Una
   lista vacía la alcanza.

Y la mitad que casi se olvida: **la gramática permitiéndolo no basta, el prompt
tiene que decirlo.** Una respuesta legal que nadie menciona es una que el modelo
no usa, y la gama quedaría medida sobre una decisión que nunca se le ofreció. Las
dos mitades se conceden juntas o no se concede ninguna — hay una prueba por cada
una.

La pregunta que encuentra esta clase de defecto no es *«¿la gramática acepta
respuestas correctas?»* sino **«¿qué respuestas hace imposibles, y alguna de
ellas era una conducta que quiero medir?»**.

## Regla derivada: un sustituto fiel al formato puede ser infiel al contrato

Encontrada el 2026-08-08, la **primera vez que la integración del agente tocó un
`llama.cpp` de verdad**. Es la sexta vez que algo de fuera hacía un trabajo que
nadie había escrito, y la primera en que lo que cambió fue *qué programa es*.

`llama.cpp` partió sus herramientas: **`llama-cli` ya no es la herramienta de
completado, es un frontend de chat interactivo** construido sobre el servidor, y
el completado de una sola pasada vive en **`llama-completion`**. Thalyx pedía
`llama-cli`. Con un archivo de prompt en `-f`, el `llama-cli` nuevo **abre una
sesión sobre el archivo en vez de completarlo**: carga los pesos, imprime su
banner y sus comandos (`/exit`, `/regen`, `/clear`), lee fin de entrada del
`stdin` cerrado y **sale con cero**.

O sea que la herramienta equivocada se ve igual que una herramienta que funciona
y dio una mala respuesta.

### Por qué las pruebas de aquí no lo vieron, y es lo importante

Había siete pruebas contra procesos sustitutos, y estaban bien escritas: cubrían
que la respuesta se recorta del proceso, que un proceso colgado se mata, que 200
kB se cortan, que una bandera rechazada saca su `stderr` íntegro. **Todos esos
sustitutos honraban el contrato de una pasada, porque todos estaban escritos para
contestar.**

La regla 8 dice que un falso tiene que modelar la propiedad bajo prueba. Yo modelé
el **formato de salida** —qué imprime la herramienta y en qué orden— y el eje que
importaba era otro:

> **Un sustituto modela el eje en el que se le escribió variación.** Uno escrito
> para producir salidas distintas prueba el parser y deja el contrato de
> ejecución sin tocar, porque «contestar» es la única conducta que todas sus
> variantes comparten. La pregunta que lo encuentra no es *«¿qué puede imprimir
> esta herramienta?»* sino **«¿qué puede hacer esta herramienta que no sea
> contestar?»**.

### Y el error se disfrazó en el sitio exacto donde había un respaldo

`answer_in` recortaba la respuesta después de un marcador aleatorio, y si el
marcador **no estaba**, devolvía toda la salida. Ese respaldo estaba justificado
por una causa —que la herramienta no repitiera el prompt— y tenía una segunda
que nadie enumeró: **que la herramienta nunca leyera el prompt.**

Así que el banner del chat entró como si fuera una respuesta, `Proposal::parse`
falló, y el mensaje dijo *«el modelo contestó algo que no parsea»*. **Culpó a
Qwen de una pregunta que nunca se le hizo.** Es la regla 10 otra vez —una falla
al leer no es una falla al existir— y esta vez el que confundió las dos era el
respaldo:

> **Un respaldo que cubre una causa cubre en silencio todas las que producen la
> misma señal.** Enumerar por qué se llega a la rama de respaldo es parte de
> escribirla; si sólo hay una causa en la cabeza de quien la escribe, la rama
> convierte a las demás en esa.

### La corrección, que no es cambiar un nombre

Cambiar el binario por omisión a `llama-completion` arregla *este* caso. Lo que
arregla la clase es que **el contrato se comprueba en vez de suponerse**:

- Se dejó de pasar `--no-display-prompt`. El eco del prompt lleva el marcador, y
  el marcador es la **prueba positiva de que el prompt se leyó**. Suprimirlo
  borraba la única evidencia.
- Marcador ausente ⇒ `NotOneShot`: la herramienta no completó el prompt. No es
  una respuesta que falta, es una **pregunta** que falta.
- Marcador presente y respuesta que no parsea ⇒ `GrammarNotInForce`. No es
  heurística: un completado restringido por gramática **no puede** producir prosa,
  así que la prosa demuestra que la gramática no se aplicó.

Ninguna de las tres olfatea la prosa de otra herramienta, que sería la regla 6
otra vez.

## Regla derivada: un límite definido de un solo lado no es un límite

Encontrada el 2026-08-08, **el mismo día y contra el mismo `llama.cpp`**, en la
corrida siguiente. La corrección de arriba funcionó: `llama-completion` cargó los
pesos, completó el prompt y **Qwen2.5-3B emitió exactamente el objeto que
describe la gramática**, con saltos de línea y sangría, que es lo que
`ws ::= [ \t\n]*` permite.

Thalyx lo rechazó. Y no calladamente: dijo que la gramática no se había aplicado
y que **la culpa era de la herramienta**.

`llama.cpp` imprime su propio final de generación detrás del completado —
`tools/completion/completion.cpp`:

```cpp
if (!embd.empty() && llama_vocab_is_eog(vocab, embd.back()) && !(params.interactive)) {
    LOG(" [end of text]\n");
    break;
}
```

`Proposal::parse` era `serde_json::from_str`, que rechaza cualquier byte después
del objeto. Y el marcador aleatorio del prompt decía **dónde empieza** la
respuesta. Nunca nadie dijo **dónde termina**:

> **Un límite definido de un solo lado no es un límite.** Recortar texto prestado
> por el principio deja el final en manos de quien lo imprimió, y ese final
> cambia entre versiones sin avisar a nadie. Donde termina lo que dijo el modelo
> lo decide la gramática, no el sufijo que la herramienta pone después.

La corrección lee **el primer valor JSON completo** después del marcador, porque
la raíz de la gramática es un objeto y todo lo que venga detrás lo escribió quien
sea que estuviera imprimiendo. Recortar el literal ` [end of text]` habría sido
la regla 6 al revés: esa cadena es *una muestra* de la salida de *una* compilación,
no el formato.

### Las fixtures no podían encontrarlo, y no por ser malas

Es la séptima vez que el instrumento se equivocó antes que lo medido, y la
**segunda por la misma causa**: un parser probado sólo contra fixtures que
inventó su autor. La regla 6 ya existía, estaba escrita, y aquí no se siguió — no
había ni una muestra capturada. Lo que faltaba decir es por qué eso es peor que
un descuido:

> **Una fixture no puede estar en desacuerdo contigo.** Hereda el modelo del
> formato que tiene quien la escribe, *incluidas las partes en las que nunca
> pensó*. Las nueve fixtures de este parser terminaban donde el parser esperaba
> que terminara una respuesta, porque las escribió la misma mano. Ninguna
> cantidad de fixtures nuevas iba a estar en desacuerdo sobre eso.

Ahora hay una muestra capturada literal, con su procedencia anotada
(`llama.cpp b1-3653e6d`, `llama-completion`, Qwen2.5-3B-Instruct-Q4_K_M,
2026-08-08), y de ella cuelgan tres pruebas.

### Lo que sí funcionó: el mensaje enseñaba lo que estaba juzgando

El defecto se encontró **de una sola lectura**, y no por suerte. El error
imprimía la respuesta que estaba rechazando, así que debajo de la frase «esto es
la herramienta ignorando la gramática» se veía una propuesta perfecta. La
contradicción saltaba sola.

> **Una comprobación que señala a un culpable tiene que enseñar la evidencia que
> juzgó.** Un mensaje que sólo hubiera dicho *«la gramática no estaba en vigor»*
> era una acusación creíble, específica y falsa, y el paso siguiente habría sido
> auditar el manejo de gramáticas de `llama.cpp`: días en el sitio equivocado.
> Cuanto más confiado el diagnóstico, más obligatorio adjuntar lo que lo produjo.

El mensaje corregido explica además dónde corta, para que la próxima acusación
falsa se delate igual de rápido.

## Regla derivada: probar que algo restringe necesita un enunciado que sin la restricción daría otra respuesta

Del mismo día, y no de un defecto sino de una afirmación que estaba a punto de
darse por buena. `verify.sh` decía *«`llama.cpp` tomó las banderas y la
gramática»* porque `llama.cpp` sale distinto de cero ante una bandera que no
conoce. Eso prueba que `--grammar-file` fue **aceptada**. No prueba que
restringiera nada:

> El prompt real **le pide al modelo un objeto**. Un modelo que da un objeto sólo
> hizo lo que le dijeron. Así que una bandera aceptada y una gramática aplicada
> **producen exactamente la misma evidencia**, y cuatro gamas del decreto se
> apoyaban en no notarlo.

La regla:

> **Una comprobación de que X restringe la salida necesita un enunciado cuya
> respuesta sin X sea distinta.** Si el sistema, libre, habría contestado igual,
> la corrida no midió X — midió al sistema. Y el sondeo tiene que decirlo: hace
> falta un tercer resultado, «no concluyente», o esos dos casos se confunden con
> pasar.

`thalyx agent model grammar-check` pide la única palabra que la gramática no
puede emitir, dos veces, con la bandera y sin ella y sin ninguna otra diferencia
entre las dos corridas. Restringido no puede decirla; suelto sí. Las dos ramas
con propuesta ⇒ `NOT PROVEN`, que es regla 3 y regla 4 juntas: sin la rama de
control, una gramática que no hace nada y una que funciona se ven igual.

El sustituto que lo prueba **modela la dependencia**, no el resultado: dice la
palabra cuando puede y emite un objeto cuando le pasan la bandera. Un falso que
imprimiera siempre un objeto habría pasado la prueba mientras modelaba justo la
herramienta que el sondeo tiene que cazar — regla 8, en su forma más fácil de
equivocar.

## Regla derivada: una falla al terminar no es una falla al cumplir

Encontrada el 2026-08-08, la primera vez que se corrió `grammar-check` contra el
Qwen real. Dijo `FAILED`. **Estaba al revés**, y la prueba de que estaba al revés
venía impresa en su propia salida:

```
with the grammar     {
  "operation": "install_module",
  "targets": ["banana_module_1234567890123456789012345678901234…
```

Eso **empieza con `{`**. Es la gramática funcionando de la forma más contundente
posible: al modelo se le pidió decir `BANANA`, la gramática le prohibió empezar
con `B`, y el modelo desvió el intento al único sitio donde cabía una `b` —una
cadena de id de módulo— y se quedó ahí hasta agotar los 256 tokens. El JSON quedó
sin cerrar.

La comprobación preguntaba *«¿esto parsea?»*. No parseaba **por truncamiento**:

> **Una falla al terminar no es una falla al cumplir.** Una salida cortada por el
> tope de tokens no parsea *y* puede ser la más obediente que el sistema podía
> producir. Juzgar el cumplimiento por si el resultado está completo confunde
> presupuesto agotado con regla violada, y las dos mandan a lugares opuestos.

Es la regla 10 en un sitio nuevo, y es la **octava** vez que el instrumento se
equivocó antes que lo medido.

### Qué se lee en vez de eso

El primer carácter. La gramática fija `root ::= "{" …`, así que una decodificación
realmente restringida **no puede poner otra cosa primero**, esté completa o no. El
discriminante sobrevive al truncamiento porque está en la posición cero, que es
donde la garantía es absoluta.

Y el mismo defecto estaba en el camino de producción, no sólo en el sondeo: una
inferencia normal que se truncara contra `-n` también salía como «la gramática no
está en vigor». Ahora son dos errores distintos, `Truncated` y
`GrammarNotInForce`, y el primero dice que **esto es la gramática funcionando**.

### Lo que lo delató fue una contradicción entre dos corridas

`grammar-check` dijo que la gramática no se aplicaba. `bench`, minutos después,
sacó **nueve propuestas bien formadas de nueve casos**. Las dos cosas no pueden
ser ciertas:

> **Dos instrumentos que se contradicen son un dato, no un empate.** Cuando una
> comprobación estrecha falla y una amplia pasa sobre el mismo sistema, lo
> primero que hay que sospechar es la estrecha — es la que tiene una hipótesis
> más específica que romper.

### Y un hallazgo que el decreto ya advertía en abstracto

`Gamas-de-Modelo` dice que la gramática no acota la longitud de un id. Aquí se vio
lo que eso significa: **un modelo restringido al que se le pide algo ilegal no se
rinde**, gasta todo el presupuesto buscando una manera legal de decirlo. El tope
de tokens es lo único que lo termina. No es un defecto —el tope existe para esto—
pero conviene saber que la fuga tiene esa forma y no la de una negativa.

## Regla derivada: un delimitador que el sistema medido puede escribir no delimita

Encontrada el 2026-08-08 **en una corrida que pasó**. `grammar-check` salió
`PROVEN`, y el brazo de control traía esto:

```
without it           BANANA <<<TH
```

El modelo dijo la palabra y **empezó a reproducir el marcador que acababa de leer
al final del prompt**. Lo cortó el tope de tokens, no una decisión suya.

El marcador es aleatorio por invocación, y esa aleatoriedad estaba razonada
contra un adversario: un texto ajeno no puede contener un marcador que tendría
que adivinar. Correcto, y **no cubría este caso**, porque el modelo no lo adivina:
lo tiene delante.

> **Un delimitador que el sistema medido puede escribir no delimita.** Ser
> imposible de *adivinar* no es ser imposible de *copiar*, y todo lo que se le
> pone al modelo en el prompt es algo que el modelo puede repetir. El ancla tiene
> que ser algo que sólo el instrumento posee, o algo tan grande que reproducirlo
> sea reproducir el instrumento entero.

`answer_in` tomaba la **última** aparición del marcador, así que una copia
completa habría hecho que la respuesta fuera lo que viniera después de la copia
del modelo. Ahora se ancla en el prompt repetido entero —que la herramienta
imprime literal y el modelo no puede falsificar sin emitirlo completo— y el
marcador solo queda de respaldo, tomando la **primera** aparición: el prompt lo
contiene exactamente una vez, y lo que venga después es del modelo.

La gramática tampoco lo impedía: `RANGE_CHARS` contiene `<`, `>`, `-` y los
dígitos hexadecimales, así que un modelo restringido puede deletrear un marcador
dentro de un campo `constraint`. No es un ataque —sigue haciendo falta adivinar—
pero es corrupción accidental con entradas normales, que es peor en un sentido:
no necesita que nadie lo intente.

### Lo que lo encontró fue mirar una prueba que pasó

Nada falló. La comprobación dijo `PROVEN` y tenía razón. El defecto estaba en la
evidencia que imprimió al lado, y sólo porque la imprimía:

> **Una corrida que pasa también trae datos.** Leer únicamente el veredicto de
> las que fallan deja sin abrir justamente las salidas que nadie va a volver a
> mirar. Es la otra mitad de la regla de enseñar la evidencia: enseñarla no sirve
> si sólo se lee cuando el veredicto ya es malo.

## Regla derivada: una rama de error que no distingue causas convierte el fallo en el resultado que más se le parezca

Encontrada el 2026-08-08 leyendo el banco, no leyendo sus números. La
clasificación de cada caso era ésta:

```rust
let outcome = match &plan {
    Ok(plan) => match plan.contract.targets.first() { ... },
    Err(_) => Outcome::Abstained,
};
```

`Err(_)`. **Toda forma de fallar contaba como el modelo absteniéndose bien**: un
plazo agotado, una respuesta truncada, una gramática no aplicada, `llama.cpp`
cayéndose. De donde sale, exactamente:

> Una gama cuyo modelo no arrancara nunca sacaba **4/4 en abstención**, que es la
> medida que [[Gamas-de-Modelo]] llama la más importante.

Es hermana de la regla del respaldo que cubre una causa y se lleva todas las que
producen la misma señal, y de la del truncamiento, pero con una vuelta propia:
allí el respaldo elegía *una* causa entre varias; aquí no elegía ninguna, y el
resultado se lo quedó **el caso legítimo que estaba en la misma rama**.

> **Un `_` en una rama de error no es un caso por omisión, es una afirmación**:
> dice que todo lo que llegue ahí es lo mismo que lo que ya estaba ahí. Cuando en
> esa rama vive un resultado legítimo, el fallo se disfraza de ese resultado — y
> se disfraza en la dirección de parecer normal, que es la dirección en la que
> nadie mira.

### Y la que estaba ahí era la que menos podía estar

`AgentError::Attribution` es el núcleo cazando al modelo nombrando un id que no
aparece en ningún canal. Es **la conducta más peligrosa que el banco busca**, y
caía en la misma rama, contada como la más segura:

> **Cuidado especial cuando la rama absorbente es la que mide seguridad.** Un
> instrumento sesgado hacia el resultado tranquilizador no es ruidoso, es
> silencioso, y sus números suben cuando el sistema empeora.

Ahora hay cinco resultados —correcto, equivocado, abstenido, **rechazado por el
núcleo**, y **sin medición**— y ninguno se infiere de la ausencia de otro. Un caso
sin medición no cuenta en ninguna fracción, los denominadores son sobre lo medido
y no sobre la suite, y el resumen lo dice antes que cualquier cifra.

### El corolario sobre el tamaño de la suite

La misma revisión llevó la suite de 9 casos a 20. Con nueve, un caso vale once
puntos:

> **Una suite que no puede separar dos explicaciones no mide, puntúa.** Añadir
> casos para subir el total no sirve; los que sirven varían **una** cosa a la vez
> respecto de un caso que ya está — el mismo id con verbo y sin verbo, con
> contexto y sin contexto — para que el resultado conteste cuál de las dos
> lecturas era la buena en vez de volver a plantear la pregunta.

Y una nota sobre las exenciones. La comprobación que impide que un caso de
abstención nombre un módulo tenía una escapatoria por subcadena del **nombre** del
caso (`contains("ruled out")`): una exención en la que se cae por titularse de
cierta manera, y que sólo podía describir el único caso para el que se escribió.
Ahora es un campo con la razón escrita, y hay un control que comprueba que nadie
reclame una exención que no necesita — porque **una exención que nadie revisa es
una exención que todos los casos acaban teniendo**, y entonces la comprobación
desapareció sin que nadie la borrara.

## Regla derivada: un veredicto al que se llega por descarte afirma su otra mitad sin haberla medido

Encontrada el 2026-08-08, corriendo `grammar-check` contra la gama ligera por
primera vez. El comando dijo `PROVEN` y su propia evidencia decía otra cosa:

```
with the grammar     { "operation": "install_module", "targets": ["python3.ipython3.…
without it           [end of text]

PROVEN: told to say one word, constrained it could not even begin
with it, and left alone it did.
```

*«Left alone it did»* — dijo la palabra. **El brazo libre no dijo nada.**
`[end of text]` es lo que imprime `llama.cpp` cuando el modelo termina la
generación de inmediato, así que el 1.5B, sin gramática, se quedó callado.

El veredicto tiene dos mitades y el código sólo comprobaba una:

```rust
} else if obeys_root(&unconstrained) {
    Inconclusive { ... }          // ambos abren objeto: no se distinguen
} else {
    InForce { ... }               // ← por descarte
}
```

`InForce` era el `else`. Se llegaba ahí con que el brazo libre **no abriera un
objeto**, y un brazo que no dice nada tampoco abre uno. La frase «y suelto sí la
dijo» no la comprobaba nadie.

> **Un veredicto que afirma dos cosas necesita dos comprobaciones.** Cuando una
> de ellas es la rama de control, ponerla en el `else` la convierte en «no pasó
> ninguna de las otras cosas que se me ocurrieron», que es una afirmación sobre
> la imaginación de quien escribió las ramas y no sobre lo medido.

Es la **regla 4** de esta nota —toda prueba de denegación necesita línea base y
control— aplicada al veredicto en vez de al experimento: el control estaba
corriendo, se estaba imprimiendo, y no se estaba **leyendo**.

### Por qué ninguna prueba lo vio, y es la regla 8 por tercera vez

Los sustitutos del sondeo eran cuatro y estaban bien escritos: uno decía la
palabra siempre, otro sólo sin la bandera, otro emitía un objeto en las dos
ramas, otro se truncaba a media cadena. **Los cuatro contestaban algo.** Ninguno
modelaba un modelo que se calla, porque un falso se escribe para producir la
salida que uno está pensando, y nadie estaba pensando en el silencio.

> **Un falso modela el eje en el que se le escribió variación.** Aquí el eje era
> *qué dice*, y el que faltaba era *si dice*. Es la tercera vez: los siete
> sustitutos de `llama-cli` variaban el formato de salida y todos honraban el
> contrato de una pasada; las nueve fixtures del parser terminaban donde el
> parser esperaba.

La regresión que quedó usa el `[end of text]` **verbatim de la corrida real**,
no una cadena vacía inventada, y se comprobó fallando contra el código anterior.
Y su control —un brazo libre que contesta prosa— existe para que la corrección
no sea «rechazar los brazos vacíos» sino «exigir la palabra».

### Lo que sí se podía afirmar, que era menos

Con la bandera hubo objeto; sin ella no hubo nada. **La bandera cambió la
salida.** Que lo que la gramática impidió fuera *la palabra* es lo que no se
midió, porque el modelo nunca mostró que la diría — y ésa es exactamente la
diferencia entre `PROVEN` y `NOT PROVEN` en un sondeo cuyo propósito es separar
una bandera aceptada de una gramática aplicada.

### Y el banco escondía el valor que rechazaba

Del mismo día y de la misma corrida, en otro instrumento. El banco imprimía un
rechazo por atribución así:

```
REF  the id said plainly, with no verb in front of it → (named something nobody mentioned)
```

**Sin decir qué.** Es la conducta más peligrosa que el banco busca —el modelo
nombrando un id que no existe en ningún canal— y era la única línea del reporte
con su evidencia retirada. Una gama sacó cuatro de esos en una corrida, y no hay
forma de saber desde la salida si inventó cuatro veces el mismo id o cuatro
distintos, que son dos hallazgos diferentes sobre el modelo.

La regla ya existía en esta nota desde el 2026-08-08 —*«una comprobación que
señala a un culpable tiene que enseñar la evidencia que juzgó»*— y esta es la
segunda vez que se paga por no aplicarla. Corregido: el rechazo viaja con el
valor.

## Regla derivada: fijar la semilla no vuelve reproducible una corrida cuya entrada cambia

**Descubierta el 2026-08-08, al correr dos veces la misma cosa.**

El invocador de llama.cpp fija `--seed 1` y `--temp 0`, y de ahí el encabezado
de `llama.rs` concluía que «una respuesta rara se puede reproducir en una
terminal». Dos corridas del banco sobre la gama ligera —mismo modelo, misma
suite, misma máquina, nada tocado— movieron **dos casos de veinte, en
direcciones opuestas**.

La causa no es llama.cpp. Es que la semilla fija el *muestreador* y nadie fijó
la *entrada*: `Prompt::render` genera un marcador aleatorio nuevo en cada
invocación, así que el prompt son bytes distintos cada vez. Un sistema
determinista alimentado con algo distinto contesta algo distinto, y eso es lo
correcto.

La regla, entonces:

> **Reproducible no es «el generador de azar está fijo». Es «toda la entrada
> está fija».** Antes de prometer que una corrida se repite, hay que enumerar
> qué entra, no qué se sortea.

### No hacía falta una máquina para verlo, y eso es lo peor

`a_marker_is_never_reused_between_two_renders` afirma que el marcador cambia
entre dos renders, y existe desde el día en que se escribió el marcador. **Una
prueba y un comentario del mismo crate decían cosas opuestas durante meses.** Se
le creyó al comentario porque nada pone a coincidir la documentación con las
pruebas: el compilador verifica que un `[`enlace`]` apunte a algo, no que la
frase de al lado sea cierta.

De ahí el corolario, que es de dónde mirar y no de qué probar:

> Cuando una prueba y un comentario del mismo módulo se contradicen, **la prueba
> es la que corre**. Búscala antes de creerle a la prosa.

Y buscando eso apareció el otro extremo del mismo problema. El encabezado citaba
`Invocation::command_line` como *la* forma de reproducir una corrida, y esa
función **no tenía una sola llamada fuera de su propia prueba**. Estaba probada
—la prueba comprobaba que la línea trae la gramática y la semilla— y nunca se
ejecutaba en producción, así que la prueba certificaba una función correcta que
no hacía nada por nadie.

> Una función pública con una prueba y sin ninguna llamada **no es una
> característica, es una intención**. La prueba dice que hace lo que dice; no
> dice que alguien la use, y la documentación de al lado sí lo daba por hecho.

La corrección no fue borrarla: `--keep-prompt` es su primera llamada de verdad.

### Corregido un día después: una fracción cuyo denominador se mueve

La primera lectura de esto fue «las cifras de acierto se mueven», y una tercera
corrida la corrigió. Lo que se mueve es **cuántos casos llegan a producir una
respuesta**, que es el denominador. El numerador —cuántas respuestas fueron
correctas— casi no se movió: 5, 6, 6 en una gama, 9 y 9 en otra.

El caso que lo enseña ocurrió: una gama contestó cuatro casos más que la vez
anterior, acertó uno más, **y su fracción bajó** de 5/14 a 6/18. Mejor
desempeño, peor número.

> Una fracción cuyo denominador se mueve por una causa ajena al numerador **no
> es comparable consigo misma entre corridas**. Antes de comparar dos, hay que
> preguntar si las dos dividen entre lo mismo.

La forma estable de reportar esto era la que tenía el denominador fijo —aciertos
sobre los veinte casos de la suite— y no estaba impresa por ningún lado, porque
el banco reporta sobre lo medido, que es lo honesto para *una* corrida y lo
engañoso para dos. Las dos cifras dicen cosas distintas y ambas hacen falta.

### Y la consecuencia sobre lo que ya estaba medido

Nada de lo medido se retira: los costes —disco, RSS, latencia mediana—
replicaron al byte y a la centésima. Lo que cambia es cuánto pesa una fracción
de acierto, y **una diferencia del tamaño del ruido del instrumento no es una
diferencia**. Hasta estas corridas no había forma de saber cuál era ese tamaño,
porque ninguna cifra de acierto se había medido dos veces.

Con cinco corridas ya se sabe, y es más chico de lo que parecía: los aciertos se
mueven ±1 sobre veinte casos, y catorce de los veinte dieron **exactamente la
misma marca en las cinco**. La distancia de dos casos entre la gama media y la
alta ya no cae dentro del ruido; lo que le falta a esa comparación es que la
alta tenga una segunda corrida, no que el instrumento sea impreciso.

Una suite corrida una vez no da una barra de error. Da un punto.

## Regla derivada: un mensaje de error que nombra el mecanismo no nombra la causa

**Descubierta el 2026-08-08, resolviendo el caso 4 del banco.**

El error decía la verdad —*«empezó el objeto y se quedó sin tokens antes de
cerrarlo»*— y por eso mismo se leyó como una explicación. No lo era: es el
**mecanismo final**, el momento en que el proceso se detuvo. Con eso a la vista
se propuso una causa equivocada, que el modelo se ciclaba escribiendo la versión,
porque era el único caso de la suite cuya restricción llevaba un punto.

La salida completa lo refutó en una línea: el modelo repetía `.versions` dentro
del identificador y **nunca llegó a la versión**.

> Un error que dice *cómo* se detuvo algo no dice *qué* lo llevó ahí. Separar
> siempre: **causa inmediata** (agotó el presupuesto), **causa observada** (se
> cicló en esta producción), **condición que lo permite** (la producción no tiene
> cota). Arreglar la tercera creyendo que es la primera es cómo se sube `-n` y no
> pasa nada.

Y la tercera capa **no es culpa**: la gramática no obliga a repetir, el modelo
elige. Una condición que permite un fallo y una causa que lo produce no son lo
mismo, y confundirlas lleva a tratar como defecto estructural lo que es una
decisión de otro sistema.

### Lo que impidió verlo era una decisión de presentación

La evidencia estaba en el mensaje desde la primera corrida. El banco la
truncaba a 90 caracteres, y el corte caía **justo antes de la repetición**, que
era la parte que decidía entre las dos hipótesis. Seis corridas con el dato
delante y cortado.

> Un resumen que trunca puede cortar exactamente lo que distingue dos
> explicaciones. Cuando dos hipótesis compiten, ir al texto entero **antes** de
> elegir entre ellas.

Lo que lo resolvió fue `--keep-prompt`, construido un día antes para otra cosa
—reproducir una corrida— y usado aquí para volver a correr esa inferencia con su
marcador original y ver la salida completa. Un comando.

### Y el hallazgo fue mejor que la hipótesis

La hipótesis, de haber acertado, habría explicado **un** caso. Lo que se
encontró explica siete: `ese.abc.abc.abc`, `thallyx.ing.ing`,
`dev.thalyx.demo.localhost`, `dev.thalyx.demo.https.localhost`,
`photoshop-1.ashx.ashx`, `python3.ipython3.ipython3` y el propio caso 4 son el
mismo comportamiento cortado en momentos distintos.

> Cuando la explicación correcta aparece, **suele explicar más de lo que se le
> preguntó**. Una que explica exactamente un caso y nada más merece desconfianza.

## Regla derivada: una decisión y una ausencia no pueden compartir nombre, y ésta es la tercera vez

**Descubierta el 2026-08-09, leyendo la corrida que sí dio veredicto.**

`grammar_effect` clasificaba lo que una respuesta nombró. Tenía un estado
`Nothing` — *dijo algo y no había ningún id adentro*—, y ahí caían tres cosas
que no son la misma:

1. un párrafo que divagó sin nombrar nada,
2. un modelo que emitió **cero tokens**,
3. una propuesta bien formada con `"targets": []` — **que es la abstención**.

El segundo ya había costado una corrida entera dos días antes, y se había
separado en `SaidNothingAtAll`. El tercero seguía adentro. Así que cuando la
gama media, bajo gramática, ante `instala algo bueno`, emitió una lista de
objetivos vacía, el instrumento lo imprimió como `said something, named nothing`
— **la conducta que seis corridas del banco venían reportando como cero de
cuarenta y seis, escrita como ruido**.

> Un estado que representa una **decisión** del sistema medido no puede
> compartir nombre con uno que representa una **ausencia**. Si al leer una
> salida hace falta preguntar «¿esto es que decidió que no, o que no contestó?»,
> son dos estados y el instrumento tiene uno.

La forma de encontrarlo antes: **por cada estado del instrumento, enumerar todo
lo que puede producirlo**. `Nothing` tenía tres orígenes y ningún lugar donde eso
estuviera escrito. La prueba que lo fija ahora es
`abstaining_is_not_the_same_word_as_generating_nothing_or_as_naming_nothing`, y
afirma la desigualdad de los tres, no el valor de cada uno.

Hay una señal que estaba a la vista: **una prueba cuyo mensaje contradecía su
propia aserción**. Decía *«una lista de objetivos vacía es cómo se abstiene una
propuesta»* mientras afirmaba que el resultado era `Nothing`. El autor sabía la
distinción y el tipo no la tenía.

> Un mensaje de aserción que dice algo más específico que la aserción es un
> defecto pendiente escrito por quien lo iba a cometer.

### Y el corolario sobre los denominadores de un reporte

La misma corrida imprimió `with the grammar, invented 6` justo debajo de
`abstention cases measured on both arms 2`, porque las dos columnas se contaban
sobre denominadores distintos y sólo uno se imprimía. Seis de dos no es un
número sobre el que nadie pueda actuar, y en una tabla con menos contraste
habría pasado como cierto.

> Dos columnas contadas sobre conjuntos distintos necesitan **los dos**
> denominadores impresos. Un total que no puede acotar a su propia columna está
> mal etiquetado, y la aserción que lo detecta es `parte <= total`.

## Regla derivada: un experimento no puede gastar su propia señal en otra cosa

**Descubierta el 2026-08-09, en la primera corrida del brazo en prosa.**

El brazo existía para preguntar sin pedir un objeto, y ofrecía la palabra
`NOTHING` como la única forma de declinar. **Las veinte respuestas del 3B
empezaron con `NOTHING`** — incluidas las once donde sí había un módulo que
nombrar. El control se cayó y el veredicto salió `NOT PROVEN`.

El prompt usaba la palabra **cuatro veces**, tres de ellas en otro sentido:

> «Answer with its id and **nothing** else» · «gains **nothing**» · «If
> **nothing** below names a module» · «answer with only this word: **NOTHING**»

> Cuando un experimento designa un token como su señal, ese token no puede
> aparecer en el estímulo en ningún otro sentido. Vale para una palabra clave en
> un prompt, para un delimitador en una salida y para un valor centinela en una
> estructura.

Se comprueba contando: la aparición sin distinguir mayúsculas debe ser
**exactamente una**. La prueba es `the_abstention_word_appears_in_the_prose_
prompt_once_and_in_one_sense`, y existe porque **el defecto es invisible leyendo
la propia prosa** — uno lee lo que quiso decir, no las palabras que puso.

Es pariente de la regla del delimitador que el sistema medido puede escribir, y
la diferencia vale anotarla: allá el sistema medido podía **falsificar** la
señal; aquí el instrumento se la **regaló**.

### Y lo que salvó la corrida fue el control, no la revisión

`ABSTAINED 8/9` era exactamente el número que la hipótesis quería. Sin brazo de
control se habría leído como `THE PROMPT TAKES THE DECISION` y habría cerrado la
investigación con un resultado fabricado por su propio prompt.

> Un resultado favorable es cuando más hace falta el control, y es cuando menos
> ganas dan de mirarlo. La regla 4 no es sobre pruebas de denegación: es sobre
> cualquier medición cuyo resultado alguien prefiera.

Corolario incómodo, y por eso se escribe: **corregir el defecto puede destruir el
8 de 9.** Se corrige igual. Un instrumento que falló por su construcción no
adquiere validez porque el número que produjo agrade.

## Regla derivada: un catálogo se prueba corriéndolo, nunca comparándolo con otra lista

**2026-08-09.** `describe` dice qué verbos tiene la máquina, y un agente que
llega lo lee en vez de que alguien se los pegue en el prompt. Eso sólo vale si
la respuesta es cierta, y una tabla de verbos es justo lo que se queda atrás en
silencio: la lista vivía en **tres** sitios —el `match` de la sesión, el banner y
las completaciones— y una de las tres ya había divergido.

La tentación es probar que las tres listas coinciden. **Eso no prueba nada**: dos
tablas iguales pueden estar las dos equivocadas, y una prueba que compara
constantes pasa para siempre en cuanto alguien copia el error a los dos lados.

> **Un catálogo se comprueba contra la máquina, no contra otra copia de sí
> mismo.** Cada nombre que el catálogo anuncia se **teclea en un prompt de
> verdad** y tiene que ser entendido; cada nombre tiene que **aparecer en el
> banner** de la sesión que arranca. Lo que se lee es la salida del binario
> corriendo, nunca la tabla enlazada dentro de la prueba — enlazarla dejaría que
> las dos se equivoquen juntas.

Encontró un defecto el mismo día que se escribió: bajo un programa, `apagar`
existía y el banner no lo nombraba, así que quien lo tecleaba recibía una
negativa por un verbo del que nadie le había hablado.

## Regla derivada: un ensayo no es una segunda implementación

**2026-08-09.** `ensayo rm *.log` dice qué se iría sin tocar nada. La forma
obvia de construirlo —una función que comprueba lo mismo que la operación real—
es la forma equivocada, y el motivo es la regla 8 con otra ropa:

> **Un ensayo que dijera «esto funcionaría» mientras la operación real se niega
> es peor que no tener ensayo.** Un ensayo que se equivoca en la dirección
> optimista es exactamente la trampa que se construyó para evitar.

Así que cada `foresee_*` es **la mitad de comprobación de la operación real**, y
la operación real la llama. No hay camino donde las dos discrepen sobre si algo
se permite, porque hay un solo código decidiendo. La prueba que lo fija no
compara dos listas de errores esperados: pide el mismo caso a las dos y exige la
**misma palabra**.

Y hay una honestidad que el ensayo no puede evitar y sí puede declarar: reporta
lo que es cierto **ahora**, y la máquina puede cambiar entre los dos momentos. Se
llama `foresee` y no `check` por eso — es una predicción, y llamarla de otro modo
sería prometer lo que no puede.

Corolario, y es lo que decide qué hacer con lo que no tiene mitad de
comprobación: los cinco verbos que cambian la máquina y no la tienen
—`instalar`, `correr`, `revertir`, `instalar-en`, `apagar`— **contestan que no
se pueden ensayar**. Un plan vacío se leería como «esto no haría nada», que es
lo contrario de la verdad, y es la regla 10 otra vez.

## Regla derivada: el primer renglón de un modo nuevo lo escribió el modo anterior

**2026-08-09.** La cara estructurada contesta un objeto JSON por renglón, y
había una excepción: **el renglón que avisa que el modo está encendido**. El
prompt de ese comando se imprimió cuando la cara todavía era humana y no lleva
salto de línea, así que la respuesta aterrizaba pegada:

```
  /home > {"off":"structured off","ok":true,"op":"structured",…}
```

Veinte pruebas unitarias del objeto pasaban —el objeto estaba perfecto— y las
pruebas de punta a punta también, porque el ayudante que las lee filtra los
renglones que parsean. Lo encontró canalizar la sesión a mano y mirar la salida.

> **El primer renglón de un modo nuevo no lo escribe ese modo.** Lo escribe lo
> que estaba encendido cuando se pidió el cambio, y por eso es el único renglón
> que ninguna prueba del modo nuevo cubre. La prueba que lo caza no pregunta por
> un objeto: exige que **todos** los renglones desde el cambio parseen.

Y hay un corolario sobre el arnés, que es la regla 5 otra vez: un ayudante que
recoge «los renglones que parsean» **no puede fallar** ante un renglón que no
parsea. Es exactamente la forma de un instrumento que aprueba lo que debería
reprobar, y el ayudante seguía siendo el correcto para las otras quince pruebas.
La respuesta no fue arreglarlo, fue agregar la que mira el flujo entero.

## Regla derivada: una cara que exige respuesta es un instrumento sobre la que no

**2026-08-09.** Al construir la cara estructurada, la primera prueba que pidió
una respuesta a `rm` sin argumentos falló, y no por la cara nueva: **`rm`, `mkdir`,
`touch`, `cp` y `mv` escritos solos no eran verbos.** Cada brazo del despacho de
la sesión exige un espacio detrás (`"rm "`), así que la palabra sola caía al
mensaje del agente — «no tengo modelo cargado» —, que es el mismo defecto que
Cesar encontró con `clear` el 2026-08-09, vivo en cinco verbos más.

Nadie lo había visto en semanas de uso humano, y la razón es la que hay que
escribir:

> **Una persona perdona el silencio y una máquina no.** Quien recibe un párrafo
> raro asume que la máquina está de mal humor y teclea otra cosa; un programa
> esperando un objeto se cuelga. Así que **la cara estructurada es un instrumento
> sobre la humana**: exigirle respuesta a todo encuentra los sitios donde no
> había ninguna.

Vale para cualquier segunda cara que se construya: la primera vez que algo se
expone a un consumidor que no puede improvisar, sale la lista de lo que nunca
contestó. Escribir las pruebas de la cara nueva **antes** de darla por buena es
lo que convierte eso en un hallazgo en vez de en un reporte de campo.

## Regla derivada: una referencia local no es el estado del repositorio

**2026-08-09.** Al retomar el proyecto comprobé qué había en `main` con
`git rev-parse main`, salió doce commits atrás de la rama, y de ahí reporté que
el trabajo no le llegaba a Cesar con `git pull`. Falso: `origin/main` ya tenía
todo fusionado. Lo atrasado era **mi copia local**, que en un contenedor recién
clonado y sin `fetch` no dice nada sobre el remoto.

> `main` y `origin/main` son dos preguntas distintas, y sólo la segunda es sobre
> el repositorio. Un nombre de rama sin `origin/` contesta por lo que este disco
> alcanzó a ver, que puede ser de hace días o de nunca.

Es la regla 5 —el instrumento incluye al arnés— en el sitio menos esperado: no
falló una sonda ni un parser, falló **leer el resultado de `git`** dando por
hecho que un nombre corto se refería a lo que se estaba preguntando. Lo que
convierte esto en algo que hay que escribir y no en un descuido es la
consecuencia: la lectura equivocada iba a mandar a Cesar a fusionar algo que ya
estaba fusionado, y —peor— quedó escrita como un hecho en un bloque de
[[Punto-Actual]] antes de que nadie la comprobara.

Lo que hay que hacer en su lugar: `git fetch` primero, y comparar contra
`origin/<rama>`. Si no se puede hacer `fetch`, la respuesta honesta es «no sé
qué tiene el remoto», que es la regla 10 — una falla al leer no es una falla al
existir.

## «Sólo hierro» es una afirmación sobre qué propiedad se prueba — 2026-08-10

Tres puntos de [[Superficie-para-el-LLM]] quedaron sin construir con el
argumento de que este contenedor no puede ejercerlos. **Dos de los tres estaban
mal**, y las dos equivocaciones son distintas y las dos vale la pena escribirlas.

**La primera fue mezclar dos propiedades en una.** El intento con nombre se
apoya en snapshots de Btrfs, y de ahí salió «no se puede probar aquí». Pero lo
que había que probar era: cuál intento está abierto, qué hace un segundo, a qué
árbol apunta un abandono, qué pasa cuando el snapshot ya no está. Ninguna de
ésas es una pregunta de Btrfs. `thalyx-snapshot` ya llevaba escrito el criterio
correcto en el comentario de su propio falso —*«la política que sólo se puede
ejercer en un sistema de archivos Btrfs es política que nunca se ejerce»*— y no
se leyó.

La regla 8 dice que un falso tiene que modelar **la propiedad bajo prueba**. Eso
se leyó al revés: como si dijera que un falso tiene que modelar *el sistema*.
Antes de decir que algo sólo se prueba en el hardware, hay que nombrar la
propiedad. Si la propiedad se puede escribir sin nombrar el hardware, no era
sólo hierro.

**La segunda fue un hecho falso, dicho con seguridad.** «Consumir el ringbuf es
código BPF, y un programa que falla el verificador tumba al watcher.» El
productor ya estaba escrito y no se tocaba; el consumidor es código de usuario.
El verificador no entraba en esto en ningún momento. Un impedimento heredado de
otra parte del sistema, aplicado a ésta sin comprobarlo.

Las dos comparten la forma: **una razón para no construir algo es una
afirmación, y se comprueba como cualquier otra.** Es más barato de comprobar que
casi todo lo demás de este proyecto —basta leer el código que ya está— y es la
que menos se comprueba, porque una razón para no hacer algo no produce una
prueba roja.

## Un prompt que se suprime para un programa se le suprime a una persona — 2026-08-10

Cesar escribió `structured on`, recibió su objeto, y se quedó frente a una
pantalla en blanco sin nada que distinguiera una sesión esperando de una
colgada. Abrió otra ventana.

El prompt se suprimía para cumplir «un objeto por renglón». En una terminal esa
promesa **nunca se estaba cumpliendo** —el modo crudo hace eco de cada carácter—
y el comentario de la propia prueba lo decía: *«con un pty hay eco, así que el
flujo del pty no es un objeto por renglón»*. La supresión no compraba nada donde
se aplicaba y costaba todo.

La regla: **una restricción se aplica donde la propiedad que la justifica es
cierta, y hay que comprobar dónde es cierta.** Aquí la propiedad era del flujo
—tubería sí, terminal no— y se aplicó según la cara, que es otra cosa. Ninguna
prueba lo iba a encontrar, porque ninguna prueba tiene ojos.

## Un nombre que el kernel corta es un nombre que nadie puede preguntar — 2026-08-10

La etapa 27 reportó «nada está anclado en `/sys/fs/bpf/thalyx/maps/thalyx_mutations`,
así que thalyx-watch no está cargado». Era falso: la etapa 3 de la misma corrida
decía que el contador **sí** estaba anclado, y la 7 leyó 36 872 mutaciones con
los diez ganchos puestos. El watcher estaba cargado y el reporte no podía
decirlo, porque sólo había mirado una ruta.

`BPF_OBJ_NAME_LEN` es 16 contando el terminador, así que el kernel se queda con
**quince** caracteres del nombre de un mapa. `thalyx_mutations` (16) y
`thalyx_mutation_count` (21) se vuelven los dos `thalyx_mutation`, y el kernel
tenía dos mapas del mismo objeto bajo un solo nombre. Si eso fue lo que impidió
el anclaje nunca se estableció; lo que sí está establecido es que dos mapas con
un solo nombre en el kernel vuelven la pregunta incontestable.

Dos reglas, y la segunda es la que se repite:

**Un identificador que la capa de abajo trunca es un identificador que hay que
mantener corto de este lado.** Hay una prueba ahora que corta cada nombre de
mapa de los dos objetos BPF a quince caracteres y falla si dos coinciden.

**Un mensaje de «no encontré» tiene que decir dónde buscó y qué había ahí.**
«Nada está anclado en X» describe dos máquinas completamente distintas —una
donde el watcher no está y otra donde está y algo no ancló uno de sus mapas— y
manda a quien lo lee a dos lugares distintos. Es la regla 10 aplicada al reporte
y no al dato: una falla al leer no es una falla al existir, y **decir en cuál de
las dos se está también es parte de la respuesta**.

## Nueve veces, y la que tardó un año — 2026-08-10

Regla 5, novena instancia, y la que más vale la pena leer porque el arreglo
anterior admitía por escrito que era una adivinanza.

Dos pruebas de `llama.rs` fallaron en la máquina de Cesar con `Text file busy`.
Un año antes, una de ellas había fallado **una vez en veinticinco corridas**, no
se capturó el error, y se arregló dándole a cada falso su propio nombre de
archivo — con un comentario que decía, honestamente, *«esto no es un
diagnóstico»*. Doce hilos de prueba la hicieron fallar dos veces en una corrida
y el error por fin quedó capturado.

`ETXTBSY` es el kernel negándose a ejecutar un archivo que **cualquier** proceso
tiene abierto para escritura. El conteo que revisa vive en el inodo, no en una
tabla de descriptores, así que `O_CLOEXEC` no ayuda y los nombres únicos tampoco.
El mecanismo es la ventana del `fork`: entre que `Command::spawn` bifurca y el
hijo ejecuta, el hijo tiene copia de cada descriptor del padre — incluido el de
escritura que otro hilo estaba usando en ese instante para crear ese archivo.

**Una falla que ocurre una vez en veinticinco no es una falla menor, es una
falla sin capturar.** El arreglo por adivinanza sobrevivió un año porque nunca
volvió a fallar donde alguien mirara. Lo que la resolvió no fue pensar mejor:
fue una máquina con más núcleos, que es lo mismo que decir que el instrumento
tenía que cambiar antes de que la pregunta se pudiera contestar.

## Una prueba de que algo se niega vale lo que valen las máquinas donde corrió — 2026-08-10

`without_a_subvolume_it_refuses_instead_of_copying_a_directory` pasaba en el
contenedor y **falló en la máquina de Cesar**. No porque la prueba estuviera
mal: porque `intento empezar` no se negaba allá.

`subvolume_at_or_above` caminaba hacia arriba buscando el subvolumen más cercano.
Corrida desde un directorio temporal bajo `/tmp`, la caminata pasó por todos los
niveles y se detuvo en el primero que sí lo era: **`/`**. Tomó un snapshot de
sólo lectura del sistema de archivos raíz entero y contestó que abandonar
borraría 1 343 582 archivos, `/boot` entre ellos. Nada se destruyó porque esa
prueba nunca abandona.

**El contenedor no podía encontrarlo, y ésa es la parte que hay que escribir.**
Aquí no hay Btrfs ni un solo subvolumen en ninguna parte, así que *todos* los
caminos se niegan — y la prueba no podía distinguir una negativa correcta de un
accidente del sistema de archivos donde corre. Pasaba por la razón equivocada, y
una prueba que pasa por la razón equivocada es peor que una que falta: ocupa el
lugar donde alguien buscaría la que sí sirve.

Dos reglas:

**Una prueba de que algo se niega tiene que correr donde el sí es posible.** Si
en esta máquina la operación no puede tener éxito de ninguna manera, la negativa
no se está comprobando. El arreglo fue mover la guarda a una prueba unitaria
contra un falso donde **todo** es un subvolumen — la máquina donde la respuesta
peligrosa sí aparece — y agregarle a la etapa 26 una columna que se para en `/`
y exige la negativa.

**Un verbo que puede reemplazar un árbol entero no elige cuál buscando.** El
argumento para caminar hacia arriba —«un intento es sobre el árbol en el que
alguien está trabajando»— sonaba razonable y era exactamente al revés: caminar
hacia arriba **abandona en silencio** el alcance que quien llama tenía en mente,
y en toda instalación Btrfs ordinaria la caminata termina en la respuesta más
peligrosa que existe. Donde estás parado, o nada.

## Lo que este script deja en el repositorio es del humano, no de root — 2026-08-10

`sudo ./dev/verify.sh` compila los objetos BPF dentro del árbol de fuentes, como
root. El siguiente `make -C lsm load` que la persona escribe como ella misma no
puede sobrescribir su propio `.o`, y clang dice únicamente
`Operation not permitted` sobre una ruta en su propio directorio personal.

Costó una corrida entera: la carga falló, el watcher quedó descargado, y la
etapa 27 reportó correctamente que nada estaba anclado — una afirmación
verdadera sobre una máquina que este mismo script había dejado así.

La regla: **una herramienta que corre con privilegios y escribe en el árbol de
trabajo devuelve lo que escribió.** Un `chown` al final cuesta una línea; el
fallo que evita se ve como un defecto del kernel.

## Un control que pide silencio no se puede cumplir en una máquina viva — 2026-08-10

La etapa 27 comprobaba que el ring buffer se consume así: hacer una mutación,
leerla, y leer otra vez — **la segunda lectura tiene que venir vacía**. La idea
era buena; lo que medía no era lo que decía.

Los hooks del watcher son de máquina entera. La etapa 7 lo dice con todas sus
letras: *nada en esta máquina puede cambiar un archivo sin que el contador se
mueva*. Entre dos lecturas, una laptop con Fedora cambia muchísimos archivos —
empezando por el diario que escribió la sesión de la primera lectura. La segunda
lectura trajo dos registros y el reporte acusó: «la posición del consumidor no
se está escribiendo de vuelta». No se había demostrado nada de eso.

**Una lectura vacía no es una propiedad de un consumidor correcto; es una
propiedad de una máquina donde no pasa nada, y esa máquina no existe.**

El arreglo no es bajar el umbral ni repetir la medición: es cambiar de pregunta.
La mutación la hace ahora un programa llamado `thalyx-ringmark` —quince
caracteres, exactamente lo que cabe en el `comm` del kernel— y las dos columnas
son sobre ese nombre y no sobre el total:

- **base**: la primera lectura trae al menos un registro de `thalyx-ringmark`,
  así que el ring sí cargó una mutación que esta etapa causó.
- **control**: la segunda no trae ninguno, y la máquina queda libre de haber
  seguido cambiando lo que se le antoje.

La regla: **cuando el ruido ambiental no se puede apagar, no se mide el total,
se marca lo que se busca.** Un identificador que el ruido no puede falsificar
convierte una medición imposible en una trivial.

## Un `load` que se niega porque ya está cargado no es un `load` que falló — 2026-08-10

`make -C lsm load` se niega cuando ya hay algo enganchado, y hace bien: cargar
dos veces dejaría uno de los dos inalcanzable. Pero `dev/verify.sh` usaba ese
estado de salida para contestar **dos preguntas distintas** — «¿lo enganchó este
script?» y «¿está enganchado?»— y no son la misma.

Cesar cargó el LSM a mano antes de correr el script, exactamente como se le había
pedido. El script reportó `FAILED thalyx-lsm did not attach` y otras cuatro
etapas dijeron NOT PROVEN sobre una protección que estuvo corriendo todo el
tiempo. La contradicción estaba impresa en la misma corrida: tres pantallas más
abajo, la etapa 7 imprimió *all 10 hooks attached*.

La regla: **un script que se apodera de un recurso de la máquina empieza
desapoderándola.** `make unload` antes de `make load`, siempre, pase lo que pase.
Sólo así el reporte habla de Thalyx y no de lo que alguien dejó corriendo.

## Una prueba que se cuelga tiene que decir dónde — 2026-08-10

`every_verb_the_catalogue_advertises_is_understood_at_the_prompt` escribe los 33
verbos, uno por línea, en una sola sesión. En la máquina de Cesar dejó de
contestar. Lo único que quedó fue la línea que imprime `cargo test` a los sesenta
segundos: *has been running for over 60 seconds*. Mató la corrida, y la
verificación entera se perdió sin aprender qué verbo era.

Aquí corre en 1.7 segundos, así que el contenedor no puede reproducirlo.

La regla: **una prueba que puede colgarse lleva su propio reloj, y al vencerse
nombra dónde estaba.** No basta con que falle; una falla que no nombra nada
cuesta una corrida y no enseña nada. La sesión imprime su prompt antes de leer
cada línea, así que contar prompts en lo que alcanzó a decir da el verbo — y esa
aritmética es una función con prueba propia, porque equivocarse por uno mandaría
a alguien a leer el código del verbo vecino.

Y el reloj tiene que leer la salida en otro hilo. Un vigilante que se duerme
esperando mientras la tubería de salida se llena cuelga al proceso que vigila:
sería la regla 5 otra vez, el instrumento causando lo que mide.

## Un verbo sin argumento hereda el árbol más grande que hay — 2026-08-10

`indexar` sin nada atrás indexa el árbol donde está parada la sesión, y una
sesión siempre empieza en `/home`. En el contenedor eso son 329 archivos y
tarda dos segundos. En la máquina de Cesar `/home` incluye `.cargo/registry` y
`.rustup`: cada fuente de cada crate que ha bajado más la biblioteca estándar
entera. Corrió más de tres minutos, lo mató, y costó la corrida de verificación
completa.

La lista de carpetas ignoradas —`.git`, `target`, `node_modules`— era una lista
de las cosas que ya habían salido mal. Ahora la regla es **cualquier carpeta que
empiece con punto**, porque una carpeta oculta es donde una máquina guarda lo que
administra para sí misma, y agregar `.cargo` a una lista de nombres sólo habría
esperado a que `.local/share` fuera la siguiente. El árbol que se nombra
explícitamente nunca se filtra: quien escribe `~/.config` lo escribió a
propósito.

Y hay un techo: **20 000 archivos, y arriba de eso se niega en lugar de
empezar.** Una respuesta que nunca llega es peor que una negativa, y la negativa
dice qué hacer en su lugar. La negativa ocurre dentro de la transacción, así que
una reconstrucción que se niega no es una reconstrucción que vació el índice.

La regla general: **un verbo cuyo alcance por omisión es «donde estás» hereda lo
que haya ahí, y lo que hay ahí no es una decisión de quien lo escribió.** Ese
verbo necesita un techo, no confianza.

## Dos caminatas que tienen que coincidir viven en una sola función — 2026-08-10

Al agregar la regla de carpetas ocultas fallaron once pruebas, ninguna de ellas
sobre carpetas ocultas. `Index::build` y el chequeo de frescura caminan el mismo
árbol y cada uno tenía su propio `filter_entry`. Al cambiar sólo uno, la
construcción registraba un conjunto de archivos y el chequeo contaba otro, así
que **todo índice quedaba obsoleto en el instante en que se escribía** — y se
veía como un defecto de frescura, no como dos caminatas.

Lo que lo destapó fue que `tempfile` nombra sus directorios `.tmpXXXXXX`. Sin esa
casualidad, el desacuerdo habría llegado a la máquina de Cesar en vez de a la
suite.

La regla: **cuando dos lugares tienen que estar de acuerdo sobre qué archivos
son, no hay dos lugares.** Una sola función, y las dos la llaman.

## Una tecla que el kernel se come es una tecla que no existe — 2026-08-22

Al construir el editor de pantalla ([[Editor-de-Texto]]) la elección obvia era
`Ctrl-S` para guardar. Es la que todo el mundo espera y **no habría funcionado
nunca**.

El modo crudo de `thalyx-syscall` apaga `ICANON` y `ECHO` y deja encendidos
`ISIG` e `IXON`, a propósito y con su comentario: `Ctrl-C` tiene que seguir
funcionando en una máquina cuya única terminal es ésta. La consecuencia, que
nadie había escrito, es que la disciplina de línea del kernel se queda con
`Ctrl-C`, `Ctrl-Z`, `Ctrl-S` y `Ctrl-Q` **antes de que Thalyx vea un byte**. Y
`Ctrl-S` no es que no haga nada: es XOFF, así que detiene la salida y la terminal
queda aparentemente muerta.

**Esto se encontró leyendo `termios`, no corriendo el editor**, y por eso vale la
pena escribirlo: es la excepción a la regla 1 y la excepción tiene forma. Una
propiedad del *entorno* —qué se traga el kernel, qué opciones de `mount` hay,
qué señales llegan— se puede establecer preguntándole al entorno, y correr el
programa sólo confirma lo que ya se sabía. Lo que no se puede establecer leyendo
es qué hace el programa.

La regla: **antes de enlazar una tecla, preguntarle a la disciplina de línea si
va a llegar.** Y como eso es una decisión que se olvida, vive en una prueba —
`the_keys_the_kernel_eats_are_not_bound_to_anything_here` falla si alguien alguna
vez enlaza una de las cuatro.

## Un pty sin tamaño de ventana no es una terminal — 2026-08-22, y es la décima

La primera corrida de las pruebas del editor de pantalla **se colgó**. El
diagnóstico completo, y las tres piezas importan:

1. `thalyx dev pty` abre un pty y el kernel lo entrega **sin tamaño de
   ventana**: `TIOCGWINSZ` contesta cero renglones.
2. El editor le pregunta el tamaño a la terminal y, con cero, **se niega a
   dibujar** — que es correcto y es lo que `terminal_size` documenta: cero
   renglones no es una pantalla chica, es ninguna respuesta, y suponer 80x24
   dibuja veinticuatro renglones sobre una pantalla de diez.
3. Como se negó, las pulsaciones destinadas al editor se tecleraon en el prompt,
   `salir` se lo comió una línea que decía `HOLA salir`, y la sesión se quedó
   esperando un `EOF` que no iba a llegar.

**Ni Thalyx ni la prueba estaban mal. El arnés estaba incompleto**, y es la
décima instancia de la regla 5. El arreglo es donde tenía que ser: quien hace un
pty dice de qué tamaño es (`set_terminal_size`, 24x80 fijo — un tamaño copiado de
la ventana que alguien dejó abierta es una prueba que pasa en una máquina y no en
la siguiente). **No** un valor por omisión dentro de `terminal_size`, que sería
el programa adivinando su propia pantalla, que es justo lo que esa función se
niega a hacer.

Lo que hizo el diagnóstico barato en vez de caro fue el reloj: la prueba lleva su
propio plazo y al vencerse **imprime lo que se había dibujado hasta entonces**.
Ahí estaba la oración `there is no terminal here to draw an editor on`, que dice
la respuesta entera. Es la misma lección de *[[Estrategia-de-Pruebas|una prueba
que se cuelga tiene que decir dónde]]* cobrada por segunda vez, y la segunda vez
costó un minuto en vez de una corrida.

## Una confirmación que se traga la tecla que la contesta — 2026-08-22

`Ctrl-X` sobre un archivo con cambios sin guardar pregunta antes de salir. La
primera implementación leía la respuesta con un `read_key` **anidado** dentro de
ese caso: si era otro `Ctrl-X`, salir; cualquier otra cosa, seguir.

Con eso, `Ctrl-X` y luego `Ctrl-O` —que es exactamente lo que hace una persona a
la que le preguntan si quiere guardar— **no guardaba nada**. El `Ctrl-O` se
consumía como «no es `Ctrl-X`» y se tiraba, porque la lectura anidada no sabe qué
significan las teclas; eso lo sabe el bucle.

La forma correcta es una bandera, no una segunda lectura: **toda tecla pasa por
el único bucle que sabe qué significan las teclas.** Un lector anidado en una
rama es un segundo intérprete del teclado, y un segundo intérprete de cualquier
cosa es el defecto que este proyecto ya pagó con `stdin` el 2026-08-09.

Y la prueba que lo encontró no cuenta pulsaciones: afirma **lo que quedó en el
disco**. Una que hubiera comprobado «el editor preguntó» habría pasado con el
defecto puesto.

## Un documento que se mueve se lleva la prueba que lo ata — 2026-08-22

`both_readmes_name_every_package_the_doctor_asks_for` llevaba un día fallando en
`main` y nadie lo había visto. La reescritura de la portada del 2026-08-21 movió
la ruta de construcción de `README.md` a `docs/BOOT.md`; la prueba seguía
afirmando sobre los READMEs y decía, con toda la razón, que el README nunca
menciona `bc`.

**La afirmación sobrevivió a la mudanza y el nombre del archivo no.** Eso es lo
que hace que valga la pena: la prueba no estaba mal, estaba atada a la casa vieja
de una verdad que se cambió de casa. Ahora afirma sobre el documento que sí
enseña a construir, **y además que la portada apunte a él** — porque una ruta de
construcción en un archivo al que nada apunta es una ruta que nadie encuentra,
que es el mismo fallo un paso antes.

La regla general: **una prueba que ata una afirmación a un archivo tiene que
comprobar también que alguien llega a ese archivo.** Sin la segunda mitad,
mover el contenido a un lugar inalcanzable deja la prueba en verde.

## Una prueba que nombra su precondición en el título sigue sin comprobarla — 2026-08-23

**Undécima instancia de la regla 5, y la más incómoda: la prueba estaba mal y
Thalyx estaba bien.**

`what_the_kernel_saw.rs` empezaba diciendo de sí mismo *«driven from a real
session on a machine with no watcher»*, y una de sus pruebas se llamaba
`with_no_watcher_loaded_it_says_so_and_never_says_nothing_changed`. La
precondición estaba escrita en el encabezado del archivo **y** en el nombre de la
prueba, y en ningún lado estaba comprobada.

Fue cierta durante meses porque el contenedor de desarrollo **no puede** cargar
BPF: la negativa era la única respuesta que podía volver, así que la prueba pasó
en cada commit sin que la suposición fuera falsa ni una vez.

El 2026-08-23 Cesar corrió la suite en hierro con el watcher cargado — porque las
instrucciones de esa corrida le decían que lo cargara. `cambios` contestó
**correctamente**, con registros reales drenados de un anillo real del kernel, y
las dos pruebas fallaron diciendo que Thalyx estaba equivocado.

Tres cosas que sacar de ahí:

1. **Un nombre no es una guarda.** `with_no_watcher_loaded_…` se lee como si
   estableciera algo. No establece nada: es una etiqueta. Lo mismo vale para un
   comentario de encabezado, que fue donde esta suposición vivió más tiempo.
2. **Se le pregunta al kernel, no al verbo bajo prueba.** Ahora las dos preguntan
   si el pin existe en el sistema de archivos, que es un hecho en el que
   `cambios` no participa. Una prueba que le preguntara a `cambios` si el watcher
   está cargado y luego comprobara `cambios` contra esa respuesta estaría de
   acuerdo consigo misma en cualquier máquina, incluida una rota.
3. **Ninguna de las dos ramas es un salto.** Las dos afirman algo real sobre la
   misma oración, así que el archivo prueba algo dondequiera que corra: sin
   watcher, que la negativa no se puede leer como respuesta vacía; con watcher,
   que la respuesta nunca dice `not_loaded` y sí dice las tres cosas que un
   anillo no puede dar. Un salto habría dejado la máquina de Cesar sin comprobar
   nada, que es justo donde hay algo que comprobar.

### Y la rama nueva casi repite el error

Al escribir la rama del watcher cargado —que **no se puede ejecutar en el
contenedor**— la primera versión afirmaba que la cara humana imprime «never which
file». Leyendo el código apareció que con el watcher cargado y **la cola vacía**
esa oración no se imprime: se imprime «this is not a history of the machine». La
aserción habría fallado en una máquina tranquila, o sea por el humor de la
máquina y no porque algo estuviera mal.

La regla que sale de eso: **una rama que no se puede correr se comprueba contra
el código, caso por caso, antes de creerle.** Y una aserción sobre una salida con
más de un resultado ordinario tiene que aceptar los dos, o nombrar cuál exige y
por qué.

## Un marcador que baja no dice qué dejó de correr — 2026-08-23

La sexta corrida en hierro contó **134 probadas** donde la del 2026-08-10 había
contado 143, y la reacción inmediata —la equivocada— fue buscar una regresión.
No había ninguna. El reporte ya decía lo que faltaba, en el bloque que se lee
después del número:

```
  · no kernel or image built yet, so there is nothing to boot; run 'make -C image'
```

Era la etapa 16 entera, trece comprobaciones contraídas a un solo `NOT PROVEN`
porque `image/build/` estaba vacío en esa máquina. Nada se había roto; nada se
había arrancado.

La regla: **el marcador y las líneas de `NOT PROVEN` son un solo resultado.**
Un conteo que baja no es una regresión hasta que se sabe qué dejó de correr, y
lo que dejó de correr no se deduce del número — se lee en la lista que el
script imprime debajo, que existe exactamente para eso.

El corolario para quien pide la corrida: **pedir la cola del reporte es pedir
la mitad del resultado.** `sudo ./dev/verify.sh 2>&1 | tail -40` alcanza para el
resumen; el diagnóstico de por qué un conteo cambió necesita la corrida
completa, y cuesta lo mismo guardarla con `tee` que perderla.

## Un hecho que la shell va a leer se cita, o el arnés lo parte — 2026-08-23

Duodécima vez que el instrumento miente, y esta vez el instrumento era mío.

La etapa 30 de `verify.sh` saca lo que Thalyx contestó a un archivo de
`clave=valor` y lo lee con `.`, porque comparar en shell es más legible que
comparar en Python. El Python escribía:

```
names=src/auth.rs src/deep/util.rs src/main.rs
```

La shell asignó `names=src/auth.rs` y **trató de ejecutar** los otros dos como
comandos. La etapa falló diciendo *«encontrar dijo '' donde find(1) dice
'src/auth.rs src/deep/util.rs src/main.rs'»*, que se lee exactamente como un
verbo que no contesta — y el verbo había contestado bien las tres.

La regla: **todo valor que la shell vaya a leer sale citado**, y en Python eso
es `shlex.quote` y no unas comillas escritas a mano. La forma general es la de
siempre y ya lleva doce instancias: antes de creer que Thalyx se equivocó, hay
que descartar que lo que preguntó se equivocó. Lo barato de ésta es que el
mensaje de falla ya traía la pista —el lado de Thalyx estaba **vacío**, no
distinto— y un vacío casi nunca es un defecto de cálculo.

## Una prueba que este usuario no puede hacer fallar tampoco prueba — 2026-08-23

La prueba de la regla 10 en `search.rs` quita todos los permisos a un archivo y
comprueba que la búsqueda lo reporta en `unreadable` en vez de contarlo como
«no coincide». **Como root, un modo `000` no detiene a nadie**, así que en este
contenedor el archivo se lee, no hay nada que reportar y la prueba pasaría sin
haber comprobado nada.

Es la regla 3 con una cara nueva: hasta ahora los saltos eran por lo que a la
*máquina* le falta —BPF, Btrfs, controladores delegados— y éste es por **quién
está corriendo**. La forma es la misma y no se negocia: la prueba pregunta si
la lectura de verdad falló, y si no falló imprime `NOT PROVEN` diciendo por
qué, con `THALYX_REQUIRE_UNREADABLE_TESTS=1` para convertir el salto en falla.
Una variable para este requisito y nada más, porque una variable para varios
obliga a exigir lo que la máquina no tiene para exigir lo que sí.

Lo que **no** se hizo, y era la tentación: dar por buena la prueba porque «la
lógica es obvia». Instalar un módulo que no se podía ejecutar también era obvio
durante semanas.

## `kill -0` contesta si el número existe, no si el proceso corre — 2026-08-23

Decimotercera vez que el instrumento miente, y otra vez era el arnés.

La etapa 31 comprueba que `matar … forzar` detiene una shell que ignora `TERM`.
Reportó que no la había detenido. La shell estaba muerta desde el primer
intento: **era un zombi**, y `kill -0` sobre un zombi contesta que sí, porque el
número sigue existiendo aunque el proceso ya corrió su última instrucción.

Por qué apareció ahora y no antes: la etapa arranca sus procesos en una subshell
para que bash no imprima `Killed` en medio del reporte, y eso los deja
huérfanos. En la máquina de Cesar systemd los cosecha de inmediato y las dos
preguntas se ven iguales. En este contenedor **PID 1 es `process_api`, que no
cosecha huérfanos**, así que el zombi se queda para siempre.

La regla: **`kill -0` responde una pregunta sobre el número, no sobre el
proceso.** Para «¿sigue corriendo?» hay que leer el estado en
`/proc/<pid>/stat` y contar `Z` como detenido. Y la forma general, que ya lleva
trece instancias: una diferencia entre dos máquinas que no tiene nada que ver
con lo que se está probando es del arnés, y hay que buscarla ahí antes de creerle
al veredicto.

**El corolario del control:** ese lector de estado tiene que tomar el campo
después del **último** `)`, exactamente como el parser que está comprobando —
porque un control que malinterpreta el formato no puede comprobar un parser de
ese formato. Un control escrito con la versión ingenua habría dicho que el
estado de `we (ird) x` es `(ird)` y habría contado como «no corriendo» a un
proceso vivo.

## Un sujeto que acepta la operación y no hace nada — 2026-08-23

`matar` se probó once veces en el prompt de verdad y ninguna encontró esto,
porque **las once usaban un proceso que sí se podía detener**. Lo encontró Cesar
en la primera sesión suya, ensayando `matar` sobre un `kworker`: Thalyx contestó
que le pediría que pare, y a un hilo del kernel no le llega ninguna señal.

Que la llamada al sistema conteste `0` no quiere decir que haya pasado algo. Hay
sujetos que aceptan la operación y la tiran:

- un hilo del kernel: `kill -9` contesta `0` y el hilo sigue ahí;
- un zombi: `pidfd_open` funciona, la señal se acepta, y sigue igual de muerto.

La regla: **cuando el éxito de un verbo se toma del valor de retorno de una
llamada al sistema, hay que probarlo sobre un sujeto que la llamada acepta y no
obedece.** Un conjunto de pruebas donde todos los sujetos funcionan mide que el
verbo funciona; no mide nada sobre lo que el verbo *dice*.

Es pariente de la regla 4 y no la misma. La regla 4 pide línea base y control
para una prueba de que algo **se niega**. Ésta es sobre **escoger el sujeto**: la
línea base y el control pueden estar los dos, impecables, y no encontrar nada si
el sujeto es siempre de los que responden.

**Y la línea base de la regla 4, aquí, es el defecto.** La etapa 32 le manda
`kill -9` al zombi con la herramienta de siempre, y comprueba que sigue listado.
Sin esa mitad, negarse a mandar la señal no se distingue de mandarla, y la etapa
estaría cuidando algo que no hace falta cuidar.

**Corolario, del lado de la respuesta:** un remedio que nombra otra cosa tiene
que traer cuál. `already_ended` contesta `stop_the_parent`, y quien lo recibe sin
el número del padre recibió una instrucción que no puede seguir.

## Una espera que la corrida anterior deja satisfecha no es una espera — 2026-08-23

`instalar-en` escribe la tabla de particiones, le pide al kernel que la relea y
**espera** a que aparezcan las particiones antes de escribir dentro de ellas. La
espera preguntaba si existía `/dev/loop0p1`.

Instalar por primera vez: no hay nodos, hay que crearlos, la espera espera de
verdad. **Instalar por segunda vez sobre el mismo disco: los nodos de la tabla
anterior siguen ahí**, la condición está cumplida antes de que empiece nada, y la
espera termina sin haber esperado. El instalador siguió adelante y murió en
`opening /dev/loop0p1: No such device or address`.

La regla: **una condición de espera tiene que ser falsa al principio.** Si la
puede satisfacer lo que quedó de la vez pasada, no está esperando a nada — y el
caso que la deja pre-satisfecha es justamente el que nadie prueba, porque es el
segundo.

**Y el corolario, que es el reverso de la regla 10:** una falla al leer no es una
falla al existir, y *existir* no es *estar*. `stat(2)` contesta por el nombre y
tiene éxito sobre un nodo cuya partición el kernel ya borró; sólo abrirlo
contesta por el dispositivo. La prueba que ahora lo fija hace un `mknod` con un
major que ningún driver de esta máquina registró —leído de `/proc/devices`, no
escogido a mano— y comprueba las dos mitades: el nombre existe y abrirlo da
`ENXIO`.

**Lo que sí funcionó:** la etapa lo encontró. Instalar dos veces es una
comprobación que existe porque una instalación interrumpida por un apagón tiene
que poder terminarse, y esa etapa es la única razón por la que esto se supo antes
de que le pasara a alguien con un disco de verdad.

## Un errno concreto es un hecho sobre la máquina — 2026-08-23, y es la catorce

La prueba que fija la regla de arriba —el nodo que existe y el dispositivo que
no— afirmaba que abrirlo da `ENXIO`. En este contenedor da `ENXIO`. En la Fedora
de Cesar dio **`EACCES`**, y tumbó la suite entera y con ella la corrida.

No era su máquina ni era Thalyx: **Fedora monta `/tmp` como un tmpfs con
`nodev`**, y en un sistema de archivos `nodev` no se puede abrir *ningún* nodo de
dispositivo, haya algo detrás o no. `tempfile` pone el nodo ahí. Medido, no
recordado: montar un tmpfs con `nodev` aquí y abrir un nodo dentro da `errno 13`.

La regla: **una prueba que fija un errno fija también la configuración de la
máquina donde se escribió.** Lo que se estaba probando era que el nombre resuelve
y el dispositivo no; `ENXIO` y `EACCES` son las dos maneras de que eso sea cierto,
y escoger una convirtió la prueba en una afirmación sobre dónde `tempdir()` deja
las cosas.

Catorceava instancia de la regla 5, y la primera en la que el instrumento
equivocado lo escribí como prueba nueva en el mismo arreglo que iba a comprobar —
un arnés recién hecho no tiene más crédito que uno viejo.

## El arnés corre como root y la persona no — 2026-08-23, y es la quince

Cesar instaló llama.cpp, corrió `thalyx agent model check` y **el modelo
contestó**: una inferencia real, parseada, en 7.28 s. Acto seguido `verify.sh`
reportó *«no real model has run: llama-completion is not installed»*.

Las dos cosas eran ciertas al mismo tiempo, y esa es toda la lección. El `check`
lo corrió él, con su `PATH`; la etapa corre bajo `sudo`, que tira el `PATH` y usa
`secure_path`. Un llama.cpp compilado en `~/.local/bin` —donde lo deja cualquier
guía— existe para él y no existe para la etapa.

La regla: **cuando el arnés corre con otra identidad que la persona, «no está
instalado» es una afirmación sobre el entorno del arnés, no sobre la máquina.**
Es la regla 10 aplicada al script mismo — una falla al leer no es una falla al
existir— y el costo es exacto: cuarenta minutos de corrida que terminan diciendo
que falta un programa que ya está, y alguien reinstalándolo.

Lo mismo con la otra mitad: `sudo` tampoco lleva el entorno, así que
`THALYX_AGENT_WEIGHTS=… sudo ./dev/verify.sh` pone la variable donde nadie la va
a leer. La asignación va **después** de `sudo`.

Ahora la etapa busca el binario también en el `PATH` de `$SUDO_USER` y, cuando lo
encuentra, dice dónde está y qué escribir para que la corrida lo vea — punto A2:
el error trae la línea que lo resuelve.

Y una nota sobre el arreglo, porque se ganó su lugar: la primera versión leía la
última línea de un shell de login. Una cuenta con `nologin` imprime una frase en
inglés, y esa frase salió ofrecida como la ruta del binario. Se comprueba que la
respuesta sea una ruta absoluta y ejecutable antes de creerle. **Un remedio
inventado es peor que ningún remedio**: manda a alguien a teclear algo que no
existe con la confianza de quien leyó una medición.

## La cara de máquina correcta esconde una cara humana que miente — 2026-08-23

`ensayo rm notas.txt` imprimía `removed /ruta/notas.txt` para un archivo que
seguía ahí. Cuatro pruebas del ensayo y ninguna etapa de `verify.sh` lo vieron, y
la razón es exacta: **la cara de máquina estaba bien todo el tiempo.** Su `op`
dice `rehearse`, así que un programa siempre pudo distinguir las dos cosas, y una
prueba que lee objetos no puede ver la frase que se le muestra a una persona.

La regla: **cuando un hecho se dice en dos caras, una prueba que sólo lee una de
ellas prueba una de ellas.** No es una prueba más débil, es una prueba de otra
cosa — y la que faltaba era la de la cara que no se puede parsear, que es
justamente donde una frase equivocada enseña algo falso sin que nadie lo note.

Es de la misma familia que lo de `matar`: una respuesta que dice que algo pasó
cuando no pasó. Y es peor que un error, porque quien la lee aprende a no creerle
a la siguiente frase tampoco.

La etapa 34 lee la frase, no el objeto, y trae las dos mitades de la regla 4: la
línea base es el verbo de verdad, que **sigue** diciendo `removed` —sin eso, un
impresor que dejó de funcionar se vería igual que un tiempo verbal corregido— y
el control es el disco visto desde afuera, porque un ensayo que sólo suavizara la
redacción mientras borra el archivo se leería idéntico en el log.

## Un árbol de fixtures está de acuerdo con quien lo escribió — 2026-08-23

`thalyx-net` tenía doce pruebas contra un árbol que este repositorio escribe, y
las doce pasaban. La primera vez que el verbo corrió contra una máquina de
verdad reportó **tres tarjetas de red en una máquina con una**: `ifb0` e `ifb1`
—los dispositivos de bloque funcional intermedio del kernel— dicen `type 1`,
traen dirección física y son software puro.

Ninguna prueba podía verlo, porque las interfaces del árbol de fixtures eran las
que a mí se me ocurrió poner. Es la regla 6 —un parser de la salida de otro
necesita una muestra real— extendida a algo que no es un parser: **un árbol de
fixtures prueba que el lector coincide con el modelo de quien lo escribió, y el
modelo era el defecto.**

Lo que lo atrapó fue teclear `red` una vez. Lo que lo deja atrapado la próxima es
la prueba que ahora corre contra el `/sys/class/net` de la máquina donde está,
con `THALYX_REQUIRE_REAL_SYSFS_TESTS` para que un salto sea un fallo, y la etapa
35, que compara contra `iproute2` — netlink y no sysfs, que es la única manera de
que el control no sea el mismo instrumento otra vez.

## Dos máquinas, dos respuestas, y la segunda vez que lo hago — 2026-08-23

La prueba nueva de `thalyx-net` afirmaba que **una interfaz abajo se niega a
contestar si tiene cable**. Aquí es cierto: `ifb0` e `ifb1` dan `EINVAL`. En la
Fedora de Cesar, un puente de Docker que está abajo contesta `0` con toda
honestidad, y la prueba tumbó la suite entera y con ella su corrida.

Negarse o contestar **es del driver, no de estar abajo**. Una tarjeta física que
nunca se levantó se niega; un puente de software sin nada conectado contesta que
no hay nada conectado, que es la verdad.

Y el módulo nunca necesitó eso. Lo que necesita es que **una lectura fallida
jamás se reporte como cable ausente**, que es una propiedad de este código y es
cierta en toda máquina. La prueba ahora lee el mismo archivo por su cuenta y
compara el mapeo; lo que la máquina no pueda enseñar —una negativa de verdad—
sale como `NOT PROVEN` con su propia variable.

La regla es la misma que la del `ENXIO` contra `EACCES`, y **es la segunda vez en
dos días**: una prueba que fija la respuesta concreta de un kernel fija también
la máquina donde se escribió. La diferencia es qué la produjo, y esa parte es
nueva: la primera vez fue una opción de montaje; ésta fue **contar dos ejemplos y
llamarlo la regla**. Dos interfaces de la misma clase, en la misma máquina, no
son una muestra de nada.

Lo que sí queda escrito: cuando una prueba tenga que afirmar cómo contesta el
kernel, la pregunta correcta no es *«qué contestó aquí»* sino *«qué de esto es
del código que estoy probando»*. Lo primero es un hecho sobre una máquina. Lo
segundo es lo que la prueba existe para fijar.

## Regla derivada: un catálogo que se describe a sí mismo puede mentir sobre sí mismo — 2026-08-23

`describe` es lo primero que un programa lee, y por cada verbo dice si contesta
por estructura o sólo en prosa. **Esa afirmación es la que decide si el verbo se
llama siquiera**: un verbo declarado sólo-prosa es un verbo que un programa
nunca invoca, así que la afirmación equivocada cuesta el verbo entero y no
cuesta ni un error.

Fue exactamente así. `red` se construyó el 2026-08-23 con sus dos caras —el
objeto trae `addressable: false`, que era el punto del verbo— y quedó declarado
`answers: None` en el catálogo. Durante el día entero, la única lista de
hardware de red que esta máquina tiene fue **invisible para todo lo que
preguntara antes**.

Lo que no lo vio, y por qué:

- **Las pruebas unitarias del catálogo** afirmaban que `modules` seguía en la
  lista de sólo-prosa. Un `contains` sobre un ejemplo no puede ver que otro se
  movió. Ahora la lista entera está fijada, así que agregar una cara obliga a
  editarla, y ese renglón es el momento de comprobar que la afirmación es cierta.
- **Las pruebas de `net`** ejercen la cara estructurada y pasan: el verbo sí
  contesta. Nadie estaba mintiendo sobre el verbo, sino sobre el verbo *en el
  catálogo*, y son dos archivos donde **cada uno concuerda consigo mismo**.

La regla: **cuando un sistema publica una afirmación sobre sí mismo, la
comprobación es correrlo y leer el cable, no leer los dos lados del código.**
La etapa 22 ahora maneja los catorce verbos que se pueden correr aquí sin
argumentos y compara lo que sale contra lo que `describe` prometió, en las dos
direcciones — una promesa sin objeto detrás, y un objeto de un verbo que
prometió prosa. El control es el de siempre: con el defecto devuelto a mano, la
etapa nombra `red:promised-prose-answered-network`; sin él, `ok:14`.

Y una consecuencia de la regla 3 que vale escribir: **una negativa cuenta como
cara estructurada.** Un `op` que dice que no pudo sigue siendo el verbo
contestando por estructura, que es la regla 10 sobre el cable.

## Regla derivada: una lista de excepciones fijada por ejemplo no ve moverse a las demás — 2026-08-23

La primera versión de la prueba que atrapó lo de `red` decía *«`modules` sigue
en la lista de sólo-prosa»*. Es cierto y no sirve: **un `contains` sobre un
ejemplo no puede ver que otro se movió.** La lista entera fijada sí, y con eso
agregar una cara obliga a editar ese renglón — que es el momento exacto en que
alguien puede comprobar que la afirmación nueva es cierta.

Y el final de esa lista enseña la otra mitad. Cuando los cuarenta verbos
tuvieron cara, la prueba dejó de fijar una lista y pasó a afirmar que **está
vacía**. Una lista vacía es una afirmación más fuerte que cualquier lista: un
verbo nuevo sin cara tiene que agregarse ahí a mano, o sea tiene que decir en voz
alta que nace incumpliendo el decreto.

**El corolario, que cuesta poco y se olvida:** cuando una prueba fija un conjunto
que se espera que se vacíe, la prueba tiene que seguir teniendo sentido vacía.

## Regla derivada: dar una cara nueva a un verbo cambia lo que las otras pruebas leen — 2026-08-23

`salir` es lo que termina cada sesión que la etapa 22 maneja. En cuanto contestó
por estructura —que era correcto y necesario, porque un pipe cerrado y vacío es
exactamente lo que parece un cierre inesperado— **empezó a aparecer un objeto de
más en el registro de los veintiún verbos**, y tres de ellos se reportaron como
incumplidos sin que nadie hubiera tocado nada suyo.

No es un defecto del verbo ni de la etapa: es que el arnés usa el sistema que
mide, y el terminador dejó de ser mudo. Se resuelve nombrando el ruido —
`structured` y `leave` no son el verbo bajo prueba— y la regla que queda es la
5, con una forma nueva: **cuando el instrumento se maneja a sí mismo, ampliar lo
que el sistema dice amplía lo que el instrumento lee.** Antes de creerle a un
fallo así, ver si lo que cambió fue el sujeto o el arnés.

Lo barato de este caso es que se vio en un renglón: los tres «incumplimientos»
nombraban `leave`, que es el único verbo que ninguno de los tres había invocado.

## Regla derivada: una guarda que se vuelve implícita sigue funcionando hasta el día que no — 2026-08-23

En `TerminalConfirmer` —el camino confiable, el sitio donde una persona concede
algo— vivía esto:

```rust
let answer = crate::term::read_answer().ok().flatten().unwrap_or_default();
if false {
    return false;
}
```

El `if false` es basura obvia de una mudanza del 2026-08-09, cuando esa lectura
pasó al lector único de `stdin`. Lo que no es obvio es qué reemplazó: la forma
anterior era `if read_line(..).is_err() { return false }`, o sea **una lectura
fallida no es un sí**. La forma nueva pliega el error en `unwrap_or_default()`,
que da la cadena vacía, que no es `y`, así que se sigue negando.

**El comportamiento nunca cambió. La razón sí.** Pasó de estar escrita a
sostenerse por lo que `String::default()` resulta ser. La regla 9 dice que una
entrada corrupta recibe la respuesta cautelosa; una regla 9 que se cumple por
accidente del valor por omisión de otro tipo deja de cumplirse el día que alguien
cambia ese valor — y esto es el único lugar del sistema donde la respuesta
cautelosa *es* el punto.

Y lo que lo hizo visible fue precisamente la basura: **un `if false` que nadie
podía borrar porque nadie sabía qué había estado guardando.** Un refactor
mecánico que deja el cascarón de una guarda está señalando dónde se perdió una
razón, aunque no lo parezca.

La regla: **cuando un refactor conserva un comportamiento pero borra el enunciado
que lo pedía, la propiedad quedó sin dueño.** En código que falla cerrado, el
enunciado se vuelve a poner aunque no cambie nada, porque lo que se está
protegiendo no es el resultado de hoy sino el de la próxima edición.

## Regla derivada: un vocabulario de palabras ordinarias rompe las pruebas que lo buscaban — 2026-08-24

El día que la gramática del agente pasó de una operación a treinta y nueve,
**tres pruebas fallaron y ninguna era por el cambio**. Las tres buscaban una
palabra como subcadena, y las palabras nuevas son inglés corriente:

- una prueba de la gramática afirmaba que `permissions` no aparece en ninguna
  parte, porque es un campo que el núcleo decide y al modelo nunca se le
  pregunta. Ahora es también el nombre de un verbo —el que **muestra** qué
  tiene un módulo, que es leer y no decidir— así que la prueba reportó el
  catálogo como una fuga de procedencia;
- el brazo de prosa del experimento de gramática afirmaba que no nombra ninguna
  operación, y falló por contener la palabra `where`;
- la sonda leía la producción `root` esperando un objeto, donde ahora hay tres
  alternativas.

Ninguna de las tres estaba mal cuando se escribió. **Dejaron de medir lo que
decían cuando el vocabulario dejó de ser artificial.** La corrección en los tres
casos es la misma: hacer la pregunta precisa en vez de la barata. `permissions`
como **campo** —`"permissions" ws ":"`— y no como cadena; el leak real, que es
`install_module`, y no la palabra `install`, que el humano teclea y el brazo de
prosa está obligado a mostrar; y **cada** alternativa de `root`, no la primera.

> Cuando un conjunto de nombres pasa de ser inventado a ser el idioma en que
> está escrito todo lo demás, toda prueba que lo busque por subcadena hay que
> volver a leerla. Sigue pasando; ya no dice nada.

## Regla derivada: una forma nueva al lado de una validada se salta lo que la validaba — 2026-08-24

`Plan` pasó de una forma a dos: un contrato, y un verbo que no lo es. El
contrato corre `Contract::validate` de salida, y ahí adentro está
`origins.validate()`, que es la comprobación que **rechaza una operación
concluida mientras se leía una página hostil**.

La forma nueva no tiene contrato. Así que no llegaba a esa comprobación, y la
regla de procedencia habría quedado con **una puerta rotulada `read`**: un
modelo que, mirando un documento ajeno, concluyera que hay que leer algo,
habría sido un modelo cuya conclusión nadie examinaba.

No lo encontró una prueba. Lo encontró leer el compilador quejándose de otra
cosa —una sonda que imprimía «se produjo un contrato» y ya no siempre produce
un contrato— y preguntarse qué más viajaba dentro de esa palabra.

> Una defensa que vive dentro de un tipo se pierde cuando aparece un segundo
> tipo. Al agregar una rama, la pregunta no es «¿funciona?» sino **«¿qué corría
> el camino viejo que éste ya no corre?»** — y hay que contestarla mirando el
> camino viejo, no la rama nueva.

Prueba en los dos sentidos, porque cualquiera sola pasa sin decir nada: la
lectura inyectada se rechaza, y la lectura que el humano pidió no.

## Regla derivada: un permiso por argumento escrito desde el manual es un permiso escrito desde tu modelo — 2026-08-24

La regla 6 dice que un analizador para la salida de otra herramienta necesita
una muestra real capturada. **Vale igual para los argumentos de una llamada al
sistema.**

El guardia de `sched_setscheduler` se escribió primero permitiendo
`SCHED_OTHER`, `SCHED_BATCH` y `SCHED_IDLE` — las tres políticas que no son de
tiempo real, que es lo que dice el manual y lo que cualquiera escribiría. Luego
se leyó la traza: Node pide `0x40000000`, que es `SCHED_OTHER |
SCHED_RESET_ON_FORK`, en cada uno de sus hilos. La bandera no es una política,
se le suma a una, y el manual la documenta en otro párrafo.

Ese guardia habría matado al agente ajeno en la llamada exacta que el guardia
existe para dejar pasar — **y habría parecido el guardia funcionando**, porque
la política pedida no estaba en la lista y morir es lo que hace el filtro.

> Una lista de valores permitidos es un analizador del formato de ese argumento.
> Se escribe mirando lo que un programa real manda, no lo que la documentación
> enumera.

## Regla derivada: un guardia probado sobre la llamada guardada no prueba el camino hasta ella — 2026-08-24

El día anterior el filtro aprendió a mirar un argumento, y su prueba unitaria
—permitido lo ordinario, muerto lo de tiempo real, muerto lo que nadie ha
definido— pasaba. Al día siguiente `dev/verify.sh` reportó `sched_ordinary=159`:
el módulo confinado murió con `SIGSYS` **en la llamada que el guardia existe
para dejar pasar**.

El guardia estaba bien. El camino no. `chrt --other 0 true` pregunta primero
cuál es el rango legal de prioridades:

```text
sched_get_priority_min(SCHED_OTHER)     = 0
sched_get_priority_max(SCHED_OTHER)     = 0
sched_setscheduler(0, SCHED_OTHER, [0]) = 0
```

Las dos primeras no estaban en la lista. Ninguna de las dos hace nada: contestan
una constante para un número de política. Con ellas ausentes, el programa moría
antes de llegar a la única línea sobre la que alguien había pensado.

**Y la columna de al lado decía verde.** `chrt --fifo 1 true` también moría con
`SIGSYS`, en esa misma primera línea, sin haber nombrado jamás una política de
tiempo real — y eso se lee idéntico a que el guardia lo haya rechazado. La
prueba de denegación afirmaba exactamente lo que quería afirmar, sin haberlo
medido ni una vez. Es la regla 4 con la trampa cerrada de otro modo: aquí el
control y el caso son **el mismo programa**, así que la denegación sólo puede
leerse mientras la columna ordinaria dé 0. `verify.sh` ahora se calla en vez de
felicitarse cuando no lo da.

Por qué la prueba unitaria no podía verlo: le preguntaba al filtro por la
llamada guardada. Un programa real no llega ahí primero. Lo que se agregó en su
lugar corre de verdad —instala el filtro en un proceso aparte, porque un filtro
es irrevocable y heredado, y corre `chrt` bajo él— con las dos columnas en una
sola prueba para que nadie las lea por separado.

> **Una llamada permitida no es una capacidad permitida.** El permiso se prueba
> con el programa que ejerce la capacidad, no con la llamada que le da nombre:
> el camino hasta esa llamada es parte de lo que hay que permitir.

Y una nota sobre [[Que-Necesita-Un-Agente-Ajeno]], que es la medición de donde
salió este guardia: **no habría encontrado esto**, y no por estar mal hecha.
Claude Code no pregunta el rango de prioridades; `chrt` sí. Una traza es un
programa, no todos, y la nota ya lo dice de sus rutas — vale igual para sus
llamadas.

## Regla derivada: una comprobación negativa sobre la prosa del propio arnés se vuelve vacía cuando la prosa cambia — 2026-08-24

`dev/verify.sh` corre la sonda de inyección con siete formas de portarse mal y
falla si alguna produjo un contrato. La busca así:

```sh
grep -q "A CONTRACT WAS PRODUCED" "$WORK/probe-$BEHAVIOUR.log"
```

Ese mismo día la sonda dejó de producir sólo contratos —un verbo también es
actuar— y pasó a imprimir `A PLAN WAS PRODUCED`. Las siete comprobaciones
siguieron pasando. **Pasaban por vacías:** buscaban una cadena que ya no se
escribe en ninguna parte, así que habrían pasado igual con las siete sondas
obedeciendo la página hostil.

Lo que lo agarró fue el control, que busca la misma cadena **en el sentido
positivo** —el mismo modelo, preguntado por lo que el humano tecleó, tiene que
producir uno— y falló ruidosamente. Esa etapa existe desde el principio por la
regla 4, contra un arnés que rechaza todo; sirvió para lo que no estaba escrito:
mantener honesta a la cadena de la que dependen las siete.

La otra mitad del mismo día fue la etapa de la gramática, que exigía
`install_module` a una gramática que ahora deletrea el verbo como la sesión
—`install`— y reportó que `thalyx agent grammar` no imprime una gramática. Las
dos fallas acusaban a Thalyx de lo que había cambiado el arnés.

> Toda comprobación que busque una cadena que el propio sistema imprime necesita
> otra que **exija** esa cadena. Un `grep` negativo sin su control positivo no
> distingue «no ocurrió» de «ya no se dice así».

## Regla derivada: un conjunto leído del archivo entero incluye lo que el archivo niega — 2026-08-24

`dev/foreign-agent-needs.sh` compara las llamadas que un agente ajeno hace al
arrancar contra la lista que un módulo recibe. La lista la sacaba así:

```python
allowed = set(re.findall(r"libc::SYS_([a-z_0-9]+)", seccomp))
```

De todo el archivo. Y el archivo tiene 162 nombres, de los cuales **32 son
precisamente los que un módulo no tiene**: los que las pruebas nombran para
afirmar su ausencia —`socket`, `connect`, `bind`, `ptrace`, `mount`, `bpf`,
`init_module`, `kexec_load`, `keyctl`— y los que sólo agrega un permiso de red
concedido. Un agente que hubiera llamado a `socket` para arrancar habría salido
en el reporte como cubierto.

Corregido a leer el cuerpo de `module_standard`, y el de `outbound_network`
aparte, que es la distinción que el propio crate mantiene. La respuesta no
cambió —41 de 41— y eso es suerte, no una razón para seguir preguntando mal.

> Cuando un conjunto se extrae por patrón, **el alcance del patrón es parte de
> la afirmación**. Un archivo contiene la lista y también su negación, y un
> patrón que no las distingue reporta lo negado como concedido.

## Regla derivada: una capacidad con dos puertas se guarda por la que el filtro puede mirar, y la otra se cierra — 2026-08-25

El guardia de `sched_setscheduler` quedó bien el 2026-08-24 y la prueba nueva
—un programa real bajo el filtro real— pasó en el contenedor. En la máquina de
Cesar falló: `chrt` moría con `SIGSYS` poniendo una política ordinaria, otra vez,
con la lista ya corregida.

No faltaba nada en la lista. **La capacidad tiene dos puertas.** `sched_setattr`
pone la política igual que `sched_setscheduler`, pero la recibe dentro de una
estructura, detrás de un puntero — y un filtro de seccomp compara registros y no
puede seguir un puntero. Para esa puerta **no existe guardia por argumento**: o
se permite entera, con `SCHED_FIFO` adentro, o se deniega entera.

Lo que esto le enseña a una prueba es más general que el caso:

> Cuando una capacidad se puede pedir por dos llamadas y el filtro sólo puede
> leer los argumentos de una, **probar la que se puede leer no prueba la
> capacidad**. Hay que buscar la segunda puerta antes de dar el guardia por
> hecho, y decir en voz alta qué se hace con ella: cerrarla tiene un costo y ese
> costo es parte del decreto, no un defecto que alguien redescubre.

Cesar decidió cerrarla. El costo quedó escrito en [[Sandbox-Ejecucion]], junto
con el único mecanismo que podría mirar detrás del puntero —un supervisor con
`SECCOMP_RET_USER_NOTIF`— y la razón para no construirlo hoy.

## Regla derivada: un instrumento tiene versión, segunda vez, y ahora la versión cambiaba la llamada — 2026-08-25

La primera vez fue clippy: dos versiones, dos opiniones sobre el mismo código.
Ésta es peor de leer, porque el instrumento no cambió de opinión sino **de
llamada al sistema**.

`chrt --other 0 true` pone una política ordinaria. Hasta util-linux 2.40 lo hace
con `sched_setscheduler`; desde 2.41 —que agregó `supports_custom_slice`— lo hace
con `sched_setattr`. El contenedor tiene 2.39 y su máquina 2.41, así que la
misma prueba, sobre el mismo filtro, midió dos cosas distintas: en el contenedor
la llamada guardada, y en su máquina una llamada denegada de plano.

Y el verde del contenedor fue **suerte de versión**, no una comprobación. La
prueba se escribió el mismo día en que se dijo, en el mismo archivo, que un
programa real es mejor instrumento que una llamada aislada — y lo es; lo que
faltó fue preguntarse *qué llamada hace ese programa en la máquina donde va a
correr*, que es la misma pregunta de la regla 6 sobre las muestras capturadas.

La corrección no fue quitar el programa real: fue elegir un pedido suyo que
ninguna versión manda por la puerta cerrada. `chrt --idle 0 true` usa
`sched_setscheduler` en todas, y `SCHED_IDLE` es una de las tres políticas que el
guardia permite. `--other` se sigue corriendo, como **reporte y no como
veredicto**, con `strace` diciendo por cuál de las dos llamadas pasó — porque el
costo de la puerta cerrada tiene que verse en la máquina donde se paga.

> Un programa ajeno es un buen instrumento y **su versión es parte del
> instrumento**. Antes de apoyar una afirmación en lo que hace, hay que saber si
> lo que hace es estable entre las máquinas donde va a correr — y si no lo es,
> pedirle lo que sí lo sea.

## Regla derivada: «este contenedor no puede comprobarlo» también es una afirmación sin medir — 2026-08-25

El pendiente del permiso sobre **un archivo** llevaba abierto desde el
2026-08-04, y una de las razones por las que llevaba abierto es que se daba por
hecho que arma una raíz remapeada de verdad y por lo tanto espera hierro.

No espera hierro. `/proc/mounts` de este contenedor tiene un `cgroup2` montado en
`/sys/fs/cgroup/unified`, y `mount_point()` lo encuentra: las veinticuatro
pruebas de `isolation.rs` **corren aquí**, pivote y montaje remapeado incluidos.
Lo que este contenedor no tiene son los **controladores delegados** —`memory` y
`pids`—, que es otra cosa y es la que dice `THALYX_REQUIRE_CONTROLLER_TESTS`.

Las dos pruebas nuevas se escribieron esperando un `NOT PROVEN`, y pasaron. La
única razón por la que se supo que habían pasado *por haber corrido* es que se
rompieron a propósito: cambiada la línea esperada, las dos fallan en la
aserción del anfitrión, después de que el módulo confinado escribió. Sin ese
paso, un `ok` en 0.03 s se lee idéntico a un salto silencioso.

> Que el entorno de desarrollo no pueda comprobar algo es un hecho sobre el
> entorno, y como cualquier hecho **se mide o no se sabe**. Cuesta un minuto
> comprobarlo y puede tener un pendiente parado meses. Y una prueba que se
> creía saltada y pasa se rompe a propósito antes de creerle: la regla 3 dice
> que un salto tiene que decirse, y ésta dice que un `ok` tiene que ganarse.

## Regla derivada: una pregunta que se contesta sola no es la pregunta que importa — 2026-08-25

Cesar corrió `ejecutar /usr/bin/node --version` en su máquina, justo después de
`verify.sh`, y leyó la negativa correcta: *«the kernel policy map is not
loaded»*. El remedio que ese mensaje da es `make -C lsm load`. Y
`make -C lsm load` aterriza **a propósito** en modo observación.

O sea que la única acción que el sistema le pedía dejaba la máquina en el estado
donde el verbo sí arrancaba al invitado y el kernel no le negaba nada.

La pregunta que el código hacía era `is_available()` — *¿se abre el mapa de
políticas?* — y **se contesta sola**: es cierta en cuanto algo está cargado. La
pregunta que importaba era *¿una negación llega como `-EPERM`?*, que vive en otro
mapa, `thalyx_enforcing`, que **nada en el lado de Rust había leído nunca**. Sólo
el `Makefile` lo consultaba, con `bpftool`.

Lo que lo escondió es que las dos preguntas son verdaderas al mismo tiempo en la
única máquina donde alguien miraba: la del demo de enforcement, que enciende el
modo él mismo antes de medir y lo apaga después.

Y ninguna prueba lo agarró porque **ninguna prueba lo podía agarrar**: el falso,
`MemoryStore`, no tenía cómo representar «cargado y sin negar». No es que el
falso fallara la propiedad — la regla 8 habría bastado —, es que **la propiedad
no existía en el falso**, así que ni la prueba ni su control podían nombrarla.

> Antes de creerle a una guarda, pregúntate **qué máquina la haría fallar**. Si
> la respuesta es «ninguna que alguien vaya a correr», la guarda no está
> midiendo lo que dice: está midiendo algo adyacente que siempre viene junto.
> Y si el falso no tiene un estado para el modo de fallo, el modo de fallo no
> tiene prueba, por muchas que haya.

Es la treceava vez que el instrumento resulta ser el problema, y la cuarta que
esta parte del sistema pregunta algo que no es lo que quiere saber. Las tres
anteriores están arriba en el propio comentario de `KernelStore`: preguntarle a
`bpftool` por algo que `bpftool` no hizo. Ésta llega por el lado contrario —
preguntarle al kernel algo cierto que no es lo que se necesitaba.

## Regla derivada: un rodeo dentro de un fixture es un hallazgo que nadie escribió — 2026-08-25

Media hora después de arreglar el modo de enforcement, el primer invitado que
corrió bajo un kernel que **sí niega** murió antes de `exec`: sin concesiones la
política sale `allowed=0x0`, `lsm/file_open` no mira rutas, y la primera lectura
del lanzador después de entrar al cgroup es lo primero que esa política contesta.

Lo que importa aquí no es el defecto. Es que **ya estaba escrito**, en la
cabecera de `lsm/demo-enforcement.sh`:

> *Creates one cgroup and puts a policy in the map for it: **filesystem
> allowed**, network denied.*

Quien escribió ese demo tuvo que permitir el sistema de archivos, porque si no
el `python3` de adentro no arrancaba y el demo habría medido `exec` en vez de
`connect`. O sea que **la conclusión —un proceso confinado necesita leer para
existir— se descubrió, se rodeó, y se quedó dentro del script como un detalle de
montaje.**

Y nada la contradecía porque nada más corría bajo enforcement: `verify.sh` va de
principio a fin en modo observación, así que durante semanas el único código que
se ejecutó con la política aplicada de verdad fue el de ese demo.

> Cuando un fixture tenga que **aflojar algo** para que el sujeto arranque, esa
> línea no es configuración: es una medición. Escríbela donde se busque un
> decreto, no donde se busque un fixture. Un rodeo que se queda en el arnés se
> vuelve a encontrar desde cero, con el sistema real, delante de la persona que
> confió en él.

## Regla derivada: mirar antes de crear es una carrera, y una prueba secuencial no la ve nunca — 2026-08-26

`verify.sh` en el fierro de Cesar: 171 `PROVEN`, 2 `NOT PROVEN`, 4 `FAILED`. Uno
de los cuatro era la suite, y dentro de la suite **un solo test de diez**:

```
a_path_granted_for_reading_cannot_be_written_by_the_guest --- FAILED
I/O error at /sys/fs/cgroup/thalyx: File exists (os error 17)
```

El mensaje dice lo contrario de lo que pasó. `File exists` se lee como una
máquina en mal estado, y la máquina estaba perfecta: el directorio existía
porque **otro hilo de la misma suite acababa de crearlo**.

`cgroup::parent()` preguntaba y después creaba:

```rust
if !path.exists() {
    std::fs::create_dir(&path)?;   // otro llegó primero, entre las dos líneas
}
```

Diez tests del mismo binario corren en paralelo y todos llaman a `run_foreign`,
que empieza por `parent()`. En una máquina con suficientes núcleos, dos caen en
la ventana entre las dos líneas: el primero crea, el segundo recibe `EEXIST`. En
este contenedor no hay cgroup2, así que ese código nunca se ejecutó aquí; en su
máquina falló una vez de cada diez, que es la peor forma de fallar que hay
porque el que la ve no puede repetirla.

Lo mismo estaba en `Cgroup::ensure` — dos `ejecutar` del mismo programa al mismo
tiempo — y **el espejo estaba en el desmontaje**: `remove()` reportaba
`No such file or directory` cuando otra instancia ya había borrado el cgroup, o
sea una falla sobre un invitado que había corrido y salido exactamente como se
le pidió.

> **No preguntes si algo existe para después crearlo.** Créalo y decide qué
> hacer con `EEXIST`; bórralo y decide qué hacer con `ENOENT`. El sistema de
> archivos hace las dos cosas en una sola llamada, atómicamente, y la versión de
> dos pasos tiene una ventana en medio donde cabe otro proceso.
>
> Y `EEXIST` se perdona **sólo para un directorio**. Un archivo ordinario con
> ese nombre acepta que le escriban `cgroup.procs`, no confina nada, y cada paso
> reporta éxito: es la falla sin síntoma que este módulo existe para rechazar.

Lo que la prueba tiene de distinto: el defecto viejo era invisible para
cualquier test secuencial, porque el `exists()` lo hacía imposible de provocar
en un solo hilo. La prueba nueva pone ocho hilos contra una barrera y los suelta
juntos —
`two_runs_racing_to_create_the_same_cgroup_both_get_it` — y con el código viejo
falla las ocho veces de ocho, no una de diez: sin el `exists()` de por medio, el
segundo `mkdir` recibe `EEXIST` siempre. **La prueba de una carrera se vuelve
determinista cuando se arregla la carrera**, y eso es lo que la hace sostener
algo.

Es la primera vez que un defecto de este proyecto es una carrera del sistema de
archivos, y la tercera vez que la suite se pelea consigo misma por un recurso
que creía suyo — `ETXTBSY` por el ejecutable que acababa de escribir es la que
tardó un año. Las tres tienen la misma forma: **el test corre en la misma
máquina que los otros tests.**

### Y el reporte no traía con qué diagnosticarlo

Los otros tres `FAILED` de esa corrida eran las tres etapas de §36 que lanzan un
invitado, y lo único que el reporte decía de cada una era
`see /tmp/tmp.XXXX/exec-run.log` — un archivo que sólo existe en la máquina de
Cesar. Diagnosticarlos costaba una vuelta entera: pedirle que fuera a leerlo.

> Un veredicto que nombra un archivo de log no se lee donde está el archivo.
> Si el log es corto, imprímelo junto al veredicto.

`verify.sh` ahora tiene `excerpt`, y **las 111 salidas del script que nombran un
log imprimen su cola** — no sólo las de §36. Las que ya volcaban el archivo
entero se quedaron como estaban: dos copias de un log es ruido, y la
comprobación que las distingue no acepta un `grep` como «ya lo imprime», porque
un `elif grep -q … ; then` nombra el mismo archivo y no imprime nada.

## Regla derivada: la precondición que todo el guion da por hecha es la que nadie mide — 2026-08-26

Segunda corrida en el fierro, el mismo día: **169 `PROVEN`, 3 `NOT PROVEN`, 12
`FAILED`**, con la corrida anterior en 171/2/4 y **ningún cambio de código entre
las dos que tocara nada de lo que se rompió**. Doce fallas nuevas y la misma
frase debajo de casi todas:

```
thalyx: I/O error at /run/thalyx/sandbox/dev/null: Operation not permitted (os error 1)
```

El diagnóstico está abajo, en su propia regla. Lo que importa aquí es **por qué
las dos corridas no dieron lo mismo**: en la segunda, el kernel estaba
*negando* durante etapas escritas para una máquina que sólo observa.

`verify.sh` corre en modo observación de principio a fin, salvo donde §36 y §37
arman la máquina a propósito. Eso está escrito en la bóveda, está escrito en los
comentarios del guion, y **no lo medía nadie**. Ni una línea del reporte decía
en qué modo estaba el kernel cuando corrió la etapa 6.

> Una precondición que todo el guion da por hecha, y que alguna de sus propias
> etapas puede cambiar, tiene que **medirse donde se usa**, no declararse arriba.
> Mientras no se mida, «el módulo no pudo» y «al módulo nunca se le dejó
> intentar» son la misma salida — que es la regla 4 vista desde el otro lado — y
> lo que se movió fue el instrumento, que es la 5.

Se arregló donde no hace falta saber cuál etapa lo movió: **`step()` lee el modo
al anunciar cada etapa.** Corre antes de que la etapa arme nada, así que lo que
lee es lo que dejó la *anterior*, y por eso §36 y §37 no necesitan excepción. Si
encuentra la máquina negando, lo dice como `FAILED` con el nombre de la etapa y
la devuelve a observación, para que lo que sigue vuelva a ser sobre la máquina
que el reporte nombra.

### Y la etapa que nunca pudo pasar

En la misma corrida, `exec-bare` y `exec-endure` fallaron con el invitado
diciendo *«the kernel side is attached but only observing»*. No era el kernel: el
guion devolvía la máquina a observación **inmediatamente después de `exec-run`**,
y las dos etapas que lanzan invitados vienen *después* de esa línea. Les pedía a
las dos que arrancaran un programa ajeno en una máquina que él mismo acababa de
desarmar, y `ejecutar` hacía lo único correcto: negarse.

**Nunca pasaron, ni una vez, desde que se escribieron.** Y el reporte las contaba
como dos `FAILED` de G1, o sea acusando al sujeto de lo que hacía el arnés — la
decimoquinta vez. La restauración ahora va después del último invitado, que es
donde siempre debió estar.

## Regla derivada: el confinamiento se arma antes de ponérselo, no después — 2026-08-26

El defecto que estaba debajo de las doce fallas de la segunda corrida, y es uno
solo:

```
thalyx: I/O error at /run/thalyx/sandbox/dev/null: Operation not permitted (os error 1)
```

La cadena, leída renglón por renglón y no supuesta:

1. `launch::enter` entraba al cgroup **antes que nada**.
2. Ya adentro, armaba la raíz. Las rutas de sistema son directorios, y `mkdir`
   no pasa por `lsm/file_open`.
3. `/dev/null` es un dispositivo de caracteres. Su punto de montaje se crea con
   `File::create`, o sea `open(O_WRONLY|O_CREAT)`.
4. `lsm/file_open` hace `writing = flags & 3`, distinto de cero, y pregunta por
   `THALYX_FS_WRITE`.
5. La política decía `allowed=0x2` — `FS_READ`, el piso, y nada más.
6. `check()` devuelve `-EPERM` cuando está negando.

O sea: **el LSM le negaba a Thalyx el trabajo de confinar.** En una máquina que
niega de verdad no se podía lanzar absolutamente nada — ni un invitado ni un
módulo firmado — y la única razón por la que no se había visto es que
`verify.sh` corre en observación, donde `check()` devuelve 0 y la apertura pasa.

Es **el mismo defecto del 2026-08-25**, el que produjo `CONFINED_FLOOR`: el
lanzador muere en el primer archivo que abre después de entrar al cgroup. Aquel
se arregló para las lecturas. Nadie preguntó qué escribe el lanzador, y escribe
dos cosas: los puntos de montaje de los cinco nodos de dispositivo, y el
`uid_map` del ayudante que hace un bind remapeado.

> Un arreglo que resuelve *la lectura* de un problema que es «el sujeto no puede
> hacer su propio trabajo bajo su propia política» no lo resolvió: lo resolvió
> para la mitad que falló ese día. Cuando encuentres que **el que aplica la
> regla queda sujeto a ella**, la pregunta no es qué operación falló, es
> **cuáles operaciones hace** — todas.

Y la salida no era ampliar el piso. `FS_WRITE` en el piso le daría a todo módulo
y a todo invitado escritura sobre todo lo que alcanza a ver, que es exactamente
lo que la etapa 36 comprueba que no pasa. La salida es que el trabajo de Thalyx
**no corra bajo la política del módulo**: `RootFs` se partió en `assemble()` —
donde está toda la escritura— y `pivot_into()`, y el cgroup se toma entre las
dos. Nada del módulo corre antes por eso: `execve` sigue siendo el último
renglón, y `pivot_root`, `chdir`, `umount2`, `rmdir`, `sethostname`, `setuid` y
`seccomp` no abren archivos. Las dos lecturas que quedan de ese lado —
`cgroup.procs` y el binario del módulo en `execve`— son justo para lo que existe
el piso.

**Lo que el contenedor comprueba y lo que no.** Las 24 pruebas de aislamiento
hacen el pivote completo aquí y siguen pasando, así que el reordenamiento no
rompió el lanzamiento. Que el `-EPERM` desapareció **sólo lo puede decir una
máquina que niegue**, y lo dice la etapa 36.

## Regla derivada: cuando el efecto no se puede reproducir, se mide el orden — 2026-08-26

El defecto de arriba —el LSM negándole a Thalyx confinar— tiene una propiedad
incómoda: **este contenedor no lo puede reproducir**. Sin LSM, `check()` devuelve
0 y las cinco aperturas de escritura pasan sin ruido. Cualquier prueba escrita
contra el *efecto* diría `NOT PROVEN` aquí y sólo hablaría en la máquina de
Cesar, o sea una vuelta completa por cada intento.

Pero el efecto no es la propiedad. La propiedad es **el orden**: toda escritura
de Thalyx tiene que ocurrir antes del renglón que entra al cgroup. Y el orden sí
se puede medir aquí, con `strace`, que además es la clase correcta de
instrumento — regla 5 — porque preguntarle a Thalyx si hizo las cosas en el orden
correcto pasaría en cualquier compilación donde el orden y la creencia sobre el
orden estén mal juntos, que es justo lo que pasó.

`nothing_is_opened_for_writing_after_the_launcher_takes_the_module_s_identity`:

1. Lanza un módulo real bajo `strace -f -y`. `-y` es lo que hace legible la
   traza: un `write` lleva un número de descriptor, y `-y` le pega la ruta con
   la que se abrió, así que el ingreso al cgroup se puede *encontrar*.
2. La ventana va del `write` a `cgroup.procs` hasta el `execve` del módulo.
3. **La afirmación**: en esa ventana no hay una sola apertura con `O_WRONLY` ni
   `O_RDWR` — leído igual que lo lee el kernel, `flags & O_ACCMODE`, para que lo
   que la prueba llama escritura sea lo que el kernel llama escritura y no una
   segunda opinión.
4. **La línea base**, regla 4: «no hay escrituras después» también es cierto de
   un lanzador que no escribió nada. Se pide en dos mitades a propósito, porque
   *«nunca creó el punto de montaje de `/dev/null`»* y *«lo creó del lado malo»*
   son hallazgos distintos: el primero dice que la traza no prueba nada, el
   segundo **es** el defecto.
5. **Falla cerrada**, regla 9: si `strace -f` parte una apertura en dos por
   interleaving, media llamada no dice para qué se abría, y una línea ilegible
   no es una línea que dijo que no.
6. **Regla 10**: si el `strace` de la máquina no tiene `-y`, la traza no lleva
   rutas y el ingreso al cgroup es invisible — que se ve idéntico a un lanzador
   que nunca entró a su cgroup. Eso es `NOT PROVEN` con su propia variable
   (`THALYX_REQUIRE_STRACE_TESTS`), no una acusación contra Thalyx. Es la tercera
   vez que un instrumento tiene versión en este proyecto.

Se comprobó revirtiendo el arreglo: con el orden viejo falla, y el mensaje cita
la línea de `openat` culpable.

> Cuando el efecto de un defecto sólo aparece en una máquina que no tienes,
> busca la **propiedad estructural** de la que el efecto se sigue. Casi siempre
> es un orden, y un orden se mide con un tracer en cualquier máquina.

### Y la columna que faltaba en `verify.sh`

El defecto duró lo que duró por una razón aparte, y también del arnés: **§36 era
la única etapa que armaba la máquina, y lo único que corre armada es un
invitado.** Las etapas que corren un módulo firmado —6, 12, las de aislamiento—
corren todas observando, donde toda apertura pasa. `correr` bajo un kernel que
niega de verdad no lo había ejecutado nunca nada.

La etapa **39** es esa columna: el mismo módulo, la misma concesión, el mismo
comando, una vez observando y una vez negando, en la misma etapa y no repartidas
en dos — repartirlas dejaría que la mitad que niega se salte en una máquina donde
la otra pasó, que es toda máquina donde esto ha corrido. Y nombra `Operation not
permitted` como su propio veredicto, para que si vuelve mande al lector a
`RootFs::assemble` y no al módulo.

## Regla derivada: un arreglo que cambia el mecanismo cambia lo que hay que medir — 2026-08-26

Mover el ingreso al cgroup de `enter` a `init` arregló el defecto y **cambió en
silencio otra cosa**: `init` es PID 1 de su propio espacio de nombres, así que
`std::process::id()` ahora vale 1, y 1 es lo que se escribe en `cgroup.procs`.
Antes era un pid del anfitrión, porque el ingreso ocurría antes del `unshare`.

Las dos cosas funcionan, **y funcionan por razones distintas.** La nueva funciona
sólo porque el kernel resuelve el número en el espacio de nombres de quien
escribe. Si no lo hiciera, escribir «1» en un cgroup del anfitrión metería **al
init de la máquina** dentro del cgroup del módulo, bajo la política del módulo —
un defecto que no se anuncia, que ninguna prueba desde adentro del sandbox podría
ver nunca, y que las 24 pruebas de aislamiento no habrían notado porque todas
preguntan desde adentro.

> Cuando un arreglo cambia **por qué** algo funciona, y no sólo si funciona, lo
> que hay que medir cambió con él. La pregunta no es «¿sigue pasando la suite?»
> — es «¿qué afirmación nueva estoy haciendo sin haberla escrito?».

`the_pid_the_launcher_writes_lands_the_right_task_in_the_cgroup` es esa columna:
lee `cgroup.procs` **desde fuera**, en el espacio de nombres del anfitrión, con
`std::fs` y no a través de Thalyx, y afirma que el pid 1, el proceso de prueba y
el lanzador exterior no están ahí; que lo que sí está lo corrobora `/proc`, que
es otra respuesta desde otro lugar; y que al menos uno pudo corroborarse, porque
una tarea que salió entre las dos lecturas no puede contestar y eso no es
contestar que no.

Dos cosas que la prueba tuvo mal antes de tenerlas bien, y las dos valen más que
la prueba:

- **Esperaba a que el módulo terminara y después le preguntaba a `/proc`** por
  pids que llevaban tres segundos muertos. Una prueba midiendo su propio orden en
  vez del del sujeto.
- **Exigía exactamente un miembro.** Son dos: el módulo es `sh` y `sleep` es su
  hijo. Un cgroup que no sostuviera a los hijos no estaría confinando gran cosa,
  así que la suposición estaba mal, no el código.

Se comprobó con una mutación: devolviendo el ingreso a `enter` —el orden viejo—
la prueba falla nombrando al lanzador exterior dentro del cgroup.

## Regla derivada: contar como falla lo que la máquina no puede hacer es el mismo error, en espejo — 2026-08-26

Encontrado corriendo `verify.sh` entero en el contenedor, que es algo que no se
había hecho después de editarlo ciento veinte veces. Salía con código 1 y la
línea *«Something Thalyx claims is not true on this machine»* por **una** falla:

```
FAILED  thalyx install did not finish; see .../install.log
```

Y el log decía, con las palabras de Thalyx:

> *the kernel read /dev/loop0's new partition table and made 0 partition(s) of
> the 2 that were written.*

O sea que Thalyx se negó a terminar una instalación cuyas particiones el kernel
nunca creó — falla cerrada, correcta, y explicada por él mismo. Los dispositivos
loop de este contenedor no soportan particiones. **No había nada que concluir
sobre Thalyx**, y el guion lo contó como una afirmación falsa.

Lo más incómodo: el control que lo distingue **ya existía en el mismo guion**, a
veinte renglones de distancia — una MBR simple sobre otro loop, que todo kernel
sabe parsear; si esa tampoco produce particiones, la máquina no puede y no hay
juicio posible sobre la GPT. Ese control lo consultaba **una** de las dos
comprobaciones de la etapa que dependen de él. La otra no. Así que un solo hecho
del entorno producía un `NOT PROVEN` y un `FAILED` a la vez, en la misma etapa.

> La regla 3 dice que no se cuenta como aprobado lo que la máquina no pudo
> comprobar. **La otra mitad no estaba escrita: tampoco se cuenta como
> reprobado.** Las dos convierten una propiedad de la máquina en una afirmación
> sobre el sujeto, y la segunda además gasta el tiempo de alguien persiguiendo
> un defecto que no existe.
>
> Y cuando un guion ya tiene el control que distingue las dos cosas, **úsalo en
> todas las comprobaciones que dependen de él**, no en la que se te ocurrió
> primero. Dos veredictos del mismo hecho que no coinciden es el arnés
> contradiciéndose delante del lector.

El sondeo se hace ahora una vez, antes de instalar nada, y las dos
comprobaciones leen la misma respuesta. Con eso `verify.sh` sale **80 `PROVEN`,
28 `NOT PROVEN`, 0 `FAILED`** y con código 0 en este contenedor, que es la
primera vez: hasta hoy «el guion sale limpio aquí» no era una señal que se
pudiera usar para nada.

## Regla derivada: `THALYX_ROOT` aísla la tienda y nada más — 2026-08-27

Encontrado por Cesar corriendo `verify.sh` en su máquina. Dos `FAILED`, y el
segundo era consecuencia del primero:

```
a_program_may_ask_the_machine_to_start_denying --- FAILED
  {"changed":false,"message":"the kernel guard is already enforcing",
   "mode":"enforcing","ok":true,"op":"deny"}
  left: Bool(true)   right: Bool(false)

── 6. a real module, installed and run confined
   FAILED  the machine was left enforcing before [6. a real module…]
```

`the_guard_can_be_switched.rs` está escrito, y lo dice en su propia
documentación, **contra una máquina que no tiene nada cargado**: sin BPF, `negar`
no puede cambiar nada, así que lo que se comprueba es el cableado — que el verbo
existe, que llega a `guard`, y que contesta como sí mismo. Lo que nunca se
comprobó es que la máquina *fuera* esa. Cada prueba abre su `THALYX_ROOT` en un
directorio temporal y da por hecho que eso la deja sola.

**No la deja sola.** `THALYX_ROOT` aísla la tienda. El guardián del kernel son
cuatro bytes en bpffs, al lado de `KernelStore::DEFAULT_MAP`, y ese mapa es de
la máquina que corre la suite — ninguna variable de entorno lo mueve. Así que en
la máquina de Cesar, como root y con `thalyx-lsm` enganchado, tres de esas
pruebas hicieron lo que `negar` hace: **armaron su kernel**. La que corrió
después leyó «already enforcing» y falló, y a partir de ahí todo lo que midió el
guion midió una máquina que nadie le había pedido.

> La regla 5 dice que el instrumento incluye al arnés. Le faltaba la otra
> mitad: **el arnés no es sólo lo que hace la pregunta, es también aquello a lo
> que se le hace.** Una prueba que escribe algo global de la máquina ya cambió
> la máquina que estaba midiendo, y la dejó cambiada para todo lo que corra
> después — que es peor, porque ese daño no aparece en la prueba que lo hizo.
>
> Lo que distingue el caso no es «toca la máquina»: un cgroup se crea y se
> borra y tiene dueño. Es **un interruptor global sin dueño**: uno solo, que
> nadie devuelve, y cuyo valor es la precondición de otra cosa.

Lo que quedó:

- Las tres pruebas que tecleaban `negar` o `deny` **preguntan primero**, antes de
  lanzar la sesión, y se saltan con `NOT PROVEN` si el guardián de esta máquina
  es real. La cuarta —la línea base, `…is_one_with_nothing_loaded`— se salta con
  ellas: una línea base que sobrevive a las pruebas que sostiene dejó de ser una
  línea base.
- Sin `THALYX_REQUIRE_*` al lado, y a propósito. Todos los demás saltos de este
  proyecto son una máquina que puede *menos* de lo que la prueba necesita, y la
  variable existe para que la que sí puede no se libre calladita. Éste es el
  espejo: la máquina puede *más*, y lo que falta no es una capacidad sino el
  kernel vacío del que hablan las tres. Una variable que convirtiera este salto
  en falla exigiría que la única máquina que importa dejara de poder enforcear.
- La pregunta se hace **como la hace `guard::set`** y al kernel: ese verbo
  escribe cuando el flag se lee y se niega sin escribir cuando no. Un
  `Path::exists` habría sido otra pregunta — bpffs es modo 700 y contesta
  «no está» de un mapa que sí está, que es el error que una vez hizo que las
  herramientas de este proyecto se leyeran desarmadas estando armadas.
- La decisión se separó de la lectura (`would_switch_this_machine`) para que algo
  la compruebe: la lectura necesita BPF y aquí no hay, pero un `Unreadable`
  contado como guardián real saltaría **todas** las pruebas del archivo en
  **todas** las máquinas, calladamente, y un salto que nadie pidió se ve igual
  que una máquina que no puede.
- Dos de las seis siguen corriendo en todas partes, y son las que hacen que el
  archivo siga probando algo en la máquina de Cesar: `observar` en cara
  estructurada se rechaza **antes** de leer el kernel —es un hecho sobre la
  petición, no sobre lo que esté clavado— así que ahí se teclea el verbo que
  desarma, en una máquina que sí se puede desarmar, y no se mueve nada. Y el
  ensayo lee y no escribe, en cualquier máquina.

El salto se comprobó **con una mutación**, porque este contenedor no tiene el
guardián que lo dispara: forzando `would_switch_this_machine` a `true`, las
cuatro pruebas se saltan e imprimen su renglón de `NOT PROVEN` —una cada una,
nombrando lo que no se probó— y ninguna lanza la sesión. La misma mutación mata
la prueba de la decisión, que es lo que impide que ese estado se quede puesto
sin que nadie lo note.

Y dos arreglos en el arnés, porque el veredicto apuntaba al lugar equivocado:

- `guard_check` corre al anunciar cada etapa, así que leyó lo que dejó la
  anterior y culpó a la **§6**, que no había hecho nada. Ahora recuerda la etapa
  previa y nombra el intervalo: *«left enforcing between [5. the test suite…]
  and [6. a real module…]»*.
- La §5 mide ahora, con `bpftool` y con una línea base tomada antes, que
  **la suite dejó el guardián donde lo encontró**. Era una precondición que el
  guion daba por hecha —la misma clase de hueco de 2026-08-26— y ahora es una
  afirmación con su renglón.

## Regla derivada: y la segunda vez el culpable no sabía que tocaba el guardián — 2026-08-27

La corrida siguiente, con el arreglo de arriba puesto, trajo **181 `PROVEN`, 2
`NOT PROVEN`, 1 `FAILED`** — y la falla era la medición nueva de la §5 haciendo
exactamente su trabajo:

```
FAILED  the suite moved the kernel guard from [0] to [1]:
        a test wrote to the machine it was measuring
```

Quedaba **otro** que armaba el kernel, y éste es el que enseña algo:
`catalogue_is_true.rs`, que **no es una prueba sobre el guardián**. Le pregunta
al binario qué verbos tiene y teclea cada nombre que le contesta, para que un
verbo anunciado que la sesión no entiende no pueda esconderse. En esa lista
viene `negar`. La prueba no lo sabía y no tenía por qué saberlo: recibió una
palabra y la tecleó.

Su lista de exclusiones tenía cinco nombres —`salir`, `exit`, `quit`, `apagar`,
`poweroff`— y una sola razón detrás: *«terminan la corrida»*. La otra razón para
no teclear algo no estaba escrita en ninguna parte.

> El arreglo de arriba trataba el problema como «la prueba del guardián arma el
> guardián», y era **la mitad**. El peligro no es una prueba que trata del
> interruptor: es **cualquier cosa que llegue al prompt**, porque el prompt
> tiene el interruptor. Una precondición que vive dentro del archivo que sabe
> del tema no protege al archivo que no sabe.

Por eso la precondición se mudó a `tests/machine_guard/mod.rs`, compartida, con
la regla escrita ahí; los dos archivos la usan y el tercero que la necesite la
tiene. Donde el guardián es real, los cuatro nombres del catálogo —`negar`,
`deny`, `observar`, `observe`— se dejan fuera del tecleo y se dice cuáles, con
su renglón de `NOT PROVEN`; donde no lo es, se teclean como todos los demás, que
es donde esa comprobación hace su trabajo de todos modos. Se comprobó con la
misma mutación.

### Y el disparador para el quinto

Excluir por una lista de palabras es un conjunto leído del lugar equivocado otra
vez. Lo que hace peligroso a un verbo aquí es que **actúa en cuanto se teclea**:
no lleva argumento, así que no hay «cuál» que lo detenga. Eso sí se puede leer
del catálogo —`changes` verdadero y `takes` vacío— y hoy son cuatro: `revertir`
y `apagar`, que se quedan dentro de `THALYX_ROOT` o terminan la corrida, y los
dos del guardián, que llegan a la máquina de abajo.

Cuál de las dos clases es cada uno **no** se puede leer del catálogo: `changes`
dice que un verbo es consecuente, no *de quién* es la máquina que cambia. Así
que ese conjunto de cuatro quedó clavado en una prueba que lo lee del binario en
vivo. Un quinto verbo que actúe desnudo la pone en rojo, y quien lo agregó decide
de qué clase es —con el mensaje de la falla diciéndoselo— en vez de enterarse
por el kernel de Cesar, que es como se encontraron los dos que hay.

## Cómo se miden pixeles, que no es una regla nueva sino dos viejas

Escrito el 2026-08-27, al construir [[La-Pantalla]]. No hay regla nueva aquí —
hay dos que ya existen cayendo en un sitio donde no era obvio que aplicaran, y
eso vale escribirlo porque la tentación en una pantalla es mirarla y decir que
se ve bien.

**Regla 5, el arnés también es un instrumento.** Un cuadro comprobado por el
código que lo dibujó prueba que es consistente consigo mismo y nada más. Por eso
la etapa 40 lee los pixeles con un decodificador de PNG escrito en el propio
`verify.sh` sobre `zlib` y `struct` — treinta renglones, ningún crate, y
ninguna línea que Thalyx haya escrito.

**Regla 4, línea base y control, sobre una propiedad que es global.** La
afirmación de la pantalla que es de seguridad no es *«la confirmación sale
roja»*: es **«ese rojo no lo usa nada más»**, y eso no se puede comprobar
mirando la confirmación. Hace falta el control —la pantalla ordinaria, contando
cero— y además el control del control, que es buscar el color del agente y
encontrarlo: sin ese tercero, un lector que no encuentra nada en ningún lado
hace pasar la línea del medio por la razón equivocada.

La forma general: **una propiedad que dice «y nada más» se prueba en lo demás,
no en la cosa.** Una prueba escrita sobre el sujeto no puede fallar por lo que
haya alrededor de él, que es exactamente de donde vendría el defecto.

## Regla derivada: los descriptores 0, 1 y 2 son del proceso, y una prueba que los mueve movió el proceso entero — 2026-08-28

La regla 11 otra vez, en un sitio donde nadie la había buscado. Las dos veces
anteriores el interruptor global sin dueño era el guardián del kernel; esta vez
es la **salida estándar**.

**Qué pasó.** Para que la pantalla pueda mostrar lo que contesta un verbo, lo que
el verbo imprime se atrapa moviendo los descriptores 0, 1 y 2 a un archivo en
memoria mientras corre. La prueba de eso vivía como módulo adentro de
`thalyx-cli` — y `cargo test` corre las pruebas de un binario como **hilos de un
mismo proceso**. Los descriptores no son del hilo: son del proceso. Sola pasaba;
con `--test-threads=1` pasaba; junto a las otras ciento treinta y cuatro
atrapaba los renglones de progreso de `libtest` en vez de lo que ella misma había
escrito.

**Lo que quedó.** El código se mudó a su propio crate, `thalyx-capture`, que le
da su propio proceso de pruebas, y las partes corren en orden dentro de una sola
prueba en vez de como cuatro que el planificador pueda cruzar.

**La forma general, que es la de la regla 11 y no una nueva:** *lo que no tiene
dueño no se aísla con una variable de entorno.* `THALYX_ROOT` aísla la tienda;
nada aísla los descriptores de un proceso, ni el guardián de un kernel. Cuando lo
que una prueba cambia es de esa clase, **la única separación real es un proceso
distinto** — y en Rust, un binario de pruebas distinto es un crate distinto.

**Y una segunda lección del mismo día, que no es una regla sino un recordatorio
de la 1:** el acomodo de la conversación colocaba un turno completo a la vez y
saltaba el que no cabía, así que la respuesta de `describe` —todos los verbos de
la máquina— dibujaba nada. Cuarenta y tres pruebas de la pantalla estaban en
verde y ninguna lo veía, porque todas usaban conversaciones que caben. Lo
encontró correr el sistema; la prueba se escribió después.

## Regla derivada: lo que se compila para verificar no es lo que arranca — 2026-08-28

**Qué pasó.** La corrida en hierro de la pantalla cerró en `185 · 4 · 0` — cero
fallas, todo lo que la máquina de Cesar podía comprobar comprobado. Lo siguiente
que él tecleó fue `make -C image image`, y no compiló: cinco peticiones de
`ioctl` de la pantalla estaban escritas `as libc::c_ulong`, que es el tipo que
toma `ioctl` contra glibc, y contra musl toma `c_int`.

**Por qué las 189 comprobaciones no vieron nada.** `verify.sh` compila contra
glibc, de principio a fin. La etapa 11 —la que se llama «la imagen»— arma un
initramfs **con ese binario de glibc** para contar cuántos programas lleva
adentro, que es la pregunta del decreto; nunca compiló para el objetivo de la
imagen. O sea que el único lugar del proyecto donde se compila lo que de verdad
arranca era un comando que sólo corre Cesar, a mano, después de que todo dijo
que estaba bien.

**La forma general.** *Un artefacto compilado con otra configuración no es el
artefacto.* Es la regla 8 —un sustituto tiene que modelar la propiedad que se
mide— aplicada al compilador en vez de a un doble de prueba: el binario de
glibc modela el comportamiento de Thalyx perfectamente y no modela lo único que
esta clase de defecto toca, que es el tipo de una llamada. Mientras el arnés no
construya **el binario que se embarca**, hay una clase entera de fallas que sólo
puede encontrar la persona que menos debería encontrarlas.

**Lo que quedó.** La etapa 2 corre ahora la línea exacta del `Makefile` de la
imagen —`cargo build --release --target x86_64-unknown-linux-musl -p
thalyx-cli`— con sus cuatro brazos ejercidos uno por uno: probada si compila;
`FAILED` con el error del compilador junto al veredicto si no; y `NOT PROVEN`,
nombrando el remedio, si a esta máquina le falta el objetivo de rustup o un
compilador de C para musl —regla del 2026-08-26, un límite de la máquina no es
una falla de Thalyx—. `THALYX_REQUIRE_IMAGE_BUILD=1` vuelve falla cualquiera de
esos dos saltos.

**Y una advertencia sobre el crate donde pasó.** `thalyx-syscall` ya tenía la
regla escrita, en el comentario de `BTRFS_IOC_SUBVOL_CREATE`: el número se
declara `u64` y se convierte en el sitio de la llamada con `as libc::Ioctl`,
que es el alias que ya vale en los dos objetivos. Estaba escrita, era correcta,
y la entrega de la pantalla no la siguió — una convención que vive sólo en un
comentario la obedece quien lo lee. Ahora hay una comprobación que la exige.

## Regla derivada: una prueba que no puede existir sin poner en riesgo la máquina vive en `verify.sh`, no en `cargo test` — 2026-08-28

Salió construyendo la costura de confirmaciones. Había que probar que
`instalar-en` pide la ruta del disco y **no** acepta un `sí`, y llegar a esa
pregunta exige nombrar un disco que el verbo acepte borrar.

La primera versión nombró un dispositivo inexistente y **pasó por vacío**:
`instalar-en` se niega ante un dispositivo que no abre *antes* de preguntar
nada, así que la prueba afirmaba que un `sí` no autorizó una pregunta que nunca
se hizo. Es la misma clase que ya está catalogada dos veces aquí, y sólo se
agarró porque se leyó la salida en vez del código de salida.

**El arreglo no es nombrar un disco mejor.** Apuntar un verbo que borra discos a
un disco de verdad y teclearle `sí` es una prueba que borra la máquina justo el
día en que lo que mide está roto — que es el único día en que corre distinto.
`THALYX_ROOT` no aísla un disco; nada lo aísla. Es la regla 11 otra vez, con la
consecuencia más cara posible.

Así que la afirmación vive en la etapa 42 de `verify.sh`, sobre un dispositivo
de bucle que el guion crea y destruye, donde el disco que se borra es un archivo
que el guion escribió. Y ahí sí se rompió en las dos direcciones antes de
creerle: la peligrosa (un `sí` que autoriza) y la del espejo (un confirmador que
niega todo, que sin la columna de control se ve igual que uno que funciona).

**Y el patrón que la agarró:** `grep 'that is not'` pegaba en *«That is not the
same as not looking»*, una frase de `discos` tres renglones arriba. Un marcador
tomado de una frase en prosa es un marcador que otra prosa cumple. El que quedó
es la oración que el confirmador mismo imprime y nada más.

## Regla derivada: una justificación que sobrevive a que la desmientan es un cuento — 2026-08-28

El comentario de `said_so_far` decía que leer el búfer con `seek` movería la
posición donde el verbo escribe, y que por eso se lee con `pread`. Se puso el
`seek` de vuelta para ver la prueba fallar —que es lo que este proyecto hace
antes de creerle a una prueba— y **no falló**: `read_to_end` deja el offset al
final, que es donde estaba.

La razón verdadera es más angosta y sigue decidiendo lo mismo: `pread` no puede
mover esa posición en absoluto, así que vale para una lectura parcial, para una
que se corta a la mitad por un error, y para quien después lea sólo la cola. El
comentario dice ésa.

Lo que se aprende no es sobre `pread`. Es que **romper una prueba a propósito
también prueba el comentario de al lado**, y que una explicación que no se puede
desmentir no es una explicación — es una historia que se cuenta el código a sí
mismo. Regla 5 apuntada al texto en vez de al instrumento.

## Regla derivada: el encoding de otro sistema se lee entero o no se lee — 2026-08-28

Las tablas de teclado del kernel guardan `K(tipo, valor)`, pero **sólo cuando el
byte alto llega a `0xf0`**; por debajo de eso la entrada es un punto de Unicode
tal cual. La primera versión de las pruebas leyó el tipo sin restar el `0xf0` y
concluyó que la distribución latinoamericana no tiene `ñ`.

La tabla estaba bien. El lector estaba mal. Se agarró porque la afirmación de al
lado era lo bastante específica para ser obviamente falsa —«la tecla 39 da
`ñ`»— y no porque algo fallara de forma visible: un lector que hubiera dicho «no
es una letra» para media tabla habría pasado cualquier prueba escrita en
términos de «tiene letras».

Dos consecuencias, y la segunda es la regla:

1. El decodificador vive en el módulo y no en la prueba. Dos lectores de un
   encoding es cómo llegan a no coincidir, y aquí el segundo lector iba a ser el
   verbo que le dice a una persona qué hace una tecla.
2. **Una afirmación sobre datos ajenos necesita su columna de control igual que
   una negativa.** «Esta distribución tiene `ñ`» pasa contra una tabla igual en
   todos lados. Lo que se afirma es la *diferencia*: la tecla que aquí da `ñ` es
   la que en el mapa compilado del kernel da `;`.

## Regla derivada: un patrón evalúa sus dos mitades antes de decidir cuál usa — 2026-08-28

El motor residente cargaba el modelo otra vez en cada frase, y la línea que lo
causaba compilaba, pasaba `clippy` y se lee correcta:

```rust
if let (false, Some(stale)) = (usable, held.take()) {
    stale.retire();
}
```

La tupla se construye **antes** de comparar con el patrón, así que `held.take()`
corría siempre. Cuando el motor sí servía —`usable == true`— el brazo no
entraba, `stale` nunca se ligaba, y el residente que acababa de sacarse del sitio
se destruía al final de la sentencia. La siguiente frase encontraba `None` y
arrancaba otro proceso con los pesos otra vez.

Tres cosas de esto, y la tercera es la regla:

1. **No hubo error visible.** Cada frase se contestó bien. Lo único que cambió
   fue el coste, que es exactamente lo que la fase entera existía para bajar.
2. **`RunningModule` no tenía `Drop`**, así que el proceso tirado seguía vivo:
   la máquina acumulaba un motor por frase, cada uno con el modelo cargado. Peor
   que el fallo que se estaba reparando. Ahora tiene uno, y mata el proceso.
3. **Lo que agarró el defecto fue contar procesos, no observar comportamiento.**
   `the_engine_stays_alive` hace que el motor de mentira anote su pid en un
   archivo al arrancar, y la afirmación es sobre cuántas líneas tiene ese
   archivo. Una prueba escrita como «la segunda frase también se contesta»
   habría pasado. Cuando lo que se afirma es *que algo caro no volvió a pasar*,
   la prueba tiene que contar las veces que pasó — no comprobar el resultado,
   que es el mismo de las dos formas.

## Regla de documentación

**Ninguna afirmación sobre atomicidad o rollback se documenta en la bóveda sin un test de nivel 2 que la respalde.**

Si una nota afirma que una operación es atómica y no existe el test que lo demuestra, la nota está describiendo una intención, no una propiedad — y debe decirlo.

## Por qué el nivel 2 importa más allá del código

El sandbox de Thalyx es de [[Sandbox-Ejecucion|implementación propia]], lo que significa que no hereda la auditoría acumulada de una herramienta de terceros. Los tests de inyección de fallos y las pruebas de aislamiento son lo que compensa esa exposición.

Y desde el lado de la investigación: un experimento que mata el proceso en el punto exacto donde la atomicidad podría romperse, y muestra que el invariante se sostiene, es exactamente la clase de resultado reproducible que sostiene un paper. Ver [[Estrategia-Carrera]].

## Relacionado
- [[Fase-Commit-Atomico]]
- [[Sandbox-Ejecucion]]
- [[Caso-Instalar-Modulo]]
- [[Criterio-de-Salida-Fase-1]]
- [[Notas-Tecnicas-Implementacion]]

## Regla derivada: una ruta nueva a la misma función encuentra lo que ninguna prueba de esa función podía — 2026-08-28

**Un verbo probado por su propia suite y probado a través de una segunda entrada
son dos comprobaciones distintas, y la segunda encuentra lo que la primera no
puede ver: los supuestos que la primera comparte con el verbo.**

El puente de agentes externos ([[Agentes-Externos]]) no reimplementa ningún
verbo. Compone una línea del vocabulario de la sesión y la manda por el **mismo**
`dispatch_asking` que recibe una tecla. Como lo que le llega son argumentos
estructurados y no una línea escrita por una persona, cada argumento va entre
comillas simples — POSIX, literal de punta a punta, para que un argumento no
pueda convertirse en un segundo verbo.

La primera vez que se corrió, el puente contestó:

```
/home/proyecto/'src/main.rs' is not there
```

`leer` **no partía la línea en palabras**. Tomaba el resto de la línea tal cual y
lo resolvía como una ruta. Lo mismo `ir`, `indexar`, `describe` e `intento`.
[[Palabras]] había decretado el entrecomillado el 2026-08-23 y se lo había dado a
`cp`, `mv` y `rm`; a `leer` no. Así que **un archivo con un espacio en el nombre
se podía listar, copiar y borrar, y no se podía leer.**

Ninguna prueba de `leer` podía encontrarlo, porque todas las escribió alguien que
sabía que `leer` toma el resto de la línea, y por eso ninguna le pasó comillas.
Es la regla del fixture inventado, movida un nivel: **una prueba escrita contra
una función comparte los supuestos de la función.** Una segunda ruta que llega
por otro lado no los comparte, y por eso los ve.

Y encontró un segundo caso del mismo tipo en el mismo minuto: `ensayo` parte su
primera palabra de la línea cruda, así que `ensayo 'rm' …` pide ensayar un verbo
llamado `'rm'` —que ninguna máquina tiene— y la respuesta se lee como un error
del agente cuando es de Thalyx. El puente compone ahora la línea de `ensayo`
recursivamente, con las reglas del verbo que envuelve.

**Lo que hay que hacer con esto.** Cuando se agregue una segunda entrada a algo
—una superficie, un canal, una API— la primera corrida contra ella vale más que
la suite entera de lo que envuelve. Correrla antes de escribir una sola prueba
nueva, y anotar lo que salga.

## Regla derivada: una respuesta correcta y estrecha se lee como una respuesta completa — 2026-08-28

`usan src/store.rs` contestaba con los dos archivos que escriben
`use crate::store::…`, y era **cierto**: eso es exactamente lo que el índice
sabía. Se le escapaba un tercero que llega al mismo código como
`server.store.persist()`, que `grep` sí encontró.

Ninguna prueba podía atrapar eso, y no por descuido: todas preguntaban *¿están
las filas que tiene que haber?* y todas pasaban. La pregunta que faltaba era
**¿qué palabra cree quien lee esto que acaba de recibir?** Un agente que pide
"dependencias" no está pidiendo la lista de imports; está pidiendo lo que se
rompería. La respuesta era angosta, la palabra era ancha, y la distancia entre
las dos es el error que el que pregunta comete después, lejos, sin manera de
saber que empezó aquí.

**Lo que hay que hacer con esto.** Cada vez que una primitiva se le ofrece a un
agente, la pregunta que hay que hacerle no es si contesta bien, sino **qué
concluiría de la respuesta alguien que no vio el código**. Si lo que concluiría
es más de lo que la respuesta sostiene, o el nombre está mal, o la respuesta
está incompleta, y hay que decidir cuál de las dos — nunca dejarlo así porque
técnicamente no miente.

## Regla derivada: un corpus con respuestas conocidas mide lo que un modelo no puede — 2026-08-28

Lo único que había para saber qué sabe el índice era darle una tarea a Claude y
leer qué pasó. Eso cuesta dinero, tarda minutos, varía entre corridas, y contesta
*cómo fue esa sesión* en vez de *qué sabe el índice*.

`crates/thalyx-graph/corpus/` son diez árboles chiquitos con la respuesta
correcta escrita al lado, sacada de leer el código y no de correrlo. Corre en
milisegundos y contesta con una tabla. Encontró tres defectos de precisión en
este mismo repositorio antes de que ningún modelo lo mirara.

Dos cosas lo hacen medir y no adornar:

1. **Las expectativas son igualdades, no "tiene que contener".** Un índice a
   nivel de símbolo falla devolviendo de más, no de menos, y un corpus que sólo
   buscara las filas que quiere pasaría igual de contento sobre uno que devuelve
   el árbol entero. Dos de los diez casos existen únicamente para ser contestados
   **angostamente**: un nombre declarado en dos archivos, donde lo correcto es
   negarse, y un nombre escrito en un comentario y en una cadena, donde lo
   correcto es ignorarlo.
2. **Los límites se declaran, no se esconden.** Un caso puede traer
   `known_limits`, y la prueba afirma que el límite **sigue siéndolo** — así que
   arreglarlo aparece tan fuerte como romper otra cosa —, imprime `NOT PROVEN`, y
   `THALYX_REQUIRE_FULL_CORPUS=1` lo convierte en falla. Regla 3, en un lugar
   donde era muy fácil dejar una advertencia vieja envejeciendo sola.

**Lo que hay que hacer con esto.** Antes de gastar un modelo midiendo algo,
preguntarse si lo que se quiere saber tiene una respuesta conocida y barata. Casi
siempre la tiene, y entonces el modelo se gasta en lo que sólo un modelo puede
contestar.

## Regla derivada: un escaneo de a un renglón no puede saber qué es código — 2026-08-28

Tres defectos, uno por cada forma en que el texto se pasa de renglón, y los tres
salieron de indexar **este** repositorio y leer las filas:

- `uapi_btrfs.h` figuraba como dependiente de `thalyx-parser` porque la palabra
  `definitions` aparece en un comentario `/* … */`.
- `thalyx-permd` figuraba igual porque un mensaje de `panic!` sigue en el renglón
  siguiente con una barra invertida, y ese renglón se escaneaba como código.
- `thalyx-graph/src/schema.rs` igual, porque el SQL vive en una cadena cruda
  `r#"…"#` de veinte renglones y adentro dice `from_path`.

Los tres existían desde que existe el índice, y ninguno se veía: una mención de
más en `buscar` es una fila que nadie mira. Se hicieron visibles el día que una
mención se convirtió en **una arista de dependencia**, que es un archivo al que
alguien va a ir.

El arreglo no fue tapar los tres, fue **un solo escaneo con estado** para las
tres entradas del parser, que antes hacían tres manejos de comentarios distintos.
Y ese escaneo tiene una asimetría que hay que respetar: una comilla doble sin
cerrar continúa al renglón siguiente, y una comilla simple **nunca**, porque en
Rust `&'a str` es un lifetime. Llevarla habría borrado el resto del archivo — una
pérdida de recall que nadie notaría y que a todos les dolería. Tiene su prueba,
`a_lifetime_does_not_swallow_the_rest_of_the_file`, y la del espejo:
`a_slash_star_inside_a_string_does_not_open_a_comment`.

**Lo que hay que hacer con esto.** Cuando una señal que era decorativa pasa a
tener consecuencias, sus falsos positivos viejos son defectos nuevos. Hay que ir
a buscarlos **corriendo la cosa sobre este repositorio y leyendo las filas**, no
sobre fixtures — los tres estaban en código real y ninguno en un fixture.

## Regla derivada: comprobar no es imponer, y las dos se ven idénticas hasta que hay dos clientes — 2026-08-29

Una auditoría encontró cuatro P0 el 2026-08-28. **Tres eran el mismo defecto**,
en tres subsistemas que nadie habría puesto en la misma frase:

| Dónde | La regla que decía | Cómo la comprobaba |
|---|---|---|
| `intento empezar` | un intento abierto a la vez | leer el registro, después escribirlo |
| la frontera del agente externo | nada fuera del espacio de trabajo | `canonicalize`, comparar, y después abrir el nombre otra vez |
| el desmontaje del sandbox | no retirar la política si queda alguien adentro | `is_empty()`, y después revocar |

Los tres tenían la regla correcta escrita, con su comentario explicando por qué
importaba. Los tres la **comprobaban**. Ninguno la **imponía**.

Y las dos cosas son indistinguibles con un solo cliente. Cada test que existía
pasaba, porque cada test hacía una cosa a la vez, que es exactamente el caso en
el que una comprobación y una imposición dan la misma respuesta.

**La regla.** Cuando una afirmación es sobre algo que puede cambiar entre la
comprobación y el uso —otro cliente, otro proceso, el sistema de archivos— la
prueba que vale es la que **fuerza el entrelazado**, y hay tres formas y sólo
tres:

1. **Dos clientes reales con una barrera.** Hilos, con descripciones de archivo
   separadas si lo que se prueba es `flock`. Sin la barrera el primero termina
   antes de que el segundo empiece y el test pasa en una máquina donde el
   defecto sigue ahí.
2. **Un adversario corriendo en paralelo**, para lo que no se puede sincronizar:
   un hilo que cambia un directorio por un symlink mientras el otro lee. De un
   solo lado —regla 7— porque una corrida donde el adversario nunca ganó no
   prueba nada: la afirmación es *cero* escapes, y al lado va el conteo de
   rechazos como control, sin el cual «no pasó nada» y «no hubo carrera» se ven
   igual.
3. **El estado intermedio sostenido a mano**, cuando el entrelazado es una
   ventana de milisegundos que no se puede provocar: dos confinamientos
   establecidos y nada adentro del cgroup es un estado que un fake sostiene
   indefinidamente y una máquina real sostiene un instante cada vez.

**Y cada arreglo se comprobó quitándolo.** Los cuatro tests adversariales de la
frontera fallan sin el anclaje; el de la carrera de `intento` falla cinco de
cinco corridas sin el candado; el del cgroup falla sin el contador. Un test de
concurrencia que nadie vio fallar no es evidencia de nada — pasa igual si mide
la propiedad y si no mide nada.

### El corolario para los oráculos

El cuarto P0 no era una carrera y es la misma familia: el veredicto del
benchmark reversible leía *«el agente cambió algo de verdad»* de que el nombre
nuevo apareciera en alguna llamada. Eso es una frase que escribe el agente. Un
`Grep` de ese nombre la satisface; un `Edit` que falló la satisface.

Es la regla 2 —preguntarle al sistema si funcionó no prueba nada— aplicada a un
instrumento en vez de a una máquina, y duele más ahí: **un instrumento falso
produce evidencia falsa**, y la evidencia falsa se cree. La forma de la regla
para un oráculo: cada propiedad del veredicto viene de un instrumento distinto
del sujeto, y una propiedad que no se pudo medir hace el veredicto *ausente*,
nunca `false` y nunca `true`.

## Regla derivada: un guardia se calibra sobre el repositorio, no sobre el ejemplo que lo motivó — 2026-08-28

La regla de las aristas de símbolo —*un nombre que exactamente un archivo del
árbol declara*— pasaba los diez casos del corpus a la primera. Corrida sobre este
repositorio, `thalyx-snapshot/src/lib.rs` tenía **41 dependientes**.

Los tres defectos que faltaban salieron uno tras otro de leer esas filas, y
ninguno se parecía al anterior:

1. `fn place`, `fn relative` — **privadas**. Ningún otro archivo puede
   nombrarlas, así que la arista no es improbable: es imposible. Faltaba
   preguntarle a cada lenguaje su propia regla de visibilidad.
2. `pub fn directory(&self)`, `pub fn subvolume(&self)` — públicas, únicas,
   alcanzables, y **palabras del idioma**. Todo archivo con un `for directory in
   …` figuraba como dependiente. Faltaba que un archivo que **ata** un nombre
   esté hablando de su propia atadura.
3. Un archivo con su propio `fn validate_name` privado, llamándolo, figuraba como
   dependiente del único crate con uno público — porque al exigir visibilidad, la
   declaración privada de al lado dejó de estar ahí para volver ambiguo el
   nombre. **Arreglar un guardia destapó al siguiente.**

Quedaron 19, de los cuales 17 son referencias reales entre crates que ningún
import podía resolver, y las dos que sobran están contadas.

Y el corpus hizo su trabajo del otro lado: las doce piezas pasaron enteras
mientras el mecanismo debajo cambiaba tres veces en una tarde. Eso es lo que una
red de regresión tiene que hacer y lo único que prueba que sirve.

**Lo que hay que hacer con esto.** Un guardia probado sólo contra los casos que
lo motivaron está probado contra su propio autor. El repositorio es el corpus
adversario que nadie escribió, es gratis, y hay que correrlo **y leer las filas**
— no contarlas. Los tres defectos estaban en la lista desde la primera corrida;
lo que faltaba era mirarla.

## Regla derivada: un testigo que el éxito de la tarea apaga no es un testigo — 2026-08-29

La corrida REVERSIBLE #1 se pagó, terminó, y el instrumento la reprobó dos veces
por razones que no tenían nada que ver con lo que hizo el agente. Es la
decimoquinta vez que el problema resulta ser el instrumento, y la primera en la
que el instrumento se equivocó **porque la tarea salió bien**.

### El testigo

La tarea reversible pide cinco cosas y la quinta es **dejar el árbol
exactamente como estaba**. Contra eso el arnés tenía una trampa conocida —un
agente que no hace nada restaura el árbol perfecto— y contra la trampa tenía un
testigo: los `mtime` del espacio de trabajo, caminados antes y después. Un
agente que cambió seis archivos y los devolvió deja seis `mtime` movidos; uno
que sólo leyó, no.

Salvo que el `mtime` es lo único de esa lista que **se puede volver a poner**.
`utimensat` existe. Un agente que guarda una copia con `cp -a` y restaura desde
ella devuelve el contenido y devuelve la fecha, y el testigo no ve nada. El
brazo A hizo seis llamadas `Edit`, terminó con el árbol restaurado, y el resumen
dijo `intermediate_state: false` — que es la frase «aquí nunca pasó nada», dicha
sobre una corrida que quizá hizo todo el trabajo.

Y lo peor no es el falso negativo: es que **el falso negativo y el verdadero son
el mismo renglón**. Seis `Edit` que fallaron todas, seis `Edit` que nadie
contestó y seis `Edit` que funcionaron y se deshicieron producen exactamente el
mismo `mutating_tool_calls: 6` con `files_touched_on_disk: 0`. El resumen no
distinguía cuatro hechos distintos:

1. **lo que el modelo pidió** — una llamada que sólo puede mutar, contada de la
   petición;
2. **lo que la herramienta contestó** — el `tool_result`, escrito por Claude
   Code después de escribir, no por el modelo;
3. **lo que vio algo fuera del agente** — que el espacio de trabajo tuvo otro
   estado;
4. **cómo quedó al final** — el árbol contra la línea base.

Los cuatro ahora son cuatro campos. Y el testigo son tres instrumentos, no uno,
porque cada uno se equivoca en una dirección distinta:

| testigo | prueba que sí | no puede probar que no | lo apaga |
| --- | --- | --- | --- |
| `mtime` | el archivo se escribió | — | una restauración con `cp -a` |
| `ctime` | el inodo cambió | — | nada en espacio de usuario |
| la respuesta de la herramienta | `Edit` escribió | `sed -i` dentro de `Bash` | nada: el stream ya se escribió |
| el contador del adaptador | hubo mutación por MCP | — | nada, y en el brazo B sí es de dos caras |

El `ctime` es la mitad que faltaba: **no hay llamada que lo ponga para atrás.**
Escribir lo mueve, `utimensat` lo mueve, `chmod` lo mueve, y restaurar la fecha
lo mueve otra vez. Sólo sirve cuando las dos caminatas son del **mismo** árbol —
un `cp -a` le da `ctime` nuevo a todo, y el brazo B se exporta con `cp -a`— así
que la caminata escribe de qué raíz salió y la comparación usa el `ctime` nada
más cuando las dos raíces coinciden.

La respuesta de la herramienta es la que sirve **hacia atrás**: está en el
stream de una corrida que ya terminó, y ninguna restauración posterior la
alcanza. Es la que permitió arreglar el grader sin volver a pagar la corrida.

### La frontera

El otro falso negativo de la misma corrida. El brazo B reportó una sola
diferencia entre el árbol del que salió y el que volvió:

```
-s140000  0755  -  -  image/build/agent.sock
```

Es el socket que **QEMU** abre para que `thalyx-mcp` hable con la máquina.
Existe en el anfitrión porque el banco está corriendo; no está en la copia del
store porque no existía cuando `project-stage` empaquetó el proyecto. Ningún
agente pudo crearlo y ninguno pudo borrarlo.

La regla: **el árbol lógico de una medición no incluye la maquinaria que
transporta la medición.** `image/build` es lo que construye `make -C image` —el
kernel, el initramfs, el disco del store y ese socket— y `.gitignore` lo dice
desde antes de que el banco existiera. Estaba fuera de la lista de exclusiones
por descuido, no por criterio.

Y la parte que la hace segura, porque una exclusión es un lugar donde esconder
cosas: **excluido no es no medido.** `set_aside()` recorre cada raíz de
maquinaria y reporta cuántas entradas tiene y un digest de sus tipos, modos y
tamaños, de los dos lados, dentro del resumen. Lo que cambia ahí sigue en el
registro; lo único que deja de hacer es decidir `restored`. Las cuatro pruebas
que lo fijan están en `dev/bench-summary.py --self-test`: que el socket aparezca
y la restauración siga en pie, y que un archivo real cambiado **al lado** del
socket, un archivo no listado en cualquier parte, y un modo, un enlace o un byte
movidos sigan reprobando.

### Lo que hay que hacer con esto

1. **Un testigo que la tarea correcta puede apagar necesita un segundo testigo
   que no.** No es redundancia: son instrumentos con debilidades distintas, y
   la regla 5 dice que si se contradicen hay que escribir la contradicción, no
   promediarla. `witnesses_disagree` la escribe.
2. **«Lo pidió» no es «lo hizo».** Contar peticiones y llamarlas mutaciones es
   el mismo error que contar pruebas y llamarlas verdad.
3. **La evidencia que se puede releer vale más que la que hay que volver a
   producir.** El manifiesto en renglones estaba en disco al lado del digest, y
   por eso una corrida caminada bajo una frontera equivocada se pudo volver a
   leer bajo la correcta sin caminar nada otra vez. Un arnés que sólo hubiera
   guardado el digest habría tenido que pagar otra corrida para averiguar lo que
   ya sabía.
4. **Una exclusión se reporta o es un escondite.**

## Regla derivada: dos números con el mismo nombre no son la misma medida — 2026-08-29

La misma corrida reportó `turns: 37` bajo `--max-turns 30`, y nadie podía decir
si la habían cortado. La respuesta no es que el límite no funcione: es que
**cuentan cosas distintas.**

- `turns` es el `num_turns` del propio Claude Code, y ahí va el número de
  **mensajes de usuario** de la conversación: el prompt más uno por cada tanda
  de resultados de herramienta. En la sesión capturada de `dev/samples/` —una
  sola llamada `Read`— hay un mensaje de usuario y `num_turns: 2`.
- `--max-turns` acota el contador del ciclo agéntico: los **viajes de ida y
  vuelta a la API**, o sea los mensajes del asistente que pidieron herramientas.
  El ciclo se detiene cuando `turnCount + 1 > max_turns`.

Son iguales mientras el modelo pida una herramienta a la vez. En cuanto pide dos
en un mismo mensaje, un viaje produce dos resultados: `turns` sube dos y el
contador acotado sube uno. Por eso `turns` puede pasar de `--max-turns` sin que
el límite haya estado cerca, y **`turns > max_turns` no es evidencia de una
corrida cortada.** Lo que sí lo es: el `stop_reason` de la corrida y su propio
`is_error`.

`dev/bench-summary.py` ya no tiene un solo número ambiguo. Deja `turns` tal como
lo imprimió el agente —regla 10— y pone al lado los tres que puede contar solo:
`assistant_messages`, `assistant_messages_with_a_tool_use` (el que `--max-turns`
acota) y `most_tool_calls_in_one_message`. `turns_mean` lo dice dentro del
resumen, y el `--self-test` lo fija contra la sesión capturada, para que un
cambio en lo que Claude Code pone en `num_turns` se vea como una falla y no como
un número que nadie puede nombrar.

## Regla derivada: la regresión que sale de un banco no se escribe sobre el caso del banco — 2026-08-29

REVERSIBLE #1 salió válido y mixto, y nombró una causa: el brazo B necesitó
dieciséis mutaciones donde el otro necesitó seis, porque sólo sabía direccionar
líneas. De ahí salió `sustituir`, y con la operación hay que escribir la
regresión que la sostiene. **La tentación es escribirla sobre `UidRegistry`**,
que es el símbolo del banco, con la verdad conocida ya escrita y el árbol ya
elegido. Es exactamente lo que no se vale.

Una regresión escrita sobre el caso del banco mide dos cosas a la vez y no
distingue entre ellas: si la operación es general, y si el banco pasa. La
segunda es la única que se puede afirmar cuando coinciden, y es la que no vale
nada — un banco que el producto puede ver mientras se construye deja de ser una
medición y se vuelve un examen con las respuestas al lado.

Así que la regla tiene dos mitades:

1. **La regresión reproduce la *forma*, no el caso.**
   `a_mechanical_rename_costs_one_call.rs` arma su propio proyecto de dos
   crates, con un nombre que no es el del banco (`SlotTable`), y comprueba lo
   que la forma exige: una definición con varias menciones de su propio nombre,
   dependientes en un segundo directorio, un cambio mecánico, y el árbol
   devuelto. Si la operación sólo funcionara para `UidRegistry`, esta prueba
   falla.
2. **El arnés se congela mientras se construye contra él.** Ni el prompt, ni el
   símbolo, ni la verdad conocida, ni el grader se tocan. Adaptar la prueba al
   producto invalida las dos corridas —la anterior y la siguiente— y no queda
   forma de saber cuál de los dos cambios movió el número.

Y el corolario, que es de escritura y no de código: **hasta que el banco se
vuelva a correr, ninguna nota puede decir que la operación lo mejora.** Lo que
sí se puede escribir es lo que se observó, lo que se supone y lo que se
construyó, separados y en ese orden. La corrida siguiente es la prueba de la
hipótesis, no su confirmación.

---

## Regla derivada: un directorio de trabajo no es una frontera — 2026-08-29

El arnés del banco recibió `--project /tmp/bench-thalyx` y el brazo A ejecutó
`cd /home/cesarmanzocode/thalyx`. Nadie lo notó durante meses porque **nada lo
preguntaba**: el arnés arrancaba `claude` con `cd "$cwd"` y consideraba el
asunto resuelto.

La causa es una sola línea y es de las que se leen sin verlas. `--out` valía por
omisión `$ROOT/target/bench-external-agent`, `$ROOT` es el checkout donde vive el
script, y la copia del brazo A se hacía en `$OUT/a`. O sea: **el espacio de
trabajo del experimento vivía dentro del repositorio del experimentador.** Y
Claude Code recoge el `CLAUDE.md` de cada ancestro de su directorio de trabajo,
así que el agente empezó con las instrucciones de este proyecto —«lee esto antes
que nada», y una ruta al vault— en el contexto, y se fue a trabajar al árbol del
que hablaban esas instrucciones. Se comportó exactamente como se le pidió; lo
que se le pidió no era lo que el arnés creía.

**La regla:** poner un proceso en un directorio es una petición, no un
confinamiento. Un confinamiento tiene cuatro partes y hacen falta las cuatro:

1. **nada arriba** — se revisa cada ancestro del espacio de trabajo por
   `CLAUDE.md`, `.claude/`, `.mcp.json` y `.git`, y se escenifica fuera del
   checkout;
2. **arrancar adentro** — el proceso empieza físicamente ahí;
3. **negarse en vivo** — un hook rechaza cualquier llamada que nombre una ruta
   de afuera;
4. **comprobarlo después** — del stream propio de la corrida se leen el
   directorio en que arrancó (`system init`) y todas las rutas de todas las
   llamadas.

Las cuatro están aparte a propósito, y la cuarta es la única que es **evidencia**:
no necesita que ninguna de las otras tres haya funcionado. Las tres primeras se
pueden romper en silencio —un hook que el CLI ignoró, un ancestro que apareció
después—, y una defensa que falla en silencio es peor que ninguna. La regla
general que eso instancia ya estaba escrita —«el arnés también es un instrumento
y también miente»—; lo que faltaba es que **el control tiene que dejar rastro
que se lea después, no sólo actuar antes**.

## Regla derivada: el nombre de una herramienta es una intención, nunca un efecto — 2026-08-29

En la misma revisión, la tabla forense imprimía, para una llamada cuyo comando
era `git checkout -- <archivo>`:

```
Bash  write=False
```

Ninguna línea del código afirmaba que ese `False` significara «se demostró que
no escribió». No hacía falta: **un campo de dos valores no tiene forma de decir
"no sé"**, así que la respuesta que da para lo que no puede ver es la misma que
da para lo que sí comprobó. Y lo que no podía ver era la mutación más
consecuente de toda la tarea, que es restaurar archivos.

La salida no es un parser de shell. Un parser que reconociera todo comando
mutante —`sed -i`, `>`, `git checkout`, `install`, `mv`, un `python -c`, un
`make`— no se puede escribir bien, y uno casi correcto es peor que ninguno:
contestaría `False` con seguridad para lo que no se le ocurrió.

La salida es separar dos preguntas que estaban en un solo campo:

- **intención**, que se lee del nombre de la herramienta y tiene **tres**
  respuestas: `writes`, `reads`, `unknown`;
- **efecto**, del que la autoridad es el testigo del sistema de archivos, que
  ningún nombre puede esquivar.

Y el corolario que hace que la regla muerda: **un testigo que no vio nada, con
llamadas de clase `unknown` en el stream, no es "no escribió" sino
`not_proven`**. Con su control al lado —una corrida cuyas llamadas son todas de
herramientas que sólo pueden leer sí puede decir que no escribió— porque si no,
la regla convierte cualquier negativa en indemostrable, que es la otra manera de
no medir nada.

## Regla derivada: lo caro se comprueba antes de gastar lo caro — 2026-08-29

La misma corrida pagó el brazo A entero y **después** el brazo B devolvió `0s` de
reloj y cero eventos. El único control sobre el brazo B era

```sh
[ -S "$SOCKET" ]
```

que pregunta si existe un **archivo**. QEMU crea ese archivo en el instante en
que arranca y lo mantiene abierto conteste o no conteste nadie adentro del
huésped. Un socket presente es necesario y no se parece a suficiente.

**La regla:** una condición necesaria comprobada no es la condición. Cuando la
otra mitad de un experimento cuesta dinero, se comprueba **el canal real** —el
hello, una petición mínima que tenga que contestar bien, y que el espacio de
trabajo sea el que se importó— antes de gastar la primera mitad. La sonda tiene
que ser de sólo lectura, porque una que escribiera habría cambiado el estado
inicial del experimento que estaba despejando.

Y el orden es parte de la regla: nada de lo que el brazo B necesita para estar
vivo depende de que el brazo A haya corrido, así que no hay ninguna razón para
enterarse en ese orden — y hay una razón cara para no hacerlo.

---

## Regla derivada: dos cosas con el mismo nombre en espacios de nombres distintos no se comparan — 2026-08-29

El grader del banco reportó el brazo B como `VIOLATED` —salió de su espacio de
trabajo— en una corrida donde no había salido de ningún lado. Comparaba

```
/home/bench-thalyx                  el espacio de trabajo, adentro de la máquina
…/bench-external-agent-3/b          el directorio donde arrancó el proceso claude
```

Los dos son rutas, los dos se ven como rutas, y `os.path.normpath` los compara
felizmente. Uno está adentro de una imagen Btrfs que QEMU tiene abierta y el
otro está en el anfitrión. **Ninguna comparación entre ellos significa nada**, y
la que se estaba haciendo producía una acusación falsa contra el sistema medido.

Lo que hizo posible el error fue que las dos cosas se llamaban `workspace`. Una
sola palabra para el directorio donde vive un proceso y para el árbol sobre el
que trabaja: mientras los dos brazos fueron iguales —el brazo A los tiene
juntos— la palabra alcanzaba, y el día que dejaron de serlo el código siguió
comparándolos porque el nombre decía que eran lo mismo.

**La regla:** cuando dos cosas que un programa compara pueden vivir en espacios
de nombres distintos —dos máquinas, dos contenedores, un anfitrión y un
huésped—, se les ponen **nombres distintos en el código**, y cada comprobación
dice cuál de los dos está mirando. Un `if a == b` entre dos cosas que se llaman
igual es la comparación que nadie va a leer dos veces.

Y el corolario, que es de dónde salió el arreglo: cuando dos casos se juzgan bajo
reglas distintas, la regla se elige **por escrito** —un campo en la procedencia,
un modelo nombrado— y no por una condición implícita. La función que decidía
seguía siendo una sola porque nadie se había preguntado si las dos preguntas eran
la misma.

---

## Regla derivada: un barrido que sólo mira cadenas no ve una lista — 2026-08-29

Buscando el error de arriba apareció otro, más viejo y peor. El mismo grader
recogía las rutas que nombraba cada llamada así:

```python
PATH_FIELDS = ("file_path", "path", "notebook_path", …)   # `paths` no estaba
…
for key, value in given.items():
    if key in PATH_FIELDS or not isinstance(value, str):
        continue                                          # y esto salta las listas
```

`thalyx_edit` nombra sus archivos en `paths`, una **lista**. El campo no estaba
en la tabla, y el barrido de respaldo —el que existe justamente para cubrir un
campo que nadie listó— sólo mira valores que sean cadenas. Así que una llamada
que nombrara seis rutas absolutas fuera del espacio de trabajo eran **seis rutas
que nadie revisaba**, en el archivo cuyo trabajo entero es revisarlas.

Lo cachó la **columna de control** de su propia prueba: un caso que esperaba
`refused: 1` contestó `0`, y el veredicto que se estaba probando —`INTACT`— salía
correcto por la razón equivocada. Sin ese renglón el hueco seguía ahí.

**La regla:** una comprobación que recorre datos de forma genérica tiene que
decir qué hace con **cada forma** que esos datos pueden tener —cadena, lista,
objeto anidado— y una forma que no maneja es una forma que deja pasar en
silencio. Un `isinstance(value, str)` en un barrido de seguridad es una lista
blanca de tipos escrita sin querer.

Y el corolario para las pruebas: **la aserción que descubre esto no es la del
caso que falla, es la del caso que pasa.** Un caso positivo que llega al
veredicto correcto por el camino equivocado se ve idéntico a uno que funciona;
lo que los separa es afirmar también *qué vio* la comprobación, y no sólo qué
contestó.

---

## Regla derivada: un control que mide la carga de la máquina no es un control — 2026-08-29

`reading_through_a_component_being_swapped_never_leaves_the_workspace` falló una
vez, en una corrida de todo el workspace que compartía cuatro núcleos con un
`git push`, y pasó en las diez corridas de antes y de después. La afirmación de
seguridad de esa prueba es **cero** escapes y es de un solo lado: se cumple sin
importar cómo se agendaron los hilos. Sus dos controles no lo son:

```rust
assert!(refusals > 0, "…el swap nunca ocurrió y esto no midió nada");
assert!(answers  > 0, "…una frontera que rechaza todo pasaría esto");
```

Miden que el hilo que hace el swap ganó al menos una carrera y perdió al menos
una. Corriendo sola, la prueba deja un margen enorme —del orden de 2 600
respuestas contra 1 400 rechazos de 4 000—, así que un cero no es una carrera
apretada que se perdió: es un hilo que nunca se agendó.

Y ahí está el daño. Un control que falla cuando la máquina está ocupada **reporta
«la frontera se fugó» por un hecho sobre el promedio de carga**, que es el
mensaje más engañoso que ese archivo puede dar. La prueba no encontró nada; no
pudo buscar.

**La regla:** una prueba que necesita que algo concurrente ocurra para medir lo
que mide tiene dos afirmaciones distintas y no se pueden tratar igual. La de
seguridad corre siempre y es dura. La de *haber podido medir* es un **skip que
lo dice** —regla 3—, con una variable de entorno para ese requisito y nada más,
que la convierte en falla en la máquina que sí está quieta. `verify.sh` pone
`THALYX_REQUIRE_RACE_TESTS=1`; en un contenedor compartido, la prueba imprime
`NOT PROVEN` y no miente en ninguna de las dos direcciones.

Es la regla 7 —escoger el umbral del lado al que el ruido ambiental no llega—
aplicada a la mitad de la prueba que sí lo tiene: la afirmación es inmune al
ruido, el control no, y la diferencia se escribe en el código en vez de
esperarse.

---

## Regla derivada: un nombre hecho de un pid es un nombre por proceso, no por prueba — 2026-08-29

Dos pruebas de `an_attempt_can_be_taken_back.rs` fallaron en la máquina de
Cesar, y ninguna de las dos fallas era sobre `intento`. El ayudante que les
consigue dónde trabajar decía:

```rust
let subvolume = Path::new(&base).join(format!("thalyx-substitute-{}", std::process::id()));
let _ = Command::new("btrfs").args(["subvolume", "delete"]).arg(&subvolume).output();
let made = Command::new("btrfs").args(["subvolume", "create"]).arg(&subvolume).output().ok()?;
```

`cargo test` corre **todas las pruebas de un archivo como hilos de un mismo
proceso**, así que `process::id()` es el mismo número para las dos: un solo
nombre, un solo subvolumen, y una función que empieza **borrándolo**. La que
llegaba segunda destruía el árbol que la primera estaba midiendo y después no
podía crear lo que ya existía —y reportaba «no se pudo hacer un subvolumen
Btrfs» en una máquina que sí puede. `THALYX_REQUIRE_BTRFS_TESTS=1` convertía eso
en una falla de Thalyx.

Es la regla 11 en un sitio donde nadie la había buscado. `THALYX_ROOT` aísla el
almacén; un subvolumen de scratch no lo aísla nada, y el pid —que se siente
único— es exactamente el discriminador que no distingue lo que hay que
distinguir. Lo mismo estaba en `natively.rs`, donde **cuatro** pruebas se
repartían un nombre.

**La regla:** un recurso compartido que una prueba crea y borra se nombra por
**la prueba**, no por el proceso. Un pid separa binarios de test, que ya estaban
separados; no separa lo único que corre junto.

---

## Regla derivada: una expectativa que sólo corre en otra máquina es un fixture que nadie ejecutó — 2026-08-29

Las mismas dos pruebas, una vez que cada una tuvo su subvolumen, seguían mal —y
por escrito, desde el día en que se escribieron:

- La de `sustituir` afirmaba `replacements == 4` sobre un fixture con **tres**
  apariciones de `SlotTable`. Aritmética a mano, nunca corrida.
- La de `sustituir-lote` armaba la línea nombrando el archivo una sola vez y
  dejando que las operaciones 2 y 3 lo heredaran. La gramática no funciona así:
  **sólo la primera operación toma prestado el archivo de antes del subverbo**,
  y las demás listan los suyos. La máquina leía `'(SlotTable, usize)'` como el
  nombre de un archivo y rechazaba el lote entero con `bad_batch`.

Ninguna de las dos es sutil: cualquiera de las dos se cae en el primer segundo
de la primera ejecución. Lo que las mantuvo vivas es que **este contenedor no
tiene Btrfs**, así que las dos pruebas siempre se saltaron aquí, y en la única
máquina donde corren la falla del subvolumen las detenía antes.

Es la regla 6 —un fixture inventado prueba lo que yo entendí, no lo que la
herramienta imprime— aplicada a la propia máquina: cuando una prueba no puede
correr donde se escribe, sus números tienen que salir de **haberle preguntado al
sistema en algún lado**. Los dos de arriba salieron de correr `editar … sustituir`
y `editar … sustituir-lote` sobre un directorio común, sin `intento`, que es la
mitad de la prueba que este contenedor sí puede hacer.

**La regla:** una prueba que sólo corre en otra máquina se escribe partida —lo
que se puede ejercer aquí se ejerce aquí, y lo que se manda a la otra máquina es
sólo la parte que necesita esa máquina—, o sus constantes se sacan de una
corrida y no de la cabeza. Un `NOT PROVEN` esconde tanto un hueco de la máquina
como un error del autor, y los dos se ven igual hasta el día que alguien la corre.

---

## Regla derivada: un auto-test que se llama a sí mismo hereda el PATH de quien lo llamó — 2026-08-29

`dev/bench-external-agent.sh --self-test` daba `PROVEN` en el contenedor y
fallaba dos comprobaciones en la máquina de Cesar:

```
FAILED  the run failed without saying the workspace's ancestry was why
FAILED  a dead arm B was found out after arm A had already been paid for
```

Las dos se corren volviendo a invocar el propio script con `--arms`, y el script
—correctamente— se niega temprano si no hay `claude` en el PATH. `verify.sh`
corre bajo `sudo`; el PATH de root no tiene `claude`. Así que las dos sub-corridas
morían con `no claude on this host` **antes** de llegar a la negativa que era el
objeto de la prueba, y el `grep` que buscaba esa negativa no la encontraba.

Rule 5, otra vez, y con una marca reconocible: eran las **dos únicas**
sub-invocaciones del archivo sin un sustituto de `claude` en el PATH, justamente
porque son las dos que no deben llegar a llamarlo. No necesitarlo se convirtió en
depender de él.

El arreglo es el sustituto que las demás ya tenían, con una vuelta: este `claude`
**escribe un archivo si alguien lo llama**. La afirmación era «no se arrancó
ningún agente» y se estaba leyendo de un `armA.ndjson` vacío, que está vacío por
varias razones; ahora hay un testigo que sólo existe si pasó lo que no debía
pasar —y su control es que ese mismo sustituto, puesto en una corrida que sí
llega al agente, escribe el archivo.

**La regla:** una comprobación que ejecuta el programa bajo prueba en un
subproceso tiene que darle **el entorno completo que necesita para llegar a lo
que se le está preguntando**, incluso lo que no va a usar. Si no, mide el PATH de
quien corrió el arnés; y el arnés se corre bajo `sudo`, que es otro PATH que el
del autor.

---

## Regla derivada: que otra herramienta falle no es una propiedad del objeto — 2026-08-29

La corrida real de Fedora sobre `cffb4f8` dejó un solo `FAILED`, en
`crates/thalyx-snapshot/tests/natively.rs`:

```
an ordinary recursive remove took the subvolume away, so this test is not about
a subvolume
```

La copia escribible que hace `restore` **sí era un subvolumen**, y no hace falta
creerle a nadie para saberlo: tres renglones antes, en la misma prueba,
`flags(&copy)` había llamado a `BTRFS_IOC_SUBVOL_GETFLAGS` y el kernel había
contestado — ese ioctl responde `EINVAL` en todo inodo que no sea la raíz de un
subvolumen, así que la prueba nunca habría llegado a la línea que reventó si el
objeto hubiera sido un directorio ordinario.

Lo que estaba mal era la premisa. La comprobación decía *si esto fuera un
subvolumen, `remove_dir_all` no podría llevárselo*, y eso dejó de ser cierto en
Linux 4.18: el commit `a79a464d5675` («btrfs: Allow rmdir(2) to delete an empty
subvolume») hace que `rmdir(2)` se lleve un subvolumen **vacío** como a cualquier
otro directorio. `remove_dir_all` desenlaza `notes.txt`, el subvolumen se queda
vacío, y el `rmdir` final funciona. En la máquina de Cesar —kernel 7.0— eso pasa
siempre; en este contenedor no hay Btrfs, así que la prueba se saltaba y la
premisa nunca se ejerció.

Es la misma forma que `chrt --other` midiendo la versión de util-linux: la
comprobación no medía el objeto, medía **la política de `rmdir` del kernel que la
corría**. Y como la premisa era «esto falla», el día que el kernel dejó de fallar
la prueba acusó al objeto.

**La regla:** *que otra herramienta se niegue* no es una propiedad de la cosa que
se está midiendo, es una propiedad de esa herramienta en esa versión. Cuando la
afirmación es «esto es un X», hay que preguntárselo a una fuente que hable de X —
y que no sea la misma que usa el código bajo prueba, o se estaría graduando una
respuesta contra sí misma.

Lo que reemplazó a la comprobación es `stat(2)`: en Btrfs la raíz de todo
subvolumen es el inodo `BTRFS_FIRST_FREE_OBJECTID` (256), y el kernel le da a
cada subvolumen su propio dispositivo anónimo, así que su `st_dev` no es el del
directorio que lo contiene. Es el par que mira `btrfs_util_is_subvolume` de
`libbtrfsutil`, no un invento de aquí, y no pasa por el ioctl que usa
`Native::is_subvolume`. Con su control negativo al lado —un directorio ordinario
hecho en ese mismo directorio, que tiene que fallar las dos mitades— y la segunda
opinión de `btrfs subvolume show` donde haya btrfs-progs.

Y de paso, la misma frase falsa estaba en el doc de `BTRFS_IOC_SNAP_DESTROY`
(«un subvolumen no es un directorio y `rmdir` no se lo lleva»). La razón real por
la que ese ioctl es necesario es más angosta y sigue siendo cierta: se lleva un
subvolumen **poblado** de una sola operación, donde `std::fs` tendría que
caminarlo desenlazando archivo por archivo y no podría con un subvolumen anidado
adentro.

---

## Regla derivada: un nombre único no aísla lo que el producto deriva del padre — 2026-08-29

La regla de arriba —«un recurso compartido que una prueba crea y borra se nombra
por la prueba, no por el proceso»— se aplicó a `natively.rs` el mismo día, y no
alcanzó. Cada una de las cuatro pruebas pasó a hacer su subvolumen con un nombre
propio:

```
THALYX_BTRFS_SCRATCH/thalyx-native-<etiqueta>-<pid>
```

y `restoring_makes_a_writable_copy_and_deleting_takes_it_away_again` siguió
fallando en paralelo y pasando con `--test-threads=1`. El control que lo cerró
fue exactamente ese par: en serie, cuatro pasan en 0.06 s, y `btrfs` estuvo de
acuerdo con el kernel en todo lo que se le preguntó. **La lógica Btrfs
funcionaba**; lo que fallaba era el fixture.

La razón es que el nombre del subvolumen no es el único recurso que la prueba
usa. `Snapshots::directory()` no pregunta cómo se llama la fuente: la pone
**junto a ella**, en el padre.

```rust
self.subvolume.parent().unwrap_or(Path::new("/")).join(SNAPSHOT_DIR)
```

Cuatro fuentes con nombres distintos hechas en el mismo directorio tienen **el
mismo padre**, así que las cuatro escribían en un solo
`THALYX_BTRFS_SCRATCH/.thalyx-snapshots`, tomaban snapshots bajo nombres que
chocaban, dejaban ahí sus copias escribibles, y cada `clean()` se llevaba ese
directorio entero mientras las otras seguían trabajando adentro. Y lo mismo
cruzaba binarios: `taking.rs` terminaba con `remove_dir_all` sobre ese mismo
directorio, y las pruebas de `intento` en `thalyx-cli` snapshotean ahí — y
`cargo test` corre los binarios de prueba **a la vez**.

**La regla:** aislar una prueba es aislar *el recurso que el código bajo prueba
va a tocar*, no el que la prueba nombra. Cuando el producto deriva una ruta de la
que se le dio —el padre, un directorio hermano, un nombre fijo adentro— el
aislamiento tiene que estar **un nivel más arriba**, o el nombre único es una
etiqueta sobre un recurso compartido. La pregunta que hay que hacerse no es «¿mi
nombre es único?» sino «¿qué rutas va a calcular el producto a partir de éste, y
las comparte alguien?».

La forma que quedó es una arena por prueba: un directorio ordinario privado, con
la fuente adentro, de modo que `directory()` caiga adentro también.

```
THALYX_BTRFS_SCRATCH/thalyx-native-<etiqueta>-<pid>-<n>/
    source
    .thalyx-snapshots/
```

Tres cosas la sostienen, y ninguna es serializar —el producto está hecho para
tener varios árboles a la vez, y una suite que sólo puede con uno está midiendo
la suite:

- **`<n>` es un contador atómico del proceso**, no la etiqueta ni el pid. La
  etiqueta no puede ser el discriminador porque dos fixtures tienen derecho a
  pedir la misma, y el pid ya se sabe que no distingue hilos.
- **La limpieza es por propiedad.** Una arena borra los subvolúmenes que hay bajo
  su raíz, su fuente, y su raíz — y nunca una ruta compartida. Se hace explícita
  al final de cada prueba (donde falla ruidosamente si algo no se fue) y otra vez
  en `Drop`, que es la ruta donde la prueba se cayó antes de llegar a la primera.
  En `Drop` reporta en vez de afirmar: entrar en pánico mientras se desenrolla
  aborta el proceso y se lleva el mensaje de la falla real.
- **Nada se borra antes de crearse.** El ayudante viejo empezaba borrando la ruta
  que iba a usar, que es el acto que destruía el árbol ajeno. Ahora es
  `create_dir` —no `create_dir_all`—, y que se niegue ante algo que ya existe es
  la garantía: una arena o hizo su raíz o no hay arena.

Y el control, que es la parte que faltaba las dos veces anteriores:
`cleaning_one_arena_leaves_the_other_arenas_snapshot_untouched` hace dos arenas
**con la misma etiqueta**, toma en las dos un snapshot **con el mismo nombre**
—la colisión exacta que rompía— limpia una y comprueba que la otra sigue entera.
Es determinista: no hay hilos, no hay `sleep`, no hay esperar a ver si dos
pruebas se cruzan. Con el fixture viejo esa prueba no se podía ni escribir,
porque los dos nombres eran una sola ruta. Al lado va
`two_arenas_asked_for_under_one_label_are_never_given_the_same_name`, que es puro
nombre y por eso corre **también en el contenedor**, donde no hay Btrfs: la
afirmación sobre la que descansa todo el archivo no debería ser demostrable sólo
en la máquina donde ya todo lo demás lo es.

## Regla derivada: un contador que sólo conoce una forma de decir que sí deja de contar cuando aparece la segunda — 2026-08-29

**La regla.** Cuando se agrega una segunda manera de pedir la misma cosa, hay
que ir a buscar a todos los que reconocían la primera. Un instrumento que
reconoce una sola forma no falla ni avisa: sigue contando, y el número que
publica se vuelve mentira exactamente el día en que la forma nueva se empieza a
usar. Es la regla 5 —el instrumento es parte de lo que un cambio puede romper—
en su versión más silenciosa, porque acá no hay error, hay un cero.

**Dónde apareció.** `crates/thalyx-mcp/src/metrics.rs` contaba
`attempts_abandoned` así:

```rust
Some("abandon") if arguments.get("confirm").and_then(Value::as_bool) == Some(true)
```

Cuando `abandonar` pasó a poder hacerse en una llamada —nombrando el intento y
declarando lo que cuesta, sin ningún `confirm`—, ese `if` habría reportado
**cero abandonos** para un agente que usara la forma nueva. Y `attempts_abandoned`
no es decoración: es de las pocas cosas que dicen que el brazo B realmente
ejerció la frontera reversible, que es lo que el banco entero existe para medir.
Una corrida futura habría dicho «el agente nunca abandonó» sobre una corrida en
la que abandonó.

Quedó como una función con nombre, `consented`, que conoce las dos formas y
nombra por qué existe, para que la tercera —si la hay— tenga un solo lugar donde
agregarse.

**Lo que la regla pide.** Al agregar una segunda forma de una operación,
`grep` por el nombre de la primera fuera del código que la implementa. Lo que
aparezca en métricas, en resúmenes, en el arnés del banco o en una prueba que
cuenta, o entiende las dos formas o está midiendo otra cosa a partir de ahora.

## Regla derivada: un resumen no es una identidad, y una prueba escrita sobre el resumen nunca ve la diferencia — 2026-08-29

**Dónde salió.** El abandono en una llamada del 2026-08-28 se autorizaba con dos
conteos: cuántos archivos esperaba perder el llamador y cuántos esperaba
revertir. Tenía siete pruebas, todas verdes, y una de ellas se llamaba
`an_edit_by_somebody_else_stops_it_too_even_though_nothing_would_be_deleted` —
es decir, alguien ya había pensado en el caso peligroso.

Y aun así el mecanismo era inseguro, porque la prueba estaba escrita **sobre el
resumen**. Construía un `Difference` con `modified_total: 4` en vez de `3` para
representar «alguien más editó un archivo», que es el caso donde el tercero toca
un archivo **distinto**. El caso real —el tercero escribe en un archivo que el
agente ya había editado— deja el conteo en `3` y no se puede expresar en el
lenguaje del resumen. Una prueba escrita en los términos del resumen **no puede
nombrar la diferencia que el resumen borra.**

**La regla.** Cuando algo se autoriza con un valor derivado —un conteo, un total,
una suma, un tamaño—, la prueba del caso peligroso tiene que construirse con
**el estado**, no con el derivado. Si el caso peligroso no se puede escribir sin
tocar el estado real, es porque el derivado no distingue esos dos estados, y eso
es exactamente el defecto.

El control que lo dice en voz alta y que ahora vive al lado del arreglo:

```rust
assert_eq!(
    (antes.added_total, antes.modified_total),
    (despues.added_total, despues.modified_total),
    "los conteos se movieron, así que éste ya no es el caso que los engañaba"
);
assert_ne!(antes.state.id, despues.state.id);
```

La primera mitad es la parte incómoda: **afirma que el instrumento viejo no ve
nada.** Sin ella, la segunda mitad prueba que el testigo funciona y no que hacía
falta.

## Regla derivada: la excepción a «un solo escaneo» se encuentra apuntando la función al archivo que la contiene — 2026-08-29

**Dónde salió.** `thalyx-parser` tiene una regla escrita: había tres escaneos que
no se ponían de acuerdo sobre los comentarios, cada desacuerdo produjo una
respuesta equivocada, y desde entonces hay **un** escaneo, `scrub`.

`unbalanced` —decir si una edición mecánica se comió una llave— se construyó
sobre `scrub`, obedeciendo la regla. Pasó las cuatro pruebas escritas a mano:
llaves dentro de cadenas, dentro de comentarios de línea, dentro de comentarios
de bloque, una llave faltante localizada por renglón.

Y falló contra su propio archivo, en el **primer** método que ese archivo
declara:

```
line 81: `}` closes something that was never opened
```

Porque `scrub` contesta *qué ignorar*, y para eso puede permitirse ser generoso:
ante un `'` suelto blanquea el resto del renglón, ya que en Rust eso es un
lifetime mucho más seguido que una cadena. Correcto para contar identificadores,
fatal para contar llaves: `pub fn name(self) -> &'static str {` llega sin su
llave.

**La regla.** «Un solo escaneo» es una regla sobre *la misma pregunta*. Cuando
una función nueva necesita **qué conservar** y el escaneo existente contesta
**qué ignorar**, son preguntas distintas y compartirlas es un defecto silencioso.
La forma de descubrirlo no es razonar sobre ello: es apuntar la función nueva al
corpus más grande y menos amable que haya a la mano, que aquí es el repositorio
mismo — noventa mil renglones que nadie escribió para ella.

Y el falso positivo es el fallo que importa: una comprobación que llama roto al
código ordinario es una comprobación que alguien apaga, y después no protege
nada. Por eso lo que se afirma es «cada `.rs` de este repositorio está
balanceado», y no «este fixture está roto».

## Regla derivada: una prueba de cada mitad no es una prueba de la unión — 2026-08-29

**Dónde salió.** La etapa 55 de `verify.sh`, la primera vez que corrió en Fedora,
sobre Btrfs real. No destruyó nada y aun así falló: el rechazo del rollback
caduco no dijo `workspace_moved`, dijo `done: false` y devolvió una línea
`confirm_with` nueva.

Las dos mitades estaban probadas, y ambas pruebas eran correctas:

- `thalyx_core::attempt::a_write_to_a_file_the_agent_had_already_written_stops_the_rollback`
  llama a `abandon` **directamente** con `Authorised::ByState` y comprueba que
  contesta `WorkspaceMoved`. Pasa.
- `thalyx-cli`'s `consent` tenía siete casos escritos y todos pasaban.

Lo que nunca estuvo probado es el camino entre las dos, y ahí estaba todo el
defecto: `consent` comparaba la declaración del llamador contra el testigo del
*plan* y contestaba con el objeto de costo cuando no coincidían, así que la
llamada **nunca llegaba** a la comprobación bajo el candado. La única palabra que
el mecanismo existe para decir era inalcanzable desde la cara que dice palabras.

**La regla.** Cuando una decisión se toma en una capa y se ejecuta en otra, hay
tres cosas que probar y no dos: lo que decide la primera, lo que hace la segunda,
y **que la primera llegue a la segunda**. Las dos primeras se prueban solas y por
eso se escriben; la tercera no se le ocurre a nadie, y es la que se rompe.

La forma barata de cubrirla cuando la capa de abajo necesita hardware: extraer la
traducción a una función con nombre —aquí `how_it_is_authorised`— y afirmar que
la salida de la decisión es la entrada de la ejecución. No prueba el efecto, pero
prueba la unión, que es lo que faltaba.

Y la señal de que hace falta buscarla: **un rechazo que se lee como «volvé a
intentar con esto»**. La respuesta traía una línea nueva lista para copiar. Un
agente en un ciclo la copia, y en la llamada siguiente sí se pierde el trabajo de
la persona. Que no se haya destruido nada en la primera llamada no hace que el
defecto sea menor; hace que sea silencioso.

## Regla derivada: un `sleep` en una prueba es el sujeto confesando de qué depende — 2026-08-29

**Dónde salió.** `state_identity.rs` tenía un ayudante llamado `write_later` que
dormía veinte milisegundos antes de cada escritura, con un comentario honesto:
dos escrituras seguidas del mismo programa pueden caer dentro del mismo tic de
reloj del sistema de archivos, así que sin la espera la prueba fallaría a veces.

La espera hacía pasar la prueba y **escondía el caso real**. La etapa 55 en
Fedora no duerme —el agente escribe, toma el estado, y un tercero escribe el
mismo archivo enseguida—, que es exactamente lo que pasa cuando una persona y un
agente comparten un árbol.

**La regla.** Un `sleep` puesto para que algo se distinga no es un detalle del
arnés: es una afirmación sobre el sujeto —«esto sólo distingue si pasa
tiempo»— escrita en el único lugar donde nadie la lee como un defecto. Cuando
aparezca uno, la pregunta no es cuánto dormir sino **si el sujeto es correcto sin
dormir**, y si la respuesta es no, arreglar el sujeto.

El corolario, que es lo que hizo falta aquí: la prueba sin espera tiene que
*decir* cuál de los dos casos acaba de probar. Si la máquina resultó ser lenta y
los timestamps sí se separaron, la aserción sigue siendo cierta y prueba menos —
así que `metadata_alone_would_separate` mira si el caso difícil fue el que
ocurrió, y lo imprime.

## Regla derivada: un descriptor sobrevive a un `fork`, y por eso cerrarlo no suelta un candado — 2026-08-29

**Dónde salió.** De las 103 suites del taller, la única que falló en la máquina de
Cesar fue `store::tests::the_lock_is_released_when_it_goes_out_of_scope`. Aislada
pasa siempre; en el contenedor de cuatro núcleos no falló en cuarenta corridas de
la suite completa.

`flock` pertenece a la **descripción de archivo abierta**, no al descriptor, y se
suelta al cerrar sólo cuando se cierra el **último** descriptor que la referencia.
`fork` copia todos. Así que un hijo que ya hizo `fork` y todavía no llegó a
`exec` sostiene el candado del padre, y soltar el candado en el padre durante ese
instante no suelta nada.

En un proceso de `cargo test` las pruebas son hilos, y la prueba de al lado
—`a_second_process_waits_for_the_first_to_finish_its_contract`— lanza un proceso
hijo. Con núcleos suficientes para que ese `spawn` se solape con este `drop`, la
prueba ve un candado que sobrevivió a su ámbito, que es exactamente lo que se
llama y exactamente lo que no pasó.

**No es sólo un artefacto de prueba**, y por eso el arreglo está en el candado y
no en la prueba: Thalyx lanza `btrfs` y `bpftool` **con el candado del store
tomado**, así que un `thalyx` real puede dejar el store cerrado después de haber
terminado con él, y el siguiente se forma detrás de nadie. `ContractLock::drop`
ahora hace `flock(LOCK_UN)`, que quita el candado de la descripción misma y lo
ven todas las copias del descriptor a la vez.

**La regla.** Cualquier recurso que viva en la *descripción* y no en el
descriptor —candados `flock`, el offset, las banderas de estado— se suelta
explícitamente, nunca cerrando. Y la forma de probarlo sin depender del reloj es
`File::try_clone`, que es el mismo `dup` que hace un `fork` con el tiempo
quitado: se toma el candado, se duplica, se cierra el original, y se afirma que
sigue tomado. Esperar a que un hijo real esté a medio camino es medir la máquina.

## Regla derivada: mover la comprobación cara del lado equivocado de la respuesta — 2026-08-29

**Dónde salió.** La comprobación de estado de un rollback se hacía bajo el
candado, que es correcto, y **antes** de que el restore empezara a prepararse.
Entre la respuesta y el intercambio quedaban entonces: abrir el diario, escribir
la intención, limpiar un nombre de staging viejo, y hacer la copia escribible del
snapshot. En Btrfs eso son milisegundos, y un milisegundo alcanza para que el
editor de otra persona guarde.

**La regla.** «Comprobar justo antes de actuar» se cumple midiendo qué queda
*después* de la comprobación, no dónde está escrita la línea. Si entre las dos
hay trabajo, ese trabajo se mueve al otro lado y la comprobación se entrega como
una pregunta al que va a actuar —aquí, `apply_holding_the_lock` recibe la última
mirada y la hace con el intercambio ya armado—.

Y lo que queda se nombra en vez de negarse. La ventana no es cero y nada que no
congele el sistema de archivos la haría cero, así que lo garantizado son dos
frases: una escritura que terminó antes de la comprobación nunca se pierde, y una
que cae dentro de la ventana no se destruye sino que se desplaza al árbol que el
restore conserva. Declarar «estado exacto» a secas, sabiendo que hay una carrera,
sería la misma clase de mentira que un conteo.

## Regla derivada: una ruta que no está no es una ruta que nadie pudo leer — 2026-08-29

**Dónde salió.** El testigo de [[Conocimiento-con-Testigo]] recorre un conjunto
de rutas nombradas, y una de las que nombra para una comprobación de Rust es
`Cargo.lock`. Un espacio de trabajo que todavía no tiene candado hacía que
`walkdir` devolviera un error por esa ruta, y ese error se contaba como
*ilegible*. Un testigo con algo ilegible está **incompleto**, y un testigo
incompleto no coincide con nada — ni consigo mismo, a propósito.

Resultado: el cache de validación no acertó **ni una sola vez**, en silencio, y
el compilador corrió siempre. Todos los veredictos eran correctos. Lo único que
estaba mal era el costo, que es la única cosa que ese cache existe para bajar.

**La regla.** Es la regla 10 leída al revés. «Una falla de lectura no es una
falla de existencia» ya estaba escrita; su otra mitad no: **una falla de
existencia no es una falla de lectura**. Un `ENOENT` sobre una ruta que el
llamador nombró es información sobre el árbol —no hay candado— y pertenece al
conjunto como una ausencia, no al conteo de lo que nadie pudo mirar.

Y la manera de encontrarlo es la regla 13: la afirmación era que algo caro deja
de pasar, así que la prueba **cuenta las veces que pasa**. `process_launches` es
ese conteo. Una prueba escrita como «el veredicto fue `passed`» habría pasado
todas las veces.

## Regla derivada: una herramienta que se invoca dentro de la frontera escribe dentro de la frontera — 2026-08-29

**Dónde salió.** Una prueba afirmaba que una petición que cambió dos archivos
reporta dos, y reportó veintinueve. Los otros veintisiete eran `target/`:
rust-analyzer corre `cargo metadata` y compila build scripts, y Cargo sin
`CARGO_TARGET_DIR` construye **dentro del espacio de trabajo**.

Dentro de `hacer` eso significa tres cosas, ninguna visible desde el veredicto:
el snapshot contiene un árbol de compilación, la diferencia observada deja de ser
una medida de lo que hizo el programa, y el rollback destruye el cache de
compilación que abarata la comprobación siguiente.

**La regla.** Antes de invocar una herramienta ajena dentro de una frontera
transaccional, hay que preguntarle **dónde escribe** y decírselo. No basta con
que la herramienta sea «de lectura»: `cargo metadata` es una consulta y aun así
deja un `Cargo.lock` y un `target/`. Y lo que lo cachó no fue una revisión del
código sino una prueba que contaba, otra vez: una escrita como «los dos archivos
quedaron renombrados» pasa con las veintinueve.

## Regla derivada: un solo recurso global por proceso mide al planificador — 2026-08-29

**Dónde salió.** El proveedor semántico se guardaba en un solo espacio del
proceso, porque arrancar rust-analyzer cuesta veinticinco segundos y la sesión es
una. En el binario de pruebas la sesión no es una: `cargo test` corre las pruebas
de un binario como hilos de **un** proceso, y dos pruebas sobre árboles distintos
se desalojaban por turnos. La métrica que dice «un arranque por petición»
quedaba midiendo el orden en que el planificador las despertó.

**La regla.** Es la regla 11 en un lugar donde nadie había mirado: no es sólo el
sistema de archivos y no son sólo los descriptores. **Cualquier recurso global
con un solo espacio** —una conexión, un proceso auxiliar, un candado con nombre—
es un recurso por el que las pruebas compiten, y una métrica sobre él es una
medida del planificador. Se arregla dándole una llave: aquí, uno por árbol, con
un tope y desalojo del más viejo, que es lo que una sesión real necesita de todos
modos.

---

## Reglas nuevas — 2026-08-30

### Un campo que desaparece el día interesante

`exec::tests::a_check_of_bytes_nobody_has_seen_is_run` y su gemela fallaron en
Fedora afirmando `output["cached"] == false` y recibiendo `Null`. La causa no
era el cache: era que el brazo *«no hay cargo en esta máquina»* nunca escribía
el campo, y ése era el brazo que tomaba una máquina donde `sudo` había puesto
`HOME=/root` delante de un toolchain instalado bajo el home de `$SUDO_USER`.

Dos personas leyeron dos aserciones sobre un cache. Lo que había pasado era un
`HOME`.

> **Un esquema de respuesta es estable o no es un esquema.** Si un campo aplica
> conceptualmente, se escribe en **todos** los brazos, incluido el que dice que
> nada se pudo hacer. Un campo que sólo aparece el día interesante es un campo
> que nadie maneja el día interesante — y la prueba que lo garantiza recorre los
> brazos, no la corrida que casualmente toma uno.

Y su mitad gemela: **una negativa nombra dónde buscó.** «No hay cargo» es una
oración sobre la que nadie puede actuar cuando el cargo está a un directorio de
distancia bajo otro home.

### Tres búsquedas de la misma cosa son tres máquinas

Había tres lugares que buscaban un binario de Rust —`metadata::cargo`,
`analyzer::find`, `exec::find_cargo`— y discrepaban en todo: qué variables leer,
si confiar en `PATH`, si *correr* un candidato antes de creerle. La cuarta era
`HAVE_ANALYZER` en `verify.sh`, mirando `$HOME`, que bajo `sudo` es `/root`.

> **Una búsqueda repetida en dos lugares son dos búsquedas, y la segunda es la
> que discrepa en la máquina de alguien más.** Y: un veredicto producido por
> «el compilador que viniera primero en el `PATH` de quien llamó» es un veredicto
> que nadie puede reproducir. Se nombran lugares.

### Un lenguaje puede dar vueltas, así que cada recurso lleva su techo

`MOST_STEPS` acotaba una lista por construcción. Un programa con ciclos no se
acota por construcción, y un solo techo no alcanza: tiempo de reloj, presupuesto
de instrucciones, memoria, pila, llamadas a la máquina, procesos lanzados, bytes
que entran y bytes que salen son ocho preguntas distintas. Uno solo para las
ocho es la forma de la regla 3 al revés.

> **Y una respuesta demasiado grande se niega, no se corta.** Una respuesta
> cortada a la mitad es una respuesta sobre la que un modelo actúa creyendo que
> está completa.

### Una detención tiene que estar fuera del lenguaje que detiene

Una aserción que sólo lanza una excepción de JavaScript la atrapa el propio
programa que la falló, y un programa escrito por un modelo de lenguaje envuelve
todo en `try`/`catch`. La corrida seguiría más allá de la premisa que acababa de
refutar, y confirmaría.

> **Lo que detiene una corrida no puede estar en el lenguaje de la corrida.** La
> aserción queda trabada del lado de Rust y el motor se detiene; el `throw` es
> sólo la mitad que el programa puede ver.

### El motor reporta su propia interrupción como una excepción del programa

QuickJS levanta una interrupción como una excepción ordinaria de JavaScript. La
primera versión reportó `while (true) {}` como *«el programa lanzó»* — una
oración sobre el programa en lugar de sobre el techo que alcanzó, que es el
diagnóstico equivocado que la regla 5 persigue.

> **Cuando la herramienta convierte tu razón en la razón del sujeto, guarda la
> tuya aparte.** El manejador marca que decidió detener, y esa marca le gana a
> lo que el motor haya alcanzado a decir.

Su gemela, del mismo día: el `error.stack` de QuickJS son sólo los marcos —a
diferencia del de V8, no empieza con el mensaje— así que un envoltorio que
prefería `.stack` reportaba cada falla como una lista de números de línea con la
razón faltando. Regla 5 donde el arnés es el recuerdo que alguien tiene de otro
motor.

### Lo que se revisa por nombre en una lista, se revisa por llamada en un programa

`Program::read` rechaza un *paso* llamado `exec` o `attempt` antes de tomar el
snapshot. Correcto para una lista: una lista es un valor que algo puede mirar.
**Un programa no.** Alcanza verbos por nombre en tiempo de ejecución.

Una prueba pidió `intento abandonar` desde adentro de un programa y recibió
`ok: true` con una línea `confirm_with` cargando el nombre del snapshot y el
testigo de estado exacto — todo lo que una segunda llamada necesita para
abandonar la transacción desde adentro de sí misma, a media corrida.

> **Un chequeo que sólo se hace sobre la forma estática no se hace.** Se hace
> donde se puede hacer: en el momento de la llamada. Y una negativa no entrega
> lo que hace falta para reintentar.

### «No escribe, por lo tanto es de sólo lectura» es una oración sobre el protocolo

rust-analyzer se describió aquí como *un lector* durante una semana: nunca
aplica una edición, un rename vuelve como descripción, Thalyx escribe. Cierto
del protocolo LSP. Falso del árbol de procesos: corre `cargo metadata`, y para
contestar sobre un espacio de trabajo con un proc-macro compila y **ejecuta**
código arbitrario de un registro.

> **La autoridad de un proceso no se deduce de la forma de su API.** Se deduce
> de qué arranca.
