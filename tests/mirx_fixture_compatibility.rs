use std::borrow::Cow;

use mirx::{Document, Layout, Reader, reader::PayloadLimits};

const FONT_FIXTURES: &[&[u8]] = &[
    include_bytes!("fixtures/misans_coverage_16_4bit.mirx"),
    include_bytes!("fixtures/misans_sdf_ascii_32.mirx"),
    include_bytes!("fixtures/misans_sdf_cjk_32.mirx"),
];

#[test]
fn shipped_font_fixtures_open_through_container_and_document_paths() {
    for &bytes in FONT_FIXTURES {
        let reader = Reader::open(bytes).unwrap();
        assert_eq!(reader.layout(), Layout::Chunk);
        assert!(reader.chunks().next().is_some());
        reader
            .validate_known_payloads(&PayloadLimits::HOST)
            .unwrap();
        assert_eq!(reader.logical_len(), bytes.len());

        let finished = Document::open(bytes).unwrap().finish().unwrap();
        assert!(matches!(finished, Cow::Borrowed(_)));
        assert_eq!(finished.as_ref(), bytes);
    }
}
