---
tipo: componente
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, sandbox, seguridad]
---

# Sandbox de ejecución de módulos

## Rol

Aísla y ejecuta el código del módulo en un entorno controlado. Es una de las [[Flujo-Canonico-Overview|9 piezas fijas]] del flujo canónico. **Está fuera de la TCB**: contiene, pero no se le cree — ver [[Modelo-de-Amenaza]].

## Decreto: el Sandbox es una pieza separada del Core

El Core valida el contrato, pero **no ejecuta el código del módulo**. El módulo se ejecuta en un entorno aislado que es una pieza independiente.

**Motivo:** si el Core ejecutara directamente el código del módulo, un bug o un ataque en el módulo podría comprometer el Core. Separarlos es una decisión de seguridad, no de complejidad.

## Decreto: instalar no ejecuta código del módulo

En Fase 1, la instalación desempaqueta y valida estructura; no corre scripts del módulo. El código del módulo se ejecuta solo en runtime. Ver [[Verificacion-y-Distribucion]].

## Decreto: el Sandbox no toca el estado oficial del sistema

Produce su artefacto en el área de staging, nunca escribe directo al destino oficial. El Core verifica y publica. Ver [[Fase-Commit-Atomico]].

## Decreto: el Sandbox nunca toca el FS en grafo ni el Journal directamente

Solo el Core actualiza el [[FS-en-Grafo]] y escribe en el [[Journal-y-Snapshots]], después de recibir y verificar el resultado del Sandbox. Esto evita que un módulo comprometido dentro del sandbox pueda corromper el índice semántico o falsificar el journal.

## Mecanismo técnico (decretado)

**Implementación propia**, sin depender de un sandbox de terceros.

Perfil `module_standard`:

- **Namespaces:** usuario, montaje, PID, red, IPC y UTS.
- **Red:** denegada por defecto; solo se habilita con permiso explícito otorgado.
- **cgroup v2:** `memory.max`, `pids.max` y `cpu.max`.
- **seccomp:** filtro por lista de permitidos, no por lista de bloqueados.
- **Filesystem:** restricciones aplicadas por `thalyx-lsm` según los permisos otorgados.

El perfil se declara en el contrato y lo aplica el Core, nunca el módulo.

### Estado real: el perfil está construido menos el namespace de usuario

Namespaces de montaje, PID, IPC, UTS y red; filtro seccomp por lista de permitidos; límites de cgroup. Todo implementado en `crates/thalyx-sandbox` y verificado **preguntándole al módulo**, no al sistema:

```
                        adentro                             afuera
pid                     1                                   6878
hostname                thalyx-module                       vm
procesos visibles       4                                   82
interfaces de red       lo:                                 lo: eth0:
raíz                    bin dev etc lib lib64 module         todo el host
                        proc sbin tmp usr
escribir en /           denegado                            —
escribir en /usr        denegado                            —
escribir en /tmp        permitido                           —
ruta concedida          legible con su mismo nombre         —
ruta no concedida       no existe                           —
socket()                SIGSYS                              funciona
unshare()               SIGSYS                              funciona
```

Preguntarle al módulo y no a Thalyx es el punto. Toda la clase de defecto que este proyecto encuentra una y otra vez es el sistema reportando éxito de trabajo que no hizo.

### Decreto: un uid por módulo

**Cada módulo corre con su propio uid sin privilegios.** Los módulos se aíslan del sistema *y entre sí*: un módulo no puede leer los archivos ni los procesos de otro, aunque los dos estén instalados y confirmados.

Se evaluó y se descartó un uid compartido para todos: es más simple, pero deja que dos módulos se lean mutuamente lo que dejen por ahí, y **la confirmación del humano fue por módulo, no por el conjunto**.

**Implementado y verificado preguntándole al módulo:** imprime `700000` mientras el proceso que lo lanzó es root.

#### Los uid no se reutilizan nunca

Un uid liberado al desinstalar se **retira**, no se recicla. Un módulo deja archivos en lugares que Thalyx no rastrea —un directorio concedido, un archivo que sobrevivió a la ejecución— y esos archivos quedan a nombre del número, no del módulo. Dárselo después a otro módulo le entregaría en silencio todo lo que el anterior dejó.

La asignación es monótona, la marca de agua se persiste, y desinstalar quita la asignación sin bajar nunca el contador. Reinstalar el mismo módulo obtiene un número nuevo, por la misma razón.

#### Dónde y en qué orden

El uid se asigna **al instalar**, no en la primera ejecución: es parte de lo que el módulo *es*, y es lo que hace que tu confirmación aplique a ese módulo y a ningún otro.

El descenso ocurre en `init`, después de todo lo que necesitaba privilegio —los montajes, el hostname— y **antes** del filtro seccomp, que deniega `setuid` de plano para que el módulo no pueda hacerlo él mismo en ninguna de las dos direcciones. Grupos suplementarios primero, luego grupo, luego usuario: en cuanto el proceso deja de ser root ya no puede cambiar ninguno.

Se usa `setresuid` y no `setuid`: `setuid` desde root deja el *saved set-user-id* en cero, y un proceso con saved uid cero puede volver. Y el uid efectivo **se relee** después: una llamada que reportara éxito y dejara el proceso como root le entregaría todo al módulo, y todos los pasos siguientes se ven idénticos en los dos casos.

#### El costo, y cómo se paga: montajes idmapped

Un módulo que corre con su propio usuario **no puede tocar un directorio del humano** — ni escribirlo, ni leerlo si está en modo 0700. Cambiarle el dueño al directorio lo arreglaría y no es aceptable: Thalyx no reescribe el filesystem del humano para acomodarse.

La respuesta es un **montaje idmapped** (`mount_setattr` con `MOUNT_ATTR_IDMAP`). El módulo ve el directorio como propio; lo que escribe aterriza en disco a nombre de quien es dueño del directorio. En disco no cambia nada.

Medido: un directorio en modo 0700 de root, concedido para escritura, un módulo corriendo como 700000. La escritura funciona y el archivo que aparece en el host es de root.

**La dirección del mapeo es la contraria a la lectura obvia.** Una línea de `uid_map` es `<adentro> <afuera>`, y para un montaje idmapped el kernel trata el id **en disco** como el de adentro. La primera versión lo tenía invertido y *no falló*: montó limpio y el directorio apareció como propiedad de `nobody`, porque el id en disco no era un id válido de adentro en ese mapa. Ese es el fallo bueno — un id que no se puede traducir se vuelve nadie, y nadie no puede escribir nada.

Las concesiones de **lectura** se remapean igual que las de escritura, y después se remontan de solo lectura: remapear vuelve al módulo el dueño aparente, lo que si no le daría permiso de escritura que nadie concedió. Las rutas de sistema no se remapean: son legibles por todo el mundo por diseño.

##### Por qué hay un proceso auxiliar

Un user namespace tiene que ser **habitado** para existir, y entrar en uno le cuesta al que llama sus privilegios — que el lanzador todavía necesita. Entonces un hijo efímero entra a uno y espera; el lanzador escribe su mapa desde afuera, donde sigue siendo root, y se queda con un descriptor al namespace. El namespace sobrevive al hijo porque el descriptor lo mantiene abierto.

### El filesystem que ve el módulo

El namespace de montaje aísla la **tabla de montajes**, no los archivos. Un módulo tenía su propio namespace y el árbol entero del host adentro. Lo que lo mantenía fuera de `/etc/shadow` era `thalyx-lsm` y nada más: contención real, pero de una capa donde el diseño pide dos.

Ahora el módulo se pivota (`pivot_root`) a una raíz que contiene:

- su propio árbol en `/module`
- las rutas de sistema que necesita para arrancar siquiera (`/usr`, `/lib`, `/lib64`, `/bin`, `/sbin`, `/etc`), **todas de solo lectura**
- un puñado de nodos de dispositivo (`/dev/null`, `/dev/zero`, `/dev/full`, `/dev/random`, `/dev/urandom`)
- un `/tmp` propio, escribible, que muere con la ejecución
- un `/proc` atado a su propio namespace de PID
- **exactamente las rutas que le fueron concedidas**, con su mismo nombre

Nada más existe para ser alcanzado. La raíz misma queda de solo lectura.

**Las dos capas dicen lo mismo desde direcciones opuestas.** El LSM deniega las aperturas que no le dijeron que permitiera; la raíz deniega porque no hay nada más presente. Las dos se derivan de los mismos permisos, así que no pueden contradecirse — y un error en cualquiera de las dos lo sigue atrapando la otra.

Una ruta concedida que no existe **se rechaza**, nombrándola. Si no, el módulo correría con un permiso que el humano confirmó y que no puede usar, sin que nada lo diga. Es la misma regla de "una promesa que el sistema no puede cumplir", espejada.

#### Tres defectos más, otra vez por ejecutarlo

1. **`/proc` era el del namespace equivocado.** `enter` hace `unshare(CLONE_NEWPID)`, así que `init` es PID 1 de un namespace nuevo mientras `/proc` sigue siendo el del host. Escribir el `uid_map` de un hijo por `/proc/<pid>` apuntaba entonces a **otro proceso completamente distinto**. Falló con `EPERM`, que es el desenlace bueno; el malo estaba disponible.
2. **La dirección del mapeo estaba invertida** — descrito arriba. Montó limpio y no aisló nada de lo que debía.
3. **Un test reportó `NOT PROVEN` para una corrida que sí había funcionado.** El proceso auxiliar anunciaba su propia liberación como error en stderr, y la guarda del test coincidía con eso. Regla que sale: **una guarda de `NOT PROVEN` se ata a lo que dice el fallo real, no a una palabra que aparezca cerca.**

#### Tres defectos del pivot, también por ejecutarlo

1. **La raíz se ensamblaba sobre `/tmp`**, y el tmpfs tapaba el árbol del módulo cuando este vivía ahí. Se movió a `/run/thalyx/sandbox`, donde lo único que puede tapar es a sí mismo.
2. **El `/tmp` del módulo se montaba después de los binds**, así que tapaba cualquier ruta concedida bajo `/tmp` — el módulo recibía "no such file or directory" por algo que el humano sí había confirmado. Regla que salió de ahí: **todo montaje que tape parte de la raíz va antes de que se monte nada debajo de él.** Se encontró por suerte, porque la ruta concedida de un test resultó ser un directorio temporal; ahora hay un test que lo busca a propósito.
3. **Sellar la raíz de solo lectura antes de borrar el punto de montaje de la raíz vieja** hacía fallar el borrado. Se sella al final.

Un detalle que no perdona: **un bind de solo lectura son dos llamadas a `mount`.** Un solo `MS_BIND | MS_RDONLY` ignora la bandera en silencio y el bind hereda la escritura del origen. Es exactamente así como un contenedor termina con un `/usr` escribible que todo el mundo cree de solo lectura.

### El costo visible del allowlist

`socket` está deliberadamente fuera de la lista: un módulo sin permiso de red no debería siquiera poder construir un socket para que se lo denieguen. El costo es real y queda anotado — `ls -l` se degrada, porque NSS quiere un socket unix para resolver nombres de usuario.

### Decreto: `socket` queda fuera, y es reversible

**Decidido el 2 de agosto de 2026:** se queda como está — un módulo sin permiso de red no puede construir ningún socket, ni siquiera local.

**Explícitamente no es una decisión final.** Se toma sabiendo que el costo es que algunas herramientas se degradan adentro, y con la intención de revisarla cuando exista un módulo real que la necesite. El mecanismo para cambiarla ya se conoce: BPF clásico puede filtrar por el argumento `domain` de `socket()`, así que permitir solo `AF_UNIX` es un cambio acotado al filtro, sin tocar manifiesto ni permisos.

La tercera opción —un tipo de permiso `ipc` que el manifiesto declare y el humano confirme— sigue sobre la mesa y es la que corresponde si los módulos llegan a hablar entre sí.

### Cómo se derivó la lista de syscalls permitidas

Ejecutando programas reales bajo el filtro y leyendo **el syscall que el kernel nombra en su log de auditoría** cuando mata al proceso. Ese instrumento es mejor que `strace`: strace traza desde fuera del sandbox, mientras que la línea de auditoría dice exactamente qué llamada mató el filtro y en qué proceso.

Así aparecieron `statfs`, `fadvise64`, `copy_file_range` y las lecturas de xattr. Ninguna estaba en la lista escrita de memoria.

Una syscall denegada **mata** el proceso, no devuelve `EPERM`. Un programa que recibe `EPERM` de una llamada que no esperaba que fallara sigue adelante hacia un estado que nadie diseñó, y el fallo aparece en otro lado, mucho después. Un kill es ruidoso, inmediato y atribuible.

### Las dos etapas

`CLONE_NEWPID` **no mueve al que llama** a un namespace de PID nuevo: hace que sus *hijos* sean los primeros procesos de uno. Entonces tiene que haber un fork después del unshare, y el proceso que se convierte en el módulo no puede ser el que hizo unshare.

```
enter   entra al cgroup, verifica, hace unshare, lanza init
  └─ init   (PID 1 del namespace nuevo)
            monta /proc, pone el hostname, instala seccomp, exec del módulo
```

Partirlo no cuesta nada de lo que importa: el cgroup se hereda en el fork, así que `init` está adentro desde su primera instrucción, y los namespaces también. Lo que `init` agrega es todo lo que solo se puede hacer desde adentro — un `/proc` que refleje el namespace de PID nuevo, y un filtro seccomp que no debía restringir el trabajo de preparación que `enter` todavía tenía que hacer.

### La regla de orden

**La política está en el kernel antes de que el proceso esté en el cgroup, y el proceso está en el cgroup antes de la primera instrucción del módulo.**

Las dos mitades importan por razones distintas:

- El LSM **falla abierto** para un cgroup del que no tiene entrada. Tiene que hacerlo: si no, cualquier proceso de la máquina quedaría denegado de todo. Entonces un proceso que entra a su cgroup antes de que la política esté escrita corre sin contención durante ese rato.
- Un proceso no puede entrar a un cgroup antes de existir. El hueco entre `fork` y la entrada es inevitable; lo evitable es que dentro de ese hueco corra código del módulo.

La salida es que ese instante le pertenezca a Thalyx. El padre vuelve a ejecutar el binario `thalyx` con un argumento marcador; ese hijo (`enter`) entra al cgroup y **relee `cgroup.procs` para confirmar que de verdad es miembro**. El proceso que finalmente se convierte en el módulo es `init`, hijo de `enter` — y **hereda la pertenencia al cgroup en el fork**, así que está adentro desde su primera instrucción. Entre el ingreso al cgroup y el `exec` del módulo no corre ni una línea de código del módulo: todo lo de en medio es código de Thalyx.

La relectura no es paranoia: escribir un pid en un archivo común tiene éxito y no contiene nada. Sin ella, un cgroup mal apuntado se vería idéntico a uno correcto hasta el momento en que el módulo hiciera algo que debía estar denegado.

Si cualquiera de los pasos falla, **el módulo no corre**. Ejecutarlo sin contención es exactamente lo que el mecanismo existe para impedir.

### La regla de desmontaje

Al terminar, el orden se invierte, y por una razón más filosa que la simetría: **el id de un cgroup es un número de inodo, y los inodos se reutilizan**. Una entrada en el mapa que sobreviva a su directorio se convertiría, en silencio, en la política del siguiente cgroup que reciba ese inodo.

Primero se retira la política, después se borra el cgroup.

Y no se desmonta nada mientras quede alguien adentro: **un cgroup por módulo, no por proceso**, porque la política es propiedad del módulo. Quitarla cuando termina la primera instancia le arrancaría a la segunda, a mitad de vuelo, permisos que el humano sí confirmó.

### Por qué implementación propia

Se evaluó apoyarse en bubblewrap, que ya resuelve este montaje y está auditado por su uso en Flatpak. Se decretó implementación propia: Thalyx es un sistema operativo, no una distribución que integra piezas ajenas, y el aislamiento es una pieza de arquitectura, no una dependencia delegada. El costo asumido conscientemente es tiempo de iteración y la necesidad de revisión externa sobre el montaje de user namespaces, donde los errores no se manifiestan como fallos sino como agujeros silenciosos.

Esa exposición se compensa con los tests de nivel 2 de la [[Estrategia-de-Pruebas]].

## Revisiones

### 2026-08-02 — Se implementa `module_standard` y se introduce `unsafe` contenido
**Antes:** el perfil estaba decretado y no construido; el aislamiento era `thalyx-lsm` y nada más.
**Ahora:** namespaces, seccomp y límites de cgroup, verificados contra el kernel real.
**Lo que cambió en las reglas del repo:** namespaces, `mount` y `seccomp` no están envueltos por la biblioteca estándar de Rust, así que el proyecto tiene que llamar al kernel directo. Eso exige `unsafe`, y el workspace lo tenía **prohibido en todos lados**.

La solución fue contenerlo, no relajarlo: un crate nuevo, `thalyx-syscall`, es el **único** lugar del workspace donde `unsafe` está permitido. Todos los demás conservan `unsafe_code = "forbid"`. Ahí está en `deny` y no en `forbid`, de modo que cada uno de los cinco bloques está marcado explícitamente y lleva un comentario de por qué es correcto. El crate entero son ~200 líneas y no contiene lógica, porque lógica ahí sería lógica que nadie puede revisar con la misma facilidad.

Se escribieron los wrappers a mano en vez de tomar un crate de bindings. La razón no es el decreto de implementación propia —un binding a libc no es un sandbox ajeno— sino más estrecha: **el `unsafe` queda visible**. Una dependencia lo movería fuera de la vista sin quitarlo, y lo que hace defendible a este crate es que se puede leer.

**Un defecto encontrado mientras se escribía la prueba que lo buscaba:** el padre ajustaba el perfil según lo que el módulo tenía concedido, y el hijo lo volvía a derivar desde el nombre del perfil — así que un módulo que **sí** tenía permiso de red terminaba igual en un namespace de red vacío. Silencioso, y solo visible preguntándole al módulo qué interfaces veía. Ahora la máscara efectiva viaja a través de la re-ejecución en vez de derivarse dos veces.

### 2026-08-02 — La decisión vive en el núcleo; el sandbox solo contiene y lanza
**Antes:** el decreto decía que el Sandbox es una pieza separada del Core, sin precisar quién decide qué puede hacer un módulo al ejecutarlo.
**Ahora:** el núcleo resuelve el entrypoint, lee los permisos vigentes, rechaza lo que no se puede aplicar y registra el intento. El sandbox recibe una decisión ya tomada y solo la ejecuta.
**Motivo:** el decreto pone al Sandbox **fuera de la TCB**. Decidir qué puede hacer un módulo no es su trabajo; si lo fuera, la decisión viviría en el componente al que no se le cree. Por eso la dependencia apunta del núcleo al sandbox y no al revés.

**Y no en la CLI:** [[Coherencia-Doble-Ruta]] exige que la ruta humana y la del agente dejen el sistema en el mismo estado. Una orquestación escrita en la CLI habría que escribirla otra vez para el agente, y las dos derivarían. Hay una sola implementación y las dos rutas la llaman.

### 2026-08-02 — El manifiesto viaja con el módulo
**Antes:** un módulo instalado eran solo sus archivos. Nada guardaba qué tenía permitido hacer ni cómo arrancarlo.
**Ahora:** el manifiesto y su firma se escriben en staging y se publican con el mismo `rename` que los archivos. No hay orden en el que exista un directorio de versión sin su registro, así que un manifiesto ausente es corrupción y no un estado normal.
**Motivo:** sin él, el runtime no tenía forma de lanzar un módulo sin que alguien le dijera el entrypoint desde afuera — lo que habría hecho autoridad al invocante en vez de al manifiesto.
**Detalles que resultaron importar:**
- Se guarda **verbatim**, no re-serializado: una vuelta por un serializador produce bytes equivalentes para quien lee e inverificables para quien verifica.
- Se **re-verifica en cada lectura** contra la clave anclada. El modelo de amenaza no le concede integridad al store; quien pueda escribir ahí podría ampliar los permisos declarados de un módulo o apuntar su entrypoint a otro lado, y nada río abajo lo notaría.
- `.thalyx/` dentro del árbol del módulo queda reservado, y una entrada del artefacto que caiga ahí se rechaza. Un módulo que pudiera escribir ahí podría reescribir lo que tiene permitido hacer.

### 2026-08-01 — Se detalla el mecanismo y se separa instalación de ejecución
**Antes:** el mecanismo estaba enunciado ("namespaces, cgroups, seccomp") pero pendiente de detalle fino, y la instalación de un módulo implicaba ejecutar su script dentro del sandbox.
**Ahora:** perfil `module_standard` concreto, implementación propia decretada, e instalación sin ejecución de código del módulo.
**Motivo:** el pendiente de sandboxing bloqueaba la Fase 1. Y ejecutar un script de instalación arbitrario hacía imposible cualquier verificación criptográfica del resultado — ver [[Verificacion-y-Distribucion]].

## Relacionado
- [[Verificacion-y-Distribucion]]
- [[Fase-Commit-Atomico]]
- [[Modelo-de-Amenaza]]
- [[Estrategia-de-Pruebas]]
- [[Flujo-Canonico-Overview]]
- [[Sistema-de-Modulos]]
