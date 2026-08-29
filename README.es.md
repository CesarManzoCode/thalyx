# Thalyx — en español

**El README principal está en inglés: [README.md](README.md).** Esta página no
es una traducción completa a propósito. Dos copias grandes del mismo texto se
desincronizan, y cuando eso pasa la que miente es siempre la que nadie está
leyendo. Aquí está lo necesario para orientarse, y el detalle vive donde ya
estaba escrito en español: en el vault.

---

## Qué es

Un sistema operativo donde la IA es ciudadana de primera clase en vez de una
aplicación, y donde el humano conserva una ruta completa que nunca pasa por
ella. El kernel de Linux es un componente que Thalyx gestiona, no el anfitrión
sobre el que descansa.

> **Thalyx es el sistema operativo.** No hay capas intermedias, no hay
> distribuciones — no hay nada que no sea Thalyx. Los módulos y el agente se
> comunican exclusivamente a través de la API de Thalyx. Si Linux desaparece,
> Thalyx encuentra otro motor. Si Thalyx desaparece, no hay sistema.
>
> — `vault/01-Filosofia/Filosofia-Fundacional.md`

La imagen contiene el kernel de Linux y **un** programa. Eso es contable en vez
de citable: `make -C image count`.

## Las dos reglas a las que responde todo el diseño

**Doble ruta.** Todo lo que el agente puede hacer, un humano puede hacerlo
directo, sin el agente y sin perder capacidad. El agente acelera; nunca es un
intermediario obligatorio. De ahí sale una consecuencia que atraviesa el diseño:
Thalyx nunca tiene conocimiento completo de su propio sistema de archivos —
porque usted es libre de cambiar cosas a sus espaldas — así que ninguna
operación destructiva puede dar por hecho que sí lo tiene.

**El agente no es de fiar.** Vive fuera de la base de cómputo confiable. No
ejecuta nada directamente, no compone los avisos contra los que usted autoriza,
y no deja que un texto ajeno que leyó decida lo que pasa. El núcleo revalida
todo lo que el agente produce.

El recuadro de autorización que aparece en el README principal es esa segunda
regla hecha concreta: lo dibuja el núcleo a partir del manifiesto **firmado**,
así que el agente no puede redactarlo, ni reescribirlo, ni enseñarle a usted un
subconjunto de lo que se está pidiendo.

## Dónde leer el resto

El vault es la autoridad: el código implementa decretos, no los inventa. Está
completo, en español, dentro de este repositorio.

| Para | Empiece en |
|---|---|
| La lectura ordenada de todo el diseño | `vault/00-Indice/Indice-Principal.md` |
| Por qué existe el proyecto, en palabras de Cesar | `vault/01-Filosofia/Filosofia-Fundacional.md` |
| La doble ruta, como principio | `vault/01-Filosofia/Principio-Doble-Ruta.md` |
| Cómo está partido el sistema | `vault/02-Arquitectura/Arquitectura-Asimetrica.md` |
| Qué es un módulo y qué puede pedir | `vault/02-Arquitectura/Sistema-de-Modulos.md` |
| Qué superficie ve el modelo | `vault/02-Arquitectura/Superficie-para-el-LLM.md` |
| Cómo se prueba, y por qué así | `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md` |
| **En qué punto está el proyecto hoy** | `vault/06-Pendientes/Punto-Actual.md` |
| Qué falta y qué está decidido | `vault/06-Pendientes/Tareas-Pendientes.md` |

## Arrancarlo

El recorrido completo está en [docs/BOOT.md](docs/BOOT.md) (en inglés, un
comando por paso). En corto, y siempre dentro de QEMU — su máquina no se toca, y
`sudo` aparece exactamente una vez, para formatear una imagen de disco:

```sh
make -C image doctor      # dice de una vez todo lo que falta; no compila nada
make -C image             # kernel, programa, imagen
make -C image store-stage
sudo make -C image store  # el único comando que necesita root
make -C image run         # arrancar
```

La máquina levanta, dice qué tiene y qué no, y **sale en la pantalla**: una sola
superficie con la conversación al centro y paneles alrededor, dibujada por
Thalyx sobre el framebuffer del firmware — sin X, sin Wayland, sin compositor, y
adentro del mismo programa, así que `make -C image count` sigue diciendo `1`. No
hay comando que la encienda: es lo que se ve al arrancar. **No hay login**,
porque no hay nadie más que ser. **No hay shell**: lo que no es una palabra que
la sesión conozca, no existe.

Si esa pantalla saliera en negro, Ctrl-C con la línea vacía devuelve la sesión de
texto **a ciegas**, y `thalyx.pantalla=no` en la línea de comandos del kernel
arranca directo en ella.

## Qué está probado y qué no

La contabilidad honesta —con fechas, qué cubrió cada verificación, los límites
abiertos y la contradicción que el proyecto publica en vez de esconder— está en
[docs/STATUS.md](docs/STATUS.md).

En una línea: la Fase 1 cerró el 2026-08-07, cuando una PC arrancó Thalyx desde
USB con su propio firmware, se instaló sola en un segundo disco y volvió a
arrancar sin el medio de instalación; `sudo ./dev/verify.sh` pasó 110
verificaciones en esa máquina; y lo que no se pudo comprobar se reporta como
`NOT PROVEN` con la razón, nunca como un pase.

## Idioma

Español para el vault y para la conversación. Inglés para todo lo demás: código,
comentarios, identificadores, esquemas, mensajes de commit, salida del CLI y
nombres de archivo.
