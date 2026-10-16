use alloc::borrow::Cow;
use alloc::vec::Vec;

use super::{ColorFormat, TexturePixels};
use crate::render::renderer::{RenderError, RenderResource};
use crate::render::scratch::PlaneLayout;

impl TexturePixels<'_> {
    pub(crate) fn rgba8_upload_pixels(&self) -> Result<Cow<'_, [u8]>, RenderError> {
        let layout = PlaneLayout::packed(self.width(), self.height(), ColorFormat::RGBA8888)
            .map_err(|_| RenderError::InvalidTexture)?;
        if self.format() == ColorFormat::RGBA8888 && self.stride() == layout.stride_bytes() {
            return Ok(Cow::Borrowed(&self.bytes()[..layout.minimum_bytes()]));
        }
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(layout.minimum_bytes())
            .map_err(|_| RenderError::ResourceLimit(RenderResource::Texture))?;
        let width = usize::from(self.width());
        let height = usize::from(self.height());
        let bytes = self.bytes();
        match self.format() {
            ColorFormat::RGBA8888 => {
                for y in 0..height {
                    let offset = y * self.stride();
                    rgba.extend_from_slice(&bytes[offset..offset + width * 4]);
                }
            }
            ColorFormat::BGRA8888 => {
                for y in 0..height {
                    let offset = y * self.stride();
                    for pixel in bytes[offset..offset + width * 4].chunks_exact(4) {
                        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
                    }
                }
            }
            ColorFormat::RGB888 => {
                for y in 0..height {
                    let offset = y * self.stride();
                    for pixel in bytes[offset..offset + width * 3].chunks_exact(3) {
                        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                    }
                }
            }
            ColorFormat::RGB565 | ColorFormat::RGB565Swapped => {
                for y in 0..height {
                    for x in 0..width {
                        let color = self.get_pixel(x as i32, y as i32);
                        rgba.extend_from_slice(&[color.r, color.g, color.b, 255]);
                    }
                }
            }
        }
        Ok(Cow::Owned(rgba))
    }
}
