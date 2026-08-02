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

### Estado real: identidad y lanzamiento, todavía no aislamiento

De ese perfil está construida **la parte de la que cuelga todo lo demás**: el cgroup v2 que le da identidad al módulo y el lanzamiento que lo mete dentro. Ver `crates/thalyx-sandbox` y [[Estado-de-Implementacion]].

Namespaces, seccomp y límites de recursos **no están implementados**. Un módulo lanzado hoy está contenido por [[Permisos-JIT|thalyx-lsm]] y por nada más.

Se escribe así, y no "el sandbox está en progreso", porque la frase cómoda —"los módulos corren en un sandbox"— sería falsa hoy, y el día que alguien la crea es el día que un módulo hace algo que nadie esperaba.

### La regla de orden

**La política está en el kernel antes de que el proceso esté en el cgroup, y el proceso está en el cgroup antes de la primera instrucción del módulo.**

Las dos mitades importan por razones distintas:

- El LSM **falla abierto** para un cgroup del que no tiene entrada. Tiene que hacerlo: si no, cualquier proceso de la máquina quedaría denegado de todo. Entonces un proceso que entra a su cgroup antes de que la política esté escrita corre sin contención durante ese rato.
- Un proceso no puede entrar a un cgroup antes de existir. El hueco entre `fork` y la entrada es inevitable; lo evitable es que dentro de ese hueco corra código del módulo.

La salida es que ese instante le pertenezca a Thalyx. El padre vuelve a ejecutar el binario `thalyx` con un argumento marcador; ese hijo entra al cgroup, **relee `cgroup.procs` para confirmar que de verdad es miembro**, y solo entonces *se convierte* en el módulo con `exec`. El proceso que entró y el proceso que corre el módulo son el mismo.

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
