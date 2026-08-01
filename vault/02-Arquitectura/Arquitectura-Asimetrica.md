---
tipo: arquitectura
estado: decretado
fecha-decreto: 2026-07-31
tags: [arquitectura, core]
---

# Arquitectura asimétrica

El sistema tiene dos caras que coexisten sobre la misma realidad física.

## Cara Humana (interfaz tradicional)

- Archivos jerárquicos (carpetas y subcarpetas).
- Interfaz gráfica con ventanas, iconos, clics.
- Permisos estáticos (lectura/escritura/ejecución).
- Notificaciones por evento.

## Cara IA (API nativa)

- **Sistema de archivos en grafo (semántico):** los archivos son nodos con etiquetas y relaciones. La IA puede hacer consultas tipo: "dame todos los archivos que dependen del módulo de autenticación". Ver [[FS-en-Grafo]].
- **Permisos just-in-time:** la IA pide acceso temporal a recursos. El SO otorga y revoca automáticamente. Ver [[Permisos-JIT]].
- **Scheduler predictivo por contexto:** la IA puede decir "esta tarea es crítica para el usuario ahora, dale el 80% de la CPU por 5 segundos". Ver [[Scheduler-Predictivo]].
- **Memoria persistente de trabajo:** la IA guarda "estado" de una tarea y lo recupera exactamente donde lo dejó, incluso después de apagar el equipo. Ver [[Memoria-Persistente]].
- **Notificaciones contextuales:** la IA decide cuándo y cómo interrumpir al humano, basado en el estado actual del usuario.

## Relacionado
- [[Filosofia-Fundacional]]
- [[Core-Nucleo]]
- [[Decision-Kernel-vs-Userspace]]
