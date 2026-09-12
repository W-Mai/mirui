use crate::types::Fixed;
use mirx::image::Region;

#[cfg(any(feature = "sdl-gpu", feature = "wgpu", test))]
use super::GlyphSurface;

#[cfg(any(feature = "sdl-gpu", feature = "wgpu", test))]
pub(crate) const fn alpha_bits(layout: mirx::image::SampleLayout) -> Option<u8> {
    match layout {
        mirx::image::SampleLayout::A1 => Some(1),
        mirx::image::SampleLayout::A2 => Some(2),
        mirx::image::SampleLayout::A4 => Some(4),
        mirx::image::SampleLayout::A8 => Some(8),
        _ => None,
    }
}

#[cfg(any(feature = "sdl-gpu", feature = "wgpu", test))]
pub(crate) fn unpack_surface(
    surface: GlyphSurface<'_>,
    output: &mut alloc::vec::Vec<u8>,
) -> Option<()> {
    let bits = alpha_bits(surface.sample_layout())?;
    let width = usize::try_from(surface.width()).ok()?;
    let height = usize::try_from(surface.height()).ok()?;
    let stride = usize::try_from(surface.stride()).ok()?;
    let len = width.checked_mul(height)?;
    output.clear();
    output.resize(len, 0);
    let max = (1u16 << bits) - 1;
    for y in 0..height {
        let row = surface
            .samples()
            .get(y.checked_mul(stride)?..)?
            .get(..stride)?;
        for x in 0..width {
            let bit = x.checked_mul(usize::from(bits))?;
            let byte = *row.get(bit / 8)?;
            let shift = 8 - bits - (bit % 8) as u8;
            let value = u16::from((byte >> shift) & max as u8);
            output[y * width + x] = ((value * 255 + max / 2) / max) as u8;
        }
    }
    Some(())
}

#[derive(Clone, Copy)]
pub(crate) struct ScalarField<'a> {
    samples: &'a [u8],
    stride: u32,
    region: Region,
    bits: u8,
    max_value: u16,
}

impl<'a> ScalarField<'a> {
    pub(crate) fn new(samples: &'a [u8], stride: u32, region: Region, bits: u8) -> Option<Self> {
        if !matches!(bits, 1 | 2 | 4 | 8) || region.width() == 0 || region.height() == 0 {
            return None;
        }
        let row_bits =
            u64::from(region.x().checked_add(region.width())?).checked_mul(u64::from(bits))?;
        if row_bits > u64::from(stride).checked_mul(8)? {
            return None;
        }
        let rows = u64::from(region.y().checked_add(region.height())?);
        let required = rows.checked_mul(u64::from(stride))?;
        if required > samples.len() as u64 {
            return None;
        }
        Some(Self {
            samples,
            stride,
            region,
            bits,
            max_value: (1u16 << bits) - 1,
        })
    }

    pub(crate) fn sample(&self, x: i32, y: i32) -> Fixed {
        Fixed::from_ratio(i32::from(self.quantized(x, y)), i32::from(self.max_value))
    }

    pub(crate) fn width(&self) -> u32 {
        self.region.width()
    }

    pub(crate) fn height(&self) -> u32 {
        self.region.height()
    }

    pub(crate) fn sample_bilinear(&self, x: Fixed, y: Fixed) -> Fixed {
        self.sample_bilinear_with_gradient(x, y).0
    }

    pub(crate) fn sample_bilinear_with_gradient(
        &self,
        x: Fixed,
        y: Fixed,
    ) -> (Fixed, Fixed, Fixed) {
        let max_x = i32::try_from(self.region.width()).unwrap_or(i32::MAX) - 1;
        let max_y = i32::try_from(self.region.height()).unwrap_or(i32::MAX) - 1;
        let x = x.max(Fixed::ZERO).min(Fixed::from_int(max_x));
        let y = y.max(Fixed::ZERO).min(Fixed::from_int(max_y));
        let x0 = x.to_int();
        let y0 = y.to_int();
        let x1 = (x0 + 1).min(max_x);
        let y1 = (y0 + 1).min(max_y);
        let fx = x - Fixed::from_int(x0);
        let fy = y - Fixed::from_int(y0);
        let q00 = self.sample(x0, y0);
        let q10 = self.sample(x1, y0);
        let q01 = self.sample(x0, y1);
        let q11 = self.sample(x1, y1);
        let top = q00 * (Fixed::ONE - fx) + q10 * fx;
        let bottom = q01 * (Fixed::ONE - fx) + q11 * fx;
        let value = top * (Fixed::ONE - fy) + bottom * fy;
        let dx = (q10 - q00) * (Fixed::ONE - fy) + (q11 - q01) * fy;
        let dy = (q01 - q00) * (Fixed::ONE - fx) + (q11 - q10) * fx;
        (value, dx, dy)
    }

    fn quantized(&self, x: i32, y: i32) -> u16 {
        let x = x.clamp(0, self.region.width() as i32 - 1) as u64;
        let y = y.clamp(0, self.region.height() as i32 - 1) as u64;
        let bit = (u64::from(self.region.y()) + y) * u64::from(self.stride) * 8
            + (u64::from(self.region.x()) + x) * u64::from(self.bits);
        let byte = self.samples[(bit / 8) as usize];
        let shift = 8 - self.bits - (bit % 8) as u8;
        u16::from((byte >> shift) & self.max_value as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
