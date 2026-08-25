---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-25
tags: [agente-ajeno, ejecutar, sandbox, g1, fase-2]
---

# Programas ajenos: `ejecutar`

Es **G1** de [[Superficie-para-el-LLM]], el punto que bloquea la vara del
proyecto desde que se midió el 2026-08-23 y que [[Que-Necesita-Un-Agente-Ajeno]]
dejó con nombre y sin ambigüedad:

> **G1, ejecutar un proceso arbitrario** — sigue entero. No es una llamada que
> falte ni una ruta: es que hoy `correr` sólo lanza módulos instalados y
> firmados, y un agente ajeno por definición no es ninguna de las dos cosas.

Cesar delegó la forma el 2026-08-25 —*«lo que veas conveniente que sea coherente
con nuestra filosofía»*—, así que esta nota es la forma y **la razón por la que
es coherente**.

## Por qué esto no contradice nada

[[Filosofia-Fundacional]] no lo permite: **lo exige**. La vara está escrita ahí
en las palabras de Cesar —*«un agente ajeno, ya escrito, corriendo sobre Thalyx,
y trabajando mejor que sobre Linux o macOS»*— junto con el estado real de
entonces, que sigue siendo el de hoy: *«hoy Claude Code no podría arrancar en
Thalyx»*. Un sistema que no puede lanzar un programa que no escribió él mismo no
llega a esa vara; ni siquiera llega a la línea de salida.

Lo que sí hay que cuidar es la firma, y se cuida **no tocándola**:

> Un programa ajeno **no es un módulo, y nunca se convierte en uno.**

La firma de [[Sistema-de-Modulos]] significa *alguien respondió por esto*. Si
Thalyx firmara al vuelo lo que se le pide ejecutar, pasaría a significar *esto
pasó por aquí*, que es dejar la palabra sin significado para quien lea la
siguiente — el mismo error que [[Superficie-para-el-LLM]] evitó el 2026-08-24 al
separar un contrato de un plan de verbo. Así que son dos verbos y no uno:

| | `correr <id>` | `ejecutar <ruta>` |
|---|---|---|
| qué lanza | un módulo instalado y firmado | un programa cualquiera |
| quién respondió por él | su publicador, con su llave | **nadie** |
| canal con la API de Thalyx | sí, nace con él | **no, nunca** |
| permisos | los que el store tiene concedidos | los que se nombran en el renglón |
| `sin-confinar` | existe, y queda en el journal como degradado | **no existe** |

## Las cinco decisiones, cada una con lo que evita

### 1. No hay canal, y ésa es la línea

Un módulo **nace sosteniendo** un canal a la API de Thalyx. Un programa ajeno no
recibe ninguno, y no porque no sepa hablarlo: porque la API es la superficie que
Thalyx le da a algo que fue firmado, instalado y al que un humano le concedió
permisos por su nombre. Un invitado corre; no se le da la casa.

Eso es también lo que impide que este verbo sea una puerta trasera al decreto de
firma. Por `ejecutar` no se instala nada, no se concede nada persistente, no se
toca el store y no se pide nada por el canal, porque no hay canal.

### 2. Se confina siempre, y aquí no hay modo degradado

`correr` tiene `sin-confinar`, y existe por una razón buena: un modo malo que se
alcanza a propósito y se nombra en el journal es mejor que uno que se alcanza por
accidente y no se nombra en ningún lado. Esa razón **no aplica aquí**. La
justificación de `sin-confinar` es que un humano leyó el manifiesto de ese módulo
y su publicador respondió por él; de un programa ajeno nadie respondió nada.

Así que `ejecutar` sin confinamiento **no es un modo que exista**. Si la máquina
no puede hacer cumplir nada, el verbo se niega y dice por qué, igual que `correr`
— [[Sandbox-Ejecucion]], falla cerrado.

### 3. Ve lo que se le nombró, y nada más

Dentro del pivote ve tres cosas:

- **su propia carpeta**, montada de sólo lectura en `/module`, que es donde el
  programa está;
- las rutas de sistema de sólo lectura que ya tiene cualquier módulo — `/usr`,
  `/lib`, `/lib64`, `/bin`, `/sbin`, `/etc` — que es lo que deja arrancar a un
  binario enlazado dinámicamente;
- **lo que se nombró en el renglón**, y sólo eso.

`leyendo <ruta>` y `escribiendo <ruta>` van adelante, como manda [[Palabras]], y
el sujeto es el programa y sus argumentos. Cada ruta concedida pasa por el
[[Camino-Confiable]] antes de que el programa exista: se muestran una por una,
dibujadas por Thalyx, y el silencio no es un sí.

**El grano es el que se nombró.** Conceder un archivo concede un archivo, no la
carpeta donde vive — que es la forma obvia de hacerlo funcionar y la que entrega
todo lo demás que hay en esa carpeta. Está probado en `isolation.rs` desde el
2026-08-25, con su control.

### 4. Su usuario es suyo, y es el mismo mañana

Un módulo recibe un uid asignado una vez y para siempre ([[Sandbox-Ejecucion]]).
Un programa ajeno no tiene id, así que la llave es **la ruta canónica del
binario**, con el prefijo `foreign:` para que no pueda chocar con el id de un
módulo.

La consecuencia es la que se quiere: el mismo programa es el mismo usuario entre
corridas, así que lo que escribió ayer sigue siendo suyo hoy, y **dos programas
ajenos distintos no comparten usuario** aunque los lance la misma persona el
mismo día.

### 5. El journal lo distingue de lo demás

La operación se llama `run_foreign` y no `run_module`, y lleva la ruta, las
concesiones y el código de salida. [[Marcado-de-Origen]] pide poder separar lo
que hizo el agente de lo que ya estaba; esto es la mitad más gruesa de esa
pregunta: separar **lo que hizo un programa que nadie firmó** de lo que hizo
Thalyx.

## Lo que este decreto no autoriza

- **No abre la red.** Es `G3` y sigue entera. Un programa ajeno arranca sin red,
  como cualquier módulo sin la concesión.
- **No es E1.** Una tarea con identidad y **concesión que expira** sigue sin
  construirse; lo de aquí son concesiones para una corrida, que terminan cuando
  el proceso termina.
- **No resuelve `G2`.** La imagen sigue llevando el kernel y un programa, y
  dentro de ella no hay libc: `ejecutar` sirve donde hay rutas de sistema que
  montar —la máquina de desarrollo—, y dentro de la imagen instalada sirve para
  lo que esté enlazado estáticamente. La pregunta del ABI sigue abierta en
  [[Tareas-Pendientes]] y esta nota no la contesta.
- **No le quita nada a `correr`.** El decreto de firma sigue rigiendo los
  módulos, entero.
- **No baja el [[Principio-Doble-Ruta]]:** nace con las dos caras, y la humana no
  es la que se agrega después.

## Cómo se comprueba

Etapa 36 de `verify.sh`, y una prueba de integración que **lanza un programa de
verdad** y le pregunta a él qué ve, con la columna de afuera al lado — regla 2 de
[[Estrategia-de-Pruebas]]. Lo que hay que comprobar:

1. un programa que nadie firmó corre, y su código de salida llega;
2. dentro no ve nada del anfitrión que no se le haya nombrado, y sí ve lo
   nombrado;
3. una ruta concedida para leer no se puede escribir;
4. sin confirmación no corre nada, y el silencio no es un sí;
5. el journal lo llama `run_foreign` y no `run_module`;
6. sin nada que haga cumplir la política, el verbo se niega;
7. **con la política cargada pero en modo observación, el verbo también se
   niega** — ver la revisión de abajo.

## Revisiones

### 2026-08-25 — «cargado» y «negando» son dos preguntas

**Qué decía antes.** El decreto decía *«si la máquina no puede hacer cumplir la
política, el verbo se niega»*, y el código preguntaba eso con
`policies.is_available()` — que responde **si el mapa de políticas se abre**.

**Qué pasó.** Cesar corrió `ejecutar /usr/bin/node --version` justo después de
`verify.sh`, que desengancha el LSM al salir, y leyó la negativa correcta. El
remedio que le dio esa negativa es `make -C lsm load`. Y `make -C lsm load`
**aterriza a propósito en modo observación**: los ganchos corren, cada negación
se escribe en el anillo, y ninguna se aplica.

O sea que el remedio del mensaje dejaba la máquina en el estado exacto donde el
verbo *sí* arrancaba al invitado y el kernel no le negaba nada. Nadie en el lado
de Rust había leído nunca el mapa `thalyx_enforcing`; sólo el `Makefile` lo
consultaba, con `bpftool`. `thalyx enforce status` imprimía «kernel policy map:
present» y se callaba.

**Qué dice ahora.** Son dos preguntas y se hacen las dos:

| | módulo firmado | programa ajeno |
|---|---|---|
| mapa sin cargar | se niega, ofrece `sin-confinar` | **se niega**, no hay a qué caer |
| cargado, observando | **corre degradado, y el journal lo dice** | **se niega**: `make -C lsm enforce` |
| no se pudo leer el modo | corre degradado, y el journal lo dice | **se niega**: regla 9 |
| cargado, negando | corre | corre |

La asimetría es la misma del resto de la nota, y por la misma razón. A un módulo
lo firmó alguien y un humano leyó su manifiesto; correr degradado con el journal
diciéndolo es una decisión que alguien puede auditar. Detrás de un invitado no
hay nadie: el confinamiento es *todo* lo que lo respalda, y un confinamiento que
no niega no es un confinamiento.

**Lo que esto abrió.** Cambiar el modo todavía se hace con `bpftool`, que la
imagen no tiene y no va a tener. Queda escrito en [[Tareas-Pendientes]].

## Relacionado
- [[Superficie-para-el-LLM]]
- [[Que-Necesita-Un-Agente-Ajeno]]
- [[Sandbox-Ejecucion]]
- [[Sistema-de-Modulos]]
- [[Camino-Confiable]]
- [[Filosofia-Fundacional]]
- [[Palabras]]
