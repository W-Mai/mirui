use super::{ColorFormat, Texture};
use crate::render::renderer::RenderError;
use crate::render::scratch::{PlaneLayout, PlaneRequirements};
use crate::types::Color;

/// Borrowed, CPU-readable pixels and their row layout. Reading a pixel never performs a transfer or allocation.
///
/// The borrow prevents writes while its pixels remain in use.
///
/// ```compile_fail,E0502
/// use mirui::render::texture::{ColorFormat, Texture};
/// let mut texture = Texture::owned(1, 1, ColorFormat::RGBA8888).unwrap();
/// let pixels = texture.read_pixels().unwrap();
/// let _write = texture.pixels_mut().unwrap();
/// assert_eq!(pixels.bytes().len(), 4);
/// ```
pub struct TexturePixels<'a> {
    bytes: &'a [u8],
    layout: PlaneLayout,
}

impl<'a> TexturePixels<'a> {
    pub(super) fn new(bytes: &'a [u8], layout: PlaneLayout) -> Self {
        Self { bytes, layout }
    }

    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }

    pub fn width(&self) -> u16 {
        self.layout.width()
    }

    pub fn height(&self) -> u16 {
        self.layout.height()
    }

    pub fn format(&self) -> ColorFormat {
        self.layout.format()
    }

    pub fn stride(&self) -> usize {
        self.layout.stride_bytes()
    }

    #[inline]
    pub fn get_pixel(&self, x: i32, y: i32) -> Color {
        let Some(offset) = offset(self.layout, x, y) else {
            return Color::rgb(0, 0, 0);
        };
        self.format().read_pixel(self.bytes, offset)
    }
}

/// Exclusive CPU pixel access. Creating this borrow advances the texture's upload revision once, without copying storage.
pub struct TexturePixelsMut<'a> {
    bytes: &'a mut [u8],
    layout: PlaneLayout,
}

impl<'a> TexturePixelsMut<'a> {
    pub(super) fn new(bytes: &'a mut [u8], layout: PlaneLayout) -> Self {
        Self { bytes, layout }
    }

    pub fn bytes(&self) -> &[u8] {
        self.bytes
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        self.bytes
    }

    pub fn width(&self) -> u16 {
        self.layout.width()
    }

    pub fn height(&self) -> u16 {
        self.layout.height()
    }

    pub fn format(&self) -> ColorFormat {
        self.layout.format()
    }

    pub fn stride(&self) -> usize {
        self.layout.stride_bytes()
    }

    #[inline]
    pub fn get_pixel(&self, x: i32, y: i32) -> Color {
        let Some(offset) = offset(self.layout, x, y) else {
            return Color::rgb(0, 0, 0);
        };
        self.format().read_pixel(self.bytes, offset)
    }

    #[inline]
    pub fn set_pixel(&mut self, x: i32, y: i32, color: &Color) {
        let Some(offset) = offset(self.layout, x, y) else {
            return;
        };
        self.format().write_pixel(self.bytes, offset, color);
    }
}

#[inline]
fn offset(layout: PlaneLayout, x: i32, y: i32) -> Option<usize> {
    super::pixels::pixel_offset(
        layout.width(),
        layout.height(),
        layout.stride_bytes(),
        layout.format(),
        x,
        y,
    )
}

impl Texture<'_> {
    pub const fn width(&self) -> u16 {
        self.width
    }

    pub const fn height(&self) -> u16 {
        self.height
    }

    pub const fn format(&self) -> ColorFormat {
        self.format
    }

    pub(super) fn pixel_layout(&self) -> Result<PlaneLayout, RenderError> {
        let layout = PlaneLayout::new(
            self.width,
            self.height,
            self.format,
            self.stride,
            PlaneRequirements::CPU,
        )
        .map_err(|_| RenderError::InvalidTexture)?;
        if self.buf.as_slice().len() < layout.minimum_bytes() {
            return Err(RenderError::InvalidTexture);
        }
        Ok(layout)
    }

    /// Interpret CPU storage with a padded row stride, rejecting layouts that exceed the supplied bytes.
    pub fn with_stride(mut self, stride: usize) -> Result<Self, RenderError> {
        self.stride = stride;
        self.pixel_layout()?;
        Ok(self)
    }

    /// Borrow resident CPU pixels without allocation or device work.
    pub fn pixels(&self) -> Result<Option<TexturePixels<'_>>, RenderError> {
        self.read_pixels().map(Some)
    }

    /// Obtain readable pixels for this texture.
    pub fn read_pixels(&self) -> Result<TexturePixels<'_>, RenderError> {
        Ok(TexturePixels::new(
            self.buf.as_slice(),
            self.pixel_layout()?,
        ))
    }

    /// Borrow writable CPU storage. Read-only storage is rejected without copying or changing the texture.
    pub fn pixels_mut(&mut self) -> Result<TexturePixelsMut<'_>, RenderError> {
        let layout = self.pixel_layout()?;
        let bytes = match &mut self.buf {
            super::TexBuf::Ref(_) => return Err(RenderError::InvalidTexture),
            super::TexBuf::Mut(bytes) => &mut **bytes,
            super::TexBuf::Owned(bytes) => bytes.as_mut_slice(),
            super::TexBuf::Aligned(bytes) => bytes.as_mut_slice(),
        };
        if let Some(revision) = self.cache_revision.checked_add(1) {
            self.cache_revision = revision;
        } else {
            self.transient = true;
        }
        Ok(TexturePixelsMut::new(bytes, layout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::texture::TexBuf;

    #[test]
    fn pixel_borrows_share_storage_without_allocating() {
        let mut texture = Texture::owned(2, 2, ColorFormat::RGBA8888).unwrap();
        let address = texture.buf.as_slice().as_ptr();
        let allocations = crate::test_support::tracked_allocations(|| {
            {
                let mut pixels = texture.pixels_mut().unwrap();
                assert_eq!(pixels.bytes().as_ptr(), address);
                pixels.set_pixel(1, 1, &Color::rgba(10, 20, 30, 255));
            }
            let first = texture.read_pixels().unwrap();
            let second = texture.pixels().unwrap().unwrap();
            assert_eq!(first.bytes().as_ptr(), address);
            assert_eq!(second.get_pixel(1, 1), Color::rgba(10, 20, 30, 255));
        });
        assert_eq!(allocations, 0);
        assert_eq!(texture.cache_revision, 1);
    }

    #[test]
    fn read_only_pixels_never_become_owned_on_write() {
        let bytes = [0u8; 8];
        let mut texture = Texture::from_ref(&bytes, 2, 1, ColorFormat::RGBA8888).unwrap();
        assert!(matches!(
            texture.pixels_mut(),
            Err(RenderError::InvalidTexture)
        ));
        assert!(matches!(texture.buf, TexBuf::Ref(_)));
        assert_eq!(texture.cache_revision, 0);
        assert_eq!(
            texture.read_pixels().unwrap().bytes().as_ptr(),
            bytes.as_ptr()
        );
    }

    #[test]
    fn guarded_access_preserves_padding_and_all_pixel_formats() {
        for format in [
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
            ColorFormat::RGB888,
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
        ] {
            let bpp = format.bytes_per_pixel();
            let mut storage = [0x6a; 32];
            let mut texture = Texture::new(&mut storage, 2, 2, format).unwrap();
            texture.stride = 12;
            let color = Color::rgba(255, 0, 255, 255);
            {
                let mut pixels = texture.pixels_mut().unwrap();
                assert_eq!(
                    (
                        pixels.width(),
                        pixels.height(),
                        pixels.format(),
                        pixels.stride()
                    ),
                    (2, 2, format, 12)
                );
                pixels.set_pixel(1, 1, &color);
                pixels.set_pixel(-1, 0, &color);
                pixels.set_pixel(2, 0, &color);
                assert_eq!(pixels.get_pixel(1, 1), color);
            }
            let pixels = texture.read_pixels().unwrap();
            assert_eq!(pixels.get_pixel(1, 1), color);
            assert!(pixels.bytes()[..12 + bpp].iter().all(|byte| *byte == 0x6a));
            assert!(
                pixels.bytes()[12 + 2 * bpp..]
                    .iter()
                    .all(|byte| *byte == 0x6a)
            );
        }
    }

    #[test]
    fn revision_exhaustion_disables_caching_instead_of_reusing_a_key() {
        let mut texture = Texture::owned(1, 1, ColorFormat::RGBA8888)
            .unwrap()
            .with_cache_revision(u64::MAX);
        texture.pixels_mut().unwrap().bytes_mut().fill(0);
        assert!(texture.transient);
        assert_eq!(texture.cache_revision, u64::MAX);
    }

    #[test]
    fn padded_layout_checks_last_pixel_without_requiring_final_padding() {
        let bytes = [0u8; 14];
        let texture = Texture::from_ref(&bytes, 3, 2, ColorFormat::RGB565)
            .unwrap()
            .with_stride(8)
            .unwrap();
        let pixels = texture.read_pixels().unwrap();
        assert_eq!(pixels.stride(), 8);
        assert_eq!(pixels.get_pixel(2, 1), Color::rgb(0, 0, 0));
        for stride in [5, 9, usize::MAX] {
            assert!(matches!(
                Texture::from_ref(&bytes, 3, 2, ColorFormat::RGB565)
                    .unwrap()
                    .with_stride(stride),
                Err(RenderError::InvalidTexture)
            ));
        }
    }
}
