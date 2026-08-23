---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-09
tags: [llm, objetivo, superficie, catalogo, no-negociable]
---

# Superficie para el LLM

> **Decretado por Cesar el 2026-08-09:**
>
> «tenemos que encontrar y decretar todas las cosas que podemos crear/cambiar
> para que el LLM se mueva mejor en nuestro sistema, así como los grafos»

[[FS-en-Grafo]] dice de sí mismo que fue *«la primera oportunidad clara de mejora
identificada para que la IA operara en el mejor terreno posible — el ejemplo
fundacional de qué significa que una primitiva sea nativa para la IA en vez de
heredada del diseño pensado para humanos»*.

**Fue la primera y nunca se escribió el resto.** [[Filosofia-Fundacional]]
decretó el objetivo el mismo día y nombró cuatro ventajas; cuatro no es un
catálogo, es lo que se alcanzó a recordar. Esta nota es el resto, y el criterio
con el que se decide si algo entra.

## Qué decreta esta nota y qué no

**Decreta el criterio** —los cinco costos de abajo— y decreta que **esta lista es
la lista**: lo que no está aquí no se ha considerado todavía, y agregar algo es
agregarlo aquí.

**No decreta que se construya nada.** Estar en el catálogo no autoriza escribir
una línea, y el [[Criterio-de-Inclusion-de-Primitivas]] sigue vigente entero: una
pieza entra ahora sólo si omitirla hoy obliga a una reescritura dolorosa después.
El orden lo decide Cesar.

---

## El criterio: cinco costos

Un LLM trabajando en un sistema operativo paga cinco cosas. Todo lo que Thalyx
puede hacer por él baja alguno de los cinco, y **una decisión que no baja ninguno
no es una mejora para el modelo por bien que suene**.

Están en este orden porque es el orden en que muerden.

### 1. Costo de descubrimiento — qué tengo que hacer para saber

En Linux un agente que llega no sabe qué hay. Corre `ls`, abre archivos de
configuración, adivina versiones, lee `--help` de herramientas que lo escriben
distinto cada una. **Cada sesión empieza en frío** y la mitad de las primeras
veinte acciones son preguntas disfrazadas de comandos.

Es el costo que Thalyx puede bajar más barato, porque el sistema **sabe** lo que
el agente está averiguando a ciegas.

### 2. Costo de contexto — cuántos tokens cuesta la respuesta

La ventana es finita y es el recurso escaso. `grep -r` en un árbol grande
devuelve cientos de renglones de los cuales importan dos; `ls -R` puede llenar la
ventana entera por accidente. Un agente que gastó su contexto en ruido **olvida
lo que estaba haciendo**, y eso no se ve como un error: se ve como un agente
tonto.

Aquí está el argumento más fuerte del proyecto y es incómodo de aceptar: **una
respuesta más corta y exacta vale más que una más completa.** Un listado que
devuelve todo y un listado que devuelve lo pedido con el total anotado no son el
mismo servicio.

### 3. Costo de ambigüedad — cuántas lecturas admite la respuesta

`exit 1` y una oración en inglés que cambia entre versiones. El agente tiene que
parsear prosa escrita para una persona, y cuando la prosa cambia, el agente se
rompe sin que nadie tocó al agente. Un hecho con una sola lectura vale más que
uno con dos, **aunque el de dos tenga más información**.

Esto es lo que [[Punto-Actual|la cara estructurada]] empezó a bajar el 2026-08-09.

### 4. Costo de equivocarse — qué cuesta un error y si se puede deshacer

**Éste es el que más cambia el comportamiento, y es el menos obvio.**

En un sistema donde todo es irreversible, un agente racional se vuelve tímido:
pregunta de más, prueba de menos, prefiere no actuar. Eso no se lee como
prudencia, se lee como incapacidad — y la causa no está en el modelo, está en
que el sistema no le ofrece ninguna manera de intentar.

Un sistema donde un error se deshace **produce un agente distinto**, no un agente
mejor cuidado. Es la ventaja que ningún otro sistema operativo tiene y la que más
lejos está de estar expuesta.

### 5. Costo de permiso — cuánto tiene que pedir, y cuánto recibe cuando pide

Hoy son dos extremos y los dos son malos: o el agente corre como el humano y
puede borrar el disco, o corre encerrado y no sirve. La consecuencia práctica es
que **la gente le da todo**, porque lo acotado no alcanza para trabajar.

Un permiso acotado a la tarea, que se pide una vez y expira solo, es lo que
permite que un agente haga treinta cosas sin treinta confirmaciones **y sin haber
recibido el sistema entero**.

### La regla de desempate entre los cinco

Cuando dos costos chocan, **gana el que se pueda medir en la tarea que el agente
está haciendo**, y el que se sacrifique se nombra. Un ejemplo real y ya resuelto:
la cara estructurada devuelve los nombres ocultos, lo que sube el costo de
contexto y baja el de descubrimiento — se eligió a sabiendas, porque un nombre
que el sistema no dio es un nombre que el agente no puede pedir después.

### Lo que **no** cuenta como mejora para el LLM

Escrito porque es donde se cuela el trabajo que se siente productivo y no lo es:

- **Que sea más rápido**, si el agente no estaba esperando.
- **Que sea más bonito o más consistente**, si ninguna de las dos lecturas cambia.
- **Que tenga más opciones**, si ninguna baja un costo. Cada verbo nuevo es
  superficie que hay que describir, probar y mantener.
- **Que se parezca más a Linux**, por sí solo. Parecerse a Linux baja el costo de
  descubrimiento —un agente que sabe `ls` encuentra `ls`— y eso ya está decretado.
  Parecerse en algo que el agente no conocía no baja nada.

---

## Las cuatro que ya existen y no están expuestas

Antes del catálogo, el estado incómodo: **Thalyx ya construyó cuatro cosas que
ningún otro sistema tiene, y ninguna de las cuatro es alcanzable por un agente
ajeno.**

| Ventaja | Dónde | Estado |
|---|---|---|
| Índice semántico | [[FS-en-Grafo]], `crates/thalyx-graph` | Construido. Sólo por CLI de Thalyx |
| Journal, snapshots, rollback | [[Journal-y-Snapshots]] | Construido. Sólo por CLI de Thalyx |
| Procedencia por campo | [[Marcado-de-Origen]] | Construido, y sólo para contratos del agente local |
| Permisos por tarea | [[Permisos-JIT]] | Construido, y sólo para módulos |

Ninguna de las cuatro está en el catálogo de abajo como «crear». Están como
**exponer**, que es un trabajo distinto y mucho más barato, y es donde está el
mayor rendimiento por hora del proyecto entero.

---

## El catálogo

Cada punto dice qué es, **qué costo baja**, y qué hay hoy. El número es su
nombre: se cita como `A1`, `D2`.

### A. El sistema se describe a sí mismo — *descubrimiento*

**A1. Un catálogo de verbos legible por máquina.**
Un verbo que devuelve todos los verbos: qué nombres tiene cada uno, qué
argumentos toma, qué banderas, qué errores puede dar y qué forma tiene su
respuesta estructurada. Un agente que llega **no necesita haber sido entrenado
sobre Thalyx: pregunta.**

Ningún sistema operativo puede hacer esto. En Linux `--help` es prosa, es por
herramienta, no es consistente y a veces no está; `man` no es parseable y muchas
veces no está instalado. Thalyx tiene una sola superficie y la conoce entera.

Es el punto de mayor rendimiento del catálogo y es de los más baratos: la lista
de verbos ya existe en el código, hoy escrita dos veces —el `match` de la sesión
y el banner— que es exactamente el número de veces que empieza a divergir.

**A2. Un error nombra la operación que lo arreglaría.**
Hoy un error trae una palabra estable (`exists`, `absent`) y una oración. Falta
el tercer campo: qué hacer. `exists` sobre un destino de `cp` sabe que la salida
es borrar el destino o elegir otro nombre; decirlo cuesta un campo y le ahorra al
agente un ciclo entero de razonar y probar.

Baja ambigüedad **y** descubrimiento, y es donde los dos se tocan: un error que
enseña es documentación entregada en el momento exacto en que sirve.

**A3. El estado de la máquina en un objeto.**
Qué hay instalado, qué está montado, qué enforcement está puesto, qué gama de
modelo, cuánto disco. `estado` ya lo contesta en prosa y ya lee la máquina de
verdad en vez de recitar; falta su segunda cara.

### B. La respuesta cabe en una ventana de contexto — *contexto*

**B1. Toda respuesta larga viene acotada, con el total y un cursor estable.**
Nunca un listado sin límite. La respuesta dice cuántos hay, cuántos manda y cómo
pedir los que siguen, y el cursor sobrevive a que el directorio cambie —o dice
que no sobrevivió, que es la otra respuesta honesta.

La falla que evita es silenciosa y cara: un `ls` de cuarenta mil archivos no da
error, da un agente que olvidó su tarea.

**B2. Toda lectura devuelve la identidad exacta de lo que leyó.**
Un hash del contenido junto al contenido. Con eso *«¿sigue siendo cierto lo que
leí hace veinte pasos?»* se contesta con una comparación en vez de volviendo a
leer el archivo. Un agente sin esto **re-lee por si acaso**, y eso es costo de
contexto pagado por no tener una respuesta barata.

**B3. «Qué cambió desde X» como pregunta de primera clase.**
Es la versión de B2 para un árbol entero, y **Thalyx ya tiene la mitad
construida**: `thalyx-watch` cuenta las mutaciones del filesystem en el kernel,
con atribución por ancestros, y `thalyx_mutations` es un ringbuf que dice *qué*
cambió y que hoy **nadie consume** ([[Tareas-Pendientes]]).

Un agente puede preguntarle al sistema qué se movió desde que él miró, en vez de
recorrer el árbol. Eso no existe en ningún sistema operativo sin que alguien
monte un vigilante aparte.

### C. Estructura en vez de texto — *ambigüedad y contexto*

**C1. El índice semántico, expuesto.**
*«Quién llama a esta función»*, *«qué depende de este archivo»*, *«qué se
rompería si lo muevo»*. `dependencies_of`, `dependents_of`, `tagged` y `nodes` ya
existen y están probados; lo que falta es que algo que no sea el CLI de Thalyx
pueda preguntarlos.

Es el ejemplo fundacional de la nota que abre este documento, y sigue sin estar
disponible para el destinatario del decreto.

**C2. Búsqueda que devuelve símbolos, no renglones.**
`grep` contesta con renglones porque no sabe qué es un símbolo. El
[[Parser-Mecanico]] sí, en cinco lenguajes. Un resultado que dice «función
`login`, `src/auth.rs`, definida aquí, llamada desde estos tres sitios» cuesta
una fracción de los tokens de la lista de coincidencias textuales **y no tiene
falsos positivos por comentarios**.

**C3. Toda respuesta trae su grado de vigencia.**
Ya está decretado para el índice como la *regla de honestidad de las consultas*
([[FS-en-Grafo]]): las filas y la advertencia llegan juntas porque separarlas
dejaría que se olvidara. **Se generaliza a toda la superficie**: una respuesta
que no dice cuán segura es obliga a quien la lee a suponer, y un agente que
supone bien nueve veces se equivoca la décima sin saber por qué.

### D. Equivocarse es barato — *equivocarse*

**D1. Ensayo en todo verbo que cambia algo.**
*«Qué pasaría si»*, contestado en la misma forma que la respuesta real y sin
tocar nada. Un agente puede planear una operación completa antes de ejecutar
ninguna parte.

Vale más de lo que parece por una razón de comportamiento: hoy la única manera
que tiene un agente de saber qué hace un comando es **hacerlo**.

**D2. El intento con nombre: empezar, confirmar, abandonar.**
Un conjunto de cambios que se aplica entero o no se aplica. `intento start` toma
un snapshot, el agente hace treinta cosas, y `intento abandonar` deja la máquina
exactamente como estaba.

Thalyx ya tiene las dos piezas —snapshots de Btrfs y el journal— y lo que falta
es unirlas y **dárselo a alguien que no es el núcleo**. Es la frase que
[[Filosofia-Fundacional]] usa como la ventaja que ningún sistema tiene:
*«intenta esto y si sale mal deshazlo»*.

Un refactor a medio aplicar en treinta archivos es peor que uno no empezado, y
hoy no hay forma de evitarlo salvo la disciplina del agente.

**D3. Toda acción dice cómo se deshace, y dice cuándo no se puede.**
La respuesta de una operación destructiva carga la manera de revertirla. Y donde
no la hay, **lo dice**: `/home` está decretado como el único sitio que ningún
rollback nuestro puede devolver ([[Coherencia-Doble-Ruta]]), así que un `rm` ahí
es definitivo y el agente tiene que saberlo **antes**, no después.

Es la regla 10 aplicada a lo reversible: *no poder deshacer* y *no haberlo dicho*
son dos cosas distintas y sólo una es aceptable.

### E. El permiso se pide una vez y alcanza — *permiso*

**E1. Un agente ajeno es una tarea con identidad, cgroup y concesión que expira.**
Exactamente lo que [[Permisos-JIT]] ya hace con un módulo, para algo que Thalyx
no compiló. El agente recibe alcance —este árbol, estas operaciones, este
tiempo—, trabaja adentro sin preguntar, y al expirar la concesión se retira del
mapa BPF sin avisarle a nadie porque no hace falta.

Esto es lo que rompe el todo-o-nada, y es la diferencia entre un agente al que se
le da el sistema entero por comodidad y uno al que se le da lo de la tarea.

**E2. Pedir más es una llamada estructurada, y la confirmación va por el camino confiable.**
Que el agente pueda **pedir** algo peligroso sin que sea peligroso pedirlo. La
solicitud es un objeto; la confirmación la redacta el núcleo desde campos del
manifiesto y el agente nunca la ve, sólo el resultado ([[Camino-Confiable]],
[[API-Interna-de-Modulos]]).

Eso es lo que le permite dejar de ser tímido sin que nadie tenga que confiar en
él.

**E3. Lo que hizo el agente se distingue de lo que ya estaba.**
[[Marcado-de-Origen]] es el mecanismo y hoy cubre los contratos del agente local.
Extendido a las acciones de un agente ajeno, un humano puede auditar una sesión
entera y separar lo suyo de lo del modelo — que es la condición para que alguien
le deje trabajar sin mirar.

### F. El agente sobrevive a su propia sesión — *descubrimiento*

**F1. Memoria persistente, accesible.**
[[Memoria-Persistente]] existe, con hechos e inferencias separados y fechados por
rutas. Un agente que retoma mañana no vuelve a empezar en frío, que es el costo
de descubrimiento pagado otra vez, entero, cada vez.

**F2. El journal como historia legible por máquina.**
*«Qué se hizo aquí y por qué»* contestado por el sistema y no reconstruido de la
conversación. Ya se escribe; falta que se lea desde afuera.

### G. El piso, sin el cual nada de lo anterior importa

Esto **no es superficie para el LLM, es la obligación** de
[[Principio-Doble-Ruta]] — y hay que tenerlo escrito aquí porque el catálogo de
arriba describe un sistema en el que hoy **Claude Code no arrancaría**.

| | Qué falta |
|---|---|
| **G1** | Ejecutar procesos: **lanzar** y **esperar**. *Matar* y *ver qué corre* existen desde el 2026-08-23 — ver [[Procesos]] — y son la mitad que no necesitaba decidir nada sobre módulos firmados |
| **G2** | Un runtime en el que un agente ajeno pueda correr |
| **G3** | Red |
| **G4** | Control de versiones, o la razón escrita de por qué no hace falta |

Ninguno de los cuatro baja un costo del modelo: **son la condición para que el
modelo esté ahí.** Y por eso no compiten con el catálogo, lo anteceden.

---

## Lo que este decreto no autoriza

Escrito porque el criterio *«si le facilita el trabajo al modelo, ésa es la
forma»* es fácil de estirar hasta romper cosas que ya están decretadas:

- **No baja la atribución.** El núcleo comprueba cada id contra los canales, y
  quitarlo no le facilita nada al modelo: le quita a Cesar la última línea que lo
  protege de un error del modelo ([[Filosofia-Fundacional]]).
- **No quita la confirmación humana.** [[Camino-Confiable]] no se toca. E2 hace
  que pedir sea barato, no que no haya que pedir.
- **No le quita nada al humano.** [[Principio-Doble-Ruta]] se cumple entero: cada
  cosa del catálogo que se construya nace con **las dos caras**, y la humana no
  es la que se agrega después.
- **No es una lista de tareas.** Nada de aquí se construye por estar aquí.

---

## Cómo se comprueba, y no cómo se recita

La vara de [[Filosofia-Fundacional]] es un agente ajeno trabajando aquí mejor que
en Linux. Eso no se puede comprobar hoy —falta G— así que hace falta una forma
comprobable **ahora**, y es ésta:

> **Un programa que nunca vio Thalyx completa una tarea de archivos usando sólo
> la cara estructurada, sin que nadie le explique la superficie.**

Es alcanzable en cuanto exista A1: el programa pide el catálogo de verbos, elige,
ejecuta y lee los hechos. Si no puede, no es que le falte entrenamiento — es que
la superficie no se describe a sí misma, y ése es el defecto.

La versión completa, para cuando G exista: un agente ajeno hace una tarea real, y
**el journal dice qué hizo, la procedencia separa lo suyo de lo que ya estaba, y
un `intento abandonar` lo deshace entero.** Los tres a la vez, porque cualquiera
de los tres solo lo tiene también Linux con suficiente trabajo encima.

---

## Estado, punto por punto

Tres estados y ninguno se infiere de la ausencia de otro. **Nada de la columna
derecha está decidido**; es el catálogo, no un plan.

| | Punto | Estado |
|---|---|---|
| A1 | Catálogo de verbos legible por máquina | **Hecho** — `describe`, 40 verbos, y desde el 2026-08-23 el `op` que promete se comprueba corriendo el verbo |
| A2 | El error nombra su remedio | **Hecho** — `remedy` como palabra estable |
| A3 | El estado de la máquina en un objeto | **Hecho** — `estado`, con los tres estados de la regla 10 |
| B1 | Respuestas acotadas con total y cursor | **Hecho** — `limite=` y `cursor=`, en `ls`, `depende`, `usan`, `buscar`, `historia`, `cambios` |
| B2 | Identidad exacta de lo leído | **Hecho** — `sha256` del archivo entero en cada lectura |
| B3 | Qué cambió desde X | **Hecho hasta donde un anillo llega** — `cambios`. No es una historia y no nombra archivos, y lo dice |
| C1 | Índice semántico expuesto | **Hecho** — `indexar`, `depende`, `usan` |
| C2 | Búsqueda por símbolos | **Hecho** — `buscar`, con la definición y los usos separados, sin comentarios ni cadenas |
| C3 | Vigencia en toda respuesta | **Hecho donde hay caché**: cada respuesta del índice trae `fresh` |
| D1 | Ensayo en todo verbo que cambia | **Hecho para los verbos de archivos** — `ensayo`; los otros cinco dicen que no pueden |
| D2 | El intento con nombre | **Hecho** — `intento empezar/confirmar/abandonar`; la política probada aquí, el Btrfs en la etapa 26 |
| D3 | Cada acción dice cómo se deshace | **Hecho** — `undo`, y `null` donde no hay vuelta |
| E1 | El agente ajeno como tarea con concesión | **Bloqueado por G1 y G2**, no por dificultad: no hay a qué darle la concesión |
| E2 | Pedir permiso en caliente | Fuera de la v1 de la API por decreto |
| E3 | Procedencia de lo que hizo el agente | Construido para contratos, sin extender |
| F1 | Memoria persistente accesible | **Hecho** — `recuerdos`, con las tres listas separadas |
| F2 | Journal legible desde afuera | **Hecho** — `historia`, el más nuevo primero, con lo que no cubre dicho en un campo |
| G1–G4 | El piso | No construido, y es lo que bloquea la vara |

**Y desde el 2026-08-23 hay una fila que no estaba en el catálogo y decide si
cualquiera de las de arriba sirve: los cuarenta verbos contestan por
estructura, y la lista de excepciones está vacía.** Catorce puntos hechos no
servían de nada mientras los seis verbos de módulos —el ciclo entero de lo que
Thalyx existe para hacer— sólo hablaran en prosa.

**Lo que esa tabla decía el 2026-08-09 y ya no dice:** que seis de los
diecinueve puntos existían y no eran alcanzables. Ninguno queda así.

**Catorce de los diecinueve están hechos.** El único del catálogo propiamente
dicho que falta es **E1**, y la razón cambió: no es dificultad ni hierro, es que
le falta el piso. Ver abajo.

## Los tres que se dijeron «sólo hierro», y por qué dos de ellos no lo eran — 2026-08-10

Cesar leyó la sección de abajo y contestó: *«me parece que ahí no hay costo
real, más bien es costo de dificultad… si realmente aportan un beneficio real y
claro a los LLMs, entonces hazlos»*. Tenía razón sobre dos de los tres, y la
razón por la que tenía razón vale más que los dos puntos.

**D2 confundía dos preguntas.** `thalyx-snapshot` ya llevaba escrito el corte
correcto en el comentario de su propio falso: *«la política que sólo se puede
ejercer en un sistema de archivos Btrfs es política que nunca se ejerce»*. Cuál
intento está abierto, qué hace un segundo, a qué árbol apunta un abandono, qué
pasa cuando el snapshot que nombra ya no está — ninguna de ésas es una pregunta
de Btrfs, y todas habrían quedado sin probar por tratarlas como si lo fueran. La
regla 8 pide que el falso modele **la propiedad bajo prueba**, y la propiedad
bajo prueba era la política.

**B3 se apoyaba en un hecho falso.** «Consumirlo es código BPF» no es cierto: el
productor ya está escrito y no se toca, y el consumidor es código de usuario —
`bpf_obj_get`, dos `mmap`, y aritmética sobre bytes. El verificador no entra en
esto. Y el protocolo del anillo es una función pura sobre bytes que un arreglo
de bytes modela exactamente, porque el lado del kernel de ese contrato *es* la
disposición en bytes.

**E1 sí queda, y por una razón distinta de la que estaba escrita.** No es que
toque seguridad —que la toca— ni que necesite el LSM. Es que **no hay a qué
darle la concesión**: G1 (ejecutar procesos) y G2 (un runtime donde un agente
ajeno pueda correr) no existen. Construir la concesión ahora produce código que
no se puede ejercer ni siquiera en la máquina de Cesar, que es una clase de
riesgo distinta a la de D2 y B3. Y antes que el código hay una decisión suya:
hoy Thalyx sólo corre módulos firmados, y un agente ajeno por definición no lo
es.

### La regla que sale de esto

Escrita en [[Estrategia-de-Pruebas]]: **«sólo hierro» es una afirmación sobre
qué propiedad se está probando, y hay que decir cuál.** Dos de los tres de abajo
eran mezclas de una propiedad que necesita el hardware y otra que no, y la
mezcla se resolvió a favor de no construir nada.

---

## Revisiones

### 2026-08-23 — Los nueve verbos que no tenían segunda cara, y ya no queda ninguno

Cesar leyó la lista de pendientes y contestó que **íbamos innecesariamente
lento**: *«todo esto es horizontal, nada es realmente complejo, son cosas
sencillas pero son muchísimas, porque no hacemos un sprint para eliminar todo el
horizonte barato?»*. Tenía razón y ésta es la primera entrega de ese sprint.

Los verbos sin cara eran nueve, más tres que se creían sin nada que contestar.
Ninguno era difícil; lo que los mantuvo así es que **el catálogo de esta nota
trata de superficie nueva**, y éstos son anteriores al decreto de las dos caras.
Nadie volvió por los viejos.

**Lo que costaba, dicho bien**: `disponibles`, `instalar`, `modulos`, `correr`,
`permisos` y `revertir` son el ciclo completo de lo único que Thalyx existe para
dejar hacer. Un programa que no los lee no puede saber si lo que va a instalar ya
está instalado, ni qué hay en el repositorio, ni qué concedió la vez pasada.
Catorce de diecinueve puntos hechos y el ciclo entero en prosa.

Ahora el ciclo se corre entero por la cara estructurada — buscar, instalar,
listar, correr, deshacer, listar — y quedó capturado en una sola sesión por
tubería.

**Tres cosas que construirlo enseñó y esta nota no anticipaba:**

1. **El camino confiable no se debilita, se reporta.** `Camino-Confiable` queda
   intacto: sin terminal no hay confirmación y no hay instalación, y
   `instalar-en` sigue pidiendo la ruta del disco tecleada. Lo único que cambia
   es que la negativa vuelve como objeto en vez de una línea en `stderr`, donde
   un parser que lee un solo flujo no la veía nunca.
2. **Una afirmación falsa salió de correrlo, no de leerlo.** El primer campo de
   la negativa decía `wrote_anything: false`, y el journal **sí** guarda una
   entrada `rejected` — que es justamente el punto, porque una negativa del
   camino confiable que no dejara rastro sería un camino confiable que nadie
   puede auditar. La cara humana llevaba el mismo exceso en prosa.
3. **«No tiene nada que contestar» era el último sitio donde quedaba silencio.**
   `limpiar`, `salir` y `apagar` contestan, y las tres razones son distintas: no
   hay pantalla del otro lado, un pipe cerrado sin nada adentro es exactamente lo
   que parece un cierre inesperado, y `apagar` no regresa cuando funciona.

Lo que lo sostiene: la prueba del catálogo **afirma que la lista de verbos sólo
prosa está vacía**, y la etapa 22 pasó de catorce verbos manejados a veintiuno.

### 2026-08-23 — `describe` prometía prosa donde había un objeto

`red` nació con sus dos caras y quedó declarado `answers: None` en el catálogo,
así que **A1 estaba mintiendo sobre A1**. No es un detalle de datos: la
declaración es lo que un programa lee *antes* de llamar, y un verbo declarado
sólo-prosa es un verbo que no se llama. La lista de hardware de red existía y
era inalcanzable para lo único para lo que se expuso.

Lo que se corrigió, y lo que enseña, está en [[Estrategia-de-Pruebas]]: una
afirmación que un sistema hace sobre sí mismo se comprueba corriéndolo, no
leyendo los dos lados del código, porque el catálogo y el despacho son dos
archivos y cada uno concuerda consigo mismo. La etapa 22 ahora corre los catorce
verbos que aquí se pueden correr sin argumentos y compara el cable contra la
promesa, en las dos direcciones.

**Los que siguen sin segunda cara, y ahora la lista es exacta**: `disponibles`,
`instalar`, `modulos`, `correr`, `permisos`, `revertir`, `nucleo`, `discos`,
`instalar-en`, más `limpiar`, `salir` y `apagar`, que no tienen qué contestar.
Los seis primeros son los verbos de módulos, y son el pendiente que
[[Tareas-Pendientes]] nombra en el punto 4c.

### 2026-08-23 — Dos verbos que este catálogo no pidió, y por qué eso está bien

`encontrar` y `contenido` no son puntos de este catálogo: salen del punto 6 de
la terminal usable de [[Tareas-Pendientes]], que es la capa 1 de
[[Principio-Doble-Ruta]] y no la superficie del agente. Se anotan aquí de todas
formas porque **quedan expuestos en la cara estructurada**, con los mismos seis
campos de ventana que todo lo demás, y porque tocan C2 de frente.

La relación con C2 hay que decirla, porque un agente que la ignore paga los
cinco costos: `buscar` sigue siendo la mejor respuesta donde aplica —cinco
lenguajes, sobre un árbol indexado— y `contenido` es la que aplica en todo lo
demás. La regla de desempate no cambia: quien pueda usar el índice, que use el
índice; `contenido` es lo que había que construir para que *no poder* dejara de
significar *no hay respuesta*. Ver [[Busqueda]].

Y una consecuencia para este documento: **estar fuera del catálogo tampoco
prohíbe construir nada.** Lo que el catálogo decide es qué se construye *para el
LLM* y con qué criterio; un verbo que una persona necesita se construye porque
una persona lo necesita, y entonces nace con sus dos caras como todo lo demás.



### 2026-08-10 — El costo de equivocarse incluye la respuesta que nunca llega
Lo que construir enseñó y el decreto no anticipaba.

El cuarto costo estaba escrito como *qué cuesta un error y si se puede deshacer*,
y se leía como si los errores fueran acciones. Hay uno que no lo es: **una
pregunta que se contesta después de veinte minutos, o nunca.** Le cuesta al que
preguntó todo lo que tenía planeado y no le deja nada que deshacer.

`indexar` sin argumento indexa donde está parada la sesión, y una sesión empieza
en `/home`. En la máquina de Cesar eso incluye `.cargo/registry` y `.rustup`.
Corrió más de tres minutos y lo mataron. Ningún verbo estaba mal escrito: el
alcance por omisión heredó lo que había, y lo que había no era decisión de quien
lo escribió.

Se agrega al criterio: **un verbo cuyo alcance por omisión es «donde estás» lleva
un techo, y arriba del techo se niega diciendo qué hacer en su lugar.** Una
negativa inmediata con una salida es más barata que cualquier respuesta lenta,
porque el que pregunta puede actuar sobre ella. `indexar` se niega arriba de
20 000 archivos con `tree_too_large`, y no entra a carpetas ocultas.

### 2026-08-10 — Catorce de diecinueve, y dos «sólo hierro» que no lo eran
B1, C2, F2, D2 y B3 construidos. Los dos últimos estaban en la lista de lo que
no se podía comprobar aquí, y esa lista estaba mal por dos razones distintas —
una confusión sobre qué propiedad se prueba, y un hecho falso sobre dónde vive
el consumidor de un ringbuf. E1 queda, con una razón nueva: le falta G.

### 2026-08-09 — Nota creada
Decretada por Cesar el mismo día que la cara estructurada de los verbos de
archivos ([[Punto-Actual]]), que es el punto A2/C3 empezado y la razón de que la
pregunta se hiciera: al exponer la primera superficie quedó claro que no había
lista de cuáles eran las demás.

## Relacionado
- [[Filosofia-Fundacional]]
- [[Principio-Doble-Ruta]]
- [[FS-en-Grafo]]
- [[Permisos-JIT]]
- [[Journal-y-Snapshots]]
- [[Marcado-de-Origen]]
- [[Memoria-Persistente]]
- [[Criterio-de-Inclusion-de-Primitivas]]
- [[Tareas-Pendientes]]
