//! The colours of the one screen, and the three voices they separate.
//!
//! `vault/02-Arquitectura/La-Pantalla.md` makes one of these a security
//! decision rather than a taste: **the person, the agent and the machine never
//! look the same**, so that provenance can be applied by a reader without
//! reading. That is `vault/11-Seguridad/Marcado-de-Origen.md` one layer further
//! out — the contract already carries per-field provenance, and this is the
//! same fact made visible.
//!
//! The palette is small on purpose. Every colour here is named for the *role*
//! it plays and not for what it looks like, because a role can be checked
//! against the decree and a shade cannot.

/// A colour, opaque, as the canvas stores it.
///
/// Alpha is not here: nothing on this screen is translucent, and a channel
/// nobody sets is a channel somebody eventually forgets to set. Coverage from
/// the rasterizer is a separate argument to the blend, where it belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// `0x00RRGGBB`, which is how [`crate::Canvas`] holds a pixel.
    pub const fn packed(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    pub const fn from_packed(word: u32) -> Self {
        Self {
            r: ((word >> 16) & 0xff) as u8,
            g: ((word >> 8) & 0xff) as u8,
            b: (word & 0xff) as u8,
        }
    }

    /// `self` laid over `under` with `coverage` of it showing, where 255 is
    /// fully `self`.
    ///
    /// Done in `u16` rather than `u8` because `a * c + b * (255 - c)` overflows
    /// a byte for every interesting value of `c`, and the compiler will not say
    /// so — it wraps, and a glyph edge comes out inverted at exactly the pixels
    /// that make text look sharp.
    pub fn over(self, under: Color, coverage: u8) -> Color {
        let mix = |top: u8, bottom: u8| -> u8 {
            let top = top as u16 * coverage as u16;
            let bottom = bottom as u16 * (255 - coverage as u16);
            ((top + bottom) / 255) as u8
        };
        Color {
            r: mix(self.r, under.r),
            g: mix(self.g, under.g),
            b: mix(self.b, under.b),
        }
    }
}

/// The background the whole screen sits on.
pub const INK: Color = Color::rgb(0x0d, 0x0f, 0x12);
/// A panel, which is the only thing that is not the background.
pub const SURFACE: Color = Color::rgb(0x14, 0x18, 0x21);
/// The hairline between two regions. Never a border with thickness: a panel
/// that needs a frame to be found is a panel in the wrong place.
pub const LINE: Color = Color::rgb(0x23, 0x29, 0x35);
/// A panel's heading.
pub const HEADING: Color = Color::rgb(0x6a, 0x76, 0x8a);
/// Prose that is not one of the three voices: labels, a panel's own words.
pub const MUTED: Color = Color::rgb(0x8b, 0x96, 0xa8);

/// **The person.** The only sovereign voice on the screen.
pub const HUMAN: Color = Color::rgb(0xff, 0xff, 0xff);
/// **The agent.** Warm, because it is somebody speaking. Never a fact.
pub const AGENT: Color = Color::rgb(0xe8, 0xb4, 0x4f);
/// **The machine.** Cool, and always monospaced: a path, a size, an id, a line
/// the journal recorded.
pub const FACT: Color = Color::rgb(0xa8, 0xc7, 0xe8);

/// Something went right, said by the machine.
pub const OK: Color = Color::rgb(0x6f, 0xcf, 0x97);
/// Something the machine refused or could not do.
pub const REFUSED: Color = Color::rgb(0xd9, 0x7b, 0x66);

/// **The trusted path, and nothing else.**
///
/// `vault/11-Seguridad/Camino-Confiable.md`: if this colour is on the screen,
/// something is about to change the machine. It is deliberately used by no
/// panel, no turn of the conversation and no error, so that its presence is
/// itself the signal.
pub const TRUST: Color = Color::rgb(0xff, 0x4d, 0x3d);
/// The ground a confirmation takes over, dark enough that [`TRUST`] on it is
/// the brightest thing on the display.
pub const TRUST_GROUND: Color = Color::rgb(0x1a, 0x08, 0x06);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_coverage_is_the_colour_itself_and_none_is_the_one_underneath() {
        assert_eq!(HUMAN.over(INK, 255), HUMAN);
        assert_eq!(HUMAN.over(INK, 0), INK);
    }

    #[test]
    fn a_half_covered_edge_lands_between_the_two_instead_of_wrapping() {
        // The bug this pins: `255 * 128 + 0 * 127` is 32640, which is not a u8.
        // Done in bytes it wraps to 128 for one channel and to something else
        // for another, and a glyph's edges come out brighter than its middle.
        let white = Color::rgb(255, 255, 255);
        let black = Color::rgb(0, 0, 0);
        let half = white.over(black, 128);
        assert!(half.r > 120 && half.r < 135, "landed at {}", half.r);
        assert_eq!(half.r, half.g);
        assert_eq!(half.g, half.b);
    }

    #[test]
    fn a_colour_survives_the_round_trip_through_a_packed_word() {
        for colour in [INK, HUMAN, AGENT, FACT, TRUST] {
            assert_eq!(Color::from_packed(colour.packed()), colour);
        }
    }

    #[test]
    fn the_trusted_path_colour_belongs_to_nothing_else() {
        // The property the decree asks for, asserted rather than trusted to
        // review: if TRUST ever equals another role, its presence stops being a
        // signal and the whole confirmation design quietly loses its point.
        for other in [
            INK, SURFACE, LINE, HEADING, MUTED, HUMAN, AGENT, FACT, OK, REFUSED,
        ] {
            assert_ne!(TRUST, other);
        }
    }
}
