use alloc::vec::Vec;

use crate::ui::clipboard::ClipboardError;

pub(super) fn encode(text: &[u8]) -> Result<Vec<u8>, ClipboardError> {
    let text = core::str::from_utf8(text).map_err(|_| ClipboardError::InvalidData)?;
    if text.as_bytes().contains(&0) {
        return Err(ClipboardError::InvalidData);
    }
    let bytes = text
        .encode_utf16()
        .count()
        .checked_add(1)
        .and_then(|units| units.checked_mul(2))
        .ok_or(ClipboardError::Capacity)?;
    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(bytes)
        .map_err(|_| ClipboardError::Capacity)?;
    for unit in text.encode_utf16().chain(core::iter::once(0)) {
        encoded.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(encoded)
}

pub(super) fn decode(bytes: &[u8]) -> Result<Vec<u8>, ClipboardError> {
    let units = || {
        bytes
            .chunks_exact(2)
            .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
    };
    let end = units()
        .position(|unit| unit == 0)
        .ok_or(ClipboardError::InvalidData)?;
    let decoded = || char::decode_utf16(units().take(end));
    let length = decoded().try_fold(0usize, |bytes, character| {
        bytes
            .checked_add(
                character
                    .map_err(|_| ClipboardError::InvalidData)?
                    .len_utf8(),
            )
            .ok_or(ClipboardError::Capacity)
    })?;
    let mut text = Vec::new();
    text.try_reserve_exact(length)
        .map_err(|_| ClipboardError::Capacity)?;
    for character in decoded() {
        let mut buffer = [0; 4];
        text.extend_from_slice(
            character
                .map_err(|_| ClipboardError::InvalidData)?
                .encode_utf8(&mut buffer)
                .as_bytes(),
        );
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips_without_lossy_surrogates_or_newline_rewriting() {
        for text in ["", "hello", "你好 🦀", "a\nb\r\nc\r", "\u{feff}abc"] {
            let native = encode(text.as_bytes()).unwrap();
            assert_eq!(&native[native.len() - 2..], &[0, 0]);
            assert_eq!(decode(&native).unwrap(), text.as_bytes());
        }
        assert!(encode(b"a\0b").is_err());
        assert!(encode(&[0xff]).is_err());
    }

    #[test]
    fn only_aligned_terminators_bound_native_text() {
        assert_eq!(decode(&[0, 1, 0, 0]).unwrap(), "Ā".as_bytes());
        let mut native = encode("🦀".as_bytes()).unwrap();
        native.extend_from_slice(&[0xff, 0xd8, 0xff]);
        assert_eq!(decode(&native).unwrap(), "🦀".as_bytes());
        assert!(decode(&[0x61, 0, 0]).is_err());
        assert!(decode(&[0, 0xd8, 0, 0]).is_err());
        assert!(decode(&[0, 0xdc, 0, 0]).is_err());
        assert!(decode(&[0, 0xd8, 0x61, 0, 0, 0]).is_err());
        assert!(decode(&[]).is_err());
    }
}
