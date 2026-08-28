//! What the screen is showing, as data.
//!
//! Nothing here draws and nothing here asks the machine anything. It is the
//! shape a caller fills in from the answers the two existing faces already
//! give, which is the whole reason `vault/02-Arquitectura/La-Pantalla.md` can
//! claim the screen costs the system nothing: since 2026-08-09 every verb
//! answers exactly one object per typed line, and this is that object arranged
//! for a display instead of for a parser.
//!
//! ## The one type that carries a promise
//!
//! [`Voice`] is not a colour choice. `vault/11-Seguridad/Marcado-de-Origen.md`
//! says provenance travels with the content, and on the screen it travels as
//! the voice a turn was said in: the person, the agent *proposing*, or the
//! machine stating. A caller that puts a model's sentence in
//! [`Voice::Machine`] has laundered a proposal into a fact, so that is the one
//! thing to get right when filling this in.

/// Who is speaking. Three, and never a fourth: anything that is not the person
/// and not the machine is a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    /// What the person typed. The only sovereign voice on the screen.
    Person,
    /// What the agent proposed. **Never executed by existing**, and never
    /// drawn like a fact.
    Agent,
    /// What the machine answered: a path, a size, an id, a journal line.
    Machine,
}

/// One thing that was said.
#[derive(Debug, Clone)]
pub struct Turn {
    pub voice: Voice,
    pub text: String,
}

impl Turn {
    pub fn person(text: impl Into<String>) -> Self {
        Self {
            voice: Voice::Person,
            text: text.into(),
        }
    }

    pub fn agent(text: impl Into<String>) -> Self {
        Self {
            voice: Voice::Agent,
            text: text.into(),
        }
    }

    pub fn machine(text: impl Into<String>) -> Self {
        Self {
            voice: Voice::Machine,
            text: text.into(),
        }
    }
}

/// How a row inside a panel reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    /// Something the machine did, and it worked.
    Ok,
    /// Something the machine would not do. Not an error in the code: a refusal
    /// is an answer.
    Refused,
    /// Present, and not the point.
    Muted,
}

/// A line inside a panel.
#[derive(Debug, Clone)]
pub enum Row {
    /// A machine fact on its own: a path, an id, a device.
    Fact { text: String, tone: Tone },
    /// A label in prose with a fact beside it, right-aligned, so a column of
    /// sizes lines up.
    Pair { label: String, value: String },
    /// The panel's own words, in prose, for when there is nothing to say.
    Note(String),
}

impl Row {
    pub fn fact(text: impl Into<String>) -> Self {
        Row::Fact {
            text: text.into(),
            tone: Tone::Plain,
        }
    }

    pub fn toned(text: impl Into<String>, tone: Tone) -> Self {
        Row::Fact {
            text: text.into(),
            tone,
        }
    }

    pub fn pair(label: impl Into<String>, value: impl Into<String>) -> Self {
        Row::Pair {
            label: label.into(),
            value: value.into(),
        }
    }

    pub fn note(text: impl Into<String>) -> Self {
        Row::Note(text.into())
    }
}

/// A region of the side columns. Not a window: it has no title bar, it does not
/// move, it does not stack and it cannot be closed.
#[derive(Debug, Clone)]
pub struct Panel {
    pub heading: String,
    pub rows: Vec<Row>,
}

impl Panel {
    pub fn new(heading: impl Into<String>, rows: Vec<Row>) -> Self {
        Self {
            heading: heading.into(),
            rows,
        }
    }

    /// How much of a column this panel deserves. Used by
    /// [`crate::Layout::split`].
    pub fn weight(&self) -> u32 {
        // Two, plus the rows: a panel with nothing in it still needs its
        // heading and the room to say why it is empty.
        2 + self.rows.len() as u32
    }
}

/// What the kernel guard is doing, which is the one thing on the bar that is
/// about whether this machine is enforcing its own promises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    /// Loaded and denying.
    Enforcing,
    /// Loaded and only watching. Every permission is advisory right now.
    Observing,
    /// Not loaded. Said plainly rather than dressed up, because a machine that
    /// cannot enforce and does not say so is the failure mode the whole LSM
    /// exists against.
    Absent,
    /// Could not be read. Rule 10: a failure to read is not a failure to
    /// exist, and the two are never printed as the same thing.
    Unknown,
}

impl Guard {
    pub fn words(self) -> &'static str {
        match self {
            Guard::Enforcing => "negando",
            Guard::Observing => "observando",
            Guard::Absent => "sin guardián",
            Guard::Unknown => "guardián ilegible",
        }
    }
}

/// The bar across the top.
#[derive(Debug, Clone)]
pub struct Bar {
    pub machine: String,
    pub store: String,
    pub guard: Guard,
    pub clock: String,
}

/// The line being typed, and where the caret is inside it.
#[derive(Debug, Clone, Default)]
pub struct Prompt {
    pub line: String,
    /// In characters, not bytes. `ñ` is two bytes and a caret counted in bytes
    /// lands between them — the same reason `thalyx-term` holds a `Vec<char>`.
    pub caret: usize,
    /// What the machine would complete, drawn dim after the caret. Never
    /// applied by itself.
    pub suggestion: Option<String>,
}

/// A confirmation on the trusted path.
///
/// When one of these exists it takes the whole display: no panel is drawn, no
/// turn of the conversation is drawn, and nothing else can be typed into. That
/// is how `vault/11-Seguridad/Camino-Confiable.md`'s single-reader property
/// survives having a screen — **there is nothing beside it that could imitate
/// it**, because there is nothing beside it.
#[derive(Debug, Clone)]
pub struct Confirmation {
    /// What is about to happen, in one sentence.
    pub what: String,
    /// What the machine read from the thing itself — not from a list it was
    /// keeping. This is the part that stops a correct command typed at the
    /// wrong machine.
    pub found: Vec<Row>,
    /// The exact words that authorise it. Never `sí` and never `y`: typing the
    /// device path protects against habit as well as against a typo.
    pub type_this: String,
    /// What has been typed so far.
    pub typed: String,
}

impl Confirmation {
    /// Whether what has been typed authorises the thing.
    ///
    /// Rule 9: the comparison is exact and the cautious answer is the default.
    /// No trimming, no case folding, no prefix match — every one of those turns
    /// *nearly right* into *yes*.
    pub fn authorised(&self) -> bool {
        !self.type_this.is_empty() && self.typed == self.type_this
    }
}

/// Everything on the display at one instant.
#[derive(Debug, Clone)]
pub struct Screen {
    pub bar: Bar,
    pub left: Vec<Panel>,
    pub right: Vec<Panel>,
    pub conversation: Vec<Turn>,
    pub prompt: Prompt,
    /// When this is set, it is the only thing drawn.
    pub confirmation: Option<Confirmation>,
}

impl Screen {
    /// The screen a machine shows before anything has happened on it.
    pub fn new(bar: Bar) -> Self {
        Self {
            bar,
            left: Vec::new(),
            right: Vec::new(),
            conversation: Vec::new(),
            prompt: Prompt::default(),
            confirmation: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_confirmation_needs_the_exact_words_and_nothing_close_to_them() {
        let mut confirmation = Confirmation {
            what: "borrar el disco entero".into(),
            found: vec![Row::fact("/dev/sdb  7 GiB  btrfs `fedora`")],
            type_this: "/dev/sdb".into(),
            typed: String::new(),
        };
        assert!(!confirmation.authorised());
        for near in [
            "/dev/sd",
            "/dev/sdb ",
            " /dev/sdb",
            "/DEV/SDB",
            "/dev/sdb1",
            "sí",
            "y",
        ] {
            confirmation.typed = near.into();
            assert!(!confirmation.authorised(), "{near:?} was accepted");
        }
        confirmation.typed = "/dev/sdb".into();
        assert!(confirmation.authorised());
    }

    #[test]
    fn a_confirmation_that_asks_for_nothing_authorises_nothing() {
        // The empty-string trap: `"" == ""` is true, so a confirmation built
        // with no words to type would be authorised the instant it appeared,
        // before anybody touched a key.
        let confirmation = Confirmation {
            what: "algo".into(),
            found: Vec::new(),
            type_this: String::new(),
            typed: String::new(),
        };
        assert!(!confirmation.authorised());
    }

    #[test]
    fn a_guard_that_could_not_be_read_does_not_say_it_is_absent() {
        // Rule 10 of the testing strategy. Reading these as the same thing is
        // how somebody concludes their machine has no enforcement when what it
        // has is a permissions problem on bpffs.
        assert_ne!(Guard::Unknown.words(), Guard::Absent.words());
    }

    #[test]
    fn a_panel_with_no_rows_still_asks_for_room() {
        // A weight of zero gives it no height at all, and then the sentence
        // saying why it is empty has nowhere to be drawn — an empty panel
        // becomes an invisible one.
        assert!(Panel::new("RED", Vec::new()).weight() > 0);
    }
}
