//! Two screens made of nothing, so the display can be looked at on a machine
//! that has none.
//!
//! These are not fixtures for a test to assert against — the tests assert on
//! properties, not on this content. They exist so that
//! `thalyx dev screen <archivo.png>` produces something worth looking at, and
//! so that a change to the layout can be *seen* rather than only measured.
//!
//! The words are the ones the real machine says: the same verbs, the same shape
//! of answer, the same `▪` the store line prints. A sample written in lorem
//! ipsum would lay out beautifully and tell nobody whether a real path fits.

use crate::state::{Bar, Confirmation, Guard, Panel, Prompt, Row, Screen, Tone, Turn};

fn bar() -> Bar {
    Bar {
        machine: "thalyx".into(),
        store: "/dev/sdb2 thalyx-store".into(),
        guard: Guard::Enforcing,
        clock: "14:32".into(),
    }
}

/// A machine in the middle of being used.
pub fn working() -> Screen {
    Screen {
        bar: bar(),
        left: vec![
            Panel::new(
                "dónde",
                vec![
                    Row::fact("/home/cesar"),
                    Row::pair("cosas", "12"),
                    Row::pair("subvolumen", "user"),
                ],
            ),
            Panel::new(
                "archivos",
                vec![
                    Row::fact("notas.md"),
                    Row::fact("thalyx.config"),
                    Row::fact("proyectos/"),
                    Row::toned("respaldo.tar.gz", Tone::Muted),
                    Row::fact("Filosofía-Fundacional.md"),
                ],
            ),
            Panel::new(
                "módulos",
                vec![
                    Row::pair("dev.thalyx.greeter", "1.0.0"),
                    Row::note("Nada más instalado en esta máquina."),
                ],
            ),
        ],
        right: vec![
            Panel::new(
                "corriendo",
                vec![
                    Row::toned("dev.thalyx.greeter  pid 214", Tone::Ok),
                    Row::note("Un módulo, en su propio cgroup y con su propio usuario."),
                ],
            ),
            Panel::new(
                "memoria",
                vec![
                    Row::pair("en uso", "6.2 GiB"),
                    Row::pair("libre", "9.4 GiB"),
                    Row::pair("recuerdos", "38"),
                ],
            ),
            Panel::new(
                "permisos",
                vec![
                    Row::toned("net/outbound  vence en 22 s", Tone::Ok),
                    Row::toned("fs/write /home/cesar/notas.md", Tone::Ok),
                ],
            ),
            Panel::new(
                "red",
                vec![
                    Row::fact("enp2s0   arriba   1000 Mb/s"),
                    Row::toned("wlp3s0   abajo", Tone::Muted),
                ],
            ),
        ],
        conversation: vec![
            Turn::person("instala el greeter y déjalo corriendo"),
            Turn::agent(
                "Propongo instalar dev.thalyx.greeter 1.0.0 desde el repositorio local, \
                 verificando la firma contra la clave anclada, y correrlo después con el \
                 perfil module_standard. No he hecho nada todavía.",
            ),
            Turn::machine(
                "ok  verify   dev.thalyx.greeter 1.0.0 ▪ ed25519 contra la clave anclada\n\
                 ok  stage    /var/thalyx/staging/01J9…  ▪ 1.2 MiB desempaquetados\n\
                 ok  commit   journal #418 ▪ rename atómico",
            ),
            Turn::person("¿y qué permisos pidió?"),
            Turn::machine(
                "net/outbound   concedido, vence en 22 s\n\
                 fs/write       /home/cesar/notas.md",
            ),
            Turn::agent(
                "Los dos están en vigor. El de red es JIT y se va a vencer solo; si lo \
                 necesitas más tiempo hay que concederlo otra vez.",
            ),
        ],
        scrollback: 0,
        prompt: Prompt {
            line: "permisos dev.thalyx.greeter".into(),
            caret: 27,
            suggestion: None,
        },
        confirmation: None,
        editor: None,
    }
}

/// The trusted path, which is the whole display and nothing else.
pub fn confirming() -> Screen {
    Screen {
        confirmation: Some(Confirmation {
            what: "Instalar Thalyx en /dev/sdb borra todo lo que ese disco tenga.".into(),
            found: vec![
                Row::fact("/dev/sdb        7 GiB   USB"),
                Row::toned("  partición 1   512 MiB  FAT32 `THALYX`", Tone::Muted),
                Row::toned("  partición 2   6.5 GiB  btrfs `thalyx-store`", Tone::Muted),
                Row::note("Leído del disco, no de la lista de hace un momento."),
            ],
            type_this: "/dev/sdb".into(),
            typed: "/dev/sd".into(),
        }),
        ..working()
    }
}
