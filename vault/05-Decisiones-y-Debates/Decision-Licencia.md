---
tipo: decision
estado: decretado
fecha-decreto: 2026-08-01
tags: [licencia, legal, kernel, no-negociable]
---

# Licencia

## Problema

La [[Filosofia-Fundacional]] decretaba "licencia tipo GPL", que no es una licencia. Y existe una restricción técnica que la bóveda no registraba y que hace que la elección obvia sea incorrecta.

## La restricción que fuerza la decisión

El kernel Linux se distribuye bajo **GPLv2 únicamente**, sin la cláusula "or later". GPLv3 y GPLv2 son licencias **incompatibles entre sí**. Un módulo de kernel licenciado bajo GPLv3 no puede enlazarse ni distribuirse junto con Linux.

Thalyx incluye `thalyx-lsm`, un módulo de seguridad del kernel, desde la Fase 1 (ver [[Permisos-JIT]]). Si el proyecto entero se licenciara bajo GPLv3, esa pieza sería legalmente indistribuible.

## Decreto

- **Userspace:** GPLv3.
- **Todo componente que se compile como módulo del kernel Linux o enlace con él:** GPLv2 explícito, indicado en el encabezado del archivo y en el macro `MODULE_LICENSE`.

Todo contribuyente externo acepta esta doble condición antes de que se acepte su código. Se documenta en el `CONTRIBUTING.md` del repositorio desde el primer día.

## Alternativas descartadas

- **GPLv3 para todo:** deja `thalyx-lsm` sin poder distribuirse. El problema no aparece hasta que se escribe el módulo, y para entonces relicenciar con contribuyentes externos ya presentes es prácticamente imposible: haría falta el consentimiento de cada uno.
- **AGPLv3:** su cláusula distintiva se activa cuando el software se ofrece como servicio por red. Thalyx *llama* a servicios remotos, no *sirve* por red, así que el disparador casi nunca aplica. A cambio, aleja contribuyentes corporativos y arrastra la misma incompatibilidad con el kernel.

## Por qué se decreta ahora y no después

Es la clase de decisión que solo es barata mientras el único titular de derechos de autor eres tú. Cada contribuyente externo que acepta un pull request se convierte en cotitular, y a partir de ahí la licencia deja de ser reversible en la práctica.

## Relacionado
- [[Filosofia-Fundacional]]
- [[Permisos-JIT]]
- [[Nomenclatura-y-Convenciones]]
