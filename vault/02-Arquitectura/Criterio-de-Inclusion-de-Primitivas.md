---
tipo: principio
estado: decretado
fecha-decreto: 2026-07-31
tags: [arquitectura, metodologia, criterio]
---

# Criterio de inclusión de primitivas

## El decreto (criterio metodológico central)

**Una primitiva o pieza arquitectónica entra a la arquitectura base *ahora* únicamente si omitirla hoy implicaría una reescritura dolorosa después** (es decir, si el resto del sistema ya asumiría su ausencia y sería costoso insertarla retroactivamente).

Si no cumple esa condición, se documenta como **"primitiva futura"** y se construye solo cuando el sistema ya tenga el problema concreto que la justifica.

## Por qué existe este criterio

Se aplicó explícitamente para evaluar candidatos a "quinta/sexta primitiva" del sistema, más allá de las [[Primitivas-Base-Overview|4 primitivas base]]. Se decretó explícitamente que **no meter estas primitivas ahora no es "sobre-ingeniería evitada por pereza", sino la aplicación consistente del mismo criterio** que ya se había usado para posponer el [[Sistema-Reputacion-Sybil|problema de Sybil attacks]] en el repositorio comunitario: un problema real que aparece con la escala no se resuelve antes de tener la escala.

Referencia comparativa usada en la discusión: Linux (kernel de Torvalds, 1991) no nació con namespaces, cgroups, SELinux, ni soporte multi-usuario robusto. Nació con lo mínimo para tener un proceso corriendo. Todo lo que hoy se considera "esencial" llegó 10-20 años después, cuando hubo problemas reales que lo justificaron.

## Primitivas futuras identificadas (documentadas, NO construidas)

### Grafo causal de eventos
Una estructura relacional que registre "el proceso X modificó Y, lo cual disparó Z, que llamó a W, que falló porque..." — pensada para **diagnóstico y razonamiento sobre fallos**, distinta del [[Journal-y-Snapshots|journal]] que sirve para revertir.

Se justifica solo cuando el sistema tenga suficientes piezas interactuando como para que un log plano ya no alcance para diagnosticar fallos. Conecta directamente con el área de [[Interpretabilidad-Mecanicista]].

### Intención declarada persistente
Un espacio donde el usuario declara objetivos de largo plazo (ej. "nunca toques mi carpeta de fotos sin preguntar dos veces"), distinto de permisos y de memoria de tareas.

Bajo riesgo de reescritura — se puede añadir después sin fricción, casi se comporta como configuración.

### Grafo de procesos en runtime
Dependencias entre procesos vivos, no solo archivos (a diferencia del [[FS-en-Grafo]], que modela dependencias de código/archivos).

Condicionada a que el [[Scheduler-Predictivo]] ya tenga un consumidor real para esa información.

## Relacionado
- [[Primitivas-Base-Overview]]
- [[Decision-Kernel-vs-Userspace]]
- [[Sistema-Reputacion-Sybil]]
