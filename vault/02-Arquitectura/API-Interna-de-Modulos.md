---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-03
tags: [arquitectura, api, modulos, core, fase-1]
---

# La API interna de módulos

La superficie por la que un módulo habla con Thalyx, y la única que tiene.
[[Core-Nucleo]] la nombra desde el 31 de julio y la dejó explícitamente sin
diseñar. Esta nota es ese diseño.

## Por qué existe, y por qué no existía

Mientras hubo un userland POSIX debajo, esta API podía no existir sin que se
notara: un módulo era un script, y hablaba con `sh`. Al quitar la distribución
([[Construccion-del-ISO]]) desapareció el interlocutor. **Sin shell y sin
utilidades, un módulo no tiene con quién hablar excepto Thalyx.**

Eso la convierte en la pieza que decide el criterio de
[[Decision-Capa-vs-SO-Nuevo]]: un programa escrito contra esta API **no corre en
ningún otro lado**, porque en ningún otro lado hay nada al otro extremo.

## El canal: un descriptor que ya está abierto

**Thalyx le entrega al módulo un socket ya abierto, en el descriptor 3, en el
momento de ejecutarlo.** El módulo no abre nada, no busca nada y no nombra nada.

Tres motivos, en orden de peso:

1. **No hay ruta que equivocar.** Un socket en `/run/thalyx.sock` es un nombre,
   y un nombre puede resolver a otra cosa. Un descriptor heredado no se resuelve:
   ya está conectado a Thalyx antes de que exista una sola instrucción del
   módulo.
2. **Sobrevive a la raíz vacía.** El sandbox hace `pivot_root` a un árbol que
   contiene únicamente lo que los permisos del módulo conceden
   ([[Sandbox-Ejecucion]]). Meter un socket ahí obligaría a agujerear esa raíz
   en todos los módulos, incluido el que no tiene ningún permiso de archivos.
3. **La ausencia es la prueba.** Un binario de módulo ejecutado fuera de Thalyx
   no encuentra el descriptor y no tiene sistema. No falla por una comprobación
   que alguien pueda quitar: falla porque no hay nadie al otro lado.

El descriptor se crea como un par de sockets antes del primer `exec`, y viaja
por las dos etapas del lanzamiento ([[Sandbox-Ejecucion]]) hasta quedar en el 3.
Thalyx conserva el otro extremo, y **cerrarlo es cómo se corta el acceso de un
módulo sin matarlo**.

## El mensaje: longitud explícita y CBOR

Cada mensaje es una longitud de 32 bits sin signo, en little-endian, seguida de
exactamente esos bytes de CBOR.

**La longitud va por fuera** para que leer un mensaje no requiera entender el
mensaje. Un marco corrupto se detecta y se cierra la conexión sin haber
interpretado nada de su contenido — que es la regla 9 de `CLAUDE.md` aplicada al
transporte.

**CBOR y no JSON** por el mismo motivo por el que
[[Formato-Manifiesto-Thmod]] descartó YAML: un canal que gobierna permisos no
puede tener conversiones implícitas de tipo. Los números de JSON son ambiguos
sobre entero contra flotante, y esta API transporta tamaños, modos y
descriptores. CBOR distingue los tipos en el propio byte.

**Y hay un techo.** Un marco declara su tamaño antes de que nadie lo haya
leído, así que un tamaño absurdo es la forma barata de agotar la memoria de
Thalyx desde un módulo. El límite es parte del protocolo, no del que llama.

## Qué puede hacer un módulo en la versión 1

Tres familias. Nada más, y la lista corta es deliberada: cada operación de esta
API es superficie de ataque permanente, y es más fácil agregar una que quitarla.

| Familia | Qué hace | Contra qué se comprueba |
|---|---|---|
| Archivos | Leer y escribir dentro del árbol concedido | Los permisos del manifiesto, resueltos por Thalyx |
| Notificar | Mostrarle algo al humano | Que el módulo esté vivo y sea quien dice |
| Identidad | Preguntar quién es y qué permisos tiene | Nada: es información que Thalyx ya tiene sobre él |

**La comprobación la hace Thalyx, no el módulo.** El módulo pide una ruta;
Thalyx decide. Que la raíz del sandbox ya no contenga lo prohibido no releva de
comprobar: dos mecanismos que dicen lo mismo es el diseño
([[Sandbox-Ejecucion]]), y uno que confía en el otro no es un mecanismo.

**Identidad es de solo lectura y no la escribe el módulo.** Un módulo no dice
quién es; pregunta. Lo que Thalyx contesta viene del manifiesto verificado y del
cgroup en el que corre, que son las dos cosas que el módulo no puede falsificar.

## Lo que queda fuera de la v1, y por qué

**Pedir permisos en caliente.** Es el corazón de [[Permisos-JIT]] y llega, pero
abrir la superficie más delicada del sistema en la primera versión de un
protocolo nuevo es cambiar dos cosas a la vez. Cuando llegue, la solicitud pasa
por el [[Camino-Confiable]] y el módulo nunca ve la confirmación, solo el
resultado.

**Acceder a hardware.** [[Core-Nucleo]] lo lista como capacidad de la API. No es
una operación: es un decreto entero sin escribir, y no existe todavía ningún
caso que lo necesite. Queda en [[Tareas-Pendientes]].

## El decreto que este texto invalida

[[Core-Nucleo]] lista **"ejecutar comandos"** entre las capacidades de esta API.

**No puede existir.** No hay comandos que ejecutar: no hay shell, no hay
utilidades, y no hay un segundo programa en la imagen que invocar
([[Filosofia-Fundacional]]). La frase es del 31 de julio, de cuando había un
userland POSIX debajo tapando el hueco, y sobrevivió al decreto que lo quitó.

Es la tercera vez que pasa lo mismo —el login en tty1, `bpftool`, y ahora
esto— y las tres tienen la forma que [[Estrategia-de-Pruebas]] describe: **una
capacidad que se apoyaba en la base envejeció callada cuando la base se cayó.**
Un módulo que necesite algo que hoy sería "ejecutar un comando" necesita, en
realidad, que esa cosa sea una operación de esta API o un módulo.

## Qué está construido

Todo lo de arriba, el 2026-08-03:

| Pieza | Dónde |
|---|---|
| Protocolo: marco, mensajes, cliente y servidor | `crates/thalyx-abi` |
| El servidor de Thalyx, contra los permisos reales | `crates/thalyx-core/api.rs` |
| El descriptor, colocado y heredado | `crates/thalyx-syscall` |
| El descriptor, por las dos etapas del sandbox | `crates/thalyx-sandbox/launch.rs` |
| El primer módulo escrito contra esto | `modules/dev.thalyx.greeter` |

**Lo que aprendió construirlo, y no estaba en este decreto:** el servidor no
está dentro del sandbox. Corre como Thalyx, fuera de los namespaces del módulo
y con el alcance de Thalyx, así que ni la raíz vacía ni el LSM protegen nada de
lo que pasa por aquí — un módulo que pide una ruta le está pidiendo a Thalyx que
la abra. De ahí que cada ruta se compruebe **dos veces**: por el nombre, y por
lo que el kernel resuelve. Sin la segunda, un symlink plantado dentro de un
directorio que el módulo puede escribir alcanza cualquier archivo de la máquina.
Es la única de estas defensas cuya ausencia habría sido explotable.

Sin comprobar todavía: la ruta confinada. El canal atraviesa dos `exec` y un
filtro seccomp, y eso necesita una máquina con cgroup v2 delegado y el LSM
cargado.

## Cómo se comprueba que sirve

La regla es la de [[Estrategia-de-Pruebas]]: una prueba de que el protocolo
codifica y decodifica **no** es una prueba de que un módulo puede hablar. Lo que
cierra esta pieza es un módulo real, compilado contra esta API, que corra dentro
del sandbox y consiga leer un archivo que sus permisos conceden — y que falle al
leer uno que no.

Y su control, sin el cual la denegación no significa nada: el mismo módulo, el
mismo archivo, con el permiso concedido.

## Relacionado
- [[Core-Nucleo]] — de donde sale, y a lo que corrige
- [[Sandbox-Ejecucion]] — el confinamiento por el que viaja el descriptor
- [[Permisos-JIT]] — lo que decide cada llamada
- [[Formato-Manifiesto-Thmod]] — de donde salen los permisos
- [[Decision-Capa-vs-SO-Nuevo]] — el criterio que esta pieza cumple
