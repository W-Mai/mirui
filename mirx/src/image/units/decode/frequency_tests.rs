use alloc::{vec, vec::Vec};

use crate::{
    coding::{Frequency, FrequencyGeometry},
    image::{ColorDescription, SampleLayout, SurfaceDescriptor, SurfaceRequirements, UnitGroup},
};

fn plane_samples(width: u32, height: u32, components: u8, seed: usize) -> Vec<u8> {
    let len = width as usize * height as usize * usize::from(components);
    (0..len)
        .map(|index| ((index * 61 + seed * 37 + index / 5 * 13) & 0xff) as u8)
        .collect()
}

fn append_plane(
    stream: &mut Vec<u8>,
    codec: Frequency,
    geometry: FrequencyGeometry,
    samples: &[u8],
) {
    let start = stream.len();
    let len = codec.encoded_len(geometry, samples).unwrap();
    stream.resize(start + len, 0);
    assert_eq!(
        codec.encode_into(geometry, samples, &mut stream[start..]),
        Ok(len)
    );
}

#[test]
fn joint_yuv_planes_decode_into_aligned_strided_storage() {
    let surface = SurfaceDescriptor::new(
        5,
        3,
        SampleLayout::NV12,
        ColorDescription::BT709_YUV_LIMITED,
    )
    .unwrap();
    let codec = Frequency::reversible();
    let mut params = [0];
    let record = codec.record_into(&mut params);
    let y_geometry = FrequencyGeometry::for_plane(SampleLayout::NV12, 0, 5, 3).unwrap();
    let uv_geometry = FrequencyGeometry::for_plane(SampleLayout::NV12, 1, 3, 2).unwrap();
    let y = plane_samples(5, 3, 1, 1);
    let uv = plane_samples(3, 2, 2, 2);
    let mut stream = Vec::new();
    append_plane(&mut stream, codec, y_geometry, &y);
    append_plane(&mut stream, codec, uv_geometry, &uv);

    let unit = UnitGroup::builder(surface, record, &stream)
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    let plan = unit
        .decode_plan(
            SurfaceRequirements::new()
                .with_base_alignment(crate::ByteAlignment::new(64).unwrap())
                .with_plane_alignment(crate::ByteAlignment::new(64).unwrap())
                .with_stride_multiple(64),
        )
        .unwrap();
    assert_eq!(plan.memory_plan().byte_len(), 320);
    #[repr(align(64))]
    struct Aligned([u8; 384]);
    let mut output = Aligned([0xa5; 384]);
    let decoded = plan.decode_into(&mut output.0).unwrap();
    let y_plane = decoded.plane(0).unwrap();
    for row in 0..3 {
        assert_eq!(
            y_plane.row(row).unwrap().unwrap(),
            &y[row as usize * 5..row as usize * 5 + 5]
        );
    }
    let uv_plane = decoded.plane(1).unwrap();
    for row in 0..2 {
        assert_eq!(
            uv_plane.row(row).unwrap().unwrap(),
            &uv[row as usize * 6..row as usize * 6 + 6]
        );
    }
    for plane in decoded.planes() {
        let memory = plane.memory();
        let row_len = plane.geometry().minimum_stride().unwrap() as usize;
        for row in 0..plane.geometry().height() as usize {
            let start = row * memory.stride() as usize + row_len;
            let end = (row + 1) * memory.stride() as usize;
            assert!(plane.bytes()[start..end].iter().all(|byte| *byte == 0));
        }
    }
    let _ = decoded;
    assert!(output.0[320..].iter().all(|byte| *byte == 0xa5));
}

#[test]
fn quantized_rgba_unit_preserves_alpha_and_uses_the_shared_memory_plan() {
    let surface =
        SurfaceDescriptor::new(13, 9, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
    let geometry = FrequencyGeometry::for_plane(SampleLayout::RGBA8888, 0, 13, 9).unwrap();
    let source = plane_samples(13, 9, 4, 3);
    let codec = Frequency::quantized(70).unwrap();
    let mut stream = Vec::new();
    append_plane(&mut stream, codec, geometry, &source);
    let mut params = [0];
    let record = codec.record_into(&mut params);
    let unit = UnitGroup::builder(surface, record, &stream)
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    let plan = unit
        .decode_plan(SurfaceRequirements::new().with_stride_multiple(64))
        .unwrap();
    assert_eq!(plan.memory_plan().plane(0).unwrap().memory().stride(), 64);
    let mut output = vec![0xa5; plan.memory_plan().byte_len() as usize + 3];
    let decoded = plan.decode_into(&mut output).unwrap();
    let plane = decoded.plane(0).unwrap();
    let mut changed = false;
    for row in 0..9 {
        let actual = plane.row(row).unwrap().unwrap();
        let expected = &source[row as usize * 52..row as usize * 52 + 52];
        for (before, after) in expected.chunks_exact(4).zip(actual.chunks_exact(4)) {
            assert_eq!(before[3], after[3]);
            changed |= before[..3] != after[..3];
        }
    }
    assert!(changed);
    assert_eq!(
        &output[plan.memory_plan().byte_len() as usize..],
        &[0xa5; 3]
    );
}

#[test]
fn profile_geometry_is_checked_before_any_unit_output_write() {
    let surface =
        SurfaceDescriptor::new(2, 2, SampleLayout::RGB565, ColorDescription::SRGB).unwrap();
    let codec = Frequency::reversible();
    let mut params = [0];
    let record = codec.record_into(&mut params);
    let unit = UnitGroup::builder(surface, record, &[0])
        .build()
        .unwrap()
        .get(0)
        .unwrap();
    assert!(matches!(
        unit.decode_plan(SurfaceRequirements::new()),
        Err(super::UnitDecodeError::Frequency(
            crate::coding::FrequencyError::UnsupportedLayout { .. }
        ))
    ));
}
