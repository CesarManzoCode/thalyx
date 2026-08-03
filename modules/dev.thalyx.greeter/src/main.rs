//! The first module.
//!
//! It exists to make one claim checkable: that a program written for Thalyx
//! runs on Thalyx and nowhere else. Not by asserting it — by having nothing to
//! talk to anywhere else.
//!
//! The one it replaces, `dev.thalyx.hola`, was a shell script. It was deleted
//! the day the distribution was removed, because a shell script is a program
//! for whichever system provides the shell, and that is the opposite of what
//! `vault/01-Filosofia/Filosofia-Fundacional.md` decrees. This one makes no
//! syscall of its own except on a socket it did not open.
//!
//! What it does, in order:
//!
//! 1. Picks up the channel Thalyx left on descriptor 3, and gives up if there
//!    is nothing there — which is what happens everywhere except inside Thalyx.
//! 2. Asks who it is. It does not know its own name; Thalyx does.
//! 3. Reads a file it was granted, and reports what it found. Which file is
//!    named on the command line, or — when nothing is — taken from the grants
//!    that came back in step 2. A module discovering what it may touch by
//!    asking is the arrangement working; being told by an argument is a
//!    convenience for whoever is testing it.
//! 4. Tries to read `/etc/shadow`, which it was not granted, and reports the
//!    refusal. A module that only ever did permitted things would demonstrate
//!    nothing about permissions.
//! 5. Tells the human what happened, over the channel rather than to a
//!    terminal, because a terminal is not something it has.

use thalyx_abi::{Channel, Denial, Level, Request, Response};

/// The file it asks for, relative to nothing: the manifest grants a directory
/// and this is the name inside it. Passed as an argument so the module carries
/// no assumption about where the human keeps anything.
const USAGE: &str = "greeter <path-to-read>";

fn main() -> std::process::ExitCode {
    let stream = match thalyx_syscall::inherited_channel() {
        Ok(stream) => stream,
        Err(error) => {
            // Deliberately to stderr, because there is no channel to say it on.
            // This is the message somebody sees if they run the binary by hand,
            // and it should say what is actually wrong rather than crash.
            eprintln!("greeter: {error}");
            eprintln!("greeter: this is a Thalyx module. It does not run on its own.");
            return std::process::ExitCode::from(64);
        }
    };

    let mut thalyx = Channel::new(stream);

    // Who am I? The module does not know and cannot know: its own identity
    // lives in a signed manifest it never sees.
    let identity = match thalyx.request(&Request::Identify) {
        Ok(Response::Identity(identity)) => {
            let _ = thalyx.request(&Request::Notify {
                level: Level::Info,
                text: format!(
                    "I am {} {}, speaking protocol {}, holding {} grant(s).",
                    identity.module_id,
                    identity.version,
                    identity.protocol,
                    identity.grants.len()
                ),
            });
            identity
        }
        other => return complain(&mut thalyx, "could not establish my own identity", other),
    };

    let Some(wanted) = std::env::args()
        .nth(1)
        .or_else(|| readable_grant(&identity))
    else {
        let _ = thalyx.request(&Request::Notify {
            level: Level::Error,
            text: format!("greeter: nothing to read, and nothing granted. Usage: {USAGE}"),
        });
        return std::process::ExitCode::from(64);
    };

    // Something it may do.
    match thalyx.request(&Request::ReadFile {
        path: wanted.clone(),
        offset: 0,
        len: 4096,
    }) {
        Ok(Response::Contents { bytes, eof }) => {
            let _ = thalyx.request(&Request::Notify {
                level: Level::Info,
                text: format!(
                    "read {} byte(s) from {wanted}{}: {}",
                    bytes.len(),
                    if eof { "" } else { " (more follows)" },
                    String::from_utf8_lossy(&bytes).trim_end()
                ),
            });
        }
        Ok(Response::Denied { reason }) => {
            let _ = thalyx.request(&Request::Notify {
                level: Level::Error,
                text: format!("I was refused {wanted}: {reason:?}. Check the manifest."),
            });
            return std::process::ExitCode::from(77);
        }
        other => return complain(&mut thalyx, &format!("could not read {wanted}"), other),
    }

    // Something it may not. The refusal is the point: without it, everything
    // above would look the same on a system that enforced nothing at all.
    match thalyx.request(&Request::ReadFile {
        path: "/etc/shadow".to_string(),
        offset: 0,
        len: 16,
    }) {
        Ok(Response::Denied {
            reason: Denial::NotGranted,
        }) => {
            let _ = thalyx.request(&Request::Notify {
                level: Level::Info,
                text: "I asked for /etc/shadow and was refused, which is correct.".to_string(),
            });
        }
        Ok(Response::Contents { .. }) => {
            // Reported rather than ignored. A module is not the right place to
            // enforce anything, but it is a fine place to notice that nobody
            // else did.
            let _ = thalyx.request(&Request::Notify {
                level: Level::Error,
                text: "I asked for /etc/shadow AND GOT IT. Enforcement is not working.".to_string(),
            });
            return std::process::ExitCode::from(1);
        }
        other => return complain(&mut thalyx, "asking for /etc/shadow", other),
    }

    std::process::ExitCode::SUCCESS
}

/// The first thing Thalyx said this module may read.
///
/// Used when nobody named a file. `net` is a grant too and is not a path, so
/// the leading `/` is checked rather than assumed — a module that sent `net` to
/// `ReadFile` would get a refusal it had earned and report it as a denial it
/// had not.
///
/// Nothing is opened here and nothing is stat'd: this module has no syscalls of
/// its own. If the grant names a directory, the read fails and the failure is
/// reported as what it is, which is why the manifest that ships in the image
/// grants the file and not the directory around it.
fn readable_grant(identity: &thalyx_abi::Identity) -> Option<String> {
    identity
        .grants
        .iter()
        .find(|grant| {
            grant.resource.starts_with('/') && matches!(grant.action.as_str(), "read" | "write")
        })
        .map(|grant| grant.resource.clone())
}

/// Say what went wrong over the channel, if the channel still works.
fn complain<T: std::io::Read + std::io::Write>(
    thalyx: &mut Channel<T>,
    what: &str,
    got: Result<Response, thalyx_abi::ChannelError>,
) -> std::process::ExitCode {
    let text = match got {
        Ok(response) => format!("{what}: unexpected answer {response:?}"),
        Err(error) => format!("{what}: {error}"),
    };
    // If this fails there is nothing left to try, and stderr may go nowhere.
    let _ = thalyx.request(&Request::Notify {
        level: Level::Error,
        text,
    });
    std::process::ExitCode::from(70)
}
