use super::TexturePixels;
#[cfg(any(feature = "wgpu", target_arch = "wasm32", test))]
use {
    super::ColorFormat,
    crate::render::{
        renderer::{RenderError, RenderResource},
        scratch::PlaneLayout,
    },
    alloc::{borrow::Cow, vec::Vec},
};

impl TexturePixels<'_> {
    /// Converts a logical row, including the palette and bit offset of indexed regions.
    pub(crate) fn sampled_rgba_row(&self, y: usize) -> impl Iterator<Item = [u8; 4]> + '_ {
        (0..i32::from(self.width())).map(move |x| {
            let color = self.get_pixel(x, y as i32);
            [color.r, color.g, color.b, color.a]
        })
    }

    #[cfg(any(feature = "wgpu", target_arch = "wasm32", test))]
    pub(crate) fn rgba8_upload_pixels(&self) -> Result<Cow<'_, [u8]>, RenderError> {
        let layout = PlaneLayout::packed(self.width(), self.height(), ColorFormat::RGBA8888)
            .map_err(|_| RenderError::InvalidTexture)?;
        if self.direct_format() == Some(ColorFormat::RGBA8888)
            && self.stride() == layout.stride_bytes()
        {
            return Ok(Cow::Borrowed(&self.bytes()[..layout.minimum_bytes()]));
        }
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(layout.minimum_bytes())
            .map_err(|_| RenderError::ResourceLimit(RenderResource::Texture))?;
        let width = usize::from(self.width());
        let height = usize::from(self.height());
        let bytes = self.bytes();
        match self.direct_format() {
            Some(ColorFormat::RGBA8888) => {
                for y in 0..height {
                    let offset = y * self.stride();
                    rgba.extend_from_slice(&bytes[offset..offset + width * 4]);
                }
            }
            Some(ColorFormat::BGRA8888) => {
                for y in 0..height {
                    let offset = y * self.stride();
                    for pixel in bytes[offset..offset + width * 4].chunks_exact(4) {
                        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                    }
                }
            }
            Some(ColorFormat::RGB888) => {
                for y in 0..height {
                    let offset = y * self.stride();
                    for pixel in bytes[offset..offset + width * 3].chunks_exact(3) {
                        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                    }
                }
            }
            Some(ColorFormat::RGB565 | ColorFormat::RGB565Swapped) | None => {
                for y in 0..height {
                    for pixel in self.sampled_rgba_row(y) {
                        rgba.extend_from_slice(&pixel);
                    }
                }
            }
        }
        Ok(Cow::Owned(rgba))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/texture/upload.rs"]
mod tests;
