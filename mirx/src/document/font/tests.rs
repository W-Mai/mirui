use alloc::{borrow::Cow, vec::Vec};
use core::cell::Cell;

use super::*;
use crate::document::{EncodeOptions, PayloadOrigin, RawChunkPolicy};
use crate::font::{
    CmapEntry, FontAdvanceSource, FontAsset, FontFace, FontMetadataError, FontRepresentation,
    GlyphId, GlyphMap, GlyphSurfaceAsset, RasterMetrics, RawGlyphs, RepresentationAsset,
};
use crate::image::SampleLayout;
use crate::{ColorFormat, ImageAsset, PayloadLimits, encode_chunks, types::Fixed};

fn font() -> Font {
    let map = GlyphMap::cells(2, 2, 2).unwrap();
    let raw = RawGlyphs::builder(map, SampleLayout::A8)
        .build(&[0x11; 8])
        .unwrap();
    let surfaces = [GlyphSurfaceAsset::raw(raw)];
    let cmap = [
        CmapEntry::new('A', GlyphId::new(0)),
        CmapEntry::new('B', GlyphId::new(1)),
    ];
    let advances = [Fixed::ONE, Fixed::from_ratio(5, 4)];
    let face = FontFace::new(
        1_000,
        GlyphId::NOTDEF,
        2,
        Fixed::from_int(2),
        Fixed::from_ratio(-1, 2),
        Fixed::ZERO,
    )
    .unwrap();
    let representations = [RepresentationAsset::new(
        FontRepresentation::coverage(8, 12, 8).unwrap(),
        0,
    )];
    let raster_metrics = [RasterMetrics::new(Fixed::ZERO, Fixed::ONE); 2];
    Font::from_asset(
        FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances)).with_rasters(
            &representations,
            &raster_metrics,
            &surfaces,
        ),
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
    assert_eq!(document.get(id).unwrap().decode_font().unwrap(), expected);
    assert_eq!(
        document.get(id).unwrap().payload_origin(),
        PayloadOrigin::OWNED
    );

    let bytes = document.encode(&EncodeOptions::new()).unwrap();
    let reopened = Document::open(&bytes).unwrap();
    let id = reopened.chunks().next().unwrap().id();
    assert_eq!(reopened.get(id).unwrap().decode_font().unwrap(), expected);
}

#[test]
fn retained_limits_gate_typed_reads_and_writes() {
    let expected = font();
    let source = file(&expected, ChunkFlags::NONE);
    let low = PayloadLimits::HOST.with_max_font_glyphs(1);
    let options = crate::document::OpenOptions::new().with_payload_limits(low);
    let document = Document::open_with(&source, &options).unwrap();
    let id = document.chunks().next().unwrap().id();
    assert!(matches!(
        document.decode_font_at(id),
        Err(FontAccessError::InvalidPayload(FontError::Metadata(
            FontMetadataError::TooManyGlyphs { .. }
        )))
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
            font.set_cmap_entry(1, CmapEntry::new('中', GlyphId::new(1)))
                .unwrap();
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
        .edit_font(id, |font| {
            font.set_cmap_entry(1, CmapEntry::new('中', GlyphId::new(1)))
                .unwrap()
        })
        .unwrap();
    assert!(document.is_dirty());
    assert_eq!(
        document
            .decode_font_at(id)
            .unwrap()
            .cmap()
            .iter()
            .map(|entry| entry.scalar())
            .collect::<Vec<_>>(),
        ['A', '中']
    );
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
        .push_raw(crate::document::RawChunkInput {
            chunk_type: ChunkType::META,
            flags: ChunkFlags::NONE,
            payload: crate::document::PayloadInput::Borrowed(b"meta"),
            policy: RawChunkPolicy::infer()
                .with_relocation(crate::document::RelocationAssumption::AssumeRelocatable)
                .with_critical_semantics(
                    crate::document::CriticalAssumption::AssumeCriticalUnderstood,
                ),
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
    assert_eq!(
        document
            .decode_font_at(id)
            .unwrap()
            .cmap()
            .iter()
            .map(|entry| entry.scalar())
            .collect::<Vec<_>>(),
        ['A', 'B']
    );
}
