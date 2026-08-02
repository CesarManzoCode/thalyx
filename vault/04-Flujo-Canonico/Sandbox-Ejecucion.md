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
                        adentro          afuera
pid                     1                6878
hostname                thalyx-module    vm
procesos visibles       4                82
interfaces de red       lo:              lo: eth0:
socket()                SIGSYS           funciona
unshare()               SIGSYS           funciona
```

Preguntarle al módulo y no a Thalyx es el punto. Toda la clase de defecto que este proyecto encuentra una y otra vez es el sistema reportando éxito de trabajo que no hizo.

**Lo que falta: el namespace de usuario.** Hacerlo de forma útil implica decidir con qué uid corre un módulo y de quién son los archivos del store — una pregunta de política, no de implementación. Un user namespace que mapee root a root cumpliría la letra del decreto y no aislaría nada, y este proyecto no publica teatro llamándolo protección. Un módulo hoy corre con el uid con el que corre Thalyx.

### El costo visible del allowlist

`socket` está deliberadamente fuera de la lista: un módulo sin permiso de red no debería siquiera poder construir un socket para que se lo denieguen. El costo es real y queda anotado — `ls -l` se degrada, porque NSS quiere un socket unix para resolver nombres de usuario.

Queda como **decisión abierta** si `module_standard` debería permitir sockets `AF_UNIX`. Es expresable en BPF clásico (se puede filtrar por el argumento `domain`), pero es una decisión de política, no de implementación.

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
