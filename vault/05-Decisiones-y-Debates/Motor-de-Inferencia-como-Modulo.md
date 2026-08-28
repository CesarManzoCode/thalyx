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

> **Resuelto el 2026-08-28.** Lo de abajo es cómo estaba cuando se midió; la
> decisión de Cesar y lo construido están en «El tamaño, resuelto» más adelante.

**El tamaño, que era el hueco de verdad.** El modelo medido pesa menos de un
megabyte. `module_standard` topaba un módulo en **1 GiB** —`profile.rs`,
`memory_max`, con un comentario que llamaba al número «una perilla de política,
no una decisión arquitectónica»— y **ningún manifiesto podía pedir más**. Los pesos
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

## El tamaño, resuelto — 2026-08-28

**Decidido por Cesar el mismo día en que se midió**, preguntado con las
alternativas al lado —subirlo a 4 GiB, a 8 GiB, dejarlo en 1— y su respuesta fue
ninguna de esas: **lo que pida el manifiesto, aprobado por él al instalar.**

Lo que eso significa, y por qué no hizo falta maquinaria nueva: una petición de
memoria **es un permiso `persistent`**. Sale por el camino confiable que ya
existe, se guarda en el registro que ya existe, y `for_permissions` —que ya
ajustaba el perfil según lo concedido, para la red— sube el techo. El gigabyte
deja de ser el techo y pasa a ser el **piso**: un módulo que no pide nada sigue
teniendo exactamente eso.

Cuatro guardas, cada una por una forma distinta de salir mal:

- **Con unidad, nunca un número pelón.** `4GiB` y no `4294967296`. Un número así
  en una confirmación es un número que nadie puede verificar. Uno sin unidad se
  niega en vez de adivinarse: leído como bytes confina en ocho bytes a un módulo
  que pidió ocho gigabytes; leído como gigabytes le entrega la máquina.
- **Nunca `jit`.** [[Tres-Tipos-de-Permiso]]: sólo `persistent` siempre exige un
  humano. El manifiesto se niega, no se sube el permiso en silencio.
- **Más de lo que la máquina tiene se niega antes de preguntar**, y negado en
  vez de recortado: un módulo confinado a menos de lo que pidió se muere en un
  límite que nunca aceptó.
- **Dos concesiones dan la mayor, nunca su suma.**

Con esto, **de la lista de abajo lo único que sigue abierto es construir el
módulo**. El confinamiento le alcanza (31 de 31), el techo ya se puede pedir, y
—esto es nuevo del mismo día— `ejecutar` ya se puede confirmar **desde la
pantalla**, que era el hueco por el que el motor no se podía ni arrancar desde
la cara con la que la máquina viene. Ver [[Punto-Actual]].

Lo que sigue sin contestarse es el libc: la medición contra musl estático. Ese
contenedor sí puede compilar contra musl desde el 2026-08-28 (`musl-tools`),
así que dejó de ser imposible ahí — pero medir un motor no es compilarlo.

## Construido — 2026-08-28, el mismo día

Los seis puntos que esta nota dejaba abiertos están cerrados. Aquí queda **cómo**
quedaron, porque el cómo es lo que la siguiente sesión necesita y lo que un
`git log` no explica.

### 1 · El tope de memoria

Cerrado antes que el resto: lo pide el manifiesto y lo aprueba Cesar al
instalar. El manifiesto del motor pide `4GiB` — no el piso de 1 GiB, porque
`module_standard` carga la caché de página al cgroup del módulo y los pesos van
mapeados: la gama ligera son ~1.1 GB antes de cualquier contexto, así que el
piso lo mataría a media carga.

### 2 · El binario, estático

`dev/build-engine.sh`, contra el tag `b10665` de llama.cpp, fijado. Tres
banderas que no son opcionales y que se encontraron con tres enlaces fallidos,
en este orden:

| Bandera | Qué falla sin ella |
|---|---|
| `-DLLAMA_OPENSSL=OFF` | `cpp-httplib` encuentra el OpenSSL del sistema, que en la mayoría de las distribuciones sólo existe como `.so`: *«attempted static link of dynamic object»* |
| `-DGGML_OPENMP=OFF` | `libgomp` igual |
| `-DGGML_NATIVE=OFF` | La máquina que construye el store no es necesariamente la que lo arranca. `-march=native` dentro de un módulo es una instrucción ilegal en el CPU de otro, y llega como un módulo que muere sin decir nada |

Lo único que el script se niega a terminar sin comprobar es un ELF **sin INTERP
y sin NEEDED**, y son dos preguntas y no una: un objeto compartido tampoco tiene
INTERP, y `file` dice «statically linked» de los dos. Adentro de Thalyx no hay
libc y no hay cargador dinámico, así que un motor dinámico funciona perfecto en
el contenedor y muere en `execve` en la máquina — regla 12.

**No es musl.** Es glibc enlazada estáticamente, y la diferencia importa menos
de lo que parece: lo que la máquina necesita es que no haya nada que cargar, y
eso se comprueba en vez de suponerse. Un `x86_64-linux-musl-g++` no existe en
Fedora ni en el contenedor, y el script toma `CXX` del entorno, así que quien
tenga uno lo usa sin tocar nada.

### 3 · El manifiesto

Dos directorios y un techo:

```toml
[[permissions]]              # los pesos
resource = "/opt/thalyx/data/engine/models"
[[permissions]]              # el prompt y la gramática de una inferencia
resource = "/opt/thalyx/data/engine/run"
[[permissions]]
resource = "memory"
action   = "4GiB"
```

Directorios y no archivos, por dos razones distintas. `run` lleva un directorio
desechable por inferencia —el prompt y la gramática de *esa* respuesta,
nombrados con su marcador— así que no hay un archivo fijo que nombrar. `models`
es un directorio para que cambiar el modelo sea copiar un archivo al store, que
es lo que Cesar pidió: cambiar de modelo no puede querer decir recompilar
Thalyx.

Las rutas son absolutas y de adentro de la máquina, porque una concesión es una
ruta que el módulo verá: `RootFs` monta lo concedido **con el nombre que ya
tiene**. Están escritas dos veces —en `dev/stage-engine.sh` y en
`crates/thalyx-cli/src/engine_module.rs`— y ésa es la única duplicación que
quedó.

### 4 · Cómo lo encuentra el agente

Una costura angosta en `llama.rs`:

```rust
pub trait Engine {
    fn describe(&self) -> PathBuf;
    fn preflight(&self) -> Result<(), LlamaError>;
    fn scratch_root(&self) -> Option<PathBuf>;
    fn complete(&self, call: EngineCall<'_>) -> Result<EngineRun, LlamaError>;
}
```

Entra un vector de argumentos, salen bytes. **Arriba de esa línea no cambió
nada**: el prompt, el marcador, la gramática, dónde termina una respuesta, qué
es una respuesta rota, la atribución, el contrato, el router. Abajo hay dos
implementaciones y difieren en una sola cosa — quién arranca el proceso:

- `ProcessEngine` lo arranca aquí, con `Command`. Es lo que usa `thalyx agent
  bench` en una máquina de desarrollo, donde hay un `llama.cpp` en el `PATH`.
- `ModuleEngine` (`crates/thalyx-cli/src/engine_module.rs`) lo corre por
  `thalyx_core::run`, el mismo lanzador de `correr`. Vive en el CLI y no en
  `thalyx-agent` a propósito: el crate del agente es donde se parsea lo que dijo
  un modelo, y no debe poder arrancar procesos confinados.

Cuál de los dos lo dice el archivo de configuración, con un campo nuevo:
`engine_module = "dev.thalyx.engine"`. Con `#[serde(default)]`, porque una
máquina que hay que reconfigurar porque Thalyx aprendió un campo es una máquina
que perdió una decisión de un humano en una actualización.

**`scratch_root` es la parte con filo.** Un módulo sólo ve lo que le
concedieron, así que el prompt tiene que escribirse *dentro* de uno de esos
directorios. Antes iba a un `tempdir()` del sistema — que adentro del sandbox no
existe — y lo que vuelve de eso es «llama.cpp no completó el prompt», culpando a
llama.cpp de un error de Thalyx. Hay una prueba que sólo falla si eso se rompe,
y su control al lado.

### 5 · Cómo llega el `.gguf` al disco

`dev/stage-engine.sh`, llamado por `make -C image store-stage`:

```
make -C image engine                                   # construye llama.cpp
make -C image store-stage MODEL=/ruta/al/modelo.gguf   # motor + pesos + gama
sudo make -C image store
```

Tres cosas van al stage, y la tercera es la que lo vuelve usable en vez de
solamente equipado: los pesos, el motor **instalado**, y la elección de gama
escrita. Instalado —donde `greeter` se deja sin instalar a propósito— porque son
requisitos contrarios: `greeter` es el paso 2 de [[Criterio-de-Salida-Fase-1]],
una persona instalando un módulo firmado, y una máquina que arrancara con él ya
puesto haría ese paso irrealizable. El motor es lo opuesto: que la máquina
arranque **pudiendo ser hablada**, sin comandos entre el botón y la primera
frase. El `.thmod` se queda además en el repositorio del store, así que se puede
reinstalar a mano.

Sin `MODEL`, el stage no falla: dice qué pieza falta y qué la arregla, y la
máquina arranca sin agente — que es un estado soportado
([[Principio-Doble-Ruta]]) y no una avería. Lo que no puede pasar es que se
quede callado: una máquina sin agente porque `MODEL` estaba mal escrito se ve
idéntica a una construida así a propósito.

### 6 · La etapa de `verify.sh`

§45. Empaqueta un `llama-completion` real y un GGUF real, lo instala, y corre
una inferencia **por el sistema de módulos**. Prueba confinado primero y sólo
cae a `--unconfined` si el mapa de política no está cargado, diciéndolo —
regla 3. Dos variables la alimentan (`THALYX_ENGINE`, `THALYX_ENGINE_MODEL`) y
`THALYX_REQUIRE_ENGINE_TESTS=1` convierte su silencio en falla.

`THALYX_ENGINE_DATA` mueve los directorios concedidos fuera de `/opt/thalyx`,
porque en la máquina de Cesar ésa es una instalación de verdad y una etapa que
creara directorios adentro sería la regla 11.

### Y el último eslabón, que no estaba en la lista

Una frase que no es un verbo, escrita en la sesión, **no llegaba a ninguna
parte**: contestaba *«no tengo modelo cargado»*. Ahora va al agente, y lo que
vuelve se convierte en **una línea del vocabulario de la sesión** que el mismo
dispatch corre — no en una llamada directa a nada.

Esa forma es la que hace que un modelo no pueda alcanzar una operación que una
persona no pueda, ni saltarse una confirmación, ni inventar un verbo: un nombre
que el dispatch no tiene simplemente no corre. Y un solo salto: lo que produjo
el modelo no vuelve a consultar al modelo, o un modelo que repite una propuesta
que nadie reconoce gira para siempre gastando una inferencia por vuelta.

## Revisión — 2026-08-28, el mismo día: el motor se queda vivo

Lo escrito arriba sobre *un proceso por respuesta* dejó de ser cierto horas
después de arrancar. `llama-completion` es de una sola respuesta por
construcción, así que la segunda frase volvía a leer el GGUF entero: la mayor
parte de lo que cuesta un modelo local, gastada otra vez en trabajo ya hecho.

**Nada de este decreto cambia.** El motor sigue siendo un módulo firmado,
instalado, corrido por `thalyx_core::run` bajo `module_standard`, con su uid, su
cgroup, su seccomp y su raíz pivotada. Lo que cambia es la forma del programa
que hay adentro: carga los pesos una vez y contesta peticiones por una tubería
hasta que Thalyx cierra el otro extremo. El binario se llama `thalyx-engine` y
lo compila el mismo `cmake`, en la misma etiqueta, con las mismas banderas.

Ver [[Motor-Residente]].

## Lo que sigue sin estar probado

- **Confinado de verdad.** El contenedor no tiene BPF LSM, así que lo corrido
  aquí fue `--unconfined` y el diario lo anotó degradado. §45 en la máquina de
  Cesar es lo que contesta.
- **Que un Qwen2.5 real acierte la intención.** El modelo de dos capas de
  `dev/tiny-model.py` produce un objeto gramatical y vacío, lo cual prueba toda
  la tubería y nada del modelo. `thalyx agent bench` es lo que mide eso.
- **La latencia desde la pantalla.** Contestado en parte por [[Motor-Residente]]:
  la carga de pesos ya no ocurre por frase, y la pantalla ya no se bloquea
  mientras se infiere. Lo que sigue sin número es cuánto tarda una inferencia
  tibia con un Qwen2.5-3B real dentro de la máquina.

## Relacionado

- [[Motor-Residente]] — la revisión del mismo día: los pesos se cargan una vez
- [[Agente-Conversacional]] — qué es el agente, y que está fuera de la TCB
- [[Gamas-de-Modelo]] — las cuatro gamas y lo que cada una cuesta
- [[Sistema-de-Modulos]] — el ciclo que esto obliga a existir de verdad
- [[Sandbox-Ejecucion]] — `module_standard`, que resultó ser suficiente
- [[Programas-Ajenos]] — `ejecutar`, el otro camino para un binario que no es Thalyx
- [[Filosofia-Fundacional]] — el kernel y un programa, que esto no toca
- [[Punto-Actual]] — dónde quedó todo
