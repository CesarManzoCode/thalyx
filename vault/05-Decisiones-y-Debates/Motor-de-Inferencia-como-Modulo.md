---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-28
tags: [agente, modulos, inferencia, motor, decreto]
---

# El motor de inferencia es el primer módulo real

## Problema

Cesar arrancó la imagen el 2026-08-28, vio la pantalla, y preguntó qué le falta
al sistema para ser *«algo real que pueda usar durante un buen tiempo de
verdad»*.

La respuesta, medida contra el código y no contra esta bóveda, es que **una
máquina Thalyx arrancada no tiene agente. Ninguno.**

Y no por un pendiente: por cómo está construido. `crates/thalyx-agent/src/llama.rs`
arranca `llama.cpp` con `Command::new`, un binario aparte buscado en el `PATH`.
La imagen lleva `/init` y `/dev/console` y nada más — `crates/thalyx-cli/src/image.rs`,
y `make -C image count` lo dice en voz alta. Así que el motor **no existe en la
máquina que arranca, y no puede existir ahí** mientras la imagen sea el kernel y
un programa.

Todo lo del agente —el router, la gramática GBNF, la atribución, las tres gamas
medidas en [[Gamas-de-Modelo]]— ha corrido siempre en la máquina de desarrollo
de Cesar, nunca sobre Thalyx. Un sistema operativo donde la IA es ciudadana de
primera arrancó sin ella y nadie lo había escrito.

## Decreto

**El motor de inferencia es un módulo**, y es el primero real. Cesar,
2026-08-28.

Un binario estático, firmado, en el store, corriendo confinado bajo
`module_standard` como cualquier otro módulo. El `.gguf` llega al disco de store
desde la máquina de Cesar, por el camino que `make store-stage` y `sudo make
store` ya recorren para `greeter`.

### Por qué esto no contradice [[Filosofia-Fundacional]]

Porque **un módulo no está en la imagen**. Vive en el store, que es la
distinción entera entre lo que Thalyx *es* y lo que le han instalado — está
escrito así en el `Makefile` de la imagen desde que `greeter` existe. `make -C
image count` sigue diciendo uno.

Lo que sí sería una contradicción es lo que hay hoy: un `PATH` del que se saca
un programa. Un módulo se instala por el ciclo que Thalyx existe para tener
—manifiesto, firma, permisos, confinamiento— y un `Command::new("llama-completion")`
no pasa por ninguno de los cuatro.

### Y lo que compra además

El sistema de módulos deja de probarse contra `greeter`. Hoy el único módulo que
existe es un saludador de cuarenta líneas que lee un archivo; el ciclo completo
—instalar, permisos, correr confinado, revertir— nunca ha llevado nada que
pesara. El motor lo obliga a existir de verdad.

### Las alternativas, y por qué no

**La inferencia dentro de `thalyx`**, en Rust: cargador de GGUF y bucle de
transformer adentro del binario. Es la lectura más pura del decreto y es de
largo la más cara —semanas, sin BLAS— y su calidad habría que medirla contra
llama.cpp antes de creerle. No descartada para siempre; descartada para ahora.

**La red primero.** Sigue trabada en la pregunta de Fase 2 que nadie ha
contestado —de dónde vienen los módulos— y no compra nada que no compre ya
copiar al disco de store. Ver [[Red]].

## Lo que se midió antes de construir nada

Una pregunta podía matar el decreto: **un módulo corre bajo `module_standard`, y
si un motor real necesita algo que ese filtro niega, esto es inconstruible.** El
momento más barato de saberlo era antes de escribir la primera línea.

Se midió el 2026-08-28 con `dev/engine-needs.sh`, que es
`dev/foreign-agent-needs.sh` —la misma comparación, contra la misma lista, leída
del mismo cuerpo de función— apuntado a un motor en vez de a un agente. Una sola
comparación y no dos: dos serían dos respuestas a una pregunta.

El motor es `llama-completion` de llama.cpp, que es el que `llama.rs` ya maneja.
El modelo lo escribe `dev/tiny-model.py` con el propio `gguf-py` de llama.cpp,
porque un archivo inventado aquí probaría que llama.cpp acepta lo que su autor
cree que es GGUF (regla 6, al revés). Dos capas y 64 dimensiones: la pregunta es
*cuáles* llamadas hace un motor, y un modelo de dos capas hace las mismas que uno
de setenta mil millones de parámetros.

### El resultado

**31 llamadas al sistema distintas para cargar el modelo, tokenizar, correr el
grafo y generar. Las 31 ya están permitidas. Ninguna falta.**

El confinamiento que ya existe es suficientemente ancho para un motor de
inferencia, y eso no se sabía.

De las 13 rutas que abre, 9 caen dentro de lo que un módulo ve. Las otras cuatro:

| Ruta | Qué es |
|---|---|
| el `.gguf` | los datos del módulo, concedidos por su manifiesto — es lo que `greeter` ya hace con `notes.txt` |
| `/sys/devices/system/cpu/online` | contar núcleos |
| `/sys/devices/system/cpu/possible` | lo mismo |
| `/dev/tty` | detectar terminal; con la salida redirigida no la necesita |

Un rastreo dice qué pasó, no qué hace falta. Que abrió esas tres no dice que las
necesite.

### Y lo que la medición NO contesta, que es la mitad cara

**El tamaño, que es el hueco de verdad.** El modelo medido pesa menos de un
megabyte. `module_standard` topa un módulo en **1 GiB** —`profile.rs`,
`memory_max`, con un comentario que llama al número «una perilla de política, no
una decisión arquitectónica»— y **ningún manifiesto puede pedir más**. Los pesos
por `mmap` son caché de página reclamable y se degradarían a golpes de disco
antes que morir, pero el KV cache y los búferes de cómputo son memoria anónima y
sí topan. Ese número es la pregunta abierta, y es de Cesar: es política y cuesta
su hierro.

**El libc.** El motor medido está ligado contra glibc y el que embarcaría es
musl estático. Regla 12: una compilación con otra configuración es otro sistema,
y el arranque es exactamente donde los dos libc difieren. La medición contra
musl no se puede hacer en este contenedor —no hay objetivo de rustup ni
compilador de C para musl— y está pendiente.

**Una gama de verdad.** [[Gamas-de-Modelo]] ya registra que la gama máxima muere
por falta de memoria en la máquina de 16 GB de Cesar, sin confinamiento de por
medio. Bajo un tope de 1 GiB la pregunta no es si cabe la máxima; es cuál cabe.

## Lo que falta para construirlo

Nada de esto está hecho. Se escribe aquí para que la siguiente sesión no lo
vuelva a derivar.

1. **Decidir el tope de memoria de un módulo**, o que un manifiesto pueda
   pedirlo. Es de Cesar y bloquea todo lo demás.
2. **Compilar `llama-completion` estático contra musl** y volver a medir. Regla
   12.
3. **El manifiesto del motor**: qué permisos declara, y por qué ruta ve su
   `.gguf`.
4. **Que el agente encuentre el motor como módulo instalado** en vez de por
   `PATH` — `config.rs` y `llama.rs`, donde `binary` es un `PathBuf` que hoy se
   resuelve contra el entorno.
5. **Que el `.gguf` llegue al disco de store**, por donde `greeter` ya llega.
6. **Una etapa en `dev/verify.sh`.**

## Relacionado

- [[Agente-Conversacional]] — qué es el agente, y que está fuera de la TCB
- [[Gamas-de-Modelo]] — las cuatro gamas y lo que cada una cuesta
- [[Sistema-de-Modulos]] — el ciclo que esto obliga a existir de verdad
- [[Sandbox-Ejecucion]] — `module_standard`, que resultó ser suficiente
- [[Programas-Ajenos]] — `ejecutar`, el otro camino para un binario que no es Thalyx
- [[Filosofia-Fundacional]] — el kernel y un programa, que esto no toca
- [[Punto-Actual]] — dónde quedó todo
