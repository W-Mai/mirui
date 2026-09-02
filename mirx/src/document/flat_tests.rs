use alloc::borrow::Cow;
use alloc::vec;
use alloc::vec::Vec;

use super::*;
use crate::{
    ColorFormat, EncodeOptions, FlatImageInput, ImagePayloadError, LayoutPolicy, Reader,
    encode_flat,
};

const ALL_FORMATS: [ColorFormat; 16] = [
    ColorFormat::I1,
    ColorFormat::I2,
    ColorFormat::I4,
    ColorFormat::I8,
    ColorFormat::A1,
    ColorFormat::A2,
    ColorFormat::A4,
    ColorFormat::A8,
    ColorFormat::L8,
    ColorFormat::RGB565,
    ColorFormat::RGB565Swapped,
    ColorFormat::RGB565A8,
    ColorFormat::RGB888,
    ColorFormat::XRGB8888,
    ColorFormat::RGBA8888,
    ColorFormat::BGRA8888,
];

fn plane_lengths(format: ColorFormat, width: u32, height: u32, stride: u32) -> (usize, usize) {
    let main = usize::try_from(stride.checked_mul(height).unwrap()).unwrap();
    let extra = usize::try_from(format.extra_size(width, height, stride).unwrap()).unwrap();
    (main, extra)
}

fn owned_asset(
    format: ColorFormat,
    width: u32,
    height: u32,
    stride: u32,
    main_byte: u8,
    extra_byte: u8,
) -> ImageAsset<'static> {
    let (main_len, extra_len) = plane_lengths(format, width, height, stride);
    ImageAsset::new(
        width,
        height,
        format,
        stride,
        Cow::Owned(vec![main_byte; main_len]),
        (extra_len != 0).then(|| Cow::Owned(vec![extra_byte; extra_len])),
    )
}

fn new_flat_error(image: ImageAsset<'static>) -> EditError {
    match Document::new_flat(image) {
        Ok(_) => panic!("expected invalid FLAT asset"),
        Err(error) => error,
    }
}

#[test]
fn new_flat_covers_every_color_format_with_padded_stride_and_exact_extra() {
    for format in ALL_FORMATS {
        let width = 9;
        let height = 3;
        let stride = format
            .minimum_stride(width)
            .unwrap()
            .checked_add(2)
            .unwrap();
        let (main_len, extra_len) = plane_lengths(format, width, height, stride);
        let document = Document::new_flat(owned_asset(
            format,
            width,
            height,
            stride,
            format.to_u8(),
            0xa5,
        ))
        .unwrap();

        assert_eq!(document.layout(), Layout::Flat);
        assert!(document.is_dirty());
        assert_eq!(document.chunks().len(), 0);
        assert_eq!(document.primary(), None);
        assert_eq!(
            document.primary_hints(),
            PrimaryHints::new(format.to_u8(), width, height, stride)
        );
        let image = document.flat_image().unwrap();
        assert_eq!(image.width(), width);
        assert_eq!(image.height(), height);
        assert_eq!(image.format(), format);
        assert_eq!(image.stride(), stride);
        assert_eq!(image.main().len(), main_len);
        assert!(image.main().iter().all(|&byte| byte == format.to_u8()));
        match image.extra() {
            Some(extra) => {
                assert_eq!(extra.len(), extra_len);
                assert!(extra.iter().all(|&byte| byte == 0xa5));
            }
            None => assert_eq!(extra_len, 0),
        }

        let encoded = document.encode(&EncodeOptions::new()).unwrap();
        let reopened = Reader::open(&encoded).unwrap();
        assert_eq!(reopened.layout(), Layout::Flat);
        let reopened = reopened.flat_image().unwrap();
        assert_eq!(reopened.width(), width);
        assert_eq!(reopened.height(), height);
        assert_eq!(reopened.format(), format);
        assert_eq!(reopened.stride(), stride);
        assert_eq!(reopened.main(), image.main());
        assert_eq!(reopened.extra(), image.extra());
    }
}

#[test]
fn extra_plane_lengths_cover_indexed_palettes_and_rgb565a8_alpha() {
    for (format, expected) in [
        (ColorFormat::I1, 8),
        (ColorFormat::I2, 16),
        (ColorFormat::I4, 64),
        (ColorFormat::I8, 1024),
    ] {
        let width = 5;
        let height = 2;
        let stride = format.minimum_stride(width).unwrap();
        let document =
            Document::new_flat(owned_asset(format, width, height, stride, 1, 2)).unwrap();
        assert_eq!(
            document.flat_image().unwrap().extra().unwrap().len(),
            expected
        );
    }

    let width = 5;
    let height = 3;
    let format = ColorFormat::RGB565A8;
    let stride = format
        .minimum_stride(width)
        .unwrap()
        .checked_add(4)
        .unwrap();
    let document = Document::new_flat(owned_asset(format, width, height, stride, 3, 4)).unwrap();
    assert_eq!(document.flat_image().unwrap().extra().unwrap().len(), 15);
}

#[test]
fn borrowed_owned_and_mixed_planes_retain_pointer_and_capacity() {
    let borrowed_main = vec![0x11; 6];
    let mut owned_extra = Vec::with_capacity(93);
    owned_extra.resize(64, 0x22);
    let owned_extra_pointer = owned_extra.as_ptr();
    let owned_extra_capacity = owned_extra.capacity();
    let document = Document::new_flat(ImageAsset::new(
        3,
        2,
        ColorFormat::I4,
        3,
        Cow::Borrowed(&borrowed_main),
        Some(Cow::Owned(owned_extra)),
    ))
    .unwrap();
    let DocumentState::Flat(record) = &document.state else {
        panic!("expected FLAT document");
    };
    let PlaneStorage::Borrowed(main) = record.main_storage() else {
        panic!("main plane must remain borrowed");
    };
    assert_eq!(main.as_ptr(), borrowed_main.as_ptr());
    let Some(PlaneStorage::Owned(extra)) = record.extra_storage() else {
        panic!("extra plane must remain owned");
    };
    assert_eq!(extra.as_ptr(), owned_extra_pointer);
    assert_eq!(extra.capacity(), owned_extra_capacity);
    let view = document.flat_image().unwrap();
    assert_eq!(view.main().as_ptr(), borrowed_main.as_ptr());
    assert_eq!(view.extra().unwrap().as_ptr(), owned_extra_pointer);

    let mut owned_main = Vec::with_capacity(37);
    owned_main.resize(6, 0x33);
    let owned_main_pointer = owned_main.as_ptr();
    let owned_main_capacity = owned_main.capacity();
    let borrowed_extra = [0x44; 64];
    let document = Document::new_flat(ImageAsset::new(
        3,
        2,
        ColorFormat::I4,
        3,
        Cow::Owned(owned_main),
        Some(Cow::Borrowed(&borrowed_extra)),
    ))
    .unwrap();
    let DocumentState::Flat(record) = &document.state else {
        panic!("expected FLAT document");
    };
    let PlaneStorage::Owned(main) = record.main_storage() else {
        panic!("main plane must remain owned");
    };
    assert_eq!(main.as_ptr(), owned_main_pointer);
    assert_eq!(main.capacity(), owned_main_capacity);
    let Some(PlaneStorage::Borrowed(extra)) = record.extra_storage() else {
        panic!("extra plane must remain borrowed");
    };
    assert_eq!(extra.as_ptr(), borrowed_extra.as_ptr());
}

#[test]
fn replacement_moves_owned_planes_without_changing_the_origin_allocation() {
    let source = encode_flat(&FlatImageInput {
        width: 1,
        height: 1,
        stride: 1,
        format: ColorFormat::A8,
        main: &[7],
        extra: None,
    });
    let source_pointer = source.as_ptr();
    let mut main = Vec::with_capacity(31);
    main.resize(4, 0x11);
    let main_pointer = main.as_ptr();
    let main_capacity = main.capacity();
    let mut extra = Vec::with_capacity(97);
    extra.resize(64, 0x22);
    let extra_pointer = extra.as_ptr();
    let extra_capacity = extra.capacity();

    let mut document = Document::open(&source).unwrap();
    document
        .replace_flat_image(ImageAsset::new(
            3,
            2,
            ColorFormat::I4,
            2,
            Cow::Owned(main),
            Some(Cow::Owned(extra)),
        ))
        .unwrap();

    assert_eq!(document.origin.source().unwrap().as_ptr(), source_pointer);
    let DocumentState::Flat(record) = &document.state else {
        panic!("expected FLAT document");
    };
    let PlaneStorage::Owned(main) = record.main_storage() else {
        panic!("replacement main must remain owned");
    };
    assert_eq!(main.as_ptr(), main_pointer);
    assert_eq!(main.capacity(), main_capacity);
    let Some(PlaneStorage::Owned(extra)) = record.extra_storage() else {
        panic!("replacement extra must remain owned");
    };
    assert_eq!(extra.as_ptr(), extra_pointer);
    assert_eq!(extra.capacity(), extra_capacity);
}

#[test]
fn same_content_replacement_preserves_source_storage_and_noop_finish() {
    let pixels = [0x12, 0x34, 0x56, 0x70];
    let palette = [0xa5; 64];
    let source = encode_flat(&FlatImageInput {
        width: 3,
        height: 2,
        stride: 2,
        format: ColorFormat::I4,
        main: &pixels,
        extra: Some(&palette),
    });
    let source_pointer = source.as_ptr();
    let mut borrowed = Document::open(&source).unwrap();
    let (main_range, extra_range) = match &borrowed.state {
        DocumentState::Flat(record) => match record.plane_storage() {
            Some((PlaneStorage::SourceRange(main), Some(PlaneStorage::SourceRange(extra)))) => {
                (*main, *extra)
            }
            _ => panic!("opened planes must remain source-backed"),
        },
        _ => panic!("expected FLAT document"),
    };
    borrowed
        .replace_flat_image(ImageAsset::new(
            3,
            2,
            ColorFormat::I4,
            2,
            Cow::Owned(pixels.to_vec()),
            Some(Cow::Borrowed(&palette)),
        ))
        .unwrap();
    assert!(!borrowed.is_dirty());
    match &borrowed.state {
        DocumentState::Flat(record) => {
            assert_eq!(
                record.main_storage(),
                &PlaneStorage::SourceRange(main_range)
            );
            assert_eq!(
                record.extra_storage(),
                Some(&PlaneStorage::SourceRange(extra_range))
            );
        }
        _ => panic!("expected FLAT document"),
    }
    let Cow::Borrowed(finished) = borrowed.finish().unwrap() else {
        panic!("exact borrowed replacement must retain no-op finish");
    };
    assert_eq!(finished.as_ptr(), source_pointer);
    assert_eq!(finished, source);

    let mut owned_source = Vec::with_capacity(source.len() + 47);
    owned_source.extend_from_slice(&source);
    let owned_pointer = owned_source.as_ptr();
    let owned_capacity = owned_source.capacity();
    let mut owned = Document::from_vec(owned_source).unwrap();
    owned
        .replace_flat_image(ImageAsset::new(
            3,
            2,
            ColorFormat::I4,
            2,
            Cow::Borrowed(&pixels),
            Some(Cow::Borrowed(&palette)),
        ))
        .unwrap();
    assert!(!owned.is_dirty());
    let Cow::Owned(finished) = owned.finish().unwrap() else {
        panic!("exact owned replacement must retain no-op finish");
    };
    assert_eq!(finished.as_ptr(), owned_pointer);
    assert_eq!(finished.capacity(), owned_capacity);
    assert_eq!(finished, source);
}

#[test]
fn changed_replacement_stays_flat_and_force_chunk_reuses_segmented_emission() {
    let source = encode_flat(&FlatImageInput {
        width: 1,
        height: 1,
        stride: 1,
        format: ColorFormat::A8,
        main: &[7],
        extra: None,
    });
    let main = [0x12, 0x34, 0x56, 0x70];
    let palette = [0x5a; 64];
    let mut document = Document::open(&source).unwrap();
    document
        .replace_flat_image(ImageAsset::new(
            3,
            2,
            ColorFormat::I4,
            2,
            Cow::Borrowed(&main),
            Some(Cow::Borrowed(&palette)),
        ))
        .unwrap();

    assert!(document.is_dirty());
    assert_eq!(document.layout(), Layout::Flat);
    assert_eq!(document.chunks().len(), 0);
    assert_eq!(document.primary(), None);
    assert_eq!(
        document.primary_hints(),
        PrimaryHints::new(ColorFormat::I4.to_u8(), 3, 2, 2)
    );
    for policy in [
        LayoutPolicy::PreserveOrPromote,
        LayoutPolicy::SmallestRepresentable,
        LayoutPolicy::ForceFlat,
    ] {
        let encoded = document
            .encode(&EncodeOptions::new().with_layout_policy(policy))
            .unwrap();
        let image = Reader::open(&encoded).unwrap().flat_image().unwrap();
        assert_eq!(image.main(), main);
        assert_eq!(image.extra(), Some(palette.as_slice()));
    }

    let encoded = document
        .encode(&EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceChunk))
        .unwrap();
    let reader = Reader::open(&encoded).unwrap();
    assert_eq!(reader.layout(), Layout::Chunk);
    let mut chunks = reader.chunks();
    let chunk = chunks.next().unwrap();
    assert_eq!(chunk.chunk_type(), ChunkType::IMAGE);
    let image = ImageView::open_payload_at(chunk.payload(), chunk.payload_offset()).unwrap();
    assert_eq!(image.main(), main);
    assert_eq!(image.extra(), Some(palette.as_slice()));
    assert!(chunks.next().is_none());

    let Cow::Owned(finished) = document.finish().unwrap() else {
        panic!("changed FLAT document must be rewritten");
    };
    assert_eq!(Reader::open(&finished).unwrap().layout(), Layout::Flat);
}

#[test]
fn new_flat_finish_emits_owned_canonical_bytes_and_reopens() {
    let main = [0x10, 0x20, 0x30, 0x40];
    let palette = [0x80; 64];
    let document = Document::new_flat(ImageAsset::new(
        3,
        2,
        ColorFormat::I4,
        2,
        Cow::Borrowed(&main),
        Some(Cow::Borrowed(&palette)),
    ))
    .unwrap();
    assert!(matches!(document.origin, Origin::New));
    assert!(document.is_dirty());

    let Cow::Owned(finished) = document.finish().unwrap() else {
        panic!("new FLAT document must encode into owned bytes");
    };
    assert_eq!(finished.len(), FLAT_HEADER_LEN + main.len() + palette.len());
    let reader = Reader::open(&finished).unwrap();
    assert_eq!(reader.layout(), Layout::Flat);
    let image = reader.flat_image().unwrap();
    assert_eq!(image.main(), main);
    assert_eq!(image.extra(), Some(palette.as_slice()));
}

#[test]
fn flat_asset_validation_reports_stride_plane_and_overflow_errors() {
    assert_eq!(
        new_flat_error(ImageAsset::new(
            2,
            1,
            ColorFormat::RGB565,
            3,
            Cow::Owned(vec![0; 3]),
            None,
        )),
        EditError::InvalidPayload(ImagePayloadError::StrideTooSmall {
            minimum: 4,
            actual: 3,
        })
    );
    assert_eq!(
        new_flat_error(ImageAsset::new(
            2,
            2,
            ColorFormat::A8,
            2,
            Cow::Owned(vec![0; 3]),
            None,
        )),
        EditError::InvalidPayload(ImagePayloadError::MainPlaneLengthMismatch {
            expected: 4,
            actual: 3,
        })
    );
    assert_eq!(
        new_flat_error(ImageAsset::new(
            3,
            2,
            ColorFormat::I4,
            2,
            Cow::Owned(vec![0; 4]),
            None,
        )),
        EditError::InvalidPayload(ImagePayloadError::ExtraPlaneLengthMismatch {
            expected: 64,
            actual: 0,
        })
    );
    assert_eq!(
        new_flat_error(ImageAsset::new(
            u32::MAX,
            1,
            ColorFormat::RGBA8888,
            u32::MAX,
            Cow::Owned(Vec::new()),
            None,
        )),
        EditError::InvalidPayload(ImagePayloadError::SizeOverflow)
    );
    assert_eq!(
        new_flat_error(ImageAsset::new(
            1,
            2,
            ColorFormat::A8,
            u32::MAX,
            Cow::Owned(Vec::new()),
            None,
        )),
        EditError::InvalidPayload(ImagePayloadError::SizeOverflow)
    );
}

#[test]
fn empty_extra_is_normalized_and_zero_sized_images_remain_representable() {
    let mut empty = Vec::with_capacity(19);
    empty.clear();
    let document = Document::new_flat(ImageAsset::new(
        0,
        3,
        ColorFormat::A8,
        0,
        Cow::Owned(Vec::new()),
        Some(Cow::Owned(empty)),
    ))
    .unwrap();
    let DocumentState::Flat(record) = &document.state else {
        panic!("expected FLAT document");
    };
    assert!(record.extra_storage().is_none());
    assert_eq!(document.flat_image().unwrap().extra(), None);
    assert_eq!(
        document.encode(&EncodeOptions::new()).unwrap().len(),
        FLAT_HEADER_LEN
    );

    let indexed_palette = [0x5a; 8];
    let indexed = Document::new_flat(ImageAsset::new(
        0,
        0,
        ColorFormat::I1,
        0,
        Cow::Owned(Vec::new()),
        Some(Cow::Borrowed(&indexed_palette)),
    ))
    .unwrap();
    assert_eq!(
        indexed.flat_image().unwrap().extra(),
        Some(indexed_palette.as_slice())
    );

    let alpha = Document::new_flat(ImageAsset::new(
        0,
        3,
        ColorFormat::RGB565A8,
        0,
        Cow::Owned(Vec::new()),
        Some(Cow::Owned(Vec::new())),
    ))
    .unwrap();
    assert_eq!(alpha.flat_image().unwrap().extra(), None);
}
