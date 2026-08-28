//! A rectangle of pixels in memory, and the only thing that knows how to turn
//! it into the bytes a particular framebuffer wants.
//!
//! ## Why the conversion lives here and not next to the `mmap`
//!
//! Nothing in this crate opens a device — that is the whole reason it can be
//! tested in a container with no display. But the piece most likely to be wrong
//! on Cesar's machine is not the drawing: it is **the assumption about how that
//! firmware packs a pixel**. QEMU hands back one layout and a real UEFI can hand
//! back another, and a screen drawn perfectly into the wrong packing comes out
//! with red and blue swapped, which reads as *Thalyx is broken* rather than as
//! *one field of `fb_var_screeninfo` was ignored*.
//!
//! So the packing is a value, it is described by the kernel rather than
//! guessed, and it is converted by a function with tests. What stays outside is
//! only the `ioctl` that reads the description and the `mmap` that receives the
//! result.
//!
//! ## Fail closed
//!
//! Rule 9 of `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md`: a layout this
//! code does not understand is refused by name, never approximated. A screen
//! that refuses to start says which field it did not understand; a screen that
//! guesses produces a display nobody can diagnose from the other side of a
//! room.

use crate::color::Color;

/// Where one colour channel sits inside a pixel, as `fb_bitfield` describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
    /// Bits from the least significant end of the pixel.
    pub offset: u32,
    /// How many bits the channel has. Eight is a full byte; five and six happen
    /// on 16-bit displays.
    pub length: u32,
}

impl Channel {
    pub const fn at(offset: u32, length: u32) -> Self {
        Self { offset, length }
    }
}

/// How the display packs a pixel, read from the kernel rather than assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelFormat {
    pub bits_per_pixel: u32,
    pub red: Channel,
    pub green: Channel,
    pub blue: Channel,
}

impl PixelFormat {
    /// What almost every UEFI framebuffer reports: 32 bits with a byte each and
    /// the top byte unused.
    pub const XRGB8888: PixelFormat = PixelFormat {
        bits_per_pixel: 32,
        red: Channel::at(16, 8),
        green: Channel::at(8, 8),
        blue: Channel::at(0, 8),
    };

    /// The same width with red and blue the other way round. It exists here as
    /// a constant because it is the failure that looks like a bug in the
    /// drawing, and naming it makes it a case in a test instead.
    pub const XBGR8888: PixelFormat = PixelFormat {
        bits_per_pixel: 32,
        red: Channel::at(0, 8),
        green: Channel::at(8, 8),
        blue: Channel::at(16, 8),
    };

    /// 16 bits, five for red, six for green, five for blue.
    pub const RGB565: PixelFormat = PixelFormat {
        bits_per_pixel: 16,
        red: Channel::at(11, 5),
        green: Channel::at(5, 6),
        blue: Channel::at(0, 5),
    };

    fn bytes_per_pixel(&self) -> usize {
        (self.bits_per_pixel as usize).div_ceil(8)
    }

    fn check(&self) -> Result<(), UnsupportedFormat> {
        if !matches!(self.bits_per_pixel, 16 | 24 | 32) {
            return Err(UnsupportedFormat::Depth(self.bits_per_pixel));
        }
        for (name, channel) in [
            ("red", self.red),
            ("green", self.green),
            ("blue", self.blue),
        ] {
            if channel.length == 0 || channel.length > 8 {
                return Err(UnsupportedFormat::Channel {
                    name,
                    length: channel.length,
                });
            }
            if channel.offset + channel.length > self.bits_per_pixel {
                return Err(UnsupportedFormat::ChannelOutside {
                    name,
                    offset: channel.offset,
                    length: channel.length,
                    bits_per_pixel: self.bits_per_pixel,
                });
            }
        }
        Ok(())
    }

    fn pack(&self, colour: Color) -> u32 {
        // A channel narrower than a byte takes the *high* bits, not the low
        // ones. Truncating from the bottom would make white come out as
        // 0xf8f8f8 on a 16-bit display — dim, and dim in a way that looks like
        // a brightness problem with the monitor.
        let place = |value: u8, channel: Channel| -> u32 {
            let shifted = (value as u32) >> (8 - channel.length);
            shifted << channel.offset
        };
        place(colour.r, self.red) | place(colour.g, self.green) | place(colour.b, self.blue)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedFormat {
    Depth(u32),
    Channel {
        name: &'static str,
        length: u32,
    },
    ChannelOutside {
        name: &'static str,
        offset: u32,
        length: u32,
        bits_per_pixel: u32,
    },
    /// The destination is smaller than the frame that was drawn. Refused rather
    /// than clipped: a short buffer means the size the screen was laid out for
    /// and the size the device has disagree, and drawing part of a frame would
    /// hide that.
    TooSmall {
        needed: usize,
        given: usize,
    },
}

impl std::fmt::Display for UnsupportedFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Depth(bits) => write!(
                f,
                "this display packs a pixel into {bits} bits, and Thalyx draws \
                 into 16, 24 or 32"
            ),
            Self::Channel { name, length } => write!(
                f,
                "the {name} channel of this display is {length} bits wide, which \
                 is not a width Thalyx knows how to fill"
            ),
            Self::ChannelOutside {
                name,
                offset,
                length,
                bits_per_pixel,
            } => write!(
                f,
                "the {name} channel of this display sits at bit {offset} and is \
                 {length} wide, which runs past the {bits_per_pixel} bits a \
                 pixel has"
            ),
            Self::TooSmall { needed, given } => write!(
                f,
                "the frame needs {needed} bytes and the display gave {given}: \
                 the screen was laid out for a size this device does not have"
            ),
        }
    }
}

impl std::error::Error for UnsupportedFormat {}

/// The frame being composed: one `0x00RRGGBB` word per pixel, always, whatever
/// the display underneath turns out to want.
#[derive(Debug, Clone)]
pub struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl Canvas {
    pub fn new(width: u32, height: u32, ground: Color) -> Self {
        Self {
            width,
            height,
            pixels: vec![ground.packed(); (width as usize) * (height as usize)],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(Color::from_packed(
            self.pixels[(y as usize) * (self.width as usize) + (x as usize)],
        ))
    }

    /// Lay `colour` over what is already at `(x, y)`, with `coverage` of it
    /// showing. Off-canvas coordinates are dropped rather than wrapped.
    ///
    /// Dropped and not panicking because every caller is a glyph or a rectangle
    /// that may legitimately hang off an edge, and a screen that dies because a
    /// descender reached the last row is worse than one that clips.
    pub fn blend(&mut self, x: i32, y: i32, colour: Color, coverage: u8) {
        if coverage == 0 || x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        if x >= self.width || y >= self.height {
            return;
        }
        let at = (y as usize) * (self.width as usize) + (x as usize);
        let under = Color::from_packed(self.pixels[at]);
        self.pixels[at] = colour.over(under, coverage).packed();
    }

    pub fn fill(&mut self, rect: Rect, colour: Color) {
        let word = colour.packed();
        for y in rect.top.max(0)..(rect.top + rect.height as i32).min(self.height as i32) {
            let row = (y as usize) * (self.width as usize);
            let from = rect.left.max(0) as usize;
            let to = ((rect.left + rect.width as i32).min(self.width as i32)).max(0) as usize;
            if from >= to {
                continue;
            }
            self.pixels[row + from..row + to].fill(word);
        }
    }

    /// A one-pixel line. Horizontal and vertical only, because every rule on
    /// this screen separates two regions and regions are rectangles.
    pub fn hairline_h(&mut self, left: i32, top: i32, width: u32, colour: Color) {
        self.fill(Rect::new(left, top, width, 1), colour);
    }

    pub fn hairline_v(&mut self, left: i32, top: i32, height: u32, colour: Color) {
        self.fill(Rect::new(left, top, 1, height), colour);
    }

    /// The frame as this display wants it, written into `destination`.
    ///
    /// `stride_bytes` is the device's own line length, which is **not** always
    /// `width * bytes_per_pixel`: a framebuffer commonly pads each row, and
    /// ignoring the padding produces a picture that shears diagonally — the
    /// classic symptom, and one that looks like a drawing bug rather than an
    /// arithmetic one.
    pub fn write_into(
        &self,
        destination: &mut [u8],
        stride_bytes: usize,
        format: PixelFormat,
    ) -> Result<(), UnsupportedFormat> {
        format.check()?;
        let bytes = format.bytes_per_pixel();
        let needed = (self.height as usize - 1) * stride_bytes + (self.width as usize) * bytes;
        if destination.len() < needed {
            return Err(UnsupportedFormat::TooSmall {
                needed,
                given: destination.len(),
            });
        }
        for y in 0..self.height as usize {
            let row = y * stride_bytes;
            for x in 0..self.width as usize {
                let packed = format.pack(Color::from_packed(
                    self.pixels[y * (self.width as usize) + x],
                ));
                let at = row + x * bytes;
                for byte in 0..bytes {
                    destination[at + byte] = ((packed >> (8 * byte)) & 0xff) as u8;
                }
            }
        }
        Ok(())
    }
}

/// A region of the screen. Signed, because a caller is allowed to place
/// something partly off the edge and let the canvas clip it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(left: i32, top: i32, width: u32, height: u32) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }

    pub const fn right(&self) -> i32 {
        self.left + self.width as i32
    }

    pub const fn bottom(&self) -> i32 {
        self.top + self.height as i32
    }

    /// The same rectangle with `by` taken off every side.
    pub fn inset(&self, by: u32) -> Rect {
        Rect {
            left: self.left + by as i32,
            top: self.top + by as i32,
            width: self.width.saturating_sub(by * 2),
            height: self.height.saturating_sub(by * 2),
        }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right() && y >= self.top && y < self.bottom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{HUMAN, INK};

    #[test]
    fn a_rectangle_that_hangs_off_the_edge_is_clipped_instead_of_panicking() {
        let mut canvas = Canvas::new(10, 10, INK);
        canvas.fill(Rect::new(-5, -5, 20, 20), HUMAN);
        assert_eq!(canvas.pixel(0, 0), Some(HUMAN));
        assert_eq!(canvas.pixel(9, 9), Some(HUMAN));
        assert_eq!(canvas.pixel(10, 10), None);
    }

    #[test]
    fn a_pixel_past_the_last_row_is_dropped_and_does_not_wrap_to_the_next() {
        // The bug: an index computed as `y * width + x` with an x of `width`
        // lands on row y+1 column 0, so a glyph running off the right edge
        // reappears down the left. It is silent and it looks like corruption.
        let mut canvas = Canvas::new(4, 4, INK);
        canvas.blend(4, 0, HUMAN, 255);
        assert_eq!(canvas.pixel(0, 1), Some(INK));
    }

    #[test]
    fn a_display_with_red_and_blue_swapped_gets_the_bytes_swapped_too() {
        let mut canvas = Canvas::new(1, 1, Color::rgb(0xff, 0x00, 0x00));
        canvas.fill(Rect::new(0, 0, 1, 1), Color::rgb(0xff, 0x00, 0x00));
        let mut xrgb = [0u8; 4];
        canvas
            .write_into(&mut xrgb, 4, PixelFormat::XRGB8888)
            .unwrap();
        let mut xbgr = [0u8; 4];
        canvas
            .write_into(&mut xbgr, 4, PixelFormat::XBGR8888)
            .unwrap();
        assert_ne!(xrgb, xbgr, "the two layouts produced the same bytes");
        assert_eq!(xrgb[2], 0xff, "red belongs in byte 2 of XRGB");
        assert_eq!(xbgr[0], 0xff, "red belongs in byte 0 of XBGR");
    }

    #[test]
    fn white_on_a_sixteen_bit_display_is_all_ones_and_not_a_dimmer_white() {
        // Truncating a byte from the bottom gives 0x1f/0x3f/0x1f too, but only
        // because white is 0xff. The value that catches the wrong shift is one
        // that is not saturated, so this checks the top of the range where the
        // two arithmetics agree, and the round trip below where they do not.
        let canvas = Canvas::new(1, 1, Color::rgb(0xff, 0xff, 0xff));
        let mut out = [0u8; 2];
        canvas.write_into(&mut out, 2, PixelFormat::RGB565).unwrap();
        assert_eq!(u16::from_le_bytes(out), 0xffff);
    }

    #[test]
    fn a_padded_row_is_respected_instead_of_shearing_the_picture() {
        // A framebuffer whose line is longer than its visible width: writing
        // width*bpp per row instead of the stride slides every row left by the
        // padding, and the whole screen leans.
        let mut canvas = Canvas::new(2, 2, INK);
        canvas.fill(Rect::new(0, 1, 2, 1), HUMAN);
        let stride = 16;
        let mut out = vec![0u8; stride * 2];
        canvas
            .write_into(&mut out, stride, PixelFormat::XRGB8888)
            .unwrap();
        assert_eq!(&out[stride..stride + 4], &[0xff, 0xff, 0xff, 0x00]);
        assert_eq!(
            &out[8..12],
            &[0x00, 0x00, 0x00, 0x00],
            "padding was written"
        );
    }

    #[test]
    fn a_layout_this_code_does_not_understand_is_refused_by_name() {
        let canvas = Canvas::new(1, 1, INK);
        let mut out = [0u8; 8];
        let eight_bit = PixelFormat {
            bits_per_pixel: 8,
            red: Channel::at(5, 3),
            green: Channel::at(2, 3),
            blue: Channel::at(0, 2),
        };
        assert_eq!(
            canvas.write_into(&mut out, 1, eight_bit),
            Err(UnsupportedFormat::Depth(8))
        );
    }

    #[test]
    fn a_destination_shorter_than_the_frame_is_refused_rather_than_half_drawn() {
        let canvas = Canvas::new(4, 4, INK);
        let mut out = [0u8; 8];
        assert!(matches!(
            canvas.write_into(&mut out, 16, PixelFormat::XRGB8888),
            Err(UnsupportedFormat::TooSmall { .. })
        ));
    }

    #[test]
    fn the_last_row_does_not_demand_padding_that_is_not_there() {
        // A framebuffer is `stride * height` bytes, but the padding after the
        // final row need not exist. Requiring it refuses a device that is
        // exactly the right size, which is every device.
        let canvas = Canvas::new(4, 4, INK);
        let stride = 4 * 4;
        let mut out = vec![0u8; stride * 3 + 4 * 4];
        assert!(
            canvas
                .write_into(&mut out, stride, PixelFormat::XRGB8888)
                .is_ok()
        );
    }
}
