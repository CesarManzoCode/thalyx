//! The verbs a person uses before any other: where am I, what is here, show me.
//!
//! `vault/01-Filosofia/Principio-Doble-Ruta.md` decrees as non-negotiable that
//! the human can do everything directly, and its **first** layer is plain file
//! work. Until this module the session had thirteen verbs and not one of them
//! touched a file, so a machine that installs itself had nowhere for its owner
//! to keep anything.
//!
//! The logic lives in `thalyx-files`, which knows nothing about printing. Here
//! is only how it reaches a person — kept apart because the decisions worth
//! testing are about what is true, not about column widths.
//!
//! ## Why the paths are not Spanish
//!
//! The verbs are (`ver`, `leer`, `ir`, `donde`), because a person at the machine
//! types them. Everything they name — `/home`, `/opt/thalyx` — stays as it is on
//! disk, because a path is not language.

use serde_json::json;
use std::path::{Path, PathBuf};

type Fallible = Result<(), Box<dyn std::error::Error>>;
use thalyx_files::{Excerpt, FileError, Kind, Listing, Size, machine};

/// Which of the two faces this line is being answered in.
///
/// `vault/01-Filosofia/Filosofia-Fundacional.md`: every thing is born with two
/// faces, the human one and a structured one a program can ask for. This is the
/// *only* place the choice is made — each verb below computes a fact and hands
/// it to one of two readers. A verb that branched on the face while composing
/// its own sentence would be a second version of events, which is the thing the
/// decree exists to prevent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Human,
    Machine,
}

impl Face {
    fn machine(self) -> bool {
        self == Face::Machine
    }

    /// The same question, for the modules that print the other verbs.
    pub fn is_machine(self) -> bool {
        self.machine()
    }

    /// Print a line of the structured face.
    ///
    /// Kept as a method so no caller has to remember that these go out without
    /// the blank lines and two-space indent the human face uses. Whitespace a
    /// person reads as breathing room is noise a parser has to strip.
    pub fn say(self, line: String) {
        println!("{line}");
    }
}

/// Where the person is, carried by the session across one line and the next.
///
/// Kept as a type rather than a bare `PathBuf` so that changing it has to go
/// through [`Where::go`], which refuses to move somewhere that is not a readable
/// directory. A session whose location is a folder that is not there prints
/// errors from then on with no way back — and the person never typed anything
/// wrong.
pub struct Where {
    at: PathBuf,
}

impl Where {
    /// A session starts in `/home`: the `user` subvolume, and the only place on
    /// the machine decreed to be the person's own.
    pub fn start() -> Self {
        Self {
            at: PathBuf::from(thalyx_files::HOME),
        }
    }

    pub fn at(&self) -> &Path {
        &self.at
    }

    /// The location as it goes in the prompt, short enough to leave room to type.
    ///
    /// Found by running it. A prompt carrying the whole path put ninety
    /// characters on the line before the person could type a character, and the
    /// console on a real machine is often eighty wide — so the prompt alone
    /// wrapped, and a system whose *prompt* does not fit is not one anybody will
    /// use.
    ///
    /// The rule is that this may be lossy and `donde` may not: the prompt is a
    /// reminder, and there is one verb whose answer is always the whole truth.
    /// Anything that shortened both would leave a person with no way to find out
    /// where they actually are.
    pub fn briefly(&self) -> String {
        const ROOM: usize = 28;

        let full = self.at.display().to_string();
        if full.chars().count() <= ROOM {
            return full;
        }

        // Whole components from the right, never a cut through the middle of a
        // name: half a folder name reads like a folder that exists.
        let parts: Vec<&str> = self.at.iter().filter_map(|p| p.to_str()).collect();
        let mut kept: Vec<&str> = Vec::new();
        let mut width = 1; // the leading `…`
        for part in parts.iter().rev() {
            if part == &"/" {
                continue;
            }
            let cost = part.chars().count() + 1;
            if width + cost > ROOM && !kept.is_empty() {
                break;
            }
            width += cost;
            kept.push(part);
        }
        kept.reverse();
        format!("…/{}", kept.join("/"))
    }

    /// Move, or say why not and stay put.
    ///
    /// The check before the move is the whole point. `list` is what is asked —
    /// not `exists` — because a directory that cannot be read is one a person
    /// would be stuck inside, and finding that out on arrival is too late.
    pub fn go(&mut self, named: &str) -> Result<(), FileError> {
        let target = thalyx_files::resolve(&self.at, named);
        match thalyx_files::list(&target) {
            Ok(_) => {
                self.at = target;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
}

/// `clear` — the verb whose absence made the system look like a toy.
///
/// Cesar typed it on the first real session and got a paragraph about the agent,
/// because an unknown line falls through to "I have no model loaded". A common
/// command answering with a speech about something else is exactly how a system
/// reads as unfinished.
///
/// The two escapes are the whole implementation: erase the screen, then put the
/// cursor back at the top. Written out rather than borrowed from a terminal
/// library, for the same reason as the cpio and the Btrfs writer — the image
/// holds the kernel and one program.
pub fn clear(face: Face) {
    use std::io::Write;

    // A program gets an answer and not an escape sequence, and it gets one for
    // the reason `machine.rs` gives for `cd`: **silence is never an answer.** A
    // parser waiting on a stream cannot tell a screen it did not need cleared
    // from a session that died mid-line, and this verb is otherwise the one that
    // produces no output at all.
    //
    // Nothing is cleared for it either. There is no screen on that end, and
    // writing `ESC[2J` into a pipe is bytes a caller has to strip before it can
    // parse the line it is on.
    if face.is_machine() {
        face.say(machine::answer("clear", vec![("cleared", json!(false))]));
        return;
    }

    print!("\x1b[2J\x1b[H");
    // Flushed here because what follows is a prompt printed with `print!` and no
    // newline of its own; leaving this in the buffer would put the prompt on
    // screen before the screen was cleared.
    let _ = std::io::stdout().flush();
}

/// `estructurado on|off` — the verb that asks for the other face.
///
/// This is the verb the objective decree was waiting on. Everything below it
/// already returned facts instead of printing; until something could ask for
/// them, the decree was written down and not built, and none of the four
/// advantages Thalyx has over Linux was exposed to anybody.
///
/// The answer is always given **in the face that is now on**, which is what
/// makes it usable from both sides: a program that asks for the structured face
/// gets a parseable acknowledgement of its own request, and a person who turns
/// it back off gets a sentence.
///
/// The acknowledgement carries the way out. A person who types this without
/// meaning to would otherwise face a session that answers only in JSON with no
/// visible way back, on a machine where there is no second terminal.
pub fn structured(face: &mut Face, rest: &str) {
    let was_human = !face.machine();
    let asked = match rest.trim() {
        "" => None,
        "on" | "si" | "sí" => Some(Face::Machine),
        "off" | "no" => Some(Face::Human),
        other => {
            let why = format!("`{other}` is neither `on` nor `off`");
            if face.machine() {
                face.say(machine::refusal("structured", &why));
            } else {
                println!("\n  {why}.\n");
            }
            return;
        }
    };

    if let Some(wanted) = asked {
        *face = wanted;
    }

    if face.machine() {
        // The one line that would otherwise not parse, and it is the line that
        // says the face is on. The human prompt for *this* command was printed
        // before it was read — the face was still human then — and it ends
        // without a newline, so the acknowledgement landed on the same line:
        // `  /home > {"op":"structured",…`. Found by piping the session and
        // reading it back, not by any test of the object itself.
        if was_human {
            println!();
        }
        face.say(machine::state(matches!(face, Face::Machine)));
        return;
    }
    println!();
    if asked.is_some() {
        println!("  Answers are for a person again.");
    } else {
        println!("  Answers are for a person. `structured on` makes them JSON,");
        println!("  one object per line, with nothing hidden and sizes exact.");
    }
    println!();
}

/// `pwd` — the one verb whose answer is always available.
pub fn where_am_i(here: &Where, face: Face) {
    if face.machine() {
        face.say(machine::location("where", here.at()));
        return;
    }
    println!();
    println!("  {}", here.at().display());
    println!();
}

/// `cd [ruta]` — with nothing after it, back to `/home`.
pub fn go(here: &mut Where, rest: &str, face: Face) {
    let named = if rest.is_empty() {
        thalyx_files::HOME
    } else {
        rest
    };

    match here.go(named) {
        // Nothing on success, because the prompt on the very next line already
        // says where the person now is. Printing it here as well put the same
        // path on screen twice for every move — noise that a person reads as
        // the machine repeating itself.
        //
        // The machine face answers anyway, and this is the sharpest case of why
        // it must: a program has no next prompt to read the new location off,
        // and cannot tell a silence that means "moved" from one that means the
        // session stopped.
        Ok(()) => {
            if face.machine() {
                face.say(machine::location("go", here.at()));
            }
        }
        Err(error) => {
            if face.machine() {
                face.say(machine::stayed("go", &error, here.at()));
                return;
            }
            println!();
            println!("  {error}");
            // Named explicitly, and this is why the failing path does not stay
            // silent: a person who is not told they did not move aims the next
            // thing they type at a place they are not.
            println!("  You are still in {}.", here.at().display());
            println!();
        }
    }
}

/// What the person asked `ls` for, on top of a place.
///
/// Parsed rather than guessed. `ls -la` is one word to a person and two flags to
/// the machine, and a listing that took `-la` as a **folder name** would answer
/// "not there" for something they typed correctly.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Asked {
    /// Include the names the system keeps for itself.
    pub all: bool,
    /// One per line, with sizes, instead of columns of names.
    pub long: bool,
    /// What is left after the flags: the place, or nothing.
    pub place: String,
    /// How many entries to answer with, when the caller said.
    ///
    /// `Superficie-para-el-LLM.md`, punto **B1**, and it is read in both faces
    /// and obeyed in one — the same rule as `-a` and `-l`, pointed the other
    /// way. A window is a fact about a context window, and a person does not
    /// have one: cutting the human listing would be taking something away, and
    /// on the image there is no pager to get it back with.
    pub limit: Option<usize>,
    /// Where to resume, exactly as the previous answer handed it over.
    ///
    /// Kept as the text that was typed rather than parsed here, so that a cursor
    /// this machine did not write is refused by the one piece of code that knows
    /// what a cursor is, with a word the caller can match on.
    pub cursor: Option<String>,
}

impl Asked {
    /// Both spellings of every flag, because the person chose to have both.
    ///
    /// The Spanish words are whole arguments and not letters, so `todo` cannot
    /// collide with a `-t` that might exist later.
    /// Takes the words rather than the line: the splitting happens once, in
    /// `words.rs`, so that `ls "mi carpeta"` and `rm "mi carpeta"` agree about
    /// where a name stops. A flag is read off the text and never off the quoting
    /// — `ls "-la"` lists everything, exactly as it does in a shell, because a
    /// shell's quoting speaks to the shell and not to the program.
    fn parse(given: &[crate::words::Word]) -> Self {
        let mut asked = Asked::default();
        let mut place = Vec::new();

        for word in given.iter().map(crate::words::Word::as_str) {
            // `nombre=valor` before anything else, and only when the value is
            // one this understands. A file really named `limite=x` is
            // vanishingly rare and a mis-typed number is not — so a value that
            // does not parse falls through to being a place, and the caller is
            // told "limite=dos is not there" instead of silently getting a
            // default window they did not ask for.
            match word.split_once('=') {
                Some(("limite" | "limit", count)) if count.parse::<usize>().is_ok() => {
                    asked.limit = count.parse().ok();
                    continue;
                }
                Some(("cursor" | "desde", token)) if !token.is_empty() => {
                    asked.cursor = Some(token.to_string());
                    continue;
                }
                _ => {}
            }

            match word {
                "todo" | "todos" | "ocultos" => asked.all = true,
                "detalles" | "largo" => asked.long = true,
                // Grouped short flags — `-la` is what people's fingers do.
                _ if word.starts_with('-') && word.len() > 1 => {
                    for letter in word.chars().skip(1) {
                        match letter {
                            'a' => asked.all = true,
                            'l' => asked.long = true,
                            // An unknown flag is not silently ignored: it is kept
                            // as part of the place, so the person gets "not
                            // there" naming exactly what they typed instead of a
                            // listing of somewhere they did not ask about.
                            _ => place.push(word.to_string()),
                        }
                    }
                }
                _ => place.push(word.to_string()),
            }
        }

        asked.place = place.join(" ");
        asked
    }
}

/// How wide to lay a listing out when nothing can say.
///
/// Eighty, because that is what a Linux console is when the framebuffer has not
/// been asked otherwise, and because guessing wider produces lines that wrap —
/// which is worse than guessing narrow and leaving space unused.
const ASSUMED_WIDTH: usize = 80;

pub fn screen_width() -> usize {
    use std::io::IsTerminal;
    let out = std::io::stdout();
    if !out.is_terminal() {
        // Redirected output has no width, and a made-up one would put column
        // padding into somebody's file.
        return ASSUMED_WIDTH;
    }
    thalyx_syscall::terminal_width(std::os::fd::AsFd::as_fd(&out))
        .map(usize::from)
        .unwrap_or(ASSUMED_WIDTH)
}

/// What the caller asked for, in the words `thalyx-files` pages by.
///
/// The default limit is applied here and not in the parser, so that "the caller
/// said nothing" and "the caller said two hundred" stay distinguishable all the
/// way down — and so there is exactly one place that decides what nothing means.
fn window_asked(asked: &Asked) -> Result<thalyx_files::window::Asked, thalyx_files::window::Cut> {
    Ok(thalyx_files::window::Asked {
        limit: asked.limit.unwrap_or(thalyx_files::window::DEFAULT_LIMIT),
        after: match &asked.cursor {
            Some(text) => Some(thalyx_files::window::Cursor::parse(text)?),
            None => None,
        },
    })
}

/// `ls [-a] [-l] [limite=N] [cursor=…] [ruta]` — what is here, or what is there.
///
/// The flags are read in both faces and **obeyed in only one**. `-a` and `-l`
/// are about how much of the truth reaches a person; the structured face always
/// carries all of it, so to a program they are neither an error nor a change.
/// That is the tie-break rule of the objective decree: the LLM is never given
/// less, and a human comfort is never allowed to cost it capability.
pub fn look(here: &Where, rest: &str, face: Face) {
    let Some(given) = crate::words::asked(face, "list", rest) else {
        return;
    };
    let asked = Asked::parse(&given);
    let target = if asked.place.is_empty() {
        here.at().to_path_buf()
    } else {
        thalyx_files::resolve(here.at(), &asked.place)
    };

    // `ls` on a file is a reasonable thing to type, so it answers instead of
    // correcting: the person wanted to know about that file. Both faces get the
    // same fallback, which they did not before — the machine face would have
    // reported "not there" for something that is there.
    let mut single = false;
    let found = match thalyx_files::list(&target) {
        Ok(listing) => Ok(listing),
        Err(error) => match thalyx_files::list_one(&target) {
            Ok(one) => {
                single = true;
                Ok(one)
            }
            // The original error, not the one from the second attempt: the
            // person asked to list a directory, and "is not there" said about
            // the directory is the answer to what they typed.
            Err(_) => Err(error),
        },
    };

    if face.machine() {
        match found {
            Ok(listing) => match window_asked(&asked) {
                Ok(window) => match listing.paged(&window) {
                    Ok((page, unreadable)) => {
                        face.say(machine::listing(&target, &page, &unreadable))
                    }
                    // Unreachable unless the listing stopped being sorted, and
                    // said rather than swallowed for exactly that reason: a
                    // cursor into unordered rows means something different on
                    // every call, and a page produced anyway would be wrong
                    // quietly.
                    Err(why) => face.say(machine::declined("list", "unordered", &why.to_string())),
                },
                Err(why) => face.say(machine::declined("list", "bad_cursor", &why.to_string())),
            },
            Err(error) => face.say(machine::failure("list", &error)),
        }
        return;
    }

    println!();
    match found {
        // One thing always shows its size: `ls archivo` is asked when the size
        // is the question, and a bare name would answer nothing the person did
        // not already type.
        Ok(listing) => print_listing(
            &target,
            &listing,
            &Asked {
                long: asked.long || single,
                ..asked
            },
        ),
        Err(error) => println!("  {error}"),
    }
    println!();
}

fn print_listing(target: &Path, listing: &Listing, asked: &Asked) {
    println!("  {}", target.display());

    let shown: Vec<&thalyx_files::Entry> = listing
        .entries
        .iter()
        .filter(|entry| asked.all || !thalyx_files::is_hidden(&entry.name))
        .collect();
    let hidden = listing.entries.len() - shown.len();

    if shown.is_empty() && listing.unreadable.is_empty() {
        println!();
        // The count is why this is not just "nothing here". A directory holding
        // thirty-five dotfiles is not an empty one, and saying so would send a
        // person to look for files they already have.
        if hidden > 0 {
            println!("  nothing but {hidden} hidden — `ls -a` shows them");
        } else {
            println!("  nothing here");
        }
        return;
    }

    println!();
    if asked.long {
        print_long(&shown);
    } else {
        let names: Vec<String> = shown.iter().map(|entry| decorate(entry)).collect();
        for line in thalyx_files::in_columns(&names, screen_width(), 4) {
            println!("    {line}");
        }
    }

    // Said, never silently done. A person who is not told they are seeing a
    // filtered listing has no reason to suspect one, and this is the sentence
    // that makes `ls -a` findable at the moment it is wanted.
    if hidden > 0 {
        println!();
        println!("  {hidden} hidden — `ls -a` shows them");
    }

    // Rule 10, printed. An entry that could not be read is not an entry that is
    // not there, and a listing that dropped it would report a smaller directory
    // than the one on the disk — which is how a person deletes a folder
    // believing it empty.
    if !listing.unreadable.is_empty() {
        println!();
        println!("  {} could not be read:", listing.unreadable.len());
        for (name, why) in &listing.unreadable {
            println!("    {}: {why}", name.to_string_lossy());
        }
    }
}

/// The name with the one character that says what it is.
fn decorate(entry: &thalyx_files::Entry) -> String {
    let name = entry.name.to_string_lossy();
    match entry.kind {
        // The trailing slash is the whole difference between a name a person can
        // enter and one they can open, and it costs one character.
        Kind::Directory => format!("{name}/"),
        // `@` for a link and `!` for one that points nowhere. A broken link that
        // looked like a file would be followed, and the person would be told the
        // file cannot be read when the truth is that it is not there.
        Kind::Link { broken: true, .. } => format!("{name}!"),
        Kind::Link { broken: false, .. } => format!("{name}@"),
        _ => name.to_string(),
    }
}

/// One per line with sizes, for when the names are not the question.
fn print_long(shown: &[&thalyx_files::Entry]) {
    // Measured, not fixed at 32. A name longer than the column pushed the size
    // out of line on Cesar's own machine — `First_Layer_Bed_Leveling_Test.stl`
    // is thirty-three characters, and one file was enough to break the column
    // for every row.
    let widest = shown
        .iter()
        .map(|entry| decorate(entry).chars().count())
        .max()
        .unwrap_or(0);

    for entry in shown {
        let name = decorate(entry);
        match &entry.kind {
            Kind::Directory => println!("    {name}"),
            Kind::File { bytes } => println!("    {name:<widest$}  {}", Size(*bytes)),
            Kind::Link { to, broken } => {
                let note = if *broken { "  — broken" } else { "" };
                println!("    {name:<widest$}  -> {}{note}", to.display());
            }
            Kind::Other(what) => println!("    {name:<widest$}  {what}"),
        }
    }
}

/// `cat <archivo>` — show a file, or say why showing it would be a mistake.
pub fn read(here: &Where, rest: &str, face: Face) {
    if rest.is_empty() {
        if face.machine() {
            face.say(machine::refusal("read", "which file"));
            return;
        }
        println!();
        println!("  Which file. `ls` lists what is here.");
        println!();
        return;
    }

    let target = thalyx_files::resolve(here.at(), rest);
    let found = thalyx_files::read(&target);

    if face.machine() {
        face.say(match &found {
            Ok(excerpt) => machine::excerpt(&target, excerpt),
            // Including `not_text`, and that refusal is the one place the two
            // faces are refusing for different reasons: printing bytes wrecks a
            // person's only terminal, and a caller asking for text has been
            // handed something that is not text. The word says which.
            Err(error) => machine::failure("read", error),
        });
        return;
    }

    println!();
    match found {
        Ok(excerpt) => print_excerpt(&target, &excerpt),
        Err(error) => println!("  {error}"),
    }
    println!();
}

fn print_excerpt(target: &Path, excerpt: &Excerpt) {
    if excerpt.truncated {
        // Both numbers, because "showing 64.0 kB" alone leaves a person believing
        // they have seen the file.
        println!(
            "  {} — showing {} of {}",
            target.display(),
            Size(excerpt.text.len() as u64),
            Size(excerpt.of_bytes)
        );
    } else {
        println!("  {} — {}", target.display(), Size(excerpt.of_bytes));
    }
    println!();

    if excerpt.of_bytes == 0 {
        println!("  the file is empty");
        return;
    }

    for line in excerpt.text.lines() {
        println!("  {line}");
    }

    if excerpt.truncated {
        println!();
        println!("  … cut here. The rest of the file is on the disk, unread.");
    }
}

// ──────────────────────────────────────────────────────────── changing what is there
//
// The objective is that an LLM works better here than anywhere else, so every
// one of these prints from the same [`thalyx_files::Done`] the operation
// returned. One fact, two faces — a second code path that composes its own
// sentence is a second version of events.

/// Turn what was typed into the paths it names, expanding `*` and `?`.
///
/// A pattern that matches nothing comes back as **the pattern itself**, so the
/// error a person reads names what they typed. Silently doing nothing is the
/// one outcome that leaves them believing something happened.
fn targets(here: &Where, word: &crate::words::Word) -> Vec<PathBuf> {
    // Asked of the word rather than of its text: `rm "a*b"` names one oddly
    // named file and `rm a*b` names several, and the quotes are gone by here.
    if !word.is_pattern() {
        return vec![thalyx_files::resolve(here.at(), word.as_str())];
    }
    let resolved = thalyx_files::resolve(here.at(), word.as_str());
    let (folder, pattern) = match (resolved.parent(), resolved.file_name()) {
        (Some(folder), Some(name)) => (folder.to_path_buf(), name.to_string_lossy().to_string()),
        _ => return vec![resolved],
    };
    match thalyx_files::expand(&folder, &pattern) {
        Ok(found) if !found.is_empty() => found,
        _ => vec![resolved],
    }
}

/// Whether these outcomes happened or were only foreseen.
///
/// The machine face does not need this — its `op` already says `rehearse`, and
/// that is how a program tells the two apart. The human face has nothing but
/// the sentence, and `ensayo rm notas.txt` answered `removed …` for a file that
/// is still there.
#[derive(Clone, Copy, PartialEq)]
enum Tense {
    Happened,
    Foreseen,
}

fn report(done: &thalyx_files::Done, tense: Tense) {
    // One fact, one sentence, two tenses. The alternative — a second printer for
    // rehearsals — is the second version of events this module exists to avoid.
    let verb = match tense {
        Tense::Happened => done.what.word().replace('_', " "),
        Tense::Foreseen => done.what.would().to_string(),
    };
    match &done.to {
        Some(to) => println!("  {verb} {} -> {}", done.path.display(), to.display()),
        None => println!("  {verb} {}", done.path.display()),
    }
}

/// Every outcome of one typed line, before either face has seen it.
///
/// Collected rather than printed as it goes, because the structured face owes
/// the caller one object per line and cannot know the count until the work is
/// done. The human face reads the same vector, so the two cannot report
/// different runs of the same command.
type Outcomes = Vec<Result<thalyx_files::Done, FileError>>;

fn speak(face: Face, op: &str, outcomes: &Outcomes, tense: Tense) {
    if face.machine() {
        let results = outcomes
            .iter()
            .map(|outcome| match outcome {
                Ok(done) => machine::fact(done),
                Err(error) => machine::problem(error),
            })
            .collect();
        face.say(machine::batch(op, results));
        return;
    }

    println!();
    for outcome in outcomes {
        match outcome {
            Ok(done) => report(done, tense),
            Err(error) => println!("  {error}"),
        }
    }
    println!();
}

/// A line that never reached the filesystem, said in whichever face is on.
///
/// A program that got silence here would wait forever for an answer that was
/// never coming, which is the failure the structured face has to not have.
fn incomplete(face: Face, op: &str, machine_why: &str, human_why: &str) {
    if face.machine() {
        face.say(machine::refusal(op, machine_why));
        return;
    }
    println!("\n  {human_why}\n");
}

/// `ensayo <verbo> <argumentos>` — what it would do, without doing any of it.
///
/// `vault/02-Arquitectura/Superficie-para-el-LLM.md`, punto **D1**. Today the
/// only way anything can find out what a command does is to run it, and in a
/// system where that cannot be taken back a careful caller stops trying things.
///
/// A **prefix and not a mode**, deliberately. A mode that rehearses can be left
/// on — and then a real `rm` does nothing while the caller believes it worked —
/// or left off, which is worse. Written in front of the command, it is a fact
/// about that one line and cannot be forgotten in either direction.
pub fn rehearse(here: &Where, store: &thalyx_core::Store, rest: &str, face: Face) -> Fallible {
    let rest = rest.trim();
    let (word, arguments) = match rest.split_once(' ') {
        Some((word, arguments)) => (word, arguments.trim()),
        None => (rest, ""),
    };

    if word.is_empty() {
        incomplete(
            face,
            "rehearse",
            "which verb to rehearse",
            "Which verb. `ensayo rm notas.txt` says what that would do.",
        );
        return Ok(());
    }

    let Some(verb) = crate::catalogue::verb_named(word) else {
        let why = format!("`{word}` is not a verb of this machine");
        if face.machine() {
            face.say(thalyx_files::machine::declined(
                "rehearse",
                "unknown_verb",
                &why,
            ));
        } else {
            println!("\n  {why}. `describe` lists them.\n");
        }
        return Ok(());
    };

    // Only the ones that can change something. Rehearsing `ls` is `ls`, and
    // answering it here would be a second, worse implementation of it.
    if !verb.changes {
        let why = format!(
            "`{}` changes nothing, so there is nothing to rehearse",
            verb.id
        );
        if face.machine() {
            face.say(thalyx_files::machine::declined(
                "rehearse", "harmless", &why,
            ));
        } else {
            println!("\n  {why}.\n");
        }
        return Ok(());
    }

    let outcomes: Outcomes = match verb.id {
        "make_directory" | "make_file" => {
            let directory = verb.id == "make_directory";
            if arguments.is_empty() {
                incomplete(face, "rehearse", "which one", "Which one.");
                return Ok(());
            }
            let Some(named) = crate::words::asked(face, "rehearse", arguments) else {
                return Ok(());
            };
            named
                .iter()
                .map(|word| {
                    let path = thalyx_files::resolve(here.at(), word.as_str());
                    if directory {
                        thalyx_files::foresee_make_directory(&path)
                    } else {
                        thalyx_files::foresee_make_file(&path)
                    }
                })
                .collect()
        }
        "copy" | "move" => {
            let Some(words) = crate::words::asked(face, "rehearse", arguments) else {
                return Ok(());
            };
            if words.len() != 2 {
                incomplete(
                    face,
                    "rehearse",
                    "two names are needed: what to take, and where it goes",
                    "Two names: what to take, and where it goes.",
                );
                return Ok(());
            }
            let from = thalyx_files::resolve(here.at(), words[0].as_str());
            let to = destination(&from, thalyx_files::resolve(here.at(), words[1].as_str()));
            vec![if verb.id == "move" {
                thalyx_files::foresee_move(&from, &to)
            } else {
                thalyx_files::foresee_copy(&from, &to)
            }]
        }
        "remove" => {
            if arguments.is_empty() {
                incomplete(face, "rehearse", "which one", "Which one.");
                return Ok(());
            }
            let Some(named) = crate::words::asked(face, "rehearse", arguments) else {
                return Ok(());
            };
            let mut chosen = Vec::new();
            for word in &named {
                chosen.extend(targets(here, word));
            }
            chosen
                .iter()
                .map(|path| thalyx_files::foresee_remove(path))
                .collect()
        }
        // The rehearsal that matters most, because it is the only one whose
        // real form cannot be taken back. It answers with everything that would
        // let somebody notice they typed the wrong four digits.
        "stop" => return crate::proc::rehearse_stop(arguments, face),
        // Worth more here than anywhere else: the input that does the damage
        // is a path to somebody else's code, and the wrong one and the right
        // one differ by a few characters that the confirmation will draw
        // either way.
        "execute" => return crate::foreign::rehearse(arguments, face),
        // `intento` is the one changing verb that already answers this, and it
        // answers it better than a rehearsal could: `intento` alone says what
        // abandoning would cost right now, and `intento abandonar` without the
        // confirming word says it again before doing anything. Sending the
        // caller there is A2 applied to a rehearsal — name the way out rather
        // than only refuse.
        "attempt" => {
            let why = "`attempt` says what it would cost by itself: `intento` alone, \
                       or `intento abandonar` without confirming";
            if face.machine() {
                face.say(thalyx_files::machine::declined(
                    "rehearse",
                    "ask_attempt_itself",
                    why,
                ));
            } else {
                println!("\n  {why}.\n");
            }
            return Ok(());
        }
        // The three whose "work it out" half already existed as a value, so the
        // rehearsal is that half with the acting half never called. `install`
        // resolves a candidate and reads what it asks for; `rollback` has had a
        // `plan` separate from `apply` since it was written; `install_onto`
        // computes the whole layout, finds the kernel and reads what is on the
        // disk **before** the confirmation, which was done so that a confirmed
        // wipe could never discover afterwards that there was no kernel — and
        // that ordering is what makes this rehearsal a stop rather than a second
        // implementation.
        "install" => return Ok(crate::modules::foresee_install(store, arguments, face)?),
        "rollback" => return Ok(crate::modules::foresee_rollback(store, face)?),
        "install_onto" => {
            crate::session::foresee_install_onto(arguments, face);
            return Ok(());
        }
        // `apagar`. Everything not written to the store is lost and that is all
        // there is to say, but it has to be said: this is the one verb where a
        // person finds out by losing it.
        "power_off" => {
            let why = "everything not written to the store would be lost, because the root filesystem is memory";
            if face.machine() {
                face.say(thalyx_files::machine::answer(
                    "rehearse",
                    vec![
                        ("verb", serde_json::json!("power_off")),
                        ("loses_unwritten_memory", serde_json::json!(true)),
                        ("would_write", serde_json::json!(false)),
                        ("message", serde_json::json!(why)),
                    ],
                ));
            } else {
                println!("\n  {why}.\n");
            }
            return Ok(());
        }
        // `correr`. The only one left, and it stays honest rather than guessing:
        // what a run would be allowed to do is a question for the kernel side,
        // and answering it from the manifest would describe a run that the
        // machine may not be able to give.
        _ => {
            let why = format!(
                "`{}` changes the machine and cannot be rehearsed yet",
                verb.id
            );
            if face.machine() {
                face.say(thalyx_files::machine::declined("rehearse", "cannot", &why));
            } else {
                println!("\n  {why}.\n");
            }
            return Ok(());
        }
    };

    speak(face, "rehearse", &outcomes, Tense::Foreseen);
    Ok(())
}

/// Naming a folder as the destination means "inside it", which is what both a
/// person and an agent mean and what every other system does.
///
/// Shared by the real operation and its rehearsal, because a rehearsal that
/// worked out a different destination would be answering a different question.
fn destination(from: &Path, mut to: PathBuf) -> PathBuf {
    if to.is_dir()
        && let Some(name) = from.file_name()
    {
        to = to.join(name);
    }
    to
}

/// `mkdir <carpeta>` / `crear <archivo>`.
pub fn make(here: &Where, rest: &str, directory: bool, face: Face) -> Fallible {
    let op = if directory {
        "make_directory"
    } else {
        "make_file"
    };
    if rest.is_empty() {
        incomplete(face, op, "which one", "Which one.");
        return Ok(());
    }

    let Some(named) = crate::words::asked(face, op, rest) else {
        return Ok(());
    };
    let outcomes: Outcomes = named
        .iter()
        .map(|word| {
            let path = thalyx_files::resolve(here.at(), word.as_str());
            if directory {
                thalyx_files::make_directory(&path)
            } else {
                thalyx_files::make_file(&path)
            }
        })
        .collect();

    speak(face, op, &outcomes, Tense::Happened);
    Ok(())
}

/// `cp <de> <a>` and `mv <de> <a>`.
pub fn transfer(here: &Where, rest: &str, moving: bool, face: Face) -> Fallible {
    let op = if moving { "move" } else { "copy" };
    let Some(words) = crate::words::asked(face, op, rest) else {
        return Ok(());
    };
    if words.len() != 2 {
        incomplete(
            face,
            op,
            "two names are needed: what to take, and where it goes",
            "Two names: what to take, and where it goes.",
        );
        return Ok(());
    }

    let from = thalyx_files::resolve(here.at(), words[0].as_str());
    let to = destination(&from, thalyx_files::resolve(here.at(), words[1].as_str()));

    let outcome = if moving {
        thalyx_files::move_to(&from, &to)
    } else {
        thalyx_files::copy(&from, &to)
    };

    speak(face, op, &vec![outcome], Tense::Happened);
    Ok(())
}

/// `rm <cosa>...`, which is the one verb here that cannot be taken back.
pub fn erase(here: &Where, rest: &str, face: Face) -> Fallible {
    if rest.is_empty() {
        incomplete(face, "remove", "which one", "Which one.");
        return Ok(());
    }

    let Some(named) = crate::words::asked(face, "remove", rest) else {
        return Ok(());
    };
    let mut chosen = Vec::new();
    for word in &named {
        chosen.extend(targets(here, word));
    }

    // Shown before anything is touched, and counted. A pattern is the one thing
    // a person types without knowing what it caught, and `/home` is decreed to
    // be the one place no rollback of ours can put back — so this listing is the
    // only warning there is.
    //
    // The machine face has no equivalent and needs none: its answer carries
    // every path it touched, and a program is not deciding whether to press
    // Return halfway through reading a list.
    if !face.machine() && chosen.len() > 1 {
        println!();
        println!("  {} things:", chosen.len());
        for path in &chosen {
            println!("    {}", path.display());
        }
    }

    let outcomes: Outcomes = chosen
        .iter()
        .map(|path| thalyx_files::remove(path))
        .collect();
    speak(face, "remove", &outcomes, Tense::Happened);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line as a verb receives it: split in `words.rs` first, because that
    /// is the only way any of these reach `parse` at the real prompt.
    fn parsed(line: &str) -> Asked {
        Asked::parse(&crate::words::words(line).expect("no quotes in these"))
    }

    // ──────────────────────────────────────────────── what the person asked for

    #[test]
    fn a_bare_ls_asks_for_nothing_in_particular() {
        let asked = parsed("");
        assert!(!asked.all);
        assert!(!asked.long);
        assert!(asked.place.is_empty());
    }

    #[test]
    fn grouped_flags_are_read_as_the_flags_they_are_and_not_as_a_folder() {
        // The failure this prevents: `-la` taken as a place, answering "not
        // there" for something the person typed correctly.
        let asked = parsed("-la");
        assert!(asked.all && asked.long);
        assert!(asked.place.is_empty(), "got {:?}", asked.place);
    }

    #[test]
    fn flags_and_a_place_together_keep_the_place() {
        let asked = parsed("-a Documentos");
        assert!(asked.all);
        assert_eq!(asked.place, "Documentos");
    }

    #[test]
    fn both_spellings_of_a_flag_mean_the_same_thing() {
        // Cesar chose to keep both vocabularies, so the flags have both too.
        assert_eq!(parsed("-a"), parsed("todo"));
        assert_eq!(parsed("-l"), parsed("detalles"));
    }

    #[test]
    fn an_unknown_flag_is_not_quietly_swallowed() {
        let asked = parsed("-z");
        // Kept as the place, so the person is told "-z is not there" instead of
        // being handed a listing of somewhere they did not ask about — which
        // would look like the flag worked.
        assert!(asked.place.contains("-z"), "got {asked:?}");
        assert!(!asked.all && !asked.long);
    }

    #[test]
    fn a_file_whose_name_begins_with_a_dash_is_still_reachable() {
        // A single `-` is not a flag: `len() > 1` is what keeps a file actually
        // named `-` from becoming unnameable.
        assert_eq!(parsed("-").place, "-");
    }

    #[test]
    fn a_session_starts_in_the_persons_own_subvolume() {
        assert_eq!(Where::start().at(), Path::new("/home"));
    }

    #[test]
    fn moving_somewhere_that_is_not_there_leaves_the_person_where_they_were() {
        let dir = tempfile::tempdir().unwrap();
        let mut here = Where::start();
        here.go(dir.path().to_str().unwrap()).unwrap();
        let before = here.at().to_path_buf();

        assert!(here.go("nowhere-at-all").is_err());

        // The failure this prevents: a session sitting in a folder that does not
        // exist, printing errors for everything typed afterwards, with the
        // person never having typed anything wrong.
        assert_eq!(here.at(), before);
    }

    #[test]
    fn moving_into_a_file_is_refused_rather_than_accepted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notas.txt"), "x").unwrap();

        let mut here = Where::start();
        here.go(dir.path().to_str().unwrap()).unwrap();
        let before = here.at().to_path_buf();

        assert!(here.go("notas.txt").is_err());
        assert_eq!(here.at(), before);
    }

    #[test]
    fn moving_up_and_back_down_returns_to_the_same_place() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("notas")).unwrap();

        let mut here = Where::start();
        here.go(dir.path().to_str().unwrap()).unwrap();
        let root = here.at().to_path_buf();

        here.go("notas").unwrap();
        here.go("..").unwrap();

        assert_eq!(here.at(), root);
    }

    // ────────────────────────────────────────── the prompt has to leave room to type

    #[test]
    fn a_short_location_goes_in_the_prompt_whole() {
        let mut here = Where::start();
        assert_eq!(here.briefly(), "/home");

        here.at = PathBuf::from("/home/cesar/notas");
        assert_eq!(here.briefly(), "/home/cesar/notas");
    }

    #[test]
    fn a_long_location_is_shortened_so_the_prompt_still_fits_eighty_columns() {
        let mut here = Where::start();
        here.at = PathBuf::from("/tmp/claude-0/-home-user-thalyx/c16462c8-b3df/scratchpad/casa");

        let brief = here.briefly();
        // The failure this prevents, found by running it: ninety characters of
        // prompt on an eighty-column console, wrapping before a key is pressed.
        assert!(brief.chars().count() <= 30, "prompt was {brief:?}");
        assert!(brief.starts_with('…'));
        assert!(brief.ends_with("casa"), "the place you are in must survive");
    }

    #[test]
    fn shortening_never_cuts_through_the_middle_of_a_name() {
        let mut here = Where::start();
        here.at = PathBuf::from("/a/veryveryverylongdirectoryname/anotherlongish/final");

        let brief = here.briefly();
        // Half a folder name reads like a folder that exists, and a person would
        // type it back and be told it is not there.
        for part in brief.trim_start_matches("…/").split('/') {
            assert!(
                ["veryveryverylongdirectoryname", "anotherlongish", "final"].contains(&part),
                "{part:?} is not a whole component of the path"
            );
        }
    }

    #[test]
    fn a_single_name_too_long_for_the_prompt_is_still_kept_whole() {
        let mut here = Where::start();
        here.at = PathBuf::from("/uno-solo-que-es-mucho-mas-largo-que-el-espacio-del-prompt");

        // Keeping nothing would print `…/` and tell the person nothing at all.
        // Overrunning is the lesser failure, and `donde` is the way out of it.
        assert!(
            here.briefly()
                .ends_with("uno-solo-que-es-mucho-mas-largo-que-el-espacio-del-prompt")
        );
    }

    #[test]
    fn donde_stays_exact_even_when_the_prompt_does_not() {
        let mut here = Where::start();
        let long = "/tmp/claude-0/-home-user-thalyx/c16462c8-b3df/scratchpad/casa";
        here.at = PathBuf::from(long);

        // The whole division of labour: the prompt may be lossy, and there is one
        // verb whose answer never is. Shortening both would leave a person with
        // no way to find out where they actually are.
        assert_ne!(here.briefly(), long);
        assert_eq!(here.at().display().to_string(), long);
    }

    #[test]
    fn going_nowhere_in_particular_returns_home() {
        let dir = tempfile::tempdir().unwrap();
        let mut here = Where::start();
        here.go(dir.path().to_str().unwrap()).unwrap();
        assert_ne!(here.at(), Path::new("/home"));

        // `ir` alone is the way back, and it has to work from anywhere — that is
        // the whole reason a person will type it.
        here.go(thalyx_files::HOME).unwrap();
        assert_eq!(here.at(), Path::new("/home"));
    }
}
