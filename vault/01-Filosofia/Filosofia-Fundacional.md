---
tipo: filosofia
estado: decretado
fecha-decreto: 2026-07-31
tags: [filosofia, core, no-negociable]
---

# Filosofía fundacional

El sistema se llama **Thalyx**. Ver [[Nomenclatura-y-Convenciones]].

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
