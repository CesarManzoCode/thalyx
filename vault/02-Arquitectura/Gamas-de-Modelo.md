---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-03
tags: [agente, modelo, gamas, inferencia, fase-1]
---

# Las gamas de modelo del agente

Este es el decreto que [[Tareas-Pendientes]] llevaba señalando como **el único
que bloqueaba** el agente, y que [[Punto-Actual]] llamaba el riesgo técnico más
grande del proyecto.

## Decreto

**El agente no ancla un modelo. Ofrece cuatro gamas de una sola familia, y el
usuario elige según su hardware.**

| Gama | Modelo | Tamaño en disco | RAM que pide |
|---|---|---|---|
| Ligera | `Qwen2.5-1.5B-Instruct` Q4_K_M | ~1.1 GB | 4 GB |
| Media | `Qwen2.5-3B-Instruct` Q4_K_M | ~2.0 GB | 8 GB |
| Alta | `Qwen2.5-7B-Instruct` Q4_K_M | ~4.7 GB | 16 GB |
| Máxima | `Qwen2.5-14B-Instruct` Q4_K_M | ~9.0 GB | 32 GB |

Los tamaños son aproximados hasta que el banco los mida; la tabla se corrige
con las cifras reales, no se deja con las estimadas.

La inferencia corre **invocando `llama.cpp` como proceso**, no enlazándolo.

## Por qué cuatro gamas y no un modelo

Porque anclar uno rompe el [[Criterio-de-Salida-Fase-1|criterio de salida]].

Ese criterio dice que la Fase 1 no termina hasta que **una persona ajena al
proyecto** use el sistema. Un modelo de 5 GB obliga a esa persona a tener 16 GB
de RAM antes de poder *intentarlo*. Un requisito de hardware que excluye las
máquinas normales no es un detalle de rendimiento: convierte el criterio de
salida en algo que no se puede cumplir, porque reduce a casi nada el conjunto
de gente que puede aceptar la invitación.

La gama también es honesta con lo que el proyecto no sabe todavía. No hay
evidencia de qué tamaño hace falta para traducir intención de forma confiable
—[[Debate-Agente-Fine-Tuning]] dice que es un problema de investigación
abierto, no de ingeniería resuelta—. Anclar un tamaño sería fingir que sí la
hay. Cuatro gamas y un banco que las mida es la forma de averiguarlo en vez de
suponerlo.

## Por qué una sola familia y no la mejor de cada tamaño

Porque si cada gama viene de una familia distinta, el banco deja de medir lo
que dice medir.

Modelos de familias distintas responden distinto al mismo prompt. Un resultado
mejor en la gama alta podría ser el modelo, o podría ser que el prompt le queda
mejor a esa familia. **Con una sola familia, el prompt y la gramática son
idénticos en las cuatro gamas, así que lo único que varía es el tamaño** — y
entonces la diferencia medida es atribuible a él.

El costo secundario también importa: cuatro familias son cuatro prompts que
mantener, y cuatro prompts se desincronizan. Uno solo no puede.

## Por qué `llama.cpp` como proceso y no como librería

Por el mismo motivo por el que `CLAUDE.md` prefiere invocar `bpftool` a
enlazar una librería de BPF:

- **Sin dependencia de build.** El workspace de Rust no necesita un toolchain
  de C++ ni headers para compilar. Alguien que solo quiere la CLI no paga el
  costo de una pieza que no va a usar.
- **Cada paso se reproduce a mano.** Cuando el agente produzca algo raro, la
  misma inferencia se puede repetir en una terminal con el mismo prompt, la
  misma gramática y la misma semilla, sin Thalyx de por medio. Un fallo que
  solo se puede observar desde dentro del proceso que lo causó es un fallo que
  cuesta el doble de encontrar.
- **El modelo queda fuera del proceso de Thalyx.** El agente está fuera de la
  TCB por [[Modelo-de-Amenaza]]; tenerlo además en otro proceso hace que esa
  frontera sea de sistema operativo y no solo de diseño.

## La gramática, y qué garantiza de verdad

Toda inferencia corre con **decodificación restringida por gramática** (GBNF).
El modelo no elige entre tokens válidos e inválidos: los inválidos no existen
en su distribución.

Consecuencia, y es la que hace defendible ofrecer una gama de 1.5B:

> **Un contrato mal formado es imposible en las cuatro gamas.** Lo que cambia
> entre gamas no es la seguridad, es el acierto al interpretar la intención.

Un modelo de 1.5B puede entender mal lo que el usuario quiso y proponer instalar
el módulo equivocado. Lo que no puede es emitir un contrato con un campo
inventado, un tipo cambiado o un permiso que el schema no contempla. Por eso la
elección de gama es una decisión legítima del usuario sobre **calidad**, y no
una decisión sobre **seguridad** que el sistema le esté delegando sin decírselo.

### Lo que la gramática no garantiza, y que por eso no se le encarga

Una gramática obliga a la forma, no a la verdad. Si el contrato tuviera un campo
`origen` y lo llenara el modelo, la gramática garantizaría que ese campo existe
y que dice uno de los valores permitidos — **y nada más**. Un modelo que
procesó una página web hostil podría emitir `origen: usuario` con la forma
perfecta, y todo [[Marcado-de-Origen]] se caería en silencio, con el schema
contento.

Por eso, decretado aquí:

> **El modelo nunca escribe la procedencia.** La gramática del contrato no
> incluye los campos de origen. Los pone el ensamblador, desde el canal por el
> que entró cada texto, que es algo que él sabe y el modelo no puede saber.

Es la misma forma que el resto del sistema: el núcleo recalcula el hash en vez
de aceptar el que le reporten, y genera él las solicitudes de autorización en
vez de dejar que se las compongan. Un dato con efecto sobre la autorización no
se le pide a la parte de la que se desconfía.

## Qué tiene que medir el banco

Un enunciado en español entra; se compara el contrato producido contra el
esperado. Por gama:

1. **Acierto de intención** — ¿eligió la acción correcta?
2. **Acierto de argumentos** — ¿el id del módulo, la versión, las rutas?
3. **Abstención** — ante un enunciado ambiguo, ¿pide aclaración o inventa? Esta
   importa más que las dos anteriores: un agente que se equivoca pidiendo
   confirmación cuesta un segundo, y uno que inventa con confianza cuesta el
   camino confiable entero.
4. **Latencia** hasta el contrato completo, en hardware descrito.
5. **RAM residente real**, no la del archivo.

Y lo que el banco **no** puede concluir: que una gama es segura y otra no. Todas
producen contratos válidos por construcción, y todas quedan igual de sujetas al
camino confiable y a la validación del núcleo. El banco mide utilidad.

### Lo que hay que documentarle al usuario

Que elija bien exige decirle, por gama, **qué se le va a dar mal**, no solo qué
tan buena es. Un porcentaje agregado no ayuda a nadie a decidir. La forma útil
es: "en la gama ligera, los enunciados con dos acciones encadenadas fallan la
mitad de las veces; en la media, casi nunca". Eso es accionable. "72%" no.

## Lo que este decreto no decide

- **El fine-tuning sigue fuera de Fase 1.** [[Debate-Agente-Fine-Tuning]] no
  cambia: se empieza con reglas escritas a mano y prompting.
- **Los modelos remotos siguen sin existir en Fase 1.** Ver
  [[Agente-Conversacional]].
- **Qué gama trae el ISO por defecto.** Depende del banco: la que sirva en la
  máquina más chica que valga la pena soportar.
- **Si Qwen2.5 es la familia correcta.** Es la hipótesis de partida, elegida por
  cubrir los cuatro tamaños con instruct y GGUF. Si el banco muestra que otra
  familia la supera *en las cuatro gamas a la vez*, se cambia entera — nunca
  una gama suelta, porque eso rompería la comparabilidad que motiva el decreto.

## Relacionado
- [[Agente-Conversacional]] — qué es el agente y qué no puede hacer
- [[Debate-Agente-Fine-Tuning]] — por qué el fine-tuning no es de Fase 1
- [[Marcado-de-Origen]] — la defensa que la gramática no puede dar
- [[Modelo-de-Amenaza]] — por qué el agente está fuera de la TCB
- [[Criterio-de-Salida-Fase-1]] — el criterio que anclar un modelo rompería
- [[Agente-Minimo]] — el primer agente que se construye con esto
