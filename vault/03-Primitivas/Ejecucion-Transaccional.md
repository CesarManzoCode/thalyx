---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-08-29
tags: [primitiva, agentes, transaccion, rollback, validacion, fase-1]
---

# Ejecución transaccional: `hacer`

## Función

Que **una sola decisión del modelo pueda causar muchas acciones deterministas de
la máquina, con seguridad transaccional, sin otra decisión del modelo en medio.**

## La hipótesis de la que sale

Está escrita entera en [[Trabajo-Entre-Inferencias]]. En corto: cada respuesta
que Thalyx le da a un agente frontera es **otro paso de inferencia**, y un paso
de inferencia arrastra la conversación completa. Las trazas reales de
[[Evidencia-de-Agentes]] dicen que la mayoría de las llamadas que gastó una tarea
reversible no llevaban ninguna decisión: abrir la frontera, preguntar qué
cambió, notar que un rename no dejó nada atrás, deshacerlo.

Todo eso una máquina determinista lo puede hacer sin preguntarle a nadie, y hasta
ahora esta máquina obligaba al modelo a mirar.

## Qué es

Un verbo que toma un **programa**: varias peticiones, qué tiene que ser cierto
cuando terminen, y qué hacer si no lo es.

```
hacer {"label":"rename",
       "steps":[{"verb":"edit","arguments":["lib.rs","sustituir-lote","2",
                                            "UidRegistry","UserRegistry","main.rs"]},
                {"verb":"grep","arguments":["UserRegistry"]}],
       "validate":[{"check":"text","text":"UidRegistry","expect":"none"},
                   {"check":"parses"}]}
```

El ciclo, completo, dentro de una sola petición externa:

```
abrir la frontera reversible (snapshot)
  → ejecutar cada petición, en orden, parando en el primer rechazo
  → observar lo que de verdad cambió
  → correr las comprobaciones
  → confirmar, o devolver el árbol tal como estaba
  → registrar todo
  → contestar una vez, corto
```

## Las cuatro cosas que no es

Están escritas así porque cada una es una manera de que esto fuera trampa.

1. **No es un shell y no arranca uno.** La imagen carga el kernel de Linux y un
   programa; no hay a qué salirse. Lo que se compone son las peticiones propias
   de Thalyx, que es el sustrato que existe de verdad.
2. **No es una segunda autoridad.** Cada paso pasa por `external::one` — la misma
   función, la misma tabla, contra el mismo espacio de trabajo por la que pasa
   una petición suelta. Un programa alcanza exactamente la unión de lo que sus
   pasos hubieran alcanzado uno por uno. Si eso no fuera cierto, este verbo sería
   la API paralela que [[Agentes-Externos]] prohíbe, y la frontera valdría para
   una petición y no para treinta.
3. **No es una etiqueta alrededor de unas escrituras.** La frontera es
   [[Camino-Confiable|el intento]], que es un snapshot; el rollback es un
   restore; y la autorización es el testigo de [[Identidad-de-Estado]],
   comprobado bajo el candado.
4. **No es validación que siempre pasa.** Cada comprobación establece algo o
   contesta `not_proven`, y **`not_proven` nunca es `passed`**: una comprobación
   que no se pudo correr devuelve el trabajo en vez de confirmarlo.

## Las comprobaciones que existen hoy

| comprobación | qué establece | dónde corre |
|---|---|---|
| `text` | un texto está ausente del (o presente en el) espacio de trabajo | en cualquier máquina |
| `parses` | cada archivo fuente que cambió cierra sus llaves, sus cadenas y sus comentarios | en cualquier máquina |
| `rust` | `cargo check`/`test` sobre los paquetes a los que pertenecen los archivos que cambiaron | sólo donde el kernel deniega |
| `program` | una ruta absoluta corre confinada y sale con 0 | sólo donde el kernel deniega |

`text` es la post-condición de un rename, que es justo lo que un agente gasta una
ronda entera en contestar: buscar, leer la respuesta, decidir que está vacía.

`parses` es lo que rompe una edición mecánica — una sustitución que se comió una
llave, un bloque pegado un renglón más arriba. No es un compilador y se dice así.

`rust` decide el alcance **por paquete**, leyendo los manifiestos del disco.
Saber qué *pruebas* podría afectar un cambio necesita un grafo de llamadas que
sobreviva a macros, genéricos y objetos de trait, y equivocarse en la dirección
optimista es una corrida verde que no probó nada. El alcance que sale barato y
sano es el paquete. Leer un archivo no es correr un programa, así que una máquina
que no puede correr `cargo` todavía puede decir **cuál** paquete no comprobó.

`rust` y `program` corren por `thalyx_core::foreign` — el camino de `ejecutar`,
confinado igual: su propio usuario, su propio cgroup, su propia raíz, el filtro
seccomp y los permisos nombrados y nada más. **En un kernel que no deniega, se
rechazan**, y ese rechazo llega como `not_proven`.

## La respuesta es chica y la evidencia no

El punto de hacer treinta cosas localmente se pierde si la respuesta son treinta
respuestas. Así que vuelve la forma de lo que pasó —qué cambió, cómo salió cada
comprobación, si se confirmó— y el material crudo se queda en el store bajo un
identificador. `evidencia <id> [paso=N]` lo trae, y nada lo trae por omisión.

La evidencia vive **en el store y nunca en el espacio de trabajo**, y eso no es
orden: el espacio de trabajo es lo que un rollback reemplaza, así que evidencia
escrita ahí sería destruida por el mismo rollback del que es la explicación.

Nada se corta en silencio. Cada límite que se aplica dice que se aplicó.

## Qué decide Cesar y qué no

Esto **no** reemplaza a `intento`. El camino manual —`empezar`, `confirmar`,
`abandonar`— sigue entero y sigue expuesto, palabra por palabra. `hacer` es el
camino rápido para lo que ya está decidido, y las dos superficies conviven a
propósito hasta que haya evidencia de un banco real que diga cuál conviene.

Sí es una decisión suya, y está en [[Tareas-Pendientes]]: **si `program` con una
ruta arbitraria debe seguir en la lista de lo que un agente externo puede pedir.**
Se construyó porque él lo pidió en esta sesión, para que el mecanismo sirva fuera
de Rust; corre confinado como cualquier programa que nadie firmó, y aun así es la
primera vez que un agente externo puede causar que arranque un proceso.

## Dónde vive

- `thalyx-cli/src/exec.rs` — el programa, el ciclo, las comprobaciones, la
  evidencia y las dos caras.
- `thalyx-cli/src/external.rs` — `one`, la única puerta, ahora compartida.
- `thalyx-mcp` — `thalyx_exec` y `thalyx_evidence`.

## Qué falta comprobar en hardware

Aquí la frontera se ejercita contra el falso de directorios, que copia donde
Btrfs comparte bloques. Lo que sólo la máquina de Cesar establece es que la
frontera sea un snapshot **real** y que el rollback que el runtime decidió solo
devuelva de verdad el árbol: `dev/verify.sh`, etapa **56**.

Y `rust`/`program` no corren en ningún lado donde el kernel no deniegue, así que
aquí sólo dicen `NOT PROVEN`.
