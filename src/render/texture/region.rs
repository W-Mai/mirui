use super::{ColorFormat, Region, Texture, TexturePixels};
use crate::render::RenderError;
use crate::types::Color;

/// A ready, read-only source region. Sampling and iteration never perform I/O or allocate.
///
/// Region coordinates refer to the original texture. Pixel views returned by [`Self::pixels`] and [`Self::tiles`] use local coordinates and retain their backing row stride. Empty regions contain no pixel views.
///
/// ```compile_fail,E0502
/// use mirui::render::texture::{ColorFormat, Region, Texture};
/// let mut texture = Texture::owned(2, 2, ColorFormat::RGBA8888).unwrap();
/// let region = texture.pixels_in(Region::new(0, 0, 1, 1).unwrap()).unwrap().unwrap();
/// let _write = texture.pixels_mut().unwrap();
/// assert!(region.get_pixel(0, 0).is_some());
/// ```
pub struct TextureRegion<'a> {
    region: Region,
    format: ColorFormat,
    pixels: Option<TexturePixels<'a>>,
}

impl TextureRegion<'_> {
    pub fn region(&self) -> Region {
        self.region
    }

    pub fn format(&self) -> ColorFormat {
        self.format
    }

    /// Borrow a continuous view when available, without joining or copying pixel storage.
    pub fn pixels(&self) -> Option<TexturePixels<'_>> {
        self.pixels.as_ref().map(TexturePixels::reborrow)
    }

    /// Visit non-overlapping source rectangles covering this region and their local pixel views.
    pub fn tiles(&self) -> impl Iterator<Item = (Region, TexturePixels<'_>)> {
        self.pixels()
            .map(|pixels| (self.region, pixels))
            .into_iter()
    }

    /// Read an absolute source coordinate, returning `None` outside the prepared region.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x < self.region.x()
            || x >= self.region.right()
            || y < self.region.y()
            || y >= self.region.bottom()
        {
            return None;
        }
        Some(
            self.pixels
                .as_ref()?
                .get_pixel((x - self.region.x()) as i32, (y - self.region.y()) as i32),
        )
    }
}

impl Texture<'_> {
    /// Borrow a resident source region without allocation, readback or decoding.
    ///
    /// `Ok(None)` means the requested data is not resident. Invalid ranges are rejected, not clipped. An in-bounds empty region is immediately ready and does not access the source device.
    pub fn pixels_in(&self, region: Region) -> Result<Option<TextureRegion<'_>>, RenderError> {
        let layout = self.storage_layout()?;
        if region.right() > u32::from(layout.width())
            || region.bottom() > u32::from(layout.height())
        {
            return Err(RenderError::InvalidTexture);
        }
        let pixels = if region.is_empty() {
            None
        } else {
            let Some(pixels) = self.pixels()? else {
                return Ok(None);
            };
            Some(pixels.into_region(region)?)
        };
        Ok(Some(TextureRegion {
            region,
            format: layout.format(),
            pixels,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_preserve_coordinates_stride_and_storage_without_allocating() {
        for format in [
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
            ColorFormat::RGB888,
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
        ] {
            let mut bytes = [0xa5; 48];
            // The final row deliberately has no trailing padding.
            let len = 32 + 3 * format.bytes_per_pixel();
            let mut texture = Texture::new(&mut bytes[..len], 3, 3, format)
                .unwrap()
                .with_stride(16)
                .unwrap();
            texture
                .pixels_mut()
                .unwrap()
                .set_pixel(2, 2, &Color::rgb(255, 0, 255));
            let full = texture.pixels().unwrap().unwrap();
            let area = Region::new(1, 1, 2, 2).unwrap();
            let allocations = crate::test_support::tracked_allocations(|| {
                let region = texture.pixels_in(area).unwrap().unwrap();
                assert_eq!(region.region(), area);
                assert_eq!(region.format(), format);
                let pixels = region.pixels().unwrap();
                let start = 16 + format.bytes_per_pixel();
                assert_eq!(pixels.bytes().as_ptr(), full.bytes()[start..].as_ptr());
                assert_eq!(pixels.bytes().len(), 16 + 2 * format.bytes_per_pixel());
                assert_eq!(
                    (pixels.width(), pixels.height(), pixels.stride()),
                    (2, 2, 16)
                );
                assert_eq!(pixels.get_pixel(1, 1), full.get_pixel(2, 2));
                assert_eq!(region.get_pixel(2, 2), Some(full.get_pixel(2, 2)));
                assert_eq!(region.get_pixel(0, 1), None);
                assert_eq!(region.get_pixel(3, 2), None);
                assert_eq!(region.get_pixel(1, u32::MAX), None);
                let mut tiles = region.tiles();
                let (rect, tile) = tiles.next().unwrap();
                assert_eq!(rect, area);
                assert_eq!(tile.bytes().as_ptr(), pixels.bytes().as_ptr());
                assert!(tiles.next().is_none());
            });
            assert_eq!(allocations, 0);
        }
    }

    #[test]
    fn empty_and_invalid_regions_have_distinct_results() {
        let bytes = [0; 16];
        let texture = Texture::from_ref(&bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
        for area in [
            Region::new(0, 0, 0, 2).unwrap(),
            Region::new(2, 2, 0, 0).unwrap(),
            Region::new(0, 2, 2, 0).unwrap(),
        ] {
            let region = texture.pixels_in(area).unwrap().unwrap();
            assert!(region.pixels().is_none());
            assert_eq!(region.tiles().count(), 0);
            assert_eq!(region.get_pixel(area.x(), area.y()), None);
        }
        for area in [
            Region::new(1, 0, 2, 1).unwrap(),
            Region::new(0, 2, 1, 1).unwrap(),
            Region::new(3, 0, 0, 0).unwrap(),
            Region::new(u32::MAX, 0, 0, 1).unwrap(),
        ] {
            assert!(matches!(
                texture.pixels_in(area),
                Err(RenderError::InvalidTexture)
            ));
        }
    }
}
