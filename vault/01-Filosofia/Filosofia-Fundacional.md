---
tipo: filosofia
estado: decretado
fecha-decreto: 2026-07-31
tags: [filosofia, core, no-negociable]
---

# Filosofía fundacional

El sistema se llama **Thalyx**. Ver [[Nomenclatura-y-Convenciones]].

## El objetivo, y todo lo demás es medio

**Decretado por Cesar el 2026-08-09.** Es la frase contra la que se juzga
cualquier decisión de este repositorio:

> todo lo que haremos girará en torno a que un LLM se desenvuelva mejor en
> nuestro sistema, todo lo demás son medios para lograrlo, y sí, el humano
> deberá tener todo lo que tiene en Linux, pero no es un objetivo, es una
> obligación que hay que cumplir.

Dos frases y hay que leerlas juntas:

1. **El objetivo es uno.** Que un LLM trabaje mejor aquí que en cualquier otro
   sistema. No «también»: es el que hay.
2. **El camino humano es obligación, no meta.** [[Principio-Doble-Ruta]] se
   cumple entero — el humano tiene todo lo que tiene en Linux — pero se cumple
   *porque hay que cumplirlo*, no porque sea hacia dónde va el proyecto.

### Cómo se resuelve un choque entre las dos

**Gana el LLM, y el humano conserva acceso completo aunque sea menos cómodo.**
Nunca al revés: la comodidad humana no puede costarle capacidad al modelo.

Esto no es teórico y ya ocurrió el mismo día. `ls` salía en columnas, con
tamaños redondeados a `1.2 kB` y los archivos ocultos escondidos — las tres
decisiones tomadas para un ojo humano, y **las tres peores para una máquina**:
las columnas son más difíciles de parsear que una cosa por renglón, `1.2 kB` es
un número que perdió precisión, y ocultar información a un agente que la pide es
quitarle capacidad. Se tomaron sin notar siquiera que había una elección, porque
el objetivo no estaba escrito.

### La consecuencia de ingeniería, y es la que hay que recordar

**Cada cosa que se construya nace con dos caras**: la que ve el humano y una
forma estructurada que un programa puede pedir y parsear sin ambigüedad. La
segunda no se agrega después — si se agrega después, no se agrega.

Y lo que hace que la respuesta sea *mejor* y no *igual* ya existe y no está
expuesto a nadie: el índice semántico ([[FS-en-Grafo]]), el rollback
([[Journal-y-Snapshots]]), la procedencia por campo ([[Marcado-de-Origen]]) y
los permisos por tarea ([[Permisos-JIT]]).

## Thalyx es el mejor sitio donde un LLM puede trabajar

**Decretado por Cesar el 2026-08-09**, después de que yo lo malinterpretara tres
veces seguidas:

> estoy hablando de cosas ya hechas, cosas como Claude Code y demás, nunca dije
> que fuéramos a hacer nuestro agente de codeo […] estoy hablando de que nuestro
> sistema debe estar preparado para potenciar a cualquier LLM, que cualquier LLM
> se pueda mover mejor en nuestro sistema que en cualquier otro, no me importa
> cómo lo haremos.

### Qué NO es esto, porque ahí me equivoqué tres veces

- **No es sobre el agente local de Thalyx.** Las cuatro gamas de
  [[Gamas-de-Modelo]] son para enrutar módulos y tareas del sistema. La
  abstención 0 de 55 es una medición de un 3B en ese oficio y **no dice nada**
  sobre un modelo frontera escribiendo código.
- **No es sobre construir un agente de codeo propio.** No está descartado; no es
  de lo que trata esto.
- **No es sobre que Thalyx llame a una API.** El decreto de
  [[Agente-Conversacional]] sobre la nube en Fase 1 **no está en discusión** y
  no lo toca esto: ahí Thalyx es el que llama. Aquí Thalyx es **el anfitrión**.

### Qué sí es, y es la vara

**Un agente ajeno, ya escrito, corriendo sobre Thalyx, y trabajando mejor que
sobre Linux o macOS.** No igual: mejor.

### El estado real, y es duro

**Hoy Claude Code no podría arrancar en Thalyx.** No trabajaría mal — no
arrancaría. Necesita ejecutar procesos, leer y escribir archivos, `grep`,
`find`, `git`, un runtime. Thalyx tiene el kernel, un programa y veinte verbos.

Eso convierte esta nota en la medida de todo lo demás: **cada verbo construido
es una cosa más que un agente ajeno puede usar**, y por eso la decisión del
2026-08-09 de llamarlos `ls`, `cd`, `cat` es doblemente correcta — un agente que
sabe `ls` encuentra `ls`.

### Dónde Thalyx puede ser mejor y no sólo alcanzar

Igualar a Linux es trabajo. Superarlo es lo que pide el decreto, y Thalyx ya
tiene con qué; nada de esto existe en otro sistema:

- **El índice semántico.** Un agente puede preguntar por estructura en vez de
  hacer `grep` a ciegas. Ver [[FS-en-Grafo]].
- **Journal, snapshots y rollback.** *«Intenta esto y si sale mal deshazlo»* no
  existe en ningún sistema operativo. Ver [[Journal-y-Snapshots]].
- **Procedencia por campo.** El sistema sabe qué vino del humano, qué de la
  máquina y qué de un texto ajeno — que es exactamente la distinción que un
  agente no puede hacer solo. Ver [[Marcado-de-Origen]].
- **Permisos por tarea, mediados.** Un agente puede recibir alcance acotado en
  vez de todo o nada. Ver [[Permisos-JIT]].

Eso es la respuesta a *«mejor que cualquier otro»*, y ninguna de las cuatro está
expuesta todavía a un agente ajeno.

## El LLM es para quien se construye, no un consumidor del sistema

**Decretado por Cesar el 2026-08-09**, después de que yo le planteara como
decisión abierta cómo debía aprender el agente a tocar archivos:

> no hay nada que discutir, no sé ni porqué lo preguntas, la fundación de esto
> es que el LLM la tenga lo más fácil posible, que todo esté hecho para que el
> LLM lo entienda y ejecute mejor.

Eso **no es una preferencia de diseño, es el criterio**. Cuando una decisión
tenga dos formas y una le facilite el trabajo al modelo, ésa es la forma, y no
hace falta preguntarlo.

### Qué corrige exactamente

La frase de arriba —*«a través de la API de Thalyx, no a través de POSIX»*— se
había estado leyendo como si el objetivo fuera **restringir** al agente. No lo
es. Es la misma confusión que hubo con [[Principio-Doble-Ruta]] y la shell:
prohibido es incrustarse en el sistema de alguien más, no tener la capacidad.
Que la superficie sea de Thalyx existe para que cada acción sea **atribuible y
mediada**, no para que el modelo pueda menos.

El ejemplo que Cesar puso es el que zanja el punto: un agente de programación
moderno ejecuta comandos, y por eso sirve.

### La consecuencia práctica, y es medible

Hoy la gramática del agente tiene **una sola operación**, `install_module`. Ese
es el techo real, y no es filosófico: el modelo no puede pedir ver una carpeta
porque no existe la producción que lo diría.

Y hay una segunda razón, más incómoda: **Thalyx casi no tiene qué ejecutar**. El
conocimiento que un LLM ya trae sobre Linux no tiene dónde aterrizar mientras la
máquina tenga veinte verbos. Por eso construir los verbos y hacer capaz al
agente **son el mismo trabajo, no dos**. Y por eso la decisión del 2026-08-09 de
llamarlos `ls`, `cd`, `cat` en vez de sólo `ver`, `ir`, `leer` es también una
decisión sobre el modelo: los primeros los ha visto mil millones de veces, los
segundos hay que enseñárselos en el prompt.

### Lo que esto no cambia

La atribución. **La abstención sale 0 de 55**: el modelo nombra ids que nadie
mencionó, y lo que lo detiene es que el núcleo comprueba cada id contra los
canales del transcript. Facilitarle el trabajo al modelo es el criterio;
quitarle la comprobación al núcleo no es facilitarle nada, es quitarle a Cesar
la última línea que lo protege de un error del modelo. Las dos cosas conviven —
un agente de programación moderno también pide permiso antes de tocar algo.

Ver [[Agente-Conversacional]] y [[Gamas-de-Modelo]].

## El decreto fundacional

*Escrito por Cesar Manzo el 2026-08-03. Es el texto del que nace todo lo demás
de este proyecto, y está aquí **literal**. No se parafrasea, no se resume, no se
"mejora". Cualquier decreto de esta bóveda que lo contradiga está equivocado, sin
importar cuándo se escribió ni con qué confianza.*

> **Thalyx es el sistema operativo.** El kernel de Linux es un componente que
> Thalyx gestiona, no el anfitrión sobre el que descansa. No hay capas
> intermedias, no hay distribuciones, no hay Alpine, no hay busybox, no hay
> glibc — no hay nada que no sea Thalyx.
>
> Los módulos y el agente se comunican **exclusivamente a través de la API de
> Thalyx**, no a través de POSIX, no a través de libc, no a través de scripts de
> shell.
>
> El binario `thalyx` es el init, es estático, es el primer proceso, y todo lo
> demás —el Core, el FS en grafo, los permisos JIT, el scheduler predictivo, la
> memoria persistente, el journal y el agente— vive dentro de ese binario o como
> módulos que se enlazan contra su API.
>
> Linux es el motor que Thalyx elige para hablar con el hardware, pero **Thalyx
> define la interfaz, Thalyx define las reglas, Thalyx define la experiencia.**
>
> Si Linux desaparece, Thalyx encuentra otro motor. Si Thalyx desaparece, no hay
> sistema.
>
> **Thalyx es el todo. Sin Thalyx no hay nada.**

### Cómo se comprueba, no cómo se recita

Este decreto es la razón por la que [[Construccion-del-ISO]] está escrito para
ser **contable**: la imagen lleva el kernel y un programa, se listan los
archivos, y o hay uno o hay más. Un texto fundacional que solo se pudiera citar
sería una consigna; éste se puede contradecir con un número.

```
make -C image count
```

### Qué invalidó, y qué sigue invalidando

Dos decretos anteriores daban por supuesto un userland que ya no existe. **Uno
está resuelto; el otro sigue abierto.**

1. ~~**`CLAUDE.md` prefiere invocar `bpftool` a enlazar una librería.**~~
   **Resuelto el 2026-08-03, y sin ejercer todavía.** La regla era buena —sin
   dependencia de headers del kernel, cada paso reproducible a mano— y en la
   imagen era imposible. La respuesta no fue enlazar una librería: Thalyx lee el
   objeto BPF y hace las llamadas al kernel él mismo, y el objeto viaja dentro
   del binario en vez de junto a él. Ver [[Cargador-BPF-Propio]]. La regla de
   `CLAUDE.md` sigue valiendo para `btrfs`, que sí existe donde se usa.
2. **[[Gamas-de-Modelo]] decreta `llama.cpp` invocado como proceso.** Eso es un
   segundo programa. La respuesta probablemente sea que el modelo del agente es
   **un módulo**, enlazado contra la API como cualquier otro, que es justo lo que
   este decreto describe — pero eso lo decide Cesar, no se deduce aquí.
3. **Los módulos son binarios de Linux, y hoy hablan POSIX.** Abierto desde el
   2026-08-04, encontrado por una auditoría externa. El decreto dice que los
   módulos se comunican *exclusivamente* por la API de Thalyx, no por POSIX y no
   por libc; el sandbox monta `/usr`, `/lib`, `/lib64`, `/bin`, `/sbin` y `/etc`
   de sólo lectura, y el filtro seccomp permite alrededor de ciento veinte
   llamadas al sistema. El propio `rootfs.rs` ya lo admitía en su
   documentación —*no es una frontera de seguridad, es casi una distribución*—
   y el README no. **La distinción que faltaba está escrita en
   [[Sistema-de-Modulos]]**: la API de Thalyx es la única superficie *mediada*,
   que no es lo mismo que la única superficie alcanzable. Cerrar la brecha del
   todo es una decisión de Fase 2 sobre cómo se construyen los módulos —
   estáticos, sin libc, con un rootfs sin `/usr` — y está en
   [[Tareas-Pendientes]].

## Declaración central

> "La IA no debe ser una aplicación más dentro del sistema operativo. Debe ser el mecanismo principal mediante el cual el usuario interactúa con la máquina."

## La idea original

Un sistema operativo de código abierto donde la inteligencia artificial no es una aplicación más, sino el mecanismo principal de interacción y gestión del sistema. El SO está diseñado desde el núcleo hacia afuera para que la IA sea "ciudadana de primera clase", no un invitado.

## Principios rectores

1. **La IA es ciudadana de primera clase.** No es un "asistente" opcional. Es el pegamento que une todas las capas del sistema.
2. **El ser humano sigue siendo el soberano.** La IA ejecuta, pero el humano manda. La IA es una extensión de la voluntad del usuario, no un sustituto. Ver [[Principio-Doble-Ruta]].
3. **Arquitectura asimétrica.** El sistema tiene dos caras. Una para el humano (interfaz gráfica tradicional, archivos jerárquicos, permisos estáticos) y otra para la IA (API semántica, sistema de archivos en grafo, permisos just-in-time, scheduler predictivo, memoria persistente). Ver [[Arquitectura-Asimetrica]].
4. **La IA no es un LLM genérico.** El agente es especializado en la API interna del sistema, la documentación, los módulos, los permisos y las políticas. No es un ChatGPT con esteroides; es un traductor de intención que convierte lenguaje natural en acciones de sistema. Ver [[Agente-Conversacional]].
5. **El sistema no es un producto.** Es código abierto: GPLv3 en userspace, GPLv2 en los componentes de kernel — ver [[Decision-Licencia]]. No se vende. El modelo de negocio (si llegara a necesitarse) serían servicios, soporte, formación o módulos premium, pero el núcleo y el agente base siempre serán gratuitos.
6. **El sistema no compite en el escritorio tradicional.** No busca reemplazar Windows en gaming o Adobe. Su nicho inicial son desarrolladores, investigadores y power users que valoran la eficiencia por encima del ocio. Estrategia similar a "Linux primero en servidores".

## El "por qué" profundo

El sistema actual (Windows, Linux, macOS) fue diseñado en una era donde los humanos eran los únicos usuarios. La interfaz, los permisos, el sistema de archivos y el scheduler están pensados para un operador humano. Una IA que opera en estos sistemas tiene que "simular" ser un humano, usando teclado/mouse o APIs que son un calco de la interacción humana. Esto crea fricción y limita lo que la IA puede hacer con soltura.

Este sistema revierte esa relación: en lugar de que la IA se adapte al SO, el SO se adapta a la IA. Las primitivas del sistema (permisos, scheduler, sistema de archivos) están diseñadas para que la IA las use de forma nativa, no emulada. El humano sigue viendo una interfaz tradicional, pero por debajo, la IA tiene acceso a un mundo de operaciones que a un humano le serían inútiles o confusas, pero que para ella son naturales.

## Revisiones

### 2026-08-01 — Nombre y licencia
**Antes:** el sistema no tenía nombre propio y la licencia era "tipo GPL".
**Ahora:** el sistema se llama Thalyx; la licencia es GPLv3 en userspace y GPLv2 en componentes de kernel.
**Motivo:** "tipo GPL" no es una licencia, y GPLv3 es incompatible con el kernel Linux, que es GPLv2 únicamente. Ver [[Decision-Licencia]].

## Relacionado
- [[Decision-Capa-vs-SO-Nuevo]] — por qué esto no puede ser una capa sobre Linux existente
- [[Nomenclatura-y-Convenciones]]
- [[Decision-Licencia]]
- [[Principio-Doble-Ruta]]
- [[00-Indice/Indice-Principal|Índice principal]]
