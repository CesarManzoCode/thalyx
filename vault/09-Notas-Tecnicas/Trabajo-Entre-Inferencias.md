---
tipo: nota-tecnica
estado: hipotesis
fecha-actualizacion: 2026-08-29
tags: [agentes, medicion, hipotesis, costo]
---

# Trabajo entre dos inferencias

> **Esto es una hipótesis, no un resultado.** Lo que hay medido está en
> [[Evidencia-de-Agentes]]; lo que hay construido está en
> [[Ejecucion-Transaccional]]. Este archivo dice qué se está apostando y con qué
> instrumento se va a contestar.

## La hipótesis

> Una fuente mayor del costo y la latencia de un agente de programación es
> **volver al modelo frontera por decisiones operativas que una máquina
> determinista podía ejecutar sola.**

Lo que se quiere maximizar:

> **trabajo útil realizado entre dos pasos de inferencia del modelo frontera.**

Hoy un agente hace, típicamente:

```
modelo → buscar → modelo → leer → modelo → editar → modelo → inspeccionar
       → modelo → elegir pruebas → modelo → correr pruebas
       → modelo → interpretar un resultado obvio → modelo → deshacer
```

Lo que se está probando:

```
modelo → expresar una intención compuesta, una vez
       → THALYX, localmente:
             ejecutar varias operaciones
             observar lo que de verdad cambió
             validar
             decidir predicados deterministas
             confirmar o deshacer
             comprimir la evidencia
       → el modelo vuelve sólo cuando hace falta otra decisión semántica
```

## Por qué ningún número que ya teníamos puede verla

Todos los números de [[Evidencia-de-Agentes]] cuentan lo que hizo **el agente**:
turnos, llamadas a herramientas, tokens, costo, reloj. Ninguno puede ver la
cantidad de la que habla la hipótesis, porque **dos corridas de una llamada cada
una se ven idénticas** haya hecho esa llamada una cosa o treinta — y hacer
treinta es toda la apuesta.

Por eso hay instrumento nuevo, y por eso está separado en dos capas:

**Adentro de la máquina** (`thalyx_exec`, en cada respuesta):

| campo | qué cuenta |
|---|---|
| `external_requests` | viajes al que preguntó. **Uno, siempre, por construcción.** Está escrito y no supuesto porque es el numerador de toda la medición. |
| `machine_operations` | peticiones despachadas adentro, frontera y rollback incluidos |
| `process_launches` | procesos arrancados bajo confinamiento |
| `filesystem_mutations` | archivos que el árbol ganó, perdió o cambió |
| `state_witness_checks` | veces que se calculó y comparó un testigo de estado |
| `machine_time_ms` | cuánto tardó todo eso |
| `internal_bytes` | bytes producidos adentro y **no** enviados al modelo |

**En el adaptador** (`thalyx-mcp --metrics`, campo `programs`): la suma de lo
anterior por sesión, más `operations_per_request`. Se lee de la respuesta de la
máquina, no se infiere: es medición y no interpretación —no decide nada, no
cambia ninguna petición— y la alternativa, que este proceso contara los pasos que
mandó, contaría **lo que se pidió** en vez de lo que pasó, y seguiría contando
después de que un programa se detuvo en su segundo paso.

**En el resumen** (`dev/bench-summary.py`, `work_between_inferences`):
`thalyx_operations_per_program` y `thalyx_internal_bytes_per_returned_byte`.

Ausentes, nunca cero, cuando no corrió ningún programa. «Esta corrida no usó
programas» y «los programas de esta corrida no hicieron nada» son hechos
distintos, y un resumen que imprimiera `0` para el primero reportaría el
mecanismo como fallido cuando simplemente no se alcanzó.

### La asimetría, dicha en voz alta

Estos números existen para el brazo B y para ningún otro. **Eso no es un pulgar
en la balanza**: las llamadas a `Bash` del brazo A también hacen muchas cosas, y
este instrumento no puede ver adentro de ellas. Lo que sí puede decir con
honestidad es lo que Thalyx contó, y por eso los campos se llaman como Thalyx y
no como «trabajo».

## Lo que está probado localmente, hoy

En el fixture de `thalyx-cli::exec`, en este contenedor, sin Btrfs y sin modelo:

- **una petición externa → diez operaciones adentro de la máquina**, con la
  aritmética escrita como igualdad exacta y no como piso, para que un cambio se
  vea como un número que hay que releer y no como una prueba que sigue pasando;
- rollback automático cuando una comprobación no se sostiene, con el árbol
  byte por byte como estaba;
- protección de estado obsoleto, con su control positivo y su control negativo;
- compresión: la respuesta mide menos de un cuarto de lo que la máquina produjo
  adentro, y lo que quedó afuera se puede pedir.

## Lo que NO está probado y no se debe decir

- que Thalyx sea más barato que Linux;
- que Thalyx sea más rápido que Linux;
- que esto reduzca los pasos de inferencia de un agente real;
- que esto pruebe la tesis del sistema operativo;
- que `Bash` no sea la mejor interfaz.

Todo eso lo contesta **el siguiente banco controlado**, y no este archivo. Lo
único que se puede afirmar hoy es estructural: para ese flujo, el número de
viajes al modelo baja por construcción, y ahora existe el instrumento para ver si
eso mueve algo.
