use super::*;

#[test]
fn constant_layouts_share_runtime_validation() {
    const PACKED: Result<PlaneLayout, PlaneError> = PlaneLayout::packed(3, 2, ColorFormat::RGB888);
    const ALIGNED: Result<PlaneLayout, PlaneError> =
        PlaneLayout::new(3, 2, ColorFormat::RGB888, 16, PlaneRequirements::CPU);
    let packed = PACKED.unwrap();
    assert_eq!(packed.required_bytes(), 18);
    assert_eq!(packed.minimum_bytes(), 18);
    let aligned = ALIGNED.unwrap();
    assert_eq!(aligned.required_bytes(), 32);
    assert_eq!(aligned.minimum_bytes(), 25);
    assert_eq!(PACKED, PlaneLayout::packed(3, 2, ColorFormat::RGB888));
}

#[test]
fn empty_planes_are_rejected_in_every_format() {
    for format in [
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ] {
        for (width, height) in [(0, 0), (0, 1), (1, 0)] {
            assert_eq!(
                PlaneLayout::packed(width, height, format),
                Err(PlaneError::EmptyDimensions)
            );
        }
        let layout = PlaneLayout::packed(u16::MAX, u16::MAX, format);
        let expected = (u16::MAX as usize)
            .checked_mul(format.bytes_per_pixel())
            .and_then(|stride| stride.checked_mul(u16::MAX as usize));
        match expected {
            Some(bytes) => assert_eq!(layout.unwrap().required_bytes(), bytes),
            None => assert_eq!(layout, Err(PlaneError::Overflow)),
        }
    }
}

#[test]
fn aligned_borrowed_plane_preserves_padded_stride() {
    let requirements = PlaneRequirements::new(64, 64).unwrap();
    let layout = PlaneLayout::new(17, 2, ColorFormat::RGBA8888, 128, requirements).unwrap();
    assert_eq!(layout.required_bytes(), 256);
    let mut storage = [0u8; 320];
    let offset = (64 - (storage.as_ptr() as usize % 64)) % 64;
    let address = storage[offset..].as_ptr();
    let mut plane = layout.bind(&mut storage[offset..offset + 256]).unwrap();
    let allocations = crate::test_support::tracked_allocations(|| {
        let texture = plane.texture();
        let pixels = texture.read_pixels().unwrap();
        assert_eq!(pixels.stride(), 128);
        assert_eq!(pixels.bytes().len(), 256);
        assert_eq!(pixels.bytes().as_ptr(), address);
        assert!(texture.transient);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn rejects_bad_stride_address_and_capacity() {
    let requirements = PlaneRequirements::new(64, 64).unwrap();
    assert_eq!(
        PlaneLayout::new(17, 2, ColorFormat::RGBA8888, 64, requirements),
        Err(PlaneError::InvalidStride {
            minimum: 68,
            actual: 64,
        })
    );
    assert_eq!(
        PlaneLayout::new(17, 2, ColorFormat::RGBA8888, 72, requirements),
        Err(PlaneError::MisalignedStride { alignment: 64 })
    );
    let layout = PlaneLayout::new(17, 2, ColorFormat::RGBA8888, 128, requirements).unwrap();
    let mut storage = [0u8; 320];
    let offset = (64 - (storage.as_ptr() as usize % 64)) % 64;
    assert!(matches!(
        layout.bind(&mut storage[offset..offset + 255]),
        Err(PlaneError::InsufficientCapacity {
            required: 256,
            available: 255
        })
    ));
    assert!(matches!(
        layout.bind(&mut storage[offset + 1..offset + 257]),
        Err(PlaneError::MisalignedAddress { alignment: 64 })
    ));
}

#[test]
fn rejects_invalid_alignment_and_overflow() {
    assert_eq!(
        PlaneRequirements::new(0, 64),
        Err(PlaneError::InvalidAlignment)
    );
    assert_eq!(
        PlaneRequirements::new(64, 3),
        Err(PlaneError::InvalidAlignment)
    );
    let requirements = PlaneRequirements::new(1, 1).unwrap();
    assert_eq!(
        PlaneLayout::new(1, 2, ColorFormat::RGBA8888, usize::MAX, requirements),
        Err(PlaneError::Overflow)
    );
}

#[test]
fn aligned_layout_rounds_stride_before_capacity() {
    let requirements = PlaneRequirements::new(64, 64).unwrap();
    let layout = PlaneLayout::aligned(17, 3, ColorFormat::RGBA8888, requirements).unwrap();
    assert_eq!(layout.stride_bytes(), 128);
    assert_eq!(layout.required_bytes(), 384);
    assert_eq!(layout.requirements(), requirements);
}
