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

## Revisión del 2026-08-03 — lo que corre antes de la firma también es superficie

"Instalar no ejecuta código" cerraba la ejecución. No cerraba el **agotamiento
de recursos**, y ahí había un agujero real, medido:

Un `.thmod` de 768 MB **sin firma alguna** llevaba la memoria del proceso a un
gigabyte. La causa no era el artefacto: era que `Bundle::read` leía cada miembro
entero a memoria *antes* del `match` que decide si el miembro es siquiera uno de
los tres que importan. Rellenar el archivo con un miembro que Thalyx **ignora**
bastaba. Y todo eso pasa antes de verificar la firma, antes de recalcular el
digest y antes de consultar ninguna clave — tiene que pasar, porque esas
comprobaciones necesitan los bytes que esa función produce.

> **Una máquina a la que un archivo que estaba a punto de rechazar puede sacar
> de memoria no tiene delante ninguna verificación de firma que valga.**

Decretado ahora:

- Cada miembro conocido tiene un tamaño máximo, y **los desconocidos no se leen
  en absoluto**. Leer un miembro para ignorarlo es hacerle el trabajo al
  atacante.
- El tamaño del encabezado se comprueba **y** la lectura se acota. Creerle al
  encabezado dejaría que un tamaño chico escondiera un cuerpo grande; acotar
  solamente truncaría en silencio en vez de rechazar.
- El artefacto no puede expandirse más de 50× su tamaño comprimido. Medido
  antes de existir el límite: 510 KB escribían 512 MB en menos de cuatro
  segundos y seguían. El comprimido está anclado dos veces —por el digest y por
  `artifact.size`— y no dice nada del expandido.
- El corte ocurre **mientras** se escribe, no después. Una comprobación
  posterior escribe igual todos los bytes de los que luego se queja, que en un
  disco lleno es el daño entero.

### Cómo se encontró, que importa más que el hallazgo

Una revisión externa listó una docena de riesgos del tar: enlaces duros,
symlinks que escapan, nodos de dispositivo, bombas de descompresión, límites de
entradas. **Al comprobarlos uno por uno, casi todos ya estaban cerrados.** Lo
que sí estaba abierto —los miembros ignorados que igual se leían— no estaba en
la lista.

La lista genérica no encontró el fallo. Lo encontró preguntar *qué corre antes
de la comprobación* y después **medirlo**. Ver [[Estrategia-de-Pruebas]].

## Regla general derivada

**El Core verifica lo que recibe, nunca lo que le reportan.** Ningún componente fuera de la TCB puede aportar el resultado de su propia verificación.

## Relacionado
- [[Formato-Manifiesto-Thmod]]
- [[Fase-Commit-Atomico]]
- [[Sandbox-Ejecucion]]
- [[Tres-Categorias-de-Autorizacion]]
- [[Modelo-de-Amenaza]]
- [[Caso-Instalar-Modulo]]
