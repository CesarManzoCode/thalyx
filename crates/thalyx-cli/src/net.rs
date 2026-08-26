//! `red` — point 8 of the usable terminal, and the last of the nine.
//!
//! The decree is `vault/02-Arquitectura/Red.md`. Cesar decided it on
//! 2026-08-23: **Thalyx learns to see the network and not to use it.** The
//! engine is `thalyx-net`; nothing here decides what an interface is.
//!
//! ## Why the sentence says what it cannot do
//!
//! Every other verb that lists something — `discos`, `procesos` — lists things
//! the next verb can act on. This one does not, and a listing that looked like
//! the others would teach that an address or a download is one command away.
//! So the closing line says the opposite out loud. It is the cheapest possible
//! version of A2: the error a person is about to make, answered before they
//! make it.
//!
//! ## The three-way facts, kept three-way in both faces
//!
//! `carrier` on an interface the kernel has taken down fails rather than
//! answering `0`, and `speed` answers `-1` when the link is up and the driver
//! does not know. Both are rule 10 — a failure to read is not a failure to
//! exist — and both survive into what is printed: `cable unknown` is a
//! different column from `no cable`, and a speed that is not known is absent
//! rather than shown as a number.

use crate::files::Face;
use serde_json::{Value, json};
use thalyx_net::{Carrier, Interface, Speed};

type Fallible = Result<(), Box<dyn std::error::Error>>;

const OP: &str = "network";

/// `red` — what network hardware this machine has.
pub fn interfaces(face: Face) -> Fallible {
    let every = match thalyx_net::every() {
        Ok(every) => every,
        Err(error) => {
            if face.is_machine() {
                println!(
                    "{}",
                    thalyx_files::machine::refused_with(
                        OP,
                        "no_sysfs",
                        "mount_sysfs",
                        &error.to_string(),
                        Vec::new(),
                    )
                );
            } else {
                println!("\n  {error}\n");
            }
            return Ok(());
        }
    };

    if face.is_machine() {
        let rows: Vec<Value> = every.iter().map(object).collect();
        println!(
            "{}",
            thalyx_files::machine::answer(
                OP,
                vec![
                    ("interfaces", json!(rows)),
                    ("count", json!(every.len())),
                    // Counted for the caller rather than left to be derived: a
                    // program that filtered on `kind == "ethernet"` would miss a
                    // wireless card, and one that counted everything would find
                    // a card on a machine that has only loopback.
                    (
                        "cards",
                        json!(every.iter().filter(|one| one.is_a_card()).count())
                    ),
                    // The whole point of the verb, said where a program reads it
                    // and not only in the human sentence.
                    ("addressable", json!(false)),
                ],
            )
        );
        return Ok(());
    }

    let cards = every.iter().filter(|one| one.is_a_card()).count();

    println!();
    if every.is_empty() {
        // Distinct from "no cards": an empty /sys/class/net means the kernel is
        // showing nothing at all, not even loopback, which is a different
        // machine from one that simply has no card in it.
        println!("  The kernel is showing no interfaces at all, not even loopback.");
        println!();
        return Ok(());
    }

    println!("  {} interface(s), {cards} of them a card:", every.len());
    println!();
    for one in &every {
        println!(
            "    {:<10} {:<10} {:<22} {}",
            one.name,
            one.kind.word(),
            link(one),
            one.mac.as_deref().unwrap_or("address unreadable"),
        );
        if let Some(driver) = &one.driver {
            println!("      {driver}");
        }
    }

    println!();
    if cards == 0 {
        println!("  No network card. Either there is none attached, or this kernel");
        println!("  has no driver for the one that is. `estado` says what else is");
        println!("  missing.");
        println!();
    }
    // Said every time, including when there are cards, because that is exactly
    // when somebody is about to look for the verb that uses them.
    println!("  Thalyx can see these and cannot use them: there is no address here,");
    println!("  and nothing sends a packet. `red` reads and never writes.");
    println!();
    Ok(())
}

/// The link column: what state it is in, whether anything is plugged in, and
/// how fast — with the two "cannot tell" answers kept apart from the negative
/// ones.
fn link(one: &Interface) -> String {
    let mut said = one
        .state
        .clone()
        .unwrap_or_else(|| "state unread".to_string());
    said.push_str(match one.carrier {
        Carrier::Up => ", cable",
        Carrier::Down => ", no cable",
        // Not "no cable". This is what a down interface answers, and the two
        // send a person to different places.
        Carrier::Unknown => ", cable unknown",
    });
    if let Speed::Mbps(mbps) = one.speed {
        said.push_str(&format!(", {mbps} Mb/s"));
    }
    said
}

fn object(one: &Interface) -> Value {
    json!({
        "name": one.name,
        "kind": one.kind.word(),
        "mac": one.mac,
        "state": one.state,
        "carrier": one.carrier.word(),
        // Three states, and a program gets all three. `null` is "the link is up
        // and nobody knows"; the key being absent would be a fourth thing.
        "speed_mbps": match one.speed {
            Speed::Mbps(mbps) => json!(mbps),
            Speed::NotKnown | Speed::Unreadable => Value::Null,
        },
        "speed_known": matches!(one.speed, Speed::Mbps(_)),
        "mtu": one.mtu,
        "driver": one.driver,
        "on_a_bus": one.on_a_bus,
        "is_card": one.is_a_card(),
    })
}
