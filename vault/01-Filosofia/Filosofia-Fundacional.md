---
tipo: filosofia
estado: decretado
fecha-decreto: 2026-07-31
tags: [filosofia, core, no-negociable]
---

# Filosofía fundacional

## Declaración central

> "La IA no debe ser una aplicación más dentro del sistema operativo. Debe ser el mecanismo principal mediante el cual el usuario interactúa con la máquina."

## La idea original

Un sistema operativo de código abierto donde la inteligencia artificial no es una aplicación más, sino el mecanismo principal de interacción y gestión del sistema. El SO está diseñado desde el núcleo hacia afuera para que la IA sea "ciudadana de primera clase", no un invitado.

## Principios rectores

1. **La IA es ciudadana de primera clase.** No es un "asistente" opcional. Es el pegamento que une todas las capas del sistema.
2. **El ser humano sigue siendo el soberano.** La IA ejecuta, pero el humano manda. La IA es una extensión de la voluntad del usuario, no un sustituto. Ver [[Principio-Doble-Ruta]].
3. **Arquitectura asimétrica.** El sistema tiene dos caras. Una para el humano (interfaz gráfica tradicional, archivos jerárquicos, permisos estáticos) y otra para la IA (API semántica, sistema de archivos en grafo, permisos just-in-time, scheduler predictivo, memoria persistente). Ver [[Arquitectura-Asimetrica]].
4. **La IA no es un LLM genérico.** El agente es especializado en la API interna del sistema, la documentación, los módulos, los permisos y las políticas. No es un ChatGPT con esteroides; es un traductor de intención que convierte lenguaje natural en acciones de sistema. Ver [[Agente-Conversacional]].
5. **El sistema no es un producto.** Es código abierto (licencia tipo GPL). No se vende. El modelo de negocio (si llegara a necesitarse) serían servicios, soporte, formación o módulos premium, pero el núcleo y el agente base siempre serán gratuitos.
6. **El sistema no compite en el escritorio tradicional.** No busca reemplazar Windows en gaming o Adobe. Su nicho inicial son desarrolladores, investigadores y power users que valoran la eficiencia por encima del ocio. Estrategia similar a "Linux primero en servidores".

## El "por qué" profundo

El sistema actual (Windows, Linux, macOS) fue diseñado en una era donde los humanos eran los únicos usuarios. La interfaz, los permisos, el sistema de archivos y el scheduler están pensados para un operador humano. Una IA que opera en estos sistemas tiene que "simular" ser un humano, usando teclado/mouse o APIs que son un calco de la interacción humana. Esto crea fricción y limita lo que la IA puede hacer con soltura.

Este sistema revierte esa relación: en lugar de que la IA se adapte al SO, el SO se adapta a la IA. Las primitivas del sistema (permisos, scheduler, sistema de archivos) están diseñadas para que la IA las use de forma nativa, no emulada. El humano sigue viendo una interfaz tradicional, pero por debajo, la IA tiene acceso a un mundo de operaciones que a un humano le serían inútiles o confusas, pero que para ella son naturales.

## Relacionado
- [[Decision-Capa-vs-SO-Nuevo]] — por qué esto no puede ser una capa sobre Linux existente
- [[Principio-Doble-Ruta]]
- [[00-Indice/Indice-Principal|Índice principal]]
