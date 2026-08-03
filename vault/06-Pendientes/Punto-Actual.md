---
tipo: estado-vivo
estado: activo
fecha-actualizacion: 2026-08-03
tags: [continuidad, punto-actual, sesiones]
---

# Punto actual

> **Este archivo se actualiza cada vez que se termina algo.** Existe para que
> una sesión nueva —humana o de IA— sepa exactamente dónde quedó el proyecto
> sin que nadie tenga que recordarlo. Si algo importante vive solo en una
> conversación, esa conversación se pierde y el conocimiento con ella.
>
> Para *cómo* trabajar en el proyecto, ver `CLAUDE.md` en la raíz del repo.

## Dónde estamos, en una frase

La Fase 1 tiene **sus tres primitivas** —de las cuatro decretadas; la cuarta es
el [[Scheduler-Predictivo]] y es de Fase 2— y su flujo canónico **construidos y
verificados en hardware real**: 44 comprobaciones en máquina real, 0 sin probar,
0 fallidas. Desde entonces se sumaron 448 pruebas y el agente mínimo, que lleva
un enunciado hasta un módulo instalado sin modelo alguno. Lo que falta para
cerrar la fase es el **modelo del agente** y el **ISO booteable**.

## Última corrida verificada

**2026-08-03, Fedora 43, kernel 7.0.11, Btrfs, `bpf` en el orden de LSM.**

```
proven 44 · not proven 0 · failed 0
```

> **La próxima corrida no dará `not proven 0`, y eso es correcto.** `verify.sh`
> tiene ahora una etapa 10 para el agente, y la mitad que necesita un modelo
> real no la ha comprobado nada. Esperar alrededor de `proven 51 · not proven 1`
> —la etapa 10 aporta varias comprobaciones nuevas—. Un número
> verde que se conserva escondiendo lo que no se probó es exactamente la clase
> de instrumento que este proyecto existe para no construir.

Es la primera vez que todo lo que Thalyx afirma se comprueba en una sola
máquina y se sostiene. Reproducirla:

```
git pull && cargo install --path crates/thalyx-cli && sudo ./dev/verify.sh
```

## Qué quedó construido y probado

| Pieza | Comprobado en hardware |
|---|---|
| Instalación de módulos, commit atómico, journal, permisos | Sí, incluida inyección de fallos |
| `thalyx-lsm` (BPF LSM) | Sí — **deniega de verdad** una conexión dentro del cgroup y la permite fuera |
| Sandbox completo: namespaces, seccomp, `pivot_root`, idmap, límites | Sí — el módulo reporta su propio pid, uid, hostname, red y raíz |
| Un uid por módulo, nunca reutilizado | Sí |
| Índice en grafo + parser mecánico | Sí |
| Contador de mutaciones del kernel, 10 hooks | Sí — 5000 escrituras por descriptor abierto, todas contadas |
| Contador acotado al árbol | Sí — 5000 dentro contadas, 5000 fuera ignoradas |
| El atajo del índice (`graph trust`) | Sí — se gana con verificación, y un cambio real sigue saliendo obsoleto |
| Memoria persistente (3ª primitiva) | Sí — el hecho deja de ser afirmable al editar el archivo por fuera |
| `rollback` | Sí — quita el módulo y sus permisos; se niega la segunda vez |
| Snapshots de Btrfs | Sí — de solo lectura, conservan el contenido viejo |
| `restore` | Sí — restaura, destruye lo posterior, y conserva lo destruido |

Detalle por crate en [[Estado-de-Implementacion]].

## Lo que sigue, en orden

### 1. Un agente mínimo (`thalyx-agent`) — decidido el 2026-08-03

Va primero, y el motivo es de descubrimiento, no de avance. El ISO desbloquea
cinco de los seis pasos del [[Criterio-de-Salida-Fase-1|criterio de salida]]
contra uno del agente, pero el ISO **integra piezas ya probadas: no puede
enseñar nada que no se sepa ya**. El agente sí puede invalidar el diseño del
contrato. Descubrir tarde que la procedencia por campo no sobrevive a varias
inferencias costaría mucho más que un ISO retrasado, y la regla 1 de
`CLAUDE.md` dice que todos los defectos reales salieron de correr el sistema.

Alcance: router de reglas más un modelo con decodificación restringida por
gramática, sobre **un solo caso de uso** —instalar un módulo—, no un agente
general.

**Construida ya la mitad que no necesita un modelo**, y probada de punta a
punta: `thalyx agent do "install dev.thalyx.demo@^1.0" --repo <dir>` resuelve
contra un repositorio local de bundles firmados, pide confirmación por el camino
confiable, y deja el módulo instalado y ejecutable. Lo que falta, en orden:

1. El `Model` real que invoca `llama.cpp` como proceso.
2. La gramática GBNF, que no se puede validar sin `llama.cpp`.
3. El banco de las cuatro gamas, para sustituir las cifras estimadas.

Los tres necesitan tu máquina: aquí no hay `llama.cpp` y la política de red del
entorno bloquea `huggingface.co`.

**El decreto que lo bloqueaba ya está escrito:** [[Gamas-de-Modelo]]. No un
modelo anclado sino **cuatro gamas de una sola familia** que el usuario elige
según su hardware, con `llama.cpp` invocado como proceso y decodificación
restringida por gramática. Anclar un modelo de 5 GB dejaría fuera a una máquina
de 8 GB, y el criterio de salida exige justamente que alguien de fuera lo use.
Con la gramática, un contrato mal formado es imposible en las cuatro gamas: lo
que cambia entre ellas es el acierto al interpretar la intención, no la
seguridad. Y **el modelo nunca escribe la procedencia** — la pone el
ensamblador, porque una gramática obliga a la forma y no a la verdad.

El alcance del primero está en [[Agente-Minimo]].

Lo que sí está listo para el agente cuando exista: el contrato estructurado con
marcado de origen, el camino confiable, la memoria persistente, y el principio
de doble ruta implementado (todo lo que el agente podrá hacer, un humano ya
puede hacerlo por la CLI).

### 2. El ISO booteable

Ver [[Construccion-del-ISO]]. Independiente del agente. El criterio de salida
**no cambia**: sigue exigiendo arrancar la imagen en QEMU y apagar la máquina,
porque esos dos pasos prueban integración del sistema y no solo de los
componentes.

### 3. Reindexado incremental

Consumir el ringbuf `thalyx_mutations` para saber *qué* cambió, no solo que
algo cambió. **Ya no hace falta para el atajo** —eso lo resolvió la atribución
por ancestros— así que es una mejora de rendimiento, no de corrección. Ver
[[FS-en-Grafo]].

## Decretos abiertos

Ninguno bloquea excepto el primero.

- [ ] **La mitad de lectura del paso 6** — que el agente lea su propia memoria y retome una tarea, no solo que la escriba.
- [ ] **Una frontera real que etiquete canales** — hoy `--foreign` es una bandera que un humano pasa a propósito; nada en Thalyx llama a `Segment::foreign()` por su cuenta, porque nada trae texto de terceros todavía. Toda la defensa de procedencia descansa sobre ese código, que no existe.
- [ ] **Correr el banco de las gamas** — el decreto ya está ([[Gamas-de-Modelo]]); faltan las cifras medidas. Necesita `llama.cpp` y los pesos, que el contenedor de desarrollo no puede tener.
- [ ] Métricas de benchmark de la Fase 2 (el umbral ya está decretado; falta el instrumento)
- [ ] Técnicas de interpretabilidad aplicables al agente
- [ ] Arquitectura del índice semántico a mayor escala (SQLite alcanza para Fase 1)
- [ ] Sistema de reputación resistente a Sybil (pospuesto a propósito)
- [ ] Dependencias entre módulos con backtracking (pospuesto hasta que un módulo real las necesite)
- [ ] Condiciones para habilitar llamadas a modelos remotos

Lista completa y viva en [[Tareas-Pendientes]].

## Lo que sigue sin validarse, y es lo más importante

**Ningún decreto de esta bóveda ha sido contrastado con una persona ajena al
proyecto.** Todo el razonamiento sobre por qué alguien elegiría Thalyx sigue
siendo a priori. El [[Criterio-de-Salida-Fase-1|criterio de salida]] está
diseñado para forzar ese contacto: la fase no cierra sin que alguien de fuera
use el sistema.

Ver [[Por-Que-Elegirian-Este-SO]] y [[Riesgo-de-Ejecucion]].

## Cosas que hay que saber para no romper nada

**El watcher del LSM es todo o nada.** Diez hooks; si el kernel no expone
alguno, declina cargarse entero en vez de cargarse pareciendo completo. Un hook
faltante no es un número más chico, es una forma concreta de que un archivo
cambie en silencio. `make -C lsm hooks` dice cuáles hay.

**`verify.sh` desengancha el LSM al salir.** Por eso `thalyx graph watcher`
dice "not loaded" después de una corrida. Es correcto, no es un fallo.

**`verify.sh` compila en `dev/.verify-target`** para no dejar el `target/` del
usuario a nombre de root. Por eso el binario que queda en el PATH es el de
`cargo install`, y hay que reinstalarlo después de cambios en la CLI.

**El store por defecto es `/opt/thalyx`**, que necesita sudo. Para uso normal:
`export THALYX_ROOT=~/.local/share/thalyx`.

**El atajo del índice está apagado por defecto en cada índice nuevo**, y
`verify.sh` reconstruye el índice del repo, así que vuelve a apagarse en cada
corrida. Para encenderlo a mano:
`thalyx graph trust ~/thalyx/crates --counter`.

## Historial de sesiones

### 2026-08-03 (4) — el enunciado llega hasta el disco, y un fallo que solo salió corriéndolo
**Corrección sobre el paso 6:** lo construido es la *mitad de escritura*. El
criterio pide que **el agente conserve el contexto de la tarea**, y lo que hay
es un registro legible por un humano — el agente escribe en su memoria pero no
la lee. Falta la mitad que hace que retome una conversación a medias.

Lo que sí quedó: `thalyx agent do --task <t>`
escribe en la memoria persistente qué se pidió y qué se instaló, y
`thalyx memory recall <t>` lo lee desde otro proceso. Los dos hechos son de
clase distinta a propósito: lo que el humano dijo **no atestigua nada** —ningún
archivo puede volver falso que lo haya dicho— y lo instalado atestigua el enlace
`current`, así que quitar el módulo deja el recuerdo *no afirmable* y lo dice,
en vez de seguir reportando una instalación que ya no está.
`thalyx agent plan` y `thalyx agent do`, más el repositorio local y la
resolución de versiones (`thalyx-core/repo.rs`): **máxima versión que satisface
el constraint y cuya firma valida**, como manda [[Resolucion-de-Versiones]]. La
cadena entera funciona contra bundles firmados de verdad — enunciado, contrato,
resolución, camino confiable, commit atómico, journal, y el módulo instalado
corre.

**El fallo del día**, y es el más instructivo que ha dado el proyecto: la
atribución tomaba el canal *menos* confiable cuando un valor aparecía en dos.
Eso volvía imposible de instalar por nombre cualquier módulo mencionado en
cualquier página leída. Pasó 39 pruebas y tres mutantes deliberados. Murió a los
tres segundos de existir el comando, tecleando una frase. De ahí la regla nueva
de [[Estrategia-de-Pruebas]]: **un mutante demuestra que una prueba es portante,
no que la decisión que codifica sea la correcta.**

También quedó `thalyx dev agent-probe`, que existe por la regla 4: sin modelo,
toda inyección se rechaza con "no model is configured", y esa denegación se ve
idéntica a la de la procedencia sin probar nada de ella.

Antes de eso, `bundle.rs`: un `.thmod` de 768 MB **sin firma** llevaba el
proceso a 1 GB de RSS porque cada miembro se leía entero antes de decidir si
importaba. Ahora hay tamaños por miembro, los desconocidos no se leen, y el
artefacto no puede expandirse más de 50× lo comprimido.

### 2026-08-03 (3) — el agente mínimo, contra un modelo que miente a propósito
Se decretó [[Gamas-de-Modelo]] —cuatro gamas de una familia, `llama.cpp` como
proceso, gramática restringida, y **el modelo nunca escribe la procedencia**— y
se construyó `crates/thalyx-agent` hasta donde este contenedor puede
comprobarlo: router, atribución, ensamblado y un falso hostil con nueve formas
de portarse mal. 39 pruebas.

Al construirlo aparecieron dos cosas que el decreto no anticipaba, ya escritas
como revisión en [[Agente-Minimo]]: atribuir un valor por **dónde aparece**
también detecta las alucinaciones, y una *operación* no se puede atribuir
buscándola, así que se atribuye por lo que la conclusión pudo leer — de donde
sale que **en cuanto hay texto ajeno en el transcript, el modelo ya no puede
originar una acción**, y el humano sí, tecleándola.

Y una regla nueva de [[Estrategia-de-Pruebas]], encontrada rompiendo cada
mecanismo a propósito para ver qué pruebas lo notaban: **dos defensas que se
solapan hacen que la prueba grande no pruebe ninguna**. La prueba de las nueve
malas conductas no falló con ninguno de los tres mutantes.

### 2026-08-03 (2) — una revisión externa encontró que la bóveda se contradecía
Una lectura externa del repo —solo código y documentación, sin el contexto de
la filosofía— encontró que `Estado-de-Implementacion` afirmaba a la vez que
`restore` estaba construido y que **no existe**, y que los límites de recursos
seguían sin probarse cuando `verify.sh` ya tenía la etapa. Al corregirlo
aparecieron tres más: dos listas incompatibles de "las cuatro primitivas"
(contando [[Parser-Mecanico]], que su propio decreto llama *componente*), un
comentario en `thalyx-sandbox/src/lib.rs` que decía que un módulo corre con el
uid de Thalyx cuando `uids.rs` lleva días dándole uno propio, y "tres
variables" de salto donde hay cuatro.

Las cinco tienen la misma forma y de ahí sale la regla nueva de
[[Estrategia-de-Pruebas]]: **una afirmación de que algo falta no la rompe
nada**. El código rompe las afirmaciones de que algo funciona; las de ausencia
envejecen calladas.

También quedó anotado el hueco simétrico: `verify.sh` activa tres de sus cuatro
variables `THALYX_REQUIRE_*`, no la de Btrfs.

De la misma revisión se descartaron dos cosas: la supuesta inconsistencia de
fechas (2 de agosto 22:13 en CDMX **son** las 04:13 UTC del 3; la bóveda fecha
en UTC) y el reproche de que Thalyx "todavía no es un sistema operativo", que
es [[Decision-Capa-vs-SO-Nuevo|un decreto deliberado]] y no un hallazgo.

### 2026-08-03 — todo verde en hardware, y las dos operaciones del decreto
Se cerró el ciclo del contador de mutaciones (10 hooks, por CPU, acotado al
árbol), se abrió la puerta del atajo (`graph trust`), y se construyeron las dos
operaciones de [[Rollback-vs-Restore]]: `rollback` y `restore`, con snapshots
de Btrfs debajo. Cuatro defectos encontrados y arreglados, **tres de ellos del
arnés y no de Thalyx** — de ahí las reglas 5 y 6 de `CLAUDE.md`.

### 2026-08-02 — la tercera primitiva y el enforcement real
Memoria persistente, montajes idmapped, un uid por módulo, `pivot_root`, perfil
`module_standard`, y la primera demostración de que el LSM deniega de verdad en
hardware.

### 2026-08-01 — los decretos
43 → 61 notas. Modelo de amenaza, formato del manifiesto, commit atómico,
sandbox, permisos JIT, estrategia de pruebas, criterio de salida de la Fase 1.

## Relacionado
- [[Estado-de-Implementacion]] — qué está construido, por crate
- [[Tareas-Pendientes]] — qué está decidido y qué no
- [[Criterio-de-Salida-Fase-1]] — cuándo se puede decir que la fase terminó
- [[00-Indice/Indice-Principal|Índice principal]]
