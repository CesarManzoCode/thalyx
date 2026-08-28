---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-08-27
tags: [pantalla, interfaz, framebuffer, doble-ruta, camino-confiable]
---

# La pantalla

> **Decretado por Cesar el 2026-08-27.** Sus palabras al abrirlo:
>
> > empezar a hacerlo realmente un SO […] actualmente estamos haciendo cosas a
> > ciegas, no sabemos si por lo menos esto es algo útil, y no, no basta con
> > comandos de terminal para verlo, necesito tenerlo de verdad y usarlo de
> > verdad, de lo contrario, seguiremos caminando en la niebla.
>
> Y la forma, elegida por él el mismo día: **una sola pantalla que es Thalyx.**
> Sin ventanas, sin escritorio, sin lanzador. No hay dónde *abrir* el agente
> porque el agente es la pantalla.

## Qué corrige este decreto

[[Fases-de-Implementacion]] sacó la interfaz gráfica de la Fase 1 el 2026-08-01,
con este motivo escrito: *«no participan del caso canónico ni de las
demostraciones, y posponerlos no obliga a reescribir nada»*. **Era cierto, y era
una razón condicionada a que la Fase 1 no estuviera terminada.**

La Fase 1 cerró el 2026-08-07 ([[Criterio-de-Salida-Fase-1]]). El aplazamiento
siguió vivo veinte días por inercia y no por vigencia, que es exactamente la
forma en que un decreto se vuelve una historia sobre una versión anterior del
proyecto. **Con este decreto, la pantalla entra.**

## Por qué ahora, y por qué no es adelantarse

La razón no es estética. Es la regla 1 de [[Estrategia-de-Pruebas]]: *todo
defecto real salió de correr el sistema, no de leerlo*, con catorce casos en la
regla 5 de instrumentos que mintieron.

Todo lo que Thalyx sabe hoy de sí mismo lo midió un instrumento que este
proyecto escribió, corriendo casi siempre sobre Fedora. **Una máquina en la que
una persona se puede sentar una hora es una clase de instrumento que no
existe**, y va a encontrar cosas que ninguna prueba encuentra, porque no
comparte los supuestos de quien escribió el código. Es la misma lógica que la
auditoría externa del 2026-08-04, que halló nueve defectos que 612 pruebas no
veían.

## Lo que la pantalla NO cuesta, y por qué

Cesar lo planteó así: *«la ui es independiente, podemos hacerla, dejarla ahí y
aun así continuar iterando sobre el sistema sin que nos afecte en absoluto»*.
**Tiene razón, y la razón está construida desde el 2026-08-09.**

`structured on` —el punto 4b de [[Tareas-Pendientes]]— hizo que cada verbo
conteste **un objeto por renglón tecleado**, con veintiún verbos cubiertos, la
etapa 22 de `verify.sh` comparando el cable contra `describe`, y una prueba que
afirma que la lista de verbos sólo-prosa está vacía. Las dos caras existentes
leen **el mismo `Done`** en vez de que cada una imprima el suyo.

**La pantalla es una tercera cara sobre esos mismos hechos.** No es una segunda
implementación de nada: lee lo que ya se contesta. Lo caro de una interfaz nunca
es dibujarla — es que el sistema tenga una superficie que no sea prosa para
humanos, y esa superficie ya estaba pagada antes de que a nadie se le ocurriera
dibujar un pixel.

## Dónde sí cuesta, dicho antes de construir

Tres costuras, porque no decirlas sería el error caro:

1. **La pantalla no es un programa aparte.** [[Filosofia-Fundacional]] dice que
   la imagen lleva el kernel y **un** programa, y `make -C image count` lo
   cuenta. Así que la pantalla va adentro de `thalyx` y ese número sigue
   diciendo `1`. Es el mismo argumento con el que SQLite se compila adentro: no
   hay una segunda cosa en el disco contra la cual enlazar. Pero significa que
   la interfaz **engorda el binario cuya promesa entera es ser uno solo**, y eso
   se acepta a sabiendas.

2. **El camino confiable se reimplementa.** [[Camino-Confiable]] está definido
   para un prompt de texto: teclear la ruta del dispositivo en vez de una `y`,
   **un solo lector de `stdin`**. Una confirmación dibujada es una segunda
   implementación de la superficie más crítica del sistema. Cómo se resuelve
   está abajo, y es la única parte de la pantalla que no es cosmética.

3. **`thalyx.config` se movería, si hiciera falta ratón.** *(Revisado el
   2026-08-27 al construirlo — ver la primera revisión al final.)* Los pixeles
   no piden ninguna opción nueva: `CONFIG_FB`, `CONFIG_FB_EFI` y `CONFIG_VT` ya
   están, y el teclado sigue llegando por la misma tty en modo crudo aunque la
   consola esté en modo gráfico. **El ratón sí pediría `CONFIG_INPUT_EVDEV` y
   un HID de ratón**, y ese archivo es el artefacto que **sólo el hardware de
   Cesar verifica** — las tres opciones que le faltaron históricamente se
   encontraron arrancando, no compilando ([[Estrategia-de-Pruebas]], y
   `config-check` es estructuralmente ciego a una opción que nadie pidió). Como
   esta pantalla no tiene ventanas que apretar, **el ratón queda fuera** y con
   él el único motivo para tocar el kernel.

## Sobre qué se dibuja: el framebuffer que el firmware ya dejó puesto

**Sin X, sin Wayland, sin compositor, sin driver de GPU.** No es una limitación
disfrazada de virtud: `CONFIG_FB_EFI` adopta el framebuffer que UEFI **ya
configuró** antes de que el kernel exista, y eso es un rectángulo de memoria
lineal con una resolución ya elegida. Thalyx lo abre en `/dev/fb0`, lo mapea y
escribe pixeles.

Lo que eso compra, dicho en los términos de [[Filosofia-Fundacional]]: *si Linux
desaparece, Thalyx encuentra otro motor*. Un compositor de Wayland sería una
capa intermedia con su propio protocolo, sus propios clientes y su propia
política de qué ventana está enfrente — o sea, exactamente lo que el decreto
fundacional prohíbe. **Un rectángulo de memoria no es una capa intermedia.**

Lo que eso cuesta, dicho igual de claro:

| Cuesta | Qué significa |
|---|---|
| Sin aceleración | Cada pixel lo pone la CPU. A 1920×1080 son 2 millones por cuadro |
| La resolución la eligió el firmware | Thalyx no la cambia. No hay modeset sin DRM/KMS |
| Sin GPU, sin video, sin 3D | Y no hacen falta para lo que esta pantalla es |
| Una sola pantalla física | Multi-monitor no existe aquí, y no está decretado que deba |

Si algún día hace falta cambiar el modo o encender una segunda pantalla, eso es
DRM/KMS y es **otro decreto**. No se toma por adelantado.

## La forma: una pantalla, sin ventanas

```
┌───────────────────────────────────────────────────────────────────┐
│  Thalyx    máquina · store · guardián: negando · 14:32            │  la barra
├──────────────┬────────────────────────────────────┬───────────────┤
│              │                                    │               │
│  DÓNDE       │                                    │  CORRIENDO    │
│  /home/cesar │                                    │  3 módulos    │
│              │        LA CONVERSACIÓN             │               │
│  ARCHIVOS    │                                    │  MEMORIA      │
│  12 cosas    │   · lo que la persona dijo         │  6.2 / 16 GB  │
│              │   · lo que el agente propone       │               │
│  MÓDULOS     │   · lo que la máquina contestó     │  PERMISOS     │
│  greeter 1.0 │                                    │  ninguno vivo │
│              │                                    │               │
│              ├────────────────────────────────────┤  RED          │
│              │  ▸ _                               │  enp2s0 arriba│
└──────────────┴────────────────────────────────────┴───────────────┘
```

**El centro es la conversación y no hay forma de cerrarla.** Los paneles de los
lados no son ventanas: no se mueven, no se apilan, no tienen barra de título y
no se pueden cerrar. Son **lo que la máquina está siendo**, dibujado al lado de
lo que se está diciendo.

Por qué no un escritorio, que era la otra opción sobre la mesa: un escritorio
convierte cada módulo en una aplicación con su ventana, y eso le devuelve al
usuario el modelo mental de que el sistema es un contenedor donde corren
programas — con la IA adentro de uno de ellos. **[[Filosofia-Fundacional]] dice
lo contrario**: la IA es ciudadana de primera clase, no una aplicación. Una
pantalla sin ventanas es esa frase dibujada.

## La tipografía dice de dónde viene lo que estás leyendo

Ésta es la única decisión de diseño de la pantalla que es de seguridad y no de
gusto, y sale directo de [[Marcado-de-Origen]].

En la pantalla hay tres voces, y **nunca se ven iguales**:

| Voz | Tipo | Color | Qué es |
|---|---|---|---|
| **La persona** | proporcional | blanco | Lo que se tecleó. Es la única voz soberana |
| **El agente** | proporcional | ámbar | Una *propuesta*. Nunca un hecho, nunca ejecutada por existir |
| **La máquina** | monoespaciada | azul frío | Un hecho: rutas, tamaños exactos, ids, lo que el journal registró |

Dicho con más precisión, porque construirlo obligó a separarlo en dos ejes:
**el color dice quién habló y la letra dice qué clase de texto es.** Prosa es
alguien hablando; monoespaciada es texto exacto de la máquina, donde un carácter
que se movió es otra respuesta. El renglón del prompt es el único lugar donde
una persona escribe en la gramática de la máquina, así que va monoespaciado y
en blanco: la letra dice que el texto es exacto, el color dice de quién es.

**Un hecho de la máquina jamás se dibuja con la letra del agente**, y ésa es la
regla que un lector puede aplicar sin leer. Un modelo que escupa
`ok store /dev/sdb2 ▪ …` produce una propuesta que *dice* eso; sale en ámbar y
proporcional, y por lo tanto no se parece a la línea que la máquina imprime
cuando de verdad encontró un store. Es la misma defensa que el marcado de origen
del contrato, un nivel más afuera: **la procedencia se ve.**

## El camino confiable, dibujado

El problema, dicho exacto: `Camino-Confiable.md` descansa en que hay **un solo
lector de `stdin`** y en que la confirmación es la única cosa en pantalla. Con
paneles y una conversación encima, un texto que el agente produzca podría
*dibujar* algo que se parezca a una confirmación.

La respuesta es la misma propiedad, conseguida de otra forma:

1. **Una confirmación toma la pantalla entera.** No se dibuja adentro de un
   panel ni encima de la conversación: todo lo demás desaparece. No hay una
   composición en la que una confirmación y otra cosa coexistan, así que **no
   hay nada al lado que la pueda imitar**.
2. **Mientras está puesta, la entrada va sólo ahí.** Ningún panel se actualiza,
   ninguna tecla llega a otro lado, el agente no puede escribir en la pantalla.
3. **Se sigue tecleando la cosa, no una `y`.** La ruta del dispositivo, el nombre
   del módulo. La confirmación protege del error de tipeo *y* de la costumbre.
4. **El color de esa pantalla no lo usa nada más.** Ni un panel, ni un turno de
   la conversación, ni un error. Si aparece, es porque algo está a punto de
   cambiar la máquina.
5. **La cara estructurada sigue sin poder pedirla** (`needs_a_human`), igual que
   hoy. Dibujarla no la vuelve alcanzable por un programa.

## Cómo se prueba una pantalla sin tener pantalla

Es el mismo patrón que `thalyx-term` y `thalyx-edit` ya usan, y la razón por la
que existen: **el crate es puro.** Estado adentro, pixeles afuera. No abre un
framebuffer, no toca un `ioctl`, no dibuja en ningún lado.

- **Lo que se prueba en el contenedor** —o sea, todo lo que decide cómo se ve—:
  el trazado, la tipografía, el acomodo de los paneles, dónde cae el cursor, qué
  se recorta cuando el texto no cabe, y que la confirmación tape todo. Un cuadro
  se compone en memoria y **se le hacen preguntas al pixel**.
- **Lo que espera al hardware de Cesar**: que `/dev/fb0` exista, que el formato
  de pixel de *ese* firmware sea el que se supuso, que la consola de texto suelte
  la pantalla, y que el ratón produzca eventos.

Y una consecuencia que vale por sí sola: **un cuadro se puede volcar a un
archivo de imagen desde el contenedor**, así que la pantalla se puede ver antes
de que exista una máquina que la muestre.

## Lo que esta pantalla NO es, escrito para que nadie lo asuma

- **No es un servidor gráfico.** Ningún otro programa puede dibujar en ella. Un
  módulo no tiene ventana; tiene un panel que Thalyx dibuja con lo que el módulo
  contestó por la API.
- **No es un navegador ni lleva uno.**
- **No reemplaza la sesión de texto.** [[Principio-Doble-Ruta]] es no
  negociable: la sesión de texto sigue siendo una ruta completa, y todo lo que la
  pantalla hace se puede pedir tecleado. Una pantalla que quitara capacidad
  sería una violación del decreto, no una mejora.
- **No decide nada.** Dibuja lo que las tres caras ya contestan.

## Lo que la primera entrega hace y lo que no

Construida el 2026-08-27. **Dibuja la máquina y el prompt acepta tecleo; los
verbos todavía no pasan por la pantalla.**

Eso es una frontera elegida, no algo a medias. `session::run` es un solo ciclo
de seiscientas líneas que imprime conforme avanza, y volverlo algo que *devuelve*
una respuesta es una edición grande al código más ejercido del proyecto. Hacer
las dos cosas en la misma entrega significaría que, si la máquina de Cesar
arranca en negro, **no hay manera de saber cuál de los dos cambios fue** — que
es la regla de `CLAUDE.md` sobre no apilar un segundo cambio sin verificar
encima del primero.

Así que esta entrega contesta lo que sólo su hardware puede contestar: si
`/dev/fb0` está, si ese firmware empaqueta un pixel como el código supuso, si la
consola suelta la pantalla y la devuelve, si el teclado sigue llegando en modo
gráfico, y si el acomodo es correcto a su resolución.

## Revisiones

### 2026-08-27 — Los pixeles no piden nada del kernel; el ratón sí, y sale

**Antes:** «Ratón y pixeles piden opciones de kernel que no estaban».

**Ahora:** los pixeles no piden ninguna. `FB`, `FB_EFI` y `VT` ya estaban desde
el 2026-08-07, cuando se pidieron para que la consola de texto se viera en
hierro. Y el teclado no cambia: `KD_GRAPHICS` sólo impide que el kernel dibuje
la consola, no que la tty entregue las teclas, así que el modo crudo que
`thalyx-term` ya usa sigue sirviendo igual.

**Motivo:** se midió al construirlo. La consecuencia es que **esta entrega no
toca `thalyx.config`**, o sea que no arriesga el arranque de la única máquina
que verifica el proyecto. El ratón queda fuera de la entrega por lo mismo: una
pantalla sin ventanas no tiene qué apretar, y sería el único motivo para mover
el kernel.

### 2026-08-27 — Ctrl-C, que en la sesión es la salida, aquí es la trampa

Encontrado al escribir el ciclo. `RawMode` deja `ISIG` prendido a propósito, y la
razón escrita a su lado es correcta: *una persona cuya máquina es una sola
terminal necesita una salida que no dependa de que el programa esté bien*.

**Con la consola en modo gráfico esa misma salida es lo que deja la máquina en
negro.** `SIGINT` mata el proceso antes de que `Drop` devuelva la consola, y lo
que queda es una pantalla apagada sobre una máquina que está corriendo bien, sin
una segunda terminal desde donde arreglarlo.

Así que la pantalla usa `RawMode::enter_without_signals`: apaga `ISIG` e `IXON`,
recibe el Ctrl-C como un byte y sale por su propio pie, restaurando al salir. **La
salida es del mismo tamaño; nada más que ahora pasa por `Drop`.** Es la misma
lección que el modo crudo aprendió una vez, un nivel más adentro: una tecla que
en un modo es el escape, en otro es lo que cierra la puerta.

## Relacionado
- [[Filosofia-Fundacional]] — el kernel y un programa; sin capas intermedias
- [[Principio-Doble-Ruta]] — por qué la sesión de texto no se retira
- [[Camino-Confiable]] — la confirmación que esta pantalla reimplementa
- [[Marcado-de-Origen]] — de dónde sale que la tipografía cargue procedencia
- [[Superficie-para-el-LLM]] — la cara estructurada sobre la que esto se apoya
- [[Construccion-del-ISO]] — dónde vive `thalyx.config`
- [[Fases-de-Implementacion]] — el decreto que este corrige
