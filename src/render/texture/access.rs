use super::{AlphaMode, ColorFormat, Texture};
use crate::core::cache::Handle;
use crate::render::renderer::RenderError;
use crate::render::scratch::{PlaneLayout, PlaneRequirements};
use crate::types::{Color, Fixed};
use core::ops::Range;

pub(super) struct CachedPixels {
    pub(super) bytes: alloc::vec::Vec<u8>,
    pub(super) _allocation: crate::render::budget::RetainedAllocation,
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
}

enum PixelBorrow<'a> {
    Direct(&'a [u8]),
    Cached {
        owner: Handle<CachedPixels>,
        range: Range<usize>,
    },
}

impl<'a> TexturePixels<'a> {
    pub(super) fn into_region(self, region: super::Region) -> Result<Self, RenderError> {
        let width = u16::try_from(region.width()).map_err(|_| RenderError::InvalidTexture)?;
        let height = u16::try_from(region.height()).map_err(|_| RenderError::InvalidTexture)?;
        if region.right() > u32::from(self.width()) || region.bottom() > u32::from(self.height()) {
            return Err(RenderError::InvalidTexture);
        }
        let layout = PlaneLayout::new(
            width,
            height,
            self.format(),
            self.stride(),
            PlaneRequirements::CPU,
        )
        .map_err(|_| RenderError::InvalidTexture)?;
        let start = usize::try_from(region.y())
            .ok()
            .and_then(|y| y.checked_mul(self.stride()))
            .and_then(|row| {
                usize::try_from(region.x())
                    .ok()?
                    .checked_mul(self.format().bytes_per_pixel())?
                    .checked_add(row)
            })
            .ok_or(RenderError::InvalidTexture)?;
        let end = start
            .checked_add(layout.minimum_bytes())
            .filter(|end| *end <= self.bytes().len())
            .ok_or(RenderError::InvalidTexture)?;
        let bytes = match self.bytes {
            PixelBorrow::Direct(bytes) => PixelBorrow::Direct(&bytes[start..end]),
            PixelBorrow::Cached { owner, range } => PixelBorrow::Cached {
                owner,
                range: range.start + start..range.start + end,
            },
        };
        Ok(Self { bytes, layout })
    }

    pub(super) fn reborrow(&self) -> TexturePixels<'_> {
        TexturePixels::new(self.bytes(), self.layout)
    }

    pub(super) fn new(bytes: &'a [u8], layout: PlaneLayout) -> Self {
        Self {
            bytes: PixelBorrow::Direct(bytes),
            layout,
        }
    }

    pub(super) fn cached(owner: Handle<CachedPixels>, layout: PlaneLayout) -> Self {
        let range = 0..owner.bytes.len();
        Self {
            bytes: PixelBorrow::Cached { owner, range },
            layout,
        }
    }

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
        self.format().read_pixel(self.bytes(), offset)
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

    pub(super) fn storage_layout(&self) -> Result<PlaneLayout, RenderError> {
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
        if matches!(self.buf, super::TexBuf::Native(_)) {
            return Err(RenderError::InvalidTexture);
        }
        self.stride = stride;
        self.pixel_layout()?;
        Ok(self)
    }

    /// Borrow resident CPU pixels without allocation or device work. `Ok(None)` means a valid source requires readback; invalid or conflicting access returns an error.
    pub fn pixels(&self) -> Result<Option<TexturePixels<'_>>, RenderError> {
        if let super::TexBuf::Native(native) = &self.buf {
            return native.pixels(self.cache_revision);
        }
        Ok(Some(TexturePixels::new(
            self.buf.cpu_bytes().ok_or(RenderError::InvalidTexture)?,
            self.pixel_layout()?,
        )))
    }

    /// Obtain readable pixels, preparing and caching a source-owned CPU copy when necessary. Repeated reads of unchanged resident pixels borrow the same storage; source readback may allocate or wait for its device.
    pub fn read_pixels(&self) -> Result<TexturePixels<'_>, RenderError> {
        if let super::TexBuf::Native(native) = &self.buf {
            return native.read_pixels(self.cache_revision);
        }
        self.pixels()?.ok_or(RenderError::InvalidTexture)
    }

    /// Borrow writable CPU storage. Read-only and native storage are rejected without copying; native CPU copies cannot be edited independently of their source.
    pub fn pixels_mut(&mut self) -> Result<TexturePixelsMut<'_>, RenderError> {
        let layout = self.pixel_layout()?;
        let bytes = match &mut self.buf {
            super::TexBuf::Ref(_) | super::TexBuf::Native(_) | super::TexBuf::Mapped(_) => {
                return Err(RenderError::InvalidTexture);
            }
            super::TexBuf::Mut(bytes) => &mut **bytes,
            super::TexBuf::Owned(bytes) => bytes.as_mut_slice(),
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
