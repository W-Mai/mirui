use super::*;

fn surface_key(
    face: u64,
    revision: u64,
    surface: u64,
    width: u32,
    height: u32,
    stride: u32,
    layout: mirx::image::SampleLayout,
) -> ScalarSurfaceKey {
    let surface = GlyphSurface::new(
        &[0; 64],
        width,
        height,
        stride,
        layout,
        mirx::types::ByteAlignment::ONE,
        FontSurfaceId::new(surface),
    )
    .unwrap();
    ScalarSurfaceKey::new(FontFaceId::new(face), revision, surface)
}

#[test]
fn surface_keys_separate_identity_revision_and_pixel_layout() {
    use mirx::image::SampleLayout;

    let key = surface_key(11, 2, 7, 4, 4, 4, SampleLayout::A8);
    for other in [
        surface_key(12, 2, 7, 4, 4, 4, SampleLayout::A8),
        surface_key(11, 3, 7, 4, 4, 4, SampleLayout::A8),
        surface_key(11, 2, 8, 4, 4, 4, SampleLayout::A8),
        surface_key(11, 2, 7, 3, 4, 4, SampleLayout::A8),
        surface_key(11, 2, 7, 4, 3, 4, SampleLayout::A8),
        surface_key(11, 2, 7, 4, 4, 8, SampleLayout::A8),
        surface_key(11, 2, 7, 4, 4, 4, SampleLayout::A4),
    ] {
        assert_ne!(key, other);
    }
}

#[test]
fn surface_keys_follow_declared_identity_without_copying_pixels() {
    use core::hash::{Hash, Hasher};

    let samples = [0; 16];
    let other_samples = [0; 16];
    let make_key = |bytes| {
        let surface = GlyphSurface::new(
            bytes,
            4,
            4,
            4,
            mirx::image::SampleLayout::A8,
            mirx::types::ByteAlignment::ONE,
            FontSurfaceId::new(7),
        )
        .unwrap();
        ScalarSurfaceKey::new(FontFaceId::new(11), 2, surface)
    };
    let allocations = crate::test_support::tracked_allocations(|| {
        let key = make_key(&samples);
        let other = make_key(&other_samples);
        assert_eq!(key, other);
        let mut first_hash = std::collections::hash_map::DefaultHasher::new();
        let mut second_hash = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut first_hash);
        other.hash(&mut second_hash);
        assert_eq!(first_hash.finish(), second_hash.finish());
    });
    assert_eq!(allocations, 0);
}

#[test]
fn reads_msb_first_packed_values_with_row_stride() {
    let field = ScalarField::new(
        &[0xab, 0xcd, 0x00, 0x12, 0x34, 0x00],
        3,
        Region::new(1, 1, 2, 1).unwrap(),
        4,
    )
    .unwrap();

    assert_eq!(field.quantized(0, 0), 2);
    assert_eq!(field.quantized(1, 0), 3);
}

#[test]
fn unpacked_surface_removes_stride_and_expands_alpha() {
    let surface = GlyphSurface::new(
        &[0b1010_0000, 0, 0b0100_0000, 0],
        3,
        2,
        2,
        mirx::image::SampleLayout::A1,
        mirx::types::ByteAlignment::ONE,
        super::super::FontSurfaceId::new(7),
    )
    .unwrap();
    let mut output = alloc::vec::Vec::new();

    unpack_surface(surface, &mut output).unwrap();

    assert_eq!(output, [255, 0, 255, 0, 255, 0]);
}

#[test]
fn rejects_regions_outside_the_declared_rows() {
    assert!(ScalarField::new(&[0xff], 1, Region::new(0, 0, 9, 1).unwrap(), 1).is_none());
    assert!(ScalarField::new(&[0xff], 1, Region::new(0, 1, 1, 1).unwrap(), 1).is_none());
}

#[test]
fn bilinear_sampling_interpolates_between_texels() {
    let field = ScalarField::new(&[0, 255], 2, Region::new(0, 0, 2, 1).unwrap(), 8).unwrap();

    assert_eq!(field.sample_bilinear(Fixed::ZERO, Fixed::ZERO), Fixed::ZERO);
    assert_eq!(
        field.sample_bilinear(Fixed::from_ratio(1, 4), Fixed::ZERO),
        Fixed::from_ratio(1, 4)
    );
    assert_eq!(field.sample_bilinear(Fixed::ONE, Fixed::ZERO), Fixed::ONE);
}

#[test]
fn bilinear_gradient_is_derived_from_the_same_four_samples() {
    let field =
        ScalarField::new(&[0, 255, 255, 255], 2, Region::new(0, 0, 2, 2).unwrap(), 8).unwrap();

    assert_eq!(
        field.sample_bilinear_with_gradient(Fixed::from_ratio(1, 4), Fixed::HALF),
        (
            Fixed::from_ratio(5, 8),
            Fixed::HALF,
            Fixed::from_ratio(3, 4),
        )
    );
}
