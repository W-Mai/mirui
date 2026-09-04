use alloc::borrow::Cow;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::Cell;

use super::payload::ResolvedNodePayload;
use super::*;
use crate::{
    ColorFormat, CriticalAssumption, EncodeError, EncodeOptions, FlatImageInput, ImageAsset,
    ImageView, LayoutPolicy, OpenOptions, PayloadOrigin, RawChunkInput, ReadOptions, Reader,
    RelocationAssumption, ReservedBitsPolicy, TrailingBytesPolicy, VERSION_MINOR, crc32,
    encode_chunks, encode_flat,
};

const CUSTOM: ChunkType = match ChunkType::new(0xbeef) {
    Some(chunk_type) => chunk_type,
    None => panic!("nonzero chunk type"),
};
const CUSTOM_HINTS: PrimaryHints =
    PrimaryHints::new(crate::image::SampleLayout::new(0xa5), 13, 21, 55);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlaneSnapshot {
    kind: u8,
    range: Option<SourceRange>,
    pointer: usize,
    len: usize,
    capacity: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FlatSnapshot {
    origin_pointer: usize,
    origin_len: usize,
    origin_capacity: Option<usize>,
    logical_len: usize,
    file: FileMeta,
    compatibility: Compatibility,
    trailing: TrailingState,
    dirty: bool,
    next_id: u32,
    image: ImageMeta,
    main: PlaneSnapshot,
    extra: Option<PlaneSnapshot>,
}

const fn explicit_policy() -> RawChunkPolicy {
    RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
        reserved_flag_bits: ReservedBitsPolicy::Reject,
    }
}

fn raw<'a>(chunk_type: ChunkType, payload: PayloadInput<'a>) -> RawChunkInput<'a> {
    RawChunkInput {
        chunk_type,
        flags: ChunkFlags::NONE,
        payload,
        policy: explicit_policy(),
    }
}

fn a8_source() -> Vec<u8> {
    encode_flat(&FlatImageInput {
        width: 2,
        height: 2,
        stride: ColorFormat::A8.minimum_stride(2).unwrap(),
        format: ColorFormat::A8,
        main: &[1, 2, 3, 4],
        extra: None,
    })
}

fn origin_snapshot(origin: &Origin<'_>) -> (usize, usize, Option<usize>) {
    match origin {
        Origin::New => (0, 0, None),
        Origin::Borrowed(bytes) => (bytes.as_ptr() as usize, bytes.len(), None),
        Origin::Owned(bytes) => (bytes.as_ptr() as usize, bytes.len(), Some(bytes.capacity())),
    }
}

fn plane_snapshot(document: &Document<'_>, plane: &PlaneStorage<'_>) -> PlaneSnapshot {
    let (kind, range, bytes, capacity) = match plane {
        PlaneStorage::SourceRange(range) => (
            0,
            Some(*range),
            document.origin.resolve(*range).unwrap(),
            None,
        ),
        PlaneStorage::Borrowed(bytes) => (1, None, *bytes, None),
        PlaneStorage::Owned(bytes) => (2, None, bytes.as_slice(), Some(bytes.capacity())),
    };
    PlaneSnapshot {
        kind,
        range,
        pointer: bytes.as_ptr() as usize,
        len: bytes.len(),
        capacity,
    }
}

fn flat_snapshot(document: &Document<'_>) -> FlatSnapshot {
    let DocumentState::Flat(record) = &document.state else {
        panic!("expected FLAT document");
    };
    let (main, extra) = record
        .plane_storage()
        .expect("promotion snapshots require plane-backed FLAT storage");
    let (origin_pointer, origin_len, origin_capacity) = origin_snapshot(&document.origin);
    FlatSnapshot {
        origin_pointer,
        origin_len,
        origin_capacity,
        logical_len: document.logical_len,
        file: document.file,
        compatibility: document.compatibility,
        trailing: document.trailing,
        dirty: document.dirty,
        next_id: document.next_id,
        image: record.image,
        main: plane_snapshot(document, main),
        extra: extra.map(|plane| plane_snapshot(document, plane)),
    }
}

fn promoted<'document, 'source>(
    document: &'document Document<'source>,
) -> (&'document ChunkSet<'source>, &'document FlatRecord<'source>) {
    let DocumentState::Chunk(chunks) = &document.state else {
        panic!("expected CHUNK document");
    };
    let record = chunks
        .promoted_flat
        .as_ref()
        .expect("expected promoted FLAT sidecar");
    assert_eq!(
        chunks
            .chunks
            .iter()
            .filter(|node| matches!(node.payload, PayloadStorage::PromotedFlat))
            .count(),
        1
    );
    (chunks, record)
}

fn promoted_id(document: &Document<'_>) -> ChunkId {
    let (chunks, _) = promoted(document);
    chunks
        .chunks
        .iter()
        .find(|node| matches!(node.payload, PayloadStorage::PromotedFlat))
        .unwrap()
        .id
}

fn refresh_flat_crc(source: &mut [u8]) {
    let checksum = crc32(&source[..24]);
    source[24..28].copy_from_slice(&checksum.to_le_bytes());
}

#[test]
fn borrowed_source_promotion_keeps_plane_pointer_and_chunk_noop_is_exact() {
    let source = a8_source();
    let main_pointer = source[FLAT_HEADER_LEN..].as_ptr();
    let mut document = Document::open(&source).unwrap();

    let image_id = document.promote_to_chunk().unwrap().unwrap();
    assert_eq!(image_id, ChunkId::new(0));
    assert_eq!(document.layout(), Layout::Chunk);
    assert_eq!(document.primary(), Some(image_id));
    assert_eq!(document.next_id, 1);
    assert!(document.is_dirty());
    let (chunks, record) = promoted(&document);
    assert_eq!(chunks.primary_hints, PrimaryHintState::Derived);
    assert_eq!(
        plane_snapshot(&document, record.main_storage()).pointer,
        main_pointer as usize
    );
    assert_eq!(
        document.primary_hints(),
        PrimaryHints::new(
            crate::image::SampleLayout::from_color_format(ColorFormat::A8),
            2,
            2,
            2
        )
    );

    let view = document.get(image_id).unwrap();
    assert_eq!(view.payload_origin(), PayloadOrigin::SYNTHESIZED);
    assert_eq!(view.payload_bytes(), None);

    let before_vector = chunks.chunks.as_ptr();
    let before_capacity = chunks.chunks.capacity();
    let before_plane = plane_snapshot(&document, record.main_storage());
    document.dirty = false;
    document.next_id = u32::MAX;
    assert_eq!(document.promote_to_chunk(), Ok(None));
    let (chunks, record) = promoted(&document);
    assert_eq!(chunks.chunks.as_ptr(), before_vector);
    assert_eq!(chunks.chunks.capacity(), before_capacity);
    assert_eq!(
        plane_snapshot(&document, record.main_storage()),
        before_plane
    );
    assert_eq!(document.next_id, u32::MAX);
    assert!(!document.is_dirty());
}

#[test]
fn borrowed_and_owned_assets_keep_both_plane_allocations() {
    let format = ColorFormat::RGB565A8;
    let width = 2;
    let height = 2;
    let stride = format.minimum_stride(width).unwrap();
    let extra_len = usize::try_from(format.extra_size(width, height, stride).unwrap()).unwrap();

    let borrowed_main = [1; 8];
    let borrowed_extra = [2; 4];
    assert_eq!(borrowed_extra.len(), extra_len);
    let mut borrowed = Document::new_flat(
        ImageAsset::new(width, height, format, stride, Cow::Borrowed(&borrowed_main))
            .with_extra(Cow::Borrowed(&borrowed_extra)),
    )
    .unwrap();
    borrowed.promote_to_chunk().unwrap();
    let (_, record) = promoted(&borrowed);
    assert_eq!(
        plane_snapshot(&borrowed, record.main_storage()).pointer,
        borrowed_main.as_ptr() as usize
    );
    assert_eq!(
        plane_snapshot(&borrowed, record.extra_storage().unwrap()).pointer,
        borrowed_extra.as_ptr() as usize
    );

    let owned_main = vec![3; 8];
    let owned_extra = vec![4; extra_len];
    let main_pointer = owned_main.as_ptr();
    let main_capacity = owned_main.capacity();
    let extra_pointer = owned_extra.as_ptr();
    let extra_capacity = owned_extra.capacity();
    let mut owned = Document::new_flat(
        ImageAsset::new(width, height, format, stride, Cow::Owned(owned_main))
            .with_extra(Cow::Owned(owned_extra)),
    )
    .unwrap();
    owned.promote_to_chunk().unwrap();
    let (_, record) = promoted(&owned);
    let main = plane_snapshot(&owned, record.main_storage());
    let extra = plane_snapshot(&owned, record.extra_storage().unwrap());
    assert_eq!(
        (main.pointer, main.capacity),
        (main_pointer as usize, Some(main_capacity))
    );
    assert_eq!(
        (extra.pointer, extra.capacity),
        (extra_pointer as usize, Some(extra_capacity))
    );

    let replacement_main = vec![5; 8];
    let replacement_extra = vec![6; extra_len];
    let replacement_main_pointer = replacement_main.as_ptr();
    let replacement_main_capacity = replacement_main.capacity();
    let replacement_extra_pointer = replacement_extra.as_ptr();
    let replacement_extra_capacity = replacement_extra.capacity();
    let mut replaced = Document::new_flat(
        ImageAsset::new(width, height, format, stride, Cow::Borrowed(&borrowed_main))
            .with_extra(Cow::Borrowed(&borrowed_extra)),
    )
    .unwrap();
    replaced
        .replace_flat_image(
            ImageAsset::new(width, height, format, stride, Cow::Owned(replacement_main))
                .with_extra(Cow::Owned(replacement_extra)),
        )
        .unwrap();
    replaced.promote_to_chunk().unwrap();
    let (_, record) = promoted(&replaced);
    let main = plane_snapshot(&replaced, record.main_storage());
    let extra = plane_snapshot(&replaced, record.extra_storage().unwrap());
    assert_eq!(
        (main.pointer, main.capacity),
        (
            replacement_main_pointer as usize,
            Some(replacement_main_capacity)
        )
    );
    assert_eq!(
        (extra.pointer, extra.capacity),
        (
            replacement_extra_pointer as usize,
            Some(replacement_extra_capacity)
        )
    );
}

#[test]
fn opened_borrowed_and_owned_sources_keep_main_extra_and_origin_allocations() {
    let format = ColorFormat::RGB565A8;
    let width = 2;
    let height = 2;
    let stride = format.minimum_stride(width).unwrap();
    let main = [1; 8];
    let extra = [2; 4];
    let source = encode_flat(&FlatImageInput {
        width,
        height,
        stride,
        format,
        main: &main,
        extra: Some(&extra),
    });
    let main_pointer = source[FLAT_HEADER_LEN..].as_ptr();
    let extra_pointer = source[FLAT_HEADER_LEN + main.len()..].as_ptr();

    let mut borrowed = Document::open(&source).unwrap();
    borrowed.promote_to_chunk().unwrap();
    let (_, record) = promoted(&borrowed);
    assert_eq!(
        plane_snapshot(&borrowed, record.main_storage()).pointer,
        main_pointer as usize
    );
    assert_eq!(
        plane_snapshot(&borrowed, record.extra_storage().unwrap()).pointer,
        extra_pointer as usize
    );

    let owned_source = source.clone();
    let origin_pointer = owned_source.as_ptr();
    let origin_capacity = owned_source.capacity();
    let main_pointer = owned_source[FLAT_HEADER_LEN..].as_ptr();
    let extra_pointer = owned_source[FLAT_HEADER_LEN + main.len()..].as_ptr();
    let mut owned = Document::from_vec(owned_source).unwrap();
    owned.promote_to_chunk().unwrap();
    let Origin::Owned(origin) = &owned.origin else {
        panic!("expected owned origin");
    };
    assert_eq!(origin.as_ptr(), origin_pointer);
    assert_eq!(origin.capacity(), origin_capacity);
    let (_, record) = promoted(&owned);
    assert_eq!(
        plane_snapshot(&owned, record.main_storage()).pointer,
        main_pointer as usize
    );
    assert_eq!(
        plane_snapshot(&owned, record.extra_storage().unwrap()).pointer,
        extra_pointer as usize
    );
}

#[test]
fn synthesized_query_materializes_one_canonical_image_payload_atomically() {
    let source = a8_source();
    let mut document = Document::open(&source).unwrap();
    let image_id = document.promote_to_chunk().unwrap().unwrap();
    let view = document.get(image_id).unwrap();

    assert_eq!(view.payload_len(), Ok(96));
    let mut short = [0xa5; 95];
    assert_eq!(
        view.copy_payload_into(&mut short),
        Err(EncodeError::BufferTooSmall {
            needed: 96,
            available: 95,
        })
    );
    assert_eq!(short, [0xa5; 95]);

    let mut target = [0xcc; 100];
    assert_eq!(view.copy_payload_into(&mut target), Ok(96));
    assert_eq!(&target[96..], &[0xcc; 4]);
    let materialized = view.payload_to_vec().unwrap();
    assert_eq!(materialized, target[..96]);
    assert_eq!(materialized[0], 1);
    assert_eq!(&materialized[28..32], &[0; 4]);
    let image = ImageView::open_payload_at(&materialized, 0).unwrap();
    assert_eq!(image.main(), &[1, 2, 3, 4]);
    assert_eq!(image.extra(), None);
}

#[test]
fn exact_raw_replacement_keeps_sidecar_but_other_encoding_clears_it() {
    let source = a8_source();
    let mut document = Document::open(&source).unwrap();
    let image_id = document.promote_to_chunk().unwrap().unwrap();
    let canonical = document.get(image_id).unwrap().payload_to_vec().unwrap();
    let (_, record) = promoted(&document);
    let before_plane = plane_snapshot(&document, record.main_storage());

    document.dirty = false;
    document
        .replace_raw(
            image_id,
            PayloadInput::Owned(canonical.clone()),
            RawChunkPolicy::infer(),
        )
        .unwrap();
    assert!(!document.is_dirty());
    let (_, record) = promoted(&document);
    assert_eq!(
        plane_snapshot(&document, record.main_storage()),
        before_plane
    );

    let offset_36 = crate::image::test_support::pad_data(canonical, 4, 0);
    ImageView::open_payload_at(&offset_36, 0).unwrap();
    document
        .replace_raw(
            image_id,
            PayloadInput::Borrowed(&offset_36),
            RawChunkPolicy::infer(),
        )
        .unwrap();
    assert!(document.is_dirty());
    let DocumentState::Chunk(chunks) = &document.state else {
        panic!("expected CHUNK document");
    };
    assert_eq!(chunks.promoted_flat, None);
    assert!(matches!(
        chunks.chunks[0].payload,
        PayloadStorage::Borrowed(_)
    ));
    assert_eq!(
        document.get(image_id).unwrap().payload_bytes(),
        Some(offset_36.as_slice())
    );
}

#[test]
fn promoted_removal_materializes_before_commit_and_clears_sidecar() {
    let source = a8_source();
    let mut document = Document::open(&source).unwrap();
    let image_id = document.promote_to_chunk().unwrap().unwrap();
    let canonical = document.get(image_id).unwrap().payload_to_vec().unwrap();
    document.dirty = false;
    let before_next = document.next_id;
    let (chunks, record) = promoted(&document);
    let before_vector = chunks.chunks.as_ptr();
    let before_plane = plane_snapshot(&document, record.main_storage());

    assert_eq!(
        document.remove_to_vec_with(image_id, |payload| {
            assert!(matches!(payload, ResolvedNodePayload::PromotedImage(_)));
            Err(EditError::AllocationFailed)
        }),
        Err(EditError::AllocationFailed)
    );
    let (chunks, record) = promoted(&document);
    assert_eq!(chunks.chunks.as_ptr(), before_vector);
    assert_eq!(
        plane_snapshot(&document, record.main_storage()),
        before_plane
    );
    assert_eq!(document.next_id, before_next);
    assert!(!document.is_dirty());

    assert_eq!(document.remove_to_vec(image_id).unwrap(), canonical);
    let DocumentState::Chunk(chunks) = &document.state else {
        panic!("expected CHUNK document");
    };
    assert!(chunks.chunks.is_empty());
    assert_eq!(chunks.promoted_flat, None);
    assert_eq!(chunks.primary, None);

    let mut without_materialization = Document::open(&source).unwrap();
    let image_id = without_materialization.promote_to_chunk().unwrap().unwrap();
    let removed = without_materialization.remove(image_id).unwrap();
    assert_eq!(removed.id, image_id);
    assert!(removed.was_primary);
    let DocumentState::Chunk(chunks) = &without_materialization.state else {
        panic!("expected CHUNK document");
    };
    assert!(chunks.chunks.is_empty());
    assert_eq!(chunks.promoted_flat, None);
}

#[test]
fn promotion_failures_keep_flat_storage_and_global_blocker_priority() {
    let source = a8_source();
    let mut exhausted = Document::open(&source).unwrap();
    exhausted.next_id = u32::MAX;
    let before = flat_snapshot(&exhausted);
    assert_eq!(
        exhausted.promote_to_chunk(),
        Err(EditError::ChunkIdExhausted)
    );
    assert_eq!(flat_snapshot(&exhausted), before);

    exhausted.next_id = 0;
    let before = flat_snapshot(&exhausted);
    assert_eq!(
        exhausted.promote_to_chunk_with_reserve(|_, additional| {
            assert_eq!(additional, 1);
            Err(EditError::AllocationFailed)
        }),
        Err(EditError::AllocationFailed)
    );
    assert_eq!(flat_snapshot(&exhausted), before);

    let mut future_source = source.clone();
    future_source[5] = VERSION_MINOR + 1;
    refresh_flat_crc(&mut future_source);
    future_source.extend_from_slice(b"tail");
    let options = OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
    let mut future = Document::open_with(&future_source, &options).unwrap();
    assert_eq!(
        future.promote_to_chunk(),
        Err(EditError::FutureSemanticsReadOnly)
    );

    let mut future_chunk = encode_chunks(&[(ChunkType::META.raw(), 0, b"meta")]);
    future_chunk[5] = VERSION_MINOR + 1;
    let checksum = crc32(&future_chunk[..40]);
    future_chunk[40..44].copy_from_slice(&checksum.to_le_bytes());
    future_chunk.extend_from_slice(b"tail");
    let mut future_with_trailing = Document::open_with(&future_chunk, &options).unwrap();
    assert_eq!(future_with_trailing.trailing, TrailingState::Preserved);
    assert_eq!(
        future_with_trailing.promote_to_chunk(),
        Err(EditError::FutureSemanticsReadOnly)
    );

    let mut trailing_source = source.clone();
    trailing_source.extend_from_slice(b"tail");
    let mut trailing = Document::open_with(&trailing_source, &options).unwrap();
    assert_eq!(
        trailing.promote_to_chunk(),
        Err(EditError::PreservedTrailingBytesReadOnly)
    );
}

#[test]
fn flat_append_pair_plans_ids_reserves_once_and_keeps_positional_edits_explicit() {
    let source = a8_source();
    let source_main_pointer = source[FLAT_HEADER_LEN..].as_ptr() as usize;
    let mut exhausted = Document::open(&source).unwrap();
    exhausted.next_id = u32::MAX - 1;
    let before = flat_snapshot(&exhausted);
    assert_eq!(
        exhausted.push_raw(RawChunkInput {
            chunk_type: CUSTOM,
            flags: ChunkFlags::NONE,
            payload: PayloadInput::Borrowed(b"opaque"),
            policy: RawChunkPolicy::infer(),
        }),
        Err(EditError::ChunkIdExhausted)
    );
    assert_eq!(flat_snapshot(&exhausted), before);

    let mut boundary = Document::open(&source).unwrap();
    boundary.next_id = u32::MAX - 2;
    let added = boundary
        .push_raw(raw(CUSTOM, PayloadInput::Borrowed(b"boundary")))
        .unwrap();
    assert_eq!(promoted_id(&boundary), ChunkId::new(u32::MAX - 2));
    assert_eq!(added, ChunkId::new(u32::MAX - 1));
    assert_eq!(boundary.next_id, u32::MAX);

    let mut invalid = Document::open(&source).unwrap();
    let before = flat_snapshot(&invalid);
    assert_eq!(
        invalid.insert_raw_at_end_with(
            RawChunkInput {
                chunk_type: CUSTOM,
                flags: ChunkFlags::NONE,
                payload: PayloadInput::Borrowed(b"opaque"),
                policy: RawChunkPolicy::infer(),
            },
            |_, _| panic!("payload policy must be checked before reserve"),
        ),
        Err(EditError::RelocationAssumptionRequired { chunk_type: CUSTOM })
    );
    assert_eq!(flat_snapshot(&invalid), before);

    let mut allocation = Document::open(&source).unwrap();
    let before = flat_snapshot(&allocation);
    assert_eq!(
        allocation.insert_raw_at_end_with(
            raw(CUSTOM, PayloadInput::Borrowed(b"opaque")),
            |_, n| {
                assert_eq!(n, 2);
                Err(EditError::AllocationFailed)
            }
        ),
        Err(EditError::AllocationFailed)
    );
    assert_eq!(flat_snapshot(&allocation), before);

    let mut appended = Document::open(&source).unwrap();
    let borrowed_payload = b"opaque";
    let borrowed_pointer = borrowed_payload.as_ptr();
    let reserve_calls = Cell::new(0);
    let added = appended
        .insert_raw_at_end_with(
            raw(CUSTOM, PayloadInput::Borrowed(borrowed_payload)),
            |nodes, n| {
                reserve_calls.set(reserve_calls.get() + 1);
                assert_eq!(n, 2);
                nodes
                    .try_reserve_exact(n)
                    .map_err(|_| EditError::AllocationFailed)
            },
        )
        .unwrap();
    assert_eq!(reserve_calls.get(), 1);
    assert_eq!(added, ChunkId::new(1));
    assert_eq!(promoted_id(&appended), ChunkId::new(0));
    assert_eq!(appended.next_id, 2);
    let (_, record) = promoted(&appended);
    assert_eq!(
        plane_snapshot(&appended, record.main_storage()).pointer,
        source_main_pointer
    );
    assert_eq!(
        appended
            .get(added)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr(),
        borrowed_pointer
    );

    let owned_payload = vec![9, 8, 7];
    let owned_pointer = owned_payload.as_ptr();
    let owned_capacity = owned_payload.capacity();
    let mut appended_owned = Document::open(&source).unwrap();
    let owned_id = appended_owned
        .push_raw(raw(CUSTOM, PayloadInput::Owned(owned_payload)))
        .unwrap();
    assert_eq!(
        appended_owned
            .get(owned_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr(),
        owned_pointer
    );
    let DocumentState::Chunk(chunks) = &appended_owned.state else {
        panic!("expected CHUNK document");
    };
    let PayloadStorage::Owned(stored) = &chunks.chunks[1].payload else {
        panic!("expected owned appended payload");
    };
    assert_eq!(stored.capacity(), owned_capacity);

    let mut positional = Document::open(&source).unwrap();
    let before = flat_snapshot(&positional);
    assert_eq!(
        positional.insert_raw_before(
            ChunkId::new(0),
            raw(CUSTOM, PayloadInput::Borrowed(b"opaque")),
        ),
        Err(EditError::ChunkLayoutRequired)
    );
    assert_eq!(flat_snapshot(&positional), before);
}

#[test]
fn primary_descriptor_and_reorder_operations_keep_the_tag_sidecar_pair() {
    let source = a8_source();
    let mut document = Document::open(&source).unwrap();
    let image_id = document.promote_to_chunk().unwrap().unwrap();
    let meta_id = document
        .push_raw(raw(ChunkType::META, PayloadInput::Borrowed(b"meta")))
        .unwrap();

    document.move_after(image_id, meta_id).unwrap();
    assert_eq!(
        document
            .chunks()
            .map(|chunk| chunk.id())
            .collect::<Vec<_>>(),
        [meta_id, image_id]
    );
    assert_eq!(promoted_id(&document), image_id);

    document.clear_primary().unwrap();
    assert_eq!(document.primary(), None);
    document.set_primary(image_id).unwrap();
    assert_eq!(document.primary(), Some(image_id));
    assert_eq!(
        document.primary_hints(),
        PrimaryHints::new(
            crate::image::SampleLayout::from_color_format(ColorFormat::A8),
            2,
            2,
            2
        )
    );
    document
        .set_flags(image_id, ChunkFlags::CRITICAL, RawChunkPolicy::infer())
        .unwrap();
    assert_eq!(promoted_id(&document), image_id);
    document
        .set_raw_policy(image_id, RawChunkPolicy::infer())
        .unwrap();
    assert_eq!(promoted_id(&document), image_id);
    document
        .set_flags(image_id, ChunkFlags::NONE, RawChunkPolicy::infer())
        .unwrap();

    document
        .set_type(image_id, ChunkType::FONT, explicit_policy())
        .unwrap();
    assert_eq!(promoted_id(&document), image_id);
    assert_eq!(
        document.primary_hints(),
        PrimaryHints::new(crate::image::SampleLayout::NONE, 0, 0, 0)
    );
    document
        .set_type(image_id, CUSTOM, explicit_policy())
        .unwrap();
    assert_eq!(document.primary_hints(), PrimaryHints::ZERO);
    assert_eq!(
        document.set_primary(image_id),
        Err(EditError::PrimaryHintsRequired { chunk_type: CUSTOM })
    );
    document
        .set_primary_with_hints(image_id, CUSTOM_HINTS)
        .unwrap();
    let custom_payload = document.get(image_id).unwrap().payload_to_vec().unwrap();
    document
        .set_flags(image_id, ChunkFlags::CRITICAL, explicit_policy())
        .unwrap();
    let custom_encoded = document.encode(&EncodeOptions::new()).unwrap();
    let structural = Reader::open_structural_with(&custom_encoded, &ReadOptions::new()).unwrap();
    let custom = structural
        .chunks()
        .find(|chunk| chunk.chunk_type() == CUSTOM)
        .unwrap();
    assert!(custom.flags().is_critical());
    assert_eq!(custom.payload(), custom_payload);
    assert_eq!(structural.primary_hints(), CUSTOM_HINTS);
    document
        .set_type(image_id, ChunkType::IMAGE, RawChunkPolicy::infer())
        .unwrap();
    document
        .set_flags(image_id, ChunkFlags::NONE, RawChunkPolicy::infer())
        .unwrap();
    assert_eq!(promoted_id(&document), image_id);
    assert_eq!(
        document.primary_hints(),
        PrimaryHints::new(
            crate::image::SampleLayout::from_color_format(ColorFormat::A8),
            2,
            2,
            2
        )
    );

    let promoted_payload = document.get(image_id).unwrap().payload_to_vec().unwrap();
    let mut second_payload = promoted_payload.clone();
    let start = crate::image::test_support::data_offset(&second_payload);
    second_payload[start..start + 4].copy_from_slice(&[9, 8, 7, 6]);
    crate::image::test_support::refresh_crc(&mut second_payload);
    let second = document
        .push_raw(RawChunkInput {
            chunk_type: ChunkType::IMAGE,
            flags: ChunkFlags::NONE,
            payload: PayloadInput::Borrowed(&second_payload),
            policy: RawChunkPolicy::infer(),
        })
        .unwrap();
    document.set_primary(second).unwrap();
    assert_eq!(document.primary(), Some(second));
    assert_eq!(
        document
            .chunks()
            .filter(|chunk| chunk.chunk_type() == ChunkType::IMAGE)
            .map(|chunk| chunk.id())
            .collect::<Vec<_>>(),
        [second, image_id]
    );
    let second_primary = document.encode(&EncodeOptions::new()).unwrap();
    let second_primary = Reader::open(&second_primary).unwrap();
    assert_eq!(second_primary.primary().unwrap().unwrap().index(), 1);
    let second_primary_chunks = second_primary.chunks().collect::<Vec<_>>();
    assert_eq!(second_primary_chunks[1].payload(), second_payload);
    assert_eq!(second_primary_chunks[2].payload(), promoted_payload);
    assert_eq!(
        second_primary.primary_hints(),
        PrimaryHints::new(
            crate::image::SampleLayout::from_color_format(ColorFormat::A8),
            2,
            2,
            2
        )
    );
    document.set_primary(image_id).unwrap();
    assert_eq!(document.primary(), Some(image_id));
    assert_eq!(
        document
            .chunks()
            .filter(|chunk| chunk.chunk_type() == ChunkType::IMAGE)
            .map(|chunk| chunk.id())
            .collect::<Vec<_>>(),
        [image_id, second]
    );
    assert_eq!(promoted_id(&document), image_id);

    let encoded = document.encode(&EncodeOptions::new()).unwrap();
    let reader = Reader::open(&encoded).unwrap();
    let chunks = reader.chunks().collect::<Vec<_>>();
    assert_eq!(chunks[0].chunk_type(), ChunkType::META);
    assert_eq!(chunks[1].chunk_type(), ChunkType::IMAGE);
    assert_eq!(chunks[1].payload(), promoted_payload);
    assert_eq!(chunks[2].payload(), second_payload);
    assert_eq!(reader.primary().unwrap().unwrap().index(), 1);
}

#[test]
fn promoted_writer_policies_share_chunk_output_and_finish_uses_default() {
    let source = a8_source();
    let mut document = Document::open(&source).unwrap();
    let image_id = document.promote_to_chunk().unwrap().unwrap();
    let meta_id = document
        .push_raw(raw(ChunkType::META, PayloadInput::Borrowed(b"x")))
        .unwrap();
    document.move_after(image_id, meta_id).unwrap();

    let default = document.encode(&EncodeOptions::new()).unwrap();
    for policy in [
        LayoutPolicy::SmallestRepresentable,
        LayoutPolicy::ForceChunk,
    ] {
        assert_eq!(
            document
                .encode(&EncodeOptions::new().with_layout_policy(policy))
                .unwrap(),
            default
        );
    }
    assert_eq!(
        document.encode(&EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceFlat)),
        Err(EncodeError::NotRepresentableAsFlat)
    );
    assert_eq!(
        document.encoded_len(&EncodeOptions::new()),
        Ok(default.len())
    );
    assert_eq!(document.encode(&EncodeOptions::new()).unwrap(), default);
    let mut target = vec![0xcc; default.len() + 4];
    assert_eq!(
        document.encode_into(&mut target, &EncodeOptions::new()),
        Ok(default.len())
    );
    assert_eq!(&target[..default.len()], default.as_slice());
    assert_eq!(&target[default.len()..], &[0xcc; 4]);
    let reader = Reader::open(&default).unwrap();
    assert_eq!(reader.layout(), Layout::Chunk);
    assert_eq!(reader.chunks().len(), 2);
    let chunks = reader.chunks().collect::<Vec<_>>();
    assert_eq!(chunks[0].chunk_type(), ChunkType::META);
    assert_eq!(chunks[1].chunk_type(), ChunkType::IMAGE);
    assert_eq!(reader.primary().unwrap().unwrap().index(), 1);
    let meta_end = chunks[0].payload_offset() as usize + chunks[0].payload().len();
    let image_start = chunks[1].payload_offset() as usize;
    assert!(default[meta_end..image_start].iter().all(|&byte| byte == 0));
    let data_offset = crate::image::test_support::data_offset(chunks[1].payload()) as u32;
    assert_eq!((chunks[1].payload_offset() + data_offset) % 4, 0);

    let mut finished = Document::open(&source).unwrap();
    let image_id = finished.promote_to_chunk().unwrap().unwrap();
    let meta_id = finished
        .push_raw(raw(ChunkType::META, PayloadInput::Borrowed(b"x")))
        .unwrap();
    finished.move_after(image_id, meta_id).unwrap();
    assert_eq!(finished.finish().unwrap().into_owned(), default);
}
