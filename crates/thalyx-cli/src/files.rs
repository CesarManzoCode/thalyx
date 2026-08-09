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

/// `donde` — the one verb whose answer is always available.
pub fn where_am_i(here: &Where) {
    println!();
    println!("  {}", here.at().display());
    println!();
}

/// `ir [ruta]` — with nothing after it, back to `/home`.
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

/// `ver [ruta]` — what is here, or what is there.
pub fn look(here: &Where, rest: &str) {
    let target = if rest.is_empty() {
        here.at().to_path_buf()
    } else {
        thalyx_files::resolve(here.at(), rest)
    };

    println!();
    match thalyx_files::list(&target) {
        Ok(listing) => print_listing(&target, &listing),
        // `ver` on a file is a reasonable thing to type, so it answers instead
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

fn print_listing(target: &Path, listing: &Listing) {
    println!("  {}", target.display());

    if listing.entries.is_empty() && listing.unreadable.is_empty() {
        println!();
        println!("  nothing here");
        return;
    }

    println!();
    for entry in &listing.entries {
        let name = entry.name.to_string_lossy();
        match &entry.kind {
            // The trailing slash is the whole difference between a name a person
            // can enter and one they can open, and it costs one character.
            Kind::Directory => println!("    {name}/"),
            Kind::File { bytes } => println!("    {:<32} {}", name, Size(*bytes)),
            Kind::Link { to, broken } => {
                let note = if *broken { "  — broken" } else { "" };
                println!("    {name} -> {}{note}", to.display());
            }
            Kind::Other(what) => println!("    {:<32} {what}", name),
        }
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

/// `ver` aimed at something that is not a directory.
fn print_one(target: &Path) {
    match target.symlink_metadata() {
        Ok(meta) if meta.is_file() => {
            println!("  {:<32} {}", target.display(), Size(meta.len()));
        }
        Ok(_) => println!("  {}", target.display()),
        Err(error) => println!("  {} could not be read: {error}", target.display()),
    }
}

/// `leer <archivo>` — show a file, or say why showing it would be a mistake.
pub fn read(here: &Where, rest: &str) {
    if rest.is_empty() {
        println!();
        println!("  Which file. `ver` lists what is here.");
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
