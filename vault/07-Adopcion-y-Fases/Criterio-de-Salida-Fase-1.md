---
tipo: estrategia
estado: decretado
fecha-decreto: 2026-08-01
tags: [fases, criterio, validacion, definicion-de-terminado]
---

# Criterio de salida de la Fase 1

## El hueco que resuelve

La Fase 2 tenía un criterio numérico y verificable (overhead <5% / >15%, con p99 en la zona gris). La Fase 1 solo decía "tener un prototipo funcional que pueda demostrar el flujo completo", que no es una definición de terminado: se puede cumplir siempre y nunca.

## Decreto

La Fase 1 se considera terminada cuando **una persona ajena al proyecto**, siguiendo únicamente el README y **sin intervención tuya**, logre:

1. Arrancar la imagen del sistema en QEMU con un solo comando.
2. Instalar un módulo firmado desde un repositorio local.
3. Revisar y confirmar sus permisos por el [[Camino-Confiable|camino confiable]].
4. Revertir la instalación.
5. Apagar la máquina.
6. Reiniciarla y comprobar que el agente conserva el contexto de la tarea.

**Ningún otro criterio lo sustituye.** Que todos los componentes estén implementados y con tests en verde no cierra la Fase 1.

> **Sustituido el 2026-08-06 por Cesar**, y sigue escrito porque el decreto que
> se sustituye se lee, no se borra. **El criterio vigente es una ISO
> independiente**, más abajo. Los seis pasos siguen siendo lo que el sistema
> tiene que hacer y se comprueban solos en cada cambio; lo que se cancela es que
> una persona ajena los ejecute ahora.

> **El paso 1 ya tiene máquina detrás.** El 2026-08-03 la imagen arrancó en QEMU
> con un comando. Eso **no cierra el paso**: el decreto exige que lo haga una
> persona ajena, siguiendo solo el README y sin ayuda. Lo que cambió es que
> hasta ese día no existía nada que esa persona pudiera arrancar. Ver
> [[Primer-Arranque]].
>
> **Los pasos 2, 3 y 4 se pueden hacer desde adentro desde el 2026-08-03.** La
> sesión entiende `disponibles`, `instalar <id>`, `permisos` y `revertir`, y el
> disco viaja con el módulo **en un repositorio y sin instalar** — porque una
> máquina que arranca con él puesto vuelve el paso 2 irrealizable y el 3 nunca
> se alcanza. La confirmación es el `TerminalConfirmer` del [[Camino-Confiable]],
> el mismo código que usa la CLI del host, no una copia.
>
> Igual que arriba: **eso no cierra los pasos.** Los cierra una persona ajena
> haciéndolos. Lo que cambió es que antes no había cómo, porque adentro de la
> máquina no hay shell y lo que no es un verbo de la sesión no existe.
>
> Comprobado manejando el prompt de verdad —etapa 15 de `verify.sh`, con un pty,
> porque el confirmador se niega sin terminal: el silencio no es consentimiento—
> y con el control que hace falta, que responder que no **no** instale.
>
> **Los seis pasos se pueden hacer desde el 2026-08-04.** Faltaban dos, y los dos
> eran del mismo tipo: existían las piezas y no había cómo alcanzarlas.
>
> **El paso 6** era el único que no tenía nada detrás. La sesión no escribía
> nada en la memoria persistente al instalar, así que reiniciar no perdía el
> contexto: no había contexto que perder. Ahora `instalar` y `revertir` escriben
> por el mismo `recollection.rs` que usa `thalyx agent do --task`, y `recuerdos`
> lo lee. Qué significa aquí *conservar el contexto* está resuelto abajo.
>
> **El paso 1** tenía máquina y no tenía camino. Ver "Lo que detiene a esa
> persona no es un problema difícil".

## La persona ajena se cancela por ahora — decidido por Cesar el 2026-08-06

**Esto cambia el decreto de arriba y lo decidió él**, después de que la
verificación en su máquina saliera entera en verde y de intentar entregarlo.

Sus palabras, que son el registro:

> La comprobación del usuario ajeno ejecutando los 6 pasos se cancela por
> completo por ahora, ni siquiera pude convencer al otro usuario de hacerlo.
> Thalyx está en una fase muy temprana, son solo comandos de terminal, no hay
> nada que probar. Cuando nuestro producto esté terminado será una ISO
> booteable, no comandos; se probará cuando haya algo que probar. De momento no
> hay nada que probar más que comandos que podemos hacer nosotros mismos.

### Qué se cancela y qué no

- **Se cancela**: que una persona ajena ejecute los seis pasos como condición
  para cerrar la Fase 1, y buscar a esa persona ahora.
- **No se cancela**: [[#Nadie de fuera toca el sistema antes de esto]]. Sigue
  entero, y por el mismo motivo — lo que esa persona determina es la escala, no
  la validez. Cancelar el paso lo *refuerza*: se retrasa el contacto, no se
  adelanta.
- **No se cancelan los seis pasos.** Siguen siendo lo que el sistema tiene que
  poder hacer, y siguen comprobados en cada cambio
  (`crates/thalyx-cli/tests/exit_criterion.rs`) y en cada corrida de hardware
  (etapa 16 de `verify.sh`, que los teclea desde un arranque frío). Lo que se
  cancela es **quién los teclea**, no que se tecleen.

### La objeción, escrita para que no se pierda

Esta nota abre diciendo que la Fase 1 *"solo decía tener un prototipo funcional
que pueda demostrar el flujo completo, que no es una definición de terminado:
se puede cumplir siempre y nunca"*. La persona ajena era lo que cerraba ese
hueco, porque era la única condición que el proyecto no podía declararse a sí
mismo.

**Quitarla deja el hueco abierto otra vez**, salvo que otra cosa ocupe ese
lugar. Y el argumento de Cesar señala cuál: *"cuando nuestro producto esté
terminado será una ISO booteable, no comandos"*. Eso es una condición
verificable y no la puede declarar nadie por decreto — o hay una ISO que
arranca en hardware, o no la hay.

> **Resuelto el mismo día.** Cesar eligió el sustituto: una ISO independiente.
> Ver la sección siguiente. El hueco estuvo abierto unas horas y queda escrito
> porque el razonamiento que lo cerró es el que importa.

## El criterio nuevo: una ISO independiente — decretado por Cesar el 2026-08-06

**Esto sustituye a la persona ajena, y cierra el hueco que su cancelación había
abierto.** Sus palabras:

> Cerrar será esto: una ISO totalmente independiente, es decir: que puedas
> ponerla en una PC sin sistema operativo y que ahora tenga Thalyx como OS.
> Obviamente lo haremos de alguna forma más fácil, por ejemplo una VM, pero el
> objetivo es que tengamos la ISO y nada más, y con ella sola podamos tener
> Thalyx corriendo.

### Por qué esto sí es una definición de terminado

Es la propiedad que esta nota pedía desde el principio y que la lista de
componentes nunca tuvo: **no la puede declarar nadie.** O existe un archivo que,
puesto en una máquina sin sistema operativo, la convierte en una máquina Thalyx,
o no existe. No hay forma de cumplirlo parcialmente ni de argumentar que ya casi.

Y es más exigente que lo que hay hoy, no menos. Hoy **QEMU es el gestor de
arranque**: `make run` le pasa `-kernel` y `-initrd`, que es el anfitrión
cargando el kernel en memoria porque nadie más lo haría. Nada de lo construido
hasta ahora sabe arrancar solo.

### Que se pruebe en una VM no lo debilita, si se dice qué prueba

Cesar acepta ejercerlo en una máquina virtual, y eso está bien **siempre que se
diga qué queda sin probar**, que es la regla 3:

- Una VM con **firmware UEFI de verdad** prueba lo que más importa: que la ISO
  arranca **sola**, encontrada por un firmware, sin `-kernel` ni `-initrd`. Eso
  se puede ejercer en `verify.sh` y es la mitad del criterio.
- Una VM **no prueba los controladores**. Sus discos son virtio y su teclado es
  emulado. Que arranque en una PC de verdad es lo único que responde eso.

Así que el criterio se cumple en dos actos, y el segundo necesita hierro. Lo que
**no** puede pasar es que el primero se reporte como si fuera el segundo.

### Lo que implica construirlo

Está en [[Construccion-del-ISO]], y en resumen: sin gestor de arranque —el kernel
con `CONFIG_EFI_STUB` **es** una aplicación UEFI, así que el medio lleva un solo
archivo y sigue siendo el decreto—, un store que hoy nadie crea porque PID 1
tiene prohibido fabricarlo, controladores reales, y una consola que hoy es un
puerto serie que las PC modernas no tienen.

### El acto 1 está hecho — 2026-08-07

**Corrido por Cesar, en una VM con OVMF.** `sudo ./dev/verify.sh` cerró en
`proven 135 · not proven 1 · failed 0` —el único no probado es llama.cpp, que es
Fase 2— y `make -C image run-installed` arrancó la máquina instalada.

Lo que ese arranque respondió, y hay que decirlo con nombres porque cada cosa es una
afirmación distinta:

- **Un firmware UEFI encontró `\EFI\BOOT\BOOTX64.EFI` en un disco que escribió
  Thalyx y lo ejecutó.** Sin `-kernel`, sin `-initrd`, sin `-append`, sin gestor de
  arranque. La tabla GPT, la FAT32 y el kernel dentro de ella los escribió Thalyx
  byte por byte.
- **Encontró su store sin que nadie se lo nombrara.** No hay `thalyx.store=` en la
  línea compilada; la máquina preguntó a cada disco cómo se llama.
- **La sesión salió por la pantalla.** `FB_EFI` adoptando el framebuffer del
  firmware, `FRAMEBUFFER_CONSOLE` y `FONT_8x16` — la ventana de QEMU mostró el
  prompt, que es lo que el orden `console=ttyS0 console=tty0` decidía.
- **Y el teclado llegó a la sesión.** Cesar escribió `apagar` **dentro de la
  ventana**, o sea por el teclado PS/2 emulado: `SERIO_I8042` + `KEYBOARD_ATKBD` +
  `VT`. Lo confirma un detalle que no se puede fingir: al intentar Impr Pant
  aparecieron símbolos raros en la sesión, que es `atkbd` traduciendo scancodes de
  una tecla que no es una letra.
- **`apagar` apagó la máquina.** `reboot: Power down`.

O sea que de los tres grupos de controladores nuevos, **la pantalla y el teclado
PS/2 están probados en vivo**. Lo que sigue sin probarse es lo de siempre y es
menos de lo que era: el teclado **USB** (xHCI + HID) y los discos **NVMe/AHCI**.

### Lo que falta para cerrarlo, al 2026-08-07

**Acto 2, en una PC.** Es lo único que responde el teclado USB y los discos
NVMe/AHCI, y sigue siendo el único punto del criterio donde hace falta hierro. Es
también lo único que queda: el acto 1 está hecho.

```
sudo dd if=image/build/installed.img of=/dev/sdX bs=4M status=progress conv=fsync
```

y desde la máquina que arranca de esa USB: `discos`, `instalar-en /dev/nvme0n1`,
`apagar`, sacar la USB, encender.

**Lo que no puede pasar es que el acto 1 se reporte como si fuera el acto 2**, que es
lo que esta nota ya decía y ahora tiene nombres concretos.

### Y una cosa que el criterio no pide y conviene no confundir

Una PC recién instalada arranca con un store bueno y **vacío**: no hay nada
instalado en ella y nada que instalar, porque la imagen lleva el kernel y un
programa y el `greeter` vive en el store de la máquina de desarrollo. Así que los
pasos 2 a 6 de la lista original **no se pueden hacer en la PC recién instalada**;
se siguen haciendo en la de desarrollo, donde `make -C image store` pone el bundle
en el repositorio, y se siguen comprobando en cada cambio y en la etapa 16.

Eso no es un hueco del criterio vigente —que es *«ponerla en una PC sin sistema
operativo y que ahora tenga Thalyx como OS»*, y eso se cumple— sino la pregunta de
cómo llega el software a una máquina que no es ésta, que es la Fase 2. Está escrito
aquí para que nadie lo descubra creyendo que descubrió un fallo.

## Y lo que la cancelación ya enseñó

Que no se pudo convencer a nadie de hacerlo **es un dato**, no un contratiempo
del calendario. [[Por-Que-Elegirian-Este-SO]] marca como la pregunta más
importante sin responder si el problema que Thalyx resuelve le duele a alguien
más. La primera medición de eso no fue una opinión sobre el sistema: fue que
pedir media hora de terminal a una persona ya cuesta más de lo que ese sistema
le ofrece hoy. Es consistente con lo que esta nota decía y llega antes.

## Cómo se comprueban los seis, desde el 2026-08-04

Cuatro de los seis ocurren en el prompt de la sesión, y **ninguno de esos cuatro
necesita hardware**: no hace falta BPF, ni Btrfs, ni un cgroup delegado. Hace
falta una terminal, porque el confirmador se niega sin ella.

Durante un día eso los dejó sin comprobar en ningún lado. La etapa 15 de
`verify.sh` era lo único que los cubría y necesitaba `script(1)`, que Fedora
trae en un subpaquete que no se instala solo — así que en la única máquina que
puede verificar Thalyx la etapa se saltaba entera y decía `NOT PROVEN`. El
criterio que cierra la fase dependía de que una persona corriera un comando a
mano, y ese comando había dejado de comprobarlo.

Ahora:

- **Thalyx hace su propia terminal** (`thalyx dev pty`), así que `verify.sh` no
  necesita nada que la máquina que corre Thalyx no tenga ya.
- **Los pasos 2, 3, 4 y 6 son pruebas del workspace**, en
  `crates/thalyx-cli/tests/exit_criterion.rs`. Corren en cada cambio, contra el
  disco y desde fuera de la sesión que dice haber hecho las cosas.
- **En `verify.sh` queda lo que sí necesita una máquina**: arrancar la imagen
  (paso 1), que el kernel deniegue de verdad, y un reinicio real (la mitad del
  paso 6 que un proceso nuevo no ejercita).

**Esto no cierra ningún paso.** Los cierra una persona ajena haciéndolos. Lo que
cambió es que ahora un cambio que rompa cualquiera de los cuatro se nota el
mismo día, en vez de la próxima vez que alguien se acuerde de correr el script.

## Qué cuenta como el paso 6

Decidido por Cesar el 2026-08-04, porque la bóveda decía dos cosas que no son
la misma y el criterio manda que ninguna otra lo sustituya.

El paso 6 dice *"el agente conserva el contexto de la tarea"*. Aparte,
[[Punto-Actual]] decía que *"para cerrar la fase falta el modelo del agente"*.
Lo segundo no está en el criterio, y el criterio es el criterio.

**El paso 6 se cumple cuando la máquina, después de un reinicio, dice qué se le
pidió y vuelve a comprobar lo que hizo.** El agente determinista es el que hay;
la sesión dice *"I have no model loaded"* cuando no entiende algo, así que la
máquina no aparenta un modelo que no tiene, y esa honestidad es lo que hace
aceptable la lectura corta.

**El modelo real no se cancela ni se adelanta**: [[Gamas-de-Modelo]] sigue
decretado y sigue siendo un decreto abierto. Deja de bloquear la fase.

### Y por qué eso no es un atajo

Porque lo que el paso 6 puede demostrar sin modelo es justo lo que lo hace
difícil: **una memoria que se vuelve a comprobar en vez de repetirse.**

Después de instalar, la máquina dice que la instalación *sigue en pie*. Después
de `revertir`, dice que la recuerda y **ya no la puede confirmar** — sola, sin
que nadie le avise de que el módulo se fue, porque el hecho quedó atestiguado
contra el enlace `current` que el rollback quitó. Y lo que se le pidió sigue
intacto, porque ningún archivo puede volver falso que alguien lo haya dicho.

Un modelo encima de eso cambia qué frases entiende. No cambia nada de esto.

## Lo que detiene a esa persona no es un problema difícil

Escrito el 2026-08-04, al hacer el paso 1 de verdad.

El decreto pide que arranque **con un solo comando** y sin ayuda. La máquina
existía desde el 2026-08-03; el camino hasta ella no. Y lo que rompe ese camino
nunca es Thalyx: es un paquete que falta, encontrado **de uno en uno**, y cada
uno *después* de que lo anterior salió bien. Un `bc` ausente cuesta la descarga
y la compilación enteras del kernel, y la siguiente herramienta que falte las
vuelve a costar.

`make -C image doctor` las junta todas y las contesta con una línea de `apt`.
No descarga ni compila nada, y `all` depende de él **primero** — hay una prueba
que lee el `Makefile` para eso, porque el orden de una lista de prerequisitos es
exactamente la clase de cosa que una edición posterior reacomoda sin pensarlo.

El peor de todos era **`pahole`**. Sin él, Kconfig descarta `DEBUG_INFO_BTF`
*sin decir nada*, el kernel compila y arranca, y el único síntoma aparece varios
pasos después como `thalyx-lsm` incapaz de engancharse — con la culpa cayendo
sobre el cargador, que no tuvo nada que ver. Es la misma forma que el hueco de
`bpftool`, y por eso tiene su propio párrafo en el `Makefile`.

Y el `doctor` se comprueba a sí mismo en un sitio: si `gcc` no está, las
cabeceras del kernel no se pueden probar, y **lo dice** en vez de callarlo. Es
la regla 3 de [[Estrategia-de-Pruebas]] aplicada al comprobador.

## Por qué este criterio y no uno técnico

Un criterio por componentes ("todo implementado y probado") se puede cumplir íntegramente con un sistema que no le sirve a nadie: "implementado" no es "usable", y la diferencia entre ambos solo aparece cuando alguien que no construyó el sistema intenta usarlo.

Un criterio temporal ("12 meses") no es un criterio de calidad, es un plazo.

Este criterio, en cambio, es binario, es demostrable ante terceros, y cubre de un solo golpe las demostraciones de adopción 1 y 2 y el [[Caso-Instalar-Modulo|caso canónico]] completo.

## El efecto secundario buscado

Este criterio **fuerza el contacto externo que hoy no existe**.

[[Por-Que-Elegirian-Este-SO]] marca como la pregunta más importante sin responder si el problema que Thalyx resuelve es un dolor real de otras personas o solo del creador, y [[Riesgo-de-Ejecucion]] identifica que ese razonamiento sigue siendo enteramente a priori. Un criterio de salida que exige a una persona real usar el sistema ataca los dos riesgos a la vez, sin necesidad de un esfuerzo de validación separado.

## Nadie de fuera toca el sistema antes de esto

Decretado el 2026-08-03, después de que una sesión de trabajo derivara justo
hacia lo contrario y hubiera que frenarla.

**El contacto externo no se adelanta.** No hay versión reducida, ni prueba con
un conocido, ni "que alguien lo vea aunque sea por encima" antes de que exista
el ISO y la Fase 1 esté terminada. Los seis pasos de arriba son el momento en
que una persona ajena toca Thalyx **por primera vez**, y el primero de esos
pasos es arrancar la imagen.

### Por qué, y el motivo no es miedo

No es que preocupe lo que esa persona vaya a decir. **Este proyecto nunca
dependió de eso.** Su objetivo no es impresionar a nadie de fuera; es
convencer a Cesar, y eso ocurre —o no ocurre— con independencia de cualquier
opinión ajena.

Lo que la persona ajena determina es **la escala, no la validez**: si esto se
queda como un proyecto excepcional o se convierte en algo mucho más grande. Y
la fase en la que está el proyecto es incompatible con lo segundo. Enseñar
temprano no adelanta esa respuesta, la contamina: mide la reacción a un sistema
a medias y la confunde con la reacción al sistema.

### La deriva concreta que esto previene

La sesión del 2026-08-03 llegó a proponer preparar un README de veinte minutos
y buscar a alguien que supiera abrir una terminal, saltándose el ISO. El
razonamiento sonaba bien —"el contacto externo es el riesgo mayor, y llevamos
cuatro sesiones esquivándolo"— y era un reflejo importado de otro tipo de
proyecto: **validar pronto porque el mercado decide**. Aquí el mercado no
decide. Decide el soberano, y después el mercado dice qué tan lejos llega.

Quien lea [[Riesgo-de-Ejecucion]] o la sección de abajo va a sentir el mismo
tirón. La respuesta ya está dada: **sí, el riesgo es real; se carga a
propósito hasta que exista el ISO.** Cargar un riesgo con los ojos abiertos no
es lo mismo que ignorarlo, y esa distinción es la que evita que esta nota se
convierta en una excusa.

## Relacionado
- [[Fases-de-Implementacion]]
- [[Condiciones-de-Adopcion]]
- [[Construccion-del-ISO]]
- [[Por-Que-Elegirian-Este-SO]]
- [[Riesgo-de-Ejecucion]]
