use alloc::vec::Vec;

use super::*;
use crate::header::CHUNK_FILE_HEADER_LEN;
use crate::{
    ColorFormat, CriticalAssumption, FlatImageInput, ImageChunkInput, PayloadInput, RawChunkInput,
    RelocationAssumption, ReservedBitsPolicy, TrailingBytesPolicy, crc32, encode_chunk_image,
    encode_chunks, encode_flat,
};

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
    primary_hints: PrimaryHintState,
    vector_pointer: usize,
    vector_capacity: usize,
    nodes: Vec<NodeSnapshot>,
}

const CUSTOM_TYPE: ChunkType = match ChunkType::new(0xbeef) {
    Some(chunk_type) => chunk_type,
    None => panic!("nonzero chunk type"),
};

fn custom_type() -> ChunkType {
    CUSTOM_TYPE
}

const fn explicit_policy() -> RawChunkPolicy {
    RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
        reserved_flag_bits: ReservedBitsPolicy::Reject,
    }
}

fn raw_input(payload: &'static [u8]) -> RawChunkInput<'static> {
    RawChunkInput {
        chunk_type: custom_type(),
        flags: ChunkFlags::NONE,
        payload: PayloadInput::Borrowed(payload),
        policy: explicit_policy(),
    }
}

fn policy_failing_raw_input() -> RawChunkInput<'static> {
    RawChunkInput {
        chunk_type: custom_type(),
        flags: ChunkFlags::NONE,
        payload: PayloadInput::Borrowed(b"invalid-policy"),
        policy: RawChunkPolicy::infer(),
    }
}

fn refresh_header_crc(source: &mut [u8]) {
    let (covered, stored) = match Layout::from_u8(source[6]).unwrap() {
        Layout::Flat => (24, 24),
        Layout::Chunk => (40, 40),
    };
    let checksum = crc32(&source[..covered]);
    source[stored..stored + 4].copy_from_slice(&checksum.to_le_bytes());
}

fn make_future_minor(source: &mut [u8]) {
    source[5] = VERSION_MINOR + 1;
    refresh_header_crc(source);
}

fn make_future_flags(source: &mut [u8]) {
    source[7] = 0x80;
    refresh_header_crc(source);
}

fn set_primary_type(source: &mut [u8], chunk_type: ChunkType) {
    source[20..22].copy_from_slice(&chunk_type.raw().to_le_bytes());
    refresh_header_crc(source);
}

fn normalize_options() -> OpenOptions<'static> {
    OpenOptions::new().with_compatibility(CompatibilityPolicy::NormalizeToCurrent)
}

fn preserve_trailing_options() -> OpenOptions<'static> {
    OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve)
}

fn normalize_and_preserve_trailing_options() -> OpenOptions<'static> {
    normalize_options().with_trailing_bytes(TrailingBytesPolicy::Preserve)
}

fn open_error(result: Result<Document<'_>, DocumentError>) -> DocumentError {
    match result {
        Ok(_) => panic!("expected document open failure"),
        Err(error) => error,
    }
}

fn snapshot(document: &Document<'_>) -> DocumentSnapshot {
    let (primary, primary_hints, vector_pointer, vector_capacity, nodes) = match &document.state {
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
        primary_hints,
        vector_pointer,
        vector_capacity,
        nodes,
    }
}

fn assert_every_mutation_blocked(document: &mut Document<'_>, id: ChunkId, expected: EditError) {
    let before = snapshot(document);
    let invalid = ChunkId::new(u32::MAX);

    assert_eq!(document.push_raw(raw_input(b"new")), Err(expected.clone()));
    assert_eq!(snapshot(document), before);
    assert_eq!(
        document.insert_raw_before(invalid, raw_input(b"new")),
        Err(expected.clone())
    );
    assert_eq!(snapshot(document), before);
    assert_eq!(
        document.insert_raw_after(invalid, raw_input(b"new")),
        Err(expected.clone())
    );
    assert_eq!(snapshot(document), before);
    assert_eq!(
        document.replace_raw(id, PayloadInput::Borrowed(b"source"), explicit_policy()),
        Err(expected.clone())
    );
    assert_eq!(snapshot(document), before);
    assert_eq!(document.remove(id), Err(expected.clone()));
    assert_eq!(snapshot(document), before);
    assert_eq!(document.remove_to_vec(id), Err(expected.clone()));
    assert_eq!(snapshot(document), before);
    assert_eq!(
        document.set_type(id, custom_type(), explicit_policy()),
        Err(expected.clone())
    );
    assert_eq!(snapshot(document), before);
    assert_eq!(
        document.set_flags(id, ChunkFlags::NONE, explicit_policy()),
        Err(expected.clone())
    );
    assert_eq!(snapshot(document), before);
    assert_eq!(
        document.set_raw_policy(id, explicit_policy()),
        Err(expected.clone())
    );
    assert_eq!(snapshot(document), before);
    assert_eq!(document.move_before(id, id), Err(expected.clone()));
    assert_eq!(snapshot(document), before);
    assert_eq!(document.move_after(id, id), Err(expected.clone()));
    assert_eq!(snapshot(document), before);
    assert_eq!(document.set_primary(id), Err(expected.clone()));
    assert_eq!(snapshot(document), before);
    assert_eq!(
        document.set_primary_with_hints(id, PrimaryHints::ZERO),
        Err(expected.clone())
    );
    assert_eq!(snapshot(document), before);
    assert_eq!(document.clear_primary(), Err(expected));
    assert_eq!(snapshot(document), before);
}

#[test]
fn future_chunk_metadata_is_preserved_or_normalized_explicitly() {
    for make_future in [make_future_minor as fn(&mut [u8]), make_future_flags] {
        let mut source = encode_chunks(&[(custom_type().raw(), 0, b"source")]);
        make_future(&mut source);

        let mut preserved = Document::open(&source).unwrap();
        let id = preserved.chunks().next().unwrap().id();
        assert_eq!(preserved.compatibility, Compatibility::FutureReadOnly);
        assert!(preserved.file_metadata().has_future_semantics());
        assert!(!preserved.is_dirty());
        assert!(
            source
                .as_ptr_range()
                .contains(&preserved.get(id).unwrap().payload_bytes().unwrap().as_ptr())
        );
        assert_eq!(
            preserved.set_type(id, custom_type(), explicit_policy()),
            Err(EditError::FutureSemanticsReadOnly)
        );

        let normalized = Document::open_with(&source, &normalize_options()).unwrap();
        let normalized_chunk = normalized.chunks().next().unwrap();
        assert_eq!(normalized.compatibility, Compatibility::Current);
        assert_eq!(normalized.file, FileMeta::CURRENT);
        assert!(normalized.is_dirty());
        assert_eq!(normalized_chunk.payload_bytes(), Some(b"source".as_slice()));
        assert!(
            source
                .as_ptr_range()
                .contains(&normalized_chunk.payload_bytes().unwrap().as_ptr())
        );
    }

    let source = encode_chunks(&[(custom_type().raw(), 0, b"source")]);
    let current = Document::open_with(&source, &normalize_options()).unwrap();
    assert_eq!(current.compatibility, Compatibility::Current);
    assert_eq!(current.file, FileMeta::CURRENT);
    assert!(!current.is_dirty());
}

#[test]
fn future_chunk_normalization_revalidates_current_header_and_table_contracts() {
    let mut header_reserved = encode_chunks(&[(custom_type().raw(), 0, b"source")]);
    make_future_minor(&mut header_reserved);
    header_reserved[10] = 1;
    refresh_header_crc(&mut header_reserved);
    Document::open(&header_reserved).unwrap();
    assert_eq!(
        open_error(Document::open_with(&header_reserved, &normalize_options())),
        DocumentError::Read(ReadError::ReservedNonZero { offset: 10 })
    );

    let mut table_reserved = encode_chunks(&[(custom_type().raw(), 0, b"source")]);
    make_future_flags(&mut table_reserved);
    let offset = CHUNK_FILE_HEADER_LEN + 12;
    table_reserved[offset] = 1;
    Document::open(&table_reserved).unwrap();
    assert_eq!(
        open_error(Document::open_with(&table_reserved, &normalize_options())),
        DocumentError::Read(ReadError::ReservedNonZero { offset })
    );
}

#[test]
fn future_chunk_descriptor_normalization_requires_container_normalization() {
    let reserved = ChunkFlags::from_bits_retain(0xa500);
    let mut source = encode_chunks(&[(custom_type().raw(), reserved.bits(), b"source")]);
    make_future_minor(&mut source);
    let policies = [RawTypePolicy {
        chunk_type: custom_type(),
        policy: RawChunkPolicy {
            reserved_flag_bits: ReservedBitsPolicy::Normalize,
            ..explicit_policy()
        },
    }];

    let preserve_options = OpenOptions::new().with_raw_type_policies(&policies);
    let preserved = Document::open_with(&source, &preserve_options).unwrap();
    assert_eq!(preserved.chunks().next().unwrap().flags(), reserved);
    assert_eq!(preserved.compatibility, Compatibility::FutureReadOnly);
    assert!(!preserved.is_dirty());

    let normalize_options =
        preserve_options.with_compatibility(CompatibilityPolicy::NormalizeToCurrent);
    let normalized = Document::open_with(&source, &normalize_options).unwrap();
    let node = match &normalized.state {
        DocumentState::Chunk(chunks) => &chunks.chunks[0],
        _ => panic!("expected CHUNK document"),
    };
    assert_eq!(node.flags, ChunkFlags::NONE);
    assert!(!node.capability.preserves_reserved_bits());
    assert_eq!(normalized.file, FileMeta::CURRENT);
    assert_eq!(normalized.compatibility, Compatibility::Current);
    assert!(normalized.is_dirty());
}

#[test]
fn future_flat_is_opaque_until_strict_current_normalization_succeeds() {
    let mut source = encode_flat(&FlatImageInput {
        width: 2,
        height: 1,
        stride: 2,
        format: ColorFormat::A8,
        main: &[7, 9],
        extra: None,
    });
    make_future_minor(&mut source);

    let preserved_reader = Reader::open(&source).unwrap();
    assert!(preserved_reader.has_future_semantics());
    assert_eq!(preserved_reader.flat_image(), None);
    let mut preserved = Document::open(&source).unwrap();
    assert_eq!(preserved.compatibility, Compatibility::FutureReadOnly);
    assert_eq!(preserved.flat_image(), None);
    assert!(!preserved.is_dirty());
    assert_eq!(
        preserved.push_raw(raw_input(b"new")),
        Err(EditError::FutureSemanticsReadOnly)
    );

    let normalized = Document::open_with(&source, &normalize_options()).unwrap();
    let image = normalized.flat_image().unwrap();
    assert_eq!(normalized.file, FileMeta::CURRENT);
    assert_eq!(normalized.compatibility, Compatibility::Current);
    assert!(normalized.is_dirty());
    assert_eq!(image.main(), &[7, 9]);
    assert_eq!(image.main().as_ptr(), source[FLAT_HEADER_LEN..].as_ptr());
    let normalized_file = normalized.file;
    let normalized_compatibility = normalized.compatibility;
    let normalized_trailing = normalized.trailing;
    drop(normalized);

    let owned_pointer = source.as_ptr();
    let owned = Document::from_vec_with(source, &normalize_options()).unwrap();
    let owned_image = owned.flat_image().unwrap();
    assert_eq!(
        owned_image.main().as_ptr() as usize,
        owned_pointer as usize + FLAT_HEADER_LEN
    );
    assert_eq!(owned.file, normalized_file);
    assert_eq!(owned.compatibility, normalized_compatibility);
    assert_eq!(owned.trailing, normalized_trailing);
}

#[test]
fn future_flat_normalization_reports_exact_current_validation_failures() {
    let mut small_stride = encode_flat(&FlatImageInput {
        width: 2,
        height: 1,
        stride: 4,
        format: ColorFormat::RGB565,
        main: &[0; 4],
        extra: None,
    });
    make_future_minor(&mut small_stride);
    small_stride[20..24].copy_from_slice(&3u32.to_le_bytes());
    refresh_header_crc(&mut small_stride);
    Document::open(&small_stride).unwrap();
    assert_eq!(
        open_error(Document::open_with(&small_stride, &normalize_options())),
        DocumentError::Read(ReadError::StrideTooSmall {
            minimum: 4,
            actual: 3,
        })
    );

    let mut unknown_format = small_stride.clone();
    unknown_format[8] = 0xfe;
    refresh_header_crc(&mut unknown_format);
    Document::open(&unknown_format).unwrap();
    assert_eq!(
        open_error(Document::open_with(&unknown_format, &normalize_options())),
        DocumentError::Read(ReadError::UnknownColorFormat(0xfe))
    );

    let mut reserved = small_stride;
    reserved[9] = 1;
    refresh_header_crc(&mut reserved);
    Document::open(&reserved).unwrap();
    assert_eq!(
        open_error(Document::open_with(&reserved, &normalize_options())),
        DocumentError::Read(ReadError::ReservedNonZero { offset: 9 })
    );
}

#[test]
fn normalized_future_flat_keeps_extra_bytes_as_an_independent_trailing_region() {
    let mut source = encode_flat(&FlatImageInput {
        width: 1,
        height: 1,
        stride: 1,
        format: ColorFormat::A8,
        main: &[7],
        extra: None,
    });
    let logical_len = source.len();
    make_future_flags(&mut source);
    source.extend_from_slice(b"tail");

    assert_eq!(
        open_error(Document::open_with(&source, &normalize_options())),
        DocumentError::Read(ReadError::TrailingBytes {
            logical_len,
            actual_len: logical_len + 4,
        })
    );

    let mut document =
        Document::open_with(&source, &normalize_and_preserve_trailing_options()).unwrap();
    assert!(matches!(document.state, DocumentState::Flat(_)));
    assert_eq!(
        document.primary_hints(),
        PrimaryHints::new(ColorFormat::A8.to_u8(), 1, 1, 1)
    );
    assert_eq!(document.compatibility, Compatibility::Current);
    assert_eq!(document.trailing, TrailingState::Preserved);
    assert_eq!(document.logical_len, logical_len);
    assert_eq!(
        document.push_raw(raw_input(b"new")),
        Err(EditError::PreservedTrailingBytesReadOnly)
    );
    document.discard_trailing_bytes().unwrap();
    assert_eq!(document.trailing, TrailingState::Discarded);
    let added = document.push_raw(raw_input(b"new")).unwrap();
    assert_eq!(added, ChunkId::new(1));
    assert_eq!(document.layout(), Layout::Chunk);
}

#[test]
fn future_blocker_precedes_trailing_ids_policies_and_noops_atomically() {
    let mut source = encode_chunks(&[(custom_type().raw(), 0, b"source")]);
    set_primary_type(&mut source, custom_type());
    make_future_minor(&mut source);
    source.extend_from_slice(b"tail");

    let mut document = Document::open_with(&source, &preserve_trailing_options()).unwrap();
    let id = document.chunks().next().unwrap().id();
    assert_eq!(document.primary(), Some(id));
    assert_eq!(document.compatibility, Compatibility::FutureReadOnly);
    assert_eq!(document.trailing, TrailingState::Preserved);
    let DocumentState::Chunk(chunks) = &document.state else {
        panic!("expected CHUNK document");
    };
    assert_eq!(
        chunks.chunks[0].capability,
        RewriteCapability::PRESERVE_ONLY
    );
    assert_every_mutation_blocked(&mut document, id, EditError::FutureSemanticsReadOnly);
    let DocumentState::Chunk(chunks) = &document.state else {
        panic!("expected CHUNK document");
    };
    assert_eq!(
        chunks.chunks[0].capability,
        RewriteCapability::PRESERVE_ONLY
    );

    let before = snapshot(&document);
    assert_eq!(
        document.discard_trailing_bytes(),
        Err(EditError::FutureSemanticsReadOnly)
    );
    assert_eq!(snapshot(&document), before);
}

#[test]
fn trailing_blocker_is_atomic_and_explicit_discard_only_changes_edit_state() {
    let mut source = encode_chunks(&[(custom_type().raw(), 0, b"source")]);
    set_primary_type(&mut source, custom_type());
    source.extend_from_slice(b"tail");

    assert_eq!(
        open_error(Document::open(&source)),
        DocumentError::Read(ReadError::TrailingBytes {
            logical_len: source.len() - 4,
            actual_len: source.len(),
        })
    );

    let mut document = Document::open_with(&source, &preserve_trailing_options()).unwrap();
    let id = document.chunks().next().unwrap().id();
    assert_eq!(document.primary(), Some(id));
    assert_eq!(document.compatibility, Compatibility::Current);
    assert_eq!(document.trailing, TrailingState::Preserved);
    assert!(!document.is_dirty());
    let DocumentState::Chunk(chunks) = &document.state else {
        panic!("expected CHUNK document");
    };
    assert_eq!(
        chunks.chunks[0].capability,
        RewriteCapability::PRESERVE_ONLY
    );
    assert_every_mutation_blocked(&mut document, id, EditError::PreservedTrailingBytesReadOnly);
    let DocumentState::Chunk(chunks) = &document.state else {
        panic!("expected CHUNK document");
    };
    assert_eq!(
        chunks.chunks[0].capability,
        RewriteCapability::PRESERVE_ONLY
    );

    let before_discard = snapshot(&document);
    document.discard_trailing_bytes().unwrap();
    assert_eq!(document.trailing, TrailingState::Discarded);
    assert!(document.is_dirty());
    assert_eq!(document.next_id, before_discard.next_id);
    assert_eq!(document.primary(), before_discard.primary);
    let after_discard = snapshot(&document);
    assert_eq!(after_discard.vector_pointer, before_discard.vector_pointer);
    assert_eq!(
        after_discard.vector_capacity,
        before_discard.vector_capacity
    );
    assert_eq!(after_discard.nodes, before_discard.nodes);
    document.discard_trailing_bytes().unwrap();
    assert_eq!(snapshot(&document), after_discard);

    document.set_raw_policy(id, explicit_policy()).unwrap();
    assert_eq!(
        document.get(id).unwrap().payload_bytes(),
        Some(b"source".as_slice())
    );
}

#[test]
fn owned_trailing_discard_retains_the_source_allocation_and_logical_boundary() {
    let mut source = encode_chunks(&[(custom_type().raw(), 0, b"source")]);
    let logical_len = source.len();
    source.extend_from_slice(b"tail");
    let source_pointer = source.as_ptr();
    let source_len = source.len();
    let source_capacity = source.capacity();

    let mut document = Document::from_vec_with(source, &preserve_trailing_options()).unwrap();
    let origin_allocation = |document: &Document<'_>| match &document.origin {
        Origin::Owned(bytes) => (bytes.as_ptr(), bytes.len(), bytes.capacity()),
        Origin::New | Origin::Borrowed(_) => panic!("expected owned origin"),
    };
    let expected_origin = (source_pointer, source_len, source_capacity);
    assert_eq!(origin_allocation(&document), expected_origin);
    assert_eq!(document.logical_len, logical_len);
    assert_eq!(document.trailing, TrailingState::Preserved);
    assert_eq!(&document.origin.source().unwrap()[logical_len..], b"tail");
    assert!(!document.is_dirty());

    document.discard_trailing_bytes().unwrap();
    assert_eq!(origin_allocation(&document), expected_origin);
    assert_eq!(document.logical_len, logical_len);
    assert_eq!(document.trailing, TrailingState::Discarded);
    assert_eq!(&document.origin.source().unwrap()[logical_len..], b"tail");
    assert!(document.is_dirty());

    document.discard_trailing_bytes().unwrap();
    assert_eq!(origin_allocation(&document), expected_origin);
    assert_eq!(document.logical_len, logical_len);
    assert_eq!(document.trailing, TrailingState::Discarded);
    assert_eq!(&document.origin.source().unwrap()[logical_len..], b"tail");
    assert!(document.is_dirty());
}

#[test]
fn no_tail_discard_is_a_clean_noop_and_trailing_precedes_flat_layout() {
    let source = encode_chunks(&[(custom_type().raw(), 0, b"source")]);
    let mut document = Document::open(&source).unwrap();
    let before = snapshot(&document);
    document.discard_trailing_bytes().unwrap();
    assert_eq!(snapshot(&document), before);

    let mut flat_source = encode_flat(&FlatImageInput {
        width: 1,
        height: 1,
        stride: 1,
        format: ColorFormat::A8,
        main: &[7],
        extra: None,
    });
    flat_source.extend_from_slice(b"tail");
    let mut flat = Document::open_with(&flat_source, &preserve_trailing_options()).unwrap();
    let invalid = ChunkId::new(0);
    assert_every_mutation_blocked(
        &mut flat,
        invalid,
        EditError::PreservedTrailingBytesReadOnly,
    );
}

#[test]
fn insertion_plans_layout_and_id_before_payload_policy_atomically() {
    let mut document = Document::new();
    let anchor = document.push_raw(raw_input(b"source")).unwrap();
    document.dirty = false;
    document.next_id = u32::MAX;
    let before = snapshot(&document);

    assert_eq!(
        document.push_raw(policy_failing_raw_input()),
        Err(EditError::ChunkIdExhausted)
    );
    assert_eq!(snapshot(&document), before);
    assert_eq!(
        document.insert_raw_before(anchor, policy_failing_raw_input()),
        Err(EditError::ChunkIdExhausted)
    );
    assert_eq!(snapshot(&document), before);
    assert_eq!(
        document.insert_raw_after(anchor, policy_failing_raw_input()),
        Err(EditError::ChunkIdExhausted)
    );
    assert_eq!(snapshot(&document), before);

    let flat_source = encode_flat(&FlatImageInput {
        width: 1,
        height: 1,
        stride: 1,
        format: ColorFormat::A8,
        main: &[7],
        extra: None,
    });
    let mut flat = Document::open(&flat_source).unwrap();
    flat.next_id = u32::MAX;
    let before_flat = snapshot(&flat);
    assert_eq!(
        flat.push_raw(policy_failing_raw_input()),
        Err(EditError::ChunkIdExhausted)
    );
    assert_eq!(snapshot(&flat), before_flat);
}

#[test]
fn normalized_future_image_uses_the_existing_absolute_offset_preflight() {
    let mut source = encode_chunk_image(&ImageChunkInput {
        width: 2,
        height: 2,
        format: ColorFormat::A8,
        stride: 2,
        main: &[1, 2, 3, 4],
        extra: None,
    });
    make_future_minor(&mut source);
    let document = Document::open_with(&source, &normalize_options()).unwrap();
    let node = match &document.state {
        DocumentState::Chunk(chunks) => &chunks.chunks[0],
        _ => panic!("expected CHUNK document"),
    };
    assert_eq!(node.chunk_type, ChunkType::IMAGE);
    assert!(node.capability.is_relocatable());
    assert!(node.capability.critical_understood());
    assert!(document.is_dirty());
}
