use super::indexed::{IndexLayout, IndexPlane};
use super::{AlphaMode, ColorFormat, Region, TexBuf, Texture};
use crate::core::cache::Handle;
use crate::render::renderer::RenderError;
use crate::render::scratch::{PlaneLayout, PlaneRequirements};
use crate::types::{Color, Fixed};
use core::ops::Range;
use mirx::{image::ColorFormat as SampleFormat, palette::ColorTableView};

pub(super) struct CachedPixels {
    pub(super) bytes: alloc::vec::Vec<u8>,
    pub(super) _allocation: super::pixel_cache::PixelAllocation,
}

impl CachedPixels {
    pub(super) const OWNER_BYTES: usize = core::mem::size_of::<
        crate::core::cache::handle::CacheEntry<Self>,
    >() + 2 * core::mem::size_of::<usize>();
}

/// Borrowed, CPU-readable pixels and their row layout. Reading a pixel never performs a transfer or allocation.
///
/// The guard borrows CPU storage or retains immutable source-owned cache storage; it does not copy pixels or hold a cache-manager borrow. Drop the guard before requesting mutable access to the texture.
///
/// ```compile_fail,E0502
/// use mirui::render::texture::{ColorFormat, Texture};
/// let mut texture = Texture::owned(1, 1, ColorFormat::RGBA8888).unwrap();
/// let pixels = texture.read_pixels().unwrap();
/// let _write = texture.pixels_mut().unwrap();
/// assert_eq!(pixels.bytes().len(), 4);
/// ```
pub struct TexturePixels<'a> {
    bytes: PixelBorrow<'a>,
    layout: PlaneLayout,
    indexed: Option<(IndexLayout, PaletteBorrow<'a>)>,
}

enum PixelBorrow<'a> {
    Direct(&'a [u8]),
    Cached {
        owner: Handle<CachedPixels>,
        range: Range<usize>,
    },
}

enum PaletteBorrow<'a> {
    Direct(ColorTableView<'a>),
    Cached(Handle<CachedPixels>),
}

impl PaletteBorrow<'_> {
    fn view(&self) -> ColorTableView<'_> {
        match self {
            Self::Direct(table) => *table,
            Self::Cached(owner) => {
                ColorTableView::from_rgba_bytes(&owner.bytes).expect("validated color table")
            }
        }
    }
}

impl<'a> TexturePixels<'a> {
    pub(super) fn into_region(mut self, region: super::Region) -> Result<Self, RenderError> {
        let width = u16::try_from(region.width()).map_err(|_| RenderError::InvalidTexture)?;
        let height = u16::try_from(region.height()).map_err(|_| RenderError::InvalidTexture)?;
        if region.right() > u32::from(self.width()) || region.bottom() > u32::from(self.height()) {
            return Err(RenderError::InvalidTexture);
        }
        let y = region.y() as usize;
        let x = region.x() as usize;
        let (start, len) = if let Some((index, _)) = &mut self.indexed {
            let bit = usize::from(index.bit)
                + x * usize::from(index.format.indexed_bit_depth().expect("indexed layout"));
            let start = y * index.stride + bit / 8;
            index.bit = (bit % 8) as u8;
            self.layout = PlaneLayout::packed(width, height, ColorFormat::RGBA8888)
                .map_err(|_| RenderError::InvalidTexture)?;
            (start, index.minimum_bytes(width, height)?)
        } else {
            let format = self.layout.format();
            let stride = self.layout.stride_bytes();
            self.layout = PlaneLayout::new(width, height, format, stride, PlaneRequirements::CPU)
                .map_err(|_| RenderError::InvalidTexture)?;
            (
                y * stride + x * format.bytes_per_pixel(),
                self.layout.minimum_bytes(),
            )
        };
        let end = start
            .checked_add(len)
            .filter(|end| *end <= self.bytes().len())
            .ok_or(RenderError::InvalidTexture)?;
        self.bytes = match self.bytes {
            PixelBorrow::Direct(bytes) => PixelBorrow::Direct(&bytes[start..end]),
            PixelBorrow::Cached { owner, range } => PixelBorrow::Cached {
                owner,
                range: range.start + start..range.start + end,
            },
        };
        Ok(self)
    }

    pub(super) fn reborrow(&self) -> TexturePixels<'_> {
        TexturePixels {
            bytes: PixelBorrow::Direct(self.bytes()),
            layout: self.layout,
            indexed: self
                .indexed
                .as_ref()
                .map(|(index, table)| (*index, PaletteBorrow::Direct(table.view()))),
        }
    }

    pub(super) fn new(bytes: &'a [u8], layout: PlaneLayout) -> Self {
        Self {
            bytes: PixelBorrow::Direct(bytes),
            layout,
            indexed: None,
        }
    }

    pub(super) fn cached(owner: Handle<CachedPixels>, layout: PlaneLayout) -> Self {
        let range = 0..owner.bytes.len();
        Self {
            bytes: PixelBorrow::Cached { owner, range },
            layout,
            indexed: None,
        }
    }

    pub(super) fn indexed(
        bytes: &'a [u8],
        palette_rgba: &'a [u8],
        width: u16,
        height: u16,
        stride: usize,
        format: SampleFormat,
    ) -> Result<Self, RenderError> {
        let layout = PlaneLayout::packed(width, height, ColorFormat::RGBA8888)
            .map_err(|_| RenderError::InvalidTexture)?;
        let palette =
            ColorTableView::from_rgba_bytes(palette_rgba).ok_or(RenderError::InvalidTexture)?;
        let index = IndexPlane::new(format, palette, stride, layout, bytes)?;
        Ok(Self::from_index_plane(bytes, layout, index))
    }

    pub(super) fn cached_indexed(
        owner: Handle<CachedPixels>,
        palette: Handle<CachedPixels>,
        width: u16,
        height: u16,
        stride: usize,
        format: SampleFormat,
    ) -> Result<Self, RenderError> {
        let view =
            TexturePixels::indexed(&owner.bytes, &palette.bytes, width, height, stride, format)?;
        let layout = view.layout;
        let index = view.indexed.expect("indexed constructor").0;
        let range = 0..owner.bytes.len();
        Ok(Self {
            bytes: PixelBorrow::Cached { owner, range },
            layout,
            indexed: Some((index, PaletteBorrow::Cached(palette))),
        })
    }

    fn from_index_plane(bytes: &'a [u8], layout: PlaneLayout, index: IndexPlane<'a>) -> Self {
        Self {
            bytes: PixelBorrow::Direct(bytes),
            layout,
            indexed: Some((index.layout, PaletteBorrow::Direct(index.palette))),
        }
    }

    /// Borrows the underlying samples. Indexed rows start at `bit_offset()` within the first byte.
    pub fn bytes(&self) -> &[u8] {
        match &self.bytes {
            PixelBorrow::Direct(bytes) => bytes,
            PixelBorrow::Cached { owner, range } => &owner.bytes[range.clone()],
        }
    }

    pub fn width(&self) -> u16 {
        self.layout.width()
    }
    pub fn height(&self) -> u16 {
        self.layout.height()
    }

    /// Direct pixel encoding, when rows can be consumed without a palette.
    pub fn direct_format(&self) -> Option<ColorFormat> {
        self.indexed.is_none().then(|| self.layout.format())
    }

    /// Actual encoding of the bytes returned by this view.
    pub fn sample_format(&self) -> SampleFormat {
        self.indexed
            .as_ref()
            .map_or_else(|| self.layout.format().samples(), |(index, _)| index.format)
    }

    pub fn indexed_color_table(&self) -> Option<ColorTableView<'_>> {
        self.indexed.as_ref().map(|(_, table)| table.view())
    }

    /// MSB-first offset of the first visible sample within each row.
    pub fn bit_offset(&self) -> u8 {
        self.indexed.as_ref().map_or(0, |(index, _)| index.bit)
    }

    pub fn stride(&self) -> usize {
        self.indexed
            .as_ref()
            .map_or_else(|| self.layout.stride_bytes(), |(index, _)| index.stride)
    }

    /// Borrows this view as a transient texture, preserving row and palette addressing.
    pub fn as_texture(&self) -> Texture<'_> {
        let storage = match &self.indexed {
            None => TexBuf::Ref(self.bytes()),
            Some((layout, palette)) => TexBuf::Indexed(
                self.bytes(),
                IndexPlane {
                    layout: *layout,
                    palette: palette.view(),
                },
            ),
        };
        Texture::from_layout(storage, self.layout, true)
    }

    #[inline]
    fn index_at(&self, index: IndexLayout, x: i32, y: i32) -> Option<u8> {
        if x < 0 || y < 0 || x >= i32::from(self.width()) || y >= i32::from(self.height()) {
            return None;
        }
        let row = &self.bytes()[y as usize * index.stride..];
        let bits = index.format.indexed_bit_depth().expect("indexed format");
        let bit = usize::from(index.bit) + x as usize * usize::from(bits);
        mirx::image::read_index_at(row, bit, index.format)
    }

    /// Reads the palette index of a packed sample, returning `None` for direct pixels or coordinates outside the view.
    #[inline]
    pub fn get_index(&self, x: i32, y: i32) -> Option<u8> {
        let (index, _) = self.indexed.as_ref()?;
        self.index_at(*index, x, y)
    }

    #[inline]
    pub fn get_pixel(&self, x: i32, y: i32) -> Color {
        if let Some((index, palette)) = &self.indexed {
            let Some(value) = self.index_at(*index, x, y) else {
                return Color::rgb(0, 0, 0);
            };
            let color = palette
                .view()
                .get(usize::from(value))
                .expect("complete color table");
            return Color::rgba(color.r, color.g, color.b, color.a);
        }
        let Some(offset) = offset(self.layout, x, y) else {
            return Color::rgb(0, 0, 0);
        };
        self.layout.format().read_pixel(self.bytes(), offset)
    }
}

/// Exclusive CPU pixel access. Creating this borrow advances the texture's upload revision once, without copying storage.
pub struct TexturePixelsMut<'a> {
    bytes: &'a mut [u8],
    layout: PlaneLayout,
    alpha_mode: AlphaMode,
}

impl<'a> TexturePixelsMut<'a> {
    pub(super) fn new(bytes: &'a mut [u8], layout: PlaneLayout, alpha_mode: AlphaMode) -> Self {
        Self {
            bytes,
            layout,
            alpha_mode,
        }
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

    /// Overwrite a destination rectangle with ready source samples. Converts to the
    /// destination format without blending. Both rectangles must fit their pixel views.
    pub fn copy_region_from(
        &mut self,
        source: &TexturePixels<'_>,
        region: Region,
        destination: [u16; 2],
    ) -> Result<(), RenderError> {
        let [dx, dy] = destination.map(u32::from);
        if region.right() > u32::from(source.width())
            || region.bottom() > u32::from(source.height())
            || dx + region.width() > u32::from(self.width())
            || dy + region.height() > u32::from(self.height())
        {
            return Err(RenderError::InvalidGeometry);
        }
        if region.is_empty() {
            return Ok(());
        }
        let [dx, dy] = destination.map(usize::from);
        let [sx, sy, width, height] =
            [region.x(), region.y(), region.width(), region.height()].map(|v| v as usize);
        let format = self.format();
        let dst_stride = self.stride();
        let dst_bpp = format.bytes_per_pixel();
        if let Some(src_format) = source.direct_format() {
            let src_bpp = src_format.bytes_per_pixel();
            for y in 0..height {
                let src_start = (sy + y) * source.stride() + sx * src_bpp;
                let dst_start = (dy + y) * dst_stride + dx * dst_bpp;
                format.copy_row_from(
                    &mut self.bytes[dst_start..dst_start + width * dst_bpp],
                    src_format,
                    &source.bytes()[src_start..src_start + width * src_bpp],
                );
            }
        } else {
            for y in 0..height {
                let dst_start = (dy + y) * dst_stride + dx * dst_bpp;
                for x in 0..width {
                    let color = source.get_pixel((sx + x) as i32, (sy + y) as i32);
                    format.write_pixel(self.bytes, dst_start + x * dst_bpp, &color);
                }
            }
        }
        Ok(())
    }

    /// Composites a color at an integer pixel using its alpha and the target's alpha mode.
    /// Coordinates outside the writable area leave the target untouched.
    #[inline]
    pub fn composite_pixel(
        &mut self,
        x: i32,
        y: i32,
        color: &Color,
        mode: crate::render::command::CompositeMode,
    ) {
        self.composite_pixel_int(x, y, color, color.a, mode);
    }
}

impl TexturePixelsMut<'_> {
    pub(crate) fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    #[inline(always)]
    pub(crate) fn blend_pixel(&mut self, x: Fixed, y: Fixed, color: &Color, opa: u8) {
        if opa == 0 {
            return;
        }

        if x.is_integer() && y.is_integer() {
            let a = ((color.a as u16) * (opa as u16) / 255) as u8;
            self.blend_pixel_int(x.to_int(), y.to_int(), color, a);
            return;
        }

        self.blend_pixel_subpixel(x, y, color, opa);
    }

    #[cold]
    #[inline(never)]
    fn blend_pixel_subpixel(&mut self, x: Fixed, y: Fixed, color: &Color, opa: u8) {
        let ix = x.to_int();
        let iy = y.to_int();
        let fx = x.fract();
        let fy = y.fract();
        let lx = Fixed::ONE - fx;
        let ty = Fixed::ONE - fy;

        let nc = color.normalized();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let base_a = nc.a * opa_norm;
        let to_alpha = |cov: Fixed| -> u8 { (base_a * cov).map01(255).to_int() as u8 };

        self.blend_pixel_int(ix, iy, color, to_alpha(lx * ty));
        self.blend_pixel_int(ix + 1, iy, color, to_alpha(fx * ty));
        self.blend_pixel_int(ix, iy + 1, color, to_alpha(lx * fy));
        self.blend_pixel_int(ix + 1, iy + 1, color, to_alpha(fx * fy));
    }

    #[inline(always)]
    pub(crate) fn blend_pixel_int(&mut self, x: i32, y: i32, color: &Color, a: u8) {
        if a == 0 {
            return;
        }
        if a == 255 {
            // Fully opaque effective alpha covers the destination.
            self.set_pixel(x, y, &Color { a: 255, ..*color });
            return;
        }
        let Some(offset) = offset(self.layout, x, y) else {
            return;
        };
        self.format()
            .blend_pixel(self.bytes, offset, color, a, self.alpha_mode);
    }

    /// Composite-aware pixel write. `src_a` is the effective alpha after
    /// `src.a × opa` folding; `mode` selects the per-channel formula.
    /// Falls back to `blend_pixel_int` for SourceOver so the existing
    /// fast path stays bit-exact.
    #[inline(always)]
    pub(crate) fn composite_pixel_int(
        &mut self,
        x: i32,
        y: i32,
        color: &Color,
        src_a: u8,
        mode: crate::render::command::CompositeMode,
    ) {
        if src_a == 0 {
            return;
        }
        if matches!(mode, crate::render::command::CompositeMode::SourceOver) {
            self.blend_pixel_int(x, y, color, src_a);
            return;
        }
        let dst = self.get_pixel(x, y);
        let aa = src_a as u32;
        let ia = 255 - aa;
        // Opaque-target RGB folds the blended channel over the destination.
        let fold = |src: u8, dst: u8| -> u8 {
            let m = mode.blend_channel(src, dst) as u32;
            let sum = m * aa + dst as u32 * ia + 127;
            ((sum + (sum >> 8)) >> 8) as u8
        };
        let out = match self.alpha_mode {
            AlphaMode::Opaque => Color {
                r: fold(color.r, dst.r),
                g: fold(color.g, dst.g),
                b: fold(color.b, dst.b),
                a: 255,
            },
            AlphaMode::Blend => {
                let dst_a = u32::from(dst.a);
                let denominator = aa * 255 + dst_a * ia;
                let channel = |src: u8, dst: u8| {
                    let blend = u32::from(mode.blend_channel(src, dst));
                    ((u32::from(src) * aa * (255 - dst_a)
                        + blend * aa * dst_a
                        + u32::from(dst) * dst_a * ia
                        + denominator / 2)
                        / denominator) as u8
                };
                Color {
                    r: channel(color.r, dst.r),
                    g: channel(color.g, dst.g),
                    b: channel(color.b, dst.b),
                    a: ((denominator + 127) / 255) as u8,
                }
            }
        };
        self.set_pixel(x, y, &out);
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

    /// Direct-color format used for display and color output. See [`Self::sample_format`] for the source encoding.
    pub const fn format(&self) -> ColorFormat {
        self.format
    }

    pub(super) fn pixel_layout(&self) -> Result<PlaneLayout, RenderError> {
        let layout = self.storage_layout()?;
        if let super::TexBuf::Native(native) = &self.buf {
            native.validate(self.cache_revision)?;
        }
        Ok(layout)
    }

    pub(crate) fn storage_layout(&self) -> Result<PlaneLayout, RenderError> {
        let layout = PlaneLayout::new(
            self.width,
            self.height,
            self.format,
            self.stride,
            PlaneRequirements::CPU,
        )
        .map_err(|_| RenderError::InvalidTexture)?;
        if let super::TexBuf::Native(native) = &self.buf {
            if layout != native.layout() {
                return Err(RenderError::InvalidTexture);
            }
            return Ok(layout);
        }
        if let super::TexBuf::Decoded(source) = &self.buf {
            return if layout == source.layout() {
                Ok(layout)
            } else {
                Err(RenderError::InvalidTexture)
            };
        }
        if let super::TexBuf::Mapped(source) = &self.buf
            && source.sample_format().indexed_bit_depth().is_some()
        {
            return (layout == source.layout())
                .then_some(layout)
                .ok_or(RenderError::InvalidTexture);
        }
        if let Some((bytes, index)) = self.indexed_plane() {
            return (index.layout.minimum_bytes(self.width, self.height)? <= bytes.len())
                .then_some(layout)
                .ok_or(RenderError::InvalidTexture);
        }
        if self
            .buf
            .cpu_bytes()
            .ok_or(RenderError::InvalidTexture)?
            .len()
            < layout.minimum_bytes()
        {
            return Err(RenderError::InvalidTexture);
        }
        Ok(layout)
    }

    /// Interpret CPU storage with a padded row stride, rejecting layouts that exceed the supplied bytes.
    pub fn with_stride(mut self, stride: usize) -> Result<Self, RenderError> {
        if let TexBuf::Indexed(bytes, plane) = &mut self.buf {
            plane.layout.stride = stride;
            if plane.layout.minimum_bytes(self.width, self.height)? > bytes.len() {
                return Err(RenderError::InvalidTexture);
            }
            return Ok(self);
        }
        if matches!(
            self.buf,
            super::TexBuf::Native(_) | super::TexBuf::Decoded(_) | super::TexBuf::IndexedAligned(_)
        ) || matches!(&self.buf, TexBuf::Mapped(source) if source.sample_format().indexed_bit_depth().is_some())
        {
            return Err(RenderError::InvalidTexture);
        }
        self.stride = stride;
        self.pixel_layout()?;
        Ok(self)
    }

    /// Borrow resident CPU pixels without allocation or source work. `Ok(None)` means a valid source requires preparation; invalid or conflicting access returns an error.
    pub fn pixels(&self) -> Result<Option<TexturePixels<'_>>, RenderError> {
        if let super::TexBuf::Native(native) = &self.buf {
            return native.pixels(self.cache_revision);
        }
        if let super::TexBuf::Decoded(source) = &self.buf {
            return source.pixels_in(
                super::Region::new(0, 0, u32::from(self.width), u32::from(self.height))
                    .map_err(|_| RenderError::InvalidTexture)?,
            );
        }
        self.direct_pixels(self.pixel_layout()?).map(Some)
    }

    pub(super) fn direct_pixels(
        &self,
        layout: PlaneLayout,
    ) -> Result<TexturePixels<'_>, RenderError> {
        if let Some((bytes, index)) = self.indexed_plane() {
            return Ok(TexturePixels::from_index_plane(bytes, layout, index));
        }
        if let TexBuf::Mapped(source) = &self.buf {
            return source.texture_pixels(layout);
        }
        Ok(TexturePixels::new(
            self.buf.cpu_bytes().ok_or(RenderError::InvalidTexture)?,
            layout,
        ))
    }

    /// Obtain readable pixels, preparing and caching a native source's CPU copy when necessary. Native readback may allocate or wait for its device. Nonresident asynchronous sources return `ResourcePending` without starting I/O; use [`Self::poll_read_pixels`] with a task context to prepare them.
    pub fn read_pixels(&self) -> Result<TexturePixels<'_>, RenderError> {
        if let super::TexBuf::Native(native) = &self.buf {
            return native.read_pixels(self.cache_revision);
        }
        self.pixels()?.ok_or(RenderError::ResourcePending)
    }

    /// Borrow writable CPU storage. Read-only and native storage are rejected without copying; native CPU copies cannot be edited independently of their source.
    pub fn pixels_mut(&mut self) -> Result<TexturePixelsMut<'_>, RenderError> {
        let layout = self.pixel_layout()?;
        let bytes = match &mut self.buf {
            super::TexBuf::Ref(_)
            | super::TexBuf::Indexed(..)
            | super::TexBuf::IndexedAligned(_)
            | super::TexBuf::Native(_)
            | super::TexBuf::Mapped(_)
            | super::TexBuf::Decoded(_) => {
                return Err(RenderError::InvalidTexture);
            }
            super::TexBuf::Mut(bytes) => &mut **bytes,
            super::TexBuf::Owned(bytes) | super::TexBuf::BudgetedOwned { bytes, .. } => {
                bytes.as_mut_slice()
            }
            super::TexBuf::Aligned(bytes) => bytes.as_mut_slice(),
        };
        if let Some(revision) = self.cache_revision.checked_add(1) {
            self.cache_revision = revision;
        } else {
            self.transient = true;
        }
        Ok(TexturePixelsMut::new(bytes, layout, self.alpha_mode))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::texture::TexBuf;

    #[test]
    fn region_copies_match_pixel_writes_and_leave_padding_intact() {
        let formats = [
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
            ColorFormat::RGB888,
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
        ];
        for src_format in formats {
            let mut storage = [0; 48];
            let mut source = Texture::new(&mut storage, 3, 3, src_format).unwrap();
            source.stride = 16;
            let mut pixels = source.pixels_mut().unwrap();
            for y in 0..3 {
                for x in 0..3 {
                    pixels.set_pixel(x, y, &Color::rgba(29 * x as u8, 103, 97 * y as u8, 61));
                }
            }
            let source = source.read_pixels().unwrap();
            for format in formats {
                let mut actual = [0xa5; 48];
                let mut expected = actual;
                let mut output = Texture::new(&mut actual, 3, 3, format).unwrap();
                output.stride = 16;
                let mut output = output.pixels_mut().unwrap();
                let region = Region::new(1, 0, 2, 2).unwrap();
                assert!(output.copy_region_from(&source, region, [2, 1]).is_err());
                assert!(output.bytes().iter().all(|v| *v == 0xa5));
                output.copy_region_from(&source, region, [0, 1]).unwrap();
                for y in 0..2 {
                    for x in 0..2 {
                        format.write_pixel(
                            &mut expected,
                            (y + 1) * 16 + x * format.bytes_per_pixel(),
                            &source.get_pixel(x as i32 + 1, y as i32),
                        );
                    }
                }
                assert_eq!(actual, expected);
            }
        }
    }

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
