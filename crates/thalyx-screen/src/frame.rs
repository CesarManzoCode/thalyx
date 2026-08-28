//! Composing one frame: a [`Screen`] in, a [`Canvas`] out, and nothing else.
//!
//! ## The two rules this file is written to keep
//!
//! **A confirmation takes the whole display.** When
//! [`Screen::confirmation`] is set, this file draws that and returns without
//! touching a panel, a turn or the prompt. It is written as an early return
//! rather than as a layer drawn on top, because *on top* is a property that can
//! be lost by reordering two statements, and this one is the trusted path.
//!
//! **Colour says who spoke; face says what kind of text it is.** The person is
//! white, the agent is amber, the machine is cool blue. Prose is somebody
//! speaking; monospace is exact machine text, where a character that moved is a
//! different answer. The pair that must never happen is an agent turn drawn in
//! the machine's face or the machine's colour — that is a proposal wearing the
//! clothes of a fact, which is exactly what
//! `vault/11-Seguridad/Marcado-de-Origen.md` exists to prevent. There is a test
//! asserting it, because it is the kind of rule that a later edit breaks by
//! accident.
//!
//! The prompt is the one place a person writes in the machine's grammar, so it
//! is set in monospace and coloured white: the face says the text is exact, the
//! colour says whose it is.

use crate::canvas::{Canvas, Rect};
use crate::color::{self, Color};
use crate::layout::{Layout, Metrics};
use crate::state::{Bar, Confirmation, Guard, Panel, Prompt, Row, Screen, Tone, Turn, Voice};
use crate::text::{Face, TextStyle, Typography};

/// Draw `screen` at `width` × `height`.
pub fn compose(screen: &Screen, typography: &mut Typography, width: u32, height: u32) -> Canvas {
    let layout = Layout::for_size(width, height);
    let mut canvas = Canvas::new(width, height, color::INK);

    // Not a layer over the rest: the rest is not drawn. See the module note.
    if let Some(confirmation) = &screen.confirmation {
        draw_confirmation(&mut canvas, typography, &layout, confirmation);
        return canvas;
    }

    draw_bar(&mut canvas, typography, &layout, &screen.bar);
    if let Some(column) = layout.left {
        draw_column(
            &mut canvas,
            typography,
            &layout,
            column,
            &screen.left,
            Side::Left,
        );
    }
    if let Some(column) = layout.right {
        draw_column(
            &mut canvas,
            typography,
            &layout,
            column,
            &screen.right,
            Side::Right,
        );
    }
    draw_conversation(&mut canvas, typography, &layout, &screen.conversation);
    draw_prompt(&mut canvas, typography, &layout, &screen.prompt);
    canvas
}

fn voice_colour(voice: Voice) -> Color {
    match voice {
        Voice::Person => color::HUMAN,
        Voice::Agent => color::AGENT,
        Voice::Machine => color::FACT,
    }
}

fn voice_face(voice: Voice) -> Face {
    match voice {
        Voice::Person | Voice::Agent => Face::Prose,
        Voice::Machine => Face::Mono,
    }
}

fn tone_colour(tone: Tone) -> Color {
    match tone {
        Tone::Plain => color::FACT,
        Tone::Ok => color::OK,
        Tone::Refused => color::REFUSED,
        Tone::Muted => color::MUTED,
    }
}

fn guard_colour(guard: Guard) -> Color {
    match guard {
        Guard::Enforcing => color::OK,
        // Not the trusted path's colour, and not the same as absent: observing
        // is a machine that works and is not binding, which is a fact and not
        // an alarm.
        Guard::Observing => color::AGENT,
        Guard::Absent => color::REFUSED,
        Guard::Unknown => color::MUTED,
    }
}

/// A heading, set in capitals with the letters pushed apart.
///
/// Tracked by hand because a heading at 12px in capitals reads as a solid block
/// otherwise, and there is no shaping engine here to ask.
fn draw_heading(
    canvas: &mut Canvas,
    typography: &mut Typography,
    x: f32,
    baseline: f32,
    max_width: f32,
    text: &str,
    metrics: &Metrics,
) {
    let style = TextStyle::new(Face::ProseBold, metrics.heading, color::HEADING);
    let tracking = (1.2 * metrics.scale).max(1.0);
    let mut pen = x;
    for ch in text.to_uppercase().chars() {
        if pen - x > max_width {
            return;
        }
        pen = typography.draw(canvas, pen, baseline, &ch.to_string(), style) + tracking;
    }
}

fn draw_bar(canvas: &mut Canvas, typography: &mut Typography, layout: &Layout, bar: &Bar) {
    let metrics = &layout.metrics;
    canvas.fill(layout.bar, color::SURFACE);
    canvas.hairline_h(0, layout.bar.bottom() - 1, layout.bar.width, color::LINE);

    let baseline = layout.bar.top as f32 + layout.bar.height as f32 * 0.66;
    let mut pen = metrics.padding;

    pen = typography.draw(
        canvas,
        pen,
        baseline,
        "Thalyx",
        TextStyle::new(Face::ProseBold, metrics.bar + 1.0, color::HUMAN),
    );
    pen += metrics.padding;

    // The machine's own facts, in the machine's face. `store` is a device path
    // and a label; showing it in prose would make it look like a name somebody
    // chose rather than a thing that was found.
    for (text, colour) in [
        (bar.machine.as_str(), color::MUTED),
        (bar.store.as_str(), color::FACT),
    ] {
        if text.is_empty() {
            continue;
        }
        pen = typography.draw(
            canvas,
            pen,
            baseline,
            text,
            TextStyle::new(Face::Mono, metrics.bar, colour),
        );
        pen = typography.draw(
            canvas,
            pen + metrics.padding * 0.5,
            baseline,
            "·",
            TextStyle::new(Face::Mono, metrics.bar, color::LINE),
        ) + metrics.padding * 0.5;
    }

    typography.draw(
        canvas,
        pen,
        baseline,
        bar.guard.words(),
        TextStyle::new(Face::Mono, metrics.bar, guard_colour(bar.guard)),
    );

    let clock_style = TextStyle::new(Face::Mono, metrics.bar, color::MUTED);
    let clock_width = typography.measure(Face::Mono, metrics.bar, &bar.clock);
    typography.draw(
        canvas,
        layout.bar.right() as f32 - metrics.padding - clock_width,
        baseline,
        &bar.clock,
        clock_style,
    );
}

enum Side {
    Left,
    Right,
}

/// How tall this panel wants to be: its heading, its rows, and the room around
/// them.
///
/// Measured rather than shared out. Giving every panel an equal share of the
/// column — or a share in proportion to its row count — leaves a void under
/// each one and strands the last against the bottom edge, which is what the
/// first frame ever drawn looked like. A panel is as tall as what it says.
fn natural_height(
    typography: &mut Typography,
    metrics: &Metrics,
    width: u32,
    panel: &Panel,
) -> u32 {
    let inner_width = width.saturating_sub(metrics.padding as u32 * 2) as f32;
    let mut height = metrics.padding;
    height += typography.line_height(Face::Prose, metrics.small) * 1.35;
    for row in &panel.rows {
        height += match row {
            Row::Fact { .. } | Row::Pair { .. } => typography.line_height(Face::Mono, metrics.fact),
            Row::Note(text) => {
                let lines = typography
                    .wrap(Face::Prose, metrics.small, inner_width, text)
                    .len();
                typography.line_height(Face::Prose, metrics.small) * lines as f32
            }
        };
    }
    height += metrics.padding;
    height.round() as u32
}

fn draw_column(
    canvas: &mut Canvas,
    typography: &mut Typography,
    layout: &Layout,
    column: Rect,
    panels: &[Panel],
    side: Side,
) {
    let metrics = &layout.metrics;
    canvas.fill(column, color::SURFACE);
    match side {
        Side::Left => canvas.hairline_v(column.right() - 1, column.top, column.height, color::LINE),
        Side::Right => canvas.hairline_v(column.left, column.top, column.height, color::LINE),
    }

    let mut top = column.top;
    for (index, panel) in panels.iter().enumerate() {
        let remaining = column.bottom() - top;
        if remaining <= metrics.padding as i32 {
            // No room left for even a heading. Stopping is the honest answer:
            // half a panel at the bottom edge reads as a drawing fault.
            break;
        }
        let wanted = natural_height(typography, metrics, column.width, panel);
        let height = (wanted as i32).min(remaining) as u32;
        draw_panel(
            canvas,
            typography,
            metrics,
            Rect::new(column.left, top, column.width, height),
            panel,
        );
        top += height as i32;
        if index + 1 < panels.len() && top < column.bottom() {
            canvas.hairline_h(
                column.left + metrics.padding as i32,
                top,
                column.width.saturating_sub(metrics.padding as u32 * 2),
                color::LINE,
            );
        }
    }
}

fn draw_panel(
    canvas: &mut Canvas,
    typography: &mut Typography,
    metrics: &Metrics,
    region: Rect,
    panel: &Panel,
) {
    let inner = Rect::new(
        region.left + metrics.padding as i32,
        region.top + metrics.padding as i32,
        region.width.saturating_sub(metrics.padding as u32 * 2),
        region.height.saturating_sub(metrics.padding as u32),
    );
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let mut baseline = inner.top as f32 + typography.ascent(Face::ProseBold, metrics.heading);
    draw_heading(
        canvas,
        typography,
        inner.left as f32,
        baseline,
        inner.width as f32,
        &panel.heading,
        metrics,
    );
    baseline += typography.line_height(Face::Prose, metrics.small) * 1.35;

    let row_height = typography.line_height(Face::Mono, metrics.fact);
    for row in &panel.rows {
        // A row whose descender would leave the panel is not drawn at all. Half
        // a row of text at the bottom edge reads as corruption; nothing reads as
        // "there is more", which is what is true.
        if baseline + typography.line_height(Face::Mono, metrics.fact) * 0.4 > inner.bottom() as f32
        {
            break;
        }
        match row {
            Row::Fact { text, tone } => {
                typography.draw_within(
                    canvas,
                    inner.left as f32,
                    baseline,
                    inner.width as f32,
                    text,
                    TextStyle::new(Face::Mono, metrics.fact, tone_colour(*tone)),
                );
                baseline += row_height;
            }
            Row::Pair { label, value } => {
                let value_style = TextStyle::new(Face::Mono, metrics.fact, color::FACT);
                let value_width = typography.measure(Face::Mono, metrics.fact, value);
                typography.draw_within(
                    canvas,
                    inner.left as f32,
                    baseline,
                    (inner.width as f32 - value_width - metrics.padding * 0.5).max(1.0),
                    label,
                    TextStyle::new(Face::Prose, metrics.small, color::MUTED),
                );
                typography.draw(
                    canvas,
                    inner.right() as f32 - value_width,
                    baseline,
                    value,
                    value_style,
                );
                baseline += row_height;
            }
            Row::Note(text) => {
                let style = TextStyle::new(Face::Prose, metrics.small, color::MUTED);
                for line in typography.wrap(Face::Prose, metrics.small, inner.width as f32, text) {
                    if baseline > inner.bottom() as f32 {
                        break;
                    }
                    typography.draw(canvas, inner.left as f32, baseline, &line, style);
                    baseline += typography.line_height(Face::Prose, metrics.small);
                }
            }
        }
    }
}

fn draw_conversation(
    canvas: &mut Canvas,
    typography: &mut Typography,
    layout: &Layout,
    turns: &[Turn],
) {
    let metrics = &layout.metrics;
    let region = layout.centre;
    let rule_width = (3.0 * metrics.scale).round().max(2.0) as u32;
    let text_left = region.left as f32 + metrics.padding * 2.0 + rule_width as f32;
    let text_width = (region.right() as f32 - text_left - metrics.padding * 1.5).max(1.0);

    // Composed from the bottom because the newest turn is the one that must be
    // on the screen. Laying it out from the top and letting the end fall off
    // means a machine that answers and shows you the question.
    let mut bottom = region.bottom() as f32 - metrics.padding;
    for turn in turns.iter().rev() {
        let face = voice_face(turn.voice);
        let size = if turn.voice == Voice::Machine {
            metrics.fact
        } else {
            metrics.body
        };
        let line_height = typography.line_height(face, size);
        let lines = typography.wrap(face, size, text_width, &turn.text);
        let block_height = line_height * lines.len() as f32;
        let top = bottom - block_height;
        if top < region.top as f32 + metrics.padding {
            // This turn does not fit. Everything above it is older, so there is
            // nothing left to draw.
            break;
        }

        canvas.fill(
            Rect::new(
                region.left + metrics.padding as i32,
                top.round() as i32,
                rule_width,
                block_height.round() as u32,
            ),
            voice_colour(turn.voice),
        );

        let style = TextStyle::new(face, size, voice_colour(turn.voice));
        let mut baseline = top + typography.ascent(face, size);
        for line in &lines {
            typography.draw(canvas, text_left, baseline, line, style);
            baseline += line_height;
        }
        bottom = top - metrics.padding * 0.9;
    }
}

fn draw_prompt(canvas: &mut Canvas, typography: &mut Typography, layout: &Layout, prompt: &Prompt) {
    let metrics = &layout.metrics;
    let region = layout.prompt;
    canvas.hairline_h(region.left, region.top, region.width, color::LINE);

    let size = metrics.body;
    let baseline = region.top as f32 + region.height as f32 * 0.6;
    let mut pen = region.left as f32 + metrics.padding;

    pen = typography.draw(
        canvas,
        pen,
        baseline,
        "▸",
        TextStyle::new(Face::Mono, size, color::AGENT),
    ) + metrics.padding * 0.5;

    let style = TextStyle::new(Face::Mono, size, color::HUMAN);
    let before: String = prompt.line.chars().take(prompt.caret).collect();
    let after: String = prompt.line.chars().skip(prompt.caret).collect();
    let caret_x = pen + typography.measure(Face::Mono, size, &before);

    typography.draw(canvas, pen, baseline, &before, style);
    let end = typography.draw(canvas, caret_x, baseline, &after, style);

    if let Some(suggestion) = &prompt.suggestion {
        typography.draw(
            canvas,
            end,
            baseline,
            suggestion,
            TextStyle::new(Face::Mono, size, color::LINE),
        );
    }

    // A block on the character the caret is on, rather than a bar between two
    // of them: on a monospaced line the block is unambiguous about which
    // character the next keystroke replaces.
    let caret_width = typography.advance(Face::Mono, 'M', size);
    let ascent = typography.ascent(Face::Mono, size);
    canvas.fill(
        Rect::new(
            caret_x.round() as i32,
            (baseline - ascent * 0.86).round() as i32,
            caret_width.round().max(2.0) as u32,
            (ascent * 1.06).round() as u32,
        ),
        color::AGENT,
    );
    // The character under the caret, redrawn dark so it stays legible through
    // the block.
    if let Some(under) = after.chars().next() {
        typography.draw(
            canvas,
            caret_x,
            baseline,
            &under.to_string(),
            TextStyle::new(Face::Mono, size, color::INK),
        );
    }
}

/// One line of the confirmation, already resolved to how it is drawn.
///
/// Built as a list first so the whole block can be measured and then centred.
/// Drawing straight down from a fixed fraction of the height puts a short
/// confirmation in the top corner of an otherwise empty display, and the one
/// screen in Thalyx that must be impossible to miss is not the one to leave
/// looking incidental.
struct Line {
    text: String,
    style: TextStyle,
    /// Blank space above this line, over and above its own height.
    space_above: f32,
}

fn draw_confirmation(
    canvas: &mut Canvas,
    typography: &mut Typography,
    layout: &Layout,
    confirmation: &Confirmation,
) {
    let metrics = &layout.metrics;
    canvas.fill(layout.screen, color::TRUST_GROUND);

    let band = (6.0 * metrics.scale).round().max(4.0) as u32;
    canvas.fill(Rect::new(0, 0, layout.screen.width, band), color::TRUST);
    canvas.fill(
        Rect::new(
            0,
            layout.screen.bottom() - band as i32,
            layout.screen.width,
            band,
        ),
        color::TRUST,
    );

    let margin = (layout.screen.width as f32 * 0.12).max(metrics.padding * 2.0);
    let left = margin;
    let width = layout.screen.width as f32 - margin * 2.0;
    let headline_size = (metrics.body * 1.7).round();
    let asked_size = (metrics.fact * 1.45).round();
    let step = typography.line_height(Face::Prose, metrics.body);

    let mut lines: Vec<Line> = Vec::new();
    for line in typography.wrap(Face::ProseBold, headline_size, width, &confirmation.what) {
        lines.push(Line {
            text: line,
            style: TextStyle::new(Face::ProseBold, headline_size, color::HUMAN),
            space_above: 0.0,
        });
    }

    // What was read from the thing itself rather than from a list somebody was
    // keeping. This is the part that stops the correct command typed at the
    // wrong machine, so it is set in the machine's face.
    let mut first_found = true;
    for row in &confirmation.found {
        let (text, colour, face) = match row {
            Row::Fact { text, tone } => (text.clone(), tone_colour(*tone), Face::Mono),
            Row::Pair { label, value } => (format!("{label}  {value}"), color::FACT, Face::Mono),
            Row::Note(text) => (text.clone(), color::MUTED, Face::Prose),
        };
        lines.push(Line {
            text,
            style: TextStyle::new(face, metrics.fact, colour),
            space_above: if std::mem::take(&mut first_found) {
                step * 1.1
            } else {
                0.0
            },
        });
    }

    lines.push(Line {
        text: "Para autorizarlo, teclea exactamente:".into(),
        style: TextStyle::new(Face::Prose, metrics.body, color::MUTED),
        space_above: step * 1.4,
    });
    lines.push(Line {
        text: confirmation.type_this.clone(),
        style: TextStyle::new(Face::MonoBold, asked_size, color::TRUST),
        space_above: step * 0.4,
    });
    lines.push(Line {
        text: confirmation.typed.clone(),
        style: TextStyle::new(Face::Mono, asked_size, color::HUMAN),
        space_above: step * 0.9,
    });
    lines.push(Line {
        text: "Escape cancela. Nada se ha hecho todavía.".into(),
        style: TextStyle::new(Face::Prose, metrics.small, color::MUTED),
        space_above: step * 1.6,
    });

    let heading_height = typography.line_height(Face::ProseBold, metrics.heading) * 2.0;
    let block: f32 = heading_height
        + lines
            .iter()
            .map(|line| line.space_above + typography.line_height(line.style.face, line.style.size))
            .sum::<f32>();

    let mut baseline = ((layout.screen.height as f32 - block) / 2.0).max(band as f32 * 4.0)
        + typography.ascent(Face::ProseBold, metrics.heading);
    draw_heading(
        canvas,
        typography,
        left,
        baseline,
        width,
        "esto cambia la máquina",
        metrics,
    );
    baseline += heading_height;

    // The caret goes after the line the person is typing into, which is the one
    // carrying their own words back at them.
    let typed_index = lines.len() - 2;
    for (index, line) in lines.iter().enumerate() {
        baseline += line.space_above;
        let end = typography.draw(canvas, left, baseline, &line.text, line.style);
        if index == typed_index {
            let caret_width = typography.advance(Face::Mono, 'M', line.style.size);
            let ascent = typography.ascent(Face::Mono, line.style.size);
            canvas.fill(
                Rect::new(
                    end.round() as i32,
                    (baseline - ascent * 0.86).round() as i32,
                    caret_width.round().max(2.0) as u32,
                    (ascent * 1.06).round() as u32,
                ),
                color::TRUST,
            );
        }
        baseline += typography.line_height(line.style.face, line.style.size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Row;

    fn a_bar() -> Bar {
        Bar {
            machine: "thalyx".into(),
            store: "/dev/sdb2".into(),
            guard: Guard::Enforcing,
            clock: "14:32".into(),
        }
    }

    #[test]
    fn the_agent_never_gets_the_machines_face_or_the_machines_colour() {
        // The rule the whole typographic scheme exists for, asserted rather than
        // reviewed: a proposal drawn like a fact is `Marcado-de-Origen` broken
        // in the one place a person would never think to check.
        assert!(!voice_face(Voice::Agent).is_the_machines());
        assert!(voice_face(Voice::Machine).is_the_machines());
        assert_ne!(voice_colour(Voice::Agent), voice_colour(Voice::Machine));
        assert_ne!(voice_colour(Voice::Agent), voice_colour(Voice::Person));
        assert_ne!(voice_colour(Voice::Agent), color::FACT);
        assert_ne!(voice_colour(Voice::Agent), color::OK);
    }

    #[test]
    fn a_confirmation_leaves_nothing_of_the_rest_of_the_screen_on_it() {
        // The trusted path's property, measured at the pixels rather than
        // trusted to the order of two statements: if a panel were still drawn
        // under or beside the confirmation, its surface colour would be
        // somewhere on the display, and something beside a confirmation is
        // something that can imitate one.
        let mut screen = Screen::new(a_bar());
        screen.left = vec![Panel::new("archivos", vec![Row::fact("notas.md")])];
        screen.right = vec![Panel::new("red", vec![Row::fact("enp2s0")])];
        screen.conversation = vec![Turn::machine("ok  store  /dev/sdb2")];
        screen.confirmation = Some(Confirmation {
            what: "Instalar en /dev/sdb borra el disco.".into(),
            found: vec![Row::fact("/dev/sdb  7 GiB")],
            type_this: "/dev/sdb".into(),
            typed: String::new(),
        });

        let mut typography = Typography::embedded();
        let canvas = compose(&screen, &mut typography, 1280, 800);
        for y in 0..canvas.height() {
            for x in 0..canvas.width() {
                let pixel = canvas.pixel(x, y).unwrap();
                assert_ne!(pixel, color::SURFACE, "a panel was drawn at {x},{y}");
                assert_ne!(
                    pixel,
                    color::INK,
                    "the ordinary ground was drawn at {x},{y}"
                );
            }
        }
    }

    #[test]
    fn without_a_confirmation_the_trusted_paths_colour_is_nowhere_on_the_screen() {
        // The control for the test above. If `TRUST` showed up during ordinary
        // use, its presence would stop meaning anything, and the confirmation
        // would be just another red thing.
        let mut screen = Screen::new(a_bar());
        screen.left = vec![Panel::new("archivos", vec![Row::fact("notas.md")])];
        screen.right = vec![Panel::new("red", vec![Row::toned("caído", Tone::Refused)])];
        screen.conversation = vec![
            Turn::person("borra todo"),
            Turn::agent("No voy a hacer eso sin confirmación."),
            Turn::machine("refused  fs/write  /"),
        ];
        screen.prompt = Prompt {
            line: "instalar-en /dev/sdb".into(),
            caret: 20,
            suggestion: None,
        };

        let mut typography = Typography::embedded();
        let canvas = compose(&screen, &mut typography, 1280, 800);
        for y in 0..canvas.height() {
            for x in 0..canvas.width() {
                assert_ne!(
                    canvas.pixel(x, y).unwrap(),
                    color::TRUST,
                    "the trusted path's colour appeared at {x},{y} with no confirmation on screen"
                );
            }
        }
    }

    #[test]
    fn the_newest_turn_is_the_one_that_is_on_the_screen() {
        // A conversation laid out from the top answers your question and shows
        // you the question. With more turns than fit, the last one must be
        // drawn — so this composes far too many and looks for ink in the band
        // just above the prompt.
        let mut screen = Screen::new(a_bar());
        for index in 0..200 {
            screen
                .conversation
                .push(Turn::machine(format!("línea {index}")));
        }
        let mut typography = Typography::embedded();
        let canvas = compose(&screen, &mut typography, 1280, 800);
        let layout = Layout::for_size(1280, 800);
        let band_top = layout.centre.bottom() - 40;
        let mut ink = false;
        for y in band_top..layout.centre.bottom() {
            for x in layout.centre.left..layout.centre.right() {
                if canvas.pixel(x as u32, y as u32) != Some(color::INK) {
                    ink = true;
                }
            }
        }
        assert!(ink, "nothing was drawn at the bottom of the conversation");
    }

    #[test]
    fn a_screen_with_nothing_on_it_still_composes() {
        // PID 1 draws this before anything has happened. A frame that panicked
        // on an empty conversation would take the machine down at the one
        // moment nobody could read why.
        let mut typography = Typography::embedded();
        let canvas = compose(&Screen::new(a_bar()), &mut typography, 800, 600);
        assert_eq!(canvas.width(), 800);
    }

    #[test]
    fn a_frame_fits_the_display_it_was_composed_for() {
        // The end of the path: a canvas that cannot be written into the buffer
        // the kernel reported is a screen that never appears, and the failure
        // would show up as a black display rather than as a size mismatch.
        let mut typography = Typography::embedded();
        let canvas = compose(&crate::sample::working(), &mut typography, 1024, 768);
        let stride = 1024 * 4;
        let mut buffer = vec![0u8; stride * 768];
        canvas
            .write_into(&mut buffer, stride, crate::canvas::PixelFormat::XRGB8888)
            .expect("the frame did not fit the display it was made for");
        assert!(buffer.iter().any(|byte| *byte != 0));
    }
}
