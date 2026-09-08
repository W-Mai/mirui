mod support;

use mirx::meta::Meta;
use mirx::{
    ChunkFlags, ChunkType, Document, Reader,
    coding::{CodingId, CodingRecord, Frequency, FrequencyGeometry, Lz4, Pixel, Rle},
    document::{EditError, EncodeOptions, PayloadOrigin},
    extension::{Critical, Extension, Policy, Relocation, ReservedFlags},
    image::{
        ColorDescription, CoverageBudget, EncodedImageAsset, EncodedImageError, ImageReadError,
        SampleLayout, SurfaceDescriptor, SurfaceRequirements, UnitDecodeError,
    },
    reader::{PayloadLimits, PayloadLocation, PayloadValidationFailure, ReadError, ReadOptions},
    types::PayloadError,
};
use support::encode_chunks;

#[test]
fn frequency_profiles_round_trip_through_container_preflight_and_decode() {
    let surface =
        SurfaceDescriptor::new(13, 9, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
    let geometry = FrequencyGeometry::for_plane(SampleLayout::RGBA8888, 0, 13, 9).unwrap();
    let samples: Vec<u8> = (0..geometry.decoded_len().unwrap())
        .map(|index| ((index * 43 + index / 11 * 29 + 7) & 0xff) as u8)
        .collect();
    for codec in [Frequency::reversible(), Frequency::quantized(75).unwrap()] {
        let mut stream = vec![0; codec.encoded_len(geometry, &samples).unwrap()];
        codec.encode_into(geometry, &samples, &mut stream).unwrap();
        let mut params = [0];
        let asset = EncodedImageAsset::new(surface, codec.record_into(&mut params), &stream);
        asset.preflight(&PayloadLimits::HOST).unwrap();
        assert!(
            asset
                .preflight(&PayloadLimits::HOST.with_max_raster_work(1))
                .is_err()
        );
        let payload = asset.encode().unwrap();
        let image = mirx::image::EncodedImageView::open(&payload).unwrap();
        image.preflight(&PayloadLimits::HOST).unwrap();
        let mut slots = [None];
        let groups = image
            .groups_into(&mut slots, &mut CoverageBudget::new(256))
            .unwrap();
        let plan = groups
            .decode_plan(
                SurfaceRequirements::new().with_stride_multiple(64),
                &PayloadLimits::HOST,
            )
            .unwrap();
        assert!(
            groups
                .decode_plan(
                    SurfaceRequirements::new().with_stride_multiple(64),
                    &PayloadLimits::HOST.with_max_raster_work(plan.work() - 1),
                )
                .is_err()
        );
        groups
            .decode_plan(
                SurfaceRequirements::new().with_stride_multiple(64),
                &PayloadLimits::HOST.with_max_raster_work(plan.work()),
            )
            .unwrap();
        assert_eq!(plan.workspace_requirements().byte_len(), 468);
        let mut output = vec![0xa5; plan.memory_plan().byte_len() as usize + 3];
        let mut workspace = vec![0xa5; plan.workspace_requirements().byte_len()];
        let decoded = plan.decode_into(&mut output, &mut workspace).unwrap();
        let plane = decoded.plane(0).unwrap();
        let mut changed = false;
        for row in 0..9 {
            let expected = &samples[row as usize * 52..row as usize * 52 + 52];
            let actual = plane.row(row).unwrap().unwrap();
            for (before, after) in expected.chunks_exact(4).zip(actual.chunks_exact(4)) {
                assert_eq!(before[3], after[3]);
                changed |= before[..3] != after[..3];
            }
        }
        assert_eq!(changed, !codec.is_reversible());
        assert_eq!(
            &output[plan.memory_plan().byte_len() as usize..],
            &[0xa5; 3]
        );
    }
}

#[test]
fn grouped_profiles_round_trip_typed_edits_and_independent_aligned_tiles() {
    use mirx::image::{GroupSelection, Region, UnitGroupRecord};
    use mirx::types::DataIntegrity;
    let surface =
        SurfaceDescriptor::new(8, 1, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
    let pixel = Pixel::new(SampleLayout::RGB888).unwrap();
    let rle = Rle::new().with_element_size(3).unwrap();
    let lz4 = Lz4::new();
    let codings = [
        pixel.record(),
        rle.record(),
        lz4.record(),
        CodingRecord::RAW,
    ];
    let samples = [
        [17, 42, 91, 17, 42, 91],
        [0, 1, 2, 0, 1, 2],
        [55, 56, 57, 55, 56, 57],
        [5, 17, 29, 47, 83, 131],
    ];
    let mut data = [0xa5; 256];
    data[192..198].copy_from_slice(&samples[3]);
    let mut table = [0; Lz4::TABLE_LEN];
    let lengths = [
        pixel.encode_into(&samples[0], &mut data[..64]).unwrap(),
        rle.encode_into(&samples[1], &mut data[64..128]).unwrap(),
        lz4.encoder(&mut table)
            .unwrap()
            .encode_into(&samples[2], &mut data[128..192])
            .unwrap(),
        samples[3].len(),
    ];
    let records: [_; 4] = core::array::from_fn(|i| {
        UnitGroupRecord::new(i as u32, (64 * i) as u32..(64 * i + lengths[i]) as u32)
            .unwrap()
            .with_tiles(2, 1)
            .with_selection(GroupSelection::List(1))
            .with_index_offset((i * 4) as u32)
            .with_input_alignment(mirx::types::ByteAlignment::new(64).unwrap())
    });
    let index = [0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0];
    let asset =
        EncodedImageAsset::from_groups(surface, &codings, &records, &data[..192 + lengths[3]])
            .with_unit_index(&index)
            .with_integrity(DataIntegrity::Indexed(&[64, 128, 192, 198]));
    let mut document = Document::new();
    let id = document
        .push_encoded_image_with_flags(&asset, ChunkFlags::CRITICAL)
        .unwrap();
    document.set_primary(id).unwrap();
    let meta = document.push_meta(&Meta::default()).unwrap();
    document.move_before(meta, id).unwrap();
    let bytes = document.encode(&EncodeOptions::new()).unwrap();
    let reader = Reader::open(&bytes).unwrap();
    let chunk = reader
        .chunks()
        .find(|c| c.chunk_type() == ChunkType::IMAGE)
        .unwrap();
    let image = chunk.image().unwrap().unwrap().encoded().unwrap();
    assert_eq!(image.codings().iter().collect::<Vec<_>>(), codings);
    image.validate_data().unwrap();
    let mut slots = [None; 4];
    let groups = image
        .groups_into(&mut slots, &mut CoverageBudget::new(4096))
        .unwrap();
    #[repr(align(64))]
    struct Aligned([u8; 128]);
    let whole_plan = surface
        .memory_plan(
            SurfaceRequirements::new()
                .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
                .with_stride_multiple(64),
        )
        .unwrap();
    let mut whole = Aligned([0x5a; 128]);
    let mut output = Aligned([0xad; 128]);
    for (ordinal, expected) in samples.iter().enumerate() {
        let unit = groups.get(ordinal).unwrap().get(0).unwrap();
        assert_eq!(
            groups.validate_unit(ordinal, 0),
            Ok(if ordinal == 3 { 6 } else { 64 })
        );
        assert_eq!(
            unit.region(),
            Region::new(ordinal as u32 * 2, 0, 2, 1).unwrap()
        );
        assert_eq!(unit.coding(), codings[ordinal]);
        let requirements = SurfaceRequirements::new()
            .with_stride_multiple(64)
            .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap());
        let plan = unit.decode_plan(requirements).unwrap();
        let decoded = plan.decode_into(&mut output.0).unwrap();
        assert_eq!(
            decoded.plane(0).unwrap().row(0).unwrap(),
            Some(expected.as_slice())
        );
        decoded.copy_into(&mut whole.0, whole_plan).unwrap();
        assert_eq!(&output.0[6..64], &[0; 58]);
        assert_eq!(&output.0[64..], &[0xad; 64]);
    }
    assert_eq!(&whole.0[..24], samples.as_flattened());
    assert_eq!(&whole.0[24..], &[0x5a; 104]);
    let plan = groups
        .decode_plan(whole_plan.requirements(), &PayloadLimits::EMBEDDED)
        .unwrap();
    assert_eq!(plan.unit_count(), 4);
    assert_eq!(plan.workspace_requirements().byte_len(), 6);
    let mut combined = Aligned([0xad; 128]);
    let mut workspace = [0x5a; 12];
    let view = plan.decode_into(&mut combined.0, &mut workspace).unwrap();
    assert_eq!(
        view.plane(0).unwrap().row(0).unwrap(),
        Some(samples.as_flattened())
    );
    assert_eq!(&combined.0[24..64], &[0; 40]);
    assert_eq!(&combined.0[64..], &[0xad; 64]);
    assert_eq!(&workspace[6..], &[0x5a; 6]);
    let region = groups
        .decode_region_plan(
            surface.region(2, 0, 4, 1).unwrap(),
            whole_plan.requirements(),
            &PayloadLimits::EMBEDDED,
        )
        .unwrap();
    assert_eq!(region.unit_count(), 2);
    assert_eq!(region.input_byte_len(), (lengths[1] + lengths[2]) as u64);
    assert_eq!(region.checksum_byte_len(), 128);
    let mut cropped = Aligned([0xad; 128]);
    let view = region.decode_into(&mut cropped.0, &mut workspace).unwrap();
    assert_eq!(
        view.plane(0).unwrap().row(0).unwrap(),
        Some(&samples.as_flattened()[6..18])
    );
    assert_eq!(&cropped.0[12..64], &[0; 52]);
    assert_eq!(&cropped.0[64..], &[0xad; 64]);
    let mut document = Document::open(&bytes).unwrap();
    let id = document
        .chunks_of_type(ChunkType::IMAGE)
        .next()
        .unwrap()
        .id();
    document
        .get_mut(id)
        .unwrap()
        .replace_encoded_image(&asset)
        .unwrap();
    assert!(!document.is_dirty());
    assert_eq!(
        document.primary_hints().sample_layout(),
        SampleLayout::RGB888
    );
    assert_eq!(document.primary_hints().stride(), 0);
}

#[test]
fn document_relocation_preserves_encoded_storage_alignment_and_derived_hints() {
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    for alignment in [1, 4, 16, 64, 256] {
        let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42])
            .with_input_alignment(mirx::types::ByteAlignment::new(alignment).unwrap())
            .encode()
            .unwrap();
        let mut document = Document::new();
        let id = document
            .push_extension(
                Extension::borrowed(ChunkType::IMAGE, &payload).with_flags(ChunkFlags::CRITICAL),
            )
            .unwrap();
        document.set_primary(id).unwrap();
        let image = document
            .get(id)
            .unwrap()
            .image()
            .unwrap()
            .encoded()
            .unwrap();
        assert_eq!(
            image.input_alignment().map(mirx::types::ByteAlignment::get),
            Ok(alignment)
        );
        assert_eq!(
            document.get(id).unwrap().payload_bytes().unwrap().as_ptr(),
            payload.as_ptr()
        );
        assert_eq!(
            document.get(id).unwrap().payload_origin(),
            PayloadOrigin::BORROWED
        );
        assert_eq!(document.primary_hints().stride(), 0);
        assert_eq!(document.primary_hints().width(), 8);
        assert_eq!(document.primary_hints().sample_layout(), SampleLayout::A8);
        let meta = document.push_meta(&Meta::default()).unwrap();
        document.move_before(meta, id).unwrap();
        let bytes = document.encode(&EncodeOptions::new()).unwrap();
        let mut reopened = Document::open(&bytes).unwrap();
        let id = reopened
            .chunks_of_type(ChunkType::IMAGE)
            .next()
            .unwrap()
            .id();
        assert_eq!(
            reopened.get(id).unwrap().payload_bytes(),
            Some(payload.as_slice())
        );
        assert_eq!(
            reopened.get(id).unwrap().payload_origin(),
            PayloadOrigin::ORIGINAL_SOURCE
        );
        let meta = reopened
            .chunks_of_type(ChunkType::META)
            .next()
            .unwrap()
            .id();
        reopened.remove(meta).unwrap();
        reopened
            .get_mut(id)
            .unwrap()
            .set_flags(ChunkFlags::NONE)
            .unwrap();
        assert_eq!(
            reopened.demote_to_flat(),
            Err(EditError::NotRepresentableAsFlat)
        );
        let bytes = reopened.encode(&EncodeOptions::new()).unwrap();
        let reader = Reader::open(&bytes).unwrap();
        reader
            .validate_known_payloads(&PayloadLimits::EMBEDDED)
            .unwrap();
        let chunk = reader.chunks().next().unwrap();
        assert_eq!(chunk.payload(), payload);
        let image = chunk.image().unwrap().unwrap().encoded().unwrap();
        image
            .validate_groups(&mut CoverageBudget::new(4096))
            .unwrap();
    }
}

#[test]
fn document_inference_checks_encoded_syntax_and_limits_before_mutation() {
    use mirx::image::ImagePayloadError;
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let valid = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42])
        .encode()
        .unwrap();
    let malformed = EncodedImageAsset::new(surface, Rle::new().record(), &[0xff])
        .encode()
        .unwrap();
    let unknown =
        EncodedImageAsset::new(surface, CodingRecord::new(CodingId::new(511), 1, &[]), &[1])
            .encode()
            .unwrap();
    for (payload, limits) in [
        (&malformed, PayloadLimits::EMBEDDED),
        (&unknown, PayloadLimits::EMBEDDED),
        (&valid, PayloadLimits::EMBEDDED.with_max_decoded_bytes(7)),
        (&valid, PayloadLimits::EMBEDDED.with_max_raster_groups(0)),
        (&valid, PayloadLimits::EMBEDDED.with_max_raster_units(0)),
        (&valid, PayloadLimits::EMBEDDED.with_max_raster_work(0)),
    ] {
        let mut document = Document::new_with_limits(limits);
        let before = document.encode(&EncodeOptions::new()).unwrap();
        assert!(matches!(
            document.push_extension(Extension::borrowed(ChunkType::IMAGE, payload)),
            Err(EditError::InvalidPayload(ImagePayloadError::Encoded(_)))
        ));
        assert_eq!(document.encode(&EncodeOptions::new()).unwrap(), before);
        assert_eq!(document.chunks().count(), 0);
    }
}

#[test]
fn opaque_encoded_relocation_remains_an_explicit_policy_and_keeps_alignment() {
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let payload =
        EncodedImageAsset::new(surface, CodingRecord::new(CodingId::new(511), 1, &[]), &[1])
            .with_input_alignment(mirx::types::ByteAlignment::new(64).unwrap())
            .encode()
            .unwrap();
    let mut document = Document::new();
    let id = document
        .push_extension(
            Extension::borrowed(ChunkType::IMAGE, &payload).with_policy(Policy {
                relocation: Relocation::AssumeRelocatable,
                critical_semantics: Critical::Infer,
                reserved_flag_bits: ReservedFlags::Reject,
            }),
        )
        .unwrap();
    assert_eq!(
        document.set_primary(id),
        Err(EditError::PrimaryHintsRequired {
            chunk_type: ChunkType::IMAGE
        })
    );
    assert_eq!(
        document.get_mut(id).unwrap().replace_extension(
            Extension::borrowed(ChunkType::IMAGE, &payload)
                .with_flags(ChunkFlags::CRITICAL)
                .with_policy(Policy {
                    relocation: Relocation::AssumeRelocatable,
                    critical_semantics: Critical::Infer,
                    reserved_flag_bits: ReservedFlags::Reject,
                })
        ),
        Err(EditError::CriticalAssumptionRequired {
            chunk_type: ChunkType::IMAGE
        })
    );
    let bytes = document.encode(&EncodeOptions::new()).unwrap();
    let reader = Reader::open(&bytes).unwrap();
    let chunk = reader.chunks().next().unwrap();
    assert_eq!(chunk.payload(), payload);
    let image = chunk.image().unwrap().unwrap().encoded().unwrap();
    assert_eq!(
        image.input_alignment().map(mirx::types::ByteAlignment::get),
        Ok(64)
    );
    image
        .validate_groups(&mut CoverageBudget::new(100))
        .unwrap();
    assert!(
        reader
            .validate_known_payloads(&PayloadLimits::EMBEDDED)
            .is_err()
    );
}

#[test]
fn critical_scalar_images_reach_aligned_caller_output_through_document_and_reader() {
    let surface =
        SurfaceDescriptor::new(3, 2, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
    let samples = [
        17, 42, 91, 17, 42, 91, 0, 1, 2, 0, 1, 2, 55, 56, 57, 55, 56, 57,
    ];
    let pixel = Pixel::new(SampleLayout::RGB888).unwrap();
    let rle = Rle::new().with_element_size(3).unwrap();
    let lz4 = Lz4::new();
    let mut table = [0; Lz4::TABLE_LEN];
    #[repr(align(64))]
    struct Output([u8; 192]);
    for coding in [pixel.record(), rle.record(), lz4.record()] {
        let mut output = Output([0xad; 192]);
        let decoded = {
            let mut stream = [0; 64];
            let size = match coding.id() {
                CodingId::PIXEL => pixel.encode_into(&samples, &mut stream).unwrap(),
                CodingId::RLE => rle.encode_into(&samples, &mut stream).unwrap(),
                _ => lz4
                    .encoder(&mut table)
                    .unwrap()
                    .encode_into(&samples, &mut stream)
                    .unwrap(),
            };
            let mut document = Document::new();
            let id = document
                .push_encoded_image_with_flags(
                    &EncodedImageAsset::new(surface, coding, &stream[..size]),
                    ChunkFlags::CRITICAL,
                )
                .unwrap();
            document.set_primary(id).unwrap();
            let bytes = document.encode(&EncodeOptions::new()).unwrap();
            let reader = Reader::open(&bytes).unwrap();
            reader
                .validate_known_payloads(&PayloadLimits::EMBEDDED)
                .unwrap();
            let image = reader.chunks().next().unwrap().image().unwrap().unwrap();
            assert_eq!(image.surface(), surface);
            assert!(image.raw().is_none());
            let mut slots = [None];
            let groups = image
                .encoded()
                .unwrap()
                .groups_into(&mut slots, &mut CoverageBudget::new(100))
                .unwrap();
            assert_eq!(groups.validate_unit(0, 0), Ok(size as u32));
            let unit = groups.get(0).unwrap().get(0).unwrap();
            assert_eq!(unit.data(), &stream[..size]);
            unit.decode_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
                    .with_plane_alignment(mirx::types::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
            )
            .unwrap()
            .decode_into(&mut output.0)
            .unwrap()
        };
        let plane = decoded.plane(0).unwrap();
        assert!(plane.address_is_aligned());
        assert_eq!(plane.memory().stride(), 64);
        assert_eq!(plane.row(0).unwrap(), Some(&samples[..9]));
        assert_eq!(plane.row(1).unwrap(), Some(&samples[9..]));
        assert_eq!(&output.0[128..], &[0xad; 64]);
    }
}

#[test]
fn lazy_encoded_inspection_does_not_bypass_critical_or_explicit_preflight() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let coding = CodingRecord::new(CodingId::new(511), 7, &[13]);
    let payload = EncodedImageAsset::new(surface, coding, &[1])
        .encode()
        .unwrap();
    let bytes = encode_chunks(&[(ChunkType::IMAGE.raw(), 0, &payload)]);
    let reader = Reader::open(&bytes).unwrap();
    let chunk = reader.chunks().next().unwrap();
    let image = chunk.image().unwrap().unwrap().encoded().unwrap();
    assert_eq!(image.codings().get(0), Some(coding));
    let error = reader
        .validate_known_payloads(&PayloadLimits::EMBEDDED)
        .unwrap_err();
    assert_eq!(
        error.location(),
        PayloadLocation::Chunk {
            index: 0,
            chunk_type: ChunkType::IMAGE,
            payload_offset: chunk.payload_offset()
        }
    );
    assert_eq!(
        error.failure(),
        PayloadValidationFailure::Image(ImageReadError::Encoded(EncodedImageError::Coding {
            group: 0,
            error: UnitDecodeError::UnsupportedCoding(coding.id())
        }))
    );
    let critical = encode_chunks(&[(
        ChunkType::IMAGE.raw(),
        ChunkFlags::CRITICAL.bits(),
        &payload,
    )]);
    assert!(matches!(
        Reader::open(&critical),
        Err(ReadError::CriticalPayload(_))
    ));
    let malformed = EncodedImageAsset::new(surface, Rle::new().record(), &[0xff])
        .encode()
        .unwrap();
    let bytes = encode_chunks(&[(
        ChunkType::IMAGE.raw(),
        ChunkFlags::CRITICAL.bits(),
        &malformed,
    )]);
    let Err(ReadError::CriticalPayload(error)) = Reader::open(&bytes) else {
        panic!("malformed coding was accepted")
    };
    assert!(matches!(
        error.failure(),
        PayloadValidationFailure::Image(ImageReadError::Encoded(EncodedImageError::Unit {
            group: 0,
            ordinal: 0,
            ..
        }))
    ));
}

#[test]
fn reader_limits_and_data_integrity_are_enforced_before_critical_success() {
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let mut payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42])
        .encode()
        .unwrap();
    let bytes = encode_chunks(&[(
        ChunkType::IMAGE.raw(),
        ChunkFlags::CRITICAL.bits(),
        &payload,
    )]);
    for limits in [
        PayloadLimits::EMBEDDED.with_max_decoded_bytes(7),
        PayloadLimits::EMBEDDED.with_max_raster_groups(0),
        PayloadLimits::EMBEDDED.with_max_raster_units(0),
        PayloadLimits::EMBEDDED.with_max_raster_work(0),
    ] {
        assert!(matches!(
            Reader::open_with(&bytes, &ReadOptions::new().with_payload_limits(limits)),
            Err(ReadError::CriticalPayload(_))
        ));
    }
    // A literal value changes without invalidating the stream's shape.
    let data = payload.len() - 5;
    payload[data] ^= 1;
    let bytes = encode_chunks(&[(
        ChunkType::IMAGE.raw(),
        ChunkFlags::CRITICAL.bits(),
        &payload,
    )]);
    let Err(ReadError::CriticalPayload(error)) = Reader::open(&bytes) else {
        panic!("corrupt data was accepted")
    };
    assert!(matches!(
        error.failure(),
        PayloadValidationFailure::Image(ImageReadError::Encoded(EncodedImageError::Media(
            PayloadError::DataCrcMismatch { .. }
        )))
    ));
}

#[test]
fn encoded_input_alignment_uses_the_chunk_position() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 42])
        .with_input_alignment(mirx::types::ByteAlignment::new(64).unwrap())
        .encode()
        .unwrap();
    let unaligned = encode_chunks(&[(
        ChunkType::IMAGE.raw(),
        ChunkFlags::CRITICAL.bits(),
        &payload,
    )]);
    let Err(ReadError::CriticalPayload(error)) = Reader::open(&unaligned) else {
        panic!("unaligned input was accepted")
    };
    assert!(matches!(
        error.failure(),
        PayloadValidationFailure::Image(ImageReadError::Encoded(
            EncodedImageError::FileAddressUnaligned { alignment, .. }
        )) if alignment == 64
    ));
    let bytes = encode_chunks(&[
        (0xbeef, 0, &[0; 52]),
        (
            ChunkType::IMAGE.raw(),
            ChunkFlags::CRITICAL.bits(),
            &payload,
        ),
    ]);
    let reader = Reader::open(&bytes).unwrap();
    let mut chunks = reader.chunks();
    assert_eq!(chunks.next().unwrap().image(), Ok(None));
    let image = chunks.next().unwrap();
    assert_eq!(image.payload_offset(), 128);
    assert!(image.image().unwrap().unwrap().encoded().is_some());
}

#[test]
fn typed_encoded_replacement_refreshes_primary_and_repairs_source_placement() {
    use mirx::image::RawImageAsset;
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let asset = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42])
        .with_input_alignment(mirx::types::ByteAlignment::new(64).unwrap());
    let payload = asset.encode().unwrap();
    // This low-level container has not satisfied the IMAGE alignment promise.
    let bytes = encode_chunks(&[(ChunkType::IMAGE.raw(), 0, &payload)]);
    let mut document = Document::open(&bytes).unwrap();
    let id = document.chunks().next().unwrap().id();
    assert!(!document.is_dirty());
    document
        .get_mut(id)
        .unwrap()
        .replace_encoded_image(&asset)
        .unwrap();
    assert!(document.is_dirty());
    assert_eq!(
        document.get(id).unwrap().payload_origin(),
        PayloadOrigin::OWNED
    );
    document.set_primary(id).unwrap();
    assert_eq!(document.primary_hints().stride(), 0);
    let bytes = document.encode(&EncodeOptions::new()).unwrap();
    let mut document = Document::open(&bytes).unwrap();
    let id = document.chunks().next().unwrap().id();
    let pointer = document.get(id).unwrap().payload_bytes().unwrap().as_ptr();
    document
        .get_mut(id)
        .unwrap()
        .replace_encoded_image(&asset)
        .unwrap();
    assert!(!document.is_dirty());
    assert_eq!(
        document.get(id).unwrap().payload_bytes().unwrap().as_ptr(),
        pointer
    );

    let large = SurfaceDescriptor::new(
        3,
        2,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let replacement = EncodedImageAsset::new(large, Rle::new().record(), &[0x89, 128]);
    document
        .get_mut(id)
        .unwrap()
        .replace_encoded_image(&replacement)
        .unwrap();
    assert_eq!(document.primary(), Some(id));
    assert_eq!(document.primary_hints().sample_layout(), SampleLayout::NV12);
    assert_eq!(document.primary_hints().width(), 3);
    assert_eq!(document.primary_hints().height(), 2);
    assert_eq!(document.primary_hints().stride(), 0);
    let output = document.encode(&EncodeOptions::new()).unwrap();
    Reader::open(&output)
        .unwrap()
        .validate_known_payloads(&PayloadLimits::EMBEDDED)
        .unwrap();
    document
        .get_mut(id)
        .unwrap()
        .replace_image(&RawImageAsset::new(surface, &[&[42; 8]]))
        .unwrap();
    assert!(document.get(id).unwrap().image().unwrap().raw().is_some());
    assert_eq!(document.primary_hints().stride(), 8);
}

#[test]
fn typed_encoded_edit_failures_leave_flat_and_primary_storage_unchanged() {
    use mirx::Layout;
    use mirx::image::{ColorFormat, ImageAsset};
    let surface = SurfaceDescriptor::new(8, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let valid = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42]);
    let invalid = EncodedImageAsset::new(surface, Rle::new().record(), &[0xff]);
    let mut document = Document::new_flat(ImageAsset::new(
        8,
        1,
        ColorFormat::A8,
        8,
        (&b"01234567"[..]).into(),
    ))
    .unwrap();
    let before = document.encode(&EncodeOptions::new()).unwrap();
    assert!(document.push_encoded_image(&invalid).is_err());
    assert_eq!(document.layout(), Layout::Flat);
    assert_eq!(document.encode(&EncodeOptions::new()).unwrap(), before);
    let id = document.push_encoded_image(&valid).unwrap();
    assert_eq!(document.layout(), Layout::Chunk);
    assert!(
        document
            .get(document.primary().unwrap())
            .unwrap()
            .image()
            .unwrap()
            .raw()
            .is_some()
    );
    document.set_primary(id).unwrap();
    let before = document.encode(&EncodeOptions::new()).unwrap();
    let pointer = document.get(id).unwrap().payload_bytes().unwrap().as_ptr();
    assert!(
        document
            .get_mut(id)
            .unwrap()
            .replace_encoded_image(&invalid)
            .is_err()
    );
    assert_eq!(
        document.get(id).unwrap().payload_bytes().unwrap().as_ptr(),
        pointer
    );
    assert_eq!(document.encode(&EncodeOptions::new()).unwrap(), before);
    let meta = document.push_meta(&Meta::default()).unwrap();
    assert_eq!(
        document
            .get_mut(meta)
            .unwrap()
            .replace_encoded_image(&invalid),
        Err(EditError::InvalidChunkType)
    );
    for limits in [
        PayloadLimits::EMBEDDED.with_max_decoded_bytes(7),
        PayloadLimits::EMBEDDED.with_max_raster_groups(0),
        PayloadLimits::EMBEDDED.with_max_raster_units(0),
        PayloadLimits::EMBEDDED.with_max_raster_work(0),
    ] {
        let mut document = Document::new_with_limits(limits);
        assert!(document.push_encoded_image(&valid).is_err());
        assert_eq!(document.chunks().count(), 0);
    }
}
