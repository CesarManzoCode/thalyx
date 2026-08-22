//! `red` at the real prompt — point 8 of the usable terminal.
//!
//! The decree is `vault/02-Arquitectura/Red.md`. `thalyx-net` has its own tests
//! and they read a directory this repository wrote; these run the **session**,
//! against whatever `/sys/class/net` this machine really has, because rule 1
//! says every real defect in this project came from running the system. The
//! first version of this verb reported three network cards on a machine with
//! one, and no fixture test caught it — the machine did, on the first run.
//!
//! What can be asserted about a machine nobody chose is narrow, so these assert
//! only what is true of every Linux machine there is, and lean on the two faces
//! having to agree about it.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn thalyx() -> &'static str {
    env!("CARGO_BIN_EXE_thalyx")
}

fn typed(root: &Path, lines: &[&str]) -> Output {
    let mut child = Command::new(thalyx())
        .arg("session")
        .env("THALYX_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the session");

    let mut script = String::new();
    for line in lines {
        script.push_str(line);
        script.push('\n');
    }
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(script.as_bytes())
        .expect("feeding the session");
    child.wait_with_output().expect("waiting for the session")
}

fn answer(output: &Output) -> serde_json::Value {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value["op"] == "network")
        .unwrap_or_else(|| {
            panic!(
                "nothing answered `network`:\n{}",
                String::from_utf8_lossy(&output.stdout)
            )
        })
}

/// Rule 3: a check this machine cannot make says so, and one variable turns the
/// skip into a failure.
fn no_sysfs_here() -> bool {
    if Path::new(thalyx_net::SYS_CLASS_NET).is_dir() {
        return false;
    }
    let gap = "NOT PROVEN: /sys/class/net is not here, so `red` was never asked";
    assert_ne!(
        std::env::var("THALYX_REQUIRE_NETWORK_TESTS").as_deref(),
        Ok("1"),
        "{gap}"
    );
    println!("{gap}");
    true
}

#[test]
fn a_program_asks_what_network_hardware_there_is_and_gets_every_interface() {
    if no_sysfs_here() {
        return;
    }
    let store = tempfile::tempdir().unwrap();
    let said = answer(&typed(store.path(), &["structured on", "red", "salir"]));

    assert_eq!(said["ok"], true);
    let interfaces = said["interfaces"].as_array().expect("an array");
    assert_eq!(said["count"], interfaces.len());

    // Loopback is on every Linux machine there is, and it is the one row whose
    // shape can be asserted without knowing anything about the hardware.
    let lo = interfaces
        .iter()
        .find(|row| row["name"] == "lo")
        .expect("every machine has loopback");
    assert_eq!(lo["kind"], "loopback");
    // The finding from the first run, pinned at the prompt: loopback and the
    // kernel's software interfaces are listed and are not cards. Counting them
    // would report a network card on a machine that has none.
    assert_eq!(lo["is_card"], false);

    // Punto B1 and the point of the verb: a caller is told, in the answer, that
    // there is nothing here it can address. Reading `count` and concluding the
    // machine is on a network is the mistake this field exists to prevent.
    assert_eq!(said["addressable"], false);
}

#[test]
fn every_interface_carries_all_three_answers_to_the_cable_question() {
    if no_sysfs_here() {
        return;
    }
    let store = tempfile::tempdir().unwrap();
    let said = answer(&typed(store.path(), &["structured on", "red", "salir"]));

    for row in said["interfaces"].as_array().unwrap() {
        // Rule 10, as a shape a program can rely on: `unknown` is always a
        // possible answer, so a caller that only wrote the up/down branches is a
        // caller that will one day read a down interface as an unplugged cable.
        let carrier = row["carrier"].as_str().expect("a carrier word");
        assert!(
            ["up", "down", "unknown"].contains(&carrier),
            "{} answered `{carrier}`, which is not one of the three",
            row["name"]
        );
        // And the speed, which has its own third state. Always present, so the
        // branch gets written; never a number when nothing measured one.
        assert!(row["speed_known"].is_boolean());
        if row["speed_known"] == false {
            assert!(
                row["speed_mbps"].is_null(),
                "a speed nobody knows was given a number"
            );
        }
    }
}

#[test]
fn a_person_is_told_what_this_cannot_do_rather_than_being_left_to_find_out() {
    if no_sysfs_here() {
        return;
    }
    // Every other listing verb lists things the next verb acts on. A person who
    // reads a list of network cards and goes looking for the verb that uses one
    // is about to spend an afternoon on it, so the answer says so first. It is
    // A2 — the way out named at the moment it is useful — applied to a
    // capability that does not exist rather than to an error.
    let store = tempfile::tempdir().unwrap();
    let output = typed(store.path(), &["red", "salir"]);
    let said = String::from_utf8_lossy(&output.stdout);

    assert!(
        said.contains("cannot use them"),
        "the human face did not say what it cannot do:\n{said}"
    );
    assert!(
        said.contains("lo "),
        "loopback was not listed for a person:\n{said}"
    );
    // The control: a person never gets the object.
    assert!(
        !said.lines().any(|line| line.trim_start().starts_with('{')),
        "a person who never asked was handed JSON:\n{said}"
    );
}

#[test]
fn the_two_faces_count_the_same_machine() {
    if no_sysfs_here() {
        return;
    }
    // The double-route principle at its narrowest: one fact, two faces. A person
    // and a program asking the same question of the same machine, one second
    // apart, must not be told different numbers.
    let store = tempfile::tempdir().unwrap();
    let said = answer(&typed(store.path(), &["structured on", "red", "salir"]));
    let output = typed(store.path(), &["red", "salir"]);
    let human = String::from_utf8_lossy(&output.stdout);

    let count = said["count"].as_u64().unwrap();
    let cards = said["cards"].as_u64().unwrap();
    assert!(
        human.contains(&format!("{count} interface(s), {cards} of them a card")),
        "the two faces disagree: the object said {count}/{cards} and the sentence was:\n{human}"
    );
}
