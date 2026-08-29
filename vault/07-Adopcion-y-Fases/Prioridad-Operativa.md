---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-28
tags: [prioridad, estrategia, agentes, evidencia, roadmap, decreto]
---

# La prioridad de esta etapa: demostrar que Thalyx mejora el trabajo de un agente

> **Esto es un cambio de orden, no un cambio de destino.** Lo que sigue
> reordena en qué se gasta el tiempo de construcción a partir del 2026-08-28.
> No retira ningún decreto, no cancela ninguna fase y no declara equivocada
> ninguna decisión anterior. Lo que había escrito antes sigue escrito donde
> estaba, con su fecha.

## Lo primero, porque es lo que se malinterpreta

**La visión no se mueve.** [[Filosofia-Fundacional]] sigue intacta, palabra por
palabra:

> Thalyx es el sistema operativo. […] **Thalyx es el todo. Sin Thalyx no hay
> nada.**

Thalyx aspira a ser el sistema completo. No es un plugin, no es una distro, no
es un IDE, y **no se convierte en ninguna de esas cosas por esta nota**. La
imagen sigue llevando el kernel de Linux y **un** programa, y `make -C image
count` lo sigue diciendo.

Lo que cambia es **en qué orden se construye lo que falta**.

## La línea temporal, que es lo que hace entendible la decisión

Escrita así porque un cambio de prioridad sin su historia se lee como que
alguien se arrepintió:

1. **Primero se cerró y maduró el núcleo.** Fase 1: el flujo canónico, el LSM
   haciendo cumplir permisos en un kernel real, el índice, la memoria, el
   sandbox, el journal y los snapshots, la imagen de un solo programa, el
   instalador, y una PC arrancando Thalyx desde su propio firmware el
   2026-08-07. Ver [[Criterio-de-Salida-Fase-1]] y [[Estado-de-Implementacion]].
2. **Después se avanzó hacia habitabilidad**: la pantalla como cara del
   arranque, el teclado, el editor, el motor de inferencia como módulo
   residente. Thalyx empezó a ser una máquina en la que se puede estar, no sólo
   una que se puede demostrar.
3. **El puente para agentes externos** ([[Agentes-Externos]], 2026-08-28) hizo
   posible por primera vez la pregunta que el proyecto tenía sin contestar
   desde el día uno: **el mismo modelo, las dos superficies.**
4. **Las primeras corridas produjeron evidencia cuantitativa** de una ventaja
   fuerte en trabajo de comprensión de código, y una corrida de escritura donde
   la ventaja no apareció. Todo está, corrida por corrida, en
   [[Evidencia-de-Agentes]].
5. **Esa evidencia provocó, a propósito, esta repriorización**: maximizar y
   validar esa ventaja **antes** de invertir más trabajo en habitabilidad
   general.
6. **Las capacidades generales siguen siendo parte de la visión de largo
   plazo.** Quedan diferidas, no retiradas.

Esto es aprendizaje del proyecto. Los pasos 1 y 2 no fueron un rodeo: sin el
núcleo no había primitivas que medir, y sin habitabilidad mínima no había
máquina donde medirlas.

## El decreto de Cesar — 2026-08-28

**Durante esta etapa, la prioridad operativa de Thalyx es, en este orden:**

1. **Demostrar que Thalyx mejora el trabajo de agentes reales.**
2. **Medir**: éxito, costo, tokens y contexto, tiempo, número de interacciones,
   y capacidad de recuperación.
3. **Dogfood**: usar Thalyx para desarrollar Thalyx siempre que la prueba sea
   válida. El protocolo está en [[Evidencia-de-Agentes]].
4. **Mejorar únicamente mecanismos cuya necesidad venga de evidencia
   concreta.**
5. **Incorporar mecanismos ya demostrados por otras herramientas** cuando
   encajen con la arquitectura de Thalyx.
6. **Reducir la fricción de adopción**: proyecto → VM de Thalyx → el mismo
   Claude/Codex/agente → trabajar.
7. **Posponer temporalmente el trabajo de habitabilidad general** que no ayude a
   esas metas.

Y la frase que resume el orden entero:

> **Primero demostrar que Thalyx hace mejor al agente. Después hacer que Thalyx
> pueda reemplazar al resto de la máquina.**

## La regla de prioridad

Una tarea tiene **prioridad inmediata** si aumenta de forma plausible y medible
alguna de estas cinco cosas:

1. **éxito o corrección** del agente;
2. **reducción de costo, tokens o contexto**;
3. **reducción de tiempo o de trabajo** (turnos, llamadas, intervención humana);
4. **seguridad y reversibilidad** del trabajo;
5. **facilidad de adopción** del entorno por agentes que ya existen.

**Si no afecta a ninguna, normalmente espera.**

### Cómo se relaciona con los cinco costos

No la sustituye. [[Superficie-para-el-LLM]] decreta los **cinco costos** —
descubrimiento, contexto, ambigüedad, equivocarse, permiso — y siguen
siendo el criterio de **diseño**: qué merece existir en la superficie.

Esta regla es más chica y más dura: dice qué merece **el tiempo de construcción
de esta etapa**, y exige algo que los cinco costos no exigen — que el efecto se
pueda **observar en una corrida**. Un cambio puede bajar el costo de
descubrimiento en el papel y no mover ninguna de las cinco palancas de arriba en
ninguna tarea medida; entonces está bien diseñado y espera.

El punto 5 —adopción— es el único que no tiene equivalente entre los cinco
costos, y por eso está escrito aparte: un agente que no puede llegar a la
máquina no paga ningún costo, porque no está.

## Lo que queda diferido

Sigue formando parte de la visión. **No está abandonado, no está equivocado, y
la decisión de construirlo algún día no se retira.** Lo que se retira es su
lugar en la cola:

- navegador general (Firefox u otro);
- soporte amplio de paquetes;
- catálogo de aplicaciones;
- escritorio generalista;
- multimedia;
- compatibilidad de software por completitud.

**Un hecho que conviene decir, porque cambia el tamaño de este párrafo:**
ninguna de esas seis cosas estaba decretada en la bóveda. Nadie prometió un
navegador, [[Construccion-del-ISO]] decreta explícitamente que **no hay gestor
de paquetes** —«el software llega en `.thmod` o no llega»— y [[La-Pantalla]]
dice que no hay escritorio ni ventanas. Así que esto no defiere decretos: defiere
una **aspiración de habitabilidad general** que vivía en la Fase 4
([[Fases-de-Implementacion]]) y en la idea de que Thalyx reemplace la máquina
entera de alguien.

Lo que sí toca, escrito con nombre para que nadie tenga que adivinarlo:

- **`DEVELOPER RUNTIME / TOOLCHAIN MODULES`** —meter toolchains, libc, git y
  agentes adentro del guest— sigue exactamente donde [[Agentes-Externos]] lo
  dejó: después de saber si las primitivas aportan valor. Esta nota no lo mueve,
  lo confirma.
- **La Fase 4 (ecosistema)** de [[Fases-de-Implementacion]] no se adelanta.
- **Las demostraciones 2 y 3 de [[Condiciones-de-Adopcion]]** —memoria entre
  sesiones, orquestación entre módulos— no son de esta etapa. La demostración 1,
  el rollback de una refactorización, **sí lo es**, porque es literalmente la
  tarea `reversible` del banco.

## Lo que esta prioridad NO significa

Escrito porque es la forma en que este decreto se puede pudrir:

- **Thalyx no se convierte en «un IDE para Claude».** El agente sigue siendo
  **software externo no confiable**: fuera de la TCB, sin ejecutar nada por su
  cuenta, con todo lo que hace marcado `origin: untrusted_content` en el journal
  ([[Marcado-de-Origen]]).
- **La estructura, los permisos, el filesystem, el índice, el journal, el
  rollback y la autoridad siguen siendo de Thalyx.** Ninguna de esas cosas se
  mueve al anfitrión para que a un agente le salga más cómodo.
- **El puente MCP sigue siendo un adaptador de adopción**, no la API interna y
  no la visión final. La regla mecánica de [[Agentes-Externos]] no se relaja:
  **nada en `thalyx-mcp` abre un archivo del workspace.**
- **[[Principio-Doble-Ruta]] no se toca.** Todo lo que un agente puede hacer, un
  humano lo puede hacer directo.

## Herramientas externas: aprender, no evitar

**Thalyx no va a evitar un mecanismo útil sólo porque ya existe en otra
herramienta.** Si Serena, Sourcegraph, Cursor, un LSP como rust-analyzer u otro
sistema tienen mecanismos con evidencia clara de que mejoran a un agente, se
estudian y se absorben los principios que encajen.

La regla, en una línea:

> **No copiar productos; absorber mecanismos demostrados.**

Candidatos que merecen **evaluación futura**, y que aquí no se aprueban ni se
implementan:

- **edición por símbolo** — `insert_before_symbol`, `insert_after_symbol`,
  `replace_symbol_body`: escribir en la unidad en la que el índice ya piensa, en
  vez de por número de renglón;
- **resultados semánticos comprimidos o progresivos** — devolver la firma y
  pedir el cuerpo sólo si hace falta;
- **batching de operaciones** — varias mutaciones relacionadas en una llamada,
  que es también menos turnos;
- **análisis más preciso respaldado por LSP/rust-analyzer** donde esté
  disponible, con el índice barato propio como base y respaldo;
- **capas de precisión**: lo caro y exacto encima, lo barato y siempre
  disponible debajo.

Ninguna de las cinco es una decisión cerrada. Son **hipótesis informadas** por
evidencia externa y por lo que CHANGE #1 mostró: en edición simple de un archivo
todavía no hay ventaja observada ([[Evidencia-de-Agentes]]). Cada una tendría
que pasar por los cinco costos de [[Superficie-para-el-LLM]] y por
[[Criterio-de-Inclusion-de-Primitivas]] antes de existir, y ninguna se
construye por intuición antes de tener la medición que la pida.

**Y dónde sí está la innovación**, porque conviene decirlo: no en inventar cada
primitiva. En integrarlas bajo una máquina coherente donde el código, el
filesystem, los permisos, el índice, los cambios, el journal, los snapshots y el
rollback **son partes del mismo sistema** para el agente, y no siete
herramientas que no se conocen entre sí.

## Cuándo se revisa esta prioridad

No tiene fecha; tiene condiciones. Se vuelve a abrir cuando pase cualquiera de
estas:

- **la evidencia se acumula en contra** — varias tareas medidas donde el brazo
  Thalyx no aporta ventaja, y no sólo la de escritura simple que ya se observó;
- **la ventaja queda demostrada lo bastante** como para que lo que bloquee la
  adopción sea la habitabilidad y no las primitivas;
- **Cesar lo decide**, que es el caso que no necesita ninguno de los otros dos.

## Relacionado

- [[Evidencia-de-Agentes]] — **el documento canónico de evidencia**: cada
  corrida, con sus números y sus límites, y el protocolo del bug real.
- [[Agentes-Externos]] — el decreto del puente, y por qué MCP es un adaptador.
- [[Filosofia-Fundacional]] — la visión, que esto no mueve.
- [[Superficie-para-el-LLM]] — los cinco costos, que siguen siendo el criterio
  de diseño.
- [[Ritmo-de-Construccion]] — qué se le pregunta a Cesar y qué se hace sin
  preguntar.
- [[Fases-de-Implementacion]] — las fases, con la revisión que apunta acá.
- [[Condiciones-de-Adopcion]] — los gates de apertura a usuarios.
- [[Criterio-de-Inclusion-de-Primitivas]] — el filtro que cualquier mecanismo
  nuevo tiene que pasar.
