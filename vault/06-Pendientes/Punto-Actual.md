---
tipo: estado-vivo
estado: activo
fecha-actualizacion: 2026-08-03
tags: [continuidad, punto-actual, sesiones]
---

# Punto actual

> **Este archivo se actualiza cada vez que se termina algo.** Existe para que
> una sesión nueva —humana o de IA— sepa exactamente dónde quedó el proyecto
> sin que nadie tenga que recordarlo. Si algo importante vive solo en una
> conversación, esa conversación se pierde y el conocimiento con ella.
>
> Para *cómo* trabajar en el proyecto, ver `CLAUDE.md` en la raíz del repo.

> ## La máquina arrancó — 2026-08-03
>
> `make -C image run`, en la Fedora de Cesar, con kernel 6.12.101 propio y un
> solo programa dentro. Montó sus siete filesystems, imprimió lo que es y lo que
> no tiene, y esperó una instrucción. **Es la primera vez que Thalyx existe como
> máquina y no como programa sobre la máquina de alguien más.**
>
> Se describió con tres `no`: sin Btrfs, sin enforcement, sin módulos. Los tres
> eran conocidos y están anotados abajo. Lo siguiente es el primero de ellos que
> tiene arreglo: **cargar `thalyx-lsm` desde dentro**.
>
> El procedimiento sigue en [[Primer-Arranque]]. Si Cesar pega la salida de un
> comando, casi siempre es de ahí.

## Dónde estamos, en una frase

**El 2026-08-03 se quitó la distribución.** La bóveda decretaba en tres notas
una base Alpine y en una —marcada no negociable— que Thalyx no es una
distribución de Linux. Se resolvió a favor de la segunda: **la imagen es el
kernel de Linux y `thalyx`, y nada más.** Ninguna distro, nunca. Ver
[[Construccion-del-ISO]].

Eso convirtió la **API interna de módulos** en la pieza que seguía: sin shell y
sin utilidades, un módulo no puede ser un script y no tiene con quién hablar
excepto Thalyx. **Diseñada el 2026-08-03** en [[API-Interna-de-Modulos]], y su
protocolo construido en `crates/thalyx-abi`. Falta conectarla a la ejecución
real de un módulo.

La Fase 1 tiene **sus tres primitivas** —de las cuatro decretadas; la cuarta es
el [[Scheduler-Predictivo]] y es de Fase 2— y su flujo canónico **construidos y
verificados en hardware real**: 44 comprobaciones en máquina real. Desde
entonces: **490 pruebas**, el agente mínimo que lleva un enunciado hasta un
módulo instalado sin modelo alguno, `thalyx` como PID 1, y la imagen que Thalyx
construye para sí mismo.

**Arrancar la imagen ya ocurrió.** Para cerrar la fase faltan dos: **la API
interna de módulos** y **el modelo del agente** — más el enforcement dentro de
la imagen, que no es de la lista original porque hasta hoy no había imagen
donde faltara.

## Última corrida verificada

**2026-08-03, Fedora 43, kernel 7.0.11, Btrfs, `bpf` en el orden de LSM.**

```
proven 44 · not proven 0 · failed 0
```

> **La próxima corrida no dará `not proven 0`, y eso es correcto.** Hay dos
> etapas nuevas —el agente y la imagen— y las dos tienen una mitad que nada ha
> comprobado. Esperar alrededor de `proven 53 · not proven 2 · failed 0`.
>
> Un número verde que se conserva escondiendo lo que no se probó es exactamente
> la clase de instrumento que este proyecto existe para no construir. Los
> comandos y qué significa cada fallo están en [[Primer-Arranque]].

Es la primera vez que todo lo que Thalyx afirma se comprueba en una sola
máquina y se sostiene. Reproducirla:

```
git pull && cargo install --path crates/thalyx-cli && sudo ./dev/verify.sh
```

## Qué quedó construido y probado

| Pieza | Comprobado en hardware |
|---|---|
| Instalación de módulos, commit atómico, journal, permisos | Sí, incluida inyección de fallos |
| `thalyx-lsm` (BPF LSM) | Sí — **deniega de verdad** una conexión dentro del cgroup y la permite fuera |
| Sandbox completo: namespaces, seccomp, `pivot_root`, idmap, límites | Sí — el módulo reporta su propio pid, uid, hostname, red y raíz |
| Un uid por módulo, nunca reutilizado | Sí |
| Índice en grafo + parser mecánico | Sí |
| Contador de mutaciones del kernel, 10 hooks | Sí — 5000 escrituras por descriptor abierto, todas contadas |
| Contador acotado al árbol | Sí — 5000 dentro contadas, 5000 fuera ignoradas |
| El atajo del índice (`graph trust`) | Sí — se gana con verificación, y un cambio real sigue saliendo obsoleto |
| Memoria persistente (3ª primitiva) | Sí — el hecho deja de ser afirmable al editar el archivo por fuera |
| `rollback` | Sí — quita el módulo y sus permisos; se niega la segunda vez |
| Snapshots de Btrfs | Sí — de solo lectura, conservan el contenido viejo |
| `restore` | Sí — restaura, destruye lo posterior, y conserva lo destruido |

Detalle por crate en [[Estado-de-Implementacion]].

## Lo que sigue, en orden

### 1. El agente — su mitad determinista ya está construida

Va primero, y el motivo es de descubrimiento, no de avance. El ISO desbloquea
cinco de los seis pasos del [[Criterio-de-Salida-Fase-1|criterio de salida]]
contra uno del agente, pero el ISO **integra piezas ya probadas: no puede
enseñar nada que no se sepa ya**. El agente sí puede invalidar el diseño del
contrato. Descubrir tarde que la procedencia por campo no sobrevive a varias
inferencias costaría mucho más que un ISO retrasado, y la regla 1 de
`CLAUDE.md` dice que todos los defectos reales salieron de correr el sistema.

Alcance: router de reglas más un modelo con decodificación restringida por
gramática, sobre **un solo caso de uso** —instalar un módulo—, no un agente
general.

**Construida ya la mitad que no necesita un modelo**, y probada de punta a
punta: `thalyx agent do "install dev.thalyx.demo@^1.0" --repo <dir>` resuelve
contra un repositorio local de bundles firmados, pide confirmación por el camino
confiable, y deja el módulo instalado y ejecutable. Lo que falta, en orden:

1. El `Model` real que invoca `llama.cpp` como proceso.
2. La gramática GBNF, que no se puede validar sin `llama.cpp`.
3. El banco de las cuatro gamas, para sustituir las cifras estimadas.

Los tres necesitan tu máquina: aquí no hay `llama.cpp` y la política de red del
entorno bloquea `huggingface.co`.

**El decreto que lo bloqueaba ya está escrito:** [[Gamas-de-Modelo]]. No un
modelo anclado sino **cuatro gamas de una sola familia** que el usuario elige
según su hardware, con `llama.cpp` invocado como proceso y decodificación
restringida por gramática. Anclar un modelo de 5 GB dejaría fuera a una máquina
de 8 GB, y el criterio de salida exige justamente que alguien de fuera lo use.
Con la gramática, un contrato mal formado es imposible en las cuatro gamas: lo
que cambia entre ellas es el acierto al interpretar la intención, no la
seguridad. Y **el modelo nunca escribe la procedencia** — la pone el
ensamblador, porque una gramática obliga a la forma y no a la verdad.

El alcance del primero está en [[Agente-Minimo]].

Lo que sí está listo para el agente cuando exista: el contrato estructurado con
marcado de origen, el camino confiable, la memoria persistente, y el principio
de doble ruta implementado (todo lo que el agente podrá hacer, un humano ya
puede hacerlo por la CLI).

### 2. La imagen ya arranca; le faltan tres cosas y las nombra

El arranque está hecho y verificado. Lo que la máquina dijo de sí misma:

```
  ok  kernel       6.12.101
  no  filesystem   rootfs — snapshots and restore need btrfs and will not work here
  ok  cgroup v2    mounted at /sys/fs/cgroup
  ok  lsm order    capability,bpf
  no  enforcement  the policy map is not loaded, so no permission would be enforced
  no  modules      nothing installed yet

  3 are not here. I will not pretend otherwise later.
```

Las tres, en el orden en que se resuelven:

1. **Cargar `thalyx-lsm` desde dentro de Thalyx.** Sin bpftool y sin shell, hoy
   no se carga. Sin esto no hay enforcement en la imagen. **Y el stub actual
   busca `/lib/thalyx/thalyx_lsm.bpf.o`, que es un segundo archivo y por lo
   tanto está prohibido por [[Filosofia-Fundacional]]** — el objeto BPF tiene
   que ir *dentro* del binario, no junto a él. Ver el decreto abierto abajo.
2. **El store.** `store.qcow2` se crea vacío: no tiene Btrfs ni los tres
   subvolúmenes, así que PID 1 monta lo que no existe y la máquina lo dice. Es
   el mismo problema de forma que el LSM —hace falta `mkfs.btrfs`, que tampoco
   puede estar en la imagen— y sin él no se puede instalar nada.
3. **La API interna de módulos** ([[Core-Nucleo]]). Decretada desde el 31 de
   julio, sin una línea escrita. Sin userland debajo, es la única superficie que
   un módulo puede tocar — y es lo que hace que un programa escrito para Thalyx
   no corra en ningún otro lado.

### 3. Reindexado incremental

Consumir el ringbuf `thalyx_mutations` para saber *qué* cambió, no solo que
algo cambió. **Ya no hace falta para el atajo** —eso lo resolvió la atribución
por ancestros— así que es una mejora de rendimiento, no de corrección. Ver
[[FS-en-Grafo]].

## Decretos abiertos

Ninguno bloquea excepto el primero.

- [ ] **Una frontera real que etiquete canales** — hoy `--foreign` es una bandera que un humano pasa a propósito; nada en Thalyx llama a `Segment::foreign()` por su cuenta, porque nada trae texto de terceros todavía. Toda la defensa de procedencia descansa sobre ese código, que no existe.
- [ ] **Correr el banco de las gamas** — el decreto ya está ([[Gamas-de-Modelo]]); faltan las cifras medidas. Necesita `llama.cpp` y los pesos, que el contenedor de desarrollo no puede tener.
- [ ] Métricas de benchmark de la Fase 2 (el umbral ya está decretado; falta el instrumento)
- [ ] Técnicas de interpretabilidad aplicables al agente
- [ ] Arquitectura del índice semántico a mayor escala (SQLite alcanza para Fase 1)
- [ ] Sistema de reputación resistente a Sybil (pospuesto a propósito)
- [ ] Dependencias entre módulos con backtracking (pospuesto hasta que un módulo real las necesite)
- [ ] Condiciones para habilitar llamadas a modelos remotos

Lista completa y viva en [[Tareas-Pendientes]].

## Lo que sigue sin validarse, y se carga a propósito

**Ningún decreto de esta bóveda ha sido contrastado con una persona ajena al
proyecto.** Todo el razonamiento sobre por qué alguien elegiría Thalyx sigue
siendo a priori. Eso es cierto y sigue siendo un riesgo real.

**Y no se adelanta.** El [[Criterio-de-Salida-Fase-1|criterio de salida]] pone a
esa persona *después* del ISO, arrancando la imagen: ese es su paso 1. Nadie de
fuera toca el sistema antes. No por miedo a lo que diga —el proyecto nunca
dependió de eso— sino porque lo que esa persona determina es **la escala, no la
validez**, y esta fase es incompatible con la escala.

El riesgo se lleva con los ojos abiertos hasta entonces, que no es lo mismo que
ignorarlo. El razonamiento completo, y la deriva concreta que previene, están en
[[Criterio-de-Salida-Fase-1]].

Ver también [[Por-Que-Elegirian-Este-SO]] y [[Riesgo-de-Ejecucion]].

## Cosas que hay que saber para no romper nada

**El watcher del LSM es todo o nada.** Diez hooks; si el kernel no expone
alguno, declina cargarse entero en vez de cargarse pareciendo completo. Un hook
faltante no es un número más chico, es una forma concreta de que un archivo
cambie en silencio. `make -C lsm hooks` dice cuáles hay.

**`verify.sh` desengancha el LSM al salir.** Por eso `thalyx graph watcher`
dice "not loaded" después de una corrida. Es correcto, no es un fallo.

**`verify.sh` compila en `dev/.verify-target`** para no dejar el `target/` del
usuario a nombre de root. Por eso el binario que queda en el PATH es el de
`cargo install`, y hay que reinstalarlo después de cambios en la CLI.

**El store por defecto es `/opt/thalyx`**, que necesita sudo. Para uso normal:
`export THALYX_ROOT=~/.local/share/thalyx`.

**El atajo del índice está apagado por defecto en cada índice nuevo**, y
`verify.sh` reconstruye el índice del repo, así que vuelve a apagarse en cada
corrida. Para encenderlo a mano:
`thalyx graph trust ~/thalyx/crates --counter`.

## Historial de sesiones

### 2026-08-03 (10) — la API interna deja de ser una línea de una nota
Decretada en [[API-Interna-de-Modulos]] y construida en `crates/thalyx-abi`:
**un socket que Thalyx entrega ya abierto en el descriptor 3** al ejecutar el
módulo —sin ruta que equivocar, sobrevive a la raíz vacía del sandbox, y su
ausencia es lo que impide que un módulo corra fuera de Thalyx—, mensajes de
longitud explícita más CBOR, y tres familias: archivos, notificar, y preguntar
quién es. **27 pruebas**, incluidas las dos mitades de la conversación
hablando por un socket real entre dos hilos.

Tres decisiones que valen más que el código:

- **Denegado y fallido son respuestas distintas.** "No puedes leer esto" y
  "esto no se pudo leer" son hechos diferentes sobre el mundo, y un módulo que
  solo supiera que falló reportaría un disco ausente como un problema de
  permisos. Es la regla 10 de `CLAUDE.md` puesta en el protocolo.
- **Un campo desconocido se rechaza, no se ignora.** Es la dirección incómoda
  —rompe con un módulo más nuevo— y la correcta: ignorarlo dejaría al que envía
  creyendo que restringió la operación y al que recibe sin haber visto la
  restricción, en un canal que gobierna permisos.
- **Un marco ilegible cierra la conexión; un mensaje ilegible se contesta.**
  Después de una longitud mala no hay dónde empezar a leer otra vez; después de
  un mensaje malo, sí.

**Y una tercera contradicción del mismo tipo que las anteriores.**
[[Core-Nucleo]] listaba *"ejecutar comandos"* entre las capacidades de esta API.
No hay comandos que ejecutar. Como el login en tty1 y como `bpftool`: una
capacidad que se apoyaba en la base y envejeció callada cuando la base se cayó.
Queda anulada por decreto, no implementada.

Falta lo que la vuelve real: pasar el descriptor por las dos etapas del
lanzamiento, el servidor contra los permisos verdaderos, y un módulo escrito
contra ella. Eso último es lo que el decreto pone como prueba de que sirve.

### 2026-08-03 (9) — existe la máquina
`make -C image run` arrancó. Kernel 6.12.101 construido desde `allnoconfig`,
initramfs con **un solo archivo**, `thalyx` como PID 1. Montó los siete
filesystems, arrancó la sesión, y la sesión imprimió el párrafo que dice que no
hay shell detrás — que solo imprime cuando su padre es el pid 1, así que la
frase no está cableada: es una comprobación.

Y se describió con tres `no` que no oculta: sin Btrfs, sin enforcement, sin
módulos. Los tres eran conocidos y están arriba con su orden de resolución.

**Lo que esto cierra**: el paso 1 del [[Criterio-de-Salida-Fase-1]] tiene por
fin una máquina detrás. No cierra el criterio —ese exige que lo haga alguien de
fuera, sin ayuda— pero hasta hoy no había nada que esa persona pudiera arrancar.

**Un hallazgo del arranque**: `attach_lsm` en `init.rs` busca
`/lib/thalyx/thalyx_lsm.bpf.o`. Ese archivo **no puede existir**: sería un
segundo archivo en una imagen que el decreto obliga a tener uno. El mensaje
"is not in the image" es cierto y su arreglo obvio es el equivocado. El objeto
BPF va incrustado en el binario.

### 2026-08-03 (8) — el kernel no compilaba, y la configuración se perdía sola
El primer `make -C image kernel` en la máquina de Cesar falló entero en
`arch/x86/boot/compressed/`: GCC 15 (Fedora 43) usa C23 por defecto, donde
`bool`, `true` y `false` son palabras reservadas, y ese directorio era el único
del kernel que nunca pasaba `-std=`. **No se puede arreglar desde fuera** —su
Makefile abre con `KBUILD_CFLAGS :=`, que tira lo que venga de arriba, así que
`KCFLAGS` jamás llega. Río arriba lo arreglaron en enero de 2025 y aterrizó en
la serie estable en **6.12.14**, comprobado tag por tag. `KVERSION` pasa a
**6.12.101**, la cabeza de la línea 6.12 LTS.

**Y al reproducir la configuración a mano apareció algo peor.** `olddefconfig`
descarta en silencio toda opción cuyas dependencias no se cumplan: **nueve de
las de `thalyx.config` no llegaban al `.config` final**, entre ellas
`CONFIG_BPF_LSM` y `CONFIG_DEBUG_INFO_BTF`. La máquina habría arrancado
perfecta y `thalyx-lsm` no se habría podido enganchar nunca, con un síntoma
idéntico al hueco de `bpftool` que ya conocíamos — la culpa habría caído sobre
el cargador, que no tenía nada que ver. También faltaban `VIRTIO_MENU` y
`BLK_DEV`, sin los cuales no hay disco del store, e `IPC_NS`.

`make -C image kernel` ahora compara lo pedido contra lo que salió y **se niega
a compilar** si falta una línea. Probado con su control: quitando `BPF_LSM` y
`BTF` a mano, los nombra y sale con error. De ahí la regla nueva de
[[Estrategia-de-Pruebas]]: **pedirle algo a una herramienta no es haberlo
obtenido**.

Con las nueve líneas puestas, 6.12.101 configura y compila limpio en el
contenedor, y el `vmlinux` trae `.BTF`. Eso comprueba la configuración, **no**
el problema de GCC 15: aquí hay GCC 13. QEMU sigue sin correr nunca.

### 2026-08-03 (7) — el decreto fundacional, y todo listo para arrancar
Cesar escribió el texto que funda el proyecto y quedó **literal** como primera
sección de [[Filosofia-Fundacional]], con la regla de que cualquier decreto que
lo contradiga está equivocado. Está enlazado desde `CLAUDE.md`, el índice y el
README, que son las cuatro puertas de entrada.

Se registraron los dos decretos que su propio texto invalida: `bpftool` (que ya
no puede estar en la imagen) y `llama.cpp` como proceso (que sería un segundo
programa — probablemente el modelo del agente sea **un módulo**, pero eso lo
decide Cesar).

`rusqlite` pasa a `bundled`: SQLite se compila dentro del binario. No es
preferencia, es necesidad — no hay libsqlite3 en el disco de la imagen contra el
que enlazar, y era el primer bloqueador del binario estático.

[[Primer-Arranque]] tiene el procedimiento completo.

### 2026-08-03 (6) — hay máquina: PID 1, la imagen, y el kernel
`thalyx` es PID 1 (`init.rs`): monta siete filesystems diciendo por qué cada
uno, arranca la sesión, y cosecha huérfanos para siempre. Si un montaje falla no
aborta — la máquina arranca describiéndose a sí misma, porque un sistema que se
niega a arrancar no te dice *por qué* desde una pantalla a la que no llegas.

**Thalyx construye su propia imagen** (`image.rs`): un cpio `newc` escrito aquí,
sin `cpio` ni herramientas ajenas. Un initramfs, no un ISO — sin gestor de
arranque, sin tabla de particiones, sin una tercera cosa donde algo se esconda.
**Un solo archivo dentro**, `/init`, porque si el decreto dice un programa,
un archivo es lo que lo vuelve cierto en vez de casi cierto.

Y se cuenta: `make -C image count` parsea el archivo y dice cuántos programas
hay. Si no dice uno, el decreto está roto y el número lo dice antes de que nadie
discuta.

`image/` lleva el Makefile y `thalyx.config`, un kernel desde `allnoconfig`.
**Jamás ejecutados**: aquí no hay red a kernel.org ni QEMU.

El hueco grande queda dicho: **`thalyx-lsm` no se carga en el arranque**. El
cargador invocaba `bpftool`, y no hay bpftool en la imagen ni shell para
llamarlo. La máquina arranca y lo dice.

### 2026-08-03 (5) — se cae la distro, y con ella lo que se apoyaba en ella
Cesar preguntó por qué habría un login al arrancar si nadie lo construyó. La
respuesta —lo pone la base— hizo visible que había una base, y que la bóveda se
contradecía en cuatro notas. Decreto: **cualquier distribución queda fuera para
siempre**; el kernel de Linux nunca estuvo en discusión.

Borrados por falsos: el esqueleto del ISO escrito esa misma noche, que producía
una distro de Alpine con el getty quitado, y el módulo `dev.thalyx.hola`, que
era un script de shell y por lo tanto corría en cualquier Linux.

Reescritos: [[Construccion-del-ISO]] entero, y las secciones de
[[Core-Nucleo]] y [[Fases-de-Implementacion]] que decretaban la base.

### 2026-08-03 (4) — el enunciado llega hasta el disco, y un fallo que solo salió corriéndolo
**El paso 6, ahora con sus dos mitades.** El agente escribe lo que hizo **y lo
lee**: `thalyx agent recall <tarea>`, y `--task` trae el contexto solo. Lo que
recuerda entra como estado de Thalyx y puede tener efecto, salvo lo que ya no
puede confirmar, que se muestra y no se usa. Falta que retome una conversación
de varios turnos, que necesita un modelo.

Lo que quedó: `thalyx agent do --task <t>`
escribe en la memoria persistente qué se pidió y qué se instaló, y
`thalyx memory recall <t>` lo lee desde otro proceso. Los dos hechos son de
clase distinta a propósito: lo que el humano dijo **no atestigua nada** —ningún
archivo puede volver falso que lo haya dicho— y lo instalado atestigua el enlace
`current`, así que quitar el módulo deja el recuerdo *no afirmable* y lo dice,
en vez de seguir reportando una instalación que ya no está.
`thalyx agent plan` y `thalyx agent do`, más el repositorio local y la
resolución de versiones (`thalyx-core/repo.rs`): **máxima versión que satisface
el constraint y cuya firma valida**, como manda [[Resolucion-de-Versiones]]. La
cadena entera funciona contra bundles firmados de verdad — enunciado, contrato,
resolución, camino confiable, commit atómico, journal, y el módulo instalado
corre.

**El fallo del día**, y es el más instructivo que ha dado el proyecto: la
atribución tomaba el canal *menos* confiable cuando un valor aparecía en dos.
Eso volvía imposible de instalar por nombre cualquier módulo mencionado en
cualquier página leída. Pasó 39 pruebas y tres mutantes deliberados. Murió a los
tres segundos de existir el comando, tecleando una frase. De ahí la regla nueva
de [[Estrategia-de-Pruebas]]: **un mutante demuestra que una prueba es portante,
no que la decisión que codifica sea la correcta.**

También quedó `thalyx dev agent-probe`, que existe por la regla 4: sin modelo,
toda inyección se rechaza con "no model is configured", y esa denegación se ve
idéntica a la de la procedencia sin probar nada de ella.

Antes de eso, `bundle.rs`: un `.thmod` de 768 MB **sin firma** llevaba el
proceso a 1 GB de RSS porque cada miembro se leía entero antes de decidir si
importaba. Ahora hay tamaños por miembro, los desconocidos no se leen, y el
artefacto no puede expandirse más de 50× lo comprimido.

### 2026-08-03 (3) — el agente mínimo, contra un modelo que miente a propósito
Se decretó [[Gamas-de-Modelo]] —cuatro gamas de una familia, `llama.cpp` como
proceso, gramática restringida, y **el modelo nunca escribe la procedencia**— y
se construyó `crates/thalyx-agent` hasta donde este contenedor puede
comprobarlo: router, atribución, ensamblado y un falso hostil con nueve formas
de portarse mal. 39 pruebas.

Al construirlo aparecieron dos cosas que el decreto no anticipaba, ya escritas
como revisión en [[Agente-Minimo]]: atribuir un valor por **dónde aparece**
también detecta las alucinaciones, y una *operación* no se puede atribuir
buscándola, así que se atribuye por lo que la conclusión pudo leer — de donde
sale que **en cuanto hay texto ajeno en el transcript, el modelo ya no puede
originar una acción**, y el humano sí, tecleándola.

Y una regla nueva de [[Estrategia-de-Pruebas]], encontrada rompiendo cada
mecanismo a propósito para ver qué pruebas lo notaban: **dos defensas que se
solapan hacen que la prueba grande no pruebe ninguna**. La prueba de las nueve
malas conductas no falló con ninguno de los tres mutantes.

### 2026-08-03 (2) — una revisión externa encontró que la bóveda se contradecía
Una lectura externa del repo —solo código y documentación, sin el contexto de
la filosofía— encontró que `Estado-de-Implementacion` afirmaba a la vez que
`restore` estaba construido y que **no existe**, y que los límites de recursos
seguían sin probarse cuando `verify.sh` ya tenía la etapa. Al corregirlo
aparecieron tres más: dos listas incompatibles de "las cuatro primitivas"
(contando [[Parser-Mecanico]], que su propio decreto llama *componente*), un
comentario en `thalyx-sandbox/src/lib.rs` que decía que un módulo corre con el
uid de Thalyx cuando `uids.rs` lleva días dándole uno propio, y "tres
variables" de salto donde hay cuatro.

Las cinco tienen la misma forma y de ahí sale la regla nueva de
[[Estrategia-de-Pruebas]]: **una afirmación de que algo falta no la rompe
nada**. El código rompe las afirmaciones de que algo funciona; las de ausencia
envejecen calladas.

También quedó anotado el hueco simétrico: `verify.sh` activa tres de sus cuatro
variables `THALYX_REQUIRE_*`, no la de Btrfs.

De la misma revisión se descartaron dos cosas: la supuesta inconsistencia de
fechas (2 de agosto 22:13 en CDMX **son** las 04:13 UTC del 3; la bóveda fecha
en UTC) y el reproche de que Thalyx "todavía no es un sistema operativo", que
es [[Decision-Capa-vs-SO-Nuevo|un decreto deliberado]] y no un hallazgo.

### 2026-08-03 — todo verde en hardware, y las dos operaciones del decreto
Se cerró el ciclo del contador de mutaciones (10 hooks, por CPU, acotado al
árbol), se abrió la puerta del atajo (`graph trust`), y se construyeron las dos
operaciones de [[Rollback-vs-Restore]]: `rollback` y `restore`, con snapshots
de Btrfs debajo. Cuatro defectos encontrados y arreglados, **tres de ellos del
arnés y no de Thalyx** — de ahí las reglas 5 y 6 de `CLAUDE.md`.

### 2026-08-02 — la tercera primitiva y el enforcement real
Memoria persistente, montajes idmapped, un uid por módulo, `pivot_root`, perfil
`module_standard`, y la primera demostración de que el LSM deniega de verdad en
hardware.

### 2026-08-01 — los decretos
43 → 61 notas. Modelo de amenaza, formato del manifiesto, commit atómico,
sandbox, permisos JIT, estrategia de pruebas, criterio de salida de la Fase 1.

## Relacionado
- [[Estado-de-Implementacion]] — qué está construido, por crate
- [[Tareas-Pendientes]] — qué está decidido y qué no
- [[Criterio-de-Salida-Fase-1]] — cuándo se puede decir que la fase terminó
- [[00-Indice/Indice-Principal|Índice principal]]
