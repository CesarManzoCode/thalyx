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
