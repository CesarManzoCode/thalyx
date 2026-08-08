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
- [ ] **Un caso de aislamiento con un permiso sobre un archivo y usuario propio.** El 2026-08-04 un punto de montaje creado como directorio sobre un archivo rompió el `correr` de la máquina, y ninguna prueba lo vio porque **todos los permisos de todas las pruebas son directorios**. Lo cubre ahora una prueba unitaria de `create_target_like` y la etapa 16 en hardware; falta un caso en `isolation.rs` que arme la raíz remapeada de verdad sobre un archivo. Ver [[Estrategia-de-Pruebas]].
- [ ] **Cargar `thalyx_watch` con el cargador propio.** Es lo único que queda de la lista de "lo que falta comprobar" de [[Punto-Actual]]. Diez hooks en lugar de dos, y el único tipo de mapa que el watcher usa y el LSM no es `PERCPU_ARRAY`. Probable no es comprobado, y no se puede intentar en el contenedor: faltan las cabeceras de `libbpf` para compilar el objeto.
- [ ] **Probar `net/outbound` de punta a punta en hardware.** Que el LSM deniegue a un módulo sin la concesión está demostrado y es reproducible; que un módulo **con** la concesión abra una conexión está implementado, cubierto por pruebas unitarias y nunca ejercido en una máquina. Ver [[Permisos-JIT]].
- [ ] **Consumir el ringbuf `thalyx_mutations`** para saber *qué* cambió, no solo que algo cambió. El atajo ya no lo necesita — lo resolvió la atribución por ancestros — así que esto solo hace falta para reindexar de forma incremental en vez de reconstruir. Ver [[FS-en-Grafo]].

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


## Pendientes de decreto formal

- [x] **Con qué se cierra la Fase 1** — resuelto el 2026-08-06: **una ISO independiente**, que puesta en una PC sin sistema operativo la deje corriendo Thalyx. Sustituye a la persona ajena y conserva lo que ella aportaba: es una condición que el proyecto no se puede declarar a sí mismo. Ver [[Criterio-de-Salida-Fase-1]].
- [x] **Si el arranque UEFI sin gestor de arranque no funciona, decidir qué se hace** — **no hubo nada que decidir: funcionó.** Un firmware arrancó el medio el 2026-08-06 y un disco escrito por Thalyx el 2026-08-07, las dos veces sin gestor de arranque, con `CONFIG_EFI_STUB` haciendo del kernel una aplicación UEFI y el medio llevando **un archivo**. Ni GRUB ni systemd-boot hicieron falta, así que [[Filosofia-Fundacional]] no tuvo que ceder nada. Cerrado el 2026-08-07 al notar que seguía marcado como abierto: **un pendiente que la realidad ya contestó se lee igual que uno vivo**, que es el reverso exacto del `[x]` de *decidido* que se veía igual que uno de *construido*. Ver [[Construccion-del-ISO]].
- [x] **Cómo encuentra su store una máquina instalada** — resuelto el 2026-08-06: **por la etiqueta del sistema de archivos**, que `mkfs.btrfs -L thalyx-store` ya escribe. Buscar un nombre que Thalyx mismo escribió no es la heurística que `store_disk.rs` prohíbe; sin etiqueta se dice que no hay store, y con dos iguales se niega en vez de elegir. Ver [[Construccion-del-ISO]].
- [x] **Quién crea el store en una máquina que no tiene uno** — resuelto el 2026-08-07 construyéndolo, y **el decreto se conservó entero**: `crates/thalyx-btrfs` escribe el Btrfs, lo invoca un humano con `thalyx disk format`, y nada de ese crate es alcanzable desde PID 1, que sigue montando y sin fabricar. La confirmación pide teclear la ruta del dispositivo en vez de una `y`. Ver [[Construccion-del-ISO]].
- [x] **Los tres subvolúmenes, desde dentro de la imagen** — construido el 2026-08-07 por `BTRFS_IOC_SUBVOL_CREATE`, porque no hay binario `btrfs` adentro. Lo que sale de `thalyx disk format` ahora es un store, y se comprueba montando cada uno con `-o subvol=<nombre>` igual que PID 1, no mirando si apareció un directorio. Correrlo dos veces es seguro, para que reparar un store a medias no cueste el disco. Ver [[Construccion-del-ISO]].
- [x] **El instalador** — construido el 2026-08-07. `crates/thalyx-install` y `thalyx install <disco> --kernel <archivo>`: GPT con las dos copias, una ESP de 512 MiB en FAT32 con `\EFI\BOOT\BOOTX64.EFI` adentro, y el resto como store con sus tres subvolúmenes. Costó dos escritores de bytes más —`sgdisk` y `mkfs.vfat` tampoco pueden ir en la imagen— y una cuarta llamada al kernel, `BLKRRPART`. **La etapa 20 de `verify.sh` lo ejerce; que un firmware arranque el disco es `make -C image run-installed` y no lo ha corrido nadie.** Ver [[Construccion-del-ISO]].
- [x] **Los controladores de una PC de verdad** — pedidos el 2026-08-07 y **sin ejercer**. `thalyx.config` ahora pide el framebuffer que el firmware ya dejó configurado (`FB_EFI` y la consola encima), teclado por USB y por PS/2, y NVMe con AHCI. Y la consola dejó de ser sólo el puerto serie: la línea compilada dice `console=ttyS0 console=tty0`, y la última es la que se vuelve `/dev/console`. **Ninguna de esas opciones se ha compilado siquiera**, y hay una regla que dice que ninguna comprobación de construcción encuentra la siguiente que falta — van tres encontradas arrancando. Ver [[Construccion-del-ISO]].
- [ ] **Confirmar las gamas con el banco** — **tres de cuatro medidas el 2026-08-08** sobre la misma máquina (Ryzen 5 5600G, 16 GB, sin GPU, CPU): ligera 5/14 intención y 2.82 GB, media 9/19 y 4.79 GB, alta 7/19 y 13.93 GB, con abstención **0** en las tres. Los tres estimados de disco acertaron; el de RAM de la media iba alto por casi el doble. **Falta la máxima**, que no cabe en esa máquina, y falta que cualquier gama se mida **dos veces** — el coste ya tiene réplica, el acierto no. Y ninguna fracción es todavía la puntuación de su gama: hubo casos sin medición en las tres. Ver [[Gamas-de-Modelo]].
- [x] **Averiguar por qué se abstuvo con el id dicho en claro** — **contestado el 2026-08-08, y la premisa era falsa: nunca se abstuvo.** `dev.thalyx.demo, ese` sale `REF` en las tres gamas medidas, o sea que el modelo nombró un id que no aparece en ningún canal y la atribución lo rechazó. Aquel `MISS` venía del banco que clasificaba `Err(_) => Abstained`, donde un rechazo por atribución se contaba como abstención correcta. Con eso queda **retirada del todo** la hipótesis de que la instrucción de abstención del prompt pesa de más, y el prompt queda absuelto de este cargo. Los casos 10 y 11 dicen lo que sí arregla ese enunciado: un verbo, o que la máquina liste el módulo. Ver [[Gamas-de-Modelo]].
- [ ] **Decidir qué hacer con un módulo mencionado y luego descartado** — **medido en tres gamas el 2026-08-08 y ninguna lo maneja.** Las tres formas de negación de la suite (casos 9, 16 y 17) fallan en ligera, media y alta, y el caso 18 —una pregunta sobre un módulo, no una petición— también. Cuatro maneras distintas de decir «esto no es una orden de instalar» y ninguna gama distingue ninguna. **Ya no se puede pedir a la gama alta**: la alta también falla, así que subir de tamaño no lo resuelve. La negación es comprensión, no gramática, y no se arregla restringiendo la salida. Falta decidir si es del prompt, del ensamblado del transcript, o de la familia. Ver [[Gamas-de-Modelo]].
- [ ] **Abstención cero en las tres gamas medidas: qué hacer, y en qué orden.** Es la medida que [[Gamas-de-Modelo]] llama la más importante y la única que sale **idéntica** en 1.5B, 3B y 7B: 0/6, 0/9 y 0/8. Un resultado plano donde lo único que varía es el tamaño apunta a lo que las tres comparten —prompt, gramática, forma de los casos— y no a lo que las separa. **Hipótesis, no conclusión, y deliberadamente sin actuar**: tocar el prompt mueve los veinte casos a la vez, y no hay un antes/después con el que comparar. Lo que corresponde antes de cambiar nada es una segunda corrida de la misma gama con el mismo prompt, para saber cuánto se mueve una cifra de acierto por sí sola. Ver [[Gamas-de-Modelo]].
- [ ] **Saber por qué seis casos de la gama ligera no produjeron respuesta.** 6/20 en ligera, 1/20 en media y en alta. El banco **imprime la razón** de cada `NO MEASUREMENT` —plazo agotado, truncamiento, gramática no aplicada, `llama.cpp` cayéndose son fallos distintos— y esa columna no llegó a la bóveda en la transcripción de la corrida. Es lo más barato que queda por saber: una corrida, sin cambiar nada, guardando la salida entera. Hasta entonces, `5/14` no es la puntuación de esa gama y tampoco se sabe qué le pasa.
- [ ] **Decidir si la columna de RAM recomendada baja.** El RSS medido quedó por debajo del estimado en dos gamas (media 4.79 contra 8 GB, ligera 2.82 contra 4 GB) y cerca en la tercera (alta 13.93 contra 16 GB). **No se cambió**: la columna es una recomendación para el usuario, que necesita sitio para el resto del sistema, y el RSS se midió sobre Fedora, que es el anfitrión de desarrollo y no el destino. Decisión de Cesar. Ver [[Gamas-de-Modelo]].
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
