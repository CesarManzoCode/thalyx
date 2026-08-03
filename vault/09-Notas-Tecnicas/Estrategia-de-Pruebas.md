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
