use alloc::vec;
use alloc::vec::Vec;

use super::*;
use crate::header::CHUNK_FILE_HEADER_LEN;
use crate::{
    ColorFormat, CriticalAssumption, FlatImageInput, ImageChunkInput, PayloadInput, RawChunkInput,
    RelocationAssumption, ReservedBitsPolicy, TrailingBytesPolicy, crc32, encode_chunk_image,
    encode_chunks, encode_flat,
};

const TYPE_A: ChunkType = match ChunkType::new(0xa001) {
    Some(chunk_type) => chunk_type,
    None => panic!("nonzero chunk type"),
};
const TYPE_B: ChunkType = match ChunkType::new(0xb001) {
    Some(chunk_type) => chunk_type,
    None => panic!("nonzero chunk type"),
};
const TYPE_C: ChunkType = match ChunkType::new(0xc001) {
    Some(chunk_type) => chunk_type,
    None => panic!("nonzero chunk type"),
};
const WIRE_HINTS: PrimaryHints = PrimaryHints::new(0xa5, 13, 21, 55);
const BORROWED_PAYLOAD: &[u8] = b"borrowed-payload";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OriginKind {
    New,
    Borrowed,
    Owned,
}

#[derive(Debug, Eq, PartialEq)]
struct OriginSnapshot {
    kind: OriginKind,
    pointer: Option<usize>,
    len: usize,
    capacity: Option<usize>,
    bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StorageKind {
    SourceRange(SourceRange),
    Borrowed,
    Owned,
}

#[derive(Debug, Eq, PartialEq)]
struct StorageSnapshot {
    kind: StorageKind,
    pointer: usize,
    len: usize,
    capacity: Option<usize>,
    bytes: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
struct NodeSnapshot {
    id: ChunkId,
    chunk_type: ChunkType,
    flags: ChunkFlags,
    capability: RewriteCapability,
    payload: StorageSnapshot,
}

#[derive(Debug, Eq, PartialEq)]
enum StateSnapshot {
    SourceFlat {
        record: FlatRecord,
        main: Vec<u8>,
        extra: Option<Vec<u8>>,
    },
    OpaqueFlat {
        hints: PrimaryHints,
    },
    Chunk {
        vector_pointer: usize,
        vector_capacity: usize,
        primary: Option<ChunkId>,
        primary_hints: PrimaryHintState,
        nodes: Vec<NodeSnapshot>,
    },
}

#[derive(Debug, Eq, PartialEq)]
struct DocumentSnapshot {
    origin: OriginSnapshot,
    logical_len: usize,
    file: FileMeta,
    layout: Layout,
    compatibility: Compatibility,
    trailing: TrailingState,
    dirty: bool,
    next_id: u32,
    public_primary_hints: PrimaryHints,
    state: StateSnapshot,
}

const fn explicit_policy() -> RawChunkPolicy {
    RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
        reserved_flag_bits: ReservedBitsPolicy::Reject,
    }
}

const fn relocation_only_policy() -> RawChunkPolicy {
    RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::Infer,
        reserved_flag_bits: ReservedBitsPolicy::Reject,
    }
}

fn raw<'a>(
    chunk_type: ChunkType,
    flags: ChunkFlags,
    payload: PayloadInput<'a>,
    policy: RawChunkPolicy,
) -> RawChunkInput<'a> {
    RawChunkInput {
        chunk_type,
        flags,
        payload,
        policy,
    }
}

fn id(counter: u32) -> ChunkId {
    ChunkId::from_session_counter(counter)
}

fn origin_snapshot(origin: &Origin<'_>) -> OriginSnapshot {
    match origin {
        Origin::New => OriginSnapshot {
            kind: OriginKind::New,
            pointer: None,
            len: 0,
            capacity: None,
            bytes: Vec::new(),
        },
        Origin::Borrowed(bytes) => OriginSnapshot {
            kind: OriginKind::Borrowed,
            pointer: Some(bytes.as_ptr() as usize),
            len: bytes.len(),
            capacity: None,
            bytes: bytes.to_vec(),
        },
        Origin::Owned(bytes) => OriginSnapshot {
            kind: OriginKind::Owned,
            pointer: Some(bytes.as_ptr() as usize),
            len: bytes.len(),
            capacity: Some(bytes.capacity()),
            bytes: bytes.clone(),
        },
    }
}

fn storage_snapshot(document: &Document<'_>, payload: &PayloadStorage<'_>) -> StorageSnapshot {
    let (kind, bytes, capacity) = match payload {
        PayloadStorage::SourceRange(range) => (
            StorageKind::SourceRange(*range),
            document
                .origin
                .resolve(*range)
                .expect("validated source range must remain resolvable"),
            None,
        ),
        PayloadStorage::Borrowed(bytes) => (StorageKind::Borrowed, *bytes, None),
        PayloadStorage::Owned(bytes) => {
            (StorageKind::Owned, bytes.as_slice(), Some(bytes.capacity()))
        }
    };
    StorageSnapshot {
        kind,
        pointer: bytes.as_ptr() as usize,
        len: bytes.len(),
        capacity,
        bytes: bytes.to_vec(),
    }
}

fn snapshot(document: &Document<'_>) -> DocumentSnapshot {
    let state = match &document.state {
        DocumentState::SourceFlat(record) => {
            let image = document
                .flat_image()
                .expect("current FLAT state must expose validated planes");
            StateSnapshot::SourceFlat {
                record: *record,
                main: image.main().to_vec(),
                extra: image.extra().map(<[u8]>::to_vec),
            }
        }
        DocumentState::OpaqueFlat(hints) => StateSnapshot::OpaqueFlat { hints: *hints },
        DocumentState::Chunk(chunks) => StateSnapshot::Chunk {
            vector_pointer: chunks.chunks.as_ptr() as usize,
            vector_capacity: chunks.chunks.capacity(),
            primary: chunks.primary,
            primary_hints: chunks.primary_hints,
            nodes: chunks
                .chunks
                .iter()
                .map(|node| NodeSnapshot {
                    id: node.id,
                    chunk_type: node.chunk_type,
                    flags: node.flags,
                    capability: node.capability,
                    payload: storage_snapshot(document, &node.payload),
                })
                .collect(),
        },
    };
    DocumentSnapshot {
        origin: origin_snapshot(&document.origin),
        logical_len: document.logical_len,
        file: document.file,
        layout: document.layout(),
        compatibility: document.compatibility,
        trailing: document.trailing,
        dirty: document.dirty,
        next_id: document.next_id,
        public_primary_hints: document.primary_hints(),
        state,
    }
}

fn assert_atomic_error<T>(
    document: &Document<'_>,
    before: &DocumentSnapshot,
    result: Result<T, EditError>,
    expected: EditError,
) {
    assert_eq!(result.err(), Some(expected));
    assert_eq!(snapshot(document), *before);
}

fn refresh_header_crc(source: &mut [u8]) {
    let (covered, stored) = match Layout::from_u8(source[6]).unwrap() {
        Layout::Flat => (24, 24),
        Layout::Chunk => (40, 40),
    };
    let checksum = crc32(&source[..covered]);
    source[stored..stored + 4].copy_from_slice(&checksum.to_le_bytes());
}

fn set_wire_primary(source: &mut [u8], chunk_type: ChunkType, hints: PrimaryHints) {
    source[20..22].copy_from_slice(&chunk_type.raw().to_le_bytes());
    source[22] = hints.color_format_raw();
    source[24..28].copy_from_slice(&hints.width().to_le_bytes());
    source[28..32].copy_from_slice(&hints.height().to_le_bytes());
    source[32..36].copy_from_slice(&hints.stride().to_le_bytes());
    refresh_header_crc(source);
}

fn make_future(source: &mut [u8]) {
    source[5] = VERSION_MINOR + 1;
    refresh_header_crc(source);
}

fn mixed_document() -> Document<'static> {
    let mut source = encode_chunks(&[
        (TYPE_A.raw(), 0, b"source-primary"),
        (TYPE_B.raw(), 0, b"source-secondary"),
    ]);
    set_wire_primary(&mut source, TYPE_A, WIRE_HINTS);
    let mut document = Document::from_vec(source).unwrap();
    document.set_raw_policy(id(0), explicit_policy()).unwrap();
    document.set_raw_policy(id(1), explicit_policy()).unwrap();
    document
        .push_raw(raw(
            TYPE_C,
            ChunkFlags::NONE,
            PayloadInput::Borrowed(BORROWED_PAYLOAD),
            explicit_policy(),
        ))
        .unwrap();
    document
        .push_raw(raw(
            ChunkType::META,
            ChunkFlags::NONE,
            PayloadInput::Owned(vec![3, 1, 4, 1, 5]),
            explicit_policy(),
        ))
        .unwrap();
    document.dirty = false;
    document
}

fn duplicate_primary_document() -> Document<'static> {
    let mut source = encode_chunks(&[
        (TYPE_B.raw(), 0, b"before"),
        (TYPE_A.raw(), 0, b"primary"),
        (TYPE_C.raw(), 0, b"middle"),
        (TYPE_A.raw(), 0, b"duplicate"),
    ]);
    set_wire_primary(&mut source, TYPE_A, WIRE_HINTS);
    Document::from_vec(source).unwrap()
}

fn image_payload(width: u32, height: u32) -> Vec<u8> {
    let format = ColorFormat::A8;
    let stride = format.minimum_stride(width).unwrap();
    let main_len = usize::try_from(stride.checked_mul(height).unwrap()).unwrap();
    let main = vec![7; main_len];
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

#[test]
fn complete_snapshot_distinguishes_origin_and_document_state_variants() {
    let new_document = Document::new_chunk();
    let new_snapshot = snapshot(&new_document);
    assert_eq!(new_snapshot.origin.kind, OriginKind::New);
    assert!(matches!(new_snapshot.state, StateSnapshot::Chunk { .. }));

    let flat_source = encode_flat(&FlatImageInput {
        width: 1,
        height: 1,
        stride: 1,
        format: ColorFormat::A8,
        main: &[7],
        extra: None,
    });
    let borrowed_flat = Document::open(&flat_source).unwrap();
    let borrowed_snapshot = snapshot(&borrowed_flat);
    assert_eq!(borrowed_snapshot.origin.kind, OriginKind::Borrowed);
    assert!(matches!(
        borrowed_snapshot.state,
        StateSnapshot::SourceFlat { .. }
    ));

    let mut future_flat = flat_source;
    make_future(&mut future_flat);
    let opaque_flat = Document::from_vec(future_flat).unwrap();
    let opaque_snapshot = snapshot(&opaque_flat);
    assert_eq!(opaque_snapshot.origin.kind, OriginKind::Owned);
    assert!(matches!(
        opaque_snapshot.state,
        StateSnapshot::OpaqueFlat { .. }
    ));
}

#[test]
fn global_edit_blockers_precede_exact_noops_without_state_changes() {
    let mut future_source = encode_chunks(&[(TYPE_A.raw(), 0, b"source")]);
    set_wire_primary(&mut future_source, TYPE_A, WIRE_HINTS);
    make_future(&mut future_source);
    future_source.extend_from_slice(b"tail");
    let options = OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
    let mut future = Document::from_vec_with(future_source, &options).unwrap();
    let primary = future.primary().unwrap();
    let before = snapshot(&future);
    let result = future.replace_raw(
        primary,
        PayloadInput::Borrowed(b"source"),
        RawChunkPolicy::infer(),
    );
    assert_atomic_error(&future, &before, result, EditError::FutureSemanticsReadOnly);

    let mut trailing_source = encode_chunks(&[(TYPE_A.raw(), 0, b"source")]);
    set_wire_primary(&mut trailing_source, TYPE_A, WIRE_HINTS);
    trailing_source.extend_from_slice(b"tail");
    let mut trailing = Document::from_vec_with(trailing_source, &options).unwrap();
    let primary = trailing.primary().unwrap();
    let before = snapshot(&trailing);
    let result = trailing.replace_raw(
        primary,
        PayloadInput::Borrowed(b"source"),
        RawChunkPolicy::infer(),
    );
    assert_atomic_error(
        &trailing,
        &before,
        result,
        EditError::PreservedTrailingBytesReadOnly,
    );
}

#[test]
fn identity_failures_and_exhaustion_precede_payload_policy() {
    let mut document = mixed_document();
    document.next_id = u32::MAX;
    let before = snapshot(&document);
    let invalid = id(u32::MAX);
    let bad_input = || {
        raw(
            TYPE_A,
            ChunkFlags::NONE,
            PayloadInput::Borrowed(b"bad-policy"),
            RawChunkPolicy::infer(),
        )
    };

    let result = document.insert_before(invalid, bad_input());
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.insert_after(invalid, bad_input());
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.replace_raw(
        invalid,
        PayloadInput::Borrowed(b"replacement"),
        RawChunkPolicy::infer(),
    );
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.remove(invalid);
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.remove_to_vec(invalid);
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.set_type(invalid, TYPE_C, RawChunkPolicy::infer());
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.set_flags(invalid, ChunkFlags::CRITICAL, RawChunkPolicy::infer());
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.set_raw_policy(invalid, RawChunkPolicy::infer());
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.move_before(invalid, id(0));
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.move_after(id(0), invalid);
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.set_primary(invalid);
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);
    let result = document.set_primary_with_hints(invalid, PrimaryHints::ZERO);
    assert_atomic_error(&document, &before, result, EditError::InvalidChunkId);

    let result = document.push_raw(bad_input());
    assert_atomic_error(&document, &before, result, EditError::ChunkIdExhausted);
    let result = document.insert_before(id(0), bad_input());
    assert_atomic_error(&document, &before, result, EditError::ChunkIdExhausted);
    let result = document.insert_after(id(0), bad_input());
    assert_atomic_error(&document, &before, result, EditError::ChunkIdExhausted);
}

#[test]
fn primary_shadowing_fails_before_payload_policy_or_reordering() {
    let mut exhausted = duplicate_primary_document();
    exhausted.next_id = u32::MAX;
    let before = snapshot(&exhausted);
    let result = exhausted.insert_before(
        id(1),
        raw(
            TYPE_A,
            ChunkFlags::NONE,
            PayloadInput::Borrowed(b"shadow"),
            RawChunkPolicy::infer(),
        ),
    );
    assert_atomic_error(&exhausted, &before, result, EditError::ChunkIdExhausted);

    let mut document = duplicate_primary_document();
    let before = snapshot(&document);
    let result = document.insert_before(
        id(1),
        raw(
            TYPE_A,
            ChunkFlags::NONE,
            PayloadInput::Borrowed(b"shadow"),
            RawChunkPolicy::infer(),
        ),
    );
    assert_atomic_error(&document, &before, result, EditError::WouldShadowPrimary);
    let result = document.move_before(id(3), id(1));
    assert_atomic_error(&document, &before, result, EditError::WouldShadowPrimary);
    let result = document.set_type(id(0), TYPE_A, RawChunkPolicy::infer());
    assert_atomic_error(&document, &before, result, EditError::WouldShadowPrimary);
}

#[test]
fn primary_hint_failures_precede_duplicate_rotation() {
    let mut opaque = duplicate_primary_document();
    let before = snapshot(&opaque);
    let result = opaque.set_primary(id(3));
    assert_atomic_error(
        &opaque,
        &before,
        result,
        EditError::PrimaryHintsRequired { chunk_type: TYPE_A },
    );

    let payload = image_payload(2, 2);
    let mut source = encode_chunks(&[
        (ChunkType::IMAGE.raw(), 0, payload.as_slice()),
        (ChunkType::IMAGE.raw(), 0, payload.as_slice()),
    ]);
    set_wire_primary(&mut source, ChunkType::IMAGE, image_hints(2, 2));
    let mut image = Document::from_vec(source).unwrap();
    let before = snapshot(&image);
    let result = image.set_primary_with_hints(id(1), PrimaryHints::ZERO);
    assert_atomic_error(
        &image,
        &before,
        result,
        EditError::InvalidPrimaryHints {
            chunk_type: ChunkType::IMAGE,
        },
    );
}

#[test]
fn raw_descriptor_reserve_and_capability_failures_preserve_full_state() {
    let mut document = mixed_document();
    let before = snapshot(&document);

    let result = document.push_raw(raw(
        TYPE_C,
        ChunkFlags::NONE,
        PayloadInput::Borrowed(b"bad-policy"),
        RawChunkPolicy::infer(),
    ));
    assert_atomic_error(
        &document,
        &before,
        result,
        EditError::RelocationAssumptionRequired { chunk_type: TYPE_C },
    );
    let result = document.replace_raw(
        id(1),
        PayloadInput::Borrowed(b"replacement"),
        RawChunkPolicy::infer(),
    );
    assert_atomic_error(
        &document,
        &before,
        result,
        EditError::RelocationAssumptionRequired { chunk_type: TYPE_B },
    );
    let result = document.set_type(id(1), TYPE_C, RawChunkPolicy::infer());
    assert_atomic_error(
        &document,
        &before,
        result,
        EditError::RelocationAssumptionRequired { chunk_type: TYPE_C },
    );
    let result = document.set_flags(id(1), ChunkFlags::CRITICAL, relocation_only_policy());
    assert_atomic_error(
        &document,
        &before,
        result,
        EditError::CriticalAssumptionRequired { chunk_type: TYPE_B },
    );
    let result = document.set_raw_policy(id(1), RawChunkPolicy::infer());
    assert_atomic_error(
        &document,
        &before,
        result,
        EditError::RelocationAssumptionRequired { chunk_type: TYPE_B },
    );
    let result = document.push_raw(raw(
        TYPE_C,
        ChunkFlags::from_bits_retain(0x8000),
        PayloadInput::Borrowed(b"reserved"),
        explicit_policy(),
    ));
    assert_atomic_error(
        &document,
        &before,
        result,
        EditError::ReservedFlagBits { bits: 0x8000 },
    );

    let result = document.insert_raw_at_end_with(
        raw(
            TYPE_C,
            ChunkFlags::NONE,
            PayloadInput::Borrowed(b"bad-policy"),
            RawChunkPolicy::infer(),
        ),
        |_| panic!("payload policy failure must precede node reserve"),
    );
    assert_atomic_error(
        &document,
        &before,
        result,
        EditError::RelocationAssumptionRequired { chunk_type: TYPE_C },
    );

    let result = document.insert_raw_at_end_with(
        raw(
            TYPE_C,
            ChunkFlags::NONE,
            PayloadInput::Owned(vec![2, 7, 1, 8]),
            explicit_policy(),
        ),
        |_| Err(EditError::AllocationFailed),
    );
    assert_atomic_error(&document, &before, result, EditError::AllocationFailed);
}

#[test]
fn current_flat_chunk_only_failures_preserve_planes_and_source() {
    let source = encode_flat(&FlatImageInput {
        width: 2,
        height: 2,
        stride: 2,
        format: ColorFormat::A8,
        main: &[1, 2, 3, 4],
        extra: None,
    });
    let mut document = Document::open(&source).unwrap();
    let before = snapshot(&document);
    let invalid = id(0);
    let result = document.replace_raw(
        invalid,
        PayloadInput::Borrowed(b"replacement"),
        RawChunkPolicy::infer(),
    );
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
    let result = document.remove(invalid);
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
    let result = document.remove_to_vec(invalid);
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
    let result = document.set_type(invalid, TYPE_A, RawChunkPolicy::infer());
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
    let result = document.set_flags(invalid, ChunkFlags::CRITICAL, RawChunkPolicy::infer());
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
    let result = document.set_raw_policy(invalid, RawChunkPolicy::infer());
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
    let result = document.move_before(invalid, invalid);
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
    let result = document.move_after(invalid, invalid);
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
    let result = document.set_primary(invalid);
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
    let result = document.set_primary_with_hints(invalid, PrimaryHints::ZERO);
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
    let result = document.clear_primary();
    assert_atomic_error(&document, &before, result, EditError::ChunkLayoutRequired);
}
