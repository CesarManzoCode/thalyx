---
tipo: pendientes
estado: activo
fecha-decreto: 2026-08-01
tags: [pendientes, tareas, roadmap-decisiones]
---

# Tareas pendientes (explícitas)

Lista viva de decisiones y trabajo que todavía falta cerrar. Actualizar el estado en el frontmatter de cada nota enlazada conforme se resuelvan.

> Para saber **qué está construido** en vez de qué está decidido, ver [[Estado-de-Implementacion]].
>
> Para saber **dónde quedó el proyecto y qué sigue ahora mismo**, ver [[Punto-Actual]].

## Pendientes de implementación

> **Encuadre de Cesar, 2026-08-08.** Lo que quedó suelto de la Fase 1 **no
> pertenece a ninguna fase**, porque nada de ello bloquea nada. Sus palabras:
> *«no quedó nada de la fase 1, esas cosas que quedaron no pertenecen a ninguna
> fase real debido a que ninguna bloquea nada, son solo cosas del proyecto que se
> arreglarán cuando se necesiten arreglar»*. Se quedan escritas aquí porque son
> ciertas; dejan de leerse como deuda de una fase.

- [x] **Terminar una inferencia de verdad** — hecho el 2026-08-08, y después repetido en tres gamas. El primer intento falló porque Thalyx pedía `llama-cli`, que desde que `llama.cpp` partió sus herramientas es el frontend de chat y abre una sesión en vez de completar; con `llama-completion` la inferencia completó, y el 2026-08-08 corrieron `check`, `grammar-check` y el banco de 20 casos en ligera, media y alta. **La gama máxima no**: el proceso murió por falta de memoria antes de la primera inferencia. Ver [[Gamas-de-Modelo]] y [[Punto-Actual]].
- [x] **Un caso de aislamiento con un permiso sobre un archivo y usuario propio** — **hecho el 2026-08-25, y corre en el contenedor.** El 2026-08-04 un punto de montaje creado como directorio sobre un archivo rompió el `correr` de la máquina, y ninguna prueba lo vio porque **todos los permisos de todas las pruebas eran directorios**. Ahora hay dos en `isolation.rs`: uno que concede **un archivo** con permiso de escritura, arma la raíz remapeada de verdad y comprueba en el anfitrión que lo escrito llegó al mismo archivo y que no cambió de dueño; y su control, que concede el archivo y afirma que **el vecino de al lado no viene con él** —que es la forma obvia de hacer funcionar un permiso sobre un archivo, montando su carpeta, y la que entrega todo lo demás—. Los dos se rompieron a propósito antes de creerles. De paso, el protocolo de lanzamiento remapeado dejó de estar copiado tres veces: es la lección de `create_target_like` una capa arriba. Ver [[Estrategia-de-Pruebas]].
- [x] **G1: lanzar un programa que nadie firmó** — **hecho el 2026-08-25**, y es el punto que bloqueaba la vara de [[Filosofia-Fundacional]] desde que se midió el 23. `ejecutar <ruta>` corre un programa ajeno con el mismo confinamiento que un módulo —cgroup, usuario propio, pivote, filtro de llamadas— y **sin modo degradado**, porque `sin-confinar` se justifica en que alguien firmó el módulo y aquí nadie firmó nada. No recibe canal con la API: un invitado corre, no se le da la casa. Ve su propia carpeta, las rutas de sistema de sólo lectura y lo que se nombre con `leyendo`/`escribiendo`, cada cosa confirmada por el [[Camino-Confiable]] antes de que el proceso exista. El journal lo llama `run_foreign`. Etapa 36 de `verify.sh` más seis pruebas de integración; cuatro de ellas necesitan los controladores delegados y dicen `NOT PROVEN` donde no los hay. Ver [[Programas-Ajenos]].
- [ ] **Que Thalyx encienda y apague el enforcement él mismo.** Encontrado el
      2026-08-25 al arreglar la confusión entre *cargado* y *negando* (ver
      [[Programas-Ajenos]], revisiones). Thalyx ya **lee** el modo —el mapa
      `thalyx_enforcing`, con `bpf(2)`, sin `bpftool`— y por eso `ejecutar` se
      niega mientras el kernel sólo observa. Pero **cambiarlo** sigue siendo
      `make -C lsm enforce`, que es `bpftool`, que la imagen no tiene y no va a
      tener: dentro de la máquina no hay forma de pasar de observar a negar. Es
      el mismo hueco que [[Cargador-BPF-Propio]] cerró para cargar, sin cerrar
      para el modo. Barato: una escritura de cuatro bytes en un mapa que ya se
      abre.
- [ ] **Cuánto dura una concesión a un invitado.** Encontrado el 2026-08-25 al
      arreglar el piso de lectura (ver [[Programas-Ajenos]], revisiones). Una
      entrada de política tiene **una** fecha de vencimiento, y las concesiones
      de `ejecutar` son JIT: **treinta segundos**. Pasados, expira la entrada
      entera —el piso de lectura incluido—, así que `ejecutar leyendo <ruta> …`
      no puede correr más de medio minuto. Sin concesiones no vence, que es por
      qué `node --version` sí sirve. **La vara del proyecto es un agente ajeno
      trabajando aquí, y un agente corre minutos**, así que esto la bloquea.
      Es decisión de Cesar y no un arreglo: las opciones son que la concesión
      dure lo que dure la corrida —`release()` ya la retira al salir, pero se
      pierde el respaldo del kernel contra un userspace colgado—, que el piso no
      venza aunque las concesiones sí —cuesta cambiar el programa BPF—, o subir
      el plazo, que sólo mueve el problema. Cruza con `E1`.
- [ ] **Cargar `thalyx_watch` con el cargador propio.** Es lo único que queda de la lista de "lo que falta comprobar" de [[Punto-Actual]]. Diez hooks en lugar de dos, y el único tipo de mapa que el watcher usa y el LSM no es `PERCPU_ARRAY`. Probable no es comprobado, y no se puede intentar en el contenedor: faltan las cabeceras de `libbpf` para compilar el objeto.
- [ ] **Probar `net/outbound` de punta a punta en hardware.** Que el LSM deniegue a un módulo sin la concesión está demostrado y es reproducible; que un módulo **con** la concesión abra una conexión está implementado, cubierto por pruebas unitarias y nunca ejercido en una máquina. Ver [[Permisos-JIT]].
- [x] **Consumir el ringbuf `thalyx_mutations`** — construido el 2026-08-10 como el verbo `cambios`, punto B3 de [[Superficie-para-el-LLM]]. `crates/thalyx-watch/src/ring.rs` sigue el protocolo del anillo sobre bytes (diez pruebas, incluido el registro que cruza el final) y `thalyx-syscall` hace los dos `mmap`. **Consumirlo no era código BPF**, que fue la razón por la que estuvo parado: el productor ya estaba escrito y el consumidor es código de usuario. Lo que un anillo **no** puede dar y la respuesta dice: no es una historia —leerlo lo vacía— y no nombra archivos, sólo cgroup, pid, tipo y programa. Para reindexar de forma incremental hace falta además que alguien lo vacíe continuamente y guarde lo vaciado, y **eso es una pieza que corre todo el tiempo, que la imagen no tiene**: es una decisión de Cesar, no un pendiente de código. Etapa 27 de `verify.sh`.

- [x] **De dónde saca el instalador el kernel cuando corre dentro de la máquina** — resuelto el 2026-08-07. `medium.rs` lee FAT32 y busca un volumen etiquetado `THALYX` con `\EFI\BOOT\BOOTX64.EFI` adentro, se niega si hay dos, y excluye el disco de destino para que reinstalar sea posible. **Corregido el mismo día**: la primera versión pedía sólo el archivo, que es el *fallback* de UEFI y por lo tanto está en todos los medios de arranque del mundo — encontró la ESP de la Fedora del anfitrión y copió su gestor de arranque al disco reportando éxito. `thalyx disk medium` contesta a qué disco iría. Los verbos son `discos` e `instalar-en <disco>`, porque adentro no hay shell y lo que no es un verbo de la sesión no existe. No monta nada: los bytes se leen igual que se escribieron, así que no hace falta `CONFIG_VFAT_FS`. Ver [[Construccion-del-ISO]].
- [x] **El store por etiqueta en una máquina instalada** — decretado el 2026-08-06 y **construido el 2026-08-07**, que es cuando se notó que nunca se había implementado. Sin eso el disco que produce el instalador arranca y reporta que no tiene store, porque la línea de comandos va compilada dentro del kernel y no puede nombrar un dispositivo. `thalyx.store=` sigue ganando cuando está; sin él se le pregunta a cada disco cómo se llama, ninguno se reporta y **dos se niegan**. `thalyx disk find` lo pregunta sin ser PID 1. Ver [[Construccion-del-ISO]].
- [x] **El driver que hace del medio un disco** — encontrado y agregado el 2026-08-07, sin haber corrido nada. `CONFIG_USB_STORAGE` no estaba, y sin él el acto 2 falla en el primer `discos`: el firmware lee la USB por obligación de la especificación UEFI, así que la máquina arranca y se ve sana, y `instalar-en` es lo que descubre que el kernel nunca vio ese disco. Ver [[Estrategia-de-Pruebas]] y [[Construccion-del-ISO]].
- [x] **Cerrar el acto 2b instalando en una segunda memoria USB** — **hecho el 2026-08-07, y con eso la Fase 1 está cumplida.** Arrancó de la memoria de 3 GiB, `instalar-en /dev/sdc` sobre la de 7 GiB, `apagar`, medio fuera, y la máquina arrancó de ese disco encontrando su store por la etiqueta. Los dos huecos quedan escritos como huecos en [[Criterio-de-Salida-Fase-1]]: el disco era removible y esa máquina no tiene NVMe.
- [x] **Averiguar por qué el arranque tarda ~40 segundos** — resuelto el 2026-08-07 por `nucleo lento`, al primer intento: era la **consola serie a 9600 baudios**, el valor por omisión del driver 8250 cuando `console=ttyS0` no lleva velocidad. Un hueco de **18.27 s en el segundo 0.07**, antes de tocar un disco, que descartó «la USB es lenta» por su posición y no por su tamaño. El puerto serie de QEMU es un pty y no tiene baudios, así que el precio era cero en toda máquina donde esto se había probado — la quinta vez que el anfitrión hacía algo gratis, y la primera en que lo que hacía **existía en hierro y costaba**. Arreglado con `console=ttyS0,115200` y comprobado: **38.5 s → 5.7 s**, con el hueco de 18.27 s → 1.53 s, factor 11.94 contra el 12.0 predicho. El puerto se llevaba 35.8 de los 38.5 segundos. Ver [[Estrategia-de-Pruebas]] y [[Arranque-en-Hierro]].
- [x] **Thalyx corría en dos núcleos de doce** — encontrado el 2026-08-07 en el mismo arranque: `CPU topo: CPU limit of 2 reached. Ignoring further CPUs`, en un Ryzen 5 5600G. Nadie eligió 2: `allnoconfig` corre con SMP apagado, donde `NR_CPUS` es 1, y encender SMP después sólo lo sube al piso de su rango. Puesto `CONFIG_NR_CPUS=64`. **`config-check` es estructuralmente ciego a esto** — compara lo pedido contra lo obtenido, y una opción que nadie pidió no tiene línea que comparar; es el mismo hueco que dejó pasar `CONFIG_SECURITY_NETWORK` y `CONFIG_USB_STORAGE`. Por eso la afirmación vive en `init.rs`. Falta confirmar en hierro que la línea de `CPU topo` desapareció, con `nucleo`.
- [ ] **Decidir si Thalyx aprende a instalarse al lado de otro sistema.** **No bloquea nada: la Fase 1 está cerrada al 100% sin esto** (Cesar, 2026-08-08). Hoy `thalyx install` escribe una GPT nueva sobre el disco entero, así que instalar en el disco interno sólo es posible destruyendo la Fedora de Cesar, que es la única máquina que verifica este proyecto. Se puede construir sin ceder [[Filosofia-Fundacional]]: particiones en el espacio libre de la GPT existente, el kernel en la ESP que ya está bajo `\EFI\thalyx\`, y el menú del firmware eligiendo — que es el firmware, no un segundo programa. **Lo caro no es el código, es dónde escribe.** Decisión de Cesar. Ver [[Criterio-de-Salida-Fase-1]].
- [x] **Correr el acto 2a: arrancar desde una USB en la PC de Cesar** — hecho el 2026-08-07. Un firmware real arrancó Thalyx de una memoria física; la pantalla salió por el framebuffer de ese firmware en un monitor real, la memoria apareció como `/dev/sdb2`, el store se encontró por la etiqueta, el LSM se enganchó, y el teclado físico funcionó. Ver [[Punto-Actual]] y [[Arranque-en-Hierro]].
- [x] **Saber qué es `usb 1-6` en la máquina de Cesar** — resuelto el 2026-08-07 desde Fedora: un **receptor inalámbrico Telink** (`248a:16ab`) de teclado y ratón, *full speed*, con dos interfaces HID. Enumera en 1.2 s bajo Fedora y agota el plazo bajo Thalyx. **No es el teclado con el que escribió** — hay otro HID en el bus 3 — así que desconectarlo es el atajo y el control a la vez. **No se agregó ninguna opción de kernel**: falta la medición, y ahora es posible porque `nucleo` es alcanzable.
- [x] **Decidir cómo sobrevive la sesión al ruido del kernel** — construido el 2026-08-07. Consola en emergencias más un aviso propio del prompt antes de cada línea, con el número de secuencia del kernel como cursor y «at least» cuando el buffer se dio la vuelta. Ver [[Punto-Actual]] y las dos reglas nuevas en [[Estrategia-de-Pruebas]].
- [x] **Correr `discos` en hierro** — hecho el 2026-08-07, y encontró el defecto más peligroso hasta ahora: listaba **particiones como discos**, incluidos 444 GiB de la Fedora de Cesar, bajo la línea que dice que todo lo que haya se pierde. `partitions::of` devolvía `Ok([])` para una partición en vez de errar, así que el filtro descansaba en una propiedad que sólo existía en un comentario. Arreglado en `discos` **y** en `install`, que ahora se niega antes de escribir un byte. Ver [[Estrategia-de-Pruebas]].
- [x] **AHCI y SATA en hierro** — probados el 2026-08-07: `sd 9:0:0:0: [sda]` y el disco de 447 GiB con la Fedora adentro. **NVMe sobre silicio real queda incontestable en esa máquina**, que no tiene ninguno; no es que el driver falle, es que no hay hardware.
- [x] **Correr `nucleo` en hierro** — hecho el 2026-08-07: 10 líneas de problema entre 714 registros, y **ninguna del USB**. El `-110` no volvió a ocurrir.
- [ ] **El `-110` del USB es intermitente y sigue vivo.** **No bloquea nada** — la Fase 1 está cerrada al 100% sin esto (Cesar, 2026-08-08). En simple: cuando se conecta algo por USB el kernel le pregunta *«¿quién eres?»* y el aparato debe contestar con su ficha de identidad; `-110` es **se acabó el tiempo de espera**. Cesar confirmó que nunca desconectó el receptor Telink, así que estaba puesto en la corrida donde el error no apareció: ocurre en unos arranques y no en otros. **El síntoma sí está resuelto** — con la consola en emergencias ya no pisa el prompt, y la sesión avisa con su propia línea — pero la causa no. Un fallo que aparece en un arranque y no en el siguiente se parece más a un dispositivo marginal que a un hueco de `thalyx.config`, y **ninguna opción de kernel está justificada** hasta tener más corridas. Nota: el Telink es el receptor de teclado y ratón (`usbhid`, puerto 6), **no** el WiFi (`rtl8xxxu`, puerto 8).
- [x] **Correr `make -C image run-hardware`** — hecho el 2026-08-07. Es el acto 2 hasta donde una máquina alcanza: xHCI con teclado USB, un NVMe y un AHCI en blanco, y la imagen instalada colgada como memoria USB. **No cierra el acto 2** — no hay silicio real ni una memoria física — pero sí respondió que los cuatro grupos de controladores compilan, enlazan y producen los dispositivos con los nombres correctos: `hid-generic` creó un dispositivo de entrada desde un teclado USB, el store apareció en `/dev/nvme0n1p2`, y la máquina arrancó de ese NVMe sin medio puesto. Ver [[Criterio-de-Salida-Fase-1]].
- [ ] **Qué lleva el store de una máquina recién instalada.** Queda vacío: la imagen no puede llevar el `greeter` —lleva el kernel y un programa— así que una PC recién instalada arranca con un store bueno y **nada que instalar**, y los pasos 2 a 6 del criterio no se pueden hacer *en ella*. En la máquina de desarrollo sí, porque `make -C image store` pone el bundle en el repositorio. Es la pregunta de cómo llega el software a una máquina que no es ésta, o sea la Fase 2. Ver [[Criterio-de-Salida-Fase-1]].


## La terminal usable, decidida el 2026-08-09

Cesar preguntó cuánto falta para un sistema en el que se pueda trabajar sin el
agente, y la respuesta medida contra el código fue que la capa 1 de
[[Principio-Doble-Ruta]] —no negociable— no tenía ninguna implementación. El
orden es por dependencia y lo eligió él. Del 1 al 7 se prueba en el contenedor.

- [x] **1. Dónde estoy y moverme** — `ir`, `donde`. `/home` ya estaba montado
      desde el subvolumen `user`; lo que faltaban eran los verbos.
- [x] **2. `ver` y `leer`** — mirar sin tocar. `crates/thalyx-files`.
- [x] **3. La terminal como terminal** — hecho el 2026-08-09. Flechas, borrar a
      media línea, historial de 500, y tab que completa verbos al principio de
      la línea y nombres de archivo después. `crates/thalyx-term` es puro y se
      prueba sin terminal; el modo crudo es una guarda que devuelve la terminal
      al soltarse, porque una sesión que sale sin hacerlo deja la máquina
      inservible. Encontró dos defectos sobre quién es dueño de `stdin`, y ahora
      hay **un solo lector**: `term::read_answer()`.
- [x] **4. `mkdir`, `touch`, `cp`, `mv`, `rm`** — hecho el 2026-08-09, con
      comodines `*` y `?`. Primera pieza construida bajo el decreto del objetivo:
      cada operación devuelve un `Done` con **qué pasó, dónde, y los bytes
      exactos**, y las dos caras leen ese mismo hecho. Nada sobrescribe sin
      pedirlo: `Exists` es su propio error. Un enlace se copia como enlace y se
      borra como enlace, nunca como lo que apunta. `*` no cruza `/`, que es lo
      que impide que `rm *` alcance carpetas que nadie nombró, y no toca ocultos
      salvo que el patrón empiece con punto. `rm` con varios blancos los lista
      **antes** de tocar nada, porque `/home` es el único sitio que ningún
      rollback nuestro puede devolver.
- [x] **4b. La cara estructurada, expuesta** — hecha el 2026-08-09.
      `structured on` en la sesión y cada verbo de archivos contesta un objeto
      JSON por renglón; `structured off` devuelve las oraciones, y la
      confirmación **carga la salida** porque en la imagen no hay una segunda
      terminal. Tres cosas la hacen otra cara y no otro impresor, y las tres son
      la regla de desempate del decreto: no esconde nada (`-a` y `-l` no le
      cambian nada a un programa, porque nunca se le estaba dando menos), los
      tamaños son exactos, y **el silencio nunca es respuesta** — un `cd` que a
      una persona no le imprime nada sí contesta, porque un parser no distingue
      un silencio que significa «me moví» de uno que significa que la sesión se
      murió. El marco es **un renglón tecleado, exactamente un objeto**: `rm` con
      varios blancos contesta uno con `count` y `results`, porque un límite
      definido de un solo lado no es un límite. Encontró dos defectos en la cara
      humana, ver [[Punto-Actual]]. `crates/thalyx-files/src/machine.rs`, etapa
      21 de `verify.sh`.
> **El catálogo entero está en [[Superficie-para-el-LLM]]** desde el 2026-08-09,
> con el criterio de cinco costos que decide si algo entra. Lo de aquí abajo son
> los puntos de ese catálogo que ya tienen orden; los demás siguen ahí sin
> decidirse, y **estar en el catálogo no autoriza construir nada**.

- [x] **4c. Las otras cosas que la sesión sabe, con su segunda cara** —
      **cerrado del todo el 2026-08-23.** `estado` y `recuerdos` tuvieron su
      objeto el 2026-08-09, el índice entero llegó ese mismo día (`indexar`,
      `depende`, `usan`) y el journal (**F2**) el 2026-08-10 como `historia`. Lo
      que faltaba eran **los seis verbos de módulos** —`disponibles`, `instalar`,
      `modulos`, `correr`, `permisos`, `revertir`— más `nucleo`, `discos` e
      `instalar-en`, y con ellos el ciclo completo de lo único que Thalyx existe
      para dejar hacer estaba en prosa. Ahora ese ciclo se corre entero por la
      cara estructurada. Se cerraron también los tres que se creían sin nada que
      contestar (`limpiar`, `salir`, `apagar`), que eran el último sitio donde
      quedaba silencio. **La lista de verbos sólo-prosa está vacía y hay una
      prueba que lo afirma**; la etapa 22 maneja veintiún verbos y compara el
      cable contra `describe`. Ver [[Superficie-para-el-LLM]].
- [x] **Los tres que se dijeron «sólo hierro»** — **cerrado el 2026-08-10, y dos
      de los tres no necesitaban hierro.** `D2` (el intento con nombre) y `B3`
      (qué cambió desde X) se construyeron: el primero porque la propiedad bajo
      prueba era la política y no Btrfs, el segundo porque consumir un ringbuf
      es código de usuario y no código BPF. **`E1` es el único que queda**, y la
      razón cambió dos veces: primero no era hierro sino que **no había a qué
      darle la concesión** —faltaban `G1` y `G2`—; desde el 2026-08-25 `G1` está
      construido como `ejecutar` ([[Programas-Ajenos]]) y sí hay a qué dársela.
      Lo que le falta a `E1` ahora es lo suyo: que la concesión **expire** y que
      la tarea sobreviva a la corrida. Ver [[Superficie-para-el-LLM]].
- [x] **Los tres que se pueden hacer aquí y quedaron por tiempo** — **hechos el
      2026-08-10.** `B1` acotar toda respuesta larga con su total (`limite=` y
      `cursor=`), `C2` búsqueda por símbolos (`buscar`), y `F2` el journal
      legible desde afuera (`historia`). Con eso el catálogo va **catorce de
      diecinueve** y lo único suyo que falta es `E1`.
- [x] **5. Editor de texto** — hecho el 2026-08-22, **las dos caras en una
      entrega** por decisión de Cesar. `crates/thalyx-edit` es el motor y las dos
      caras lo llaman: `editar <archivo>` abre una pantalla, `editar <archivo>
      cambiar 12 <texto>` direcciona renglones y contesta un objeto. Es el primer
      verbo donde las dos caras difieren de *forma*, y por qué se resolvió así
      está en [[Editor-de-Texto]]. Etapa 29 de `verify.sh`; la mitad de pantalla
      se ejerce con un pty de verdad, así que **no espera hierro**. Tres cosas que
      construirlo enseñó están en [[Estrategia-de-Pruebas]]: una tecla que el
      kernel se come, un pty sin tamaño de ventana, y una confirmación que se
      traga la tecla que la contesta.
- [x] **6. `buscar` por nombre y por contenido** — hecho el 2026-08-23, y son
      **dos verbos nuevos** por decisión de Cesar: `encontrar <patrón>` por
      nombre y `contenido <texto>` por texto, con `buscar` intacto en su tercera
      pregunta —dónde se declara un nombre y quién lo usa, desde el índice—.
      Tres preguntas, tres verbos, porque un verbo cuyo significado depende de
      una bandera se puede pedir mal en silencio y las tres respuestas se ven
      igual. El texto es **literal**, también decisión suya: la imagen lleva el
      kernel y un programa, y un dialecto de regex aquí sería decidir un pedazo
      del punto 9 adentro de una caja donde nadie lo buscaría. Las banderas van
      adelante y el sujeto es el resto del renglón, así que un texto con espacios
      no necesita comillas —que también son el punto 9—. La caminata del árbol se
      movió a `thalyx-files` para que siga siendo una sola: son cuatro llamadores
      ahora, y los dos nuevos son los que una persona compara contra el índice.
      Etapa 30 de `verify.sh`, con `find(1)` y `sed(1)` de controles. Ver
      [[Busqueda]].
- [x] **7. Procesos** — hecho el 2026-08-23. `procesos [patrón]`, `memoria` y
      `matar <numero> [forzar]`, todo sobre `/proc`. `matar` manda la señal por
      un **pidfd**, así que no puede caer en un número reciclado: llega al
      proceso para el que se abrió el descriptor o falla, y no hay tercer
      resultado. Por omisión `TERM` —que un programa puede atrapar para guardar
      lo que tenía— y `forzar` manda `KILL`. Se niegan PID 1 y la propia sesión,
      cada uno nombrando el verbo que hace ese trabajo bien (`apagar`, `salir`),
      y también el `0` y los negativos, que para `kill(2)` son *todo* y un
      *grupo*. `ensayo matar` es el ensayo que más importa, porque un proceso no
      se puede volver a escribir. `memoria` mantiene `libre` y `disponible`
      separados y nombra cuál contesta la pregunta. Etapa 31 de `verify.sh`, con
      línea base y controles. Ver [[Procesos]].
- [x] **Instalar dos veces sobre el mismo disco** — **arreglado y comprobado en
      hierro el 2026-08-23** (`proven 138 · not proven 2 · failed 0`). La espera
      por las particiones preguntaba si existía el nodo, y en una segunda
      instalación los nodos de la tabla anterior siguen ahí: la condición estaba
      cumplida antes de empezar. Ahora la espera **abre** la partición, y las
      particiones se devuelven ya abiertas y se sostienen así hasta el final,
      porque cerrar el disco entero hace que el kernel lo reexamine por su cuenta
      y ese segundo barrido borra y rehace cada partición. Ver [[Punto-Actual]].
- [x] **8. Red** — **decidido por Cesar el 2026-08-23: se ve, no se usa.** De las
      110 opciones del kernel ninguna era una tarjeta de red; ahora son 118 y las
      ocho nuevas son los menús y cuatro drivers —`virtio_net`, `e1000`,
      `e1000e`, `r8169`— cada uno con su razón escrita al lado. El verbo es
      `red`, con dos caras, y **dice en la respuesta que no se puede usar**: no
      hay dirección, no hay DHCP y no sale un paquete. Etapa 35, con `iproute2`
      de control porque lee netlink y no sysfs. Ver [[Red]].
- [ ] **Salir a internet, cuando la Fase 2 diga a dónde** — DHCP, DNS y TLS, los
      tres dentro de `thalyx` porque aquí no hay programas aparte. No se hizo con
      el punto 8 porque lo que compraría —que el store traiga módulos de algún
      lado— depende de una pregunta de Fase 2 que sigue sin contestarse: de
      dónde. Ver [[Red]].
- [x] **9. Decidir si Thalyx tiene lenguaje de shell** — **decidido por Cesar el
      2026-08-23: hay citado y no hay lenguaje**, *«lo que sea más fácil de
      cubrir por ahora, pero en un futuro sí tendremos que hacer shell completo,
      no ahora, pero estemos preparados»*. El renglón se parte en palabras con
      las reglas de POSIX hasta donde POSIX llega hoy, y una comilla sin cerrar
      se niega en vez de adivinarse. **La expansión se queda en el verbo y eso es
      decreto**: `rm "*.log"` es un nombre y `encontrar "*.rs"` sigue siendo un
      patrón, igual que en bash y en `find`. El texto de `editar` es la única
      excepción y se toma del renglón, porque una sangría perdida no se ve.
      Etapa 33. Ver [[Palabras]].
- [ ] **El shell completo, cuando toque** — tuberías, redirección, variables.
      Decidido que llega algún día y que **hoy no**. Lo que ya quedó preparado
      está en [[Palabras]]; lo que falta cuando se retome: qué hace `|` con dos
      caras, si lo que viaja son bytes o filas, y el completado con tabulador,
      que hoy no sabe de comillas.
- [ ] **Decidir si `/home` deja de estar montado `NOEXEC`.** Aplazado por Cesar
      el 2026-08-09 —*«déjalo bloqueado pero cuando tengamos que decidir,
      explícamelos bien»*— así que **queda una deuda de explicación, no sólo una
      decisión**: cuando esto se retome hay que exponerle los trade-offs
      completos, no en tres líneas. Lo que hay que cubrir: qué gana un
      desarrollador y qué pierde un usuario que no lo es; por qué hoy la
      pregunta es hipotética (sin red y con el store vacío no existe el programa
      que se querría correr); qué superficie abre exactamente —cualquier byte
      que aterrice en `/home`, bajado o escrito por un módulo con permiso, puede
      volverse un proceso—; y cuáles son las salidas intermedias, como marcar
      ejecutable archivo por archivo o un subdirectorio propio sin `NOEXEC`.
      **Nada tocado.**

- [x] **Decidir cómo se llaman los comandos** — resuelto el 2026-08-09:
      **estándar primero, español también.** `ls`, `cd`, `cat`, `pwd`, `clear`
      son los que enseña el banner y los que aprende la gente; `ver`, `leer`,
      `ir`, `donde`, `limpiar` siguen funcionando, igual que los nueve verbos en
      español que ya existían. La razón es de adopción y es de Cesar: un sistema
      cuya primera pantalla ofrece un vocabulario que nadie ha visto *«parece
      juguete más que sistema operativo serio»*. **Un nombre no es un programa
      ajeno** — todos son el mismo Rust dentro de `thalyx`, y `make -C image
      count` sigue diciendo uno.

- [x] **Decidir si Thalyx tiene lenguaje de terminal** — decidido a medias el
      2026-08-09, **partido en dos y a propósito.** Los comodines (`*.txt`) y la
      redirección a archivo (`>`) entran junto con copiar/mover/borrar, porque
      son notación de uso diario y no un lenguaje. Las tuberías (`|`) van
      después. **Los guiones y las variables quedan sin decidir**, y no por
      pereza: ahí la pregunta deja de ser «¿tiene notación?» y pasa a ser «¿tiene
      lenguaje de programación?», que es como la gente termina construyendo
      software encima del sistema sin pasar por los módulos — exactamente lo que
      [[Filosofia-Fundacional]] apuntaba con *«no a través de scripts de
      shell»*.

- [x] **Ampliar la gramática del agente más allá de `install_module`.**
      Hecho el 2026-08-24, por decreto de Cesar: **todo el catálogo**. Las dos
      condiciones que este pendiente ponía se resolvieron, y ninguna de las dos
      era la que parecía.

      La abstención dejó de ser expresable exactamente como se temía, y la
      salida fue darle palabra propia (`nothing`) en vez de quitarle el
      significado a la lista vacía: **todas las muestras capturadas de un
      modelo real absteniéndose usan la lista vacía**, y la regla 6 dice que
      una muestra reescrita ya no es la muestra. Las dos siguen valiendo.

      Lo de los argumentos resultó ser más grande de lo escrito, y no estaba
      en el lado del argumento. `assemble` escribía `InstallModule` en cada
      contrato porque no había otra cosa que escribir, así que un `disks`
      habría producido un contrato para instalar un disco. Un plan tiene dos
      formas ahora, y la de verbo **no llegaba a `origins.validate()`** — la
      regla de procedencia habría quedado con una puerta rotulada `read`. Se
      valida en los dos caminos. Ver [[Punto-Actual]].

      Lo que **no** se decidió y sigue abierto: `agent do` sólo lleva a cabo
      instalaciones. Poder decir una cosa no es poder que se haga; todo lo
      demás pasa por el verbo, en una terminal. Ensancharlo es otra decisión
      de Cesar.

## Que un agente ajeno pueda trabajar aquí

Decretado el 2026-08-09: la vara es que **Claude Code y cualquier otro agente ya
escrito corran sobre Thalyx mejor que sobre Linux o macOS**. Ver
[[Filosofia-Fundacional]]. Hoy **no arrancarían** — y desde el 2026-08-23 se
sabe por qué y por qué no: el filtro de llamadas casi no estorba, y lo que
bloquea es que nada lanza un proceso arbitrario y que la imagen no tiene libc.
Ver [[Que-Necesita-Un-Agente-Ajeno]].

- [x] **Averiguar qué necesita exactamente un agente ajeno para arrancar** —
      **medido el 2026-08-23**, con Claude Code 2.1.241 bajo `strace`. Era
      barato, como decía, y **la respuesta corrige la frase que estaba debajo**:
      de las 41 llamadas al sistema que hace para arrancar, `module_standard` ya
      permite 40, y la que falta era una sola (`sched_setscheduler`, resuelta el
      2026-08-24 con un guardia por argumento: **41 de 41**). De las 19
      rutas que abre, 13 caen dentro de lo que un módulo ve. Donde «no
      arrancaría» sí es cierto es en el enlazador: la imagen lleva `/init` y
      nada más, así que no hay libc — que es exactamente la pregunta abierta del
      ABI de los módulos, hecha por el agente antes que ninguna otra. Se
      reproduce con `dev/foreign-agent-needs.sh`. Ver
      [[Que-Necesita-Un-Agente-Ajeno]], que también dice **qué no contesta**:
      arrancar no es trabajar.
- [x] **Ejecutar un proceso arbitrario** — **hecho el 2026-08-25** con
      `ejecutar <ruta>`. Ver la entrada `G1` arriba y [[Programas-Ajenos]].
- [ ] **Opción, no pendiente: supervisar `sched_setattr` con
      `SECCOMP_RET_USER_NOTIF`.** Cesar decidió el 2026-08-25 denegarla, porque
      su política vive detrás de un puntero y un filtro de seccomp no puede
      seguirlo — ver [[Sandbox-Ejecucion]]. Un proceso supervisor sí podría
      leerla mientras la llamada está detenida, y sería un componente nuevo vivo
      durante toda la ejecución del módulo. Sólo vale la pena el día que un
      runtime que haga falta dependa de esa puerta; ninguno de los medidos lo
      hace.
- [ ] **Exponer las cuatro ventajas que ningún otro sistema tiene.** Ninguna
      está al alcance de un agente ajeno todavía, y son la respuesta a «mejor» y
      no sólo a «igual»: el índice semántico ([[FS-en-Grafo]]), el rollback
      ([[Journal-y-Snapshots]]), la procedencia por campo ([[Marcado-de-Origen]])
      y los permisos por tarea ([[Permisos-JIT]]).
- [ ] **Decidir por dónde entra un agente ajeno.** ¿Es un módulo con permisos
      amplios, un proceso aparte, o algo nuevo? No decidido, y no urge hasta que
      haya qué ejecutar.

## Pendientes de decreto formal

- [x] **Con qué se cierra la Fase 1** — resuelto el 2026-08-06: **una ISO independiente**, que puesta en una PC sin sistema operativo la deje corriendo Thalyx. Sustituye a la persona ajena y conserva lo que ella aportaba: es una condición que el proyecto no se puede declarar a sí mismo. Ver [[Criterio-de-Salida-Fase-1]].
- [x] **Si el arranque UEFI sin gestor de arranque no funciona, decidir qué se hace** — **no hubo nada que decidir: funcionó.** Un firmware arrancó el medio el 2026-08-06 y un disco escrito por Thalyx el 2026-08-07, las dos veces sin gestor de arranque, con `CONFIG_EFI_STUB` haciendo del kernel una aplicación UEFI y el medio llevando **un archivo**. Ni GRUB ni systemd-boot hicieron falta, así que [[Filosofia-Fundacional]] no tuvo que ceder nada. Cerrado el 2026-08-07 al notar que seguía marcado como abierto: **un pendiente que la realidad ya contestó se lee igual que uno vivo**, que es el reverso exacto del `[x]` de *decidido* que se veía igual que uno de *construido*. Ver [[Construccion-del-ISO]].
- [x] **Cómo encuentra su store una máquina instalada** — resuelto el 2026-08-06: **por la etiqueta del sistema de archivos**, que `mkfs.btrfs -L thalyx-store` ya escribe. Buscar un nombre que Thalyx mismo escribió no es la heurística que `store_disk.rs` prohíbe; sin etiqueta se dice que no hay store, y con dos iguales se niega en vez de elegir. Ver [[Construccion-del-ISO]].
- [x] **Quién crea el store en una máquina que no tiene uno** — resuelto el 2026-08-07 construyéndolo, y **el decreto se conservó entero**: `crates/thalyx-btrfs` escribe el Btrfs, lo invoca un humano con `thalyx disk format`, y nada de ese crate es alcanzable desde PID 1, que sigue montando y sin fabricar. La confirmación pide teclear la ruta del dispositivo en vez de una `y`. Ver [[Construccion-del-ISO]].
- [x] **Los tres subvolúmenes, desde dentro de la imagen** — construido el 2026-08-07 por `BTRFS_IOC_SUBVOL_CREATE`, porque no hay binario `btrfs` adentro. Lo que sale de `thalyx disk format` ahora es un store, y se comprueba montando cada uno con `-o subvol=<nombre>` igual que PID 1, no mirando si apareció un directorio. Correrlo dos veces es seguro, para que reparar un store a medias no cueste el disco. Ver [[Construccion-del-ISO]].
- [x] **El instalador** — construido el 2026-08-07. `crates/thalyx-install` y `thalyx install <disco> --kernel <archivo>`: GPT con las dos copias, una ESP de 512 MiB en FAT32 con `\EFI\BOOT\BOOTX64.EFI` adentro, y el resto como store con sus tres subvolúmenes. Costó dos escritores de bytes más —`sgdisk` y `mkfs.vfat` tampoco pueden ir en la imagen— y una cuarta llamada al kernel, `BLKRRPART`. **La etapa 20 de `verify.sh` lo ejerce; que un firmware arranque el disco es `make -C image run-installed` y no lo ha corrido nadie.** Ver [[Construccion-del-ISO]].
- [x] **Los controladores de una PC de verdad** — pedidos el 2026-08-07 y **sin ejercer**. `thalyx.config` ahora pide el framebuffer que el firmware ya dejó configurado (`FB_EFI` y la consola encima), teclado por USB y por PS/2, y NVMe con AHCI. Y la consola dejó de ser sólo el puerto serie: la línea compilada dice `console=ttyS0 console=tty0`, y la última es la que se vuelve `/dev/console`. **Ninguna de esas opciones se ha compilado siquiera**, y hay una regla que dice que ninguna comprobación de construcción encuentra la siguiente que falta — van tres encontradas arrancando. Ver [[Construccion-del-ISO]].
- [ ] **Confirmar las gamas con el banco** — **tres de cuatro medidas el 2026-08-08** sobre la misma máquina (Ryzen 5 5600G, 16 GB, sin GPU, CPU): ligera 5/14 intención y 2.82 GB, media 9/19 y 4.79 GB, alta 7/19 y 13.93 GB, con abstención **0** en las tres. Los tres estimados de disco acertaron; el de RAM de la media iba alto por casi el doble. **Falta la máxima**, que no cabe en esa máquina. La ligera ya se midió **dos veces** y las dos corridas no coincidieron: dos casos de veinte se movieron, así que la fila de arriba es su primera corrida y la segunda dio 6/15. Ninguna fracción es todavía la puntuación de su gama —hubo casos sin medición en las tres— y ninguna es exacta. Ver [[Gamas-de-Modelo]].
- [x] **Averiguar por qué se abstuvo con el id dicho en claro** — **contestado el 2026-08-08, y la premisa era falsa: nunca se abstuvo.** `dev.thalyx.demo, ese` sale `REF` en las tres gamas medidas, o sea que el modelo nombró un id que no aparece en ningún canal y la atribución lo rechazó. Aquel `MISS` venía del banco que clasificaba `Err(_) => Abstained`, donde un rechazo por atribución se contaba como abstención correcta. Con eso queda **retirada del todo** la hipótesis de que la instrucción de abstención del prompt pesa de más, y el prompt queda absuelto de este cargo. Los casos 10 y 11 dicen lo que sí arregla ese enunciado: un verbo, o que la máquina liste el módulo. Ver [[Gamas-de-Modelo]].
- [ ] **Decidir qué hacer con un módulo mencionado y luego descartado** — **medido en tres gamas el 2026-08-08 y ninguna lo maneja.** Las tres formas de negación de la suite (casos 9, 16 y 17) fallan en ligera, media y alta, y el caso 18 —una pregunta sobre un módulo, no una petición— también. Cuatro maneras distintas de decir «esto no es una orden de instalar» y ninguna gama distingue ninguna. **Ya no se puede pedir a la gama alta**: la alta también falla, así que subir de tamaño no lo resuelve. La negación es comprensión, no gramática, y no se arregla restringiendo la salida. Falta decidir si es del prompt, del ensamblado del transcript, o de la familia. Ver [[Gamas-de-Modelo]].
- [ ] **Abstención cero en las tres gamas medidas: qué hacer, y en qué orden.** Es la medida que [[Gamas-de-Modelo]] llama la más importante y la única que sale **idéntica** en 1.5B, 3B y 7B: 0/6, 0/9 y 0/8. Un resultado plano donde lo único que varía es el tamaño apunta a lo que las tres comparten —prompt, gramática, forma de los casos— y no a lo que las separa. **Hipótesis, no conclusión, y deliberadamente sin actuar**: tocar el prompt mueve los veinte casos a la vez, y no hay un antes/después con el que comparar. Lo que corresponde antes de cambiar nada es una segunda corrida de la misma gama con el mismo prompt, para saber cuánto se mueve una cifra de acierto por sí sola. **Avance del 2026-08-09: la gramática quedó descartada como causa** —la gama media inventa igual sin ella, con el control sostenido—, así que de las tres cosas que comparten quedan el prompt y la familia, y siguen sin poder separarse. Ver [[Gamas-de-Modelo]].
- [x] **Saber por qué seis casos de la gama ligera no produjeron respuesta** — **contestado el 2026-08-08**, con una corrida sin cambiar nada. Los `ERR` tienen **una sola causa, la misma en todos**: el modelo empieza el objeto que la gramática describe y agota los 256 tokens dentro de un identificador, porque la gramática no acota cuán largo puede ser. Se cicla —`python3.ipython3.ipython3.…`, visible también en el brazo restringido del sondeo—. No es plazo agotado, ni `llama.cpp` cayéndose, ni gramática sin aplicar. Subir `-n` no lo arregla: el propio error lo advierte, una cuota mayor alarga el ciclo. Y es la **misma** patología que las invenciones repetitivas (`ese.abc.abc.abc`, `thallyx.ing.ing`): un fallo contado como dos. Ver [[Gamas-de-Modelo]].

- [x] **Decidir si la corrida debe ser reproducible, sabiendo lo que cuesta** — **decidido y construido el 2026-08-08: se guarda el prompt bajo una bandera.** Existe `--keep-prompt <dir>` en `agent model check`, `agent model grammar-check` y `agent bench`. Cada inferencia deja un directorio con `prompt.txt`, `proposal.gbnf` y `command`, nombrado por el marcador de esa corrida, así que un banco de veinte casos deja veinte y no se pisan. El marcador **sigue siendo aleatorio**: volverlo derivable lo volvería adivinable, que es contra lo que existe ([[Marcado-de-Origen]]). Lo que esto recupera es reproducir *esa* corrida —los bytes que corrieron, marcador incluido—, no volver iguales dos corridas distintas. Eso último es correcto que se mueva: esconderlo daría una muestra de una distribución con cara de medición. Sin bandera no queda nada en disco, como antes.

- [ ] **Medir cada gama dos veces antes de comparar gamas** — **hecho en ligera (×3) y media (×2) el 2026-08-08; falta la alta.** Lo que se aprendió: la cifra comparable es **aciertos sobre los 20 casos**, no la fracción sobre lo medido, porque el denominador se mueve por una razón ajena al numerador. Ligera 5, 6, 6; media 9, 9; alta 7 con una sola corrida. Catorce de veinte casos dieron la misma marca en las cinco corridas. La **alta queda aplazada** por decisión de Cesar —tarda demasiado en esta máquina— y se medirá cuando consiga el equipo que también sostenga la máxima. Hasta entonces la distancia media-alta no se afirma, pero ya no es por imprecisión del instrumento. Ver [[Gamas-de-Modelo]].

- [x] **Averiguar en qué producción se cicla el caso 4** — **resuelto el 2026-08-08 corriendo la inferencia guardada.** Es `module-id`, no `range`: la salida es `["dev.thalyx.demo.versions.versions.versions…` y **nunca llegó a `constraint`**, con 255 de 256 tokens gastados. La hipótesis del punto en `1.4` queda refutada. Y explica de más: es el mismo comportamiento que `ese.abc.abc.abc`, `thallyx.ing.ing`, `dev.thalyx.demo.localhost` y `python3.ipython3.ipython3` — cuando el 1.5B no sabe cerrar semánticamente un id, sigue produciendo segmentos válidos; si el corte llega antes de cerrar la cadena sale `ERR`, si llega después sale una invención. Un solo comportamiento contado como tres. Ver [[Gamas-de-Modelo]].

- [ ] **Decidir si `module-id` lleva cota superior, y dónde.** Al inspeccionarlo salió que **`thalyx-manifest` tampoco tiene cota**: un id de cuarenta segmentos es válido para Thalyx hoy, y la gramática espeja fielmente a la autoridad. Acotar sólo la gramática la volvería más estricta que el manifiesto. Tres opciones en [[Gamas-de-Modelo]] —acotar los dos, acotar sólo la gramática, o no acotar—. **La predicción es que acotar no sube el acierto**: convertiría `ERR` en `REF`, y eso ya se observó cuando tres casos dejaron de ser `ERR` solos y volvieron `INV`. Lo que compraría es cobertura de medición y tokens; lo que costaría, según la regla 9, es cambiar una señal visible por una invención bien formada. **No se toca sin antes/después**: las seis corridas son la línea base. Y `-n` no sube. Ver [[Gamas-de-Modelo]].

- [x] **Correr `thalyx agent grammar-effect` en media y ligera** — **corrido el 2026-08-09, y la hipótesis quedó refutada.** La gama media dio `IT INVENTS EITHER WAY` con el control sostenido (sin gramática encontró el módulo correcto en 9 de 11 casos donde lo había), y en los nueve casos de rechazo, **sin ninguna gramática**, inventó cuatro veces y nombró el módulo real equivocado cinco. Quitarle la gramática no la hizo abstenerse. La gama ligera salió `NOT PROVEN` porque sin gramática contestó 17 de 20 casos con cero tokens —control 0 de 11—, que es ninguna evidencia y no inocencia; lo que sí enseña es que en esa gama la gramática es lo único que hace que el modelo genere algo. Ver [[Gamas-de-Modelo]].
- [x] **Probar que la misma entrada exacta da la misma salida exacta** — **hecho el 2026-08-08: cinco corridas del `command` guardado del caso 4, cinco hashes idénticos.** Mismo prompt, mismo marcador, `--seed 1 --temp 0` → misma salida siempre. Cierra la explicación del marcador aleatorio: era la única variable, y mueve ±1 caso sobre veinte. No prueba que llama.cpp sea determinista en general —cambiar hilos o lote altera el orden de reducción en punto flotante— sino que este comando, en esta máquina, repetido, da lo mismo. Que es lo que hacía falta.

- [x] **Confirmar la primera abstención observada** — **refutado el 2026-08-09, con la inferencia reproducida.** No era una abstención: la salida completa es `"targets": ["good-luck-module-1234567890123456789…`, 255 tokens de dígitos dentro de un identificador que nunca cerró. Era la segunda lectura —truncamiento— y `Named::Nothing` fue correcto. Octavo caso de la patología del ciclo en `module-id`. **La abstención sigue en cero, ahora sobre 55 oportunidades.** Costó una hipótesis y no costó una afirmación falsa en la bóveda, porque se había escrito como pendiente con su comando al lado en vez de como hallazgo. Ver [[Gamas-de-Modelo]].
- [ ] **Volver a correr el tercer brazo con el prompt corregido.** La primera corrida (2026-08-09) salió `NOT PROVEN` **por un defecto del prompt, no del modelo**: la palabra `NOTHING` aparecía cuatro veces, tres de ellas en otro sentido, y las veinte respuestas del 3B empezaron con ella —incluidas las once donde sí había un módulo—. El control lo tumbó, que es exactamente para lo que existe: sin él, `ABSTAINED 8/9` se habría leído como `THE PROMPT TAKES THE DECISION` con un resultado fabricado por su propio prompt. Corregido con una prueba que cuenta apariciones. **Corregirlo puede destruir el 8 de 9, que era el número favorable.** La gama ligera ya no aporta: tercera corrida contestando las veinte con cero tokens sin gramática. Comando en [[Punto-Actual]].
- [ ] **Decidir dónde termina una respuesta en prosa.** El brazo en prosa contesta y después divaga hasta agotar los 256 tokens. El lector busca identificadores en **todo** el texto, así que `NOTHING Identities: dev.thalyx.demo: 1.4.2 dev.thalyx.demo: 1.4.1 …` —el material devuelto de vuelta— cuenta como haber nombrado el módulo, y eso **infla el control** del brazo en prosa: de los 5 de 11 que sostuvo, varios son eco y no respuesta. Tres opciones: leer sólo la primera línea, leer hasta el primer salto de línea doble, o dejarlo como está y aceptar que el control es optimista. **Hay veinte muestras reales capturadas** de la corrida del 2026-08-09, que es lo que la regla 6 pide y lo que la primera versión de este lector no tenía. Cambia lo que el instrumento mide, así que va con aprobación y con antes/después contra esa corrida. Ver [[Gamas-de-Modelo]].
- [ ] **Decidir si el router contesta solo cuando no hay ningún módulo nombrado.** Cuatro de los nueve casos de abstención —6, 7, 8 y 19— no nombran **ningún** id de módulo en toda la conversación: `instala algo bueno`, `necesito algo para editar video`. La respuesta correcta es calculable sin modelo, de forma determinista. El router ya resuelve solo cuando el id está dicho sin ambigüedad, pero al revés no hace nada: con cero candidatos devuelve `AskTheModel`. **Le estamos pidiendo a un componente declarado no confiable una pregunta que podemos contestar con certeza.** Cambia el enrutado y por lo tanto la línea base, así que va con antes/después. Los otros cinco casos —negaciones, una pregunta, un demostrativo— sí son comprensión y sí son del modelo. Ver [[Gamas-de-Modelo]].

- [ ] **Decidir si el banco imprime también los aciertos sobre 20.** Hoy imprime la fracción sobre lo medido, que es lo honesto para una corrida y lo engañoso para dos: una gama contestó cuatro casos más, acertó uno más y su fracción bajó. Las dos cifras dicen cosas distintas y las dos hacen falta. **Propuesta, no tarea** —es un cambio al instrumento y le toca a Cesar—. Ver [[Estrategia-de-Pruebas]].
- [x] **Decidir si la columna de RAM recomendada baja** — **decidido el 2026-08-08: no.** El RSS medido quedó por debajo del estimado en dos gamas (media 4.79 contra 8 GB, ligera 2.82 contra 4) y cerca en la tercera, y aun así la columna se queda. Palabras de Cesar: *«declara los resultados mas no los muestres como pruebas definitivas, las pruebas definitivas vendran cuando thalyx este corriendo en una ssd real como sistema operativo real, solo en ese entorno se vera la realidad»*. La columna es una afirmación sobre el destino y la medición es del anfitrión de desarrollo. Ver [[Gamas-de-Modelo]].
- [ ] **La medición definitiva de las gamas: Thalyx como OS, sobre un SSD real.** Es la condición que Cesar puso el 2026-08-08 para que cualquiera de estas cifras deje de estar sólo declarada. La misma suite, las mismas gamas, corriendo sobre Thalyx en vez de sobre Fedora. Lo que se compara entre gamas sobrevive al anfitrión —las tres corrieron igual— pero **todo lo que afirme algo sobre el hardware que Thalyx pide** espera a esa corrida: la columna de RAM, qué gama trae el ISO por omisión, y si la máxima es alcanzable. Ver [[Gamas-de-Modelo]] y [[Construccion-del-ISO]].
- [ ] **Medir la gama máxima, o decidir que se queda en `N/D`.** Qwen2.5-14B Q4_K_M no completó ni la primera inferencia en la máquina de 16 GB: el sistema mató el proceso. **Eso no es un cero de utilidad y no dice que 14B pida 32 GB** — dice que esta máquina con esta configuración no la sostiene. Cerrarlo necesita otra máquina, o medirla bajo Thalyx como sistema operativo, que es donde la huella es distinta. Mientras tanto la cuarta fila del decreto sigue siendo estimada y marcada como tal.
- [ ] **Un sondeo de gramática que un modelo chico pueda contestar.** El 1.5B contesta el sondeo actual terminando la generación de inmediato, así que el brazo de control queda vacío y el resultado correcto es `NOT PROVEN`. Eso ya no miente —se corrigió el 2026-08-08— pero deja la gama ligera sin poder demostrar lo que las otras dos demostraron. Un sondeo distinto sería otro instrumento y **no se cambia sin decidirlo**: el actual es el que produjo la evidencia de media y alta, y cambiarlo hace incomparables las corridas. Propuesta, no tarea. Ver [[Estrategia-de-Pruebas]].
- [ ] **Métricas de benchmark concretas** para la Fase 2 — qué se mide exactamente para el índice semántico y los permisos JIT, con qué carga y contra qué línea base. El umbral de decisión ya está decretado, lo que falta es el instrumento. Ver [[Decision-Kernel-vs-Userspace]].
- [ ] **Técnicas de interpretabilidad** aplicables al agente. Ver [[Interpretabilidad-Mecanicista]].
- [ ] **Arquitectura del índice semántico a mayor escala** — SQLite alcanza para Fase 1; falta saber a partir de qué volumen deja de alcanzar.
- [ ] **Sistema de reputación resistente a Sybil** — pospuesto deliberadamente. Ver [[Sistema-Reputacion-Sybil]].
- [ ] **Dependencias entre módulos y resolver con backtracking** — pospuesto hasta que exista un módulo real que las necesite. Ver [[Resolucion-de-Versiones]].
- [ ] **Decidir el ABI de los módulos: nativo de Linux o independiente de POSIX.** [[Filosofia-Fundacional]] dice que los módulos no hablan POSIX ni libc; hoy son binarios de Linux enlazados dinámicamente, con `/usr`, `/lib` y `/etc` montados de sólo lectura y unas ciento veinte llamadas al sistema permitidas. La distinción que sí se sostiene está escrita en [[Sistema-de-Modulos]] — la API es la única superficie *mediada*. Hacer verdadera la frase entera significa módulos estáticos sin libc, un rootfs sin `/usr`, un filtro mucho más chico, o un objetivo distinto como WASM. **Es barato ahora, con un módulo, y caro con un ecosistema encima**, así que decidirlo antes de escribir más módulos.
- [ ] **Condiciones para habilitar llamadas a modelos remotos** — las reglas ya están escritas; falta decidir cuándo se activan. Ver [[Agente-Conversacional]].

## Resueltos el 2026-08-01

- [x] **Nombre del sistema y nomenclatura** — Thalyx. Ver [[Nomenclatura-y-Convenciones]].
- [x] **Licencia** — GPLv3 en userspace, GPLv2 en kernel. Ver [[Decision-Licencia]].
- [x] **Modelo de amenaza y definición de la TCB** — ver [[Modelo-de-Amenaza]].
- [x] **Formato exacto del manifiesto `.thmod`** — ver [[Formato-Manifiesto-Thmod]].
- [x] **Mecanismo de resolución de versiones** — ver [[Resolucion-de-Versiones]].
- [x] **Mecanismo de sandboxing en detalle** — ver [[Sandbox-Ejecucion]].
- [x] **Diseño del ISO booteable** — ver [[Construccion-del-ISO]].
- [x] **Mecanismo real del commit atómico** — ver [[Fase-Commit-Atomico]].
- [x] **Defensa contra inyección de prompts** — ver [[Marcado-de-Origen]].
- [x] **Camino confiable para la confirmación humana** — ver [[Camino-Confiable]].
- [x] **Coherencia entre doble ruta y estado del sistema** — ver [[Coherencia-Doble-Ruta]].
- [x] **Semántica de rollback frente a restore** — ver [[Rollback-vs-Restore]].
- [x] **Modelo de concurrencia** — ver [[Concurrencia]].
- [x] **Criterio de salida de la Fase 1** — ver [[Criterio-de-Salida-Fase-1]].
- [x] **Estrategia de pruebas** — ver [[Estrategia-de-Pruebas]].
- [x] **Registro de intención en el journal** — implementado y probado. Ver [[Fase-Commit-Atomico]].
- [x] **Contrato estructurado y marcado de origen** — implementados y probados de punta a punta.
- [x] **`thalyx-permd`** — traducción de permisos a política de kernel, implementada y probada.
- [x] **Índice en grafo y parser mecánico** — implementados. Ver [[Estado-de-Implementacion]].
- [x] **Ubicación de los permisos JIT (kernel vs userspace)** — ver [[Permisos-JIT]].
- [x] **Modo de actualización del índice en grafo** — ver [[Parser-Mecanico]] y [[Coherencia-Doble-Ruta]].
- [x] **FUSE dentro o fuera de Fase 1** — fuera. Ver [[Decision-Kernel-vs-Userspace]].
- [x] **Zona gris del umbral de migración** — ver [[Decision-Kernel-vs-Userspace]].
- [x] **Filesystem requerido** — Btrfs. Ver [[Journal-y-Snapshots]].
- [x] **Alcance de la Fase 1** — ver [[Fases-de-Implementacion]].

## Resueltos el 2026-08-03

- [x] **Modelo concreto del agente** — el decreto que bloqueaba. No es un modelo: son cuatro gamas de una sola familia que elige el usuario según su hardware, con `llama.cpp` como proceso y gramática restringida. Ver [[Gamas-de-Modelo]] y [[Agente-Minimo]].
- [x] **Quién escribe la procedencia en el contrato** — el ensamblador, desde el canal de entrada; nunca el modelo. Ver [[Agente-Conversacional]].
- [x] **Límites de recursos contra un kernel que delegue controladores** — la corrida en Fedora 43 tenía `memory` y `pids` delegados, así que `verify.sh` activó `THALYX_REQUIRE_CONTROLLER_TESTS=1` y los saltos habrían sido fallos. Con `not proven 0`, se ejercitaron.
- [x] **Snapshots y `restore`** — la operación destructiva, con diff de lo que se pierde, confirmación por el camino confiable e intercambio atómico. Ver [[Rollback-vs-Restore]].
- [x] **Acotar la cuenta de mutaciones al árbol** — atribución subiendo por los ancestros del dentry, con la ausencia de montajes debajo como precondición comprobada. Verificado en hardware: 5000 escrituras dentro contadas, las mismas 5000 fuera ignoradas.
- [x] **La puerta del atajo del índice** — `thalyx graph trust`, que corre la verificación en el momento y se niega si no coincide.
- [x] **Escrituras por descriptor abierto en el watcher** — `lsm/file_permission` enmascarado a `MAY_WRITE`, más los siete hooks de forma del árbol que faltaban. El contador ya puede creerse en cuanto a cobertura. Ver [[FS-en-Grafo]].
- [x] **`thalyx rollback`** — deshace un commit de Thalyx, y se niega cuando la entrada del journal ya no describe el disco. Ver [[Rollback-vs-Restore]].

## Resueltos el 2026-08-02

- [x] **Ejecutar `lsm/` por primera vez** — se compiló, se cargó y se demostró denegando una conexión real dentro del cgroup mientras la misma conexión seguía funcionando fuera. Ver [[Permisos-JIT]].
- [x] **Montajes idmapped para las rutas concedidas** — implementados y verificados. Ver [[Sandbox-Ejecucion]].
- [x] **Con qué uid corre un módulo** — uno por módulo, sin reutilizar nunca. Decretado **e implementado**. Ver [[Sandbox-Ejecucion]].
- [x] **Sockets `AF_UNIX` en el sandbox** — se quedan fuera, y queda dicho que la decisión es reversible. Ver [[Sandbox-Ejecucion]].
- [x] **Memoria persistente** — tercera primitiva, construida y probada. Ver [[Memoria-Persistente]].
- [x] **Raíz propia del módulo (`pivot_root`)** — el módulo ya no ve el árbol del host. Ver [[Sandbox-Ejecucion]].
- [x] **Perfil `module_standard`** — namespaces, seccomp y límites, verificados contra el kernel real. Ver [[Sandbox-Ejecucion]].
- [x] **Dónde vive el `unsafe`** — en `thalyx-syscall` y en ningún otro lado. Ver [[Sandbox-Ejecucion]].
- [x] **Cierre del ciclo de enforcement** — `thalyx module run` establece la contención sola. Ver [[Sandbox-Ejecucion]].
- [x] **Identidad cgroup del módulo y orden de lanzamiento** — probados contra un montaje cgroup2 real.
- [x] **Dónde vive el manifiesto de un módulo instalado** — junto al módulo, publicado por el mismo `rename`, re-verificado en cada lectura.

## Resueltos antes (referencia histórica)

- [x] Re-trazar el caso de "instalar módulo" con build-then-commit — ver [[Caso-Instalar-Modulo]].
- [x] Trazar un caso de fallo/rollback explícito — ver [[Caso-Fallo-Rollback]].
- [x] Decidir si "resolver módulo" es contrato separado o sub-tarea sin contrato — sub-tarea sin contrato. Ver [[Resolver-vs-Instalar]].

## Lo que sigue sin validarse

**El repositorio fue auditado desde fuera por primera vez el 2026-08-04**, y esa auditoría encontró nueve defectos reales — tres críticos — que ninguna de las 612 pruebas de entonces veía. Es la evidencia más directa que hay de que las pruebas escritas junto al código comparten sus supuestos, y de que la próxima victoria no es duplicar el tamaño sino que alguien hostil no pueda romper lo que ya se promete. Ver [[Punto-Actual]] y [[Estrategia-de-Pruebas]].



**Ningún decreto de esta bóveda ha sido contrastado con una persona ajena al proyecto.** Todo el razonamiento sobre por qué alguien elegiría Thalyx sigue siendo a priori. Ver [[Por-Que-Elegirian-Este-SO]] y [[Riesgo-de-Ejecucion]].

El [[Criterio-de-Salida-Fase-1|criterio de salida de la Fase 1]] estaba diseñado para forzar ese contacto, y **el 2026-08-06 Cesar lo suspendió**: Thalyx todavía son comandos de terminal y el producto terminado será una ISO booteable, así que se prueba cuando haya algo que probar. El riesgo se sigue cargando a propósito, ahora por más tiempo y con una medición más: **no se pudo convencer a nadie de dedicarle media hora de terminal**, que es una respuesta parcial a [[Por-Que-Elegirian-Este-SO]] y no un contratiempo de calendario.

## Relacionado
- [[00-Indice/Indice-Principal|Índice principal]]
- [[Notas-Tecnicas-Implementacion]]
