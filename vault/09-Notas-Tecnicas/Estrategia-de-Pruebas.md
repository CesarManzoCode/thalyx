---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-01
tags: [pruebas, ci, atomicidad, evidencia]
---

# Estrategia de pruebas

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

Y van ocho, con la del 2026-08-07: el parser que comprueba los offsets de
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
