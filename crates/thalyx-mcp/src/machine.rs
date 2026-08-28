//! The Thalyx machine, as this adapter talks to it.
//!
//! One socket, one request at a time, and no interpretation. Everything this
//! knows about Thalyx is `thalyx-bridge`, which the machine links too — so the
//! definition of a frame is one definition, and a version skew is a version skew
//! rather than two ends silently disagreeing about a length.

use std::io::{BufReader, BufWriter};
use std::os::unix::net::UnixStream;
use std::path::Path;
use thalyx_bridge::{FromThalyx, ToThalyx, WireError, read_frame, write_frame};

/// What the machine said about itself when this connected.
#[derive(Debug, Clone)]
pub struct Greeting {
    pub thalyx: String,
    pub workspace: String,
    pub verbs: Vec<String>,
}

pub struct Machine {
    input: BufReader<UnixStream>,
    output: BufWriter<UnixStream>,
    greeting: Greeting,
    /// Counts up, so two answers can never be matched to the wrong question even
    /// though only one is ever in flight. It is what would catch a machine that
    /// answered late, which is the failure a request id exists for.
    next: u64,
}

#[derive(Debug)]
pub enum Trouble {
    /// The machine is not there, or stopped being there.
    Channel(String),
    /// The machine answered, and the answer is a refusal.
    Refused {
        word: String,
        remedy: String,
        message: String,
    },
}

impl std::fmt::Display for Trouble {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trouble::Channel(why) => write!(f, "{why}"),
            Trouble::Refused {
                word,
                remedy,
                message,
            } => write!(f, "{word} ({remedy}): {message}"),
        }
    }
}

impl Machine {
    /// Connect and read the machine's hello.
    ///
    /// The hello is waited for rather than assumed, and that wait is the useful
    /// part: a socket QEMU is holding open for a guest that has not started its
    /// bridge accepts a connection and answers nothing, so a client that did not
    /// wait would advertise a full set of tools against a machine that has none.
    pub fn connect(socket: &Path, wait: std::time::Duration) -> Result<Self, Trouble> {
        // Retried rather than refused on the first try, and the reason is what
        // the ordinary sequence looks like: `make -C image run-agent` creates
        // the socket the instant QEMU starts, and the machine behind it takes
        // twenty seconds to mount its store and reach its session. A client that
        // gave up immediately would fail for every person who started the two in
        // the order the README puts them in.
        let until = std::time::Instant::now() + wait;
        let stream = loop {
            match UnixStream::connect(socket) {
                Ok(stream) => break stream,
                Err(error) if std::time::Instant::now() < until => {
                    let _ = error;
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
                Err(error) => {
                    return Err(Trouble::Channel(format!(
                        "no Thalyx machine at {} after waiting {}s: {error}. Is the VM \
                         booted with the agent channel — `make -C image run-agent`?",
                        socket.display(),
                        wait.as_secs()
                    )));
                }
            }
        };
        let output = stream
            .try_clone()
            .map_err(|error| Trouble::Channel(error.to_string()))?;

        // The wait budget has to cover the hello as well, and it did not.
        //
        // `wait=off` means QEMU is holding the socket open from the instant the
        // VM starts, so the connect above succeeds on the first try, every time,
        // and the loop's whole budget is spent on the one thing that is never
        // slow. What is slow is the guest: the socket accepts and then says
        // nothing until Thalyx reaches its session and starts the bridge
        // thread. With no deadline on the read, a client that started before the
        // machine did — the ordinary order, and the one the README puts the
        // commands in — waited on it forever with nothing on the screen.
        let remaining = until.saturating_duration_since(std::time::Instant::now());
        stream
            .set_read_timeout(Some(remaining.max(std::time::Duration::from_secs(1))))
            .map_err(|error| Trouble::Channel(error.to_string()))?;

        let mut machine = Self {
            input: BufReader::new(stream),
            output: BufWriter::new(output),
            greeting: Greeting {
                thalyx: String::new(),
                workspace: String::new(),
                verbs: Vec::new(),
            },
            next: 0,
        };

        // Matched on the error's **kind** and never on its text. A read that
        // hits `SO_RCVTIMEO` on Linux comes back as `WouldBlock` — "Resource
        // temporarily unavailable (os error 11)" — with the words "timed out"
        // nowhere in it, so a check written against the sentence would have
        // reported the one state this deadline exists to name as an unexplained
        // channel failure. Rule 5.
        let first = match machine.read_wire() {
            Ok(message) => message,
            Err(WireError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(Trouble::Channel(format!(
                    "the socket at {} is there and the machine never said hello, after {}s. \
                     It is still booting, or it came up without a workspace — its own boot \
                     report says which, on the QEMU console",
                    socket.display(),
                    wait.as_secs()
                )));
            }
            Err(other) => return Err(channel(other)),
        };

        // Off again for everything after the hello. A deadline that made sense
        // for a machine that had not booted yet would, left on, turn a model
        // thinking between two tool calls into a channel that failed.
        machine
            .input
            .get_ref()
            .set_read_timeout(None)
            .map_err(|error| Trouble::Channel(error.to_string()))?;

        match first {
            FromThalyx::Hello {
                protocol,
                thalyx,
                workspace,
                verbs,
            } => {
                if protocol != thalyx_bridge::PROTOCOL {
                    // Refused rather than tried. Two ends that disagree about
                    // the protocol and carry on anyway is how a "successful"
                    // edit lands in the wrong file.
                    return Err(Trouble::Channel(format!(
                        "the machine speaks protocol {protocol} and this speaks {}. \
                         One of the two is from another version of Thalyx",
                        thalyx_bridge::PROTOCOL
                    )));
                }
                machine.greeting = Greeting {
                    thalyx,
                    workspace,
                    verbs,
                };
            }
            FromThalyx::Error { word, message, .. } => {
                return Err(Trouble::Channel(format!(
                    "the machine refused the connection: {word} — {message}"
                )));
            }
            other => {
                return Err(Trouble::Channel(format!(
                    "the machine's first message was not a hello: {other:?}"
                )));
            }
        }
        Ok(machine)
    }

    pub fn greeting(&self) -> &Greeting {
        &self.greeting
    }

    /// Ask one thing and get the answer back, verbatim.
    pub fn ask(
        &mut self,
        verb: &str,
        arguments: Vec<String>,
    ) -> Result<serde_json::Value, Trouble> {
        self.next += 1;
        let id = format!("mcp-{}", self.next);
        let request = ToThalyx::Request {
            id: id.clone(),
            verb: verb.to_string(),
            arguments,
        };
        write_frame(&mut self.output, &request.encode()).map_err(channel)?;

        match self.read()? {
            FromThalyx::Response {
                id: answered,
                answer,
            } if answered == id => Ok(answer),
            FromThalyx::Error {
                id: answered,
                word,
                remedy,
                message,
            } if answered == id || answered.is_empty() => Err(Trouble::Refused {
                word,
                remedy,
                message,
            }),
            // An answer to a question nobody asked. Never quietly used as the
            // answer to this one: matching by position instead of by id is
            // exactly how a read of one file gets reported as a read of another.
            other => Err(Trouble::Channel(format!(
                "the machine answered `{}` to a question that was not it",
                other.id().unwrap_or("(none)")
            ))),
        }
    }

    fn read(&mut self) -> Result<FromThalyx, Trouble> {
        self.read_wire().map_err(channel)
    }

    /// The same read with the wire's own error, for the one caller that has to
    /// tell one kind of I/O failure from another.
    fn read_wire(&mut self) -> Result<FromThalyx, WireError> {
        let body = read_frame(&mut self.input)?;
        FromThalyx::decode(&body)
    }
}

fn channel(error: WireError) -> Trouble {
    Trouble::Channel(match error {
        WireError::Closed => {
            "the machine closed the channel — the VM was shut down, or the bridge stopped".into()
        }
        other => other.to_string(),
    })
}
