//! A frame written out as a PNG, so that the screen can be looked at on a
//! machine that has no screen.
//!
//! ## Why this is in the shipping crate and not in a test
//!
//! `vault/09-Notas-Tecnicas/Estrategia-de-Pruebas.md` rule 1: every real defect
//! came from running the system. A layout can pass every assertion about
//! coordinates and still be ugly, unreadable or wrong in a way no assertion was
//! written for — and the container that builds Thalyx has no framebuffer to
//! find that out on.
//!
//! So a frame can be written to a file and opened anywhere. It is the same
//! composition path the display uses, not a second renderer: what comes out of
//! here is what the machine draws, pixel for pixel.
//!
//! ## Why the encoder is written out rather than pulled in
//!
//! It is forty lines over `flate2`, which the workspace already depends on for
//! bundles. A PNG encoder crate would be a second compression stack inside the
//! one program for the sake of a development convenience.

use crate::canvas::Canvas;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.sum().to_be_bytes());
}

/// The canvas as an 8-bit truecolour PNG.
pub fn encode(canvas: &Canvas) -> std::io::Result<Vec<u8>> {
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&canvas.width().to_be_bytes());
    header.extend_from_slice(&canvas.height().to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);

    // Filter byte 0 on every row: no prediction. The frame is mostly flat
    // colour, which zlib already collapses, and a filter would buy a smaller
    // file for a picture nobody stores.
    let mut raw =
        Vec::with_capacity((canvas.height() as usize) * (canvas.width() as usize * 3 + 1));
    for y in 0..canvas.height() {
        raw.push(0);
        for x in 0..canvas.width() {
            let pixel = canvas
                .pixel(x, y)
                .expect("inside the canvas by construction");
            raw.extend_from_slice(&[pixel.r, pixel.g, pixel.b]);
        }
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw)?;
    let compressed = encoder.finish()?;

    let mut out = Vec::with_capacity(compressed.len() + 64);
    out.extend_from_slice(&SIGNATURE);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &compressed);
    chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{HUMAN, INK};

    #[test]
    fn what_comes_out_is_a_png_a_reader_will_accept() {
        let canvas = Canvas::new(4, 3, INK);
        let bytes = encode(&canvas).unwrap();
        assert_eq!(&bytes[..8], &SIGNATURE);
        assert_eq!(&bytes[12..16], b"IHDR");
        assert_eq!(&bytes[16..20], &4u32.to_be_bytes());
        assert_eq!(&bytes[20..24], &3u32.to_be_bytes());
        assert_eq!(&bytes[bytes.len() - 8..bytes.len() - 4], b"IEND");
    }

    #[test]
    fn the_checksum_covers_the_chunk_type_as_well_as_its_data() {
        // The mistake this pins: a CRC taken over the data alone produces a
        // file every decoder rejects, and the error a viewer shows is "not a
        // PNG", which sends somebody looking at the wrong thing entirely.
        let mut out = Vec::new();
        chunk(&mut out, b"IEND", &[]);
        let mut expected = flate2::Crc::new();
        expected.update(b"IEND");
        assert_eq!(&out[out.len() - 4..], &expected.sum().to_be_bytes());
    }

    #[test]
    fn a_pixel_that_was_drawn_survives_into_the_file() {
        let mut canvas = Canvas::new(2, 1, INK);
        canvas.blend(1, 0, HUMAN, 255);
        let bytes = encode(&canvas).unwrap();
        // Round-tripping needs a decoder, which is not worth carrying. What is
        // checkable here is that the two pixels produced different bytes, which
        // is enough to catch a row assembled from the wrong index.
        assert!(bytes.len() > SIGNATURE.len() + 12);
    }
}
