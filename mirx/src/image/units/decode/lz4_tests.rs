use super::*;
use crate::image::{
    ColorDescription, GroupPlanes, Region, SampleLayout, SurfaceDescriptor, UnitGroup,
};
use alloc::vec;

#[test]
fn history_spans_padded_rows_and_planes_for_every_known_layout() {
    let codec = Lz4::new();
    let mut table = [0; Lz4::TABLE_LEN];
    let mut encoder = codec.encoder(&mut table).unwrap();
    let mut visited = 0;
    for raw in 0..=u16::MAX {
        let layout = SampleLayout::new(raw);
        if !layout.is_known() {
            continue;
        }
        visited += 1;
        let color = if layout.is_yuv() {
            ColorDescription::BT709_YUV_LIMITED
        } else if layout.is_alpha() {
            ColorDescription::NONE
        } else {
            ColorDescription::SRGB
        };
        let surface = SurfaceDescriptor::new(13, 11, layout, color).unwrap();
        let len: usize = surface
            .planes()
            .map(|p| p.minimum_stride().unwrap() as usize * p.height() as usize)
            .sum();
        for period in [1, 3, 7, 19] {
            let input: alloc::vec::Vec<_> =
                (0..len).map(|i| ((i % period) * 43 + 17) as u8).collect();
            let mut encoded = vec![0; encoder.encoded_len(&input).unwrap()];
            encoder.encode_into(&input, &mut encoded).unwrap();
            let unit = UnitGroup::builder(surface, codec.record(), &encoded)
                .build()
                .unwrap()
                .get(0)
                .unwrap();
            let plan = unit
                .decode_plan(
                    SurfaceRequirements::new()
                        .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
                        .with_plane_alignment(crate::ByteAlignment::new(64).unwrap())
                        .with_stride_multiple(64)
                        .with_height_multiple(4),
                )
                .unwrap();
            #[repr(align(64))]
            struct Buffer([u8; 4096]);
            let mut output = Buffer([0xad; 4096]);
            let mut expected = [0xad; 4096];
            expected[..plan.memory_plan().byte_len() as usize].fill(0);
            let mut position = 0;
            for plane in plan.memory_plan().planes() {
                let geometry = plane.geometry();
                let row_len = geometry.minimum_stride().unwrap() as usize;
                let used =
                    (u64::from(geometry.width()) * u64::from(geometry.bits_per_element())) % 8;
                let mask = if used == 0 { 0xff } else { 0xff << (8 - used) };
                for row in 0..geometry.height() as usize {
                    let start = plane.memory().data_offset() as usize
                        + row * plane.memory().stride() as usize;
                    expected[start..start + row_len]
                        .copy_from_slice(&input[position..position + row_len]);
                    expected[start + row_len - 1] &= mask;
                    position += row_len;
                }
            }
            let decoded = plan.decode_into(&mut output.0).unwrap();
            assert_eq!(decoded.planes().len(), surface.plane_count() as usize);
            assert!(decoded.planes().all(|plane| plane.address_is_aligned()));
            assert_eq!(output.0, expected, "layout={raw} period={period}");
        }
    }
    assert!(visited >= 22);
}

#[test]
fn packed_tail_history_is_normalized_only_after_reconstruction() {
    let surface = SurfaceDescriptor::new(33, 8, SampleLayout::A1, ColorDescription::NONE).unwrap();
    let mut input = [0; 40];
    for (i, byte) in input.iter_mut().enumerate() {
        *byte = [1, 2, 3, 4, 255, 6][i % 6];
    }
    let codec = Lz4::new();
    let mut table = [0; Lz4::TABLE_LEN];
    let mut encoded = [0; 64];
    let len = codec
        .encoder(&mut table)
        .unwrap()
        .encode_into(&input, &mut encoded)
        .unwrap();
    let unit = UnitGroup::builder(surface, codec.record(), &encoded[..len])
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    let plan = unit
        .decode_plan(SurfaceRequirements::new().with_stride_multiple(8))
        .unwrap();
    let mut output = [0xad; 72];
    let decoded = plan.decode_into(&mut output).unwrap();
    for (row, expected) in decoded
        .plane(0)
        .unwrap()
        .rows()
        .unwrap()
        .zip(input.chunks_exact(5))
    {
        assert_eq!(&row[..4], &expected[..4]);
        assert_eq!(row[4], expected[4] & 0x80);
    }
    assert!(output[64..].iter().all(|b| *b == 0xad));
}

#[test]
fn chroma_only_units_keep_indices_regions_and_source_independent_lifetimes() {
    let mut output = [0xad; 32];
    let decoded = {
        let surface = SurfaceDescriptor::new(
            9,
            7,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let codec = Lz4::new();
        let mut table = [0; Lz4::TABLE_LEN];
        let mut encoded = vec![0; 32];
        let len = codec
            .encoder(&mut table)
            .unwrap()
            .encode_into(&[128; 8], &mut encoded)
            .unwrap();
        let cells = 4u32.to_le_bytes();
        let selection = crate::media::UnitSelection::list(6, &cells).unwrap();
        let unit = UnitGroup::builder(surface, codec.record(), &encoded[..len])
            .with_planes(GroupPlanes::Plane(1))
            .with_tiles(2, 2)
            .with_selection(selection)
            .build()
            .unwrap()
            .get(0)
            .unwrap();
        unit.decode_plan(SurfaceRequirements::new().with_stride_multiple(8))
            .unwrap()
            .decode_into(&mut output)
            .unwrap()
    };
    assert_eq!(decoded.plane(0), None);
    let plane = decoded.plane(1).unwrap();
    assert_eq!(
        decoded.memory_plan().plane(1).unwrap().source_region(),
        Region::new(2, 2, 2, 2).unwrap()
    );
    assert_eq!(plane.row(0).unwrap().unwrap(), &[128; 4]);
    assert_eq!(plane.row(1).unwrap().unwrap(), &[128; 4]);
    assert_eq!(&output[16..], &[0xad; 16]);
}

#[test]
fn invalid_profile_history_and_output_fail_before_writes() {
    let surface = SurfaceDescriptor::new(13, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    let valid = [0x13, b'a', 1, 0, 0x50, b't', b'a', b'i', b'l', b'!'];
    for (record, bytes) in [
        (
            crate::media::CodingRecord::new(CodingId::LZ4, 2, &[]),
            &valid[..],
        ),
        (
            crate::media::CodingRecord::new(CodingId::LZ4, 1, &[0]),
            &valid[..],
        ),
        (Lz4::new().record(), &[0x13, b'a', 2, 0][..]),
    ] {
        let unit = UnitGroup::builder(surface, record, bytes)
            .build()
            .unwrap()
            .get(0)
            .unwrap();
        assert!(matches!(
            unit.decode_plan(SurfaceRequirements::new()),
            Err(UnitDecodeError::Lz4(_))
        ));
    }
    let unit = UnitGroup::builder(surface, Lz4::new().record(), &valid)
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    let plan = unit
        .decode_plan(
            SurfaceRequirements::new()
                .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
                .with_stride_multiple(64),
        )
        .unwrap();
    #[repr(align(64))]
    struct Buffer([u8; 128]);
    let mut output = Buffer([0xad; 128]);
    assert!(matches!(
        plan.decode_into(&mut output.0[..63]),
        Err(UnitDecodeError::Output(_))
    ));
    assert!(matches!(
        plan.decode_into(&mut output.0[1..]),
        Err(UnitDecodeError::Output(_))
    ));
    assert_eq!(output.0, [0xad; 128]);
}
