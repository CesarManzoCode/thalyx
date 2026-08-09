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

use std::path::{Path, PathBuf};
use thalyx_files::{Excerpt, FileError, Kind, Listing, Size};

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
pub fn clear() {
    use std::io::Write;
    print!("\x1b[2J\x1b[H");
    // Flushed here because what follows is a prompt printed with `print!` and no
    // newline of its own; leaving this in the buffer would put the prompt on
    // screen before the screen was cleared.
    let _ = std::io::stdout().flush();
}

/// `pwd` — the one verb whose answer is always available.
pub fn where_am_i(here: &Where) {
    println!();
    println!("  {}", here.at().display());
    println!();
}

/// `cd [ruta]` — with nothing after it, back to `/home`.
pub fn go(here: &mut Where, rest: &str) {
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
        Ok(()) => {}
        Err(error) => {
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
}

impl Asked {
    /// Both spellings of every flag, because the person chose to have both.
    ///
    /// The Spanish words are whole arguments and not letters, so `todo` cannot
    /// collide with a `-t` that might exist later.
    fn parse(rest: &str) -> Self {
        let mut asked = Asked::default();
        let mut place = Vec::new();

        for word in rest.split_whitespace() {
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

fn screen_width() -> usize {
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

/// `ls [-a] [-l] [ruta]` — what is here, or what is there.
pub fn look(here: &Where, rest: &str) {
    let asked = Asked::parse(rest);
    let target = if asked.place.is_empty() {
        here.at().to_path_buf()
    } else {
        thalyx_files::resolve(here.at(), &asked.place)
    };

    println!();
    match thalyx_files::list(&target) {
        Ok(listing) => print_listing(&target, &listing, &asked),
        // `ls` on a file is a reasonable thing to type, so it answers instead
        // of correcting: the person wanted to know about that file.
        Err(FileError::Unreadable { .. } | FileError::Absent(_))
            if target.is_file() || target.symlink_metadata().is_ok() =>
        {
            print_one(&target)
        }
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

/// `ls` aimed at something that is not a directory.
fn print_one(target: &Path) {
    match target.symlink_metadata() {
        Ok(meta) if meta.is_file() => {
            println!("  {:<32} {}", target.display(), Size(meta.len()));
        }
        Ok(_) => println!("  {}", target.display()),
        Err(error) => println!("  {} could not be read: {error}", target.display()),
    }
}

/// `cat <archivo>` — show a file, or say why showing it would be a mistake.
pub fn read(here: &Where, rest: &str) {
    if rest.is_empty() {
        println!();
        println!("  Which file. `ls` lists what is here.");
        println!();
        return;
    }

    let target = thalyx_files::resolve(here.at(), rest);
    println!();
    match thalyx_files::read(&target) {
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

#[cfg(test)]
mod tests {
    use super::*;

    // ──────────────────────────────────────────────── what the person asked for

    #[test]
    fn a_bare_ls_asks_for_nothing_in_particular() {
        let asked = Asked::parse("");
        assert!(!asked.all);
        assert!(!asked.long);
        assert!(asked.place.is_empty());
    }

    #[test]
    fn grouped_flags_are_read_as_the_flags_they_are_and_not_as_a_folder() {
        // The failure this prevents: `-la` taken as a place, answering "not
        // there" for something the person typed correctly.
        let asked = Asked::parse("-la");
        assert!(asked.all && asked.long);
        assert!(asked.place.is_empty(), "got {:?}", asked.place);
    }

    #[test]
    fn flags_and_a_place_together_keep_the_place() {
        let asked = Asked::parse("-a Documentos");
        assert!(asked.all);
        assert_eq!(asked.place, "Documentos");
    }

    #[test]
    fn both_spellings_of_a_flag_mean_the_same_thing() {
        // Cesar chose to keep both vocabularies, so the flags have both too.
        assert_eq!(Asked::parse("-a"), Asked::parse("todo"));
        assert_eq!(Asked::parse("-l"), Asked::parse("detalles"));
    }

    #[test]
    fn an_unknown_flag_is_not_quietly_swallowed() {
        let asked = Asked::parse("-z");
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
        assert_eq!(Asked::parse("-").place, "-");
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
