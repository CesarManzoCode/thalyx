//! The only surface a module can touch.
//!
//! Thalyx hands a module one open socket and nothing else. There is no shell
//! behind it, no libc to call into, and no second program to invoke — so every
//! interaction a module has with the system it runs on arrives as a message on
//! this channel. That is what makes a program written for Thalyx unable to run
//! anywhere else: elsewhere, there is nobody on the other end.
//!
//! The decree is `vault/02-Arquitectura/API-Interna-de-Modulos.md`.
//!
//! ## The shape
//!
//! ```text
//!   module                         Thalyx
//!     │   Request  (frame + CBOR)     │
//!     ├──────────────────────────────►│   checks the manifest's permissions
//!     │                               │   and does the work, or refuses
//!     │◄──────────────────────────────┤
//!     │   Response                    │
//! ```
//!
//! Strictly one answer per question, in order. Nothing is pushed at a module
//! that did not ask, which means a module never has to be ready to be
//! interrupted, and a stalled module cannot make Thalyx queue work for it.
//!
//! ## Where the channel comes from
//!
//! It is already open when the module starts, on [`CHANNEL_FD`]. A module does
//! not open it, name it, or find it — see the decree for why a path would be
//! the wrong answer. Turning that inherited descriptor into a socket needs
//! `unsafe`, so it lives in `thalyx-syscall`, which is the only crate allowed
//! any; this crate stays free of it and speaks over anything that reads and
//! writes.

pub mod frame;
pub mod message;

pub use frame::{FrameError, MAX_FRAME, MAX_READ};
pub use message::{
    CodecError, Denial, Failure, Grant, Identity, Level, PROTOCOL_VERSION, Request, Response,
};

use std::io::{Read, Write};

/// The descriptor a module finds its channel on.
///
/// Three, because zero through two are the standard streams and a module that
/// writes to its own stdout should not be writing to Thalyx by accident.
pub const CHANNEL_FD: i32 = 3;

#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error(transparent)]
    Frame(#[from] FrameError),

    #[error(transparent)]
    Codec(#[from] CodecError),

    /// The peer hung up where an answer was due.
    ///
    /// Distinct from a clean close, which only a server can legitimately see.
    /// A module whose question goes unanswered has lost the system, and saying
    /// so is better than returning something that looks like a refusal.
    #[error("the channel closed before an answer arrived")]
    NoAnswer,
}

/// A module's end of the channel.
pub struct Channel<T> {
    stream: T,
}

impl<T: Read + Write> Channel<T> {
    pub fn new(stream: T) -> Self {
        Self { stream }
    }

    /// Ask, and wait for the one answer.
    pub fn request(&mut self, request: &Request) -> Result<Response, ChannelError> {
        frame::write(&mut self.stream, &message::encode(request)?)?;
        match frame::read(&mut self.stream)? {
            Some(payload) => Ok(message::decode(&payload)?),
            None => Err(ChannelError::NoAnswer),
        }
    }

    pub fn into_inner(self) -> T {
        self.stream
    }
}

/// Thalyx's end: whatever actually does the work.
///
/// Implementations decide with the manifest's permissions in hand. The trait is
/// deliberately total — every request gets a response, including the ones being
/// refused — so that a module can never be left waiting on a question the
/// system chose not to dignify.
pub trait Handler {
    fn handle(&mut self, request: Request) -> Response;
}

/// Serve one module until it goes away.
///
/// Two kinds of bad input, treated differently on purpose:
///
/// - **A message that does not decode** is answered [`Response::Unsupported`]
///   and the connection continues. The framing is still in step, so the next
///   message is readable, and a module built against a newer protocol should
///   learn that rather than be cut off without a word.
/// - **A frame that does not parse** ends the connection. Once a length is
///   wrong there is no way to find where the next message begins, and guessing
///   would mean interpreting the peer's bytes at an offset nobody agreed on.
pub fn serve(
    stream: &mut (impl Read + Write),
    handler: &mut impl Handler,
) -> Result<(), ChannelError> {
    while let Some(payload) = frame::read(stream)? {
        let response = match message::decode::<Request>(&payload) {
            Ok(request) => handler.handle(request),
            Err(error) => Response::Unsupported {
                detail: error.to_string(),
            },
        };
        frame::write(stream, &message::encode(&response)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    /// A stand-in for Thalyx that models the one property under test: a grant
    /// covers a prefix and an action, and anything outside it is refused.
    ///
    /// It models it rather than approximating it, because a fake that answers
    /// every read is not a fake of a permission check — it is a different
    /// system, and a test against it would prove nothing about this one.
    struct Fake {
        grants: Vec<Grant>,
        files: Vec<(String, Vec<u8>)>,
        notified: Vec<String>,
    }

    impl Fake {
        fn covers(&self, path: &str, action: &str) -> Option<Result<(), Denial>> {
            let mut wrong_action = false;
            for grant in &self.grants {
                if !path.starts_with(&grant.resource) {
                    continue;
                }
                if grant.action != action {
                    wrong_action = true;
                    continue;
                }
                if grant.expires_unix == Some(0) {
                    return Some(Err(Denial::Expired));
                }
                return Some(Ok(()));
            }
            if wrong_action {
                return Some(Err(Denial::WrongAction));
            }
            None
        }
    }

    impl Handler for Fake {
        fn handle(&mut self, request: Request) -> Response {
            match request {
                Request::Identify => Response::Identity(Identity {
                    protocol: PROTOCOL_VERSION,
                    module_id: "dev.thalyx.demo".to_string(),
                    version: "1.0.0".to_string(),
                    grants: self.grants.clone(),
                }),

                Request::ReadFile { path, offset, len } => {
                    if len > MAX_READ {
                        return Response::Failed {
                            kind: Failure::TooLarge,
                            detail: format!("{len} is over the {MAX_READ} ceiling"),
                        };
                    }
                    match self.covers(&path, "read") {
                        None => Response::Denied {
                            reason: Denial::NotGranted,
                        },
                        Some(Err(reason)) => Response::Denied { reason },
                        Some(Ok(())) => match self.files.iter().find(|(name, _)| *name == path) {
                            None => Response::Failed {
                                kind: Failure::NotFound,
                                detail: path,
                            },
                            Some((_, content)) => {
                                let start = (offset as usize).min(content.len());
                                let end = (start + len as usize).min(content.len());
                                Response::Contents {
                                    bytes: content[start..end].to_vec(),
                                    eof: end == content.len(),
                                }
                            }
                        },
                    }
                }

                Request::WriteFile { path, bytes, .. } => match self.covers(&path, "write") {
                    None => Response::Denied {
                        reason: Denial::NotGranted,
                    },
                    Some(Err(reason)) => Response::Denied { reason },
                    Some(Ok(())) => Response::Written {
                        bytes: bytes.len() as u32,
                    },
                },

                Request::Notify { text, .. } => {
                    self.notified.push(text);
                    Response::Noted
                }
            }
        }
    }

    fn fake() -> Fake {
        Fake {
            grants: vec![
                Grant {
                    resource: "/home/user/projects".to_string(),
                    action: "read".to_string(),
                    expires_unix: None,
                },
                Grant {
                    resource: "/home/user/out".to_string(),
                    action: "write".to_string(),
                    expires_unix: None,
                },
                Grant {
                    resource: "/home/user/expired".to_string(),
                    action: "read".to_string(),
                    expires_unix: Some(0),
                },
            ],
            files: vec![(
                "/home/user/projects/notes.txt".to_string(),
                b"the vault is the authority".to_vec(),
            )],
            notified: Vec::new(),
        }
    }

    /// Run a conversation over a real socket pair, with the two halves in
    /// separate threads — the same shape the sandbox will have, so the test
    /// exercises the framing across a kernel buffer rather than a `Vec`.
    fn converse(requests: Vec<Request>) -> (Vec<Response>, Fake) {
        let (module_side, thalyx_side) = UnixStream::pair().expect("a socket pair");

        let server = std::thread::spawn(move || {
            let mut handler = fake();
            let mut stream = thalyx_side;
            serve(&mut stream, &mut handler).expect("serving");
            handler
        });

        let mut channel = Channel::new(module_side);
        let answers = requests
            .iter()
            .map(|request| channel.request(request).expect("asking"))
            .collect();

        // Dropping the module's end is how a module exits: the server sees a
        // clean close at a frame boundary and returns.
        drop(channel);
        (answers, server.join().expect("the server thread"))
    }

    #[test]
    fn a_module_can_ask_who_it_is_and_is_told_by_thalyx_not_by_itself() {
        let (answers, _) = converse(vec![Request::Identify]);

        match &answers[0] {
            Response::Identity(identity) => {
                assert_eq!(identity.module_id, "dev.thalyx.demo");
                assert_eq!(identity.protocol, PROTOCOL_VERSION);
                assert_eq!(identity.grants.len(), 3);
            }
            other => panic!("expected an identity, got {other:?}"),
        }
    }

    #[test]
    fn a_module_reads_a_file_its_permissions_cover() {
        let (answers, _) = converse(vec![Request::ReadFile {
            path: "/home/user/projects/notes.txt".to_string(),
            offset: 0,
            len: 64,
        }]);

        match &answers[0] {
            Response::Contents { bytes, eof } => {
                assert_eq!(bytes.as_slice(), b"the vault is the authority");
                assert!(*eof);
            }
            other => panic!("expected contents, got {other:?}"),
        }
    }

    #[test]
    fn a_module_is_refused_a_file_outside_its_permissions() {
        // The denial. On its own this proves nothing — see the baseline above,
        // which reads a file through the same code path and succeeds.
        let (answers, _) = converse(vec![Request::ReadFile {
            path: "/etc/shadow".to_string(),
            offset: 0,
            len: 64,
        }]);

        assert_eq!(
            answers[0],
            Response::Denied {
                reason: Denial::NotGranted
            }
        );
    }

    #[test]
    fn a_grant_for_the_wrong_action_says_so_rather_than_looking_ungranted() {
        // A module told only "not granted" when its manifest asked for read on
        // a path it may write would go looking for the wrong bug.
        let (answers, _) = converse(vec![Request::ReadFile {
            path: "/home/user/out/log.txt".to_string(),
            offset: 0,
            len: 8,
        }]);

        assert_eq!(
            answers[0],
            Response::Denied {
                reason: Denial::WrongAction
            }
        );
    }

    #[test]
    fn an_expired_grant_is_a_denial_and_not_an_absence() {
        let (answers, _) = converse(vec![Request::ReadFile {
            path: "/home/user/expired/old.txt".to_string(),
            offset: 0,
            len: 8,
        }]);

        assert_eq!(
            answers[0],
            Response::Denied {
                reason: Denial::Expired
            }
        );
    }

    #[test]
    fn a_permitted_file_that_is_not_there_fails_rather_than_being_denied() {
        // Rule 10 on the wire: a failure to read is not a failure to exist, and
        // a module must be able to tell which happened.
        let (answers, _) = converse(vec![Request::ReadFile {
            path: "/home/user/projects/absent.txt".to_string(),
            offset: 0,
            len: 8,
        }]);

        match &answers[0] {
            Response::Failed { kind, .. } => assert_eq!(*kind, Failure::NotFound),
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn a_read_larger_than_the_protocol_can_carry_is_refused_not_trimmed() {
        let (answers, _) = converse(vec![Request::ReadFile {
            path: "/home/user/projects/notes.txt".to_string(),
            offset: 0,
            len: MAX_READ + 1,
        }]);

        match &answers[0] {
            Response::Failed { kind, .. } => assert_eq!(*kind, Failure::TooLarge),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn several_questions_get_their_own_answers_in_order() {
        // The property the whole framing exists for: without it, a module would
        // read the answer to the previous question and act on it.
        let (answers, _) = converse(vec![
            Request::Identify,
            Request::ReadFile {
                path: "/etc/shadow".to_string(),
                offset: 0,
                len: 1,
            },
            Request::Notify {
                level: Level::Info,
                text: "done".to_string(),
            },
        ]);

        assert!(matches!(answers[0], Response::Identity(_)));
        assert!(matches!(answers[1], Response::Denied { .. }));
        assert_eq!(answers[2], Response::Noted);
    }

    #[test]
    fn what_a_module_notifies_reaches_thalyx_and_not_just_an_acknowledgement() {
        // Asking the system whether it worked proves nothing: the fake is
        // inspected afterwards for the text it was actually told.
        let (_, handler) = converse(vec![Request::Notify {
            level: Level::Warning,
            text: "the index is stale".to_string(),
        }]);

        assert_eq!(handler.notified, vec!["the index is stale".to_string()]);
    }

    #[test]
    fn a_message_this_version_cannot_read_is_answered_and_the_channel_survives() {
        let (module_side, thalyx_side) = UnixStream::pair().expect("a socket pair");
        let server = std::thread::spawn(move || {
            let mut handler = fake();
            let mut stream = thalyx_side;
            serve(&mut stream, &mut handler).expect("serving");
        });

        let mut stream = module_side;
        // Well-framed, and not a message this protocol has.
        frame::write(&mut stream, &[0xff, 0xff, 0xff]).expect("writing garbage");
        let answer: Response = message::decode(
            &frame::read(&mut stream)
                .expect("reading")
                .expect("an answer"),
        )
        .expect("decoding");
        assert!(matches!(answer, Response::Unsupported { .. }), "{answer:?}");

        // And the next question still works, which is the half that would be
        // lost by closing on an unreadable message.
        let mut channel = Channel::new(stream);
        assert!(matches!(
            channel.request(&Request::Identify).expect("asking again"),
            Response::Identity(_)
        ));

        drop(channel);
        server.join().expect("the server thread");
    }

    #[test]
    fn a_frame_whose_length_is_a_lie_ends_the_connection() {
        // The other half of the rule above. There is no resynchronising after a
        // bad length, so continuing would mean reading the peer's bytes at an
        // offset nobody agreed on.
        let (module_side, thalyx_side) = UnixStream::pair().expect("a socket pair");
        let server = std::thread::spawn(move || {
            let mut handler = fake();
            let mut stream = thalyx_side;
            serve(&mut stream, &mut handler)
        });

        let mut stream = module_side;
        stream
            .write_all(&u32::MAX.to_le_bytes())
            .expect("writing an absurd header");
        drop(stream);

        let outcome = server.join().expect("the server thread");
        assert!(
            matches!(
                outcome,
                Err(ChannelError::Frame(FrameError::TooLarge { .. }))
            ),
            "got {outcome:?}"
        );
    }

    #[test]
    fn a_module_whose_answer_never_comes_is_told_so() {
        // Not the same as a refusal: the system is gone, and a module that read
        // this as a denial would report the wrong thing to the human.
        let (module_side, thalyx_side) = UnixStream::pair().expect("a socket pair");
        drop(thalyx_side);

        let mut channel = Channel::new(module_side);
        let outcome = channel.request(&Request::Identify);
        assert!(
            matches!(
                outcome,
                Err(ChannelError::NoAnswer) | Err(ChannelError::Frame(FrameError::Io(_)))
            ),
            "got {outcome:?}"
        );
    }
}
