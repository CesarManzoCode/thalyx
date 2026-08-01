---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-01
tags: [nomenclatura, identidad, convenciones, no-negociable]
---

# Nomenclatura y convenciones

## Decreto: el sistema se llama Thalyx

El sistema operativo se llama **Thalyx**. Hasta el 1 de agosto de 2026 la bóveda se refería a él como "el SO", sin nombre propio.

## Nomenclatura de componentes

| Componente | Nombre | Rol |
|---|---|---|
| Orquestador central | `thalyx-core` | Valida contratos, resuelve versiones, verifica artefactos, hace commit, escribe el journal. Ver [[Core]] |
| Agente | `thalyx-agent` | Traductor de intención. Ver [[Agente-Conversacional]] |
| Broker de permisos | `thalyx-permd` | Otorga y revoca permisos. Ver [[Permisos-JIT]] |
| Aislamiento de ejecución | `thalyx-sandbox` | Ejecuta código de módulos. Ver [[Sandbox-Ejecucion]] |
| Módulo de seguridad del kernel | `thalyx-lsm` | Aplica permisos e intercepta mutaciones del filesystem |
| Interfaz de línea de comandos | `thalyx` | Punto de entrada único para el humano |

## Convenciones

- **Extensión de módulo:** `.thmod` (reemplaza a `.osmod`).
- **Identificador de módulo:** DNS inverso e inmutable, por ejemplo `org.publisher.pyassist`. El identificador nunca cambia durante la vida del módulo; el nombre visible sí puede cambiar.
- **CLI:** un solo binario `thalyx` con subcomandos, no una familia de binarios sueltos. Ejemplos: `thalyx module install`, `thalyx graph build`, `thalyx rollback`, `thalyx restore`.

## Decreto: política de idioma

- **Inglés:** todo el código, los schemas, los identificadores, los mensajes de commit, los nombres de archivo del código, la salida de la CLI y los mensajes de error.
- **Español:** la bóveda, hasta que un decreto posterior ordene su traducción.

No se mezclan idiomas dentro de un mismo artefacto. Un archivo de código en inglés no lleva comentarios en español, y una nota de la bóveda no lleva prosa en inglés fuera de los bloques de código.

## Razón

Los nombres provisionales (`.osmod`, `os-assistant`) no son marca ni son buscables: "os module" devuelve ruido de cualquier sistema operativo. Cambiarlos el día que se decreta cuesta cero; cambiarlos en Fase 2 significa tocar documentación publicada, imágenes ISO distribuidas y módulos de terceros ya escritos.

## Relacionado
- [[Filosofia-Fundacional]]
- [[Formato-Manifiesto-Thmod]]
- [[Decision-Licencia]]
