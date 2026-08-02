---
tipo: primitiva
estado: decretado
fecha-decreto: 2026-07-31
tags: [primitiva, permisos, seguridad, kernel, fase-1]
---

# Permisos just-in-time (JIT)

## Función

La IA pide acceso temporal a recursos, el SO otorga y revoca automáticamente.

## Implementación (Fase 1)

- **`thalyx-lsm`**, el enforcement en el kernel, implementado como **programa BPF LSM**.
- **`thalyx-permd`**, el broker en userspace que decide qué se otorga y por cuánto tiempo.

La frontera entre ambos es un **mapa BPF anclado**: `thalyx-permd` escribe la política, `thalyx-lsm` la lee y decide en el hook. El userspace nunca está en el camino crítico de una decisión de seguridad, y el kernel nunca contiene lógica de política.

El LSM aplica los permisos y además intercepta las mutaciones del filesystem que alimentan el [[FS-en-Grafo|índice semántico]] — ver [[Coherencia-Doble-Ruta]].

### Por qué BPF LSM y no un módulo del kernel

**En Linux mainline un LSM no puede ser un módulo cargable.** `security_add_hooks()` no está exportado a módulos; los LSM se registran en el arranque con `DEFINE_LSM()` y el parámetro `lsm=`. La capacidad de cargarlos dinámicamente se retiró del kernel deliberadamente hace casi veinte años.

BPF LSM (kernel 5.7+, `CONFIG_BPF_LSM=y`) permite enganchar programas a los mismos hooks en tiempo de ejecución, con la misma capacidad de denegar. No es una versión debilitada del enforcement: es el mismo punto del kernel, con el mismo poder de veto.

Lo que sí cambia es el ciclo de trabajo: cargar y recargar toma segundos en vez de recompilar el kernel y reiniciar. Eso importa más de lo que parece — **todos los defectos reales encontrados en este proyecto salieron de ejecutar el sistema, no de revisarlo**, y un ciclo de quince minutos por iteración es la forma más eficaz de dejar de encontrarlos.

### Camino hacia el LSM in-tree

El conjunto de hooks y la política que se descubran en BPF se trasladan directamente a un LSM in-tree cuando estén estables. La condición para migrar es una limitación documentada del verificador de BPF que impida expresar una política necesaria — no una preferencia estética.

Thalyx compila su propio kernel en cualquiera de los dos casos: BPF LSM exige `CONFIG_BPF_LSM=y` y `bpf` en el orden de `lsm=`. La autoridad de diseño de [[Decision-Capa-vs-SO-Nuevo]] se conserva íntegra.

Flujo:

1. El agente solicita: `REQUEST_PERMISSION(resource: "/home/user/docs", action: "write", type: "jit", duration: "30s")`.
2. `thalyx-permd` evalúa la solicitud contra la política y escribe la concesión en el mapa BPF, indexada por el cgroup del módulo.
3. `thalyx-lsm` lee el mapa en cada hook y permite o deniega.
4. Al expirar, `thalyx-permd` retira la entrada del mapa. La revocación es inmediata: no hay proceso al que avisar.

## Tipos de permiso (decretado — reemplaza el campo `duracion` simple)

Se modela con un campo `type` en vez de solo duración, porque representan **políticas de seguridad distintas**, no solo diferencias de tiempo:

```json
{
  "type": "jit | session | persistent",
  "requires_confirmation": true,
  "revocable": true
}
```

- **JIT:** automático, sin confirmación humana, expira solo (ej. 30 segundos), pensado para acciones rutinarias de bajo riesgo.
- **Sesión:** válido mientras dure la sesión activa del agente/usuario, se revoca al cerrar.
- **Persistente:** requiere confirmación explícita del humano **sin excepciones**, no expira solo, permanece activo hasta revocación manual.

### Regla de política decretada
Cualquier permiso persistente sobre red o carpetas de usuario requiere confirmación explícita, **siempre**, sin importar la reputación del módulo que lo solicite. La confirmación se presenta por el [[Camino-Confiable|camino confiable]], nunca a través del agente.

### Regla de alcance
Los permisos efectivos de un módulo son exactamente los declarados en su manifiesto. Ver [[Formato-Manifiesto-Thmod]] y la regla de contención en [[Contrato-Estructurado]].

### Regla de orden y de vigencia

Un permiso confirmado por el usuario se registra en estado **pendiente**, atado al `request_id` del contrato. Su registro se escribe **antes** del commit, pero **solo tiene fuerza mientras el módulo al que pertenece sea la versión actual**.

Esa condición es lo que hace que el intercambio del enlace simbólico del commit sea el **único punto atómico** que decide a la vez "instalado" y "autorizado". No hay una segunda transición que pueda interrumpirse por separado.

Consecuencias:

- Si el proceso muere antes del commit, el registro queda escrito pero **inerte**: ningún módulo apunta a él, así que no otorga nada.
- Si muere después, el módulo y sus permisos son consistentes de inmediato, sin depender de ningún paso posterior.
- Un registro inerte no se muestra nunca como permiso vigente. Mostrarlo sería mentirle al soberano sobre lo que autorizó.

La clave del publicador se ancla en el mismo punto y por la misma razón: anclarla después del commit dejaba, ante un corte, un módulo instalado **sin clave anclada a su `id`** — de modo que el siguiente paquete ofrecido para ese identificador, firmado por cualquiera, se habría aceptado como primer avistamiento. Es la suplantación de publicador del [[Modelo-de-Amenaza]], abierta por una interrupción en vez de por un ataque.

Ver [[Fase-Commit-Atomico]] y [[Estado-de-Implementacion]].

## Estado: demostrado en hardware real

El 1 de agosto de 2026, en un Ryzen 5 5600G con Fedora 43 (kernel 6.18), el enforcement denegó una conexión de red a un proceso que no tenía el permiso, y **solo a ese proceso**:

```
enforcement OFF   inside:   errno=111 Connection refused
enforcement ON    inside:   errno=1   Operation not permitted
                  outside:  errno=111 Connection refused
```

Mismo cgroup, misma política, mismo destino. Lo único que cambió entre las dos mediciones fue el flag de enforcement, y el fallo pasó de ECONNREFUSED a EPERM. Fuera del cgroup no cambió nada.

Esa última línea es la que importa tanto como la denegación: una política que rompiera todo se vería idéntica a una que funciona si solo se mirara desde dentro. Reproducible con `make -C lsm demo`.

**Esta es la primera de las cuatro primitivas con evidencia de que hace lo que el decreto dice.** Las otras tres siguen siendo diseño.

## Origen de esta distinción

Esta distinción de tres tipos no estaba en el diseño original — surgió al trazar el [[Caso-Instalar-Modulo|caso concreto de instalar un módulo]], cuando se descubrió que un permiso persistente (acceso a red, sin expiración) no encajaba en el modelo original de "JIT de 30 segundos". Es un ejemplo de por qué trazar casos concretos revela huecos que el diseño abstracto no muestra.

## Revisiones

### 2026-08-01 — Se resuelve la contradicción sobre dónde vive el enforcement
**Antes:** [[Decision-Kernel-vs-Userspace]] situaba esta primitiva en el kernel desde el inicio, mientras que [[Fases-de-Implementacion]] la listaba como daemon de userspace en Fase 1 y posponía el LSM a Fase 3. Dos notas decretadas en contradicción directa.
**Ahora:** `thalyx-lsm` se escribe desde la Fase 1. `thalyx-permd` en userspace decide la política; el LSM la aplica.
**Motivo:** un broker de userspace sin enforcement en el kernel deja los permisos en régimen cooperativo — un módulo que ignore al broker no queda contenido, y la primitiva no cumple lo que promete. Se evaluó y se descartó apoyarse en Landlock: Thalyx es un sistema operativo propio, no una distribución de Linux, y el control del enforcement es parte de su arquitectura, no una dependencia delegada.

### 2026-08-01 — Se añaden las reglas de alcance y de orden
**Motivo:** al trazar el caso canónico contra el [[Modelo-de-Amenaza]] se detectó que nada cruzaba los permisos del contrato contra los del manifiesto, y que un permiso persistente podía quedar vivo tras una instalación fallida.

### 2026-08-01 — La vigencia se ata a la versión actual, no al orden de escritura
**Antes:** el decreto decía que los permisos "se vuelven efectivos únicamente dentro del commit atómico", sin precisar cómo. Al implementarlo quedó claro que no puede hacerse literalmente: el commit publica archivos y el registro de permisos es otro archivo, así que serían dos escrituras, y una de las dos quedaría fuera de la ventana atómica.
**Ahora:** el registro se escribe antes del commit y su vigencia se condiciona a que el módulo sea la versión actual.
**Motivo:** escribir los permisos *después* del commit deja una ventana donde el módulo está instalado y no tiene nada; escribirlos antes sin condicionar la vigencia deja un permiso vivo para un módulo que no existe. Condicionar la vigencia elimina ambas: hay un solo punto atómico, el del enlace simbólico, y gobierna las dos cosas.
**Cómo se descubrió:** al ejecutar el sistema real tras un crash inyectado a mitad del commit. El núcleo filtraba bien, pero la interfaz mostraba dos permisos persistentes de red vigentes para un módulo que no estaba instalado. Ver [[Estrategia-de-Pruebas]].

## Revisiones

### 2026-08-01 — El LSM pasa a ser un programa BPF LSM
**Antes:** se decretaba `thalyx-lsm` como "un módulo de seguridad del kernel propio", cargable.
**Ahora:** programa BPF LSM, con un mapa anclado como frontera entre política (userspace) y enforcement (kernel).
**Motivo:** el decreto no era implementable. Un LSM no puede ser un módulo cargable en Linux mainline — `security_add_hooks()` no está exportado y el registro ocurre en el arranque. La intención del decreto (enforcement real en el kernel, no cooperativo) se conserva entera; lo que cambia es el mecanismo, y de paso el ciclo de iteración pasa de quince minutos a segundos.
**Qué no cambia:** se sigue rechazando Landlock por la misma razón de antes. BPF LSM no es una política ajena que se acata, es infraestructura del kernel sobre la que Thalyx escribe la suya.

## Relacionado
- [[Caso-Instalar-Modulo]]
- [[Tres-Categorias-de-Autorizacion]]
- [[Camino-Confiable]]
- [[Modelo-de-Amenaza]]
- [[Flujo-Canonico-Overview]]
