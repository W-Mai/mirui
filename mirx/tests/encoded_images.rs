use mirx::{
    ChunkFlags, ChunkType, PayloadLimits, PayloadLocation, PayloadValidationFailure, ReadError,
    ReadOptions, Reader,
    coding::{Lz4, Pixel, Rle},
    encode_chunks,
    image::{
        ColorDescription, CoverageBudget, EncodedImageAsset, EncodedImageError, ImageReadError,
        SampleLayout, SurfaceDescriptor, SurfaceRequirements, UnitDecodeError,
    },
    media::{CodingId, CodingRecord, MediaPayloadError},
};

#[test]
fn critical_scalar_images_reach_aligned_caller_output_through_reader() {
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
            let payload = EncodedImageAsset::new(surface, coding, &stream[..size])
                .encode()
                .unwrap();
            let bytes = encode_chunks(&[(
                ChunkType::IMAGE.raw(),
                ChunkFlags::CRITICAL.bits(),
                &payload,
            )]);
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
                    .with_base_alignment(64)
                    .with_plane_alignment(64)
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
        PayloadLimits::EMBEDDED.with_max_image_groups(0),
        PayloadLimits::EMBEDDED.with_max_image_units(0),
        PayloadLimits::EMBEDDED.with_max_image_work(0),
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
            MediaPayloadError::DataCrcMismatch { .. }
        )))
    ));
}

#[test]
fn encoded_input_alignment_uses_the_chunk_position() {
    let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 42])
        .with_input_alignment(64)
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
            EncodedImageError::FileAddressUnaligned { alignment: 64, .. }
        ))
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
