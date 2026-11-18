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
mod tests {
    use super::*;
    use crate::render::texture::{Region, Texture};
    use mirx::{image::ColorFormat as Sample, palette::ColorTableView};

    #[test]
    fn indexed_upload_expands_only_the_cropped_rows_at_each_source_depth() {
        for format in [Sample::I1, Sample::I2, Sample::I4, Sample::I8] {
            let depth = usize::from(format.indexed_bit_depth().unwrap());
            let max = (1usize << depth) - 1;
            let stride = format.minimum_stride(11).unwrap() as usize + 2;
            let palette: Vec<_> = (0..=max)
                .flat_map(|i| [i as u8, 22, 65, (255 - i) as u8])
                .collect();
            let color_table = ColorTableView::from_rgba_bytes(&palette).unwrap();
            let mut bytes = alloc::vec![0; 3 * stride];
            let index = |x: usize, y: usize| max - (x + y * 7) % (max + 1);
            for y in 0..3 {
                for x in 0..11 {
                    let bit = x * depth;
                    bytes[y * stride + bit / 8] |= (index(x, y) << (8 - depth - bit % 8)) as u8;
                }
            }
            let src = Texture::from_indexed(&bytes, 11, 3, format, color_table)
                .unwrap()
                .with_stride(stride)
                .unwrap();
            let crop = src
                .pixels_in(Region::new(1, 1, 7, 2).unwrap())
                .unwrap()
                .unwrap();
            let view = crop.pixels().unwrap();
            let upload = view.rgba8_upload_pixels().unwrap();
            let mut expected = Vec::new();
            for y in 1..3 {
                for x in 1..8 {
                    let at = 4 * index(x, y);
                    expected.extend_from_slice(&palette[at..at + 4]);
                }
            }
            assert_eq!(upload.as_ref(), expected, "{format:?}");
        }
    }
}
