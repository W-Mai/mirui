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

    pub(crate) fn width(&self) -> u32 {
        self.scalar.width()
    }

    pub(crate) fn height(&self) -> u32 {
        self.scalar.height()
    }

    pub(crate) fn sample_with_gradient(&self, x: Fixed, y: Fixed) -> (Fixed, Fixed, Fixed) {
        let (sample, dx, dy) = self.scalar.sample_bilinear_with_gradient(x, y);
        let distance_scale = self.spread * 2;
        (
            (sample * 2 - Fixed::ONE) * self.spread,
            dx * distance_scale,
            dy * distance_scale,
        )
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/font/sdf.rs"]
mod tests;
