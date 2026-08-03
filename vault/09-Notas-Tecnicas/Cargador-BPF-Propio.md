---
tipo: nota-tecnica
estado: activo
fecha-decreto: 2026-08-03
tags: [bpf, lsm, kernel, imagen, fase-1]
---

# El cargador de BPF propio

> **Estado: probado en hardware el 2026-08-03**, después de un fallo que vale
> la pena leer. La primera corrida de la etapa 14 rechazó el programa con
> `bpf_lsm_socket_connect() is not modifiable`: `BPF_LSM_MAC` estaba escrito
> como 26, que es `BPF_MODIFY_RETURN`, la entrada anterior del mismo `enum`. El
> kernel aplicó la comprobación equivocada y lo dijo con esas palabras. Ver la
> regla sobre constantes capturadas en [[Estrategia-de-Pruebas]].

## Qué problema resuelve

[[Filosofia-Fundacional]] decreta que la imagen lleva el kernel de Linux y **un
programa**. `thalyx-lsm` es lo que vuelve reales los permisos, y la única forma
de cargarlo era invocar `bpftool` —un segundo programa— desde una shell —un
tercero—. La imagen no tiene ninguno de los dos.

Peor: el cargador buscaba `/lib/thalyx/thalyx_lsm.bpf.o`, un segundo archivo. El
mensaje que imprimía al no encontrarlo sugería que alguien lo pusiera ahí, o sea
**sugería romper el decreto que estaba reportando**.

La respuesta es la misma forma que la del store: el trabajo se mueve. El objeto
BPF se compila al construir, viaja **dentro** del binario, y Thalyx hace las
llamadas al kernel él mismo.

## Cómo está partido, y por qué

| Dónde | Qué hace | `unsafe` |
|---|---|---|
| `crates/thalyx-bpf` | Leer ELF, leer BTF, sacar la forma de los mapas, resolver CO-RE, reubicar | prohibido |
| `crates/thalyx-syscall` | Las cuatro llamadas `bpf(2)` | permitido, como en todo lo demás |
| `crates/thalyx-cli/build.rs` | Meter el objeto en el binario | — |

Ese corte es lo que permite que **casi todo se pueda probar en una máquina sin
BPF**, que son casi todas. 41 pruebas del cargador no necesitan kernel.

## El orden, que no es intercambiable

1. **Los mapas primero.** Un programa se refiere a un mapa por descriptor de
   archivo, y no hay descriptor hasta que el mapa existe.
2. **CO-RE contra el kernel que está corriendo.** Ver abajo.
3. **Los descriptores de los mapas**, escritos dentro de las instrucciones.
4. **Cargar**, que es donde el verificador acepta o explica largamente por qué
   no.
5. **Enlazar.** Un programa cargado y no enlazado está en el kernel y en el
   camino de decisión de nadie. Se lista igual que uno vivo. Por eso `make
   status` cuenta enlaces y no pins.
6. **Fijar (pin)**, porque `thalyx-permd` es otro proceso y encuentra el mapa de
   política por su ruta en bpffs. Sin pin, el enforcement estaría puesto y nada
   podría escribirle un permiso.

Si algún programa falla, los que ya se enlazaron se sueltan y todo falla.
Enforcement con uno de sus dos hooks vivo es peor que ninguno: la sesión
reportaría que está puesto, y los archivos se revisarían mientras las conexiones
no.

## Las tres cosas que no son obvias

### Los mapas no están en la sección de datos

`.maps` son ceros. Un mapa declarado así:

```c
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 4096);
    __type(key, __u64);
} thalyx_policy SEC(".maps");
```

no contiene ningún número. `__uint(x, N)` se expande a *un puntero a un arreglo
de N elementos* —el número es el largo del arreglo— y `__type(key, T)` a un
puntero a T, cuyo tamaño hay que medir. Todo lo real está en los **tipos**. Es
lo que hace libbpf, y el motivo es que así no hace falta que el compilador sepa
nada especial: es C ordinario que codifica enteros donde la información de tipos
los conserva.

### CO-RE lleva offsets deliberadamente equivocados

Cada `BPF_CORE_READ(file, f_flags)` compila a una instrucción con el offset del
header contra el que se compiló. El objeto registra, aparte, **qué tipo y qué
campo** quiso decir. El cargador lo busca en el BTF del kernel que está
corriendo y parcha la instrucción.

Saltarse ese paso da un programa que carga, pasa el verificador, corre, y lee
los cuatro bytes equivocados — para `file_open`, decide lectura-contra-escritura
a partir de lo que haya en ese offset. **No falla nunca. Aplica lo incorrecto
para siempre.**

Se resuelve **por nombre de campo, no por índice**. El objeto registra índices y
usarlos directo sería menos código: un kernel que insertó un campo corre todos
los índices siguientes, y el cargador calcularía un offset plausible del miembro
equivocado.

### Un pin no es un enlace

Un programa puede estar cargado, fijado y en el camino de nadie, y se lista
idéntico a uno vivo. Es exactamente cómo una herramienta de seguridad se lee
como armada estando desarmada. La etapa 14 cuenta **enlaces**.

## Qué se rechaza en vez de adivinarse

Todo lo que no reconoce: un `kind` de BTF desconocido, una clase de reubicación
CO-RE que no realiza, una clase de instrucción sin sitio definido donde parchar,
un tipo sin tamaño. Una reubicación saltada es una instrucción con un número de
otro kernel, y la regla 9 de [[Estrategia-de-Pruebas]] pide la respuesta
cautelosa, nunca la rápida.

## Cómo se comprueba sin kernel

`crates/thalyx-bpf/tests/captured/` tiene dos muestras, las dos salida real de
clang y ninguna escrita a mano:

- `thalyx_lsm.bpf.o` — el objeto de verdad, compilado del `.c` de verdad.
- `kernelish.btf` — un sustituto del BTF de un kernel, donde `f_flags` está en
  el byte **20** detrás de tres campos que el header del objeto no tiene, y
  `sa_family` sigue en **0**.

Ese par es la prueba entera: **un campo que se movió y uno que no.** Un cargador
que no hiciera nada pasaría con `sa_family` y fallaría con `f_flags`. Es la
forma de línea base y control que pide [[Estrategia-de-Pruebas]], hecha con dos
campos en vez de con dos máquinas.

## Lo que sigue faltando

- **`thalyx_watch`**, el contador de mutaciones, se sigue cargando con bpftool.
  Diez hooks en vez de dos y ninguna reubicación CO-RE nueva, así que el mismo
  cargador debería servir; no se ha intentado.
- **Quitar `vmlinux.h` de la construcción.** Hoy `make -C lsm` necesita bpftool
  para generarlo. Un header escrito a mano con `preserve_access_index` produciría
  el mismo objeto —CO-RE resuelve los offsets al cargar, así que lo único que el
  header local tiene que acertar son los **nombres**—, y eso quitaría bpftool
  también del lado de construir. No está hecho y no es urgente: construir no es
  arrancar.

## Relacionado
- [[Filosofia-Fundacional]] — el decreto que esto hace cumplir
- [[Construccion-del-ISO]] — qué lleva la imagen
- [[Permisos-JIT]] — qué política escribe el mapa que esto crea
- [[Estrategia-de-Pruebas]] — las reglas que dan forma a los rechazos
- [[Primer-Arranque]] — dónde se ve el resultado
