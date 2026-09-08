use std::borrow::Cow;

use mirx::image::ColorFormat;
use mirx::{
    ChunkType, CriticalAssumption, Document, Layout, RawChunkPolicy, Reader, RelocationAssumption,
    ReservedBitsPolicy,
    document::{OpenOptions, RawTypePolicy},
};

fn decode_hex(source: &str) -> Vec<u8> {
    source
        .split_ascii_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

const fn relocatable_policy() -> RawChunkPolicy {
    RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::Infer,
        reserved_flag_bits: ReservedBitsPolicy::Reject,
    }
}

#[test]
fn flat_golden_opens_through_reader_and_document_paths() {
    let bytes = decode_hex(include_str!("fixtures/flat-a8-v1.hex"));
    assert_eq!(bytes.len(), 32);

    let reader = Reader::open(&bytes).unwrap();
    assert_eq!(reader.layout(), Layout::Flat);
    let image = reader.flat_image().unwrap();
    assert_eq!((image.width(), image.height(), image.stride()), (2, 2, 2));
    assert_eq!(image.format(), ColorFormat::A8);
    assert_eq!(image.main(), [0x00, 0x40, 0x80, 0xff]);
    assert!(image.extra().is_none());

    let finished = Document::open(&bytes).unwrap().finish().unwrap();
    assert!(matches!(finished, Cow::Borrowed(_)));
    assert_eq!(finished.as_ref(), bytes);
}

#[test]
fn chunk_golden_preserves_entries_and_supports_checked_rewrite() {
    let bytes = decode_hex(include_str!("fixtures/chunk-custom-v1.hex"));
    assert_eq!(bytes.len(), 87);
    let first_type = ChunkType::new(0xbeef).unwrap();
    let second_type = ChunkType::new(0xcafe).unwrap();

    let reader = Reader::open(&bytes).unwrap();
    assert_eq!(reader.chunks().len(), 2);
    assert_eq!(
        reader
            .chunks()
            .find(|chunk| chunk.chunk_type() == first_type)
            .map(|chunk| chunk.payload()),
        Some(b"abc".as_slice())
    );
    assert_eq!(
        reader
            .chunks()
            .find(|chunk| chunk.chunk_type() == second_type)
            .map(|chunk| chunk.payload()),
        Some(b"WXYZ".as_slice())
    );
    assert_eq!(
        reader
            .chunks()
            .map(|chunk| chunk.payload())
            .collect::<Vec<_>>(),
        [b"abc".as_slice(), b"WXYZ".as_slice()]
    );
    let finished = Document::open(&bytes).unwrap().finish().unwrap();
    assert!(matches!(finished, Cow::Borrowed(_)));
    assert_eq!(finished.as_ref(), bytes);

    let policies = [
        RawTypePolicy {
            chunk_type: first_type,
            policy: relocatable_policy(),
        },
        RawTypePolicy {
            chunk_type: second_type,
            policy: relocatable_policy(),
        },
    ];
    let mut document = Document::open_with(
        &bytes,
        &OpenOptions::host_tools().with_raw_type_policies(&policies),
    )
    .unwrap();
    let mut chunks = document.chunks();
    let first = chunks.next().unwrap().id();
    let second = chunks.next().unwrap().id();
    document.move_before(second, first).unwrap();
    let rewritten = document.finish().unwrap();

    assert_eq!(
        Reader::open(&rewritten)
            .unwrap()
            .chunks()
            .map(|chunk| chunk.payload())
            .collect::<Vec<_>>(),
        [b"WXYZ".as_slice(), b"abc".as_slice()]
    );
    let rewritten = Reader::open(&rewritten).unwrap();
    assert_eq!(
        rewritten
            .chunks()
            .find(|chunk| chunk.chunk_type() == first_type)
            .map(|chunk| chunk.payload()),
        Some(b"abc".as_slice())
    );
    assert_eq!(
        rewritten
            .chunks()
            .find(|chunk| chunk.chunk_type() == second_type)
            .map(|chunk| chunk.payload()),
        Some(b"WXYZ".as_slice())
    );
}
