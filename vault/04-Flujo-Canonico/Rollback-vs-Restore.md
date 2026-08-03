---
tipo: especificacion
estado: decretado
fecha-decreto: 2026-08-01
tags: [flujo, rollback, snapshots, cli]
---

# `rollback` y `restore` son dos operaciones distintas

## La ambigüedad que resuelve

[[Fase-Commit-Atomico|Build-then-commit]] protege las **instalaciones de módulos**: si la verificación falla, no hubo commit y no hay nada que deshacer.

Pero la demostración de adopción #1 de [[Condiciones-de-Adopcion]] promete otra cosa: *"el usuario mueve 10 archivos, actualiza dependencias, y con un solo comando el sistema revierte TODO"*. Eso es trabajo del usuario, no un artefacto publicado por Thalyx, y revertirlo exige restaurar un snapshot del filesystem.

Son dos operaciones con garantías, costos y riesgos completamente distintos, y la bóveda usaba una sola palabra para ambas.

## Decreto

### `thalyx rollback`

Deshace un **commit registrado de Thalyx**.

- Ámbito acotado: solo lo que Thalyx publicó.
- Barato y rápido.
- Garantizado por la arquitectura de commit atómico.
- No tiene efecto sobre archivos que Thalyx no publicó.
- No requiere confirmación adicional: no puede destruir trabajo del usuario.

### `thalyx restore <snapshot>`

Devuelve un subvolumen de Btrfs al estado de un snapshot.

- Ámbito amplio: todo lo que haya en ese subvolumen.
- **Destructivo.** Puede eliminar trabajo del usuario posterior al snapshot.
- Exige la comprobación de estado del decreto 4 de [[Coherencia-Doble-Ruta]]: si hay cambios que Thalyx no originó, se detiene.
- Exige confirmación explícita por el [[Camino-Confiable|camino confiable]], mostrando el diff de lo que se perderá.

## Por qué dos nombres y no uno inteligente

Se consideró un único comando que decidiera según el contexto. Se descartó: el usuario escribe una palabra esperando la operación barata y podría recibir la destructiva. En una herramienta que promete rollback como argumento de venta, esa confusión es exactamente la que arruina la confianza que la demostración pretende construir.

## Consecuencia sobre las demostraciones de adopción

La demostración #1 usa `restore`, no `rollback`. Decirlo en voz alta deja claro que la demostración más impactante del proyecto depende del comando peligroso, y por lo tanto de que la comprobación de estado previa funcione bien.

## Revisiones

### 2026-08-03 — `rollback` construido; lo que se aprendió al construirlo
**Antes:** el decreto separaba las dos operaciones y nada las implementaba.
**Ahora:** `thalyx rollback` existe y es exactamente la operación acotada. `restore` sigue sin existir: necesita Btrfs y se escribirá donde pueda ejecutarse.

Construirlo dejó dos cosas que el decreto no había anticipado:

**Casi todo el trabajo es negarse, y el peligro no está donde parecía.** Deshacer el commit es fácil. Lo difícil es negarse a deshacer una entrada que **ya no describe el mundo**: si el módulo se actualizó después, la versión que hay en disco no es la que esa entrada publicó, y "deshacer la instalación" borraría una versión que el humano sí quiere. Por eso el plan se calcula contra el disco y no contra el journal —el journal solo registra lo que hizo Thalyx, y el [[Principio-Doble-Ruta|principio de doble ruta]] garantiza que el humano pudo hacer otra cosa— y se vuelve a comprobar al aplicarlo, porque perder esa carrera cuesta borrar la instalación de alguien más.

**Cada negativa dice cuál de los motivos aplica.** Un intento rechazado no publicó nada; uno no comprometido es build-then-commit funcionando; una eliminación no se puede deshacer porque los bytes ya no están y lo que el humano quiere después es reinstalar. Juntarlos en "eso no se puede deshacer" escondería el único caso que es buena noticia.

Y una regla que salió sola: **nombrar una entrada que no se puede deshacer se niega, no cae hacia atrás a la última que sí.** El humano nombró una solicitud; deshacer otra no es una versión más pequeña de lo que pidió.

### 2026-08-03 — `restore` construido, y cómo se resolvió la contradicción aparente
El decreto dice dos cosas que suenan opuestas: **"si hay cambios que Thalyx no originó, se detiene"** y **"exige confirmación mostrando el diff de lo que se perderá"**. Si se detiene, ¿para qué el diff?

No se contradicen. **La deriva es el caso normal**: la demostración de adopción entera es un humano deshaciendo *su propio* trabajo, que es deriva por definición. Lo que el decreto prohíbe es seguir **sin que el humano se haya enterado**. Así que se detiene, muestra exactamente lo que encontró, y no avanza hasta que alguien que vio eso diga que sí.

Tres decisiones que salieron al construirlo:

**Lo que se reemplaza se conserva, no se borra.** En Btrfs no cuesta nada, y convierte "esto destruye tu trabajo" en "esto destruye tu trabajo y aquí quedó". Borrarlo es un acto aparte y deliberado.

**El intercambio es un solo `RENAME_EXCHANGE`.** La alternativa obvia —mover el árbol vivo a un lado, mover la copia restaurada a su lugar— tiene una ventana donde el directorio en el que trabaja el humano **no existe**. Un árbol que desaparece por un milisegundo es exactamente la "mitad" que build-then-commit existe para descartar. Donde el filesystem no lo soporta se cae al camino de dos renames, y **el journal registra cuál de los dos fue**: son dos garantías distintas, y una auditoría que no las distingue no puede decir si una interrupción habría sido sobrevivible.

**Se escribe la palabra, no una tecla.** `y` es memoria muscular. La cantidad de archivos que se van a borrar está en pantalla justo encima, y tener que escribir `restore` es la última oportunidad de leerla. Sin terminal la respuesta es no: silencio no es consentimiento, y un restore lanzado por un script que nadie miraba es justo lo que convierte una función de seguridad en un reporte de pérdida de datos.

Y una que se decidió sin que el decreto la mencionara: **el snapshot sobrevive a restaurar desde él.** Mover el snapshot a su lugar consumiría el momento que registra — un restore que solo se puede hacer una vez, y que destruye en silencio aquello desde lo que restauró. Se hace una copia escribible primero.

## Relacionado
- [[Coherencia-Doble-Ruta]]
- [[Journal-y-Snapshots]]
- [[Fase-Commit-Atomico]]
- [[Ramas-de-Fallo]]
- [[Condiciones-de-Adopcion]]
