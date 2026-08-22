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

**Y van diez.** Las tres últimas son todas del 2026-08-07 y las tres tienen
mecanismos distintos, por eso cada una tiene su regla abajo:

- **La octava**, el parser de headers que descartaba campos con comentario al
  final de la línea — está justo debajo.
- **La novena**, el control de la etapa 18 que dañaba espacio libre y acusaba al
  kernel de montar basura. Ahí el arnés no se equivocó al preguntar: se equivocó
  en *cuándo* sacó la muestra.
- **La décima**, clippy fallando en una máquina y pasando en la otra contra el
  mismo código. Ahí el arnés era el correcto y tenía **otra versión**, que es un
  arnés distinto.

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
