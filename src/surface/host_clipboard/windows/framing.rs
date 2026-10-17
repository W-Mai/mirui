use crate::ui::clipboard::ClipboardError;

pub(super) fn png(bytes: &[u8]) -> Result<&[u8], ClipboardError> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(ClipboardError::InvalidData);
    }
    let mut offset = 8usize;
    let mut data = false;
    loop {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or(ClipboardError::InvalidData)?;
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let end = offset
            .checked_add(12)
            .and_then(|start| start.checked_add(length))
            .ok_or(ClipboardError::Capacity)?;
        if end > bytes.len() || (offset == 8 && (&header[4..] != b"IHDR" || length != 13)) {
            return Err(ClipboardError::InvalidData);
        }
        match &header[4..] {
            b"IEND" => {
                if length != 0 || !data {
                    return Err(ClipboardError::InvalidData);
                }
                return Ok(&bytes[..end]);
            }
            b"IDAT" => data = true,
            b"IHDR" if offset != 8 => return Err(ClipboardError::InvalidData),
            _ => (),
        }
        offset = end;
    }
}

pub(super) fn rtf(bytes: &[u8]) -> Result<&[u8], ClipboardError> {
    if !bytes.starts_with(b"{\\rtf") {
        return Err(ClipboardError::InvalidData);
    }
    let (mut offset, mut depth) = (0, 0usize);
    while let Some(byte) = bytes.get(offset) {
        offset += 1;
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(&bytes[..offset]);
                }
            }
            b'\\' => offset = control(bytes, offset)?,
            0 => return Err(ClipboardError::InvalidData),
            _ => (),
        }
    }
    Err(ClipboardError::InvalidData)
}

fn control(bytes: &[u8], start: usize) -> Result<usize, ClipboardError> {
    let first = *bytes.get(start).ok_or(ClipboardError::InvalidData)?;
    if first == 0 {
        return Err(ClipboardError::InvalidData);
    }
    if first == b'\'' {
        let digits = bytes
            .get(start + 1..start + 3)
            .ok_or(ClipboardError::InvalidData)?;
        return if digits.iter().all(u8::is_ascii_hexdigit) {
            Ok(start + 3)
        } else {
            Err(ClipboardError::InvalidData)
        };
    }
    if !first.is_ascii_alphabetic() {
        return Ok(start + 1);
    }
    let mut offset = start;
    while bytes.get(offset).is_some_and(u8::is_ascii_alphabetic) {
        offset += 1;
    }
    let binary = &bytes[start..offset] == b"bin";
    let negative = bytes.get(offset) == Some(&b'-');
    offset += usize::from(negative);
    let digits = offset;
    let mut count = 0usize;
    while let Some(digit) = bytes.get(offset).filter(|byte| byte.is_ascii_digit()) {
        if binary {
            count = count
                .checked_mul(10)
                .and_then(|value| value.checked_add((digit - b'0') as usize))
                .ok_or(ClipboardError::Capacity)?;
        }
        offset += 1;
    }
    if binary && (negative || offset == digits) {
        return Err(ClipboardError::InvalidData);
    }
    offset += usize::from(bytes.get(offset) == Some(&b' '));
    if binary {
        offset = offset.checked_add(count).ok_or(ClipboardError::Capacity)?;
    }
    if offset > bytes.len() {
        return Err(ClipboardError::InvalidData);
    }
    Ok(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtf_binary_groups_and_escaped_delimiters_do_not_end_the_record() {
        for content in [
            b"{\\rtf1 hello}".as_slice(),
            br"{\rtf1 {\b bold} \{literal\} \'7b}",
            b"{\\rtf1 \\bin4 }\0{}tail}",
            br"{\rtf1 \u-1234?}",
        ] {
            let mut padded = content.to_vec();
            padded.extend_from_slice(&[0, 0xff, b'}']);
            assert_eq!(rtf(&padded).unwrap(), content);
            for end in 0..content.len() {
                assert!(rtf(&content[..end]).is_err());
            }
        }
        for invalid in [
            br"{\rtf1\bin-1 x}".as_slice(),
            br"{\rtf1\bin x}",
            br"{\rtf1\bin9 a}",
            br"{\rtf1\'xx}",
            b"{\\rtf1 \0}",
        ] {
            assert!(rtf(invalid).is_err());
        }
    }

    #[test]
    fn png_chunks_delimit_encoded_bytes_without_parsing_compressed_pixels() {
        let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
        for (kind, payload) in [
            (b"IHDR", &[0u8; 13][..]),
            (b"IDAT", &[1, 2, 3][..]),
            (b"IEND", &[][..]),
        ] {
            data.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            data.extend_from_slice(kind);
            data.extend_from_slice(payload);
            data.extend_from_slice(&[0; 4]);
        }
        let length = data.len();
        for end in 0..length {
            assert!(png(&data[..end]).is_err());
        }
        data.extend_from_slice(&[0xff; 16]);
        assert_eq!(png(&data).unwrap().len(), length);
        data[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(png(&data).is_err());
    }
}
