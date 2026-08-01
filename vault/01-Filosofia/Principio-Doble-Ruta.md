---
tipo: principio
estado: decretado
fecha-decreto: 2026-07-31
tags: [filosofia, no-negociable, casos-de-uso]
---

# Principio de doble ruta

## Decreto

**Todo lo que el agente puede hacer, el humano puede hacerlo directamente, sin pasar por el agente, y sin degradación de capacidad.**

El agente es un acelerador y traductor de intención, nunca un intermediario obligatorio.

## Razones

- **Independencia de fallo:** si el agente crashea o el usuario no confía en él, el sistema sigue siendo 100% operable.
- **Eficiencia:** hay tareas triviales donde pasar por lenguaje natural es más lento que un comando directo.

## Seis capas de casos de uso para garantizar esto

1. **Operaciones de archivo básicas** — cara humana pura: crear, mover, copiar, borrar vía gestor gráfico o terminal POSIX estándar, sin tocar el agente.
2. **Gestión de módulos** — dual: instalar/desinstalar manualmente con `thalyx module install <archivo>` o vía agente.
3. **Permisos y seguridad** — el humano puede otorgar/revocar permisos permanentes manualmente, sin necesitar el mecanismo JIT del agente; puede ver el log de auditoría sin preguntarle al agente.
4. **Scheduling y recursos** — herramientas tipo `htop`/`nice`/`renice` siguen funcionando nativamente; el scheduler predictivo del agente es una capa opcional encima, no un reemplazo.
5. **Estado y memoria de tareas** — el humano puede ver, editar o borrar el estado guardado por el agente en texto plano/JSON, sin depender de preguntarle.
6. **Uso del agente en sí** — la ruta mediada, en lenguaje natural, para velocidad o cuando no se conoce el comando exacto.

## Relacionado
- [[Filosofia-Fundacional]]
- [[Flujo-Canonico-Overview]]
