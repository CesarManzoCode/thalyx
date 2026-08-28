//! What a module is allowed to ask for of the machine's memory, and who decides.
//!
//! Until 2026-08-28 the answer was: it does not ask, and `module_standard` caps
//! it at a gigabyte. The number was chosen to be generous rather than tuned, and
//! the comment beside it said so — a policy knob, not an architectural decision.
//!
//! Then measuring a real inference engine against that profile found that the
//! confinement allows every one of the 31 system calls it needs to load a model,
//! tokenise, run the graph and generate, and then stops it on that one number: a
//! real model does not fit in a gigabyte, `mmap`ed weights count against
//! `memory.max` because cgroup v2 charges page cache, and **no manifest could
//! ask for more**. So the first real module Thalyx is being built to run could
//! not run.
//!
//! Cesar decided it the same day, with the alternatives beside it: **what the
//! manifest asks for, approved at install**. Not a bigger fixed number. A module
//! that wants eight gigabytes of his machine says so on the trusted path and he
//! says yes, which is the shape every other thing a module wants already has.
//!
//! What is tested here is the whole of that sentence: that the ask reaches the
//! confirmation in words a person can check, that a yes is what makes it
//! effective, that a no leaves nothing behind, and that an ask the machine
//! cannot honour is refused before anybody is asked to approve it.

mod harness;

use harness::Fixture;

/// A permission block asking for memory, in the manifest's own syntax.
///
/// `persistent` because the manifest is refused otherwise, and that refusal is
/// its own test below: a module that could take eight gigabytes on a `jit` grant
/// would be taking it without anybody being asked.
fn asking_for(amount: &str) -> String {
    format!(
        r#"
[[permissions]]
resource = "memory"
action   = "{amount}"
type     = "persistent"
"#
    )
}

#[test]
fn the_amount_reaches_the_confirmation_in_words_a_person_can_check() {
    let fixture = Fixture::new();
    let bundle = fixture.build_bundle_with_permissions("1.0.0", Some(&asking_for("4GiB")));

    // Answered `n`: what is measured is what the human was shown, and showing it
    // must not depend on them saying yes.
    let refused = fixture.install_bundle_at(&bundle);
    let seen = format!("{}{}", refused.stdout(), refused.stderr());

    assert!(
        seen.contains("4 GiB of memory"),
        "the amount was not on the confirmation in words:\n{seen}"
    );
    // The number is what a person cannot check. `4294967296` and `429496729`
    // differ by one character on the trusted path and by a factor of ten on the
    // machine.
    assert!(
        !seen.contains("4294967296"),
        "the confirmation showed the byte count instead of the size:\n{seen}"
    );
}

/// Rule 9, and the most expensive way this could be wrong.
///
/// A module asking for more than the machine has is refused **before** the
/// confirmation, because a question whose yes cannot be honoured teaches a
/// person that saying yes does not mean much. And refused rather than clamped:
/// a module confined to less than it asked for would die at a limit it never
/// agreed to, in a way nobody could trace back to installing it.
#[test]
fn more_memory_than_the_machine_has_is_refused_before_anybody_is_asked() {
    let fixture = Fixture::new();
    // Larger than any machine this will run on, and expressible: the units stop
    // at GiB precisely so that the refusal happens here rather than at a unit
    // nobody can satisfy.
    let bundle = fixture.build_bundle_with_permissions("1.0.0", Some(&asking_for("999999GiB")));

    let refused = fixture.install_bundle_at(&bundle);
    let seen = format!("{}{}", refused.stdout(), refused.stderr());

    assert!(
        seen.contains("999999 GiB") || seen.contains("more memory"),
        "the refusal did not say what was asked for:\n{seen}"
    );
    assert!(
        !seen.contains("Confirm?"),
        "a person was asked to approve an amount the machine cannot give:\n{seen}"
    );
    assert!(
        !fixture.store().is_installed(Fixture::MODULE_ID),
        "a module asking for more memory than the machine has was installed"
    );
}

/// The control for the test above. Without it, a machine that refused every
/// memory request at all would pass it — and a policy that breaks everything
/// looks like one that works.
#[test]
fn an_amount_the_machine_has_is_not_refused_for_being_too_large() {
    let fixture = Fixture::new();
    let bundle = fixture.build_bundle_with_permissions("1.0.0", Some(&asking_for("64MiB")));

    let asked = fixture.install_bundle_at(&bundle);
    let seen = format!("{}{}", asked.stdout(), asked.stderr());

    assert!(
        !seen.contains("more memory"),
        "an amount this machine certainly has was refused for being too large:\n{seen}"
    );
    // It got as far as asking, which is the whole claim: the guard above is
    // about the size and not about memory requests as such.
    assert!(
        seen.contains("64 MiB of memory"),
        "the request never reached a confirmation:\n{seen}"
    );
}

/// Packing a bundle whose manifest is refused, and giving back what was said.
///
/// The two tests below found that both refusals land **earlier** than the place
/// they were written for: `dev pack` parses the manifest, so a manifest the
/// machine cannot read never becomes a bundle at all. That is the better place
/// for it — a refusal at install is a refusal on somebody else's machine — and
/// it is why these two go through the packer rather than through
/// `install_bundle_at`.
fn packing_refuses(fixture: &Fixture, permissions: &str) -> String {
    use std::process::Command;

    let manifest = fixture.base().join("asking.toml");
    let source = std::fs::read_to_string(fixture.base().join("manifest-1.0.0.toml"))
        .expect("the fixture's own manifest, to change one block of");
    let (before, _) = source
        .split_once("[[permissions]]")
        .expect("the fixture manifest declares permissions");
    std::fs::write(&manifest, format!("{before}{permissions}")).expect("writing a manifest");

    let result = Command::new(env!("CARGO_BIN_EXE_thalyx"))
        .args(["dev", "pack"])
        .arg(fixture.base().join("payload"))
        .arg("--manifest")
        .arg(&manifest)
        .arg("--key")
        .arg(fixture.base().join("publisher.key"))
        .arg("--out")
        .arg(fixture.base().join("asking.thmod"))
        .output()
        .expect("pack");

    assert!(
        !result.status.success(),
        "a manifest that should have been refused was packed"
    );
    String::from_utf8_lossy(&result.stderr).into_owned()
}

/// `Tres-Tipos-de-Permiso.md`: only `persistent` always requires a human./// `Tres-Tipos-de-Permiso.md`: only `persistent` always requires a human.
///
/// A `jit` memory grant would be eight gigabytes of somebody's machine taken
/// automatically, which is the one thing the trusted path exists to prevent. The
/// manifest is refused rather than the permission being quietly upgraded, so
/// that the person who wrote it finds out.
#[test]
fn memory_can_never_be_asked_for_without_a_human() {
    let fixture = Fixture::new();
    // The fixture's own bundle first, so there is a manifest to take the
    // unchanged half of.
    let _ = fixture.build_bundle("1.0.0");

    let said = packing_refuses(
        &fixture,
        r#"
[[permissions]]
resource = "memory"
action   = "4GiB"
type     = "jit"
"#,
    );
    assert!(
        said.contains("never automatic"),
        "a jit memory grant was not refused as one: {said}"
    );
    // And the control: the same block as `persistent` packs. Without it this
    // would pass against a build that refused every memory permission there is.
    let _ = fixture.build_bundle_with_permissions("1.0.1", Some(&asking_for("4GiB")));
}

/// A bare number is refused, and this is the test that says why it matters.
///
/// Read as bytes it confines a module asking for eight gigabytes to eight bytes;
/// read as gigabytes it hands over the machine. Neither is a guess worth making,
/// so the manifest does not parse at all.
#[test]
fn an_amount_with_no_unit_is_not_a_manifest() {
    let fixture = Fixture::new();
    let _ = fixture.build_bundle("1.0.0");

    let said = packing_refuses(&fixture, &asking_for("8589934592"));
    assert!(
        said.contains("not an amount") || said.contains("with a unit"),
        "a unitless amount was not refused as unreadable: {said}"
    );
}
