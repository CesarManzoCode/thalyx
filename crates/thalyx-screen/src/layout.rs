//! Where the regions of the one screen are, for whatever size the firmware
//! happened to give.
//!
//! ## Why this is computed and not a set of constants
//!
//! Thalyx does not choose the resolution. `CONFIG_FB_EFI` adopts the
//! framebuffer UEFI already configured, so the screen is whatever that
//! machine's firmware decided — 1920×1080 on Cesar's monitor, 1024×768 on a
//! firmware that fell back, and something else on the next machine. A layout
//! written as pixel constants is a layout that is correct on exactly one
//! computer.
//!
//! ## The rule that survives every size
//!
//! `vault/02-Arquitectura/La-Pantalla.md`: **the centre is the conversation and
//! there is no way to close it.** So when the screen is too narrow to hold
//! everything, the side panels are what goes — never the conversation, and
//! never the prompt. That is asserted by a test at sizes from a very small
//! framebuffer up, because a layout bug at an unusual resolution shows up as a
//! machine that boots to something unusable, on the one machine nobody has.

use crate::canvas::Rect;

/// The sizes everything is drawn at, derived once from the display's height.
///
/// Derived from height rather than width because type is measured in ems and a
/// wide, short display should not get bigger letters — it should get more room
/// beside the conversation, which is what the region arithmetic already does.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub scale: f32,
    /// Prose in the conversation.
    pub body: f32,
    /// A machine fact in the conversation, and rows inside a panel.
    pub fact: f32,
    /// A panel's own words.
    pub small: f32,
    /// A panel's heading, which is set in capitals and tracked out.
    pub heading: f32,
    /// The bar across the top.
    pub bar: f32,
    /// The space inside a panel, and between a panel's edge and its text.
    pub padding: f32,
    /// The space between two regions.
    pub gutter: f32,
}

impl Metrics {
    fn for_height(height: u32) -> Self {
        // Clamped at the bottom so a 480-row framebuffer does not produce type
        // nothing can read, and at the top so a 4K display does not get a
        // screen that fits four sentences.
        let scale = (height as f32 / 1080.0).clamp(0.78, 1.8);
        Self {
            scale,
            body: (17.0 * scale).round(),
            fact: (15.0 * scale).round(),
            small: (14.0 * scale).round(),
            heading: (12.0 * scale).round(),
            bar: (14.0 * scale).round(),
            padding: (18.0 * scale).round(),
            gutter: (1.0 * scale).max(1.0).round(),
        }
    }
}

/// Every region of the screen, already placed.
#[derive(Debug, Clone)]
pub struct Layout {
    pub metrics: Metrics,
    /// The whole display.
    pub screen: Rect,
    /// The bar across the top: which machine, which store, what the guard is
    /// doing, the time.
    pub bar: Rect,
    /// Context: where you are, what is here, what is installed. `None` when the
    /// display is too narrow to hold it.
    pub left: Option<Rect>,
    /// The conversation. Never `None`.
    pub centre: Rect,
    /// The line being typed. Never `None`.
    pub prompt: Rect,
    /// What is alive: what is running, memory, permissions, the network.
    pub right: Option<Rect>,
}

impl Layout {
    pub fn for_size(width: u32, height: u32) -> Self {
        let metrics = Metrics::for_height(height);
        let screen = Rect::new(0, 0, width, height);

        let bar_height = (44.0 * metrics.scale).round() as u32;
        let prompt_height = (64.0 * metrics.scale).round() as u32;
        let bar = Rect::new(0, 0, width, bar_height);

        let body_top = bar.bottom();
        let body_height = height.saturating_sub(bar_height);

        // A side column is worth having only if it can hold a path and a size
        // beside each other; below that it is a stripe that says nothing.
        let side_min = (260.0 * metrics.scale).round() as u32;
        let side_want = ((width as f32 * 0.185).round() as u32)
            .clamp(side_min, (360.0 * metrics.scale).round() as u32);
        // The conversation's floor. A centre narrower than this is a column of
        // three words, and at that point the panels are costing more than they
        // give.
        let centre_min = (560.0 * metrics.scale).round() as u32;

        let gutter = metrics.gutter as u32;
        let (left_width, right_width) = if width >= centre_min + (side_want + gutter) * 2 {
            (side_want, side_want)
        } else if width >= centre_min + side_want + gutter {
            // Only one fits. It is the right-hand one that stays: what is
            // running, what is granted and what the machine has left is the
            // half a person needs while typing. Where you are is one line the
            // prompt can carry.
            (0, side_want)
        } else {
            (0, 0)
        };

        let left = (left_width > 0).then(|| Rect::new(0, body_top, left_width, body_height));
        let right = (right_width > 0).then(|| {
            Rect::new(
                (width - right_width) as i32,
                body_top,
                right_width,
                body_height,
            )
        });

        let centre_left = left.map(|r| r.right() + gutter as i32).unwrap_or(0);
        let centre_right = right
            .map(|r| r.left - gutter as i32)
            .unwrap_or(width as i32);
        let centre_width = (centre_right - centre_left).max(1) as u32;

        let conversation_height = body_height.saturating_sub(prompt_height);
        let centre = Rect::new(centre_left, body_top, centre_width, conversation_height);
        let prompt = Rect::new(
            centre_left,
            body_top + conversation_height as i32,
            centre_width,
            prompt_height,
        );

        Self {
            metrics,
            screen,
            bar,
            left,
            centre,
            prompt,
            right,
        }
    }

    /// The rows a panel column is divided into, one per panel, sharing the
    /// height in proportion to how many rows each asked for.
    ///
    /// Proportional rather than equal because `RED` has one line and `ARCHIVOS`
    /// has twenty, and giving them the same height wastes half a column to make
    /// a grid look tidy.
    pub fn split(column: Rect, weights: &[u32], gutter: u32) -> Vec<Rect> {
        if weights.is_empty() {
            return Vec::new();
        }
        let total: u32 = weights.iter().sum::<u32>().max(1);
        let gutters = gutter * (weights.len() as u32 - 1);
        let usable = column.height.saturating_sub(gutters);
        let mut out = Vec::with_capacity(weights.len());
        let mut top = column.top;
        let mut spent = 0;
        for (index, weight) in weights.iter().enumerate() {
            let height = if index + 1 == weights.len() {
                // The last one absorbs the rounding, so the column always ends
                // exactly where it started plus its height. Dividing each share
                // independently leaves a stripe of background at the bottom
                // that moves as the numbers change.
                usable.saturating_sub(spent)
            } else {
                usable * weight / total
            };
            out.push(Rect::new(column.left, top, column.width, height));
            top += height as i32 + gutter as i32;
            spent += height;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: &[(u32, u32)] = &[
        (640, 480),
        (800, 600),
        (1024, 768),
        (1280, 800),
        (1366, 768),
        (1600, 900),
        (1920, 1080),
        (2560, 1440),
        (3840, 2160),
    ];

    #[test]
    fn the_conversation_and_the_prompt_exist_at_every_size() {
        // The decree's one invariant: the centre cannot be closed. A layout
        // that dropped it at some resolution would produce a machine that boots
        // to something with no way to say anything, on whichever computer has
        // that resolution.
        for &(width, height) in SIZES {
            let layout = Layout::for_size(width, height);
            assert!(
                layout.centre.width > 0,
                "no conversation at {width}x{height}"
            );
            assert!(
                layout.centre.height > 0,
                "no conversation at {width}x{height}"
            );
            assert!(layout.prompt.height > 0, "no prompt at {width}x{height}");
        }
    }

    #[test]
    fn a_narrow_display_loses_the_panels_and_not_the_conversation() {
        let narrow = Layout::for_size(640, 480);
        assert!(narrow.left.is_none());
        assert!(narrow.right.is_none());
        assert_eq!(narrow.centre.left, 0);
        assert_eq!(narrow.centre.width, 640);
    }

    #[test]
    fn a_display_that_fits_one_panel_keeps_the_right_hand_one() {
        // Which one survives is a decision, not an accident: the right column is
        // what is running and what is granted, and that is what a person needs
        // while they are typing.
        let layout = Layout::for_size(800, 600);
        assert!(
            layout.left.is_none(),
            "the left panel took room the centre needed"
        );
        assert!(layout.right.is_some(), "the wrong panel was dropped");
    }

    #[test]
    fn no_two_regions_overlap_at_any_size() {
        // Regions that overlap draw over each other, and the symptom is a panel
        // that flickers or a conversation with a stripe through it — both of
        // which look like a drawing bug rather than an arithmetic one.
        for &(width, height) in SIZES {
            let layout = Layout::for_size(width, height);
            let mut regions = vec![
                ("bar", layout.bar),
                ("centre", layout.centre),
                ("prompt", layout.prompt),
            ];
            if let Some(left) = layout.left {
                regions.push(("left", left));
            }
            if let Some(right) = layout.right {
                regions.push(("right", right));
            }
            for (i, (one_name, one)) in regions.iter().enumerate() {
                for (other_name, other) in regions.iter().skip(i + 1) {
                    let overlaps = one.left < other.right()
                        && other.left < one.right()
                        && one.top < other.bottom()
                        && other.top < one.bottom();
                    assert!(
                        !overlaps,
                        "{one_name} and {other_name} overlap at {width}x{height}"
                    );
                }
            }
        }
    }

    #[test]
    fn nothing_is_placed_outside_the_display() {
        for &(width, height) in SIZES {
            let layout = Layout::for_size(width, height);
            let mut regions = vec![layout.bar, layout.centre, layout.prompt];
            regions.extend(layout.left);
            regions.extend(layout.right);
            for region in regions {
                assert!(
                    region.left >= 0 && region.top >= 0,
                    "{region:?} at {width}x{height}"
                );
                assert!(
                    region.right() <= width as i32,
                    "{region:?} at {width}x{height}"
                );
                assert!(
                    region.bottom() <= height as i32,
                    "{region:?} at {width}x{height}"
                );
            }
        }
    }

    #[test]
    fn the_conversation_is_the_widest_region_wherever_the_panels_fit() {
        for &(width, height) in SIZES {
            let layout = Layout::for_size(width, height);
            if let Some(left) = layout.left {
                assert!(
                    layout.centre.width > left.width,
                    "a panel was wider than the conversation at {width}x{height}"
                );
            }
        }
    }

    #[test]
    fn a_split_column_ends_exactly_where_the_column_ends() {
        // Dividing each share independently leaves a few pixels of background
        // at the bottom that grow and shrink as the panels change — a shimmer
        // nobody can attribute to anything.
        let column = Rect::new(0, 100, 300, 901);
        let parts = Layout::split(column, &[3, 5, 2], 8);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2].bottom(), column.bottom());
    }

    #[test]
    fn splitting_a_column_into_nothing_gives_nothing_rather_than_dividing_by_zero() {
        assert!(Layout::split(Rect::new(0, 0, 10, 10), &[], 4).is_empty());
        let all_zero = Layout::split(Rect::new(0, 0, 10, 100), &[0, 0], 4);
        assert_eq!(all_zero.len(), 2);
    }
}
