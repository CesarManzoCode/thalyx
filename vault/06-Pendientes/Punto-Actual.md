---
tipo: estado-vivo
estado: activo
fecha-actualizacion: 2026-08-04
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
> Se describió con tres `no`: sin Btrfs, sin enforcement, sin módulos.
>
> ## Y ahora tiene dónde guardar cosas — 2026-08-03, esa misma noche
>
> **Dos de los tres `no` están cerrados.** El store existe: `sudo make -C image
> store` formatea un disco Btrfs con los tres subvolúmenes decretados e instala
> el `greeter` adentro; PID 1 lo monta por el nombre que le da `thalyx.store=` y
> **nunca lo crea**. La sesión sabe `modulos`, `correr <id>` y `apagar`.
>
> Lo que hace la máquina ahora, entero: arranca, monta su disco, lista el módulo
> que tiene instalado, lo corre, y el módulo le pregunta a Thalyx quién es y qué
> puede leer —porque `correr` no le pasa ningún argumento—, lee lo que le
> concedieron y le niegan `/etc/shadow`. Todo por un socket que él no abrió,
> dentro de la máquina, sin shell.
>
> **Nada de eso se ha ejercido dentro de QEMU todavía.** Ver "Lo que falta
> comprobar" abajo.
>
> ## Y arrancó con su disco puesto — 2026-08-03
>
> ```
>   ok  store        /dev/vda — three subvolumes
>   ok  filesystem   btrfs
>   ok  modules      1: dev.thalyx.greeter 1.0.0
> ```
>
> **De tres `no` en el primer arranque a uno.** El que queda es el enforcement,
> que es el hueco de arquitectura que sigue. La máquina arranca, monta su store
> de Btrfs, y sabe qué tiene instalado.
>
> Cuatro cosas se rompieron entre construir el store y verlo montado, y las
> cuatro fueron del constructor y no del sistema — están abajo, en "Los cuatro
> fallos del camino", porque tres de ellas son la misma regla.
>
> ## Y ahora carga su propio enforcement — 2026-08-03, escrito y sin ejercer
>
> El último hueco de arquitectura. `attach_lsm` invocaba `bpftool` —un segundo
> programa, desde una shell, en una imagen que no tiene ninguno de los dos— y
> buscaba `/lib/thalyx/thalyx_lsm.bpf.o`, un segundo archivo. **El mensaje que
> imprimía sugería romper el decreto que estaba reportando.**
>
> Ahora el objeto BPF va dentro del binario y Thalyx hace las llamadas al kernel
> él mismo: `crates/thalyx-bpf` lee ELF, BTF, la forma de los mapas y las
> reubicaciones CO-RE sin una línea de `unsafe`, y `thalyx-syscall` hace las
> cuatro llamadas. Ver [[Cargador-BPF-Propio]].
>
> **Nada de esto se ha ejercido.** El contenedor no tiene BPF LSM. La etapa 14
> de `verify.sh` es donde se comprueba, y es lo siguiente que hay que correr.
>
> ## Y el cargador funciona — 2026-08-03, dos fallos después
>
> La etapa 14 en la máquina de Cesar: **cargó, atachó, dejó los mapas donde
> `permd` los busca y se soltó limpio.** El cargador propio es real.
>
> Los dos fallos que costó están en [[Cargador-BPF-Propio]]. El segundo importa
> más que el primero porque no era del cargador: **la demo de denegación se negó
> a correr contra enforcement que estaba vivo**, tres líneas después de que la
> misma etapa demostrara que lo estaba. Preguntaba por un directorio que solo
> crea `bpftool`.
>
> Y tirando de ahí apareció algo peor, que llevaba puesto desde antes: **la
> sesión reportaba enforcement preguntándole a `bpftool` si había un mapa
> fijado.** Dos errores en una línea — la imagen no tiene `bpftool`, así que
> adentro contestaba «no» pasara lo que pasara; y un mapa fijado es un lugar
> donde poner permisos, no algo que los lea. Una máquina con todo fijado y nada
> atachado habría reportado enforcement.
>
> Ahora Thalyx le pregunta al kernel qué programas suyos corre un enlace vivo, y
> lo hace con llamadas `bpf(2)` propias, así que **funciona dentro de la imagen**.
> Hay una respuesta más que antes: *parte de los hooks vivos*, que se nombra
> aparte porque es peor que ninguno.
>
> ## Y la etapa 14 salió verde entera — 2026-08-03
>
> `proven 72 · not proven 2 · failed 0`. **Thalyx carga su propio enforcement,
> lo atacha, y ese enforcement deniega**: una conexión negada adentro del cgroup
> y permitida afuera, contra hooks que puso Thalyx y no `bpftool`.
>
> **Ese era el último hueco de arquitectura de la Fase 1.** Lo que queda sin
> probar no es arquitectura: es que llegue el modelo de verdad.
>
> Las dos cosas que la máquina de Cesar no puede establecer siguen siendo las
> mismas: `llama.cpp` no está instalado y el camino del modelo real no está
> escrito, y `verify.sh` no arranca la imagen.
>
> ## Y la máquina ya puede instalar, confirmar y revertir — 2026-08-03
>
> **El objetivo es cerrar la Fase 1**, y el criterio de salida no es una lista
> de componentes: son [[Criterio-de-Salida-Fase-1|seis cosas que hace una
> persona ajena]]. De esas seis, tres pasaban por la sesión y ninguna se podía
> hacer: adentro no hay shell, así que lo que no es un verbo de la sesión no
> existe para esa persona. La sesión entendía seis palabras y ninguna era
> `instalar`.
>
> Ahora entiende `disponibles`, `instalar <id>`, `permisos` y `revertir`. Nada
> de la lógica es nueva —el repositorio local, el camino confiable y el rollback
> ya estaban escritos— y ese era justo el problema: **estaban escritos y no
> alcanzables.**
>
> Y el disco cambió de contenido: lleva el módulo **en un repositorio, sin
> instalar**. Una máquina que arranca con él puesto vuelve el paso 2
> irrealizable, y el paso 3 —el camino confiable— nunca se alcanza. Hay una
> prueba que lee `image/Makefile` para que eso no se deshaga sin que nadie lo
> note, porque deshacerlo mejora lo que la máquina *aparenta*: arrancaría
> listando un módulo.
>
> La etapa 15 maneja el prompt de verdad, con un pty, y trae el control que
> hace falta: **responder que no no instala.** Sin eso, una sesión que
> instalara pase lo que pase pasaría todas las demás comprobaciones.
>
> ## Y construir esto ya no necesita bpftool — 2026-08-03
>
> Cesar decidió mandarle la máquina a una persona ajena **cuando los seis pasos
> sean reales**, no antes. Eso convirtió cada dependencia de construcción en un
> sitio donde esa persona se atora, y la peor era `bpftool`: en Ubuntu y
> derivados —Linux Mint, en este caso— viene en `linux-tools-$(uname -r)`, un
> paquete por versión de kernel cuyo nombre a menudo no coincide con el que está
> corriendo. Y se topaba con eso **después** de compilar un kernel entero.
>
> `lsm/vmlinux.h` ahora está escrito a mano: nueve structs, que es lo que los dos
> programas tocan, en vez de las cien mil líneas que generaba bpftool. Ver
> [[Cargador-BPF-Propio]] y la regla nueva en [[Estrategia-de-Pruebas]] — porque
> esto abrió una forma de mentir sin síntoma, y hay una prueba que la muerde.
>
> ## Y los seis pasos existen — 2026-08-04
>
> **Se puede hacer el criterio de salida entero.** Faltaban dos pasos y los dos
> eran lo mismo: las piezas estaban escritas y no había cómo alcanzarlas.
>
> **El paso 6 no tenía nada detrás.** La sesión no escribía nada en la memoria
> persistente al instalar, así que reiniciar no perdía el contexto — no había
> contexto. Ahora `instalar` y `revertir` escriben por el mismo
> `recollection.rs` que usa `thalyx agent do --task`, y `recuerdos` lo lee. Todo
> vive en `<store>/state/`, que es el subvolumen `system`, que viene del disco:
> hay una prueba que lo afirma contra la tabla de montajes, porque una memoria
> en el tmpfs se ve idéntica hasta el momento de apagar, que es el único que le
> importa al paso 6.
>
> Lo que sale después de instalar, reiniciar y `revertir`:
>
> ```
>   About `session`, you told me:
>     · the human asked: instalar dev.thalyx.greeter
>     · the human asked: revertir
>
>   And this I remember but can no longer confirm:
>     ? installed dev.thalyx.greeter 1.0.0
> ```
>
> **Eso es lo que distingue una memoria de una bitácora**, y es lo que hace que
> el paso 6 valga sin modelo: nadie le avisó que el módulo se fue. El hecho
> quedó atestiguado contra el enlace `current`, `revertir` lo quitó, y la
> máquina fue a ver. Lo que se le pidió sigue intacto porque ningún archivo
> puede volver falso que alguien lo haya dicho.
>
> Cesar decidió el 2026-08-04 que **eso es el paso 6** y que el modelo real deja
> de bloquear la fase. Sigue decretado en [[Gamas-de-Modelo]]. El razonamiento
> está en [[Criterio-de-Salida-Fase-1]].
>
> **El paso 1 tenía máquina y no tenía camino.** `make -C image doctor` junta
> todas las herramientas que faltan y las contesta con una línea de `apt`, sin
> descargar ni compilar nada, y `all` depende de él primero. Lo que detiene a la
> persona ajena nunca es Thalyx: es un paquete, encontrado de uno en uno, cada
> uno después de que lo anterior salió bien. El peor era `pahole` — sin él
> Kconfig descarta `DEBUG_INFO_BTF` en silencio y la culpa cae sobre el
> cargador. El README tiene la sección **Boot it**, que son los seis pasos y
> nada más.
>
> **Un defecto encontrado al hacerlo**, y dio regla nueva: la frase que explica
> un hecho no confirmable decía que algo había cambiado *"without going through
> Thalyx"*, y con `revertir` esa causa dejó de ser cierta. Ninguna prueba se
> rompió. Ver la regla del mensaje que nombra la causa en
> [[Estrategia-de-Pruebas]].
>
> **Nada de esto se ha corrido en hardware.** Es lo siguiente: `sudo
> ./dev/verify.sh`, donde la etapa 15 creció seis comprobaciones, y después
> `make -C image run`.
>
> ## La imagen arrancó con el cargador propio, y le falta un hook — 2026-08-04
>
> `make -C image run` en la Fedora de Cesar. **El cargador funcionó**: llegó
> hasta preguntarle al kernel por sus hooks y dijo exactamente cuál falta.
>
> ```
> no  thalyx-lsm  this kernel does not expose `bpf_lsm_socket_connect`
> ```
>
> `thalyx.config` tenía `CONFIG_SECURITY` y no `CONFIG_SECURITY_NETWORK`. Todos
> los hooks de socket de `lsm_hook_defs.h` están adentro de ese `#ifdef`, así que
> el símbolo **nunca se compiló**. Arreglado: la línea está en `thalyx.config`
> con su párrafo.
>
> Y `config-check` pasó en verde, correctamente — compara lo pedido contra lo
> obtenido, y nadie había pedido esa opción. **Un punto ciego con forma propia**,
> ver la regla nueva en [[Estrategia-de-Pruebas]]. Ahora existe `hook-check`: le
> pregunta al objeto BPF a qué símbolos se engancha (`thalyx enforce hooks`) y
> los busca en el `System.map` del kernel recién compilado, antes de arrancar
> nada. Probado con sus tres respuestas — falta uno, están los dos, y no hay
> kernel construido.
>
> **El resto del arranque salió bien**, y es la primera vez: de tres `no` a dos,
> con el store de Btrfs montado y `filesystem btrfs`.
>
> **Y la etapa 15 se saltó entera**: Fedora no trae `script` — está en
> `util-linux-script`. Los siete controles del paso 6 no corrieron, así que ese
> trabajo sigue sin ejercer en hardware.
>
> ## Y el hook existía y no se le podía enganchar nada — 2026-08-04
>
> Con `CONFIG_SECURITY_NETWORK` puesto, el símbolo apareció y el arranque falló
> un paso más adelante:
>
> ```
> no  thalyx-lsm  attaching `thalyx_socket_connect`: Resource busy (os error 16)
> ```
>
> Faltaba `CONFIG_FUNCTION_TRACER`. BPF se engancha a un hook LSM con un
> trampolín, y sin ftrace dinámico el kernel parcha el texto él mismo esperando
> el NOP de cinco bytes que esa opción pone al principio de cada función. No
> estaba, el `memcmp` falló, y ese camino devuelve `EBUSY` — que se lee como que
> algo más tiene el hook tomado, y no había nada.
>
> `CONFIG_FTRACE=y` ya estaba y es solo el menú: no emite ningún NOP.
>
> Dos arreglos, y el segundo importa más:
>
> 1. Las cuatro líneas en `thalyx.config`, con `DYNAMIC_FTRACE_WITH_DIRECT_CALLS`
>    pedida explícitamente aunque sea derivada, para que `config-check` reporte
>    si no se materializa.
> 2. **`hook-check` pregunta por el artefacto**: `register_ftrace_direct` solo se
>    compila bajo esa opción, así que su presencia en el `System.map` *es* la
>    propiedad. Probado con sus dos respuestas.
>
> Y el mensaje del cargador ahora dice **las dos** causas de `EBUSY` en ese
> camino y que no las puede distinguir. Con su control: otro errno no lleva el
> párrafo.
>
> **Ya son tres opciones del kernel encontradas arrancando**, y la regla nueva en
> [[Estrategia-de-Pruebas]] dice por qué ninguna comprobación de construcción va
> a encontrar la cuarta.
>
> ## Y ahora `verify.sh` arranca la máquina — 2026-08-04
>
> Decidido por Cesar después del tercer arranque a mano. **La etapa 16 arranca
> la imagen en QEMU y teclea los seis pasos**: espera a que la máquina diga que
> es la máquina, y escribe `recuerdos`, `disponibles`, `instalar`, la
> confirmación, `permisos`, `correr`, `revertir`, `apagar`. Después arranca otra
> vez y pregunta `recuerdos`.
>
> **Dos arranques, porque eso es lo que dice el paso 6.** Un proceso nuevo no es
> un reinicio; lo único que cruza entre los dos es el disco.
>
> Y la consola serie **es** un terminal: lo que ve el invitado es `/dev/console`
> sobre `ttyS0`, sea lo que sea el stdin de QEMU. Así que el camino confiable se
> ejerce como lo encuentra una persona, sin `script` de por medio.
>
> El disco se copia primero. Arrancar lo modifica, y una etapa que cambiara el
> disco que alguien construyó haría que la segunda corrida empezara desde otro
> lado que la primera.
>
> `make -C image boot` es lo que corre, y **no construye nada**. `run` depende
> del kernel y de la imagen, y la regla del binario depende de `toolchain`, que
> es `.PHONY` — así que pedir `run` puede arrancar un `cargo build`, y bajo
> `sudo` eso corre como root y deja archivos de root en `target/`. Es el mismo
> fallo por el que `store` se partió en dos, y la misma regla: **la frontera de
> privilegio es la frontera de target.**
>
> **El arnés se ejerció contra una máquina falsa** —una que se queda callada
> hasta estar lista, para que teclear temprano se note, y una que se muere de
> inmediato, que tiene que volver como «nunca llegó al prompt» y no como un
> cuelgue—. La etapa en sí **nunca ha corrido contra una imagen de verdad**: el
> contenedor no tiene QEMU ni kernel que arrancar.
>
> ## Lo que sigue sin verse
>
> **La imagen arrancando con enforcement puesto.** PID 1 llama a `attach_lsm` y
> el kernel de la imagen tiene `CONFIG_BPF_LSM=y` y `CONFIG_DEBUG_INFO_BTF=y`,
> así que debería salir `ok thalyx-lsm`. Sería el tercero de los tres `no` del
> primer arranque, cerrado. Nadie lo ha visto: se ve con `make -C image run`.
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
excepto Thalyx. **Diseñada y construida el 2026-08-03** en
[[API-Interna-de-Modulos]]: protocolo, servidor, el canal por el sandbox, y
`dev.thalyx.greeter`, el primer módulo escrito contra ella.

La Fase 1 tiene **sus tres primitivas** —de las cuatro decretadas; la cuarta es
el [[Scheduler-Predictivo]] y es de Fase 2— y su flujo canónico **construidos y
verificados en hardware real**: 44 comprobaciones en máquina real. Desde
entonces: **520 pruebas**, el agente mínimo que lleva un enunciado hasta un
módulo instalado sin modelo alguno, `thalyx` como PID 1, la imagen que Thalyx
construye para sí mismo, y el disco donde guarda lo que le instalan.

**Los huecos de arquitectura de la Fase 1 están cerrados.** El último era el
enforcement dentro de la imagen; el cargador propio salió verde en hardware el
2026-08-03.

**Y desde el 2026-08-04 los seis pasos del criterio de salida se pueden hacer.**
Lo que falta para cerrar la fase **no es código**: es que los haga una persona
ajena, siguiendo solo el README, sin ayuda. El modelo del agente sigue
decretado en [[Gamas-de-Modelo]] y ya no bloquea la fase — por decisión de
Cesar del 2026-08-04, razonada en [[Criterio-de-Salida-Fase-1]].

## Lo que falta comprobar

Escrito aparte para que no se confunda con lo que sí está probado:

| Qué | Estado |
|---|---|
| El mecanismo del store | **Probado**, etapa 13, en verde el 2026-08-03. |
| El store arrancando en QEMU | **Probado**, arrancó con el disco montado y el módulo instalado. |
| El cargador de BPF propio | **Probado**, etapa 14, en verde entera el 2026-08-03. |
| **El paso 6** | **Escrito y sin ejercer.** La etapa 15 se saltó entera el 2026-08-04: Fedora no trae `script`, está en `util-linux-script`. Es lo siguiente que hay que correr. |
| El `doctor` | Escrito. Sin correr en la máquina de Cesar. |
| **La imagen con enforcement puesto** | **Casi.** Arrancó el 2026-08-04 y le faltaba `CONFIG_SECURITY_NETWORK`; ya está en `thalyx.config` y `hook-check` lo atrapa antes de arrancar. Falta recompilar el kernel y volver a verlo. |
| `thalyx_watch` cargado sin bpftool | No intentado. Diez hooks en vez de dos; el mismo cargador debería servir. |

## Los cuatro fallos del camino, y por qué tres son el mismo

Entre que el store quedó escrito y que la máquina arrancó con él montado, nada
de lo que falló fue del sistema. Los cuatro fueron del constructor:

1. **`sudo make store` no encontraba `rustup`.** `sudo` reinicia el `PATH`. Eso
   es lo chico: de haber funcionado habría corrido toda la compilación de Rust
   como root, con los scripts de build de cada dependencia con privilegio y
   archivos de root en `target/`. **La frontera de privilegio es la frontera de
   target** — `store-stage` construye, `store` formatea y se niega a construir.
2. **`NOT STATIC` sobre un binario perfectamente estático.** La comprobación era
   `file | grep 'statically linked'` y Rust enlaza musl como *static-pie*, que
   `file` llama `static-pie linked`. Ahora lee el segmento `INTERP` del ELF.
3. **QEMU no pudo abrir el disco.** La comprobación era `test -r` y QEMU abre el
   disco para escribir. Y el disco había quedado de root: son **dos
   pertenencias distintas** —el archivo es del host, lo de adentro es de la
   máquina— y confundirlas da un store que o QEMU no abre o la máquina no posee.
4. **Backticks dentro de un mensaje de ayuda**, dos veces. `echo "corre \`sudo
   make store\`"` es sustitución de comandos: el mensaje que explica qué correr
   lo habría corrido.

El 2, el 3 y el 4 son **la misma regla**: comprobar un sustituto de la
propiedad en vez de la propiedad, o escribir sobre una herramienta en vez de
preguntarle. Está escrita en [[Estrategia-de-Pruebas]].

Y hay una lección de arriba de todas: **el 2 mintió durante un rato y la máquina
ya lo había desmentido.** La imagen había arrancado con ese mismo binario como
`/init`; uno dinámico habría dado `No working init found`. Cuando una
comprobación contradice algo que la máquina ya demostró, la comprobación es la
sospechosa. Van siete.

Hay pruebas para los cuatro, y tres de ellas leen el `Makefile`.

## Última corrida verificada

**2026-08-03, Fedora 43, kernel 7.0.11, Btrfs, `bpf` en el orden de LSM,
`main @ f1a6dd0`.**

```
proven 72 · not proven 2 · failed 0
```

Las dos `not proven` son cosas que **todavía no existen**, no cosas que no se
pudieron comprobar: el modelo del agente (`llama.cpp` no está y la ruta real no
está escrita) y arrancar la imagen desde este script, que se hace a mano con
`make -C image run`.

Lo que esta corrida cerró y las anteriores no: **el cargador de BPF propio**.
La etapa 14 cargó los dos programas sin `bpftool`, los enganchó, dejó los tres
mapas donde `permd` los busca, **denegó una conexión adentro del cgroup y la
dejó pasar afuera**, y se soltó sin dejar un enlace vivo.

**Todo lo posterior a `f1a6dd0` está sin correr en hardware**: los verbos de la
sesión, `vmlinux.h` escrito a mano, el paso 6 y el `doctor`.
También ejerció el `EXDEV` en el que descansa el layout, con línea base y
control — porque una afirmación que sostiene un diseño hay que ejercerla, no
citarla.

Reproducirla:

```
git checkout main && git pull && cargo install --path crates/thalyx-cli && sudo ./dev/verify.sh
```

> **El encabezado dice qué commit se está probando.** Existe porque una corrida
> contra código viejo se ve idéntica a una donde el arreglo no funcionó: misma
> etapa, mismo fallo, mismo mensaje. Pasó — dos arreglos estaban en `main` y la
> máquina seguía en la rama de la que salieron. Si la línea no dice `main` y el
> commit que esperas, la corrida no significa nada.

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

### 0. Que una persona ajena haga los seis pasos

**Es lo único que cierra la Fase 1**, y desde el 2026-08-04 no falta código para
que ocurra. Falta correrlo en hardware primero — `sudo ./dev/verify.sh` y `make
-C image run` — y después entregarlo. Cesar decidió el 2026-08-03 que eso pasa
**cuando los seis pasos sean reales**, y ya lo son.

### 1. El agente — su mitad determinista ya está construida

Ya no bloquea la fase; ver el paso 6 en [[Criterio-de-Salida-Fase-1]]. Va
primero de lo que queda, y el motivo es de descubrimiento, no de avance. El ISO desbloquea
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

### 2. La imagen ya arranca; le faltaban tres cosas y queda una

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
2. ~~**El store.**~~ **Hecho el 2026-08-03.** El disco se hace al construir con
   `sudo make -C image store` —Btrfs, tres subvolúmenes, el `greeter` instalado
   adentro— porque `mkfs.btrfs` no puede estar en la imagen, que es la misma
   forma que el problema del LSM y la misma respuesta: el trabajo se mueve al
   momento de construir. PID 1 lo monta por `thalyx.store=` y nunca lo crea. Ver
   [[Construccion-del-ISO]] y la tabla de montajes en [[Journal-y-Snapshots]].
3. ~~**La API interna de módulos.**~~ **Hecha el 2026-08-03.** Protocolo,
   servidor, el canal atravesando el sandbox y `dev.thalyx.greeter`. Ver
   [[API-Interna-de-Modulos]].

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

### 2026-08-04 — los seis pasos existen
El objetivo pasó a ser cerrar la Fase 1, y quedaban dos pasos del criterio de
salida sin nada detrás. Los dos tenían la misma forma: **la pieza estaba escrita
y no había cómo alcanzarla.**

**El 6.** La memoria persistente es la tercera primitiva y está probada en
hardware desde el 2026-08-02, y la sesión no escribía en ella. Ahora `instalar`
y `revertir` escriben por el mismo `recollection.rs` del agente —no una copia— y
`recuerdos` lo lee. Lo que lo vuelve una prueba y no una demostración: después
de `revertir`, la instalación sale como *no confirmable* **sola**, porque quedó
atestiguada contra el enlace `current` que el rollback quitó.

Antes hubo que decidir qué cuenta como el paso 6, porque la bóveda decía dos
cosas distintas. Lo decidió Cesar: la memoria sobreviviendo al reinicio; el
modelo real deja de bloquear la fase sin cancelarse.

**El 1.** `make -C image doctor`. Lo que detiene a la persona ajena nunca es
Thalyx: es un paquete que falta, encontrado de uno en uno y cada uno después de
que lo anterior salió bien. Ahora salen todos juntos, con la línea de `apt` que
los instala, antes de descargar o compilar nada. El peor era `pahole`, cuya
ausencia hace que Kconfig descarte `DEBUG_INFO_BTF` **en silencio** y la culpa
caiga sobre el cargador de BPF varios pasos después.

Y el `doctor` se comprueba a sí mismo: sin `gcc` no puede probar las cabeceras,
y lo dice en vez de callarlo. Regla 3 aplicada al comprobador.

**Un defecto propio, y dio regla nueva.** El párrafo que explica un hecho no
confirmable decía que algo había cambiado *"without going through Thalyx"*.
Cierto mientras la única ruta fuera una edición por fuera; con `revertir` pasó a
ser una explicación segura de una causa que ese código no puede ver. Ninguna
prueba se rompió. Ver [[Estrategia-de-Pruebas]].

También se corrigió el README, que seguía diciendo *"Phase 1 — Thalyx core on an
Alpine base"* — un decreto derogado el 2026-08-03 que sobrevivió en una de las
cuatro puertas de entrada. Es la regla de que una afirmación de ausencia caduca
sola, en su versión más incómoda: caducan también las de presencia cuando nadie
las vuelve a leer.

### 2026-08-03 (12) — dos fallos en hardware, y ninguno era de Thalyx en el sentido esperado
La corrida en la máquina de Cesar dio `proven 59 · failed 2`. Los dos se
arreglaron y los dos enseñaron algo.

**El primero era del arnés.** `verify.sh` activaba
`THALYX_REQUIRE_BTRFS_TESTS` porque había btrfs-progs, y nunca ponía
`THALYX_BTRFS_SCRATCH`, que es lo que ese test necesita para crear un
subvolumen. Exigió una comprobación y le negó su entrada. El error de fondo:
**tener la herramienta y tener dónde usarla son dos hechos**, y en Fedora se
separan de inmediato porque `/tmp` es tmpfs. Ahora se establecen los dos, y el
segundo creando un subvolumen de verdad — `stat -f` dice btrfs también para un
montaje de solo lectura. Séptima vez que el culpable es el instrumento.

**El segundo era real y estaba en el `allowlist` de seccomp.** El módulo moría
con `SIGSYS` en su primera respuesta. La causa la dio `strace` en tres minutos y
no la habría dado leer el código: **un `UnixStream` de Rust lee con `recv(2)` y
escribe con `send(2)`**, no con `read` y `write`. `recvfrom` y `sendto` no
estaban en la lista.

Lo que lo explica es más interesante que el arreglo: el `allowlist` se derivó
empíricamente corriendo módulos reales, que es el método correcto — pero **todos
esos módulos eran scripts de shell, y `/bin/sh` no toca un socket**. El método
cubre exactamente los programas que se usaron para derivarlo. De ahí la regla
nueva de [[Estrategia-de-Pruebas]]: **un sustituto que nunca ejerció el
mecanismo no lo probó.**

`recvfrom` y `sendto` entran; `socket`, `connect` y `bind` siguen fuera. Un
módulo puede **usar** el socket que le dieron y no puede **fabricarse** otro, y
la prueba afirma las dos mitades juntas a propósito: separadas, cada una pasaría
sola y una sola no sirve.

### 2026-08-03 (11) — hay un módulo, y habla
`dev.thalyx.greeter` existe: el primer módulo desde que se borró el que era un
script de shell. Se instala desde un bundle firmado, corre, y **habla con
Thalyx por un socket que nunca abrió**. Lo que sale por pantalla:

```
  dev.thalyx.greeter said:
    I am dev.thalyx.greeter 1.0.0, speaking protocol 1, holding 1 grant(s).
    read 27 byte(s) from .../notes.txt: the vault is the authority
    I asked for /etc/shadow and was refused, which is correct.
```

Las tres líneas dicen cosas distintas. La primera: **un módulo no sabe quién
es**, pregunta, y lo que le contestan sale del manifiesto firmado. La segunda:
la línea base. La tercera: la denegación — sin la segunda no probaría nada,
porque un Thalyx que negara todo se vería igual.

Y una cuarta que no sale por pantalla: **ejecutado a mano no arranca**. No
porque compruebe una licencia, sino porque en el descriptor 3 no hay nadie.
Eso es [[Filosofia-Fundacional]] vuelta comprobación.

Lo construido: `thalyx-syscall` coloca el descriptor (`place_on`,
`spawn_with_channel`, `inherited_channel`), `launch.rs` lo lleva por las dos
etapas del sandbox, y `thalyx-core/api.rs` es el servidor.

**El hallazgo que más importa está en `api.rs`, y es de seguridad.** El
servidor **no está dentro del sandbox**: corre como Thalyx, con el alcance de
Thalyx. Un módulo que pide una ruta le está pidiendo a *Thalyx* que la abra, así
que la raíz vacía del sandbox y el LSM no protegen nada ahí. Cada ruta se
comprueba dos veces: por el nombre, y por **lo que el kernel resuelve** — que es
lo único que atrapa un symlink plantado dentro de un directorio que el módulo
puede escribir. Esa era la vía que sí habría funcionado.

Etapa 12 en `verify.sh`, con su control. Y **una guarda mía salió mal primero**:
se disparaba con "cgroup2 montado" cuando la condición real es "el LSM está
cargado", así que exigió a este contenedor algo que no puede hacer y reportó
roto a Thalyx. Es la regla 3 otra vez: un salto que se dispara solo se ve
idéntico a un fallo real.

Falta la ruta confinada —el canal por dos `exec` y un filtro seccomp— que solo
se puede comprobar en máquina con LSM.

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
