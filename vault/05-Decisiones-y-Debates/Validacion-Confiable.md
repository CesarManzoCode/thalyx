---
tipo: propuesta
estado: propuesto
fecha-propuesta: 2026-08-29
tags: [agentes, validacion, sandbox, seguridad, propuesta]
---

# `validar`: cerrar el ciclo del agente sin darle una shell

> **Esto no está decretado.** Es una propuesta escrita después de medir el
> costo real de implementarla, para que la próxima sesión no vuelva a
> diseñarla desde cero. Lo que decide qué se construye es Cesar.

## El cuello de botella

Un agente externo hoy puede **entender → leer → modificar**. No puede completar

```
entender → modificar → compilar/probar → ver el fallo → corregir
```

y ése es el ciclo entero de programar. Un agente que cambia código y no puede
saber si compila trabaja a ciegas: propone, y la comprobación la hace un humano
en otra ventana. Todo lo que [[Agentes-Externos]] mide —índice, frontera
reversible, costo por respuesta— mejora *la mitad del ciclo que ya existía*.
Esta es la mitad que falta.

## Lo que no puede ser

**No una shell.** `nothing_that_changes_the_machine_rather_than_the_workspace_is_exposed`
es un test asertado, y la lista `EXPOSED` de `external.rs` es la afirmación
«éstos y sólo éstos». Un verbo que corre lo que el modelo escriba es la lista
entera anulada por una entrada.

**No un CI, no un gestor de paquetes, no toolchains generales.** El
[[Filosofia-Fundacional|decreto de la imagen]] es el kernel y un programa; un
compilador vive en el store como módulo o no vive.

## La forma propuesta

`validar <objetivo>`, donde **los objetivos no los inventa el modelo**: los lee
de una declaración, y lo que la declaración puede decir es deliberadamente
pobre.

```toml
# .thalyx/validate.toml, en la raíz del espacio de trabajo
[[objetivo]]
nombre = "check"
modulo = "org.rust.cargo"

[[objetivo]]
nombre = "unit"
modulo = "org.rust.cargo"
```

`validar check` arranca el módulo `org.rust.cargo` con un solo argumento: la
palabra `check`. **La declaración no compone una línea de comandos** — no puede
nombrar un programa, no puede pasar banderas, no puede encadenar nada. Nombra un
módulo y una palabra. Qué significa esa palabra lo decide el módulo, que está
firmado e instalado.

### Las dos llaves

El archivo vive adentro del espacio de trabajo, que es lo que el agente puede
escribir. Por sí solo eso sería «el agente elige qué módulo correr», que es una
capacidad nueva y no pequeña. Así que hacen falta dos llaves y sólo una es del
proyecto:

- **el proyecto** dice qué objetivos tiene y qué módulo los atiende;
- **el humano** decide qué módulos existen en la máquina. Instalar no está
  expuesto, y un objetivo que nombra un módulo que no está instalado se rechaza
  diciendo cuál falta —una respuesta útil, no un fallo silencioso.

Y una restricción más, para que la primera llave no abra sola: un módulo sólo es
un destino válido de `validar` si **su propio manifiesto lo declara**
(`validador = true`, o el permiso equivalente). Un proyecto no puede apuntar
`validar` a un módulo que nunca se ofreció para eso.

## Lo que hace falta construir, medido

Lo caro no es el verbo. Es que **el validador ejecuta código del espacio de
trabajo** —`build.rs`, macros procedurales, las pruebas mismas— y eso es
exactamente lo que hay que contener. Cuatro piezas, en orden de costo:

1. **Un perfil `validator` con su propia lista blanca de seccomp.** La de
   `module_standard` tiene 36 entradas y está afinada para un módulo que habla
   por un socket. `rustc` no cabe ahí ni de lejos, y adivinar la lista es
   exactamente el error que `dev/foreign-agent-needs.sh` existe para no cometer:
   se deriva corriendo la herramienta real y leyendo lo que pidió, no
   escribiéndola a mano.
2. **Una forma de raíz que incluya un toolchain.** `rootfs.rs` ya monta `/usr`,
   `/lib`, `/bin`, `/etc` de sólo lectura, así que un compilador en `/usr/bin`
   ya se vería. Lo que falta es lo que vive en el home —`~/.cargo`, `~/.rustup`—
   y decidir si eso se concede o si el módulo trae lo suyo.
3. **Una decisión sobre `target/`.** Compilar escribe, y escribe mucho. O el
   espacio de trabajo se monta escribible —y entonces el validador puede cambiar
   el código que estaba validando, que es una propiedad que el benchmark
   reversible mide— o `target/` se monta aparte, fuera del árbol que
   `tree_hash` compara. **La segunda.** Ninguna de las dos es gratis y la
   primera arruina el instrumento.
4. **Red según política explícita.** Descargar dependencias es red, y una
   validación que puede alcanzar la red desde adentro del sandbox es una
   capacidad que el humano tiene que haber concedido. Por omisión: sin red, y un
   fallo por dependencia faltante que **dice** que fue por eso.

Todo lo demás ya existe y no se vuelve a escribir: `Confinement::establish`, el
cgroup, los límites, el uid por módulo, el journal, la captura acotada de
stdout/stderr, y `machine::answer` para la respuesta estructurada.

## La respuesta

Un objeto, como todo lo demás:

```json
{"op":"validate","ok":true,"target":"check","module":"org.rust.cargo",
 "exit_code":0,"timed_out":false,
 "stdout":"…","stderr":"…","output_truncated":false}
```

`ok` es «la validación corrió», no «pasó». Un `exit_code` distinto de cero es un
resultado y no un error del verbo: el agente **necesita** ver el fallo, ése es
el punto entero. Y la salida va acotada por bytes, con `output_truncated` dicho
en vez de callado — regla 10.

## El orden de construcción

1. El perfil `validator` y su lista blanca derivada de una corrida real. Es lo
   único que no se puede apurar y lo único cuyo error es silencioso.
2. El verbo `validar` en la API nativa, con la declaración y las dos llaves.
3. `EXPOSED` gana una entrada, y la línea de `Agentes-Externos.md` que dice qué
   puede un agente externo cambia — **con revisión fechada**, porque es una
   frontera decretada.
4. MCP gana una herramienta que adapta el verbo. Nunca antes de que el verbo
   exista: `CLAUDE.md` dice que MCP es adaptador y no la API interna.

## Lo que esto todavía no da

Un agente que corrige un fallo de compilación y vuelve a validar hace *dos*
corridas del compilador por iteración, en un sandbox que arranca de cero cada
vez. Si eso resulta caro, la respuesta es la misma que dio
[[Motor-Residente]] para el motor —un proceso que se queda vivo— y no una
excepción a la contención. Escrito aquí para que cuando se note no se resuelva
aflojando el sandbox.

## Relacionado

- [[Agentes-Externos]] — qué mide el benchmark y qué puede un agente hoy
- [[Sandbox-Ejecucion]] — el perfil, la raíz, el orden de arranque
- [[Superficie-para-el-LLM]] — por qué los verbos son pocos y explícitos
