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
| **G1** | Ejecutar procesos: lanzar, esperar, matar, ver la salida |
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
| A1 | Catálogo de verbos legible por máquina | **Hecho** — `describe`, 29 verbos |
| A2 | El error nombra su remedio | **Hecho** — `remedy` como palabra estable |
| A3 | El estado de la máquina en un objeto | **Hecho** — `estado`, con los tres estados de la regla 10 |
| B1 | Respuestas acotadas con total y cursor | Propuesto |
| B2 | Identidad exacta de lo leído | **Hecho** — `sha256` del archivo entero en cada lectura |
| B3 | Qué cambió desde X | Medio construido: el ringbuf existe y nadie lo consume. **Sólo hierro** |
| C1 | Índice semántico expuesto | **Hecho** — `indexar`, `depende`, `usan` |
| C2 | Búsqueda por símbolos | Medio construido: el parser existe |
| C3 | Vigencia en toda respuesta | **Hecho donde hay caché**: cada respuesta del índice trae `fresh` |
| D1 | Ensayo en todo verbo que cambia | **Hecho para los verbos de archivos** — `ensayo`; los otros cinco dicen que no pueden |
| D2 | El intento con nombre | Medio construido: snapshots y journal existen. **Sólo hierro** |
| D3 | Cada acción dice cómo se deshace | **Hecho** — `undo`, y `null` donde no hay vuelta |
| E1 | El agente ajeno como tarea con concesión | Construido para módulos, sin extender. **Sólo hierro** |
| E2 | Pedir permiso en caliente | Fuera de la v1 de la API por decreto |
| E3 | Procedencia de lo que hizo el agente | Construido para contratos, sin extender |
| F1 | Memoria persistente accesible | **Hecho** — `recuerdos`, con las tres listas separadas |
| F2 | Journal legible desde afuera | Propuesto: no es verbo de la sesión todavía |
| G1–G4 | El piso | No construido, y es lo que bloquea la vara |

**Lo que esa tabla decía el 2026-08-09 y ya no dice:** que seis de los
diecinueve puntos existían y no eran alcanzables por nadie. Cinco de esos seis
ya lo son. Lo que queda sin exponer se parte limpio en dos, y la línea entre las
dos partes **no es de dificultad, es de dónde se puede comprobar**:

- **B3, D2 y E1** necesitan BPF, Btrfs o cgroups delegados. El contenedor donde
  se escribe este proyecto **no tiene ninguno de los tres**, así que construirlos
  aquí produce código que nadie puede ejercer hasta que Cesar lo corra. Ver
  «Los tres que no se construyeron, y por qué» abajo.
- **B1, C2 y F2** se pueden comprobar aquí y no se construyeron por tiempo, no
  por riesgo.

## Los tres que no se construyeron, y por qué — 2026-08-09

Escrito porque el decreto pide que se diga cuándo se pierde demasiado, y aquí se
perdería:

**D2, el intento con nombre.** Es el punto de mayor valor que queda —
*«intenta esto y si sale mal deshazlo»*— y se apoya entero en snapshots de
Btrfs. El contenedor no tiene Btrfs, así que la única prueba posible aquí sería
contra un falso, y la **regla 8** dice que un falso tiene que modelar la
propiedad bajo prueba: un falso de un snapshot que no puede fallar como falla un
snapshot no es un falso, es otro sistema. Se construye cuando haya una corrida
en la máquina de Cesar dedicada a ejercerlo.

**B3, qué cambió desde X.** La mitad existe: `thalyx-watch` cuenta las
mutaciones en el kernel y el ringbuf `thalyx_mutations` dice cuáles. Consumirlo
es código BPF, y `CLAUDE.md` es explícito: un programa que falla el verificador
tumba al watcher entero, y no se apilan dos cambios sin verificar. Va solo, en
su propia corrida.

**E1, el agente ajeno como tarea con concesión.** Necesita el LSM y cgroups
delegados. Además es el punto donde una equivocación no cuesta una prueba roja,
cuesta una concesión mal puesta — y esto es lo único del catálogo que toca
seguridad. No se toca sin que Cesar lo decida aparte.



---

## Revisiones

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
