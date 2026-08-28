//! The channel an agent outside the machine reaches Thalyx through.
//!
//! `vault/07-Adopcion-y-Fases/Agentes-Externos.md`. What is here is the
//! transport and the loop; what may be asked for is [`crate::external`], and the
//! grammar of a message is `thalyx-bridge`, which the host links too so there is
//! one definition of a frame rather than two.
//!
//! ## Why this is a thread and not a daemon
//!
//! `Filosofia-Fundacional.md`: the image carries the Linux kernel and one
//! program, and `make -C image count` says so out loud. A second process in the
//! initramfs would be the first thing on that machine that is not Thalyx, added
//! for the convenience of something that is not even running on it. So the
//! endpoint is a thread of the session — the same binary, the same store, the
//! same verbs, and nothing new on the disk.
//!
//! ## And why it is silent when there is no port
//!
//! An ordinary Thalyx machine has no agent channel and must not be able to tell
//! that this code exists: no error, no wait, no line on the boot report. The
//! whole of that is [`port`] returning `None` and the thread never starting.
//!
//! ## What the container this was written in cannot check
//!
//! virtio-serial. There is no QEMU here and no `/sys/class/virtio-ports`, so
//! [`port`] has never found one — it is exercised against a directory laid out
//! like sysfs, which proves the search and not the driver. Everything above the
//! transport *is* exercised, over a UNIX socket, by `thalyx bridge serve
//! --listen`: the framing, the confinement, the verbs and the answers are the
//! same code whichever character device they are on. `dev/verify.sh` has the
//! stages, and the one that needs QEMU is named as needing it.

use crate::external::{ExternalAgentSession, Refusal};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use thalyx_bridge::{FromThalyx, ToThalyx, WireError, read_frame, write_frame};
use thalyx_core::Store;

/// The name the port carries on the QEMU command line and inside the guest.
///
/// One constant, because `image/Makefile` writes it into `-device
/// virtserialport,name=…` and this reads it out of sysfs, and a machine where
/// those two disagree comes up with a channel nobody is listening on.
pub const PORT_NAME: &str = "org.thalyx.agent";

/// Where the kernel says which virtio-serial port is which.
const PORTS: &str = "/sys/class/virtio-ports";

/// Find the character device this machine's agent port is, if it has one.
///
/// Asked of sysfs rather than assumed to be `/dev/virtio-ports/<name>`. That
/// path is a symlink **udev** makes, and Thalyx has no udev: on this machine the
/// node is `/dev/vport0p1`, and which number it is depends on what else QEMU was
/// given. The name is the stable fact and the kernel publishes it.
pub fn port() -> Option<PathBuf> {
    port_under(Path::new(PORTS), Path::new("/dev"))
}

/// The same search, rooted where a test can build one.
fn port_under(ports: &Path, dev: &Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(ports).ok()?.flatten() {
        // `continue`, never `?`. A port with no name has no `name` file at all
        // — the driver creates that attribute only for a port QEMU named, under
        // the comment *"Since we only have one sysfs attribute, 'name', create
        // it only if we have a name for the port"* — so abandoning the search at
        // the first unreadable one meant a single unnamed port could hide this
        // machine's channel, depending on nothing but the order `read_dir`
        // happened to return them in. Rule 10: a name that cannot be read is
        // not a port that is not there.
        let Ok(name) = std::fs::read_to_string(entry.path().join("name")) else {
            continue;
        };
        if name.trim() != PORT_NAME {
            continue;
        }
        let node = dev.join(entry.file_name());
        // The name being published does not mean the node is there. Rule 10 —
        // and here the two have different remedies: a missing node means the
        // machine came up without devtmpfs, which is a much larger problem than
        // no agent channel.
        if node.exists() {
            return Some(node);
        }
    }
    None
}

/// Start the endpoint on its own thread, if this machine has a port at all.
///
/// Called by the session once, and its return value is what the boot report
/// says. Nothing about it can fail in a way that matters to the machine: a
/// thread that dies takes the channel with it and leaves Thalyx running, which
/// is the whole point of it being one.
pub fn start(store_root: PathBuf, workspace: PathBuf) -> Option<PathBuf> {
    let node = port()?;
    let opened = node.clone();
    std::thread::Builder::new()
        .name("thalyx-agent-bridge".into())
        .spawn(move || {
            // Forever, and re-opening each time round. A host that disconnects
            // gives this thread an end-of-file, and the next agent to connect
            // must find a channel rather than a socket that was used once.
            loop {
                if let Err(error) = serve_port(&opened, &store_root, &workspace) {
                    // Nowhere to print: the screen owns descriptor 1 while it
                    // draws, and a line from this thread would land in the
                    // middle of a frame. The person's route to this is
                    // `historia`, which is where the journal already is.
                    let _ = error;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        })
        .ok()?;
    Some(node)
}

/// One pass over a virtio-serial port: open it, serve it until it closes.
///
/// **Opened once, for both directions, and the second handle is a `dup` of the
/// first.** This used to open the node twice — once read-only and once
/// write-only — which is the shape the socket has and is not the shape a
/// virtio-serial port has: `port_fops_open` in `drivers/char/virtio_console.c`
/// refuses the second open with `EBUSY`, under the comment *"Allow only one
/// process to open a particular port at a time"*. So on the one transport this
/// code exists for, every pass failed at its second line, the error went into
/// the `let _ = error` above, and the machine came up advertising a channel that
/// answered nothing — a host would connect, wait for a hello that could never
/// arrive, and read it as a VM that was still booting.
///
/// Nothing here was ever wrong on a socket, which is exactly why no test caught
/// it: rule 8, and rule 12. The fake was a different system at the one point
/// that mattered.
fn serve_port(node: &Path, store_root: &Path, workspace: &Path) -> Result<(), WireError> {
    let port = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(node)?;
    let write = port.try_clone()?;
    serve(port, write, store_root, workspace)
}

/// Serve one connection, whatever it arrived on.
///
/// Two handles and not one, because the two real transports differ exactly
/// there: a character device is one file opened twice, and a UNIX socket is one
/// stream split in two. Above this line neither is visible.
pub fn serve(
    mut input: impl Read,
    mut output: impl Write,
    store_root: &Path,
    workspace: &Path,
) -> Result<(), WireError> {
    // Opened per connection rather than held, so that a store which was not
    // mounted when the machine came up is picked up by the next agent instead of
    // being wrong for the life of the boot.
    let store = Store::open(store_root).map_err(|error| {
        WireError::Io(std::io::Error::other(format!(
            "the store at {} could not be opened: {error}",
            store_root.display()
        )))
    })?;

    let mut session = match ExternalAgentSession::open(workspace) {
        Ok(session) => session,
        // Said on the wire and not swallowed. A host that connected and got
        // silence cannot tell a machine with no workspace from a machine that is
        // not there — and the first is one command away from being fixed.
        Err(refusal) => {
            write_frame(
                &mut output,
                &FromThalyx::Error {
                    id: String::new(),
                    word: refusal.word.to_string(),
                    remedy: refusal.remedy.to_string(),
                    message: refusal.message,
                }
                .encode(),
            )?;
            return Ok(());
        }
    };

    write_frame(
        &mut output,
        &FromThalyx::Hello {
            protocol: thalyx_bridge::PROTOCOL,
            thalyx: env!("CARGO_PKG_VERSION").to_string(),
            workspace: session.workspace().display().to_string(),
            verbs: ExternalAgentSession::verbs(),
        }
        .encode(),
    )?;

    loop {
        let body = match read_frame(&mut input) {
            Ok(body) => body,
            // The ordinary end of a session. Never an error: an agent that
            // finished is not an agent that crashed, and a machine that reported
            // one as the other would make the journal useless for the times it
            // was the other.
            Err(WireError::Closed) => return Ok(()),
            Err(error) => return Err(error),
        };

        let answer = match ToThalyx::decode(&body) {
            Ok(ToThalyx::Request {
                id,
                verb,
                arguments,
            }) => answer_one(&mut session, &store, &id, &verb, &arguments),
            // Fail closed, and answer anyway. A malformed frame with no reply
            // leaves a host waiting on an id it will never see; one that is
            // refused by name tells it which of its messages was wrong.
            Err(error) => FromThalyx::Error {
                id: String::new(),
                word: "unintelligible".into(),
                remedy: "send_a_request".into(),
                message: error.to_string(),
            },
        };

        write_frame(&mut output, &answer.encode())?;
    }
}

/// Run one request through the confined session and turn it into a message.
fn answer_one(
    session: &mut ExternalAgentSession,
    store: &Store,
    id: &str,
    verb: &str,
    arguments: &[String],
) -> FromThalyx {
    // Asked before, not after. A session that had somehow been moved out of its
    // workspace must not answer one more question — and this is the check that
    // makes "somehow" survivable rather than fatal.
    if !session.still_confined() {
        return refused(
            id,
            Refusal {
                word: "outside_workspace",
                remedy: "reconnect",
                message: "this session is no longer standing in its workspace, so it \
                          will answer nothing else"
                    .into(),
            },
        );
    }

    let outcome = session.answer(store, verb, arguments);
    record(store, session, verb, arguments, &outcome);

    match outcome {
        Ok(answer) => FromThalyx::Response {
            id: id.to_string(),
            answer,
        },
        Err(refusal) => refused(id, refusal),
    }
}

fn refused(id: &str, refusal: Refusal) -> FromThalyx {
    FromThalyx::Error {
        id: id.to_string(),
        word: refusal.word.to_string(),
        remedy: refusal.remedy.to_string(),
        message: refusal.message,
    }
}

/// Write down what the external agent did, where a person can find it.
///
/// **Not everything.** A reading agent asks hundreds of questions and a journal
/// holding all of them is a journal nobody reads, which is the same as no
/// journal. Two things go in, and they are the two somebody would come looking
/// for: anything that **changed** the workspace, by the catalogue's own
/// `changes` flag, and any request that was **refused for leaving it** — the
/// second because a refused escape is exactly the entry whose absence would
/// matter.
///
/// The origin is `UntrustedContent`, which is the honest word: the line came
/// from a program on somebody's host, and `Marcado-de-Origen` exists so that
/// what the machine did on its own account and what it did on somebody else's
/// are not the same colour in the record.
fn record(
    store: &Store,
    session: &ExternalAgentSession,
    verb: &str,
    arguments: &[String],
    outcome: &Result<serde_json::Value, Refusal>,
) {
    let changes = crate::catalogue::VERBS
        .iter()
        .any(|entry| entry.id == verb && entry.changes);
    let escaped = matches!(outcome, Err(refusal) if refusal.word == "outside_workspace");
    if !changes && !escaped {
        return;
    }

    let Ok(journal) = thalyx_journal::Journal::open(store.journal_path()) else {
        return;
    };
    // `Rejected` and not a failure: nothing physical happened, which is exactly
    // what that variant is for and exactly what a refused escape is.
    let outcome_word = match outcome {
        Ok(_) => thalyx_journal::Outcome::Success,
        Err(refusal) => thalyx_journal::Outcome::Rejected {
            reason: format!("{}: {}", refusal.word, refusal.message),
        },
    };
    let notes = vec![
        format!("workspace {}", session.workspace().display()),
        format!("{verb} {}", arguments.join(" ")),
    ];

    let _ = journal.append(&thalyx_journal::Entry {
        timestamp: thalyx_journal::now(),
        // One operation name, so `historia` can be asked for exactly this and a
        // person can see what came from outside without reading everything.
        operation: "external_agent".into(),
        module_id: None,
        version: None,
        outcome: outcome_word,
        request_id: crate::new_request_id(),
        origin: thalyx_journal::Origin::UntrustedContent,
        snapshot: None,
        notes,
    });
}

/// Serve on a UNIX socket instead of a character device.
///
/// For a host, and it is not a convenience: it is what makes everything above
/// the transport testable in a container with no QEMU. The confinement, the
/// verbs, the framing and the answers are the same code either way — what a
/// socket cannot prove is that virtio-serial carries bytes, which is one claim
/// and is named as unproven where it is unproven.
pub fn listen(socket: &Path, store_root: &Path, workspace: &Path) -> std::io::Result<()> {
    // Removed first: a socket left by a process that was killed is a file the
    // bind refuses, and "address already in use" for a machine that is not
    // running is the least helpful error there is.
    let _ = std::fs::remove_file(socket);
    let listener = std::os::unix::net::UnixListener::bind(socket)?;
    println!("  agent bridge on {}", socket.display());
    println!("  workspace {}", workspace.display());

    for stream in listener.incoming() {
        let stream = stream?;
        let input = stream.try_clone()?;
        if let Err(error) = serve(input, stream, store_root, workspace) {
            println!("  the connection ended: {error}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_with_no_virtio_ports_at_all_finds_none() {
        // The ordinary machine, and the one property it must have: no error, no
        // wait, and nothing on the boot report.
        let empty = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            port_under(&empty.path().join("class/virtio-ports"), empty.path()),
            None
        );
    }

    #[test]
    fn a_port_is_found_by_its_name_and_never_by_its_number() {
        // Which `vportNpM` it is depends on what else QEMU was given, so the
        // number is not a fact about this machine. The name is.
        let root = tempfile::tempdir().expect("tempdir");
        let ports = root.path().join("class/virtio-ports");
        let dev = root.path().join("dev");
        std::fs::create_dir_all(ports.join("vport0p1")).expect("mk");
        std::fs::create_dir_all(ports.join("vport0p2")).expect("mk");
        std::fs::create_dir_all(&dev).expect("mk");
        std::fs::write(ports.join("vport0p1/name"), "org.qemu.guest_agent.0\n").expect("write");
        std::fs::write(ports.join("vport0p2/name"), format!("{PORT_NAME}\n")).expect("write");
        std::fs::write(dev.join("vport0p1"), "").expect("node");
        std::fs::write(dev.join("vport0p2"), "").expect("node");

        assert_eq!(
            port_under(&ports, &dev),
            Some(dev.join("vport0p2")),
            "the port was chosen by position rather than by name"
        );
    }

    #[test]
    fn a_port_with_no_name_at_all_does_not_hide_the_one_that_has_one() {
        // The unnamed port is `vport0p1` and ours is `vport0p2`, so it is read
        // first: this is the layout of a machine given any other virtio-serial
        // port beside Thalyx's, and the search used to stop dead at it.
        let root = tempfile::tempdir().expect("tempdir");
        let ports = root.path().join("class/virtio-ports");
        let dev = root.path().join("dev");
        std::fs::create_dir_all(ports.join("vport0p1")).expect("mk");
        std::fs::create_dir_all(ports.join("vport0p2")).expect("mk");
        std::fs::create_dir_all(&dev).expect("mk");
        // No `name` file for the first one, which is what the driver does for a
        // port nobody named — not an empty file, no file.
        std::fs::write(ports.join("vport0p2/name"), format!("{PORT_NAME}\n")).expect("write");
        std::fs::write(dev.join("vport0p1"), "").expect("node");
        std::fs::write(dev.join("vport0p2"), "").expect("node");

        assert_eq!(
            port_under(&ports, &dev),
            Some(dev.join("vport0p2")),
            "an unnamed port beside it hid the agent channel"
        );
    }

    #[test]
    fn a_named_port_with_no_device_node_is_not_a_port() {
        // Fail closed. A name in sysfs with nothing in /dev means the machine
        // came up without devtmpfs, and returning the path anyway would turn
        // that into a bridge thread failing forever on a file that is not there.
        let root = tempfile::tempdir().expect("tempdir");
        let ports = root.path().join("class/virtio-ports");
        let dev = root.path().join("dev");
        std::fs::create_dir_all(ports.join("vport0p1")).expect("mk");
        std::fs::create_dir_all(&dev).expect("mk");
        std::fs::write(ports.join("vport0p1/name"), format!("{PORT_NAME}\n")).expect("write");
        assert_eq!(port_under(&ports, &dev), None);
    }
}
