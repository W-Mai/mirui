use super::{ColorFormat, PlaneLayout, RenderError, TexBuf, Texture};
use mirx::{image::ColorFormat as SampleFormat, palette::ColorTableView};

#[derive(Clone, Copy)]
pub(super) struct IndexLayout {
    pub format: SampleFormat,
    pub stride: usize,
    pub bit: u8,
}

#[derive(Clone, Copy)]
pub(super) struct IndexPlane<'a> {
    pub layout: IndexLayout,
    pub palette: ColorTableView<'a>,
}

impl<'a> IndexPlane<'a> {
    pub fn new(
        format: SampleFormat,
        palette: ColorTableView<'a>,
        stride: usize,
        logical: PlaneLayout,
        bytes: &[u8],
    ) -> Result<Self, RenderError> {
        let depth = format
            .indexed_bit_depth()
            .ok_or(RenderError::InvalidTexture)?;
        if palette.len() != 1usize << depth {
            return Err(RenderError::InvalidTexture);
        }
        let layout = IndexLayout {
            format,
            stride,
            bit: 0,
        };
        if layout.minimum_bytes(logical.width(), logical.height())? > bytes.len() {
            return Err(RenderError::InvalidTexture);
        }
        Ok(Self { layout, palette })
    }
}

impl IndexLayout {
    pub fn minimum_bytes(self, width: u16, height: u16) -> Result<usize, RenderError> {
        let depth = self
            .format
            .indexed_bit_depth()
            .ok_or(RenderError::InvalidTexture)?;
        let leading = u32::from(self.bit) / u32::from(depth);
        let row = self
            .format
            .minimum_stride(u32::from(width) + leading)
            .ok_or(RenderError::InvalidTexture)? as usize;
        if row > self.stride || height == 0 || width == 0 {
            return Err(RenderError::InvalidTexture);
        }
        self.stride
            .checked_mul(usize::from(height) - 1)
            .and_then(|prefix| prefix.checked_add(row))
            .ok_or(RenderError::InvalidTexture)
    }
}

impl ColorFormat {
    pub(super) const fn samples(self) -> SampleFormat {
        match self {
            Self::RGB565 => SampleFormat::RGB565,
            Self::RGB565Swapped => SampleFormat::RGB565Swapped,
            Self::RGB888 => SampleFormat::RGB888,
            Self::RGBA8888 => SampleFormat::RGBA8888,
            Self::BGRA8888 => SampleFormat::BGRA8888,
        }
    }
}

impl<'a> Texture<'a> {
    /// Borrows densely packed, MSB-first palette indices and a complete straight-alpha RGBA color table.
    /// A new row starts on a byte boundary. Use `with_stride` for padded rows.
    pub fn from_indexed(
        bytes: &'a [u8],
        width: u16,
        height: u16,
        format: SampleFormat,
        palette: ColorTableView<'a>,
    ) -> Result<Self, RenderError> {
        let logical = PlaneLayout::packed(width, height, ColorFormat::RGBA8888)
            .map_err(|_| RenderError::InvalidTexture)?;
        let stride = format
            .minimum_stride(u32::from(width))
            .ok_or(RenderError::InvalidTexture)? as usize;
        let plane = IndexPlane::new(format, palette, stride, logical, bytes)?;
        Ok(Self::from_layout(
            TexBuf::Indexed(bytes, plane),
            logical,
            true,
        ))
    }

    pub(super) fn indexed_plane(&self) -> Option<(&[u8], IndexPlane<'_>)> {
        match &self.buf {
            TexBuf::Indexed(bytes, index) => Some((bytes, *index)),
            TexBuf::IndexedAligned(source) => Some((source.0.as_slice(), source.1)),
            _ => None,
        }
    }

    /// Physical encoding of a source sample, including packed palette indices.
    pub fn sample_format(&self) -> SampleFormat {
        match &self.buf {
            TexBuf::Mapped(source) => source.sample_format(),
            TexBuf::Decoded(source) => source.sample_format(),
            _ => self
                .indexed_plane()
                .map_or_else(|| self.format.samples(), |(_, index)| index.layout.format),
        }
    }

    /// The immutable palette used to interpret this texture's packed indices.
    pub fn indexed_color_table(&self) -> Option<ColorTableView<'_>> {
        match &self.buf {
            TexBuf::Mapped(source) => source.indexed_color_table(),
            TexBuf::Decoded(source) => source.indexed_color_table(),
            _ => self.indexed_plane().map(|(_, index)| index.palette),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::texture::Region;
    use crate::types::Color;
    use mirx::image::{ColorDescription, RawImageAsset, SampleLayout, SurfaceDescriptor};

    #[test]
    fn every_index_width_preserves_packed_samples_across_nested_crops_and_blits() {
        use SampleFormat::*;
        for (bit, format) in [(1usize, I1), (2, I2), (4, I4), (8, I8)] {
            let colors: alloc::vec::Vec<u8> = (0..1usize << bit)
                .flat_map(|i| [i as u8, 12, 28, (255 - i) as u8])
                .collect();
            let table = ColorTableView::from_rgba_bytes(&colors).unwrap();
            let stride = format.minimum_stride(17).unwrap() as usize + 2;
            let mut samples = alloc::vec![0; 3 * stride];
            let index_at = |x: usize, y: usize| (29 * x + 37 * y + 1) % (1usize << bit);
            for y in 0..3 {
                for x in 0..17 {
                    let value = index_at(x, y);
                    for j in 0..bit {
                        let dst = x * bit + j;
                        samples[y * stride + dst / 8] |=
                            (((value >> (bit - 1 - j)) & 1) << (7 - dst % 8)) as u8;
                    }
                }
            }
            let mut source = Texture::from_indexed(&samples, 17, 3, format, table)
                .unwrap()
                .with_stride(stride)
                .unwrap();
            assert!(source.pixels_mut().is_err());
            let region = source
                .pixels_in(Region::new(1, 1, 15, 2).unwrap())
                .unwrap()
                .unwrap();
            let pixels = region.pixels().unwrap();
            let cropped = pixels.as_texture();
            let inner = cropped
                .pixels_in(Region::new(2, 0, 10, 2).unwrap())
                .unwrap()
                .unwrap();
            let sub = inner.pixels().unwrap();
            assert_eq!(sub.direct_format(), None);
            assert_eq!(sub.sample_format(), format);
            assert_eq!(
                sub.indexed_color_table().unwrap().as_bytes().as_ptr(),
                colors.as_ptr()
            );
            assert_eq!(
                sub.bytes().as_ptr(),
                samples[stride + 3 * bit / 8..].as_ptr()
            );
            assert_eq!(sub.bit_offset() as usize, (3 * bit) % 8);
            for y in 0..2 {
                for x in 0..10 {
                    let index = index_at(3 + x, 1 + y);
                    assert_eq!(
                        sub.get_pixel(x as i32, y as i32),
                        Color::rgba(index as u8, 12, 28, (255 - index) as u8)
                    );
                }
            }
        }
    }

    #[test]
    fn raw_file_and_caller_reconstruction_keep_indices_and_palette() {
        let colors = [255, 0, 0, 64, 0, 255, 0, 255];
        let samples = [0b1011_1000, 0b0011_0000];
        let descriptor =
            SurfaceDescriptor::new(5, 2, SampleLayout::I1, ColorDescription::SRGB).unwrap();
        let mut document = mirx::Document::new();
        let chunk = document
            .push_image(&RawImageAsset::new(descriptor, &[&samples]).with_color_table(&colors))
            .unwrap();
        document.set_primary(chunk).unwrap();
        let bytes = alloc::boxed::Box::leak(
            document
                .encode(&mirx::document::EncodeOptions::new())
                .unwrap()
                .into_boxed_slice(),
        );
        let source = Texture::from_mirx(bytes).unwrap();
        assert_eq!(source.sample_format(), SampleFormat::I1);
        let pixels = source.read_pixels().unwrap();
        assert_eq!(pixels.bytes(), samples);
        let plan =
            Texture::plan_mirx(bytes, super::super::MirxTextureOptions::new(), &mut []).unwrap();
        assert_eq!(plan.output_len(), samples.len());
        let mut out = [0; 2];
        let decoded = plan.decode_into(&mut out, &mut []).unwrap();
        let actual = decoded.read_pixels().unwrap();
        assert_eq!(actual.bytes(), samples);
        assert_eq!(actual.get_pixel(1, 0), Color::rgba(255, 0, 0, 64));
        assert_eq!(
            actual.indexed_color_table().unwrap().as_bytes().as_ptr(),
            pixels.indexed_color_table().unwrap().as_bytes().as_ptr()
        );
    }
}
