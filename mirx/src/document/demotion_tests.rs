use alloc::borrow::Cow;
use alloc::vec;
use alloc::vec::Vec;

use super::*;
use crate::{
    CHUNK_FILE_HEADER_LEN, ColorFormat, CompatibilityPolicy, EncodeError, EncodeOptions,
    FlatImageInput, LayoutPolicy, OpenOptions, PayloadInput, RawChunkInput, RawChunkPolicy,
    RawTypePolicy, Reader, TrailingBytesPolicy, VERSION_MINOR, crc32, encode_flat,
};

const FORMATS: [ColorFormat; 16] = [
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

fn image_payload(
    format: ColorFormat,
    width: u32,
    height: u32,
    stride: u32,
    data_offset: u32,
) -> Vec<u8> {
    let main_len = usize::try_from(stride.checked_mul(height).unwrap()).unwrap();
    let extra_len = usize::try_from(format.extra_size(width, height, stride).unwrap()).unwrap();
    let data_start = usize::try_from(data_offset).unwrap();
    let mut payload = vec![0; data_start + main_len + extra_len];
    payload[0..4].copy_from_slice(&width.to_le_bytes());
    payload[4..8].copy_from_slice(&height.to_le_bytes());
    payload[8] = format.to_u8();
    payload[12..16].copy_from_slice(&stride.to_le_bytes());
    payload[16..20].copy_from_slice(&data_offset.to_le_bytes());
    payload[20..24].copy_from_slice(&u32::try_from(main_len + extra_len).unwrap().to_le_bytes());
    payload[24..28].copy_from_slice(&u32::try_from(extra_len).unwrap().to_le_bytes());
    for (index, byte) in payload[data_start..data_start + main_len]
        .iter_mut()
        .enumerate()
    {
        *byte = index as u8 ^ 0x5a;
    }
    for (index, byte) in payload[data_start + main_len..].iter_mut().enumerate() {
        *byte = index as u8 ^ 0xa5;
    }
    payload
}

fn push_image<'a>(document: &mut Document<'a>, payload: PayloadInput<'a>) -> ChunkId {
    let id = document
        .push_raw(RawChunkInput {
            chunk_type: ChunkType::IMAGE,
            flags: ChunkFlags::NONE,
            payload,
            policy: RawChunkPolicy::infer(),
        })
        .unwrap();
    document.set_primary(id).unwrap();
    id
}

fn push_opaque_image<'a>(document: &mut Document<'a>, payload: PayloadInput<'a>) -> ChunkId {
    let id = document
        .push_raw(RawChunkInput {
            chunk_type: ChunkType::IMAGE,
            flags: ChunkFlags::NONE,
            payload,
            policy: RawChunkPolicy {
                relocation: RelocationAssumption::AssumeRelocatable,
                critical_semantics: CriticalAssumption::Infer,
                reserved_flag_bits: ReservedBitsPolicy::Reject,
            },
        })
        .unwrap();
    document
        .set_primary_with_hints(id, PrimaryHints::ZERO)
        .unwrap();
    id
}

const fn relocatable_policy() -> RawChunkPolicy {
    RawChunkPolicy {
        relocation: RelocationAssumption::AssumeRelocatable,
        critical_semantics: CriticalAssumption::Infer,
        reserved_flag_bits: ReservedBitsPolicy::Reject,
    }
}

fn encoded_image_chunk(payload: &[u8]) -> Vec<u8> {
    let mut document = Document::new_chunk();
    push_image(&mut document, PayloadInput::Borrowed(payload));
    document.encode(&EncodeOptions::new()).unwrap()
}

fn encoded_layout(bytes: &[u8]) -> Layout {
    Reader::open(bytes).unwrap().layout()
}

#[test]
fn current_flat_is_an_exact_noop() {
    let source = encode_flat(&FlatImageInput {
        width: 2,
        height: 2,
        stride: ColorFormat::A8.minimum_stride(2).unwrap(),
        format: ColorFormat::A8,
        main: &[1, 2, 3, 4],
        extra: None,
    });
    let mut document = Document::open(&source).unwrap();
    let source_pointer = source.as_ptr();

    assert_eq!(document.demote_to_flat(), Ok(false));
    assert!(!document.is_dirty());
    assert_eq!(document.layout(), Layout::Flat);
    let Cow::Borrowed(finished) = document.finish().unwrap() else {
        panic!("clean borrowed FLAT source must remain borrowed");
    };
    assert_eq!(finished.as_ptr(), source_pointer);
    assert_eq!(finished, source);
}

#[test]
fn promoted_image_demotes_without_copying_and_ids_are_not_reused() {
    let source = encode_flat(&FlatImageInput {
        width: 2,
        height: 2,
        stride: ColorFormat::A8.minimum_stride(2).unwrap(),
        format: ColorFormat::A8,
        main: &[1, 2, 3, 4],
        extra: None,
    });
    let main_pointer = source[FLAT_HEADER_LEN..].as_ptr();
    let mut document = Document::open(&source).unwrap();
    let first_id = document.promote_to_chunk().unwrap().unwrap();

    assert_eq!(document.demote_to_flat(), Ok(true));
    assert_eq!(document.layout(), Layout::Flat);
    assert_eq!(document.flat_image().unwrap().main().as_ptr(), main_pointer);
    assert_eq!(document.next_id, 1);

    let second_id = document.promote_to_chunk().unwrap().unwrap();
    assert_ne!(second_id, first_id);
    assert_eq!(second_id, ChunkId::new(1));
}

#[test]
fn borrowed_and_owned_unplaced_payloads_keep_their_storage() {
    for data_offset in 32..=35 {
        let borrowed = image_payload(ColorFormat::A8, 2, 2, 2, data_offset);
        let expected_main = borrowed[usize::try_from(data_offset).unwrap()..].as_ptr();
        let mut document = Document::new_chunk();
        push_image(&mut document, PayloadInput::Borrowed(&borrowed));
        assert_eq!(document.demote_to_flat(), Ok(true));
        assert_eq!(
            document.flat_image().unwrap().main().as_ptr(),
            expected_main
        );
    }

    let owned = image_payload(ColorFormat::I4, 3, 2, 2, 35);
    let pointer = owned.as_ptr();
    let capacity = owned.capacity();
    let mut document = Document::new_chunk();
    push_image(&mut document, PayloadInput::Owned(owned));
    assert_eq!(document.demote_to_flat(), Ok(true));
    let DocumentState::Flat(record) = &document.state else {
        panic!("expected FLAT document");
    };
    let FlatStorage::Payload {
        backing: PayloadStorage::Owned(backing),
        main,
        extra,
    } = &record.storage
    else {
        panic!("owned IMAGE payload must remain one owned allocation");
    };
    assert_eq!(backing.as_ptr(), pointer);
    assert_eq!(backing.capacity(), capacity);
    assert_eq!(main.start(), 35);
    assert_eq!(extra.unwrap().start(), 39);
    assert_eq!(
        document.flat_image().unwrap().main().as_ptr() as usize,
        pointer as usize + 35
    );
}

#[test]
fn payload_backed_flat_survives_exact_replacement_and_forced_chunk_encoding() {
    let format = ColorFormat::I4;
    let width = 3;
    let height = 2;
    let stride = format.minimum_stride(width).unwrap();
    let payload = image_payload(format, width, height, stride, 35);
    let expected_main = payload[35..39].to_vec();
    let expected_extra = payload[39..].to_vec();
    let backing_pointer = payload.as_ptr();
    let backing_capacity = payload.capacity();
    let mut document = Document::new_chunk();
    push_image(&mut document, PayloadInput::Owned(payload));
    assert_eq!(document.demote_to_flat(), Ok(true));

    document.dirty = false;
    document
        .replace_flat_image(ImageAsset::new(
            width,
            height,
            format,
            stride,
            Cow::Borrowed(&expected_main),
            Some(Cow::Borrowed(&expected_extra)),
        ))
        .unwrap();
    assert!(!document.is_dirty());
    let DocumentState::Flat(record) = &document.state else {
        panic!("expected FLAT document");
    };
    let FlatStorage::Payload {
        backing: PayloadStorage::Owned(backing),
        ..
    } = &record.storage
    else {
        panic!("exact replacement must retain the owned payload backing");
    };
    assert_eq!(backing.as_ptr(), backing_pointer);
    assert_eq!(backing.capacity(), backing_capacity);

    let options = EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceChunk);
    let needed = document.encoded_len(&options).unwrap();
    let mut short = vec![0x5a; needed - 1];
    let short_before = short.clone();
    assert_eq!(
        document.encode_into(&mut short, &options),
        Err(EncodeError::BufferTooSmall {
            needed,
            available: needed - 1,
        })
    );
    assert_eq!(short, short_before);

    let mut output = vec![0xcc; needed + 7];
    assert_eq!(document.encode_into(&mut output, &options), Ok(needed));
    assert_eq!(&output[needed..], &[0xcc; 7]);
    let reader = Reader::open(&output[..needed]).unwrap();
    assert_eq!(reader.layout(), Layout::Chunk);
    let chunks = reader.chunks().collect::<Vec<_>>();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].chunk_type(), ChunkType::IMAGE);
    let image =
        ImageView::open_payload_at(chunks[0].payload(), chunks[0].payload_offset()).unwrap();
    assert_eq!(image.main(), expected_main);
    assert_eq!(image.extra(), Some(expected_extra.as_slice()));
}

#[test]
fn opened_source_payloads_keep_origin_allocation_and_exact_plane_ranges() {
    let data_offset = 33;
    let payload = image_payload(ColorFormat::I4, 3, 2, 2, data_offset);
    let source = encoded_image_chunk(&payload);
    let chunk = Reader::open(&source).unwrap().chunks().next().unwrap();
    let main_offset = usize::try_from(chunk.payload_offset() + data_offset).unwrap();
    let expected_main = source[main_offset..].as_ptr();
    let mut borrowed = Document::open(&source).unwrap();

    assert_eq!(borrowed.demote_to_flat(), Ok(true));
    assert_eq!(
        borrowed.flat_image().unwrap().main().as_ptr(),
        expected_main
    );
    let DocumentState::Flat(record) = &borrowed.state else {
        panic!("expected FLAT document");
    };
    let FlatStorage::Payload {
        backing: PayloadStorage::SourceRange(_),
        main,
        extra,
    } = &record.storage
    else {
        panic!("opened IMAGE payload must retain source-backed storage");
    };
    assert_eq!(main.start(), usize::try_from(data_offset).unwrap());
    assert_eq!(extra.unwrap().start(), main.start() + 4);

    let mut owned_source = Vec::with_capacity(source.len() + 41);
    owned_source.extend_from_slice(&source);
    let origin_pointer = owned_source.as_ptr();
    let origin_capacity = owned_source.capacity();
    let mut owned = Document::from_vec(owned_source).unwrap();
    assert_eq!(owned.demote_to_flat(), Ok(true));
    let Origin::Owned(origin) = &owned.origin else {
        panic!("owned source must remain owned");
    };
    assert_eq!(origin.as_ptr(), origin_pointer);
    assert_eq!(origin.capacity(), origin_capacity);
    assert_eq!(
        owned.flat_image().unwrap().main().as_ptr() as usize,
        origin_pointer as usize + main_offset
    );
}

#[test]
fn source_backed_images_keep_strict_absolute_alignment() {
    let payload = image_payload(ColorFormat::A8, 2, 2, 2, 33);
    let mut source = encoded_image_chunk(&payload);
    let entry = CHUNK_FILE_HEADER_LEN;
    let old_offset = u32::from_le_bytes(source[entry + 4..entry + 8].try_into().unwrap());
    source.insert(usize::try_from(old_offset).unwrap(), 0);
    let moved_offset = old_offset + 1;
    source[entry + 4..entry + 8].copy_from_slice(&moved_offset.to_le_bytes());
    let file_size = u32::try_from(source.len()).unwrap();
    source[16..20].copy_from_slice(&file_size.to_le_bytes());
    let checksum = crc32(&source[..40]);
    source[40..44].copy_from_slice(&checksum.to_le_bytes());

    let policies = [RawTypePolicy {
        chunk_type: ChunkType::IMAGE,
        policy: relocatable_policy(),
    }];
    let mut document = Document::open_with(
        &source,
        &OpenOptions::new().with_raw_type_policies(&policies),
    )
    .unwrap();
    let DocumentState::Chunk(chunks) = &document.state else {
        panic!("expected CHUNK document");
    };
    assert!(chunks.chunks[0].capability.is_relocatable());
    assert_eq!(
        document.demote_to_flat(),
        Err(EditError::NotRepresentableAsFlat)
    );
    assert_eq!(document.layout(), Layout::Chunk);
}

#[test]
fn every_color_format_demotes_and_reopens_with_identical_planes() {
    for format in FORMATS {
        let width = 3;
        let height = 2;
        let stride = format
            .minimum_stride(width)
            .unwrap()
            .checked_add(1)
            .unwrap();
        let payload = image_payload(format, width, height, stride, 33);
        let expected_main = payload[33..33 + usize::try_from(stride * height).unwrap()].to_vec();
        let expected_extra = payload[33 + expected_main.len()..].to_vec();
        let mut document = Document::new_chunk();
        push_image(&mut document, PayloadInput::Borrowed(&payload));

        assert_eq!(document.demote_to_flat(), Ok(true), "{format:?}");
        let encoded = document.encode(&EncodeOptions::new()).unwrap();
        let reopened = Reader::open(&encoded).unwrap().flat_image().unwrap();
        assert_eq!(reopened.format(), format);
        assert_eq!(reopened.width(), width);
        assert_eq!(reopened.height(), height);
        assert_eq!(reopened.stride(), stride);
        assert_eq!(reopened.main(), expected_main, "{format:?}");
        assert_eq!(
            reopened.extra().unwrap_or_default(),
            expected_extra,
            "{format:?}"
        );
    }
}

#[test]
fn writer_policies_share_the_lossless_candidate_without_mutating_state() {
    let payload = image_payload(ColorFormat::RGB565A8, 2, 2, 4, 35);
    let mut document = Document::new_chunk();
    let id = push_image(&mut document, PayloadInput::Borrowed(&payload));
    let next_id = document.next_id;

    let preserved = document.encode(&EncodeOptions::new()).unwrap();
    let forced_chunk = document
        .encode(&EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceChunk))
        .unwrap();
    let smallest = document
        .encode(&EncodeOptions::new().with_layout_policy(LayoutPolicy::SmallestRepresentable))
        .unwrap();
    let forced_flat = document
        .encode(&EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceFlat))
        .unwrap();

    assert_eq!(encoded_layout(&preserved), Layout::Chunk);
    assert_eq!(forced_chunk, preserved);
    assert_eq!(encoded_layout(&smallest), Layout::Flat);
    assert_eq!(forced_flat, smallest);
    assert_eq!(document.layout(), Layout::Chunk);
    assert_eq!(document.primary(), Some(id));
    assert_eq!(document.next_id, next_id);

    assert_eq!(document.demote_to_flat(), Ok(true));
    assert_eq!(document.encode(&EncodeOptions::new()).unwrap(), smallest);
}

#[test]
fn structural_and_payload_rejections_are_failure_atomic() {
    let valid = image_payload(ColorFormat::A8, 2, 2, 2, 32);
    let mut empty = Document::new_chunk();
    assert_eq!(
        empty.demote_to_flat(),
        Err(EditError::NotRepresentableAsFlat)
    );

    let mut no_primary = Document::new_chunk();
    no_primary
        .push_raw(RawChunkInput {
            chunk_type: ChunkType::IMAGE,
            flags: ChunkFlags::NONE,
            payload: PayloadInput::Borrowed(&valid),
            policy: RawChunkPolicy::infer(),
        })
        .unwrap();
    let next_id = no_primary.next_id;
    assert_eq!(
        no_primary.demote_to_flat(),
        Err(EditError::NotRepresentableAsFlat)
    );
    assert_eq!(no_primary.chunks().len(), 1);
    assert_eq!(no_primary.next_id, next_id);

    for malformed in malformed_payloads(&valid) {
        let mut document = Document::new_chunk();
        let id = push_opaque_image(&mut document, PayloadInput::Owned(malformed));
        let before_pointer = document.get(id).unwrap().payload_bytes().unwrap().as_ptr();
        let before = document.get(id).unwrap().payload_bytes().unwrap().to_vec();
        let next_id = document.next_id;
        assert_eq!(
            document.demote_to_flat(),
            Err(EditError::NotRepresentableAsFlat)
        );
        assert_eq!(document.layout(), Layout::Chunk);
        assert_eq!(document.primary(), Some(id));
        assert_eq!(document.next_id, next_id);
        assert_eq!(document.get(id).unwrap().payload_bytes().unwrap(), before);
        assert_eq!(
            document.get(id).unwrap().payload_bytes().unwrap().as_ptr(),
            before_pointer
        );
    }
}

#[test]
fn count_type_and_flags_cannot_be_discarded_by_demotion() {
    let valid = image_payload(ColorFormat::A8, 1, 1, 1, 32);

    let mut multiple = Document::new_chunk();
    let primary = push_image(&mut multiple, PayloadInput::Borrowed(&valid));
    multiple
        .push_raw(RawChunkInput {
            chunk_type: ChunkType::META,
            flags: ChunkFlags::NONE,
            payload: PayloadInput::Borrowed(&[]),
            policy: relocatable_policy(),
        })
        .unwrap();
    assert_eq!(
        multiple.demote_to_flat(),
        Err(EditError::NotRepresentableAsFlat)
    );
    assert_eq!(multiple.primary(), Some(primary));
    assert_eq!(multiple.chunks().len(), 2);

    let mut wrong_type = Document::new_chunk();
    let id = wrong_type
        .push_raw(RawChunkInput {
            chunk_type: ChunkType::META,
            flags: ChunkFlags::NONE,
            payload: PayloadInput::Borrowed(b"meta"),
            policy: relocatable_policy(),
        })
        .unwrap();
    wrong_type.set_primary(id).unwrap();
    assert_eq!(
        wrong_type.demote_to_flat(),
        Err(EditError::NotRepresentableAsFlat)
    );

    for flags in [ChunkFlags::CRITICAL, ChunkFlags::from_bits_retain(0x0002)] {
        let mut flagged = Document::new_chunk();
        let id = flagged
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::IMAGE,
                flags,
                payload: PayloadInput::Borrowed(&valid),
                policy: RawChunkPolicy {
                    relocation: RelocationAssumption::Infer,
                    critical_semantics: CriticalAssumption::Infer,
                    reserved_flag_bits: if flags == ChunkFlags::CRITICAL {
                        ReservedBitsPolicy::Reject
                    } else {
                        ReservedBitsPolicy::Preserve
                    },
                },
            })
            .unwrap();
        flagged.set_primary(id).unwrap();
        assert_eq!(
            flagged.demote_to_flat(),
            Err(EditError::NotRepresentableAsFlat)
        );
        assert_eq!(flagged.get(id).unwrap().flags(), flags);
    }

    let flat = encode_flat(&FlatImageInput {
        width: 1,
        height: 1,
        stride: 1,
        format: ColorFormat::A8,
        main: &[7],
        extra: None,
    });
    let mut promoted = Document::open(&flat).unwrap();
    let id = promoted.promote_to_chunk().unwrap().unwrap();
    let DocumentState::Chunk(chunks) = &mut promoted.state else {
        panic!("expected CHUNK document");
    };
    chunks.chunks[0].chunk_type = ChunkType::META;
    assert_eq!(
        promoted.demote_to_flat(),
        Err(EditError::NotRepresentableAsFlat)
    );
    assert_eq!(promoted.get(id).unwrap().chunk_type(), ChunkType::META);
}

fn malformed_payloads(valid: &[u8]) -> Vec<Vec<u8>> {
    let mut cases = Vec::new();
    cases.push(valid[..10].to_vec());

    let mut reserved = valid.to_vec();
    reserved[10] = 1;
    cases.push(reserved);

    let mut compressed = valid.to_vec();
    compressed[9] = 1;
    cases.push(compressed);

    let mut unknown = valid.to_vec();
    unknown[8] = 0xff;
    cases.push(unknown);

    let mut small_stride = valid.to_vec();
    small_stride[8] = ColorFormat::RGB565.to_u8();
    small_stride[0..4].copy_from_slice(&3u32.to_le_bytes());
    small_stride[12..16].copy_from_slice(&5u32.to_le_bytes());
    cases.push(small_stride);

    let mut early_data = valid.to_vec();
    early_data[16..20].copy_from_slice(&31u32.to_le_bytes());
    cases.push(early_data);

    let mut padding = valid.to_vec();
    padding[16..20].copy_from_slice(&33u32.to_le_bytes());
    padding.insert(32, 1);
    cases.push(padding);

    let mut extra_size = valid.to_vec();
    extra_size[24..28].copy_from_slice(&1u32.to_le_bytes());
    cases.push(extra_size);

    let mut data_size = valid.to_vec();
    data_size[20..24].copy_from_slice(&5u32.to_le_bytes());
    cases.push(data_size);

    let mut trailing = valid.to_vec();
    trailing.push(0);
    cases.push(trailing);

    let mut overflow = valid.to_vec();
    overflow[8] = ColorFormat::RGBA8888.to_u8();
    overflow[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
    overflow[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    cases.push(overflow);
    cases
}

#[test]
fn nonrepresentable_smallest_falls_back_and_force_flat_is_atomic() {
    let payload = image_payload(ColorFormat::A8, 1, 1, 1, 32);
    let mut document = Document::new_chunk();
    push_image(&mut document, PayloadInput::Borrowed(&payload));
    document
        .push_raw(RawChunkInput {
            chunk_type: ChunkType::META,
            flags: ChunkFlags::NONE,
            payload: PayloadInput::Borrowed(b"meta"),
            policy: RawChunkPolicy {
                relocation: RelocationAssumption::AssumeRelocatable,
                critical_semantics: CriticalAssumption::Infer,
                reserved_flag_bits: ReservedBitsPolicy::Reject,
            },
        })
        .unwrap();

    let default = document.encode(&EncodeOptions::new()).unwrap();
    let smallest = document
        .encode(&EncodeOptions::new().with_layout_policy(LayoutPolicy::SmallestRepresentable))
        .unwrap();
    assert_eq!(smallest, default);

    let options = EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceFlat);
    let mut output = vec![0x5a; default.len() + 16];
    let before = output.clone();
    assert_eq!(
        document.encode_into(&mut output, &options),
        Err(EncodeError::NotRepresentableAsFlat)
    );
    assert_eq!(output, before);
}

#[test]
fn global_write_blockers_precede_layout_and_representability() {
    let payload = image_payload(ColorFormat::A8, 1, 1, 1, 32);
    let source = encoded_image_chunk(&payload);

    let mut future_with_tail = source.clone();
    future_with_tail[5] = VERSION_MINOR + 1;
    let checksum = crc32(&future_with_tail[..40]);
    future_with_tail[40..44].copy_from_slice(&checksum.to_le_bytes());
    future_with_tail.extend_from_slice(b"tail");
    let mut future = Document::open_with(
        &future_with_tail,
        &OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve),
    )
    .unwrap();
    assert_eq!(
        future.demote_to_flat(),
        Err(EditError::FutureSemanticsReadOnly)
    );

    let mut with_tail = source.clone();
    with_tail.extend_from_slice(b"tail");
    let mut trailing = Document::open_with(
        &with_tail,
        &OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve),
    )
    .unwrap();
    assert_eq!(
        trailing.demote_to_flat(),
        Err(EditError::PreservedTrailingBytesReadOnly)
    );
    trailing.discard_trailing_bytes().unwrap();
    assert_eq!(trailing.demote_to_flat(), Ok(true));

    let mut future_source = source;
    future_source[5] = VERSION_MINOR + 1;
    let checksum = crc32(&future_source[..40]);
    future_source[40..44].copy_from_slice(&checksum.to_le_bytes());
    let mut normalized = Document::open_with(
        &future_source,
        &OpenOptions::new().with_compatibility(CompatibilityPolicy::NormalizeToCurrent),
    )
    .unwrap();
    assert_eq!(normalized.file, FileMeta::CURRENT);
    assert_eq!(normalized.demote_to_flat(), Ok(true));

    let mut flat_with_tail = encode_flat(&FlatImageInput {
        width: 1,
        height: 1,
        stride: 1,
        format: ColorFormat::A8,
        main: &[1],
        extra: None,
    });
    flat_with_tail.extend_from_slice(b"tail");
    let mut flat = Document::open_with(
        &flat_with_tail,
        &OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve),
    )
    .unwrap();
    assert_eq!(
        flat.demote_to_flat(),
        Err(EditError::PreservedTrailingBytesReadOnly)
    );
}

#[test]
fn zero_geometry_uses_the_existing_format_rules() {
    for format in [ColorFormat::A8, ColorFormat::I4, ColorFormat::RGB565A8] {
        let payload = image_payload(format, 0, 0, 0, 32);
        let mut document = Document::new_chunk();
        push_image(&mut document, PayloadInput::Borrowed(&payload));
        assert_eq!(document.demote_to_flat(), Ok(true));
        let image = document.flat_image().unwrap();
        assert!(image.main().is_empty());
        assert_eq!(
            image.extra().map_or(0, <[u8]>::len),
            usize::try_from(format.extra_size(0, 0, 0).unwrap()).unwrap()
        );
    }
}

#[test]
fn flat_candidate_uses_payload_metadata_instead_of_stale_hints() {
    let payload = image_payload(ColorFormat::A8, 2, 3, 2, 32);
    let mut source = encoded_image_chunk(&payload);
    source[22] = 0xff;
    source[24..28].copy_from_slice(&9u32.to_le_bytes());
    source[28..32].copy_from_slice(&8u32.to_le_bytes());
    source[32..36].copy_from_slice(&7u32.to_le_bytes());
    let checksum = crc32(&source[..40]);
    source[40..44].copy_from_slice(&checksum.to_le_bytes());
    let mut document = Document::open(&source).unwrap();

    assert!(document.primary().is_some());
    assert_eq!(document.demote_to_flat(), Ok(true));
    assert_eq!(
        document.primary_hints(),
        PrimaryHints::new(ColorFormat::A8.to_u8(), 2, 3, 2)
    );
}

#[test]
fn flat_header_crc_is_recomputed_after_demotion() {
    let payload = image_payload(ColorFormat::A8, 1, 2, 1, 35);
    let mut document = Document::new_chunk();
    push_image(&mut document, PayloadInput::Borrowed(&payload));
    document.demote_to_flat().unwrap();
    let encoded = document.encode(&EncodeOptions::new()).unwrap();
    assert_eq!(
        u32::from_le_bytes(encoded[24..28].try_into().unwrap()),
        crc32(&encoded[..24])
    );
}
