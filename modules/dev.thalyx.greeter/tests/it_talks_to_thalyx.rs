//! The claim this whole piece exists to make: a module talks to Thalyx, and
//! only to Thalyx.
//!
//! Everything here runs the real binary as a real child process, with a real
//! socket placed on descriptor 3 by the same code the launcher uses. Nothing is
//! simulated in-process — the module's side of the conversation crosses an
//! `exec`, which is the part that has historically been wrong while every unit
//! test passed.

use std::io::Write;
use thalyx_abi::Level;
use thalyx_core::api::ModuleApi;
use thalyx_manifest::{Manifest, Permission, PermissionKind};

/// A manifest that validates. The module never sees it: it is what Thalyx
/// answers `Identify` from, which is the point — a module cannot state its own
/// identity, only ask for it.
fn manifest() -> Manifest {
    Manifest::parse(
        r#"
format_version = 1
id = "dev.thalyx.greeter"
name = "Greeter"
version = "1.0.0"
description = "The first module written against Thalyx's internal API"
license = "GPL-3.0-or-later"
publisher_key = "ed25519:3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29"
distribution = "prebuilt"

[artifact]
hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
size = 1

[requires]
thalyx = "^1.0"

[entrypoints]
run = "bin/greeter"
"#,
    )
    .expect("a manifest the test can lean on")
}

fn permissions(grants: &[(&str, &str)]) -> Vec<Permission> {
    grants
        .iter()
        .map(|(resource, action)| Permission {
            resource: resource.to_string(),
            action: action.to_string(),
            kind: PermissionKind::Persistent,
        })
        .collect()
}

/// Run the real module against a real Thalyx, and return what it said plus its
/// exit code.
fn run_greeter(grants: &[(&str, &str)], argument: &str) -> (Vec<(Level, String)>, Option<i32>) {
    run_greeter_with(grants, Some(argument))
}

/// The same, with the argument optional.
///
/// Separate rather than a default, because "nobody said which file" is a state
/// the module has to handle and not an oversight in the test.
fn run_greeter_with(
    grants: &[(&str, &str)],
    argument: Option<&str>,
) -> (Vec<(Level, String)>, Option<i32>) {
    let (thalyx_end, module_end) = std::os::unix::net::UnixStream::pair().expect("a socket pair");

    let mut child = {
        use std::os::fd::AsRawFd;
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_greeter"));
        if let Some(argument) = argument {
            command.arg(argument);
        }
        thalyx_syscall::spawn_with_channel(&mut command, module_end.as_raw_fd())
            .expect("starting the module")
    };

    // Thalyx must not keep the module's end: with it open the server would wait
    // forever for a writer that is this very process.
    drop(module_end);

    let mut api = ModuleApi::for_module(&manifest(), &permissions(grants));
    let serving = std::thread::spawn(move || {
        let mut stream = thalyx_end;
        thalyx_abi::serve(&mut stream, &mut api).expect("serving the module");
        api
    });

    let status = child.wait().expect("waiting for the module");
    let api = serving.join().expect("the serving thread");

    (api.said().to_vec(), status.code())
}

fn said_anything_like(said: &[(Level, String)], needle: &str) -> bool {
    said.iter().any(|(_, text)| text.contains(needle))
}

#[test]
fn the_module_learns_its_own_name_from_thalyx() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let granted = home.path().join("granted");
    std::fs::create_dir(&granted).expect("the granted directory");
    std::fs::write(granted.join("notes.txt"), b"hello from the disk").expect("a file to read");

    let (said, code) = run_greeter(
        &[(granted.to_str().unwrap(), "read")],
        granted.join("notes.txt").to_str().unwrap(),
    );

    assert_eq!(code, Some(0), "the module did not finish cleanly: {said:?}");
    assert!(
        said_anything_like(&said, "I am dev.thalyx.greeter 1.0.0"),
        "the module never learned who it was: {said:?}"
    );
}

#[test]
fn the_module_reads_a_file_it_was_granted() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let granted = home.path().join("granted");
    std::fs::create_dir(&granted).expect("the granted directory");
    std::fs::write(granted.join("notes.txt"), b"hello from the disk").expect("a file to read");

    let (said, code) = run_greeter(
        &[(granted.to_str().unwrap(), "read")],
        granted.join("notes.txt").to_str().unwrap(),
    );

    assert_eq!(code, Some(0), "{said:?}");
    assert!(
        said_anything_like(&said, "hello from the disk"),
        "the module did not read the file: {said:?}"
    );
}

#[test]
fn the_module_is_refused_what_it_was_not_granted() {
    // The denial, which the module reports itself. Without the baseline above
    // this would also pass on a Thalyx that refused everything.
    let home = tempfile::tempdir().expect("a temporary directory");
    let granted = home.path().join("granted");
    std::fs::create_dir(&granted).expect("the granted directory");
    std::fs::write(granted.join("notes.txt"), b"hello from the disk").expect("a file to read");

    let (said, code) = run_greeter(
        &[(granted.to_str().unwrap(), "read")],
        granted.join("notes.txt").to_str().unwrap(),
    );

    assert_eq!(code, Some(0), "{said:?}");
    assert!(
        said_anything_like(&said, "asked for /etc/shadow and was refused"),
        "the module was not refused /etc/shadow: {said:?}"
    );
    assert!(
        !said_anything_like(&said, "AND GOT IT"),
        "enforcement is not working: {said:?}"
    );
}

#[test]
fn a_module_granted_nothing_is_refused_the_very_file_it_was_told_to_read() {
    // The control for the grant itself. If this passed with contents, the
    // grants would be decorative.
    let home = tempfile::tempdir().expect("a temporary directory");
    let file = home.path().join("notes.txt");
    std::fs::write(&file, b"hello from the disk").expect("a file to read");

    let (said, code) = run_greeter(&[], file.to_str().unwrap());

    assert_eq!(
        code,
        Some(77),
        "a module with no grants read a file anyway: {said:?}"
    );
    assert!(said_anything_like(&said, "I was refused"), "{said:?}");
}

#[test]
fn told_nothing_the_module_reads_what_thalyx_said_it_may_read() {
    // This is how the module runs on the machine: the session starts it with no
    // arguments at all, so the only way it can know what to touch is to have
    // asked. A module that needed to be told would be a module carrying an
    // assumption about where the human keeps things.
    let home = tempfile::tempdir().expect("a temporary directory");
    let file = home.path().join("notes.txt");
    std::fs::write(&file, b"found without being told").expect("a file to read");

    let (said, code) = run_greeter_with(&[(file.to_str().unwrap(), "read")], None);

    assert_eq!(code, Some(0), "{said:?}");
    assert!(
        said_anything_like(&said, "found without being told"),
        "the module did not find its grant: {said:?}"
    );
}

#[test]
fn told_nothing_and_granted_nothing_the_module_says_so_rather_than_guessing() {
    // The control for the test above. Without it, a module that quietly fell
    // back to some path of its own would pass the first one and be doing the
    // one thing the arrangement forbids.
    let (said, code) = run_greeter_with(&[], None);

    assert_eq!(code, Some(64), "{said:?}");
    assert!(
        said_anything_like(&said, "nothing granted"),
        "the module did not say why it had nothing to do: {said:?}"
    );
}

#[test]
fn a_network_grant_is_not_mistaken_for_a_file_to_read() {
    // `net` is a grant and is not a path. A module that sent it to ReadFile
    // would be refused and would report the refusal as a permissions problem,
    // sending whoever read that message to look at the wrong thing entirely.
    let (said, code) = run_greeter_with(&[("net", "outbound")], None);

    assert_eq!(code, Some(64), "{said:?}");
    assert!(
        said_anything_like(&said, "nothing granted"),
        "a network grant was treated as a file: {said:?}"
    );
}

#[test]
fn the_module_refuses_to_run_without_thalyx() {
    // The claim from `Filosofia-Fundacional`, made checkable: this program does
    // not run anywhere else. Not because it checks a licence or a hostname —
    // because there is nothing on the other end of a channel it never opened.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_greeter"))
        .arg("/etc/hostname")
        .output()
        .expect("starting the module by hand");

    assert_eq!(
        output.status.code(),
        Some(64),
        "the module ran without Thalyx"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not run on its own"),
        "stderr was {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn a_descriptor_that_is_not_a_socket_is_not_mistaken_for_thalyx() {
    // Descriptor 3 exists here and is a file. Without the check in
    // `inherited_channel`, the module would start writing frames into whatever
    // happened to be open — which looks, from outside, like a module talking to
    // a system that is not there.
    let home = tempfile::tempdir().expect("a temporary directory");
    let decoy = home.path().join("decoy");
    let mut file = std::fs::File::create(&decoy).expect("the decoy file");
    file.write_all(b"not a socket").expect("writing the decoy");

    let output = {
        use std::os::fd::AsRawFd;
        let opened = std::fs::File::open(&decoy).expect("opening the decoy");
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_greeter"));
        command.arg("/etc/hostname");
        let mut child = thalyx_syscall::spawn_with_channel(&mut command, opened.as_raw_fd())
            .expect("starting the module");
        child.wait().expect("waiting")
    };

    assert_eq!(
        output.code(),
        Some(64),
        "a plain file was accepted as the channel to Thalyx"
    );
}
