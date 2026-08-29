//! What the kernel turns a key press into, and how Thalyx says which.
//!
//! ## The defect this exists for
//!
//! Thalyx talks to the person in Spanish. Every sentence the machine says, every
//! verb it takes, the whole vault. `thalyx-screen` carries a comment on the font
//! cache saying a missing `ñ` is not a cosmetic problem, and it warms the glyphs
//! for `áéíóúüñÁÉÍÓÚÜÑ¿¡` so the display can draw them.
//!
//! **And until this module there was no way to type one.**
//!
//! Not a pending item, not a limitation anybody had written down: a `grep` of
//! the whole repository for `keymap` came back empty. The kernel carries one
//! keymap compiled into it — `drivers/tty/vt/defkeymap.c`, US QWERTY — and the
//! program that replaces it on every other Linux is `loadkeys`, which the image
//! does not have and cannot have, because the image is the kernel and one
//! program. So on a Thalyx machine the key a Latin American keyboard prints `ñ`
//! on sends `;`, and `á` cannot be typed at all: the default map has no dead
//! keys.
//!
//! An operating system whose every sentence is in Spanish, in which Spanish
//! cannot be written. Found by asking what a whole day inside it would need.
//!
//! ## Why the tables are generated and never edited
//!
//! A layout is data about the world — which physical key carries `ñ`, what
//! Shift makes of it, what AltGr does — and rule 6 of `CLAUDE.md` says a fixture
//! written by the person who needs it proves only that it matches their model of
//! the format. Nobody here has memorised twelve modifier tables of 256 keycodes,
//! and a layout that is subtly wrong is worse than one that is missing: it is
//! found one key at a time, months later, by somebody who assumes they mistyped.
//!
//! So the tables come from `kbd`'s own files, resolved by `loadkeys --mktable`
//! and converted by `dev/keymap-table.py`. Reading `la-latin1.kmap` directly
//! would have been the mistake: it is a forty-line **diff** against two includes
//! and means nothing without them.
//!
//! ## Why the whole table and not the difference
//!
//! Setting only the keys that differ from the kernel's built-in map would be a
//! third of the data and would make the result depend on what the kernel
//! happened to start with. Rule 9: a layout that is *this table, entirely* is
//! the same layout on every machine, and one built by patching whatever was
//! there is a layout nobody can state.
//!
//! ## What this module does not do
//!
//! It does not touch a console. Everything here is a table and a lookup, so it
//! is tested on a machine with no keyboard at all — which is every machine this
//! project's tests run on. The two ioctls that load it live in `thalyx-syscall`,
//! and what they cost when they are wrong is a machine whose keyboard types the
//! wrong letters, so the check that they worked asks **the kernel** what it now
//! holds rather than asking Thalyx what it sent. Rule 5.

mod defkeymap;
mod la_latin1;

/// One modifier table: what each of the 256 keycodes produces.
///
/// The values are the kernel's own encoding, `K(type, value)` from
/// `linux/keyboard.h`, carried through unaltered. Re-encoding them here would be
/// a second implementation of a format that already exists, and the first thing
/// it would get wrong is the types that are not letters.
pub type MapEntries = [u16; 256];

/// A dead key and what it makes of the letter after it.
///
/// This is how `á` is typed: `'` then `a`. The kernel calls it the accent table
/// and it is set separately from the keymap — a layout loaded without it has a
/// dead key that swallows a keystroke and produces nothing, which reads as a
/// broken keyboard rather than as a missing table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accent {
    /// The dead key, as the character it would otherwise have been.
    pub dead: u8,
    /// The key pressed after it.
    pub base: u8,
    /// What the two make.
    pub made: u16,
}

/// A whole layout, as the kernel would hold it.
pub struct Layout {
    /// What it is called, in the same words `kbd` calls it, so that anybody
    /// looking for where it came from finds the file it came from.
    pub name: &'static str,
    /// The modifier tables that this layout defines, by their kernel index.
    ///
    /// Sparse on purpose: the indices are a bitmask of modifiers and a layout
    /// defines the combinations it has an opinion about. Filling the gaps with
    /// empty tables would replace *the kernel has nothing here* with *this
    /// layout says nothing happens here*, which are different keyboards.
    pub tables: &'static [(u8, &'static MapEntries)],
    pub accents: &'static [Accent],
}

/// What one keymap entry produces, in the kernel's own encoding.
///
/// **The 0xf000 is not decoration and getting it wrong is silent.** The kernel
/// reads an entry as a plain Unicode code point when its high byte is below
/// `0xf0`, and as `K(type, value)` — the types in `linux/keyboard.h` — when it
/// is at or above it, with `0xf0` subtracted first. So `0xf0f1` is
/// `KT_LATIN`/`0xf1`, the letter `ñ`, and reading its type as `0xf0` instead of
/// as `0` says the entry is not a letter at all.
///
/// This lives here rather than in the test that found it, because `teclado` has
/// to answer the same question to say what a key does, and two decoders of one
/// encoding is how they come to disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Produces {
    /// A character. Either a direct Unicode entry or a `KT_LATIN`/`KT_LETTER`.
    Letter(char),
    /// A dead key: it produces nothing until the next key, and what the two make
    /// is in [`Layout::accents`].
    Dead,
    /// Something that is not text — a modifier, a function key, a console
    /// switch. Named rather than folded into "no letter", because *this key
    /// makes no character* and *this entry is a letter this build cannot
    /// represent* are different facts. Rule 10.
    Something,
}

/// `KT_LATIN`. The header says the code depends on this being zero.
const KT_LATIN: u16 = 0;
/// `KT_DEAD` — the acute and the diaeresis of a Spanish layout.
const KT_DEAD: u16 = 4;
/// `KT_LETTER` — a latin that CapsLock is allowed to act on.
const KT_LETTER: u16 = 11;
/// Below this in the high byte, an entry is a Unicode code point and not a type.
const TYPED: u16 = 0xf0;

pub fn produces(entry: u16) -> Produces {
    let high = entry >> 8;
    if high < TYPED {
        return match char::from_u32(u32::from(entry)) {
            Some(letter) => Produces::Letter(letter),
            None => Produces::Something,
        };
    }
    match high - TYPED {
        KT_LATIN | KT_LETTER => match char::from_u32(u32::from(entry & 0xff)) {
            Some(letter) => Produces::Letter(letter),
            None => Produces::Something,
        },
        KT_DEAD => Produces::Dead,
        _ => Produces::Something,
    }
}

impl Layout {
    /// What this layout makes of one keycode with no modifier held.
    ///
    /// For asking a question about a layout without a console — which is the
    /// only way this can be asked in a container, and the only way it can be
    /// asked *before* loading one on a machine that has a console.
    pub fn plainly(&self, keycode: u8) -> Option<u16> {
        self.tables
            .iter()
            .find(|(index, _)| *index == 0)
            .map(|(_, table)| table[keycode as usize])
    }
}

/// Every layout this machine carries.
///
/// Two, and the second one is not decoration: a machine whose keyboard has just
/// been changed to something the person cannot read needs a way back, and on the
/// image there is no second terminal to type it from. `defkeymap` is the
/// kernel's own compiled-in map, so `teclado ingles` puts back exactly what the
/// machine booted with rather than something close to it.
pub static LAYOUTS: &[&Layout] = &[&la_latin1::LAYOUT, &defkeymap::LAYOUT];

/// The layout a Thalyx machine comes up on.
pub const AT_BOOT: &str = "la-latin1";

/// Find one by name, or by the word a person is likely to type for it.
pub fn by_name(asked: &str) -> Option<&'static Layout> {
    let asked = asked.trim().to_lowercase();
    let wanted = match asked.as_str() {
        // The names are in Spanish because the person typing them is, and the
        // `kbd` name is kept beside each so that somebody reading
        // `dev/keymap-table.py` can see which file produced which.
        "latino" | "latinoamericano" | "la" | "la-latin1" | "es" => "la-latin1",
        "ingles" | "inglés" | "us" | "eeuu" | "defkeymap" => "defkeymap",
        _ => return None,
    };
    LAYOUTS.iter().copied().find(|layout| layout.name == wanted)
}

/// The names a person may type, for the catalogue and for the refusal.
pub fn names() -> [&'static str; 2] {
    ["latino", "ingles"]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keycode of the key that carries `ñ` on a Latin American keyboard,
    /// which is the key US layouts print `;` on.
    const NTILDE_KEY: u8 = 39;
    /// The key that carries `¿`, which a US layout has no character for at all.
    const QUESTION_KEY: u8 = 13;

    fn character(entry: u16) -> Option<char> {
        match produces(entry) {
            Produces::Letter(letter) => Some(letter),
            _ => None,
        }
    }

    /// The defect, stated as the difference between the two tables.
    ///
    /// Not «the Latin American layout has an ñ» on its own — that would pass
    /// against a table that was the same everywhere. The claim is that this is
    /// **a difference from what the machine boots with**, and the second half is
    /// what makes the first half worth anything. Rule 4, on data.
    #[test]
    fn the_key_that_carries_ene_types_a_semicolon_on_the_map_the_kernel_ships() {
        let latin = by_name("latino").expect("the Latin American layout");
        let kernel = by_name("ingles").expect("the kernel's own map");

        assert_eq!(
            character(latin.plainly(NTILDE_KEY).expect("a plain table")),
            Some('ñ')
        );
        assert_eq!(
            character(kernel.plainly(NTILDE_KEY).expect("a plain table")),
            Some(';'),
            "the map a Thalyx machine boots with has something other than `;` on \
             the ñ key, so the reason this module exists is not the reason written \
             at the top of it"
        );

        assert_eq!(
            character(latin.plainly(QUESTION_KEY).expect("a plain table")),
            Some('¿')
        );
    }

    /// Every letter the screen warms a glyph for has to be reachable, or the
    /// display can draw a character the keyboard cannot produce — which is the
    /// state this module found the machine in.
    #[test]
    fn every_letter_spanish_needs_can_be_typed_somehow() {
        let latin = by_name("latino").expect("the Latin American layout");

        let mut reachable: Vec<char> = Vec::new();
        for (_, table) in latin.tables {
            reachable.extend(table.iter().filter_map(|entry| character(*entry)));
        }
        // Composed with a dead key counts as typed: `á` is `'` then `a`, and a
        // test that demanded a key of its own would demand a keyboard nobody
        // makes.
        reachable.extend(
            latin
                .accents
                .iter()
                .filter_map(|accent| char::from_u32(u32::from(accent.made))),
        );

        for needed in "áéíóúüñÁÉÍÓÚÜÑ¿¡".chars() {
            assert!(
                reachable.contains(&needed),
                "`{needed}` is drawn by the screen and cannot be typed on this layout"
            );
        }
    }

    /// A dead key with no accent table swallows the keystroke and produces
    /// nothing, which reads as a broken keyboard rather than as a missing table.
    #[test]
    fn a_layout_with_a_dead_key_carries_the_table_that_makes_it_mean_something() {
        let latin = by_name("latino").expect("the Latin American layout");

        let has_a_dead_key = latin
            .tables
            .iter()
            .any(|(_, table)| table.iter().any(|entry| produces(*entry) == Produces::Dead));
        assert!(has_a_dead_key, "the acute key is gone from this layout");
        assert!(
            !latin.accents.is_empty(),
            "a dead key with nothing to compose is a key that eats what follows it"
        );

        // The one a person types first and most.
        assert!(latin.accents.contains(&Accent {
            dead: b'\'',
            base: b'a',
            made: 0xe1,
        }));
    }

    /// Rule 9 on the way back. A machine whose keyboard has just been changed to
    /// something unreadable has no second terminal on the image to fix it from,
    /// so the return trip has to be reachable by typing letters that are in the
    /// same place on both layouts.
    #[test]
    fn the_way_back_is_typable_on_the_layout_it_is_a_way_back_from() {
        let latin = by_name("latino").expect("the Latin American layout");
        let kernel = by_name("ingles").expect("the kernel's own map");

        for word in ["teclado", "ingles"] {
            for letter in word.chars() {
                let on_each = [latin, kernel].map(|layout| {
                    layout.tables.iter().find_map(|(index, table)| {
                        (*index == 0)
                            .then(|| table.iter().position(|e| character(*e) == Some(letter)))
                            .flatten()
                    })
                });
                assert_eq!(
                    on_each[0], on_each[1],
                    "`{letter}` is on a different key in the two layouts, so the \
                     words that put the keyboard back cannot be typed on the \
                     keyboard they put back"
                );
            }
        }
    }

    /// The encoding, pinned, because the first version of these tests read the
    /// type as `0xf0` and concluded the Latin American layout has no `ñ` on it.
    ///
    /// The table was right and the reader was wrong — rule 5, the instrument
    /// includes the harness — and the only reason it was caught is that the
    /// claim beside it was specific enough to be obviously false.
    #[test]
    fn an_entry_is_read_the_way_the_kernel_reads_it() {
        // `KT_LATIN` with `ñ` in the low byte, which is what keycode 39 holds.
        assert_eq!(produces(0xf0f1), Produces::Letter('ñ'));
        // `KT_LETTER` with `q`, which is what an ordinary letter key holds.
        assert_eq!(produces(0xfb71), Produces::Letter('q'));
        // `K_DACUTE`, the key that makes `á` out of the `a` after it.
        assert_eq!(produces(0xf401), Produces::Dead);
        // A modifier: it makes no character and that is not a failure to read.
        assert_eq!(produces(0xf702), Produces::Something);
        // Below 0xf000 the entry is a Unicode code point outright, which is the
        // half of the encoding neither layout here exercises and the half a
        // reader that only handled types would get silently wrong.
        assert_eq!(produces(0x0107), Produces::Letter('ć'));
    }

    #[test]
    fn a_layout_nobody_has_is_not_silently_the_default() {
        // Rule 9. Answering an unknown name with the boot layout would make a
        // typo a silent change of keyboard.
        assert!(by_name("dvorak").is_none());
        assert!(by_name("").is_none());
        for name in names() {
            assert!(by_name(name).is_some(), "`{name}` is advertised and absent");
        }
        assert!(
            by_name(AT_BOOT).is_some(),
            "the machine boots into a layout it does not have"
        );
    }
}
