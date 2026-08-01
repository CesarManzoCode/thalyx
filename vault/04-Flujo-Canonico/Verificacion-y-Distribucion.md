---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-01
tags: [flujo, verificacion, firma, modulos, seguridad]
---

# Verificación y distribución de módulos

## El hueco que resuelve

El trazado original decía, en el paso 10, que el Sandbox ejecutaba el script de instalación del módulo y devolvía un `hash_verificacion`; y en el paso 11, que el Core "verificaba firma, hash e integridad".

Ese paso 11 no podía funcionar. El artefacto lo acababa de producir un script arbitrario dentro del sandbox: **no existía ningún hash esperado contra el cual compararlo**. La "verificación" no tenía referente. Y el hash que se iba a comparar lo reportaba el propio Sandbox, que por el [[Modelo-de-Amenaza|modelo de amenaza]] está fuera de la TCB.

## Decreto: instalar no es ejecutar

En Fase 1, los módulos se distribuyen **exclusivamente como artefactos prebuildeados y firmados** por su publicador (`distribution = "prebuilt"` en el [[Formato-Manifiesto-Thmod|manifiesto]]).

- **La instalación no ejecuta código del módulo.** `thalyx-sandbox` desempaqueta el artefacto y valida su estructura: que los archivos estén dentro de las rutas declaradas y que no haya enlaces ni rutas que escapen del árbol.
- **`thalyx-core` verifica** la firma del manifiesto, el hash del artefacto **recalculado por él mismo**, y la coherencia del manifiesto.
- **Solo entonces publica**, mediante el commit atómico de [[Fase-Commit-Atomico]].

El sandbox sigue siendo esencial, pero para cuando el módulo **corre**, no para instalarlo. Son dos momentos distintos con dos superficies de riesgo distintas, y el diseño original los trataba como uno solo.

## Consecuencia: la verificación pasa a ser real

Con artefactos prebuildeados, "el Core verifica" es una afirmación criptográficamente comprobable: existe un hash esperado, publicado y firmado por alguien identificable, y el Core lo recalcula por su cuenta.

Con build local, lo máximo alcanzable habría sido verificación **estructural** — comprobar que el script escribió solo donde dijo que iba a escribir — que es una garantía mucho más débil y que no detecta un artefacto adulterado, solo uno desprolijo.

## Qué se pospone

La distribución desde fuente (`distribution = "source"`), con su verificación estructural y eventualmente builds reproducibles, se decreta cuando exista demanda real: un módulo concreto que necesite compilar contra el equipo del usuario. En Fase 1 no hay ecosistema, así que la restricción no le cuesta nada a nadie.

## Regla general derivada

**El Core verifica lo que recibe, nunca lo que le reportan.** Ningún componente fuera de la TCB puede aportar el resultado de su propia verificación.

## Relacionado
- [[Formato-Manifiesto-Thmod]]
- [[Fase-Commit-Atomico]]
- [[Sandbox-Ejecucion]]
- [[Tres-Categorias-de-Autorizacion]]
- [[Modelo-de-Amenaza]]
- [[Caso-Instalar-Modulo]]
