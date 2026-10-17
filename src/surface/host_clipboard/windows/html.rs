use alloc::vec::Vec;
use core::ops::Range;

use crate::ui::clipboard::ClipboardError;

const HEADER: &[u8] = b"Version:1.0\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";
const PREFIX: &[u8] = b"<html><body><!--StartFragment-->";
const SUFFIX: &[u8] = b"<!--EndFragment--></body></html>";

pub(super) fn encode(fragment: &[u8]) -> Result<Vec<u8>, ClipboardError> {
    core::str::from_utf8(fragment).map_err(|_| ClipboardError::InvalidData)?;
    if fragment.contains(&0) {
        return Err(ClipboardError::InvalidData);
    }
    let start = HEADER.len() + PREFIX.len();
    let end = start
        .checked_add(fragment.len())
        .ok_or(ClipboardError::Capacity)?;
    let end_html = end
        .checked_add(SUFFIX.len())
        .ok_or(ClipboardError::Capacity)?;
    let capacity = end_html.checked_add(1).ok_or(ClipboardError::Capacity)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| ClipboardError::Capacity)?;
    bytes.extend_from_slice(HEADER);
    for (field, value) in [
        (b"StartHTML:".as_slice(), HEADER.len()),
        (b"EndHTML:".as_slice(), end_html),
        (b"StartFragment:".as_slice(), start),
        (b"EndFragment:".as_slice(), end),
    ] {
        let offset = HEADER
            .windows(field.len())
            .position(|part| part == field)
            .expect("static clipboard header field")
            + field.len();
        let mut value = value;
        for digit in bytes[offset..offset + 10].iter_mut().rev() {
            *digit = b'0' + (value % 10) as u8;
            value /= 10;
        }
        if value != 0 {
            return Err(ClipboardError::Capacity);
        }
    }
    bytes.extend_from_slice(PREFIX);
    bytes.extend_from_slice(fragment);
    bytes.extend_from_slice(SUFFIX);
    bytes.push(0);
    Ok(bytes)
}

#[derive(Default)]
struct Header {
    version: bool,
    html_start: Option<isize>,
    html_end: Option<isize>,
    fragment_start: Option<isize>,
    fragment_end: Option<isize>,
    selection_start: Option<isize>,
    selection_end: Option<isize>,
}

impl Header {
    fn field(&mut self, line: &[u8]) -> Result<bool, ClipboardError> {
        let Some(colon) = line.iter().position(|&b| b == b':') else {
            return Ok(false);
        };
        let name = &line[..colon];
        let value = &line[colon + 1..];
        if name == b"Version" {
            if self.version || !matches!(value, b"0.9" | b"1.0") {
                return Err(ClipboardError::InvalidData);
            }
            self.version = true;
            return Ok(true);
        }
        let slot = match name {
            b"StartHTML" => &mut self.html_start,
            b"EndHTML" => &mut self.html_end,
            b"StartFragment" => &mut self.fragment_start,
            b"EndFragment" => &mut self.fragment_end,
            b"StartSelection" => &mut self.selection_start,
            b"EndSelection" => &mut self.selection_end,
            b"SourceURL" => return Ok(true),
            _ => return Ok(false),
        };
        if value != b"-1" && (value.is_empty() || !value.iter().all(u8::is_ascii_digit)) {
            return Err(ClipboardError::InvalidData);
        }
        let value = core::str::from_utf8(value)
            .map_err(|_| ClipboardError::InvalidData)?
            .parse::<isize>()
            .map_err(|_| ClipboardError::InvalidData)?;
        if slot.replace(value).is_some() {
            return Err(ClipboardError::InvalidData);
        }
        Ok(true)
    }

    fn body_start(&self) -> Option<usize> {
        self.html_start
            .filter(|&value| value >= 0)
            .or(self.fragment_start)
            .and_then(|value| usize::try_from(value).ok())
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<&[u8], ClipboardError> {
    let mut header = Header::default();
    let mut offset = 0;
    while offset < bytes.len() && header.body_start().is_none_or(|start| offset < start) {
        let limit = header.body_start().unwrap_or(bytes.len()).min(bytes.len());
        let Some(length) = bytes[offset..limit]
            .iter()
            .position(|&b| matches!(b, b'\r' | b'\n'))
        else {
            break;
        };
        if !header.field(&bytes[offset..offset + length])? {
            break;
        }
        offset += length + 1;
        if bytes[offset - 1] == b'\r' && bytes.get(offset) == Some(&b'\n') {
            offset += 1;
        }
    }
    if !header.version {
        return Err(ClipboardError::InvalidData);
    }
    let fragment = range(header.fragment_start, header.fragment_end, bytes, offset)?;
    match (header.html_start, header.html_end) {
        (Some(-1), Some(-1)) => {}
        (start, end) => {
            let context = range(start, end, bytes, offset)?;
            if fragment.start < context.start || fragment.end > context.end {
                return Err(ClipboardError::InvalidData);
            }
        }
    }
    if header.selection_start.is_some() || header.selection_end.is_some() {
        let selection = range(header.selection_start, header.selection_end, bytes, offset)?;
        if selection.start < fragment.start || selection.end > fragment.end {
            return Err(ClipboardError::InvalidData);
        }
    }
    Ok(&bytes[fragment])
}

fn range(
    start: Option<isize>,
    end: Option<isize>,
    bytes: &[u8],
    header_end: usize,
) -> Result<Range<usize>, ClipboardError> {
    let start = start
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(ClipboardError::InvalidData)?;
    let end = end
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(ClipboardError::InvalidData)?;
    let text = bytes.get(start..end).ok_or(ClipboardError::InvalidData)?;
    let splits_character = [start, end]
        .into_iter()
        .any(|offset| bytes.get(offset).is_some_and(|byte| byte & 0xc0 == 0x80));
    if start < header_end
        || splits_character
        || text.contains(&0)
        || core::str::from_utf8(text).is_err()
    {
        return Err(ClipboardError::InvalidData);
    }
    Ok(start..end)
}

#[cfg(test)]
#[path = "html/tests.rs"]
mod tests;
