---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-01
tags: [seguridad, autorizacion, agente, no-negociable]
---

# Camino confiable

## Problema

En el trazado original de [[Caso-Instalar-Modulo]], la confirmación de permisos viajaba así:

```
Core → Agente → Usuario
```

Pero el [[Modelo-de-Amenaza]] decreta que el agente **no pertenece a la TCB**. Delegarle redactar y mostrar la solicitud de permisos significa que un agente manipulado puede solicitar una cosa y mostrarle otra al humano.

Sin resolver esto, el principio de soberanía humana de la [[Filosofia-Fundacional]] queda vacío: el soberano solo puede decidir sobre lo que ve, y lo que ve se lo escribe el componente que no es confiable.

## Decreto

Toda solicitud de autorización se **genera y se renderiza por `thalyx-core`**.

1. **El contenido lo genera el Core**, a partir de los campos del contrato ya validado, usando plantillas fijas. El agente no compone, no reformula y no resume el texto de la solicitud.
2. **El canal lo controla el Core.** La solicitud se presenta por una vía que el agente no puede interceptar, suprimir ni retrasar.
3. **La solicitud está identificada** de forma visualmente inconfundible como emitida por Thalyx, no por el agente.
4. **La prosa del agente se muestra aparte.** El agente puede explicar, recomendar y acompañar, pero su texto va en un área separada y marcada como no confiable. Nunca aparece dentro del bloque sobre el que el humano decide.

## Alcance

Aplica a las tres categorías de [[Tres-Categorias-de-Autorizacion|autorización]]:

- Autorización operacional ("¿ejecuto esto?").
- Autorización de capacidades ("¿aceptás estos permisos?").
- Confirmación de operaciones destructivas, incluido `thalyx restore` — ver [[Rollback-vs-Restore]] y [[Coherencia-Doble-Ruta]].

## Consecuencia de diseño

El camino confiable obliga a que la CLI de Fase 1 tenga un modo de presentación que no pase por el flujo conversacional del agente. Es un requisito de arquitectura, no un detalle de interfaz: define dónde termina el agente y dónde empieza el sistema.

## Regla que apareció al auditar el marco

**Que el Core genere el texto no basta si el Core dibuja lo que le entregan.**

El prompt se arma con plantillas fijas del Core, y aun así interpola el `name`
del módulo, su id, su versión y los recursos de cada permiso. Todos son
cadenas que escribió un publicador. La firma dice **quién** las escribió; no
dice nada sobre **qué** escribió, y para un id nuevo la firma propia es todo lo
que el uso-en-primera-vez exige.

Así que un publicador podía poner un salto de línea y un `│` en el nombre del
módulo y pintar líneas de más dentro del marco, o una secuencia ANSI y repintar
la pantalla entera. El marco existe precisamente para que el humano distinga a
Thalyx de todo lo que corre dentro, y el marco dibujaba lo que le dieran.

Ahora todo campo de origen no confiable pasa por un saneador antes de llegar a
la pantalla: una sola línea, sin caracteres de control, sin escapes, longitud
acotada. Los caracteres de control se **reemplazan** por `·` en vez de
eliminarse — un carácter eliminado no deja evidencia, y un nombre que se lee
raro es como alguien nota que traía algo que no debía.

### El módulo no tiene terminal

La segunda mitad, y la que estaba abierta de par en par: el módulo heredaba
`stdin`, `stdout` y `stderr` de Thalyx, o sea que compartía terminal con el
camino confiable. De ahí salen dos cosas, y las dos deshacen este decreto:

- **Podía leer `stdin`.** El prompt de confirmación se responde escribiendo en
  ese mismo descriptor. Un módulo corriendo puede ganarle a la `s` del humano, o
  peor, ver qué está respondiendo a otra pregunta.
- **Podía escribir `stdout`.** Cualquier cosa, incluido un marco que dijera
  `┌─ Thalyx — capability authorisation`.

El canal en el descriptor 3 es cómo un módulo le habla al humano: a través de
Thalyx, etiquetado y acotado. Ése es el diseño entero — un módulo tiene canal
precisamente para no necesitar terminal. `dev.thalyx.greeter` lo dice en su
propia documentación: *sobre el canal y no a una terminal, porque una terminal
no es algo que tenga*. La tenía.

Desde el 4 de agosto de 2026 no la tiene: `stdin` cerrado, y `stdout` y `stderr`
son tuberías de las que Thalyx tiene el otro extremo, también con `--unconfined`
— que significa "sin cgroup y sin política del kernel" y nunca significó "puede
falsificar el camino confiable".

Y lo que el módulo dice por el canal se sanea igual antes de imprimirlo. Pasar
el texto por Thalyx no logra nada por sí solo si después ese texto puede traer
un salto de línea y repintar la marca que dice quién habla.

### Qué se le quita exactamente, y qué no

Fueron tuberías a `/dev/null` durante un día, y eso resultó ser más de lo que
este decreto pide. La etapa 6 de `verify.sh` prueba el sandbox **preguntándole
al programa confinado qué ve** —regla 2 de [[Estrategia-de-Pruebas]]— y esas
respuestas viajan por `stdout`. Descartarlas dejó a la contención sin testigo:
un módulo aislado y uno sin aislar pasaron a contestar lo mismo, que es nada.

La propiedad que este decreto necesita no es que el módulo calle. Es:

> **Un módulo no puede empezar una línea.**

Todo lo que escribe llega detrás del marcador de Thalyx, saneado, acotado y
etiquetado como lo que es: bytes en un descriptor que Thalyx **no** media, a
diferencia del canal. Es la misma afirmación que se hace sobre el nombre de un
publicador — sus palabras se ven, porque son suyas y el humano debe verlas; lo
que no puede es agregar una *línea*.

Un módulo que dice algo y un módulo al que se le borra lo que dijo son
sistemas distintos, y solo el segundo hace imposible que un módulo diga por qué
falló.

## Revisiones

### 2026-08-04 — El marco deja de dibujar lo que le entregan
**Antes:** el decreto se consideraba cumplido porque el Core generaba el texto.
**Ahora:** además lo sanea, y el módulo no tiene terminal.
**Motivo:** las dos formas de falsificar el marco no pasaban por componer el
texto — pasaban por los campos que el texto interpola y por el descriptor donde
se dibuja.
**Cómo se encontró:** una auditoría externa señaló los campos del manifiesto;
la terminal heredada apareció al ir a comprobarlo.

### 2026-08-05 — Quitarle la terminal no es quitarle la voz
**Antes:** `stdout` y `stderr` del módulo iban a `/dev/null`.
**Ahora:** van a tuberías que Thalyx drena y reimprime marcadas y saneadas. La
propiedad es que el módulo no puede empezar una línea, no que no pueda hablar.
**Motivo:** el descarte cegó a la etapa 6 de `verify.sh`, que prueba el sandbox
preguntándole al programa confinado qué ve. Seis comprobaciones pasaron a
reportar `nothing`, que es lo mismo que reportaría un sandbox que no aisló nada.
Y un módulo que muere con un mensaje en `stderr` dejaba de poder decir por qué,
contra la regla 10 de `CLAUDE.md`.
**Cómo se encontró:** corriendo `verify.sh` en la máquina de Cesar. La prueba
que debía atrapar esto afirmaba una **ausencia** —que el texto del módulo no
aparece— y eso se satisface borrándolo todo; pasó en verde la misma corrida.

## Relacionado
- [[Modelo-de-Amenaza]]
- [[Tres-Categorias-de-Autorizacion]]
- [[Permisos-JIT]]
- [[Filosofia-Fundacional]]
- [[Caso-Instalar-Modulo]]
- [[Sandbox-Ejecucion]]
