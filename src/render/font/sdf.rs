//! Signed-distance sampling for MIRX scalar glyph planes.

use super::scalar::ScalarField;
use crate::types::Fixed;
use mirx::image::Region;

#[derive(Clone, Copy)]
pub(crate) struct SignedDistanceField<'a> {
    scalar: ScalarField<'a>,
    spread: Fixed,
}

impl<'a> SignedDistanceField<'a> {
    pub(crate) fn new(
        samples: &'a [u8],
        stride: u32,
        region: Region,
        bits: u8,
        spread: u16,
    ) -> Option<Self> {
        Some(Self {
            scalar: ScalarField::new(samples, stride, region, bits)?,
            spread: Fixed::from_int(i32::from(spread)),
        })
    }

    pub(crate) fn sample(&self, x: Fixed, y: Fixed) -> Fixed {
        (self.scalar.sample_bilinear(x, y) * 2 - Fixed::ONE) * self.spread
    }

    pub(crate) fn width(&self) -> u32 {
        self.scalar.width()
    }

    pub(crate) fn height(&self) -> u32 {
        self.scalar.height()
    }

    pub(crate) fn sample_with_gradient(&self, x: Fixed, y: Fixed) -> (Fixed, Fixed, Fixed) {
        let center = self.sample(x, y);
        let dx = (self.sample(x + Fixed::ONE, y) - self.sample(x - Fixed::ONE, y)) / 2;
        let dy = (self.sample(x, y + Fixed::ONE) - self.sample(x, y - Fixed::ONE)) / 2;
        (center, dx, dy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirx::image::{ColorDescription, SampleLayout, SurfaceDescriptor};

    #[test]
    fn samples_msb_first_regions_with_row_stride() {
        let surface =
            SurfaceDescriptor::new(4, 2, SampleLayout::A4, ColorDescription::NONE).unwrap();
        let region = surface.region(1, 1, 2, 1).unwrap();
        let samples = [0x00, 0xee, 0x04, 0xfe];
        let field = SignedDistanceField::new(&samples, 2, region, 4, 2).unwrap();

        assert!(field.sample(Fixed::ZERO, Fixed::ZERO) < Fixed::ZERO);
        assert!(field.sample(Fixed::ONE, Fixed::ZERO) > Fixed::ZERO);
    }
}
