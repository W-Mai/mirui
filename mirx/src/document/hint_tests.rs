use alloc::borrow::Cow;
use alloc::vec;
use alloc::vec::Vec;

use super::*;
use crate::header::{CHUNK_FILE_HEADER_LEN, chunk_type};
use crate::{
    AnimationFrames, AnimationSettings, AtlasFrames, ColorFormat, CriticalAssumption, Frame,
    FramesAsset, ImageChunkInput, PRIMARY_FORMAT_NONE, PayloadInput, RawChunkInput,
    RelocationAssumption, ReservedBitsPolicy, crc32, encode_chunk_image, encode_chunks,
};

const CUSTOM: ChunkType = match ChunkType::new(0xbeef) {
    Some(chunk_type) => chunk_type,
    None => panic!("nonzero chunk type"),
};
const OTHER_CUSTOM: ChunkType = match ChunkType::new(0xcafe) {
    Some(chunk_type) => chunk_type,
    None => panic!("nonzero chunk type"),
};
const WIRE_HINTS: PrimaryHints = PrimaryHints::new(0xa5, 13, 21, 55);
const OTHER_HINTS: PrimaryHints = PrimaryHints::new(0x5a, 34, 12, 68);
const NON_IMAGE_HINTS: PrimaryHints = PrimaryHints::new(PRIMARY_FORMAT_NONE, 0, 0, 0);
const SUGGESTED_HINTS: PrimaryHints = PrimaryHints::new(PRIMARY_FORMAT_NONE, 20, 30, 0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeSnapshot {
    id: ChunkId,
    chunk_type: ChunkType,
    flags: ChunkFlags,
    capability: RewriteCapability,
    storage_kind: u8,
    payload_pointer: usize,
    payload_len: usize,
    owned_capacity: Option<usize>,
}

#[derive(Debug, Eq, PartialEq)]
struct DocumentSnapshot {
    logical_len: usize,
    file: FileMeta,
    compatibility: Compatibility,
    trailing: TrailingState,
    dirty: bool,
    next_id: u32,
    primary: Option<ChunkId>,
    primary_hint_state: PrimaryHintState,
    public_hints: PrimaryHints,
    origin_kind: u8,
    origin_pointer: usize,
    origin_len: usize,
    origin_capacity: Option<usize>,
    vector_pointer: usize,
    vector_capacity: usize,
    nodes: Vec<NodeSnapshot>,
}

const fn explicit_policy() -> RawChunkPolicy {
    RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
        reserved_flag_bits: ReservedBitsPolicy::Reject,
    }
}

fn image_payload(width: u32, height: u32, fill: u8) -> Vec<u8> {
    let format = ColorFormat::A8;
    let stride = format.minimum_stride(width).unwrap();
    let main_len = usize::try_from(stride.checked_mul(height).unwrap()).unwrap();
    let main = vec![fill; main_len];
    let encoded = encode_chunk_image(&ImageChunkInput {
        width,
        height,
        format,
        stride,
        main: &main,
        extra: None,
    });
    let entry = CHUNK_FILE_HEADER_LEN;
    let start = u32::from_le_bytes(encoded[entry + 4..entry + 8].try_into().unwrap()) as usize;
    let len = u32::from_le_bytes(encoded[entry + 8..entry + 12].try_into().unwrap()) as usize;
    encoded[start..start + len].to_vec()
}

fn image_hints(width: u32, height: u32) -> PrimaryHints {
    PrimaryHints::new(
        ColorFormat::A8.to_u8(),
        width,
        height,
        ColorFormat::A8.minimum_stride(width).unwrap(),
    )
}

fn frame(source_x: u32, duration_ticks: u32) -> Frame {
    Frame {
        source_x,
        source_y: 0,
        width: 1,
        height: 1,
        target_x: source_x,
        target_y: 0,
        duration_ticks,
    }
}

fn atlas_frames(width: u32, height: u32) -> FramesAsset<'static> {
    let format = ColorFormat::A8;
    let stride = format.minimum_stride(width).unwrap();
    let mut first = frame(0, 0);
    first.target_x = 0;
    FramesAsset::Atlas(AtlasFrames::new(
        ImageAsset::new(
            width,
            height,
            format,
            stride,
            Cow::Owned(vec![0; (stride * height) as usize]),
        ),
        vec![first],
    ))
}

fn animation_frames(canvas_width: u32, canvas_height: u32) -> FramesAsset<'static> {
    let format = ColorFormat::A8;
    let atlas_width = 2;
    let atlas_height = 1;
    let stride = format.minimum_stride(atlas_width).unwrap();
    FramesAsset::Animation(AnimationFrames::new(
        ImageAsset::new(
            atlas_width,
            atlas_height,
            format,
            stride,
            Cow::Owned(vec![1, 2]),
        ),
        vec![frame(0, 20), frame(1, 30)],
        AnimationSettings::new(canvas_width, canvas_height, 1_000, 40),
    ))
}

fn raw<'a>(
    chunk_type: ChunkType,
    payload: PayloadInput<'a>,
    policy: RawChunkPolicy,
) -> RawChunkInput<'a> {
    RawChunkInput {
        chunk_type,
        flags: ChunkFlags::NONE,
        payload,
        policy,
    }
}

fn set_wire_primary(source: &mut [u8], chunk_type: u16, hints: PrimaryHints) {
    source[20..22].copy_from_slice(&chunk_type.to_le_bytes());
    source[22] = hints.color_format_raw();
    source[24..28].copy_from_slice(&hints.width().to_le_bytes());
    source[28..32].copy_from_slice(&hints.height().to_le_bytes());
    source[32..36].copy_from_slice(&hints.stride().to_le_bytes());
    refresh_header_crc(source);
}

fn refresh_header_crc(source: &mut [u8]) {
    let checksum = crc32(&source[..40]);
    source[40..44].copy_from_slice(&checksum.to_le_bytes());
}

fn chunk_hint_state(document: &Document<'_>) -> PrimaryHintState {
    let DocumentState::Chunk(chunks) = &document.state else {
        panic!("expected CHUNK document");
    };
    chunks.primary_hints
}

fn snapshot(document: &Document<'_>) -> DocumentSnapshot {
    let (origin_kind, origin_pointer, origin_len, origin_capacity) = match &document.origin {
        Origin::New => (0, 0, 0, None),
        Origin::Borrowed(bytes) => (1, bytes.as_ptr() as usize, bytes.len(), None),
        Origin::Owned(bytes) => (
            2,
            bytes.as_ptr() as usize,
            bytes.len(),
            Some(bytes.capacity()),
        ),
    };
    let (primary, primary_hint_state, vector_pointer, vector_capacity, nodes) =
        match &document.state {
            DocumentState::Chunk(chunks) => {
                let nodes = chunks
                    .chunks
                    .iter()
                    .map(|node| {
                        let (storage_kind, payload, owned_capacity) = match &node.payload {
                            PayloadStorage::SourceRange(range) => (
                                0,
                                document
                                    .origin
                                    .resolve(*range)
                                    .expect("validated source range"),
                                None,
                            ),
                            PayloadStorage::Borrowed(bytes) => (1, *bytes, None),
                            PayloadStorage::Owned(bytes) => {
                                (2, bytes.as_slice(), Some(bytes.capacity()))
                            }
                            PayloadStorage::PromotedFlat => (3, &[] as &[u8], None),
                        };
                        NodeSnapshot {
                            id: node.id,
                            chunk_type: node.chunk_type,
                            flags: node.flags,
                            capability: node.capability,
                            storage_kind,
                            payload_pointer: payload.as_ptr() as usize,
                            payload_len: payload.len(),
                            owned_capacity,
                        }
                    })
                    .collect();
                (
                    chunks.primary,
                    chunks.primary_hints,
                    chunks.chunks.as_ptr() as usize,
                    chunks.chunks.capacity(),
                    nodes,
                )
            }
            DocumentState::Flat(_) | DocumentState::OpaqueFlat(_) => {
                (None, PrimaryHintState::Missing, 0, 0, Vec::new())
            }
        };
    DocumentSnapshot {
        logical_len: document.logical_len,
        file: document.file,
        compatibility: document.compatibility,
        trailing: document.trailing,
        dirty: document.dirty,
        next_id: document.next_id,
        primary,
        primary_hint_state,
        public_hints: document.primary_hints(),
        origin_kind,
        origin_pointer,
        origin_len,
        origin_capacity,
        vector_pointer,
        vector_capacity,
        nodes,
    }
}

#[test]
fn open_classifies_derived_default_and_opaque_hints_for_borrowed_and_owned_sources() {
    let image = image_payload(3, 2, 7);
    let image_source = encode_chunks(&[(chunk_type::IMAGE, 0, image.as_slice())]);
    let borrowed_image = Document::open(&image_source).unwrap();
    assert_eq!(chunk_hint_state(&borrowed_image), PrimaryHintState::Derived);
    assert_eq!(borrowed_image.primary_hints(), image_hints(3, 2));

    let owned_pointer = image_source.as_ptr();
    let owned_len = image_source.len();
    let owned_capacity = image_source.capacity();
    let owned_image = Document::from_vec(image_source).unwrap();
    assert_eq!(chunk_hint_state(&owned_image), PrimaryHintState::Derived);
    assert_eq!(owned_image.primary_hints(), image_hints(3, 2));
    let owned_snapshot = snapshot(&owned_image);
    assert_eq!(owned_snapshot.origin_pointer, owned_pointer as usize);
    assert_eq!(owned_snapshot.origin_len, owned_len);
    assert_eq!(owned_snapshot.origin_capacity, Some(owned_capacity));

    for chunk_type in [
        ChunkType::FONT,
        ChunkType::VECTOR,
        ChunkType::META,
        ChunkType::PALETTE,
    ] {
        let mut source = encode_chunks(&[(chunk_type.raw(), 0, b"opaque")]);
        set_wire_primary(&mut source, chunk_type.raw(), PrimaryHints::ZERO);
        let document = Document::open(&source).unwrap();
        assert_eq!(
            chunk_hint_state(&document),
            PrimaryHintState::KnownNonImageDefault
        );
        assert_eq!(document.primary_hints(), NON_IMAGE_HINTS);
    }

    let mut suggested = encode_chunks(&[(ChunkType::FONT.raw(), 0, b"font")]);
    set_wire_primary(&mut suggested, ChunkType::FONT.raw(), SUGGESTED_HINTS);
    let suggested = Document::open(&suggested).unwrap();
    assert_eq!(
        chunk_hint_state(&suggested),
        PrimaryHintState::Explicit(SUGGESTED_HINTS)
    );
    assert_eq!(suggested.primary_hints(), SUGGESTED_HINTS);

    let mut noncanonical = encode_chunks(&[(ChunkType::VECTOR.raw(), 0, b"vector")]);
    set_wire_primary(&mut noncanonical, ChunkType::VECTOR.raw(), WIRE_HINTS);
    let noncanonical = Document::open(&noncanonical).unwrap();
    assert_eq!(
        chunk_hint_state(&noncanonical),
        PrimaryHintState::KnownNonImageDefault
    );
    assert_eq!(noncanonical.primary_hints(), NON_IMAGE_HINTS);

    for (chunk_type, payload) in [
        (ChunkType::FRAMES, b"frames".as_slice()),
        (CUSTOM, b"custom".as_slice()),
        (ChunkType::IMAGE, b"malformed".as_slice()),
    ] {
        let mut source = encode_chunks(&[(chunk_type.raw(), 0, payload)]);
        set_wire_primary(&mut source, chunk_type.raw(), WIRE_HINTS);
        let document = Document::open(&source).unwrap();
        assert_eq!(
            chunk_hint_state(&document),
            PrimaryHintState::PreservedOpaque(WIRE_HINTS)
        );
        assert_eq!(document.primary_hints(), WIRE_HINTS);
    }
}

#[test]
fn frames_primary_hints_follow_atlas_and_animation_display_geometry() {
    let atlas = atlas_frames(3, 2);
    let atlas_payload = atlas.encode_payload().unwrap();
    let mut atlas_source = encode_chunks(&[(ChunkType::FRAMES.raw(), 0, &atlas_payload)]);
    set_wire_primary(&mut atlas_source, ChunkType::FRAMES.raw(), WIRE_HINTS);
    let mut atlas_document = Document::open(&atlas_source).unwrap();
    let atlas_id = atlas_document.primary().unwrap();
    let atlas_hints = PrimaryHints::new(ColorFormat::A8.to_u8(), 3, 2, 3);

    assert_eq!(chunk_hint_state(&atlas_document), PrimaryHintState::Derived);
    assert_eq!(atlas_document.primary_hints(), atlas_hints);
    assert_eq!(
        atlas_document.set_primary_with_hints(atlas_id, OTHER_HINTS),
        Err(EditError::InvalidPrimaryHints {
            chunk_type: ChunkType::FRAMES,
        })
    );

    let replacement = atlas_frames(4, 1);
    atlas_document
        .replace_frames(atlas_id, &replacement)
        .unwrap();
    assert_eq!(
        atlas_document.primary_hints(),
        PrimaryHints::new(ColorFormat::A8.to_u8(), 4, 1, 4)
    );
    let encoded = atlas_document.encode(&EncodeOptions::new()).unwrap();
    assert_eq!(
        Document::open(&encoded).unwrap().primary_hints(),
        PrimaryHints::new(ColorFormat::A8.to_u8(), 4, 1, 4)
    );

    let animation = animation_frames(5, 4);
    let animation_payload = animation.encode_payload().unwrap();
    let mut animation_source =
        encode_chunks(&[(ChunkType::FRAMES.raw(), 0, animation_payload.as_slice())]);
    set_wire_primary(
        &mut animation_source,
        ChunkType::FRAMES.raw(),
        PrimaryHints::ZERO,
    );
    let animation_document = Document::from_vec(animation_source).unwrap();
    assert_eq!(
        chunk_hint_state(&animation_document),
        PrimaryHintState::Derived
    );
    assert_eq!(
        animation_document.primary_hints(),
        PrimaryHints::new(ColorFormat::A8.to_u8(), 5, 4, 0)
    );
}

#[test]
fn valid_frames_can_be_selected_as_primary_without_explicit_hints() {
    let mut document = Document::new();
    let atlas_id = document
        .push_frames(ChunkFlags::NONE, &atlas_frames(2, 1))
        .unwrap();
    let animation_id = document
        .push_frames(ChunkFlags::NONE, &animation_frames(6, 3))
        .unwrap();

    document.clear_primary().unwrap();
    document.set_primary(animation_id).unwrap();
    assert_eq!(document.primary(), Some(animation_id));
    assert_eq!(
        document.primary_hints(),
        PrimaryHints::new(ColorFormat::A8.to_u8(), 6, 3, 0)
    );

    document.set_primary(atlas_id).unwrap();
    assert_eq!(document.primary(), Some(atlas_id));
    assert_eq!(
        document.primary_hints(),
        PrimaryHints::new(ColorFormat::A8.to_u8(), 2, 1, 2)
    );
}

#[test]
fn legacy_zero_stale_none_future_and_flat_sources_remain_unambiguous() {
    let mut custom_zero = encode_chunks(&[(CUSTOM.raw(), 0, b"custom")]);
    set_wire_primary(&mut custom_zero, CUSTOM.raw(), PrimaryHints::ZERO);
    let custom = Document::open(&custom_zero).unwrap();
    assert_eq!(
        chunk_hint_state(&custom),
        PrimaryHintState::PreservedOpaque(PrimaryHints::ZERO)
    );
    assert_eq!(custom.primary_hints(), PrimaryHints::ZERO);

    for primary_type in [0, 0xd00d] {
        let mut source = encode_chunks(&[(ChunkType::META.raw(), 0, b"meta")]);
        set_wire_primary(&mut source, primary_type, WIRE_HINTS);
        let document = Document::open(&source).unwrap();
        assert_eq!(document.primary(), None);
        assert_eq!(chunk_hint_state(&document), PrimaryHintState::Missing);
        assert_eq!(document.primary_hints(), PrimaryHints::ZERO);
    }

    let image = image_payload(2, 2, 9);
    let mut future = encode_chunks(&[(ChunkType::IMAGE.raw(), 0, image.as_slice())]);
    set_wire_primary(&mut future, ChunkType::IMAGE.raw(), WIRE_HINTS);
    future[5] = VERSION_MINOR + 1;
    refresh_header_crc(&mut future);
    let normalized_future_image = Document::open_with(
        &future,
        &OpenOptions::new().with_compatibility(CompatibilityPolicy::NormalizeToCurrent),
    )
    .unwrap();
    assert_eq!(
        chunk_hint_state(&normalized_future_image),
        PrimaryHintState::Derived
    );
    assert_eq!(normalized_future_image.primary_hints(), image_hints(2, 2));
    assert!(normalized_future_image.is_dirty());

    let future = Document::open(&future).unwrap();
    assert_eq!(future.compatibility, Compatibility::FutureReadOnly);
    assert_eq!(
        chunk_hint_state(&future),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );
    assert_eq!(future.primary_hints(), WIRE_HINTS);

    let mut future_custom = encode_chunks(&[(CUSTOM.raw(), 0, b"custom")]);
    set_wire_primary(&mut future_custom, CUSTOM.raw(), WIRE_HINTS);
    future_custom[5] = VERSION_MINOR + 1;
    refresh_header_crc(&mut future_custom);
    let normalized = Document::open_with(
        &future_custom,
        &OpenOptions::new().with_compatibility(CompatibilityPolicy::NormalizeToCurrent),
    )
    .unwrap();
    assert_eq!(normalized.compatibility, Compatibility::Current);
    assert_eq!(chunk_hint_state(&normalized), PrimaryHintState::Missing);
    assert_eq!(normalized.primary_hints(), PrimaryHints::ZERO);
    assert!(normalized.is_dirty());

    for (wire_hints, expected_state) in [
        (NON_IMAGE_HINTS, PrimaryHintState::KnownNonImageDefault),
        (SUGGESTED_HINTS, PrimaryHintState::Explicit(SUGGESTED_HINTS)),
    ] {
        let mut future_meta = encode_chunks(&[(ChunkType::META.raw(), 0, b"meta")]);
        set_wire_primary(&mut future_meta, ChunkType::META.raw(), wire_hints);
        future_meta[5] = VERSION_MINOR + 1;
        refresh_header_crc(&mut future_meta);
        let normalized = Document::open_with(
            &future_meta,
            &OpenOptions::new().with_compatibility(CompatibilityPolicy::NormalizeToCurrent),
        )
        .unwrap();
        assert_eq!(chunk_hint_state(&normalized), expected_state);
        assert_eq!(normalized.primary_hints(), wire_hints);
        assert!(normalized.is_dirty());
    }

    let flat_source = crate::encode_flat(&crate::FlatImageInput {
        width: 3,
        height: 2,
        stride: ColorFormat::A8.minimum_stride(3).unwrap(),
        format: ColorFormat::A8,
        main: &[1, 2, 3, 4, 5, 6],
        extra: None,
    });
    let flat = Document::open(&flat_source).unwrap();
    assert_eq!(flat.primary_hints(), image_hints(3, 2));

    let mut future_flat = flat_source;
    future_flat[5] = VERSION_MINOR + 1;
    future_flat[8] = WIRE_HINTS.color_format_raw();
    future_flat[12..16].copy_from_slice(&WIRE_HINTS.width().to_le_bytes());
    future_flat[16..20].copy_from_slice(&WIRE_HINTS.height().to_le_bytes());
    future_flat[20..24].copy_from_slice(&WIRE_HINTS.stride().to_le_bytes());
    let checksum = crc32(&future_flat[..24]);
    future_flat[24..28].copy_from_slice(&checksum.to_le_bytes());
    let future_flat_pointer = future_flat.as_ptr();
    let future_flat_len = future_flat.len();
    let future_flat_capacity = future_flat.capacity();
    let borrowed_future_flat = Document::open(&future_flat).unwrap();
    assert_eq!(
        borrowed_future_flat.state,
        DocumentState::OpaqueFlat(WIRE_HINTS)
    );
    assert_eq!(borrowed_future_flat.primary_hints(), WIRE_HINTS);
    assert_eq!(
        borrowed_future_flat.origin.source().unwrap().as_ptr(),
        future_flat_pointer
    );

    let owned_future_flat = Document::from_vec(future_flat).unwrap();
    assert_eq!(
        owned_future_flat.state,
        DocumentState::OpaqueFlat(WIRE_HINTS)
    );
    assert_eq!(owned_future_flat.primary_hints(), WIRE_HINTS);
    let owned_snapshot = snapshot(&owned_future_flat);
    assert_eq!(owned_snapshot.origin_pointer, future_flat_pointer as usize);
    assert_eq!(owned_snapshot.origin_len, future_flat_len);
    assert_eq!(owned_snapshot.origin_capacity, Some(future_flat_capacity));
}

#[test]
fn primary_selection_requires_or_records_hints_without_changing_allocations() {
    let image = image_payload(2, 1, 3);
    let owned_pointer = image.as_ptr();
    let owned_capacity = image.capacity();
    let mut document = Document::new();
    let image_id = document
        .push_raw(raw(
            ChunkType::IMAGE,
            PayloadInput::Owned(image),
            RawChunkPolicy::infer(),
        ))
        .unwrap();
    let meta_id = document
        .push_raw(raw(
            ChunkType::META,
            PayloadInput::Borrowed(b"meta"),
            explicit_policy(),
        ))
        .unwrap();
    let frames_id = document
        .push_raw(raw(
            ChunkType::FRAMES,
            PayloadInput::Borrowed(b"frames"),
            explicit_policy(),
        ))
        .unwrap();
    let custom_id = document
        .push_raw(raw(
            CUSTOM,
            PayloadInput::Borrowed(b"custom"),
            explicit_policy(),
        ))
        .unwrap();
    let malformed_image = document
        .push_raw(raw(
            ChunkType::IMAGE,
            PayloadInput::Borrowed(b"bad"),
            explicit_policy(),
        ))
        .unwrap();
    document.dirty = false;
    let allocations = snapshot(&document);

    document.set_primary(image_id).unwrap();
    assert_eq!(document.primary(), Some(image_id));
    assert_eq!(chunk_hint_state(&document), PrimaryHintState::Derived);
    assert_eq!(document.primary_hints(), image_hints(2, 1));
    let selected_image = snapshot(&document);
    assert_eq!(selected_image.vector_pointer, allocations.vector_pointer);
    assert_eq!(selected_image.vector_capacity, allocations.vector_capacity);
    assert_eq!(selected_image.next_id, allocations.next_id);
    assert_eq!(
        selected_image.nodes[0].payload_pointer,
        owned_pointer as usize
    );
    assert_eq!(selected_image.nodes[0].owned_capacity, Some(owned_capacity));

    document.dirty = false;
    let before_image_mismatch = snapshot(&document);
    assert_eq!(
        document.set_primary_with_hints(image_id, OTHER_HINTS),
        Err(EditError::InvalidPrimaryHints {
            chunk_type: ChunkType::IMAGE,
        })
    );
    assert_eq!(snapshot(&document), before_image_mismatch);
    document
        .set_primary_with_hints(image_id, image_hints(2, 1))
        .unwrap();
    assert_eq!(snapshot(&document), before_image_mismatch);

    document.set_primary(meta_id).unwrap();
    assert_eq!(document.primary(), Some(meta_id));
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::KnownNonImageDefault
    );
    assert_eq!(document.primary_hints(), NON_IMAGE_HINTS);

    document.dirty = false;
    document
        .set_primary_with_hints(meta_id, SUGGESTED_HINTS)
        .unwrap();
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::Explicit(SUGGESTED_HINTS)
    );
    assert_eq!(document.primary_hints(), SUGGESTED_HINTS);
    assert!(document.is_dirty());
    document.dirty = false;
    let before_invalid_non_image = snapshot(&document);
    assert_eq!(
        document.set_primary_with_hints(meta_id, WIRE_HINTS),
        Err(EditError::InvalidPrimaryHints {
            chunk_type: ChunkType::META,
        })
    );
    assert_eq!(snapshot(&document), before_invalid_non_image);

    for (id, chunk_type) in [
        (frames_id, ChunkType::FRAMES),
        (custom_id, CUSTOM),
        (malformed_image, ChunkType::IMAGE),
    ] {
        document.dirty = false;
        let before = snapshot(&document);
        assert_eq!(
            document.set_primary(id),
            Err(EditError::PrimaryHintsRequired { chunk_type })
        );
        assert_eq!(snapshot(&document), before);
    }

    document
        .set_primary_with_hints(custom_id, WIRE_HINTS)
        .unwrap();
    assert_eq!(document.primary(), Some(custom_id));
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::Explicit(WIRE_HINTS)
    );
    assert_eq!(document.primary_hints(), WIRE_HINTS);

    document.dirty = false;
    let before_noop = snapshot(&document);
    document
        .set_primary_with_hints(custom_id, WIRE_HINTS)
        .unwrap();
    assert_eq!(snapshot(&document), before_noop);
    document.set_primary(custom_id).unwrap();
    assert_eq!(snapshot(&document), before_noop);

    document
        .set_primary_with_hints(custom_id, OTHER_HINTS)
        .unwrap();
    assert!(document.is_dirty());
    assert_eq!(document.primary_hints(), OTHER_HINTS);
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::Explicit(OTHER_HINTS)
    );

    document.clear_primary().unwrap();
    assert_eq!(document.primary(), None);
    assert_eq!(chunk_hint_state(&document), PrimaryHintState::Missing);
    assert_eq!(document.primary_hints(), PrimaryHints::ZERO);
    document.dirty = false;
    let before_clear_noop = snapshot(&document);
    document.clear_primary().unwrap();
    assert_eq!(snapshot(&document), before_clear_noop);
}

#[test]
fn opaque_primary_transitions_preserve_only_untouched_wire_hints() {
    let mut source = encode_chunks(&[(CUSTOM.raw(), 0, b"source")]);
    set_wire_primary(&mut source, CUSTOM.raw(), WIRE_HINTS);
    let mut document = Document::open(&source).unwrap();
    let primary = document.primary().unwrap();
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );

    let before_preserved_noop = snapshot(&document);
    document
        .set_primary_with_hints(primary, WIRE_HINTS)
        .unwrap();
    assert_eq!(snapshot(&document), before_preserved_noop);
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );

    document.set_raw_policy(primary, explicit_policy()).unwrap();
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );
    assert!(!document.is_dirty());

    let before_exact = snapshot(&document);
    document
        .replace_raw(
            primary,
            PayloadInput::Borrowed(b"source"),
            RawChunkPolicy::infer(),
        )
        .unwrap();
    assert_eq!(snapshot(&document), before_exact);

    let before_policy_error = snapshot(&document);
    assert_eq!(
        document.replace_raw(
            primary,
            PayloadInput::Borrowed(b"changed"),
            RawChunkPolicy::infer(),
        ),
        Err(EditError::RelocationAssumptionRequired { chunk_type: CUSTOM })
    );
    assert_eq!(snapshot(&document), before_policy_error);

    document
        .replace_raw(
            primary,
            PayloadInput::Borrowed(b"changed"),
            explicit_policy(),
        )
        .unwrap();
    assert_eq!(chunk_hint_state(&document), PrimaryHintState::Missing);
    assert_eq!(document.primary_hints(), PrimaryHints::ZERO);

    document.dirty = false;
    let before_missing_error = snapshot(&document);
    assert_eq!(
        document.set_primary(primary),
        Err(EditError::PrimaryHintsRequired { chunk_type: CUSTOM })
    );
    assert_eq!(snapshot(&document), before_missing_error);

    document
        .set_primary_with_hints(primary, WIRE_HINTS)
        .unwrap();
    document
        .set_type(primary, ChunkType::META, explicit_policy())
        .unwrap();
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::KnownNonImageDefault
    );
    assert_eq!(document.primary_hints(), NON_IMAGE_HINTS);

    document
        .set_type(primary, ChunkType::FRAMES, explicit_policy())
        .unwrap();
    assert_eq!(chunk_hint_state(&document), PrimaryHintState::Missing);
    assert_eq!(document.primary_hints(), PrimaryHints::ZERO);

    document.dirty = false;
    let before_type_noop = snapshot(&document);
    document
        .set_type(primary, ChunkType::FRAMES, RawChunkPolicy::infer())
        .unwrap();
    assert_eq!(snapshot(&document), before_type_noop);

    let removed = document.remove(primary).unwrap();
    assert!(removed.was_primary);
    assert_eq!(document.primary(), None);
    assert_eq!(chunk_hint_state(&document), PrimaryHintState::Missing);
    assert_eq!(document.primary_hints(), PrimaryHints::ZERO);
}

#[test]
fn image_replacement_and_type_changes_rederive_only_valid_payload_hints() {
    let first = image_payload(2, 2, 1);
    let second = image_payload(4, 1, 2);
    let mut source = encode_chunks(&[(ChunkType::IMAGE.raw(), 0, first.as_slice())]);
    set_wire_primary(&mut source, ChunkType::IMAGE.raw(), WIRE_HINTS);
    let mut document = Document::open(&source).unwrap();
    let primary = document.primary().unwrap();
    assert_eq!(chunk_hint_state(&document), PrimaryHintState::Derived);
    assert_eq!(document.primary_hints(), image_hints(2, 2));

    document
        .set_type(primary, CUSTOM, explicit_policy())
        .unwrap();
    assert_eq!(chunk_hint_state(&document), PrimaryHintState::Missing);
    document
        .set_type(primary, ChunkType::IMAGE, RawChunkPolicy::infer())
        .unwrap();
    assert_eq!(chunk_hint_state(&document), PrimaryHintState::Derived);
    assert_eq!(document.primary_hints(), image_hints(2, 2));

    document
        .replace_raw(
            primary,
            PayloadInput::Borrowed(&second),
            RawChunkPolicy::infer(),
        )
        .unwrap();
    assert_eq!(chunk_hint_state(&document), PrimaryHintState::Derived);
    assert_eq!(document.primary_hints(), image_hints(4, 1));

    document
        .replace_raw(
            primary,
            PayloadInput::Borrowed(b"malformed"),
            explicit_policy(),
        )
        .unwrap();
    assert_eq!(chunk_hint_state(&document), PrimaryHintState::Missing);
    assert_eq!(document.primary_hints(), PrimaryHints::ZERO);
}

#[test]
fn primary_flag_changes_preserve_explicit_and_opaque_hints_atomically() {
    let reserved = 0x8000;
    let mut source = encode_chunks(&[(CUSTOM.raw(), reserved, b"custom")]);
    set_wire_primary(&mut source, CUSTOM.raw(), WIRE_HINTS);

    let normalize_policy = RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
        reserved_flag_bits: ReservedBitsPolicy::Normalize,
    };
    let open_policies = [RawTypePolicy {
        chunk_type: CUSTOM,
        policy: normalize_policy,
    }];
    let normalized_open = Document::open_with(
        &source,
        &OpenOptions::new().with_raw_type_policies(&open_policies),
    )
    .unwrap();
    assert_eq!(
        chunk_hint_state(&normalized_open),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );
    assert_eq!(normalized_open.primary_hints(), WIRE_HINTS);
    assert_eq!(
        normalized_open
            .get(normalized_open.primary().unwrap())
            .unwrap()
            .flags(),
        ChunkFlags::NONE
    );
    assert!(normalized_open.is_dirty());

    let mut document = Document::open(&source).unwrap();
    let primary = document.primary().unwrap();
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );

    let failing_policy = RawChunkPolicy {
        relocation: RelocationAssumption::Infer,
        critical_semantics: CriticalAssumption::Infer,
        reserved_flag_bits: ReservedBitsPolicy::Normalize,
    };
    let before = snapshot(&document);
    assert_eq!(
        document.set_raw_policy(primary, failing_policy),
        Err(EditError::RelocationAssumptionRequired { chunk_type: CUSTOM })
    );
    assert_eq!(snapshot(&document), before);

    document.set_raw_policy(primary, normalize_policy).unwrap();
    assert_eq!(document.get(primary).unwrap().flags(), ChunkFlags::NONE);
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );
    assert_eq!(document.primary_hints(), WIRE_HINTS);
    assert!(document.is_dirty());

    document
        .set_flags(primary, ChunkFlags::CRITICAL, explicit_policy())
        .unwrap();
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );
    assert_eq!(document.primary_hints(), WIRE_HINTS);

    document
        .set_primary_with_hints(primary, OTHER_HINTS)
        .unwrap();
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::Explicit(OTHER_HINTS)
    );
    document
        .set_flags(primary, ChunkFlags::NONE, explicit_policy())
        .unwrap();
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::Explicit(OTHER_HINTS)
    );
    assert_eq!(document.primary_hints(), OTHER_HINTS);
}

#[test]
fn non_primary_edits_do_not_change_primary_hint_state() {
    let mut source = encode_chunks(&[
        (CUSTOM.raw(), 0, b"primary"),
        (ChunkType::META.raw(), 0, b"secondary"),
    ]);
    set_wire_primary(&mut source, CUSTOM.raw(), WIRE_HINTS);
    let mut document = Document::open(&source).unwrap();
    let primary = document.primary().unwrap();
    let secondary = document.chunks().nth(1).unwrap().id();

    document
        .replace_raw(
            secondary,
            PayloadInput::Borrowed(b"replacement"),
            explicit_policy(),
        )
        .unwrap();
    assert_eq!(document.primary(), Some(primary));
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );
    assert_eq!(document.primary_hints(), WIRE_HINTS);

    document
        .set_type(secondary, OTHER_CUSTOM, explicit_policy())
        .unwrap();
    assert_eq!(document.primary(), Some(primary));
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );
    assert_eq!(document.primary_hints(), WIRE_HINTS);

    let preserve_flags = RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
        reserved_flag_bits: ReservedBitsPolicy::Preserve,
    };
    document
        .set_flags(
            secondary,
            ChunkFlags::from_bits_retain(0x8001),
            preserve_flags,
        )
        .unwrap();
    assert_eq!(document.primary(), Some(primary));
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );
    assert_eq!(document.primary_hints(), WIRE_HINTS);

    let normalize_flags = RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
        reserved_flag_bits: ReservedBitsPolicy::Normalize,
    };
    document.set_raw_policy(secondary, normalize_flags).unwrap();
    assert_eq!(document.primary(), Some(primary));
    assert_eq!(
        chunk_hint_state(&document),
        PrimaryHintState::PreservedOpaque(WIRE_HINTS)
    );
    assert_eq!(document.primary_hints(), WIRE_HINTS);
}
