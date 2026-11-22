use super::*;
use mirx::image::{ColorDescription, SampleLayout, SurfaceDescriptor};

#[test]
fn samples_msb_first_regions_with_row_stride() {
    let surface = SurfaceDescriptor::new(4, 2, SampleLayout::A4, ColorDescription::NONE).unwrap();
    let region = surface.region(1, 1, 2, 1).unwrap();
    let samples = [0x00, 0xee, 0x04, 0xfe];
    let field = SignedDistanceField::new(&samples, 2, region, 4, 2).unwrap();

    assert!(field.sample_with_gradient(Fixed::ZERO, Fixed::ZERO).0 < Fixed::ZERO);
    assert!(field.sample_with_gradient(Fixed::ONE, Fixed::ZERO).0 > Fixed::ZERO);
}
