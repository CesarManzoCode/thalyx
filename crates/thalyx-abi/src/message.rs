//! What a module can say, and what Thalyx can answer.
//!
//! The list is short on purpose. Every operation here is permanent attack
//! surface — a module speaks to nothing else, so anything reachable from this
//! enum is the whole of what a module can reach — and it is far easier to add
//! an operation than to take one away once something depends on it.
//!
//! See `vault/02-Arquitectura/API-Interna-de-Modulos.md`, which is the decree
//! this file implements.

use serde::{Deserialize, Serialize};

/// The version of this protocol.
///
/// Answered by [`Request::Identify`] rather than negotiated up front. A module
/// that needs to know asks; one that does not care never pays for a handshake.
pub const PROTOCOL_VERSION: u32 = 1;

/// Everything a module may ask for.
///
/// `deny_unknown_fields`, and the reason is not tidiness. A field this version
/// does not know about is a request written against a protocol this Thalyx does
/// not implement, and silently ignoring it would answer a question nobody
/// asked — the newer side believes it constrained the operation, the older side
/// never saw the constraint. On a channel that governs permissions, that is the
/// wrong direction to fail in.
///
/// It sits on the enum rather than on each variant, which is the only place
/// serde accepts it. That it still reaches the variants is not taken on faith:
/// `a_known_request_carrying_an_unknown_field_is_refused_and_not_trimmed` sends
/// a `ReadFile` with an extra field and requires it to be rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Request {
    /// Who am I, and what am I allowed to do?
    Identify,

    /// Read from a file inside what this module was granted.
    ReadFile {
        /// UTF-8 only. Paths on this wire are text, so a name that is not valid
        /// UTF-8 cannot be expressed rather than travelling as bytes that each
        /// side might interpret differently.
        path: String,
        offset: u64,
        /// Capped by [`crate::frame::MAX_READ`]; a larger ask is refused, not
        /// silently trimmed, so a module never mistakes a short answer for the
        /// end of a file.
        len: u32,
    },

    /// Write into a file inside what this module was granted.
    WriteFile {
        path: String,
        offset: u64,
        bytes: Vec<u8>,
    },

    /// Say something to the human.
    Notify { level: Level, text: String },
}

/// How much the human is being asked to care.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Level {
    Info,
    Warning,
    Error,
}

/// Everything Thalyx may answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    Identity(Identity),

    /// Bytes, and whether the file ended inside this answer.
    ///
    /// `eof` is carried explicitly because a short read and the end of a file
    /// are different events that produce the same byte count, and a module that
    /// could not tell them apart would either loop forever or stop early.
    Contents {
        bytes: Vec<u8>,
        eof: bool,
    },

    Written {
        bytes: u32,
    },

    Noted,

    /// The operation is understood, well-formed, and not allowed.
    Denied {
        reason: Denial,
    },

    /// The operation is allowed and did not work.
    ///
    /// Split from [`Response::Denied`] deliberately. "You may not read this"
    /// and "this could not be read" are different facts about the world, and a
    /// module told only that it failed would report a missing disk as a
    /// permission problem, or a permission problem as a missing file.
    Failed {
        kind: Failure,
        detail: String,
    },

    /// The request did not decode, or names something this version has no
    /// implementation for.
    Unsupported {
        detail: String,
    },
}

/// Why an allowed-looking operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Denial {
    /// No grant covers this at all.
    NotGranted,
    /// A grant covered it and no longer does.
    Expired,
    /// A grant covers this path for a different action — read where write was
    /// asked, or the reverse. Named separately because it is the one denial
    /// that tells a module its manifest is wrong rather than its luck.
    WrongAction,
}

/// Why something that was permitted still did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Failure {
    /// It is not there.
    NotFound,
    /// It is there and could not be read.
    Unreadable,
    /// It is there and could not be written.
    Unwritable,
    /// The ask was larger than the protocol can carry.
    TooLarge,
}

/// What Thalyx knows about the module asking.
///
/// None of it comes from the module. The identifier and version come from the
/// verified manifest and the grants from the permission store, which are the
/// two things a module cannot write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub protocol: u32,
    pub module_id: String,
    pub version: String,
    pub grants: Vec<Grant>,
}

/// One permission, in the vocabulary of the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    /// `net`, or a path. The same string the manifest carried.
    pub resource: String,
    /// `outbound`, `read`, `write`.
    pub action: String,
    /// Unix seconds, or absent for a persistent grant.
    pub expires_unix: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("could not encode a message: {0}")]
    Encode(String),
    #[error("could not decode a message: {0}")]
    Decode(String),
}

/// Turn a message into the bytes of one frame's payload.
pub fn encode<T: Serialize>(message: &T) -> Result<Vec<u8>, CodecError> {
    let mut bytes = Vec::new();
    ciborium::into_writer(message, &mut bytes).map_err(|e| CodecError::Encode(e.to_string()))?;
    Ok(bytes)
}

/// Turn one frame's payload back into a message.
///
/// Anything unrecognised fails here rather than arriving as a default. A
/// request this version cannot name is not an empty request.
pub fn decode<T: for<'de> Deserialize<'de>>(payload: &[u8]) -> Result<T, CodecError> {
    ciborium::from_reader(payload).map_err(|e| CodecError::Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_request_survives_a_round_trip() {
        let messages = vec![
            Request::Identify,
            Request::ReadFile {
                path: "/home/user/projects/notes.txt".to_string(),
                offset: 4096,
                len: 512,
            },
            Request::WriteFile {
                path: "/home/user/projects/out.txt".to_string(),
                offset: 0,
                bytes: vec![1, 2, 3],
            },
            Request::Notify {
                level: Level::Warning,
                text: "the index is stale".to_string(),
            },
        ];

        for message in messages {
            let bytes = encode(&message).expect("encoding");
            let back: Request = decode(&bytes).expect("decoding");
            assert_eq!(back, message);
        }
    }

    #[test]
    fn every_response_survives_a_round_trip() {
        let messages = vec![
            Response::Identity(Identity {
                protocol: PROTOCOL_VERSION,
                module_id: "org.publisher.pyassist".to_string(),
                version: "2.3.1".to_string(),
                grants: vec![Grant {
                    resource: "/home/user/projects".to_string(),
                    action: "read".to_string(),
                    expires_unix: None,
                }],
            }),
            Response::Contents {
                bytes: vec![9, 9, 9],
                eof: true,
            },
            Response::Written { bytes: 3 },
            Response::Noted,
            Response::Denied {
                reason: Denial::Expired,
            },
            Response::Failed {
                kind: Failure::NotFound,
                detail: "no such file".to_string(),
            },
            Response::Unsupported {
                detail: "unknown request".to_string(),
            },
        ];

        for message in messages {
            let bytes = encode(&message).expect("encoding");
            let back: Response = decode(&bytes).expect("decoding");
            assert_eq!(back, message);
        }
    }

    #[test]
    fn a_request_this_version_cannot_name_fails_to_decode() {
        // What a module built against a later protocol would send. It must not
        // arrive as some nearby variant that happens to parse.
        #[derive(Serialize)]
        enum Later {
            ExecuteCommand { argv: Vec<String> },
        }

        let bytes = encode(&Later::ExecuteCommand {
            argv: vec!["sh".to_string()],
        })
        .expect("encoding the newer message");

        let outcome: Result<Request, _> = decode(&bytes);
        assert!(
            outcome.is_err(),
            "an unknown request decoded into {outcome:?}"
        );
    }

    #[test]
    fn a_known_request_carrying_an_unknown_field_is_refused_and_not_trimmed() {
        // The dangerous shape: a later protocol adds a constraint to an
        // operation this version already has. Ignoring the field would run the
        // operation without the constraint its sender believed it had applied.
        #[derive(Serialize)]
        enum Later {
            ReadFile {
                path: String,
                offset: u64,
                len: u32,
                follow_symlinks: bool,
            },
        }

        let bytes = encode(&Later::ReadFile {
            path: "/home/user/projects/notes.txt".to_string(),
            offset: 0,
            len: 16,
            follow_symlinks: true,
        })
        .expect("encoding");

        let outcome: Result<Request, _> = decode(&bytes);
        assert!(
            outcome.is_err(),
            "an unknown field was silently dropped, leaving {outcome:?}"
        );
    }

    #[test]
    fn a_denial_and_a_failure_do_not_decode_into_each_other() {
        // The two carry the distinction the whole answer rests on: may not,
        // versus could not. If either could be mistaken for the other, a module
        // would misreport why it stopped.
        let denied = encode(&Response::Denied {
            reason: Denial::NotGranted,
        })
        .expect("encoding");
        let failed = encode(&Response::Failed {
            kind: Failure::NotFound,
            detail: String::new(),
        })
        .expect("encoding");

        assert_ne!(denied, failed);
        assert!(matches!(
            decode::<Response>(&denied).expect("decoding"),
            Response::Denied { .. }
        ));
        assert!(matches!(
            decode::<Response>(&failed).expect("decoding"),
            Response::Failed { .. }
        ));
    }

    #[test]
    fn garbage_does_not_decode_into_a_request() {
        let outcome: Result<Request, _> = decode(&[0xff, 0xff, 0xff, 0xff]);
        assert!(outcome.is_err(), "garbage decoded into {outcome:?}");
    }
}
