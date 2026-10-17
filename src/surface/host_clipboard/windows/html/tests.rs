use super::*;
use alloc::format;

fn replace_offset(bytes: &mut [u8], key: &[u8], value: usize) {
    let offset = bytes.windows(key.len()).position(|b| b == key).unwrap() + key.len();
    bytes[offset..offset + 10].copy_from_slice(format!("{value:010}").as_bytes());
}

#[test]
fn fragments_round_trip_without_native_header_or_allocation_padding() {
    for fragment in [
        "",
        "<p>hello</p>",
        "<p>你好 🦀</p>",
        "StartHTML:0\r\nEndFragment:0",
    ] {
        let mut native = encode(fragment.as_bytes()).unwrap();
        native.extend_from_slice(b"\0\0allocation padding");
        assert_eq!(decode(&native).unwrap(), fragment.as_bytes());
    }
    assert!(encode(b"a\0b").is_err());
    assert!(encode(&[0xff]).is_err());
}

#[test]
fn contextless_records_support_native_header_line_endings() {
    for newline in ["\r\n", "\n", "\r"] {
        let header = format!(
            "Version:0.9{newline}StartHTML:-1{newline}EndHTML:-1{newline}StartFragment:0000000000{newline}EndFragment:0000000000{newline}"
        );
        let mut bytes = header.into_bytes();
        let start = bytes.len();
        bytes.extend_from_slice("<b>汉字</b>".as_bytes());
        let end = bytes.len();
        replace_offset(&mut bytes, b"StartFragment:", start);
        replace_offset(&mut bytes, b"EndFragment:", end);
        assert_eq!(decode(&bytes).unwrap(), "<b>汉字</b>".as_bytes());
    }
}

#[test]
fn offsets_must_describe_nested_utf8_ranges() {
    let original = encode("你好".as_bytes()).unwrap();
    let fragment_start = HEADER.len() + PREFIX.len();
    for (field, value) in [
        (b"StartHTML:".as_slice(), 0),
        (b"EndHTML:".as_slice(), original.len() + 1),
        (b"StartFragment:".as_slice(), fragment_start + 1),
        (b"EndFragment:".as_slice(), fragment_start + 1),
        (b"EndFragment:".as_slice(), fragment_start - 1),
        (b"EndFragment:".as_slice(), original.len()),
    ] {
        let mut native = original.clone();
        replace_offset(&mut native, field, value);
        assert_eq!(decode(&native), Err(ClipboardError::InvalidData));
    }
    for len in 0..original.len() - 1 {
        assert!(decode(&original[..len]).is_err());
    }
}

#[test]
fn selection_requires_a_complete_pair_inside_the_fragment() {
    for (start, end, valid) in [
        (0, 6, true),
        (3, 3, true),
        (1, 1, false),
        (1, 6, false),
        (0, 7, false),
        (6, 0, false),
    ] {
        let header = b"Version:1.0\nStartHTML:-1\nEndHTML:-1\nStartFragment:0000000000\nEndFragment:0000000000\nStartSelection:0000000000\nEndSelection:0000000000\n";
        let mut bytes = header.to_vec();
        bytes.extend_from_slice("你好".as_bytes());
        replace_offset(&mut bytes, b"StartFragment:", header.len());
        replace_offset(&mut bytes, b"EndFragment:", header.len() + 6);
        replace_offset(&mut bytes, b"StartSelection:", header.len() + start);
        replace_offset(&mut bytes, b"EndSelection:", header.len() + end);
        assert_eq!(decode(&bytes).is_ok(), valid);
    }
    let mut native = encode(b"x").unwrap();
    let end = native.windows(8).position(|b| b == b"EndHTML:").unwrap();
    native[end..end + 8].copy_from_slice(b"Unknown:");
    assert!(decode(&native).is_err());
}

#[test]
fn metadata_cannot_redirect_parsing_into_payload_or_duplicate_offsets() {
    let valid = encode(b"StartHTML:0\nEndHTML:0\n").unwrap();
    assert_eq!(decode(&valid).unwrap(), b"StartHTML:0\nEndHTML:0\n");
    let mut duplicate = valid.clone();
    let start = duplicate.windows(8).position(|b| b == b"EndHTML:").unwrap();
    duplicate.splice(start..start + 8, b"StartHTML:".iter().copied());
    assert!(decode(&duplicate).is_err());
    let mut signed = valid.clone();
    let start = signed.windows(10).position(|b| b == b"StartHTML:").unwrap() + 10;
    signed[start] = b'+';
    assert!(decode(&signed).is_err());
    let mut missing_selection = valid;
    let start = missing_selection
        .windows(8)
        .position(|b| b == b"EndHTML:")
        .unwrap();
    missing_selection.splice(start..start + 8, b"StartSelection:".iter().copied());
    assert!(decode(&missing_selection).is_err());
}
