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
#[path = "../../../tests/unit/render/texture/indexed.rs"]
mod tests;
