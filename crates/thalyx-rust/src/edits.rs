//! Turning what rust-analyzer described into the text a file should hold.
//!
//! The server answers a rename with ranges, and a range is `(line, character)`
//! — where `character` is a count of UTF-16 code units unless the server agreed
//! to bytes. So this is where the two coordinate systems meet, and it is the
//! only place either of them appears: everything above works in whole files.
//!
//! Applied back to front, largest position first. Front to back, the second
//! edit of a line is measured against a line the first one already moved —
//! which is a whole class of off-by-`len(new) - len(old)` that only shows up
//! when two names on one line change, and looks like the server got it wrong.

use crate::analyzer::Spot;

/// The text a file should hold after these edits, or why it cannot be produced.
pub fn applied(text: &str, edits: &[(Spot, String)], utf8_columns: bool) -> Result<String, String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    // Byte offset of the start of every line, plus the end of the file, so a
    // position on the last line has somewhere to land.
    let mut starts: Vec<usize> = Vec::with_capacity(lines.len() + 1);
    let mut offset = 0;
    for line in &lines {
        starts.push(offset);
        offset += line.len();
    }
    starts.push(offset);

    let mut spans: Vec<(usize, usize, &str)> = Vec::with_capacity(edits.len());
    for (spot, replacement) in edits {
        let from = byte_at(&lines, &starts, spot.line, spot.character, utf8_columns)?;
        let to = byte_at(
            &lines,
            &starts,
            spot.end_line,
            spot.end_character,
            utf8_columns,
        )?;
        if to < from {
            return Err(format!(
                "an edit ends at {to} and starts at {from}, which is not a range"
            ));
        }
        spans.push((from, to, replacement.as_str()));
    }
    spans.sort_by_key(|(from, _, _)| *from);
    // Two edits over the same bytes have no defined result, and picking one
    // silently would produce a file that compiles and is not what either edit
    // asked for. Rule 9: the cautious answer.
    for pair in spans.windows(2) {
        if pair[1].0 < pair[0].1 {
            return Err(format!(
                "two edits overlap over bytes {}..{}",
                pair[1].0, pair[0].1
            ));
        }
    }

    let mut out = text.to_string();
    for (from, to, replacement) in spans.into_iter().rev() {
        out.replace_range(from..to, replacement);
    }
    Ok(out)
}

fn byte_at(
    lines: &[&str],
    starts: &[usize],
    line: u32,
    character: u32,
    utf8_columns: bool,
) -> Result<usize, String> {
    let index = line as usize;
    let Some(start) = starts.get(index) else {
        return Err(format!("line {line} is past the end of the file"));
    };
    let Some(text) = lines.get(index) else {
        // The position of the very end of the file: a line that is only a
        // start. Legal, and the one case where there is no line text.
        return Ok(*start);
    };
    let column = if utf8_columns {
        character as usize
    } else {
        let mut units = 0usize;
        let mut bytes = text.len();
        for (offset, character_here) in text.char_indices() {
            if units >= character as usize {
                bytes = offset;
                break;
            }
            units += character_here.len_utf16();
        }
        if units < character as usize {
            text.len()
        } else {
            bytes
        }
    };
    if column > text.len() {
        return Err(format!("column {character} is past the end of line {line}"));
    }
    if !text.is_char_boundary(column) {
        return Err(format!(
            "column {character} on line {line} falls inside a character"
        ));
    }
    Ok(start + column)
}
