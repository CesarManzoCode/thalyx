//! The four faces the screen has, and everything that turns a string into
//! coverage on a canvas.
//!
//! ## Why there are two families and not one
//!
//! `vault/02-Arquitectura/La-Pantalla.md` gives the typography a job that is
//! not decoration: **which face a line is set in says where the line came
//! from.** Prose is somebody speaking — the person, or the agent proposing.
//! Monospace is the machine stating a fact: a path, an exact size, a module id,
//! a line the journal recorded.
//!
//! That is `vault/11-Seguridad/Marcado-de-Origen.md` made visible. A model that
//! emits `ok  store  /dev/sdb2 ▪ …` produces a *proposal that says that*, and it
//! is drawn in the agent's face and the agent's colour, so it does not resemble
//! the line the machine prints when it really found a store. A reader applies
//! the rule without reading it.
//!
//! ## Why the fonts are inside the binary
//!
//! The image carries the Linux kernel and one program. There is no fontconfig,
//! no `/usr/share/fonts`, and nothing on disk to look a typeface up in — the
//! same situation that makes SQLite compile in rather than link. So the four
//! faces are `include_bytes!` and the rasterizer is pure Rust with no C.
//!
//! Both families are SIL Open Font License 1.1, and the licences ship beside
//! them in `fonts/`.
//!
//! ## Why glyphs are cached and why the cache is not behind a lock
//!
//! Rasterizing `e` at 15px is the same work every time it appears, and a screen
//! has thousands of characters on it. The cache is a plain map owned by
//! [`Typography`], so drawing takes `&mut self`: there is exactly one screen and
//! exactly one thread drawing it, and a lock would be paying for a race that the
//! design does not have.

use crate::canvas::Canvas;
use crate::color::Color;
use std::collections::HashMap;

/// The regular prose face. Humanist and proportional: this is somebody
/// speaking.
const PROSE: &[u8] = include_bytes!("../fonts/WorkSans-Regular.ttf");
const PROSE_BOLD: &[u8] = include_bytes!("../fonts/WorkSans-Bold.ttf");
/// The machine's face. Monospaced, so that a column of sizes lines up and a
/// path cannot be mistaken for prose.
const MONO: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");
const MONO_BOLD: &[u8] = include_bytes!("../fonts/JetBrainsMono-Bold.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Face {
    Prose,
    ProseBold,
    Mono,
    MonoBold,
}

impl Face {
    fn index(self) -> usize {
        match self {
            Face::Prose => 0,
            Face::ProseBold => 1,
            Face::Mono => 2,
            Face::MonoBold => 3,
        }
    }

    /// True when this face is one of the machine's.
    ///
    /// Exists so that the rule in the decree can be *asserted* by a test rather
    /// than kept by review: a machine fact drawn in a prose face is the failure
    /// the whole typographic scheme exists to prevent.
    pub fn is_the_machines(self) -> bool {
        matches!(self, Face::Mono | Face::MonoBold)
    }
}

/// A face, a size and a colour — everything a run of text needs.
#[derive(Debug, Clone, Copy)]
pub struct TextStyle {
    pub face: Face,
    pub size: f32,
    pub colour: Color,
}

impl TextStyle {
    pub const fn new(face: Face, size: f32, colour: Color) -> Self {
        Self { face, size, colour }
    }
}

#[derive(Clone)]
struct Glyph {
    metrics: fontdue::Metrics,
    coverage: Vec<u8>,
}

pub struct Typography {
    faces: [fontdue::Font; 4],
    /// Keyed by face, character and size in whole pixels. Sizes on this screen
    /// come from a small table, so whole pixels never collide two different
    /// renderings into one entry.
    cache: HashMap<(usize, char, u32), Glyph>,
}

impl Typography {
    /// The four faces compiled into the binary.
    ///
    /// # Panics
    ///
    /// If one of them does not parse. That is a corrupt build artifact rather
    /// than a runtime condition — the bytes are `include_bytes!` and cannot
    /// differ between one run and the next — so there is nothing a caller could
    /// do about it and nothing to report.
    pub fn embedded() -> Self {
        let load = |bytes: &[u8], name: &str| {
            fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).unwrap_or_else(
                |why| panic!("the {name} face compiled into this binary is not a font: {why}"),
            )
        };
        Self {
            faces: [
                load(PROSE, "prose"),
                load(PROSE_BOLD, "bold prose"),
                load(MONO, "monospace"),
                load(MONO_BOLD, "bold monospace"),
            ],
            cache: HashMap::new(),
        }
    }

    fn glyph(&mut self, face: Face, ch: char, size: f32) -> &Glyph {
        let key = (face.index(), ch, size as u32);
        self.cache.entry(key).or_insert_with(|| {
            let (metrics, coverage) = self.faces[face.index()].rasterize(ch, size);
            Glyph { metrics, coverage }
        })
    }

    /// How far the pen moves for one character, without drawing it.
    pub fn advance(&mut self, face: Face, ch: char, size: f32) -> f32 {
        self.glyph(face, ch, size).metrics.advance_width
    }

    /// The width of a run, in pixels.
    pub fn measure(&mut self, face: Face, size: f32, text: &str) -> f32 {
        text.chars().map(|ch| self.advance(face, ch, size)).sum()
    }

    /// Baseline to baseline for consecutive lines in this face.
    pub fn line_height(&self, face: Face, size: f32) -> f32 {
        self.faces[face.index()]
            .horizontal_line_metrics(size)
            .map(|m| m.new_line_size)
            // A face with no horizontal line metrics is not a face this screen
            // uses, but guessing 1.4em is a better failure than dividing by a
            // height of zero and drawing every line on top of the first.
            .unwrap_or(size * 1.4)
    }

    /// How far the top of a line sits above its baseline.
    pub fn ascent(&self, face: Face, size: f32) -> f32 {
        self.faces[face.index()]
            .horizontal_line_metrics(size)
            .map(|m| m.ascent)
            .unwrap_or(size)
    }

    /// Draw `text` with its baseline at `baseline`, returning where the pen
    /// ended up.
    pub fn draw(
        &mut self,
        canvas: &mut Canvas,
        x: f32,
        baseline: f32,
        text: &str,
        style: TextStyle,
    ) -> f32 {
        let mut pen = x;
        for ch in text.chars() {
            let glyph = self.glyph(style.face, ch, style.size).clone();
            let left = pen.round() as i32 + glyph.metrics.xmin;
            // fontdue measures `ymin` upward from the baseline to the bottom of
            // the bitmap, and the bitmap's own rows run downward. Forgetting to
            // add the height puts every glyph one body too low, which reads as
            // the text being clipped by the line below rather than as a sign
            // error.
            let top = baseline.round() as i32 - (glyph.metrics.ymin + glyph.metrics.height as i32);
            for row in 0..glyph.metrics.height {
                for column in 0..glyph.metrics.width {
                    let coverage = glyph.coverage[row * glyph.metrics.width + column];
                    canvas.blend(
                        left + column as i32,
                        top + row as i32,
                        style.colour,
                        coverage,
                    );
                }
            }
            pen += glyph.metrics.advance_width;
        }
        pen
    }

    /// Draw `text`, and if it does not fit in `max_width`, end it with `…`.
    ///
    /// Returns whether anything was cut, because a panel that silently drops the
    /// end of a path is how somebody reads `/home/cesar/proyect` and believes
    /// it.
    pub fn draw_within(
        &mut self,
        canvas: &mut Canvas,
        x: f32,
        baseline: f32,
        max_width: f32,
        text: &str,
        style: TextStyle,
    ) -> bool {
        if self.measure(style.face, style.size, text) <= max_width {
            self.draw(canvas, x, baseline, text, style);
            return false;
        }
        let ellipsis = self.advance(style.face, '…', style.size);
        let mut kept = String::new();
        let mut used = 0.0;
        for ch in text.chars() {
            let next = self.advance(style.face, ch, style.size);
            if used + next + ellipsis > max_width {
                break;
            }
            used += next;
            kept.push(ch);
        }
        kept.push('…');
        self.draw(canvas, x, baseline, &kept, style);
        true
    }

    /// Break `text` into lines no wider than `max_width`.
    ///
    /// Breaks at spaces; a single word longer than the line is broken where it
    /// runs out, because the alternative is one line that leaves the panel and
    /// is drawn over whatever is beside it.
    pub fn wrap(&mut self, face: Face, size: f32, max_width: f32, text: &str) -> Vec<String> {
        let mut lines = Vec::new();
        for paragraph in text.split('\n') {
            let mut line = String::new();
            let mut width = 0.0;
            for word in paragraph.split_inclusive(' ') {
                let word_width = self.measure(face, size, word);
                if !line.is_empty() && width + word_width > max_width {
                    lines.push(std::mem::take(&mut line).trim_end().to_string());
                    width = 0.0;
                }
                if word_width > max_width {
                    // Longer than a whole line even on its own: place it a
                    // character at a time so it stays inside the region.
                    for ch in word.chars() {
                        let advance = self.advance(face, ch, size);
                        if width + advance > max_width && !line.is_empty() {
                            lines.push(std::mem::take(&mut line));
                            width = 0.0;
                        }
                        line.push(ch);
                        width += advance;
                    }
                    continue;
                }
                line.push_str(word);
                width += word_width;
            }
            lines.push(line.trim_end().to_string());
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{HUMAN, INK};

    fn faces() -> Typography {
        Typography::embedded()
    }

    #[test]
    fn every_letter_spanish_needs_exists_in_all_four_faces() {
        // The machine is used in Spanish. A missing `ñ` is not a cosmetic
        // problem: fontdue answers a glyph that is not there with an empty
        // bitmap, so the character silently disappears from a filename and
        // nobody sees a failure — they see a different filename.
        let mut typography = faces();
        for face in [Face::Prose, Face::ProseBold, Face::Mono, Face::MonoBold] {
            for ch in "áéíóúüñÁÉÍÓÚÜÑ¿¡«»—…·".chars() {
                let glyph = typography.glyph(face, ch, 16.0);
                assert!(
                    glyph.metrics.width > 0 && glyph.metrics.height > 0,
                    "{face:?} has no glyph for {ch:?}"
                );
            }
        }
    }

    #[test]
    fn the_machines_faces_are_the_monospaced_ones_and_only_those() {
        assert!(Face::Mono.is_the_machines());
        assert!(Face::MonoBold.is_the_machines());
        assert!(!Face::Prose.is_the_machines());
        assert!(!Face::ProseBold.is_the_machines());
    }

    #[test]
    fn the_machines_face_gives_every_character_the_same_advance() {
        // The property a column of sizes depends on. If this stopped being
        // true, `12 GiB` under `447 GiB` would not line up and the panel would
        // look broken without anything having thrown an error.
        let mut typography = faces();
        let width = typography.advance(Face::Mono, 'm', 15.0);
        for ch in "il1 W/.9ñ".chars() {
            assert_eq!(typography.advance(Face::Mono, ch, 15.0), width, "at {ch:?}");
        }
    }

    #[test]
    fn the_prose_face_does_not_give_every_character_the_same_advance() {
        // The control for the test above: a proportional face that measured
        // like a monospaced one would mean the two families got swapped, and
        // then provenance would be carried by nothing.
        let mut typography = faces();
        assert_ne!(
            typography.advance(Face::Prose, 'i', 15.0),
            typography.advance(Face::Prose, 'W', 15.0)
        );
    }

    #[test]
    fn a_glyph_lands_above_its_baseline_and_not_below_it() {
        // The sign error this pins: forgetting `+ height` when going from
        // fontdue's y-up metrics to the canvas's y-down rows puts the whole line
        // one body lower, which looks like clipping rather than like a bug in
        // one expression.
        let mut typography = faces();
        let mut canvas = Canvas::new(40, 40, INK);
        let baseline = 30.0;
        typography.draw(
            &mut canvas,
            4.0,
            baseline,
            "H",
            TextStyle::new(Face::Mono, 20.0, HUMAN),
        );
        let mut highest = None;
        for y in 0..40 {
            for x in 0..40 {
                if canvas.pixel(x, y) != Some(INK) {
                    highest.get_or_insert(y);
                }
            }
        }
        let highest = highest.expect("nothing was drawn at all");
        assert!(
            highest < baseline as u32,
            "the H started at row {highest}, at or below the baseline"
        );
        assert!(
            highest > 5,
            "the H started at row {highest}, far higher than a 20px body"
        );
    }

    #[test]
    fn text_that_fits_is_not_cut_and_text_that_does_not_says_so() {
        let mut typography = faces();
        let mut canvas = Canvas::new(400, 40, INK);
        let style = TextStyle::new(Face::Mono, 14.0, HUMAN);
        assert!(!typography.draw_within(&mut canvas, 0.0, 20.0, 380.0, "/home/cesar", style));
        assert!(typography.draw_within(
            &mut canvas,
            0.0,
            20.0,
            40.0,
            "/home/cesar/proyectos",
            style
        ));
    }

    #[test]
    fn no_wrapped_line_is_wider_than_the_width_it_was_given() {
        let mut typography = faces();
        let text = "Thalyx es el sistema operativo. El kernel de Linux es un componente \
                    que Thalyx gestiona, no el anfitrión sobre el que descansa.";
        for line in typography.wrap(Face::Prose, 15.0, 200.0, text) {
            let width = typography.measure(Face::Prose, 15.0, &line);
            assert!(width <= 200.0, "{line:?} came out {width} wide");
        }
    }

    #[test]
    fn a_word_longer_than_the_line_is_broken_instead_of_leaving_the_panel() {
        let mut typography = faces();
        let lines = typography.wrap(
            Face::Mono,
            14.0,
            60.0,
            "/home/cesar/proyectos/thalyx/crates",
        );
        assert!(lines.len() > 1, "the long path was left on one line");
        for line in &lines {
            assert!(
                typography.measure(Face::Mono, 14.0, line) <= 60.0,
                "{line:?}"
            );
        }
    }

    #[test]
    fn wrapping_keeps_every_character_that_went_in() {
        // A wrap that drops text is the worst kind of bug on this screen: it
        // produces a sentence that is grammatical and wrong.
        let mut typography = faces();
        let text = "instalar dev.thalyx.greeter y después revisar los permisos";
        // Compared with the whitespace taken out of both sides, so the claim
        // holds for the hard-break path too: a word longer than the line comes
        // back split across two lines, and the characters are still all there.
        for width in [60.0, 120.0, 400.0] {
            let kept: String = typography
                .wrap(Face::Prose, 15.0, width, text)
                .concat()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            let went_in: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            assert_eq!(kept, went_in, "at {width} wide");
        }
    }

    #[test]
    fn an_empty_line_in_the_middle_of_a_paragraph_survives_the_wrap() {
        let mut typography = faces();
        let lines = typography.wrap(Face::Prose, 15.0, 400.0, "uno\n\ndos");
        assert_eq!(
            lines,
            vec!["uno".to_string(), String::new(), "dos".to_string()]
        );
    }
}
