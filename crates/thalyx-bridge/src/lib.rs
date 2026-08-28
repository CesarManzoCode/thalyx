//! The wire between a program outside a Thalyx machine and the machine itself.
//!
//! `vault/07-Adopcion-y-Fases/Agentes-Externos.md` decrees the arrangement this
//! serves: during the adoption phase the programming agent runs on the host and
//! Thalyx is the machine it works *on*, reached through one local channel. This
//! crate is that channel's grammar and nothing else — no verbs, no paths, no
//! policy. Both ends link it so that there is one definition of a frame rather
//! than two that agree until they do not.
//!
//! ## What it deliberately does not know
//!
//! **MCP.** The decree is that MCP is an adapter and not an interface: it lives
//! entirely in `thalyx-mcp` on the host, and Thalyx's own surface is the
//! authority. If this file ever mentions a tool, a schema or a client, the
//! adapter has leaked into the machine.
//!
//! **The transport.** A frame is bytes on something that implements `Read` or
//! `Write`. Today that is a virtio-serial port inside the guest and a UNIX
//! socket on the host; it could be a pipe, and the two tests below run it over
//! one. Nothing here opens anything.
//!
//! ## Why a length prefix and not a line
//!
//! Because an answer carries file contents. A newline-delimited protocol makes
//! every payload a place where the framing can be forged from inside the data —
//! read a file with a newline in it and the reader sees two messages. The
//! project has already written down what that costs, on 2026-08-08, when the
//! agent's prompt had a marker for where an answer began and nothing for where
//! it ended: **a boundary defined on one side only is not a boundary.** A count
//! of bytes is a boundary defined on both.
//!
//! Four bytes, little-endian, then exactly that many bytes of UTF-8 JSON. The
//! count comes first so a reader knows how much to wait for before it has read
//! anything it would have to guess about.
//!
//! ## Fail closed
//!
//! Rule 9 of `CLAUDE.md`. A length past [`MAX_FRAME`] is refused **without
//! reading the body**, because reading it is the denial of service; a body that
//! is not UTF-8, or not JSON, or JSON of a shape this version has no name for,
//! is an error and never a guess. A reader that repaired a malformed frame
//! would be inventing a request that nobody made, on a machine that would then
//! carry it out.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// The version both ends must agree on before anything else is sent.
///
/// Bumped when the meaning of an existing field changes, never for a field that
/// is added: an old host reading a new machine's answer ignores what it does not
/// know, and that is the whole reason the payload is JSON rather than a struct.
pub const PROTOCOL: u32 = 1;

/// The largest frame either end will read or write, in bytes.
///
/// Four mebibytes, which is sixty-four times the largest excerpt `leer` will
/// ever produce ([`thalyx_files::EXCERPT`] is 64 kB) and still small enough that
/// a malformed length cannot make either end allocate its way out of memory. A
/// frame that would exceed it is refused rather than truncated: a truncated
/// answer is a wrong answer that looks like a right one.
pub const MAX_FRAME: usize = 4 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    /// The other end closed cleanly between frames.
    ///
    /// Its own variant and not an I/O error, because it is the ordinary way a
    /// session ends and the endpoint must not report it as a fault. A bridge
    /// that logged "the agent crashed" every time an agent finished would make
    /// the log useless for the times one did.
    #[error("the other end closed the channel")]
    Closed,

    #[error("the channel failed: {0}")]
    Io(#[from] std::io::Error),

    #[error(
        "a frame announced {announced} bytes and the limit is {MAX_FRAME}; \
         nothing was read past the length"
    )]
    TooLarge { announced: u64 },

    #[error("a frame's body is not UTF-8, so it is not a message this speaks")]
    NotText,

    #[error("a frame's body is not a message this understands: {0}")]
    Unintelligible(String),
}

/// Write one message.
///
/// The length and the body go out in one `write_all` rather than two, so a
/// reader on the far end never sees a length with no body behind it — which on a
/// character device is a reader blocked forever on bytes that are still in this
/// process.
pub fn write_frame(out: &mut impl Write, body: &[u8]) -> Result<(), WireError> {
    if body.len() > MAX_FRAME {
        return Err(WireError::TooLarge {
            announced: body.len() as u64,
        });
    }
    let mut framed = Vec::with_capacity(4 + body.len());
    framed.extend_from_slice(&(body.len() as u32).to_le_bytes());
    framed.extend_from_slice(body);
    out.write_all(&framed)?;
    out.flush()?;
    Ok(())
}

/// Read one message, or say which of the three ways it did not arrive.
///
/// Rule 10 of `CLAUDE.md` on the wire: closed, unreadable and malformed are
/// three different facts and each sends whoever reads them somewhere different.
pub fn read_frame(input: &mut impl Read) -> Result<Vec<u8>, WireError> {
    let mut length = [0u8; 4];
    match input.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(WireError::Closed);
        }
        Err(error) => return Err(WireError::Io(error)),
    }

    let announced = u32::from_le_bytes(length) as u64;
    if announced > MAX_FRAME as u64 {
        // Refused before a single byte of the body is read. Draining it first
        // to "resynchronise" is what turns a wrong number into an attack: the
        // number is the only thing saying how much to drain.
        return Err(WireError::TooLarge { announced });
    }

    let mut body = vec![0u8; announced as usize];
    match input.read_exact(&mut body) {
        Ok(()) => Ok(body),
        // A length with no body behind it is not a clean close. The sender
        // promised bytes it did not send, and treating that as the end of a
        // session would hide a truncating transport.
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Err(WireError::Io(
            std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "the frame was cut short"),
        )),
        Err(error) => Err(WireError::Io(error)),
    }
}

/// What the machine says.
///
/// `id` is echoed from the request it answers and is the only thing tying the
/// two together: v0 answers one request at a time, and the field exists so that
/// a future that does not is a change to the endpoint rather than to the wire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromThalyx {
    /// Sent unasked, once, as soon as something connects.
    ///
    /// It carries what a host needs before it can ask anything: the version, the
    /// workspace the session is confined to, and the verbs that session will
    /// accept. The full catalogue is a `describe` away and deliberately not in
    /// here — one small message that always arrives beats one large one that
    /// sometimes does not fit.
    Hello {
        protocol: u32,
        /// The version of `thalyx` inside the machine.
        thalyx: String,
        /// The absolute path, inside the machine, that this session cannot
        /// leave. Said so the host can show it and so a mismatch with what the
        /// host imported is visible rather than mysterious.
        workspace: String,
        /// The verb ids this session will accept, from Thalyx's own catalogue.
        verbs: Vec<String>,
    },
    /// One request, answered. `answer` is a structured answer of Thalyx's own
    /// surface, passed through byte for byte — the bridge never rewords one.
    Response {
        id: String,
        answer: serde_json::Value,
    },
    /// The request never reached a verb.
    ///
    /// Distinct from a `Response` carrying a refusal, and the distinction is the
    /// one that matters to a caller: a refusal is Thalyx answering, this is
    /// Thalyx declining to ask. `word` and `remedy` follow punto **A2** of
    /// `Superficie-para-el-LLM.md` — an error that names the way out.
    Error {
        id: String,
        word: String,
        remedy: String,
        message: String,
    },
}

/// What the host says. One shape, because there is one thing to say.
///
/// **A verb and its arguments, never a line.** The endpoint composes the line
/// itself, quoting each argument, so an argument cannot become a second verb —
/// the shell-injection shape, closed by construction rather than by escaping at
/// the far end. It is also what lets the endpoint know which arguments are paths
/// without parsing prose: the position is the meaning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToThalyx {
    Request {
        id: String,
        /// A verb id from Thalyx's catalogue — `read`, `symbol`, `attempt`.
        /// Never one of the words a person types: those are translations and
        /// this is an interface.
        verb: String,
        #[serde(default)]
        arguments: Vec<String>,
    },
}

impl FromThalyx {
    pub fn encode(&self) -> Vec<u8> {
        // Cannot fail: every variant is a struct of owned strings and a
        // `serde_json::Value` that was itself parsed from JSON.
        serde_json::to_vec(self).expect("a message of this crate is always JSON")
    }

    pub fn decode(body: &[u8]) -> Result<Self, WireError> {
        decode(body)
    }

    /// The id this is about, for a caller matching answers to questions.
    pub fn id(&self) -> Option<&str> {
        match self {
            FromThalyx::Hello { .. } => None,
            FromThalyx::Response { id, .. } | FromThalyx::Error { id, .. } => Some(id),
        }
    }
}

impl ToThalyx {
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a message of this crate is always JSON")
    }

    pub fn decode(body: &[u8]) -> Result<Self, WireError> {
        decode(body)
    }
}

fn decode<T: for<'a> Deserialize<'a>>(body: &[u8]) -> Result<T, WireError> {
    let text = std::str::from_utf8(body).map_err(|_| WireError::NotText)?;
    serde_json::from_str(text).map_err(|error| WireError::Unintelligible(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pipe of bytes, which is what both real transports are.
    fn round_trip(messages: &[Vec<u8>]) -> Vec<Vec<u8>> {
        let mut wire = Vec::new();
        for body in messages {
            write_frame(&mut wire, body).expect("write");
        }
        let mut reader = wire.as_slice();
        let mut back = Vec::new();
        loop {
            match read_frame(&mut reader) {
                Ok(body) => back.push(body),
                Err(WireError::Closed) => break,
                Err(error) => panic!("{error}"),
            }
        }
        back
    }

    #[test]
    fn a_body_holding_newlines_survives_the_wire_whole() {
        // The reason the framing is a length and not a line. A file read out of
        // the machine is the ordinary payload here, and every file has newlines.
        let body = b"{\"text\":\"one\\ntwo\\nthree\"}\nnot a second message\n".to_vec();
        assert_eq!(round_trip(std::slice::from_ref(&body)), vec![body]);
    }

    #[test]
    fn several_frames_come_back_as_several_and_in_order() {
        let bodies: Vec<Vec<u8>> = (0..5)
            .map(|n| format!("{{\"n\":{n}}}").into_bytes())
            .collect();
        assert_eq!(round_trip(&bodies), bodies);
    }

    #[test]
    fn an_empty_body_is_a_frame_and_not_an_end_of_stream() {
        // Zero is a legal length, and a reader that treated it as the close
        // would hang up on a message rather than answer it.
        assert_eq!(round_trip(&[Vec::new()]), vec![Vec::<u8>::new()]);
    }

    #[test]
    fn a_clean_close_between_frames_is_not_an_error() {
        let mut empty: &[u8] = &[];
        assert!(matches!(read_frame(&mut empty), Err(WireError::Closed)));
    }

    #[test]
    fn a_length_with_no_body_behind_it_is_a_fault_and_not_a_close() {
        // The difference decides what whoever reads this does next: one means
        // the agent finished, the other means the channel is losing bytes.
        let mut cut: &[u8] = &[10, 0, 0, 0, b'{'];
        let error = read_frame(&mut cut).expect_err("a cut frame is not a message");
        assert!(matches!(error, WireError::Io(_)), "{error}");
    }

    #[test]
    fn an_impossible_length_is_refused_without_reading_the_body() {
        // The body is never allocated: the number claiming four gigabytes is
        // the only thing saying there are four gigabytes to read.
        let mut absurd: &[u8] = &[0xff, 0xff, 0xff, 0xff];
        match read_frame(&mut absurd) {
            Err(WireError::TooLarge { announced }) => assert_eq!(announced, u32::MAX as u64),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_body_that_is_not_utf8_is_named_as_that_and_not_as_bad_json() {
        // Rule 10 again. "The bytes are not text" and "the text is not a
        // message" send whoever reads them to two different places.
        assert!(matches!(
            ToThalyx::decode(&[0xff, 0xfe]),
            Err(WireError::NotText)
        ));
    }

    #[test]
    fn a_message_of_a_shape_this_version_has_no_name_for_is_refused() {
        let error = ToThalyx::decode(br#"{"type":"execute","program":"/bin/sh"}"#)
            .expect_err("an unknown type is not a request");
        assert!(matches!(error, WireError::Unintelligible(_)), "{error}");
    }

    #[test]
    fn a_request_survives_the_round_trip_with_its_arguments_intact() {
        let request = ToThalyx::Request {
            id: "r1".into(),
            verb: "read".into(),
            // A name with a space and a quote in it: the two things a line-based
            // protocol would have to escape and could get wrong.
            arguments: vec!["a file's name.txt".into()],
        };
        assert_eq!(ToThalyx::decode(&request.encode()).unwrap(), request);
    }

    #[test]
    fn an_answer_is_carried_through_without_being_reworded() {
        let answer = serde_json::json!({"op": "read", "ok": true, "text": "x\ny"});
        let response = FromThalyx::Response {
            id: "r1".into(),
            answer: answer.clone(),
        };
        match FromThalyx::decode(&response.encode()).unwrap() {
            FromThalyx::Response { answer: back, .. } => assert_eq!(back, answer),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_frame_larger_than_the_limit_is_refused_at_the_writer_too() {
        // Both ends, because the reader's refusal alone would let one end
        // produce a message the other can only ever fail on — and the failure
        // would land on the far side of the machine from the bug.
        let huge = vec![b'x'; MAX_FRAME + 1];
        assert!(matches!(
            write_frame(&mut Vec::new(), &huge),
            Err(WireError::TooLarge { .. })
        ));
    }
}
