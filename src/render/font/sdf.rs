//! Signed-distance sampling for MIRX scalar glyph planes.

use crate::types::Fixed;
use mirx::image::Region;

#[allow(clippy::too_many_arguments)]
pub(crate) fn sample_signed_distance(
    samples: &[u8],
    stride: u32,
    region: Region,
    bits: u8,
    spread: u16,
    sx: Fixed,
    sy: Fixed,
) -> Fixed {
    let width = region.width() as i32;
    let height = region.height() as i32;
    let sx = sx
        .max(Fixed::ZERO)
        .min(Fixed::from_int(width.saturating_sub(1)));
    let sy = sy
        .max(Fixed::ZERO)
        .min(Fixed::from_int(height.saturating_sub(1)));
    let x0 = sx.to_int();
    let y0 = sy.to_int();
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let fx = sx - Fixed::from_int(x0);
    let fy = sy - Fixed::from_int(y0);
    let read = |x, y| {
        let bit = (u64::from(region.y()) + y as u64) * u64::from(stride) * 8
            + (u64::from(region.x()) + x as u64) * u64::from(bits);
        let byte = samples[(bit / 8) as usize];
        let shift = 8 - bits - (bit % 8) as u8;
        (byte >> shift) & ((1 << bits) - 1)
    };
    let distance = |q| {
        let max = (1i32 << bits) - 1;
        let centered = Fixed::from_int(i32::from(q) * 2 - max) / Fixed::from_int(max);
        centered * Fixed::from_int(i32::from(spread))
    };
    let top = distance(read(x0, y0)) * (Fixed::ONE - fx) + distance(read(x1, y0)) * fx;
    let bottom = distance(read(x0, y1)) * (Fixed::ONE - fx) + distance(read(x1, y1)) * fx;
    top * (Fixed::ONE - fy) + bottom * fy
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
        assert!(
            sample_signed_distance(&samples, 2, region, 4, 2, Fixed::ZERO, Fixed::ZERO)
                < Fixed::ZERO
        );
        assert!(
            sample_signed_distance(&samples, 2, region, 4, 2, Fixed::ONE, Fixed::ZERO)
                > Fixed::ZERO
        );
    }
}
