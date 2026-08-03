---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-03
tags: [iso, imagen, build, qemu, fase-1, no-negociable]
---

# Construcción de la imagen del sistema

> Esta nota se reescribió entera el 2026-08-03. La versión anterior decretaba
> una distribución de Alpine y contradecía a [[Decision-Capa-vs-SO-Nuevo]] sin
> que nadie lo notara durante tres días. Lo que decía, y por qué era falso, está
> al final.

## Decreto

**La imagen contiene el kernel de Linux y un programa: `thalyx`.**

Eso es todo, y está escrito así porque es **contable**: se listan los archivos, y
o hay dos cosas o hay más. Un criterio que se comprueba mirando, no discutiendo.

Lo que no lleva, y no llevará:

- Ninguna distribución. **Ni Alpine ni ninguna otra, nunca.**
- Ningún shell.
- Ningún conjunto de utilidades de sistema — `ls`, `cat`, `id`, busybox.
- Ningún gestor de paquetes. El software llega en `.thmod` o no llega.
- Ningún init de terceros. PID 1 es `thalyx`.
- Ninguna herramienta de construcción de distros: `mkimage.sh`, apkovl,
  live-build.

## Qué es y qué no es "depender del kernel de Linux"

**El kernel de Linux nunca estuvo en discusión.** Es el kernel, `thalyx-lsm` lo
extiende desde la Fase 1, y escribir uno propio es otro proyecto que
[[Fases-de-Implementacion]] pone en años 2-3 y "si aplica".

Una distribución es otra cosa: es el userland, el gestor de paquetes, el init y
las decisiones de arranque que alguien más ya tomó. **Eso es lo que queda
fuera.** El kernel es una pieza que se usa; una distro es un sistema operativo
ajeno dentro del cual el nuestro estaría incrustado — que es exactamente donde
estaba hasta hoy.

`musl` se enlaza **estáticamente**, en tiempo de compilación. No es un programa
en el disco: es código compilado adentro, como cualquier biblioteca. En la
imagen no hay cargador dinámico ni libc que alguien pueda invocar, porque no hay
nadie que pueda invocarla.

## La consecuencia que hace que esto valga la pena

Sin userland, **un módulo no puede ser un script**. No hay `sh` que lo interprete
ni `ls` al que llamar.

Un módulo no tiene con quién hablar excepto Thalyx, y entonces la **API interna**
que [[Core-Nucleo]] decreta —y que hasta hoy no existía ni en una línea— deja de
ser opcional: es la única forma de que un módulo haga algo.

Eso cierra el hueco que [[Decision-Capa-vs-SO-Nuevo]] nombraba y Thalyx no
cumplía. El criterio de ese decreto es que los programas se escriban contra el
contrato de Thalyx; con un userland POSIX debajo siempre había otro contrato
disponible, y ninguno usaba el nuestro. Sin él no hay alternativa.

**Quitar la distro y volverse un sistema operativo son el mismo acto.**

## Cómo se construye, decidido el 2026-08-03

**Un initramfs, no un ISO.** Un ISO necesita gestor de arranque, tabla de
particiones y un filesystem donde ponerlos. Un initramfs no necesita ninguno de
los tres: el kernel desempaca un archivo cpio en un tmpfs y ejecuta `/init`. A
QEMU se le pasan el kernel y el archivo, y nada más.

No es un atajo alrededor del decreto: es el decreto sin nada sobrante. **No hay
una tercera cosa en el camino donde algo se pueda esconder**, que es exactamente
por donde entró la base de Alpine la vez pasada.

**El archivo lo construye Thalyx**, en `crates/thalyx-cli/image.rs`. No por
desconfiar de `cpio` —es un programa perfectamente bueno— sino porque la forma
del error anterior fue *agarrar lo que la base ofrecía*, y un sistema que
embarca un solo programa puede producir su propio sistema de archivos raíz en
doscientas líneas en vez de heredar una cuarta cosa que nadie eligió.

**Un archivo, no dos.** La primera versión ponía el binario en `/thalyx` y en
`/init` por familiaridad, y duplicaba una imagen de 47 MB para decir dos veces lo
mismo. Si el decreto dice "un programa", un solo archivo es lo que lo vuelve
cierto en vez de casi cierto — y es lo que devuelve la cuenta.

**El kernel parte de `allnoconfig`.** La dirección importa: un kernel de
distribución con cosas apagadas sigue conteniendo todo lo que nadie se acordó de
apagar, y nadie puede decir qué hay adentro. Partiendo de cero, lo que está
encendido es lo que alguien decidió encender. La lista está en
`image/thalyx.config`, con un motivo al lado de cada grupo.

**Y se comprueba que sobreviva.** `olddefconfig` descarta en silencio cualquier
opción cuyas dependencias no se cumplan: la línea simplemente no aparece en el
`.config` resultante, sin una advertencia. Nueve de las opciones pedidas se
estaban perdiendo así, entre ellas `CONFIG_BPF_LSM` y `CONFIG_DEBUG_INFO_BTF`
—las dos que `thalyx-lsm` necesita para existir—. Por eso `make -C image kernel`
compara línea por línea lo pedido contra lo que salió y **detiene la compilación**
si falta alguna. Ver la regla de [[Estrategia-de-Pruebas]] sobre pedir y tener.

**El estado persistente va en otro disco.** La raíz es un tmpfs que no conserva
nada entre arranques. PID 1 monta los tres subvolúmenes; **no los crea**, porque
una máquina que se fabrica un store nuevo cada vez que no encuentra el viejo
nunca podría avisarte de que lo perdió.

### Cómo se cuenta

El decreto se escribió para ser contable, y esto es lo que cuenta:

```
make -C image count
```

Si dice algo distinto de `1 program(s) in the image`, el decreto está roto y el
número lo dice antes de que nadie tenga que discutirlo. Lo cuenta **parseando el
archivo**, no reportando lo que el constructor creía haber metido.

## Qué queda por decidir, y no se hereda

La versión anterior heredó de Alpine cosas que nadie eligió — el login en tty1
fue la más visible, no la única. Lo que sigue abierto se escribe aquí para que
se decida en vez de aparecer:

- **Cargar `thalyx-lsm` desde dentro de Thalyx.** Es el hueco grande. El cargador
  invocaba `bpftool`, y no hay bpftool en la imagen ni shell desde donde
  llamarlo. La máquina arranca y lo dice —"enforcement absent"— con las mismas
  palabras que usa `thalyx session` en cualquier máquina que no lo tenga. Es
  honesto y es el agujero más grande que queda.
- **Crear los subvolúmenes del store la primera vez.** PID 1 los monta y no los
  crea; falta decidir quién los hace y cuándo.
- **Qué pasa al apagar.**
- **Que el binario sea estático de verdad.** Hoy enlaza glibc dinámicamente. El
  `Makefile` lo comprueba y se niega si no lo es, pero nunca se ha corrido.

## Cómo se arranca

QEMU, mientras dure la Fase 1. El paso 1 del [[Criterio-de-Salida-Fase-1]] es
arrancar la imagen; el soporte de hardware real llega cuando el sistema
justifique correr fuera de una máquina virtual.

## Historial: lo que esta nota decretaba antes, y por qué era falso

Del 2026-08-01 al 2026-08-03 decía:

> La imagen se construye con las herramientas nativas de Alpine (`mkimage` y
> apkovl). […] **Base Alpine minimalista.** […] Por qué Alpine y no otra base:
> coherencia con la base ya decretada en [[Core-Nucleo]], y tamaño.

`mkimage.sh` es la herramienta con la que se construyen distribuciones de
Alpine, y lo que produce es una distribución de Alpine. La bóveda decretaba, al
mismo tiempo:

| Nota | Decía |
|---|---|
| [[Decision-Capa-vs-SO-Nuevo]] *(no negociable)* | "No es una capa y **no es una distribución de Linux**" |
| Esta nota | Se construye con las herramientas de Alpine, base Alpine |
| [[Fases-de-Implementacion]] | "Fase 1: Núcleo de Thalyx **sobre base Alpine**" |
| [[Core-Nucleo]] | "Base: kernel Linux minimalista **(tipo Alpine)**" — Alpine no tiene kernel propio; es un error de categoría |

Tres notas decretaban una distro y una decretaba que no lo era. Convivieron tres
días, y las cuatro se leyeron el mismo día sin que saltara.

Lo encontró Cesar preguntando por qué habría un login al arrancar si nadie lo
construyó. La respuesta —que lo pone la base— hizo visible que había una base.

El esqueleto de imagen escrito esa misma noche se borró con esta reescritura,
junto con el módulo `dev.thalyx.hola`, que era un script de shell. Quitarle el
getty a una distro de Alpine hace que no la veas; no hace que no esté. Y eso es
peor que dejarla a la vista, porque esconde justo la pregunta que hay que poder
hacerse.

## Relacionado
- [[Decision-Capa-vs-SO-Nuevo]] — por qué esto no es negociable
- [[Core-Nucleo]] — qué lleva el sistema, y su API interna
- [[Fases-de-Implementacion]]
- [[Criterio-de-Salida-Fase-1]]
- [[Condiciones-de-Adopcion]]
