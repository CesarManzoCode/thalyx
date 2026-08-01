---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-01
tags: [seguridad, amenazas, tcb, no-negociable]
---

# Modelo de amenaza

Toda la arquitectura de seguridad de Thalyx — sandbox, permisos JIT, firma de módulos, build-then-commit — existía sin declarar contra quién defiende. Esta nota lo declara.

## Base de cómputo confiable (TCB)

La TCB es el conjunto de componentes cuyo compromiso compromete todo el sistema. Si un componente está en la TCB, no hay mecanismo dentro de Thalyx que pueda contenerlo.

**Dentro de la TCB:**
- El kernel Linux y `thalyx-lsm`.
- `thalyx-core`.
- `thalyx-permd`.

**Fuera de la TCB:**
- **`thalyx-agent`.** El agente no es confiable. Puede alucinar, puede ser manipulado por contenido que lee, y el Core revalida todo lo que produce.
- **`thalyx-sandbox`.** El sandbox *contiene*, pero no se le cree: sus reportes de resultado se verifican, no se aceptan.
- Todos los módulos, sin excepción, incluidos los Core Modules.

Mantener la TCB pequeña es un objetivo de diseño explícito. Toda propuesta que agregue un componente a la TCB debe justificar por qué no puede vivir fuera.

## Adversarios, en orden de prioridad

### 1. El agente engañado o inyectado

**Es el adversario prioritario.** El agente lee contenido que no controla —descripciones de módulos, reseñas, archivos del usuario, metadatos de red— y produce contratos que cambian el estado del sistema. Un atacante que consigue influir en ese texto influye en lo que Thalyx ejecuta.

*Por qué va primero:* es el problema que Thalyx **crea al existir**. Los otros tres tienen décadas de arte previo del cual tomar prestado; este no. Si el modelo de amenaza no lo prioriza, el diseño va a seguir optimizando contra adversarios que ya se sabe vencer.

*Mitigaciones decretadas:* [[Marcado-de-Origen]], [[Camino-Confiable]], validación independiente del contrato por el Core, y la exclusión del agente de la TCB.

### 2. El módulo malicioso

Código de terceros que intenta salir de su aislamiento, acceder a recursos que no declaró, o persistir más allá de lo autorizado.

*Mitigaciones decretadas:* [[Sandbox-Ejecucion]], [[Permisos-JIT]], la regla de que los permisos efectivos son los del manifiesto ([[Formato-Manifiesto-Thmod]]), y [[Verificacion-y-Distribucion]].

### 3. El repositorio comunitario comprometido

Artefactos alterados, manifiestos manipulados, o suplantación de un publicador conocido.

*Mitigaciones decretadas:* firma detached por publicador, anclaje de clave al identificador del módulo con confianza al primer uso, y verificación criptográfica antes del commit. Ver [[Verificacion-y-Distribucion]].

### 4. El usuario engañado por presentación falsa

El usuario autoriza algo distinto de lo que cree estar autorizando, porque el texto que leyó no describía la acción real.

*Mitigación decretada:* [[Camino-Confiable]].

## Supuestos declarados

- **El usuario es honesto pero engañable.** Toda decisión de seguridad que dependa de que el usuario lea un texto debe asumir que ese texto pudo haber sido redactado por el adversario, salvo que provenga del camino confiable.
- **El agente puede equivocarse siempre.** Thalyx no intenta hacer confiable al agente; lo contiene por diseño. Ningún mecanismo de seguridad puede depender de que el agente se comporte bien.
- **La verificación se hace sobre lo que se recibe, no sobre lo que se reporta.** El Core recalcula los hashes; no acepta los que le devuelve un componente fuera de la TCB.

## Fuera de alcance (no-objetivos declarados)

Thalyx **no** promete defender contra:

- Un atacante que ya posee root local. Si el adversario controla la TCB, no hay garantías.
- Un atacante con acceso físico al equipo.
- Canales laterales (temporización, caché, consumo energético).
- La integridad del modelo de lenguaje en sí. Thalyx no verifica pesos ni detecta puertas traseras dentro del modelo: asume que el agente puede fallar y lo contiene.
- Cambios hechos al disco con Thalyx apagado —arrancar otro sistema, montar el disco en otra máquina— ver el tratamiento en [[Coherencia-Doble-Ruta]].

Declarar los no-objetivos es parte del modelo: una promesa de seguridad sin límites explícitos no es verificable.

## Relacionado
- [[Camino-Confiable]]
- [[Marcado-de-Origen]]
- [[Verificacion-y-Distribucion]]
- [[Sandbox-Ejecucion]]
- [[Permisos-JIT]]
- [[Interpretabilidad-Mecanicista]]
