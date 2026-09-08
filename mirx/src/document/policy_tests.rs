use alloc::vec;
use alloc::vec::Vec;

use super::*;
use crate::header::{CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, chunk_type};
use crate::{
    ColorFormat, CriticalAssumption, EditError, FlatImageInput, ImageChunkInput, PayloadInput,
    RawChunkInput, RelocationAssumption, ReservedBitsPolicy, crc32, encode_chunk_image,
    encode_chunks, encode_flat,
};

fn custom_type() -> ChunkType {
    ChunkType::new(0xbeef).unwrap()
}

const fn policy(
    relocation: RelocationAssumption,
    critical_semantics: CriticalAssumption,
    reserved_flag_bits: ReservedBitsPolicy,
) -> RawChunkPolicy {
    RawChunkPolicy {
        relocation,
        critical_semantics,
        reserved_flag_bits,
    }
}

const fn relocatable_policy(reserved_flag_bits: ReservedBitsPolicy) -> RawChunkPolicy {
    policy(
        RelocationAssumption::AssumeRelocatable,
        CriticalAssumption::Infer,
        reserved_flag_bits,
    )
}

const fn complete_policy(reserved_flag_bits: ReservedBitsPolicy) -> RawChunkPolicy {
    policy(
        RelocationAssumption::AssumeRelocatable,
        CriticalAssumption::AssumeCriticalUnderstood,
        reserved_flag_bits,
    )
}

fn document_error(result: Result<Document<'_>, DocumentError>) -> DocumentError {
    match result {
        Ok(_) => panic!("expected document open failure"),
        Err(error) => error,
    }
}

fn reader_error(result: Result<Reader<'_>, ReadError>) -> ReadError {
    match result {
        Ok(_) => panic!("expected reader open failure"),
        Err(error) => error,
    }
}

fn chunk_set<'document, 'source>(
    document: &'document Document<'source>,
) -> &'document ChunkSet<'source> {
    match &document.state {
        DocumentState::Chunk(chunks) => chunks,
        _ => panic!("expected CHUNK document"),
    }
}

fn node<'document, 'source>(
    document: &'document Document<'source>,
    index: usize,
) -> &'document ChunkNode<'source> {
    &chunk_set(document).chunks[index]
}

fn payload_bytes<'document>(document: &'document Document<'_>, index: usize) -> &'document [u8] {
    match &node(document, index).payload {
        PayloadStorage::SourceRange(range) => document.origin.resolve(*range).unwrap(),
        PayloadStorage::Borrowed(bytes) => bytes,
        PayloadStorage::Owned(bytes) => bytes.as_slice(),
        PayloadStorage::PromotedFlat => &[],
    }
}

fn assert_capability(
    node: &ChunkNode<'_>,
    relocatable: bool,
    critical_understood: bool,
    preserves_reserved_bits: bool,
) {
    assert_eq!(node.capability.is_relocatable(), relocatable);
    assert_eq!(node.capability.critical_understood(), critical_understood);
    assert_eq!(
        node.capability.preserves_reserved_bits(),
        preserves_reserved_bits
    );
}

fn options_for<'p>(policies: &'p [RawTypePolicy]) -> OpenOptions<'p> {
    OpenOptions::new().with_raw_type_policies(policies)
}

fn valid_image_payload() -> Vec<u8> {
    let file = encode_chunk_image(&ImageChunkInput {
        width: 2,
        height: 2,
        format: ColorFormat::A8,
        stride: 2,
        main: &[1, 2, 3, 4],
        extra: None,
    });
    let entry = CHUNK_FILE_HEADER_LEN;
    let start = u32::from_le_bytes(file[entry + 4..entry + 8].try_into().unwrap()) as usize;
    let len = u32::from_le_bytes(file[entry + 8..entry + 12].try_into().unwrap()) as usize;
    file[start..start + len].to_vec()
}

fn offset_sensitive_image_source() -> Vec<u8> {
    let payload = crate::image::test_support::pad_data(valid_image_payload(), 1, 2);
    let template = encode_chunks(&[(chunk_type::IMAGE, 0, payload.as_slice())]);
    let table_end = CHUNK_FILE_HEADER_LEN + CHUNK_TABLE_ENTRY_LEN;
    let mut source = vec![0; 63 + payload.len()];
    source[..table_end].copy_from_slice(&template[..table_end]);
    source[63..].copy_from_slice(&payload);
    let file_size = source.len() as u32;
    source[16..20].copy_from_slice(&file_size.to_le_bytes());
    source[CHUNK_FILE_HEADER_LEN + 4..CHUNK_FILE_HEADER_LEN + 8]
        .copy_from_slice(&63u32.to_le_bytes());
    let checksum = crc32(&source[..40]);
    source[40..44].copy_from_slice(&checksum.to_le_bytes());
    source
}

#[test]
fn reader_remains_strict_while_document_accepts_explicit_critical_capability() {
    let malformed_image = [7, 9];
    let cases = [
        (custom_type(), b"opaque".as_slice()),
        (ChunkType::FONT, b"unsupported".as_slice()),
        (ChunkType::IMAGE, malformed_image.as_slice()),
    ];

    for (chunk_type, payload) in cases {
        let source = encode_chunks(&[(chunk_type.raw(), ChunkFlags::CRITICAL.bits(), payload)]);
        let strict = reader_error(Reader::open(&source));
        assert_eq!(
            document_error(Document::open(&source)),
            DocumentError::Read(strict.clone())
        );

        let relocate_only = [RawTypePolicy {
            chunk_type,
            policy: relocatable_policy(ReservedBitsPolicy::Reject),
        }];
        assert_eq!(
            document_error(Document::open_with(&source, &options_for(&relocate_only))),
            DocumentError::Read(strict)
        );

        let critical_only = [RawTypePolicy {
            chunk_type,
            policy: policy(
                RelocationAssumption::Infer,
                CriticalAssumption::AssumeCriticalUnderstood,
                ReservedBitsPolicy::Reject,
            ),
        }];
        let document = Document::open_with(&source, &options_for(&critical_only)).unwrap();
        assert_capability(node(&document, 0), false, true, false);
        assert!(!document.is_dirty());

        let complete = [RawTypePolicy {
            chunk_type,
            policy: complete_policy(ReservedBitsPolicy::Reject),
        }];
        let document = Document::open_with(&source, &options_for(&complete)).unwrap();
        assert_capability(node(&document, 0), true, true, false);
        assert!(!document.is_dirty());
    }
}

#[test]
fn duplicate_type_policy_uses_the_last_entry_without_retaining_the_slice() {
    let source = encode_chunks(&[(custom_type().raw(), ChunkFlags::CRITICAL.bits(), b"opaque")]);
    let strict = reader_error(Reader::open(&source));

    let rejected = [
        RawTypePolicy {
            chunk_type: custom_type(),
            policy: complete_policy(ReservedBitsPolicy::Reject),
        },
        RawTypePolicy {
            chunk_type: custom_type(),
            policy: RawChunkPolicy::infer(),
        },
    ];
    assert_eq!(
        document_error(Document::open_with(&source, &options_for(&rejected))),
        DocumentError::Read(strict)
    );

    let document = {
        let accepted = [
            RawTypePolicy {
                chunk_type: custom_type(),
                policy: RawChunkPolicy::infer(),
            },
            RawTypePolicy {
                chunk_type: custom_type(),
                policy: complete_policy(ReservedBitsPolicy::Reject),
            },
        ];
        let options = options_for(&accepted);
        Document::open_with(&source, &options).unwrap()
    };
    assert_capability(node(&document, 0), true, true, false);
    assert_eq!(document.chunks().len(), 1);
}

#[test]
fn open_classifies_valid_and_opaque_payloads_without_changing_reader_policy() {
    let valid_image = valid_image_payload();
    let malformed_image = [0xaa];
    let source = encode_chunks(&[
        (
            chunk_type::IMAGE,
            ChunkFlags::CRITICAL.bits(),
            valid_image.as_slice(),
        ),
        (chunk_type::IMAGE, 0, malformed_image.as_slice()),
        (chunk_type::FONT, 0, b"unsupported"),
        (custom_type().raw(), 0, b"custom"),
    ]);

    Reader::open(&source).unwrap();
    let document = Document::open(&source).unwrap();
    assert_capability(node(&document, 0), true, true, false);
    for index in 1..4 {
        assert_capability(node(&document, index), false, false, false);
    }
    assert!(!document.is_dirty());
}

#[test]
fn open_reserved_policy_preserves_rejects_or_normalizes_as_a_capability_grant() {
    let reserved = ChunkFlags::from_bits_retain(0xa500);
    let source = encode_chunks(&[(custom_type().raw(), reserved.bits(), b"opaque")]);

    let default = Document::open(&source).unwrap();
    assert_eq!(node(&default, 0).flags, reserved);
    assert_capability(node(&default, 0), false, false, false);
    assert!(!default.is_dirty());

    let reject = [RawTypePolicy {
        chunk_type: custom_type(),
        policy: complete_policy(ReservedBitsPolicy::Reject),
    }];
    let rejected_policy = Document::open_with(&source, &options_for(&reject)).unwrap();
    assert_eq!(node(&rejected_policy, 0).flags, reserved);
    assert_capability(node(&rejected_policy, 0), true, true, false);
    assert!(!rejected_policy.is_dirty());

    let preserve = [RawTypePolicy {
        chunk_type: custom_type(),
        policy: complete_policy(ReservedBitsPolicy::Preserve),
    }];
    let preserved = Document::open_with(&source, &options_for(&preserve)).unwrap();
    assert_eq!(node(&preserved, 0).flags, reserved);
    assert_capability(node(&preserved, 0), true, true, true);
    assert!(!preserved.is_dirty());

    let normalize = [RawTypePolicy {
        chunk_type: custom_type(),
        policy: complete_policy(ReservedBitsPolicy::Normalize),
    }];
    let normalized = Document::open_with(&source, &options_for(&normalize)).unwrap();
    assert_eq!(node(&normalized, 0).flags, ChunkFlags::NONE);
    assert_capability(node(&normalized, 0), true, true, false);
    assert!(normalized.is_dirty());
}

#[test]
fn host_and_custom_chunk_limits_are_applied_to_document_materialization() {
    let entries = vec![(chunk_type::META, 0, &[][..]); 257];
    let source = encode_chunks(&entries);
    assert_eq!(
        document_error(Document::open(&source)),
        DocumentError::Read(ReadError::TooManyChunks {
            count: 257,
            limit: OpenOptions::DEFAULT_MAX_CHUNKS,
        })
    );
    assert_eq!(
        Document::open_with(&source, &OpenOptions::host_tools())
            .unwrap()
            .chunks()
            .len(),
        257
    );
    assert_eq!(
        document_error(Document::open_with(
            &source,
            &OpenOptions::new().with_max_chunks(17)
        )),
        DocumentError::Read(ReadError::TooManyChunks {
            count: 257,
            limit: 17,
        })
    );
}

#[test]
fn from_vec_with_retains_the_source_allocation_and_copies_only_capabilities() {
    let source = encode_chunks(&[(custom_type().raw(), 0, b"owned")]);
    let source_pointer = source.as_ptr();
    let payload_pointer = {
        let entry = CHUNK_FILE_HEADER_LEN;
        let offset = u32::from_le_bytes(source[entry + 4..entry + 8].try_into().unwrap()) as usize;
        source[offset..].as_ptr()
    };
    let document = {
        let policies = [RawTypePolicy {
            chunk_type: custom_type(),
            policy: complete_policy(ReservedBitsPolicy::Reject),
        }];
        let options = options_for(&policies);
        Document::from_vec_with(source, &options).unwrap()
    };

    assert_eq!(document.origin.source().unwrap().as_ptr(), source_pointer);
    assert_eq!(
        document
            .chunks()
            .next()
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr(),
        payload_pointer
    );
    assert_capability(node(&document, 0), true, true, false);
    assert!(!document.is_dirty());
}

#[test]
fn set_raw_policy_is_strict_atomic_and_dirties_only_flag_normalization() {
    let reserved = ChunkFlags::from_bits_retain(0xa500);
    let source = encode_chunks(&[(custom_type().raw(), reserved.bits(), b"opaque")]);
    let mut document = Document::open(&source).unwrap();
    let id = node(&document, 0).id;
    let before = (
        node(&document, 0).flags,
        node(&document, 0).capability,
        payload_bytes(&document, 0).as_ptr(),
        chunk_set(&document).chunks.as_ptr(),
        chunk_set(&document).chunks.capacity(),
        chunk_set(&document).primary,
        document.next_id,
        document.is_dirty(),
    );

    assert_eq!(
        document.set_raw_policy(id, RawChunkPolicy::infer()),
        Err(EditError::ReservedFlagBits { bits: 0xa500 })
    );
    assert_eq!(
        document.set_raw_policy(
            id,
            policy(
                RelocationAssumption::Infer,
                CriticalAssumption::Infer,
                ReservedBitsPolicy::Normalize,
            )
        ),
        Err(EditError::RelocationAssumptionRequired {
            chunk_type: custom_type(),
        })
    );
    assert_eq!(
        (
            node(&document, 0).flags,
            node(&document, 0).capability,
            payload_bytes(&document, 0).as_ptr(),
            chunk_set(&document).chunks.as_ptr(),
            chunk_set(&document).chunks.capacity(),
            chunk_set(&document).primary,
            document.next_id,
            document.is_dirty(),
        ),
        before
    );

    document
        .set_raw_policy(id, relocatable_policy(ReservedBitsPolicy::Preserve))
        .unwrap();
    assert_eq!(node(&document, 0).flags, reserved);
    assert_capability(node(&document, 0), true, false, true);
    assert!(!document.is_dirty());

    document
        .set_raw_policy(id, relocatable_policy(ReservedBitsPolicy::Preserve))
        .unwrap();
    assert!(!document.is_dirty());

    document
        .set_raw_policy(id, relocatable_policy(ReservedBitsPolicy::Normalize))
        .unwrap();
    assert_eq!(node(&document, 0).flags, ChunkFlags::NONE);
    assert_capability(node(&document, 0), true, false, false);
    assert!(document.is_dirty());
    assert_eq!(payload_bytes(&document, 0).as_ptr(), before.2);
    assert_eq!(chunk_set(&document).chunks.as_ptr(), before.3);
    assert_eq!(chunk_set(&document).chunks.capacity(), before.4);
    assert_eq!(chunk_set(&document).primary, before.5);
    assert_eq!(document.next_id, before.6);
}

#[test]
fn set_raw_policy_validates_layout_id_and_critical_assumption_before_apply() {
    let flat_source = encode_flat(&FlatImageInput {
        width: 1,
        height: 1,
        stride: 1,
        format: ColorFormat::A8,
        main: &[7],
        extra: None,
    });
    let mut flat = Document::open(&flat_source).unwrap();
    assert_eq!(
        flat.set_raw_policy(ChunkId::new(0), complete_policy(ReservedBitsPolicy::Reject)),
        Err(EditError::ChunkLayoutRequired)
    );

    let source = encode_chunks(&[(custom_type().raw(), ChunkFlags::CRITICAL.bits(), b"opaque")]);
    let grants = [RawTypePolicy {
        chunk_type: custom_type(),
        policy: complete_policy(ReservedBitsPolicy::Reject),
    }];
    let mut document = Document::open_with(&source, &options_for(&grants)).unwrap();
    assert_eq!(
        document.set_raw_policy(ChunkId::new(9), complete_policy(ReservedBitsPolicy::Reject)),
        Err(EditError::InvalidChunkId)
    );

    let id = node(&document, 0).id;
    let before = (node(&document, 0).capability, document.is_dirty());
    assert_eq!(
        document.set_raw_policy(id, relocatable_policy(ReservedBitsPolicy::Reject)),
        Err(EditError::CriticalAssumptionRequired {
            chunk_type: custom_type(),
        })
    );
    assert_eq!((node(&document, 0).capability, document.is_dirty()), before);
}

#[test]
fn policy_edits_keep_source_borrowed_and_owned_payload_allocations() {
    let source = encode_chunks(&[(custom_type().raw(), 0, b"source")]);
    let borrowed_payload = b"borrowed";
    let owned_payload = Vec::from(&b"owned"[..]);
    let owned_pointer = owned_payload.as_ptr();
    let mut document = Document::open(&source).unwrap();
    let source_id = node(&document, 0).id;
    let borrowed_id = document
        .push_raw(RawChunkInput {
            chunk_type: custom_type(),
            flags: ChunkFlags::NONE,
            payload: PayloadInput::Borrowed(borrowed_payload),
            policy: complete_policy(ReservedBitsPolicy::Reject),
        })
        .unwrap();
    let owned_id = document
        .push_raw(RawChunkInput {
            chunk_type: custom_type(),
            flags: ChunkFlags::NONE,
            payload: PayloadInput::Owned(owned_payload),
            policy: complete_policy(ReservedBitsPolicy::Reject),
        })
        .unwrap();
    document.dirty = false;

    let vector_pointer = chunk_set(&document).chunks.as_ptr();
    let vector_capacity = chunk_set(&document).chunks.capacity();
    let pointers = [
        payload_bytes(&document, 0).as_ptr(),
        payload_bytes(&document, 1).as_ptr(),
        payload_bytes(&document, 2).as_ptr(),
    ];
    assert_eq!(pointers[1], borrowed_payload.as_ptr());
    assert_eq!(pointers[2], owned_pointer);

    for id in [source_id, borrowed_id, owned_id] {
        document
            .set_raw_policy(id, relocatable_policy(ReservedBitsPolicy::Reject))
            .unwrap();
    }

    assert!(!document.is_dirty());
    assert_eq!(chunk_set(&document).chunks.as_ptr(), vector_pointer);
    assert_eq!(chunk_set(&document).chunks.capacity(), vector_capacity);
    for (index, pointer) in pointers.into_iter().enumerate() {
        assert_eq!(payload_bytes(&document, index).as_ptr(), pointer);
        assert_capability(node(&document, index), true, false, false);
    }
}

#[test]
fn source_image_policy_reuses_the_absolute_payload_offset() {
    let source = offset_sensitive_image_source();
    Reader::open(&source).unwrap();
    let mut document = Document::open(&source).unwrap();
    let id = node(&document, 0).id;
    let payload_pointer = payload_bytes(&document, 0).as_ptr();

    document
        .set_raw_policy(id, RawChunkPolicy::infer())
        .unwrap();
    assert!(!document.is_dirty());
    document
        .set_flags(id, ChunkFlags::CRITICAL, RawChunkPolicy::infer())
        .unwrap();
    assert!(document.is_dirty());
    assert_eq!(node(&document, 0).flags, ChunkFlags::CRITICAL);
    assert_capability(node(&document, 0), true, true, false);
    assert_eq!(payload_bytes(&document, 0).as_ptr(), payload_pointer);
}
