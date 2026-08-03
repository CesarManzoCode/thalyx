//! Getting one message off a socket without having understood it yet.
//!
//! A frame is a 32-bit little-endian length followed by exactly that many bytes
//! of CBOR. The length lives outside the message on purpose: reading a message
//! must not require interpreting it, so a corrupt or hostile frame is rejected
//! before any of its content has been handed to a parser.
//!
//! The other half of that is the ceiling. A frame announces its size before
//! anybody has read it, which makes an absurd size the cheapest way for a
//! module to exhaust Thalyx's memory — the peer says four gigabytes and the
//! reader dutifully allocates it. So the limit is part of the protocol rather
//! than something each caller remembers to apply.

use std::io::{Read, Write};

/// The largest frame either side will send or accept.
///
/// One mebibyte. Large enough that a file read is one round trip for anything a
/// module realistically reads at once, small enough that a peer cannot make
/// Thalyx allocate its way out of memory one frame at a time.
pub const MAX_FRAME: u32 = 1024 * 1024;

/// How many bytes of payload a single `ReadFile` may ask for.
///
/// Below [`MAX_FRAME`] with room to spare, because the answer travels as a
/// message with its own fields around the bytes. A cap that exactly equalled
/// the frame limit would make the largest legal request produce an illegal
/// response — a request that is accepted and then cannot be answered.
pub const MAX_READ: u32 = 512 * 1024;

/// If these two ever met, the largest request a module may legally make would
/// produce a response too large to send: accepted, then unanswerable, and the
/// symptom would be a dead channel rather than a refused request.
///
/// Checked at compile time rather than by a test, because the relationship is
/// a property of the two numbers and not of any run — a test could be deleted
/// along with the constant it guarded and nothing would notice.
const _: () = assert!(MAX_READ < MAX_FRAME);

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// The peer stopped mid-frame.
    ///
    /// Deliberately distinct from a clean close, which is not an error at all
    /// and comes back as `Ok(None)`. A module that exited between two messages
    /// and a module that was cut off halfway through one are different events,
    /// and a channel that reported them identically would make a crash look
    /// like a goodbye.
    #[error("the peer closed the connection in the middle of a frame")]
    Truncated,

    /// A length no legal sender would write.
    #[error("frame declares {declared} bytes, which is over the {MAX_FRAME} limit")]
    TooLarge { declared: u32 },

    /// No message encodes to nothing, so a zero length is corruption.
    #[error("frame declares zero bytes, which no message can be")]
    Empty,

    #[error("could not read or write the channel: {0}")]
    Io(#[from] std::io::Error),
}

/// Read one frame, or report that the peer went away between messages.
///
/// `Ok(None)` means the connection closed cleanly at a frame boundary. Every
/// other outcome is an error, including a close one byte later.
pub fn read(source: &mut impl Read) -> Result<Option<Vec<u8>>, FrameError> {
    let mut header = [0u8; 4];
    match read_exactly(source, &mut header)? {
        // Nothing at all: the peer is done, and that is not a failure.
        Filled::Nothing => return Ok(None),
        Filled::Partially => return Err(FrameError::Truncated),
        Filled::Completely => {}
    }

    let declared = u32::from_le_bytes(header);
    if declared == 0 {
        return Err(FrameError::Empty);
    }
    if declared > MAX_FRAME {
        return Err(FrameError::TooLarge { declared });
    }

    // Only now is an allocation of this size safe to make: the number has been
    // checked against the ceiling, so it is bounded by the protocol and not by
    // what the peer felt like claiming.
    let mut payload = vec![0u8; declared as usize];
    match read_exactly(source, &mut payload)? {
        Filled::Completely => Ok(Some(payload)),
        // A header that promised bytes which never arrived. Unlike the header
        // case, stopping here is never legitimate.
        Filled::Nothing | Filled::Partially => Err(FrameError::Truncated),
    }
}

/// Write one frame.
pub fn write(sink: &mut impl Write, payload: &[u8]) -> Result<(), FrameError> {
    let declared = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    if declared == 0 {
        return Err(FrameError::Empty);
    }
    if declared > MAX_FRAME {
        return Err(FrameError::TooLarge { declared });
    }

    sink.write_all(&declared.to_le_bytes())?;
    sink.write_all(payload)?;
    sink.flush()?;
    Ok(())
}

/// How much of a buffer a read managed to fill.
enum Filled {
    Nothing,
    Partially,
    Completely,
}

/// `read_exact`, but able to say that it read nothing rather than only that it
/// failed.
///
/// `std`'s version collapses both into `UnexpectedEof`, and the difference is
/// exactly the one this protocol needs: nothing means the peer finished, some
/// means it was cut off.
fn read_exactly(source: &mut impl Read, buffer: &mut [u8]) -> Result<Filled, FrameError> {
    let mut filled = 0;
    while filled < buffer.len() {
        match source.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            // A signal arrived mid-read. Nothing has gone wrong and the bytes
            // are still coming.
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(FrameError::Io(error)),
        }
    }

    Ok(if filled == 0 {
        Filled::Nothing
    } else if filled < buffer.len() {
        Filled::Partially
    } else {
        Filled::Completely
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_survives_a_round_trip() {
        let mut channel = Vec::new();
        write(&mut channel, b"hello").expect("writing a small frame");

        let read_back = read(&mut channel.as_slice()).expect("reading it back");
        assert_eq!(read_back.as_deref(), Some(b"hello".as_slice()));
    }

    #[test]
    fn several_frames_come_back_in_order_and_whole() {
        let mut channel = Vec::new();
        for payload in [b"one".as_slice(), b"two", b"three"] {
            write(&mut channel, payload).expect("writing");
        }

        let mut source = channel.as_slice();
        let mut seen = Vec::new();
        while let Some(frame) = read(&mut source).expect("reading") {
            seen.push(frame);
        }

        assert_eq!(
            seen,
            vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]
        );
    }

    #[test]
    fn a_peer_that_leaves_between_messages_is_not_an_error() {
        let empty: &[u8] = &[];
        let outcome = read(&mut { empty }).expect("a clean close is not a failure");
        assert!(outcome.is_none());
    }

    #[test]
    fn a_peer_that_leaves_inside_a_message_is_an_error() {
        // A header promising ten bytes, followed by three.
        let mut channel = 10u32.to_le_bytes().to_vec();
        channel.extend_from_slice(b"abc");

        let error = read(&mut channel.as_slice()).expect_err("a cut-off frame must fail");
        assert!(matches!(error, FrameError::Truncated), "got {error:?}");
    }

    #[test]
    fn a_header_that_stops_halfway_is_an_error_and_not_a_goodbye() {
        // Two bytes of a four-byte length. This is the case `read_exact` would
        // have reported the same way as an empty stream.
        let error = read(&mut [0u8, 0].as_slice()).expect_err("half a header must fail");
        assert!(matches!(error, FrameError::Truncated), "got {error:?}");
    }

    #[test]
    fn an_enormous_declared_length_is_refused_before_anything_is_allocated() {
        // Four gigabytes announced, and not one byte of payload behind it. If
        // the ceiling were checked after allocating, this test would not
        // return — which is the whole point of it.
        let channel = u32::MAX.to_le_bytes();

        let error = read(&mut channel.as_slice()).expect_err("an absurd length must be refused");
        assert!(
            matches!(error, FrameError::TooLarge { declared } if declared == u32::MAX),
            "got {error:?}"
        );
    }

    #[test]
    fn a_length_one_byte_over_the_ceiling_is_refused() {
        // The control for the test above: without it, a ceiling of zero would
        // also pass, and so would one that rejected everything.
        let channel = (MAX_FRAME + 1).to_le_bytes();
        let error = read(&mut channel.as_slice()).expect_err("over the limit must be refused");
        assert!(
            matches!(error, FrameError::TooLarge { .. }),
            "got {error:?}"
        );
    }

    #[test]
    fn a_frame_exactly_at_the_ceiling_is_accepted() {
        // The other control. A limit that also rejected the largest legal frame
        // would look identical in every test above.
        let payload = vec![0x41u8; MAX_FRAME as usize];
        let mut channel = Vec::new();
        write(&mut channel, &payload).expect("the largest legal frame must be writable");

        let read_back = read(&mut channel.as_slice()).expect("and readable");
        assert_eq!(read_back.map(|frame| frame.len()), Some(MAX_FRAME as usize));
    }

    #[test]
    fn a_zero_length_frame_is_corruption_and_not_an_empty_message() {
        let channel = 0u32.to_le_bytes();
        let error = read(&mut channel.as_slice()).expect_err("zero bytes is not a message");
        assert!(matches!(error, FrameError::Empty), "got {error:?}");
    }
}
