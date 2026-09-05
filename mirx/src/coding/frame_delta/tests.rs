use super::*;

#[test]
fn profile_identity_and_bounds_are_exact() {
    let codec = FrameDelta::new();
    assert_eq!(FrameDelta::from_record(codec.record()), Ok(codec));
    assert_eq!(codec.encoded_bound(0), Ok(0));
    assert_eq!(codec.encoded_bound(1), Ok(2));
    assert_eq!(codec.encoded_bound(64), Ok(65));
    assert_eq!(codec.encoded_bound(65), Ok(67));
    assert!(matches!(
        FrameDelta::from_record(CodingRecord::new(CodingId::FRAME_DELTA, 2, &[])),
        Err(FrameDeltaError::UnsupportedRevision(2))
    ));
    assert!(matches!(
        FrameDelta::from_record(CodingRecord::new(CodingId::FRAME_DELTA, 1, &[1])),
        Err(FrameDeltaError::UnexpectedParameters)
    ));
}

#[test]
fn canonical_runs_and_literals_roundtrip_wrapping_residuals() {
    let reference = [250, 1, 2, 3, 4, 5, 6];
    let current = [5, 1, 2, 9, 4, 5, 7];
    let codec = FrameDelta::new();
    let mut encoded = [0xa5; 16];
    let len = codec
        .encode_into(&reference, &current, &mut encoded)
        .unwrap();
    assert_eq!(&encoded[..len], &[0x80, 11, 1, 0x80, 6, 1, 0x80, 1]);
    assert!(encoded[len..].iter().all(|&byte| byte == 0xa5));

    let plan = codec.plan(&encoded[..len], reference.len()).unwrap();
    assert_eq!(plan.decoded_len(), current.len());
    let mut decoded = [0xa5; 9];
    assert_eq!(
        plan.decode_into(&reference, &mut decoded),
        Ok(current.len())
    );
    assert_eq!(&decoded[..current.len()], &current);
    assert_eq!(&decoded[current.len()..], &[0xa5; 2]);
}

#[test]
fn all_equal_and_long_literal_boundaries_are_deterministic() {
    let reference = [7; 260];
    let codec = FrameDelta::new();
    let mut encoded = [0xa5; 265];
    let len = codec
        .encode_into(&reference, &reference, &mut encoded)
        .unwrap();
    assert_eq!(&encoded[..len], &[PATTERN, 2, 1, 0]);

    let current = [12; 260];
    let len = codec
        .encode_into(&reference, &current, &mut encoded)
        .unwrap();
    assert_eq!(&encoded[..len], &[PATTERN, 2, 1, 5]);

    let current = core::array::from_fn::<_, 260, _>(|index| index as u8);
    let len = codec
        .encode_into(&reference, &current, &mut encoded)
        .unwrap();
    assert_eq!(len, codec.encoded_bound(current.len()).unwrap());
    let mut decoded = [0; 260];
    codec
        .plan(&encoded[..len], reference.len())
        .unwrap()
        .decode_into(&reference, &mut decoded)
        .unwrap();
    assert_eq!(decoded, current);
}

#[test]
fn repeated_channel_residuals_use_one_bounded_pattern() {
    let codec = FrameDelta::new();
    let reference = [10; 40];
    let mut current = reference;
    for pixel in current.chunks_exact_mut(4) {
        pixel[3] = 3;
    }
    let mut encoded = [0xa5; 48];
    let len = codec
        .encode_into(&reference, &current, &mut encoded)
        .unwrap();
    assert_eq!(&encoded[..len], &[PATTERN | 3, 8, 0, 0, 0, 0, 249]);
    let mut decoded = [0; 40];
    codec
        .plan(&encoded[..len], decoded.len())
        .unwrap()
        .decode_into(&reference, &mut decoded)
        .unwrap();
    assert_eq!(decoded, current);
}

#[test]
fn validated_blocks_dispatch_through_the_selected_kernel() {
    #[derive(Default)]
    struct CountingKernel {
        literals: usize,
        repeats: usize,
        patterns: usize,
    }
    impl FrameDeltaKernel for CountingKernel {
        fn add_literals(&mut self, output: &mut [u8], residuals: &[u8]) {
            self.literals += 1;
            ScalarFrameDelta.add_literals(output, residuals);
        }

        fn add_repeat(&mut self, output: &mut [u8], residual: u8) {
            self.repeats += 1;
            ScalarFrameDelta.add_repeat(output, residual);
        }

        fn add_pattern(&mut self, output: &mut [u8], residuals: &[u8]) {
            self.patterns += 1;
            ScalarFrameDelta.add_pattern(output, residuals);
        }
    }

    let input = [
        1,
        REPEAT | 1,
        3,
        LITERAL | 1,
        1,
        2,
        PATTERN | 3,
        0,
        0,
        1,
        2,
        3,
        4,
    ];
    let plan = FrameDelta::new().plan(&input, 14).unwrap();
    let mut output = [10; 14];
    let mut kernel = CountingKernel::default();
    plan.apply_with(&mut output, &mut kernel).unwrap();
    assert_eq!(
        (kernel.literals, kernel.repeats, kernel.patterns),
        (1, 1, 1)
    );
    assert_eq!(
        output,
        [10, 10, 13, 13, 11, 12, 11, 12, 13, 14, 11, 12, 13, 14]
    );
}

#[cfg(target_arch = "aarch64")]
#[test]
fn neon_and_scalar_kernels_are_bit_exact() {
    fn compare(input: &[u8], reference: &[u8]) {
        let plan = FrameDelta::new().plan(input, reference.len()).unwrap();
        let mut scalar = reference.to_vec();
        plan.apply_with(&mut scalar, &mut ScalarFrameDelta).unwrap();
        let mut neon = reference.to_vec();
        plan.apply_with(&mut neon, &mut NeonFrameDelta).unwrap();
        assert_eq!(neon, scalar);
    }

    compare(&[REPEAT | 15, 9], &[250; 16]);
    compare(
        &[
            LITERAL | 15,
            0,
            1,
            2,
            3,
            4,
            5,
            6,
            7,
            8,
            9,
            10,
            11,
            12,
            13,
            14,
            15,
        ],
        &[250; 16],
    );
    compare(&[PATTERN | 2, 38, 0, 1, 2, 3], &[250; 120]);

    let reference = core::array::from_fn::<_, 257, _>(|index| (index * 47) as u8);
    let current = core::array::from_fn::<_, 257, _>(|index| {
        reference[index].wrapping_add([0, 7, 0, 249][index % 4])
    });
    let codec = FrameDelta::new();
    let mut encoded = [0; 262];
    let len = codec
        .encode_into(&reference, &current, &mut encoded)
        .unwrap();
    let plan = codec.plan(&encoded[..len], current.len()).unwrap();
    let mut scalar = reference;
    plan.apply_with(&mut scalar, &mut ScalarFrameDelta).unwrap();
    let mut neon = reference;
    plan.apply_with(&mut neon, &mut NeonFrameDelta).unwrap();
    assert_eq!(scalar, current);
    assert_eq!(neon, current);
}

#[test]
fn all_failures_precede_output_writes() {
    let codec = FrameDelta::new();
    let reference = [1, 2, 3];
    assert!(matches!(
        codec.encoded_len(&reference, &[1, 2]),
        Err(FrameDeltaError::LengthMismatch {
            reference: 3,
            current: 2,
        })
    ));
    let mut output = [0xa5; 4];
    assert!(matches!(
        codec.encode_into(&reference, &[4, 5, 6], &mut output[..1]),
        Err(FrameDeltaError::OutputTooSmall { .. })
    ));
    assert_eq!(output, [0xa5; 4]);

    for input in [
        &[0x82, 1, 2][..],
        &[3][..],
        &[0, 0][..],
        &[0xd0, 0, 0, 1][..],
        &[PATTERN, 0][..],
    ] {
        assert!(codec.plan(input, reference.len()).is_err());
    }
    let plan = codec.plan(&[0x82, 1, 2, 3], reference.len()).unwrap();
    assert!(matches!(
        plan.decode_into(&reference, &mut output[..2]),
        Err(FrameDeltaError::OutputTooSmall {
            needed: 3,
            available: 2,
        })
    ));
    assert_eq!(output, [0xa5; 4]);
}
