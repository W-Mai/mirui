use std::borrow::Cow;

use mirx::{Document, Layout, MirxFile, PayloadLimits, Reader, parse, parse_chunk};

const FONT_FIXTURES: &[&[u8]] = &[
    include_bytes!("fixtures/misans_gray_16_4bit.mirx"),
    include_bytes!("fixtures/misans_regular_ascii_32_4bit.mirx"),
    include_bytes!("fixtures/misans_regular_cjk_32_4bit.mirx"),
];

#[test]
fn shipped_font_fixtures_open_through_legacy_and_bounded_paths() {
    for &bytes in FONT_FIXTURES {
        let legacy = parse_chunk(bytes).unwrap();
        assert!(!legacy.entries.is_empty());
        assert!(matches!(parse(bytes).unwrap(), MirxFile::Chunk(_)));

        let reader = Reader::open(bytes).unwrap();
        assert_eq!(reader.layout(), Layout::Chunk);
        reader
            .validate_known_payloads(&PayloadLimits::HOST)
            .unwrap();
        assert_eq!(reader.logical_len(), bytes.len());

        let finished = Document::open(bytes).unwrap().finish().unwrap();
        assert!(matches!(finished, Cow::Borrowed(_)));
        assert_eq!(finished.as_ref(), bytes);
    }
}
