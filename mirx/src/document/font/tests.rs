use alloc::{borrow::Cow, vec::Vec};
use core::cell::Cell;

use super::*;
use crate::font::{
    GlyphMap, GlyphMetrics, GlyphSurfaceAsset, LineMetrics, RawGlyphs, RepresentationAsset,
};
use crate::image::SampleLayout;
use crate::{
    ColorFormat, EncodeOptions, Fixed, FontAsset, FontRepresentation, ImageAsset, PayloadLimits,
    PayloadOrigin, RawChunkPolicy, encode_chunks,
};

fn font() -> Font {
    let map = GlyphMap::glyph_major(2, 2, 2).unwrap();
    let raw = RawGlyphs::builder(map, SampleLayout::A8)
        .build(&[0x11; 8])
        .unwrap();
    let surfaces = [GlyphSurfaceAsset::raw(raw)];
    let metrics = [
        GlyphMetrics::new(Fixed::ONE, Fixed::ZERO, Fixed::ONE),
        GlyphMetrics::new(Fixed::from_ratio(5, 4), Fixed::ZERO, Fixed::ONE),
    ];
    let line = LineMetrics::new(
        Fixed::from_int(2),
        Fixed::from_ratio(-1, 2),
        Fixed::from_ratio(5, 2),
    )
    .unwrap();
    let representations = [RepresentationAsset::new(
        FontRepresentation::coverage(8, 12, 8).unwrap(),
        0,
        line,
        &metrics,
    )];
    Font::from_asset(
        FontAsset::new(&['A', 'B'], &representations, &surfaces),
        &PayloadLimits::HOST,
    )
    .unwrap()
}

fn file(font: &Font, flags: ChunkFlags) -> Vec<u8> {
    let payload = font.encode().unwrap();
    encode_chunks(&[(ChunkType::FONT.raw(), flags.bits(), payload.as_slice())])
}

#[test]
fn owned_font_round_trips_through_document_and_file() {
    let expected = font();
    let mut document = Document::new();
    let id = document
        .push_font_with_flags(&expected, ChunkFlags::CRITICAL)
        .unwrap();
    assert_eq!(document.decode_font(id).unwrap(), expected);
    assert_eq!(
        document.get(id).unwrap().payload_origin(),
        PayloadOrigin::OWNED
    );

    let bytes = document.encode(&EncodeOptions::new()).unwrap();
    let reopened = Document::open(&bytes).unwrap();
    let id = reopened.chunks().next().unwrap().id();
    assert_eq!(reopened.decode_font(id).unwrap(), expected);
}

#[test]
fn retained_limits_gate_typed_reads_and_writes() {
    let expected = font();
    let source = file(&expected, ChunkFlags::NONE);
    let low = PayloadLimits::HOST.with_max_font_glyphs(1);
    let options = crate::OpenOptions::new().with_payload_limits(low);
    let document = Document::open_with(&source, &options).unwrap();
    let id = document.chunks().next().unwrap().id();
    assert!(matches!(
        document.decode_font(id),
        Err(FontAccessError::InvalidPayload(
            FontError::TooManyGlyphs { .. }
        ))
    ));
    let mut authored = Document::new_with_limits(low);
    assert!(matches!(
        authored.push_font(&expected),
        Err(EditError::InvalidFont(FontError::TooManyGlyphs { .. }))
    ));
    assert_eq!(authored.chunks().len(), 0);
}

#[test]
fn no_op_and_failed_callbacks_preserve_source_storage() {
    let expected = font();
    let source = file(&expected, ChunkFlags::NONE);
    let mut document = Document::open(&source).unwrap();
    let id = document.chunks().next().unwrap().id();
    let original = document.get(id).unwrap().payload_bytes().unwrap().as_ptr();
    document.edit_font(id, |_| {}).unwrap();
    assert!(!document.is_dirty());
    assert_eq!(
        document.get(id).unwrap().payload_bytes().unwrap().as_ptr(),
        original
    );

    assert_eq!(
        document.try_edit_font(id, |font| {
            font.set_codepoint(1, '中').unwrap();
            Err("cancel")
        }),
        Err(TryEditError::Callback("cancel"))
    );
    assert!(!document.is_dirty());
    assert_eq!(
        document.get(id).unwrap().payload_bytes().unwrap().as_ptr(),
        original
    );
}

#[test]
fn successful_metadata_edit_reauthors_canonical_payload() {
    let expected = font();
    let source = file(&expected, ChunkFlags::NONE);
    let mut document = Document::open(&source).unwrap();
    let id = document.chunks().next().unwrap().id();
    document
        .edit_font(id, |font| font.set_codepoint(1, '中').unwrap())
        .unwrap();
    assert!(document.is_dirty());
    assert_eq!(document.decode_font(id).unwrap().codepoints(), ['A', '中']);
    assert_eq!(
        document.get(id).unwrap().payload_origin(),
        PayloadOrigin::OWNED
    );
}

#[test]
fn identity_type_and_layout_errors_precede_callbacks() {
    let called = Cell::new(false);
    let main = [1, 2, 3, 4];
    let mut flat = Document::new_flat(ImageAsset::new(
        2,
        2,
        ColorFormat::A8,
        2,
        Cow::Borrowed(&main),
    ))
    .unwrap();
    assert_eq!(
        flat.try_edit_font(ChunkId::new(0), |_| {
            called.set(true);
            Ok::<_, ()>(())
        }),
        Err(TryEditError::Edit(EditError::ChunkLayoutRequired))
    );
    assert!(!called.get());

    let mut document = Document::new();
    let meta = document
        .push_raw(crate::RawChunkInput {
            chunk_type: ChunkType::META,
            flags: ChunkFlags::NONE,
            payload: crate::PayloadInput::Borrowed(b"meta"),
            policy: RawChunkPolicy::infer()
                .with_relocation(crate::RelocationAssumption::AssumeRelocatable)
                .with_critical_semantics(crate::CriticalAssumption::AssumeCriticalUnderstood),
        })
        .unwrap();
    assert_eq!(
        document.try_edit_font(meta, |_| {
            called.set(true);
            Ok::<_, ()>(())
        }),
        Err(TryEditError::Edit(EditError::InvalidChunkType))
    );
    assert!(!called.get());
}

#[test]
fn malformed_existing_font_never_invokes_callback_but_accepts_replacement() {
    let source = encode_chunks(&[(ChunkType::FONT.raw(), 0, b"bad")]);
    let mut document = Document::open(&source).unwrap();
    let id = document.chunks().next().unwrap().id();
    let called = Cell::new(false);
    assert!(matches!(
        document.edit_font(id, |_| called.set(true)),
        Err(EditError::InvalidFont(_))
    ));
    assert!(!called.get());
    document.replace_font(id, &font()).unwrap();
    assert_eq!(document.decode_font(id).unwrap().codepoints(), ['A', 'B']);
}
