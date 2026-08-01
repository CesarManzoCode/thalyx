---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-07-31
tags: [flujo, autorizacion, seguridad]
---

# Tres categorías de autorización

Se identificó que el flujo canónico ya distinguía, sin nombrarlo explícitamente, tres tipos de autorización distintos. Esta nota los formaliza.

## 1. Autorización operacional

¿El usuario quiere que esto se ejecute?

> Ejemplo: "¿Instalo pyassist-core?"

## 2. Autorización de capacidades

¿El usuario acepta los permisos específicos que la acción requiere?

> Ejemplo: "¿Aceptas acceso a red y a tu carpeta de proyectos?"

Se relaciona directamente con [[Tres-Tipos-de-Permiso]] — especialmente los permisos de tipo `persistente`, que requieren esta autorización explícita sin excepciones.

## 3. Autorización de publicación

¿El resultado de la ejecución se incorpora oficialmente al sistema?

Esta es la categoría **más nueva** y se desprende directamente de la arquitectura [[Fase-Commit-Atomico|build-then-commit]]: aunque el Sandbox termine sin errores, el Core puede rechazar la publicación si detecta firma inválida, hash distinto al esperado, o una dependencia rota.

### La frontera clave

Existe una frontera explícita entre **"ejecutado"** e **"instalado oficialmente"**. El éxito de la ejecución del Sandbox no implica automáticamente un cambio permanente del sistema — eso solo ocurre en el commit, después de la verificación del Core.

## Por qué importa esta distinción

Evita que el éxito de la ejecución implique automáticamente un cambio permanente del sistema. Es la base conceptual de por qué el flujo tiene una etapa de verificación explícita antes del commit, separada de la ejecución en sí.

## Relacionado
- [[Fase-Commit-Atomico]]
- [[Tres-Tipos-de-Permiso]]
- [[Flujo-Canonico-Overview]]
