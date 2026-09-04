use super::*;

#[test]
fn profile_identity_and_sample_layout_are_checked_independently() {
    for layout in [SampleLayout::RGB888, SampleLayout::RGBA8888] {
        let codec = Pixel::new(layout).unwrap();
        assert_eq!(Pixel::from_record(codec.record(), layout), Ok(codec));
        assert_eq!(codec.layout(), layout);
        assert_eq!(codec.record().id(), CodingId::PIXEL);
        assert_eq!(codec.record().revision(), 1);
        assert!(codec.record().params().is_empty());
        assert!(matches!(
            Pixel::from_record(CodingRecord::new(CodingId::RAW, 1, &[]), layout),
            Err(PixelError::UnexpectedCoding(_))
        ));
        assert!(matches!(
            Pixel::from_record(CodingRecord::new(CodingId::PIXEL, 2, &[]), layout),
            Err(PixelError::UnsupportedRevision(2))
        ));
        assert_eq!(
            Pixel::from_record(CodingRecord::new(CodingId::PIXEL, 1, &[0]), layout),
            Err(PixelError::UnexpectedParameters)
        );
    }
    for layout in [
        SampleLayout::A8,
        SampleLayout::I8,
        SampleLayout::NV12,
        SampleLayout::BGRA8888,
        SampleLayout::XRGB8888,
        SampleLayout::new(0xffff),
    ] {
        assert_eq!(
            Pixel::new(layout),
            Err(PixelError::UnsupportedLayout(layout))
        );
    }
}

#[test]
fn canonical_vectors_fix_opcode_classes_and_run_cache_updates() {
    let codec = Pixel::new(SampleLayout::RGB888).unwrap();
    let samples = [0, 0, 0, 0, 0, 0, 1, 1, 1, 9, 8, 7, 200, 30, 90, 0, 0, 0];
    let expected = [1, 0xbf, 0xe7, 0x97, RGB, 200, 30, 90, 0x75];
    let mut encoded = [0xad; 16];
    assert_eq!(codec.encoded_len(&samples), Ok(expected.len()));
    assert_eq!(
        codec.encode_into(&samples, &mut encoded),
        Ok(expected.len())
    );
    assert_eq!(&encoded[..expected.len()], &expected);
    assert!(encoded[expected.len()..].iter().all(|b| *b == 0xad));
    let plan = codec.plan(&expected, 6).unwrap();
    assert_eq!(plan.codec(), codec);
    assert_eq!(plan.input(), expected);
    assert_eq!(plan.pixel_count(), 6);
    assert_eq!(plan.decoded_len(), samples.len());
    let mut decoded = [0xad; 24];
    assert_eq!(plan.decode_into(&mut decoded), Ok(samples.len()));
    assert_eq!(&decoded[..samples.len()], &samples);
    assert!(decoded[samples.len()..].iter().all(|b| *b == 0xad));

    let rgba = Pixel::new(SampleLayout::RGBA8888).unwrap();
    let samples = [0, 0, 0, 0, 255, 0, 0, 128, 255, 0, 0, 128, 0, 0, 0, 0];
    let expected = [0x40, RGBA, 255, 0, 0, 128, 0, 0x40];
    let mut bytes = [0; 20];
    assert_eq!(rgba.encode_into(&samples, &mut bytes), Ok(expected.len()));
    assert_eq!(&bytes[..expected.len()], expected);
    let mut decoded = [0; 16];
    rgba.plan(&expected, 4)
        .unwrap()
        .decode_into(&mut decoded)
        .unwrap();
    assert_eq!(decoded, samples);
}

#[test]
fn delta_arithmetic_wraps_and_empty_rgb_cache_cannot_change_alpha() {
    let codec = Pixel::new(SampleLayout::RGB888).unwrap();
    let mut decoded = [0; 6];
    codec
        .plan(&[0xc0, 0x0f, 0xff, 0x80], 2)
        .unwrap()
        .decode_into(&mut decoded)
        .unwrap();
    // (-40,-32,-25), then (+31,+31,+23), all modulo256.
    assert_eq!(decoded, [216, 224, 231, 247, 255, 254]);
    assert_eq!(
        codec.plan(&[0x40], 1),
        Err(PixelError::InvalidAlpha { offset: 0 })
    );
    assert_eq!(
        codec.plan(&[RGBA, 0, 0, 0, 255], 1),
        Err(PixelError::InvalidOpcode {
            offset: 0,
            opcode: RGBA
        })
    );
    assert!(codec.plan(&[RGB, 0, 0, 0, 0x75], 2).is_ok());
}

#[test]
fn exact_input_and_output_counts_bound_runs_and_all_failure_writes() {
    let codec = Pixel::new(SampleLayout::RGB888).unwrap();
    let mut destination = [0xad; 8];
    assert!(matches!(
        codec.encode_into(&[1, 2], &mut destination),
        Err(PixelError::PartialPixel { .. })
    ));
    assert_eq!(destination, [0xad; 8]);
    let samples = [255, 80, 20];
    assert!(codec.encode_into(&samples, &mut destination[..3]).is_err());
    assert_eq!(destination, [0xad; 8]);
    let plan = codec.plan(&[0], 1).unwrap();
    assert!(plan.decode_into(&mut destination[..2]).is_err());
    assert_eq!(destination, [0xad; 8]);
    assert_eq!(
        codec.plan(&[1], 1),
        Err(PixelError::RunOverflow {
            remaining: 1,
            count: 2
        })
    );
    assert_eq!(
        codec.plan(&[0, 0], 1),
        Err(PixelError::TrailingData { offset: 1 })
    );
    assert_eq!(codec.plan(&[], 1), Err(PixelError::Truncated { offset: 0 }));
    for bytes in [&[RGB][..], &[RGB, 1], &[RGB, 1, 2], &[0xff]] {
        assert!(matches!(
            codec.plan(bytes, 1),
            Err(PixelError::Truncated { .. })
        ));
    }
    assert_eq!(codec.plan(&[], usize::MAX), Err(PixelError::SizeOverflow));
    assert_eq!(
        codec.encoded_bound(usize::MAX),
        Err(PixelError::SizeOverflow)
    );
    assert_eq!(codec.encode_into(&[], &mut destination), Ok(0));
    assert_eq!(
        codec.plan(&[], 0).unwrap().decode_into(&mut destination),
        Ok(0)
    );
    assert_eq!(destination, [0xad; 8]);
}

#[test]
fn canonical_run_boundaries_and_independent_resets_are_deterministic() {
    let codec = Pixel::new(SampleLayout::RGBA8888).unwrap();
    let mut samples = [0; 4 * 125];
    for pixel in samples.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
    let mut encoded = [0; 16];
    assert_eq!(codec.encode_into(&samples, &mut encoded), Ok(3));
    assert_eq!(&encoded[..3], &[61, 61, 0]);
    let mut decoded = [0; 4 * 125];
    codec
        .plan(&encoded[..3], 125)
        .unwrap()
        .decode_into(&mut decoded)
        .unwrap();
    assert_eq!(decoded, samples);
    let independent = codec.plan(&[0x40], 1).unwrap();
    independent.decode_into(&mut decoded[..4]).unwrap();
    assert_eq!(&decoded[..4], &[0, 0, 0, 0]);
    assert_eq!(codec.encoded_bound(125), Ok(625));
}
