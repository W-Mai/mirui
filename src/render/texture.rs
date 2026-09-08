use crate::core::cache::HasSize;
use crate::types::{Color, Fixed};
use alloc::vec::Vec;

/// Owned byte storage whose visible start satisfies a requested alignment.
///
/// The backing allocation is over-sized once and never resized, so the aligned
/// view remains stable for the lifetime of the value without custom allocation.
pub struct AlignedBytes {
    storage: Vec<u8>,
    start: usize,
    len: usize,
    alignment: usize,
}

impl AlignedBytes {
    fn zeroed(len: usize, alignment: usize) -> Option<Self> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return None;
        }
        let storage_len = len.checked_add(alignment - 1)?;
        let storage = alloc::vec![0; storage_len];
        let address = storage.as_ptr() as usize;
        let aligned = address.checked_add(alignment - 1)? & !(alignment - 1);
        Some(Self {
            storage,
            start: aligned - address,
            len,
            alignment,
        })
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.storage[self.start..self.start + self.len]
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.storage[self.start..self.start + self.len]
    }

    pub const fn alignment(&self) -> usize {
        self.alignment
    }
}

impl Clone for AlignedBytes {
    fn clone(&self) -> Self {
        let mut cloned = Self::zeroed(self.len, self.alignment)
            .expect("an existing aligned allocation has valid dimensions");
        cloned.as_mut_slice().copy_from_slice(self.as_slice());
        cloned
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorFormat {
    RGB565,
    RGB565Swapped,
    RGB888,
    RGBA8888,
    BGRA8888,
}

impl ColorFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::RGB565 | Self::RGB565Swapped => 2,
            Self::RGB888 => 3,
            Self::RGBA8888 | Self::BGRA8888 => 4,
        }
    }

    /// Pack a [`Color`] into the little-endian byte layout of this format.
    /// Returns the packed bytes in a u32 (LSB-first for 2-byte formats).
    pub fn pack(self, color: &Color) -> u32 {
        match self {
            Self::RGBA8888 => {
                (color.r as u32)
                    | ((color.g as u32) << 8)
                    | ((color.b as u32) << 16)
                    | ((color.a as u32) << 24)
            }
            Self::BGRA8888 => {
                (color.b as u32)
                    | ((color.g as u32) << 8)
                    | ((color.r as u32) << 16)
                    | ((color.a as u32) << 24)
            }
            Self::RGB888 => (color.r as u32) | ((color.g as u32) << 8) | ((color.b as u32) << 16),
            Self::RGB565 => {
                let px = ((color.r as u16 >> 3) << 11)
                    | ((color.g as u16 >> 2) << 5)
                    | (color.b as u16 >> 3);
                px as u32
            }
            Self::RGB565Swapped => {
                let px = ((color.r as u16 >> 3) << 11)
                    | ((color.g as u16 >> 2) << 5)
                    | (color.b as u16 >> 3);
                ((px >> 8) as u32) | (((px & 0xFF) as u32) << 8)
            }
        }
    }
}

pub enum TexBuf<'a> {
    Ref(&'a [u8]),
    Mut(&'a mut [u8]),
    Owned(Vec<u8>),
    Aligned(AlignedBytes),
}

impl TexBuf<'_> {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Ref(s) => s,
            Self::Mut(s) => s,
            Self::Owned(v) => v,
            Self::Aligned(v) => v.as_slice(),
        }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            Self::Ref(data) => {
                *self = Self::Owned(data.to_vec());
                match self {
                    Self::Owned(v) => v,
                    _ => unreachable!(),
                }
            }
            Self::Mut(s) => s,
            Self::Owned(v) => v,
            Self::Aligned(v) => v.as_mut_slice(),
        }
    }
}

/// Destination buffer interpretation for blend writes.
///
/// `Opaque` is the framebuffer path: `dst.a` is written as 255 on
/// every pixel (ignoring whatever alpha the source carried). `Blend`
/// is the alpha-aware path used when the destination buffer's alpha
/// channel matters downstream — `dst.a` accumulates via
/// non-premultiplied source-over so a sampler reading the buffer's
/// alpha sees a correct silhouette.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlphaMode {
    #[default]
    Opaque,
    Blend,
}

pub struct Texture<'a> {
    pub buf: TexBuf<'a>,
    pub width: u16,
    pub height: u16,
    pub format: ColorFormat,
    pub stride: usize,
    pub alpha_mode: AlphaMode,
    /// Bypass GPU-side upload caches keyed by buffer pointer:
    /// `sample_target_region` drops its `Vec` each frame and the next
    /// allocation lands in the same slot, faking a cache hit on stale
    /// pixels.
    pub transient: bool,
}

impl HasSize for Texture<'_> {
    fn cache_size(&self) -> usize {
        self.buf.as_slice().len()
    }
}

impl Clone for Texture<'static> {
    fn clone(&self) -> Self {
        let buf = match &self.buf {
            TexBuf::Ref(s) => TexBuf::Ref(s),
            TexBuf::Owned(v) => TexBuf::Owned(v.clone()),
            TexBuf::Aligned(v) => TexBuf::Aligned(v.clone()),
            TexBuf::Mut(_) => {
                unreachable!("Texture<'static> with TexBuf::Mut is not constructible")
            }
        };
        Self {
            buf,
            width: self.width,
            height: self.height,
            format: self.format,
            stride: self.stride,
            alpha_mode: self.alpha_mode,
            transient: self.transient,
        }
    }
}

impl<'a> Texture<'a> {
    pub const fn from_static(buf: &'a [u8], width: u16, height: u16, format: ColorFormat) -> Self {
        let stride = width as usize * format.bytes_per_pixel();
        Self {
            buf: TexBuf::Ref(buf),
            width,
            height,
            format,
            stride,
            alpha_mode: AlphaMode::Opaque,
            transient: false,
        }
    }

    pub fn new(buf: &'a mut [u8], width: u16, height: u16, format: ColorFormat) -> Self {
        let stride = width as usize * format.bytes_per_pixel();
        Self {
            buf: TexBuf::Mut(buf),
            width,
            height,
            format,
            stride,
            alpha_mode: AlphaMode::Opaque,
            transient: false,
        }
    }

    pub fn from_ref(buf: &'a [u8], width: u16, height: u16, format: ColorFormat) -> Self {
        let stride = width as usize * format.bytes_per_pixel();
        Self {
            buf: TexBuf::Ref(buf),
            width,
            height,
            format,
            stride,
            alpha_mode: AlphaMode::Opaque,
            transient: false,
        }
    }

    pub fn owned(width: u16, height: u16, format: ColorFormat) -> Self {
        let stride = width as usize * format.bytes_per_pixel();
        let buf = alloc::vec![0u8; stride * height as usize];
        Self {
            buf: TexBuf::Owned(buf),
            width,
            height,
            format,
            stride,
            alpha_mode: AlphaMode::Opaque,
            transient: false,
        }
    }

    pub fn with_transient(mut self, transient: bool) -> Self {
        self.transient = transient;
        self
    }

    #[inline(always)]
    fn offset(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some(y as usize * self.stride + x as usize * self.format.bytes_per_pixel())
    }

    #[inline(always)]
    pub fn get_pixel(&self, x: i32, y: i32) -> Color {
        let Some(i) = self.offset(x, y) else {
            return Color::rgb(0, 0, 0);
        };
        let buf = self.buf.as_slice();
        match self.format {
            ColorFormat::RGBA8888 => Color::rgba(buf[i], buf[i + 1], buf[i + 2], buf[i + 3]),
            ColorFormat::BGRA8888 => Color::rgba(buf[i + 2], buf[i + 1], buf[i], buf[i + 3]),
            ColorFormat::RGB888 => Color::rgb(buf[i], buf[i + 1], buf[i + 2]),
            ColorFormat::RGB565 => {
                let lo = buf[i] as u16;
                let hi = buf[i + 1] as u16;
                let px = lo | (hi << 8);
                Color::rgb(
                    ((px >> 11) as u8) << 3,
                    (((px >> 5) & 0x3F) as u8) << 2,
                    ((px & 0x1F) as u8) << 3,
                )
            }
            ColorFormat::RGB565Swapped => {
                let hi = buf[i] as u16;
                let lo = buf[i + 1] as u16;
                let px = lo | (hi << 8);
                Color::rgb(
                    ((px >> 11) as u8) << 3,
                    (((px >> 5) & 0x3F) as u8) << 2,
                    ((px & 0x1F) as u8) << 3,
                )
            }
        }
    }

    #[inline(always)]
    pub fn set_pixel(&mut self, x: i32, y: i32, color: &Color) {
        let Some(i) = self.offset(x, y) else { return };
        let buf = self.buf.as_mut_slice();
        match self.format {
            ColorFormat::RGBA8888 => {
                buf[i] = color.r;
                buf[i + 1] = color.g;
                buf[i + 2] = color.b;
                buf[i + 3] = color.a;
            }
            ColorFormat::BGRA8888 => {
                buf[i] = color.b;
                buf[i + 1] = color.g;
                buf[i + 2] = color.r;
                buf[i + 3] = color.a;
            }
            ColorFormat::RGB888 => {
                buf[i] = color.r;
                buf[i + 1] = color.g;
                buf[i + 2] = color.b;
            }
            ColorFormat::RGB565 | ColorFormat::RGB565Swapped => {
                let px = ((color.r as u16 >> 3) << 11)
                    | ((color.g as u16 >> 2) << 5)
                    | (color.b as u16 >> 3);
                let (b0, b1) = if self.format == ColorFormat::RGB565 {
                    (px as u8, (px >> 8) as u8)
                } else {
                    ((px >> 8) as u8, px as u8)
                };
                buf[i] = b0;
                buf[i + 1] = b1;
            }
        }
    }

    #[inline(always)]
    pub fn blend_pixel(&mut self, x: Fixed, y: Fixed, color: &Color, opa: u8) {
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
    pub fn blend_pixel_int(&mut self, x: i32, y: i32, color: &Color, a: u8) {
        if a == 0 {
            return;
        }
        if a == 255 {
            // Fully opaque source: covers dst regardless of mode. The
            // source-over identity (1·src + 0·dst) gives both `out.rgb
            // = src.rgb` and `out.a = src.a` — `set_pixel` does both.
            self.set_pixel(x, y, color);
            return;
        }
        // Alpha blend in plain u8 space: out = (src·a + dst·(255−a) + 127)/255.
        // Avoids the NormColor round-trip (8 divisions per call) that the
        // old implementation did; exact within ±1 over the full range.
        let dst = self.get_pixel(x, y);
        let ia = 255 - a as u32;
        let aa = a as u32;
        let blend = |src: u8, dst: u8| -> u8 {
            let sum = src as u32 * aa + dst as u32 * ia + 127;
            ((sum + (sum >> 8)) >> 8) as u8
        };
        // Blend mode accumulates dst.a via non-premultiplied source-over:
        //   out.a = src.a + dst.a × (255 − src.a) / 255
        // so a downstream sampler reading the buffer's alpha sees a
        // correct silhouette. Opaque mode writes 255 — matches the
        // pre-AlphaMode behaviour for the framebuffer path.
        let out_a = match self.alpha_mode {
            AlphaMode::Opaque => 255,
            AlphaMode::Blend => {
                let src_a = a as u32;
                let dst_a = dst.a as u32;
                let inv = 255 - src_a;
                let sum = src_a * 255 + dst_a * inv + 127;
                ((sum + (sum >> 8)) >> 8) as u8
            }
        };
        let out = Color {
            r: blend(color.r, dst.r),
            g: blend(color.g, dst.g),
            b: blend(color.b, dst.b),
            a: out_a,
        };
        self.set_pixel(x, y, &out);
    }

    /// Composite-aware pixel write. `src_a` is the effective alpha after
    /// `src.a × opa` folding; `mode` selects the per-channel formula.
    /// Falls back to `blend_pixel_int` for SourceOver so the existing
    /// fast path stays bit-exact.
    #[inline(always)]
    pub fn composite_pixel_int(
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
        // out_rgb = mode(src, dst) * src.a + dst * (1 - src.a),  per channel.
        let fold = |src: u8, dst: u8| -> u8 {
            let m = mode.blend_channel(src, dst) as u32;
            let sum = m * aa + dst as u32 * ia + 127;
            ((sum + (sum >> 8)) >> 8) as u8
        };
        let out_a = match self.alpha_mode {
            AlphaMode::Opaque => 255,
            AlphaMode::Blend => {
                // Uniform SourceOver alpha accumulation across modes
                // (sign-off: dst.a stays mode-agnostic).
                let dst_a = dst.a as u32;
                let sum = aa * 255 + dst_a * ia + 127;
                ((sum + (sum >> 8)) >> 8) as u8
            }
        };
        let out = Color {
            r: fold(color.r, dst.r),
            g: fold(color.g, dst.g),
            b: fold(color.b, dst.b),
            a: out_a,
        };
        self.set_pixel(x, y, &out);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirxLoadError {
    Read(mirx::ReadError),
    Image(mirx::image::ImageReadError),
    Surface(mirx::image::ImageEncodeError),
    Plan(mirx::image::SurfacePlanError),
    Groups(mirx::image::EncodedImageError),
    Request(mirx::image::DecodeRequestError),
    Decode(mirx::image::DecodeError),
    Copy(mirx::image::SurfaceCopyError),
    /// Format mirx writes but mirui can't render (indexed, alpha-only, luma, RGB565A8).
    UnsupportedFormat(mirx::image::ColorFormat),
    /// The decoded surface requires more than one physical plane.
    UnsupportedLayout(mirx::image::SampleLayout),
    /// Parsed OK but no IMAGE chunk (e.g. VECTOR-only file).
    NoImageChunk,
    /// `width` or `height` exceeds `u16`.
    DimensionOverflow,
    /// The aligned output or workspace allocation size overflowed `usize`.
    AllocationSizeOverflow,
    /// The requested output or workspace must be supplied by the caller.
    ExternalMemoryRequired,
}

impl From<mirx::ReadError> for MirxLoadError {
    fn from(err: mirx::ReadError) -> Self {
        MirxLoadError::Read(err)
    }
}

impl From<mirx::image::ImageReadError> for MirxLoadError {
    fn from(err: mirx::image::ImageReadError) -> Self {
        MirxLoadError::Image(err)
    }
}

impl From<mirx::image::ImageEncodeError> for MirxLoadError {
    fn from(err: mirx::image::ImageEncodeError) -> Self {
        MirxLoadError::Surface(err)
    }
}

impl From<mirx::image::SurfacePlanError> for MirxLoadError {
    fn from(err: mirx::image::SurfacePlanError) -> Self {
        MirxLoadError::Plan(err)
    }
}

impl From<mirx::image::EncodedImageError> for MirxLoadError {
    fn from(err: mirx::image::EncodedImageError) -> Self {
        MirxLoadError::Groups(err)
    }
}

impl From<mirx::image::DecodeRequestError> for MirxLoadError {
    fn from(err: mirx::image::DecodeRequestError) -> Self {
        MirxLoadError::Request(err)
    }
}

impl From<mirx::image::DecodeError> for MirxLoadError {
    fn from(err: mirx::image::DecodeError) -> Self {
        MirxLoadError::Decode(err)
    }
}

impl From<mirx::image::SurfaceCopyError> for MirxLoadError {
    fn from(err: mirx::image::SurfaceCopyError) -> Self {
        MirxLoadError::Copy(err)
    }
}

impl From<core::num::TryFromIntError> for MirxLoadError {
    fn from(_: core::num::TryFromIntError) -> Self {
        MirxLoadError::DimensionOverflow
    }
}

pub(super) fn map_mirx_format(fmt: mirx::image::ColorFormat) -> Result<ColorFormat, MirxLoadError> {
    match fmt {
        mirx::image::ColorFormat::RGB565 => Ok(ColorFormat::RGB565),
        mirx::image::ColorFormat::RGB565Swapped => Ok(ColorFormat::RGB565Swapped),
        mirx::image::ColorFormat::RGB888 => Ok(ColorFormat::RGB888),
        // XRGB and RGBA share byte layout; opaque-vs-blend lives in AlphaMode.
        mirx::image::ColorFormat::XRGB8888 | mirx::image::ColorFormat::RGBA8888 => {
            Ok(ColorFormat::RGBA8888)
        }
        mirx::image::ColorFormat::BGRA8888 => Ok(ColorFormat::BGRA8888),
        other => Err(MirxLoadError::UnsupportedFormat(other)),
    }
}

/// Decode execution, memory placement, physical layout, and limits for a MIRX texture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MirxTextureOptions {
    request: mirx::image::DecodeRequest,
    limits: mirx::PayloadLimits,
}

impl MirxTextureOptions {
    pub const fn new() -> Self {
        Self {
            request: mirx::image::DecodeRequest::new(mirx::image::SurfaceRequirements::new()),
            limits: mirx::PayloadLimits::EMBEDDED,
        }
    }

    pub const fn with_requirements(
        mut self,
        requirements: mirx::image::SurfaceRequirements,
    ) -> Self {
        self.request = self.request.with_requirements(requirements);
        self
    }

    pub const fn with_decode_request(mut self, request: mirx::image::DecodeRequest) -> Self {
        self.request = request;
        self
    }

    pub const fn with_execution(mut self, execution: mirx::image::DecodeExecution) -> Self {
        self.request = self.request.with_execution(execution);
        self
    }

    pub const fn with_input_memory(mut self, placement: mirx::image::MemoryPlacement) -> Self {
        self.request = self.request.with_input(placement);
        self
    }

    pub const fn with_output_memory(mut self, placement: mirx::image::MemoryPlacement) -> Self {
        self.request = self.request.with_output(placement);
        self
    }

    pub const fn with_workspace_memory(mut self, placement: mirx::image::MemoryPlacement) -> Self {
        self.request = self.request.with_workspace(placement);
        self
    }

    pub const fn with_workspace_alignment(mut self, alignment: mirx::types::ByteAlignment) -> Self {
        self.request = self.request.with_workspace_alignment(alignment);
        self
    }

    pub const fn with_base_alignment(mut self, alignment: mirx::types::ByteAlignment) -> Self {
        self.request = self
            .request
            .with_requirements(self.request.requirements().with_base_alignment(alignment));
        self
    }

    pub const fn with_plane_alignment(mut self, alignment: mirx::types::ByteAlignment) -> Self {
        self.request = self
            .request
            .with_requirements(self.request.requirements().with_plane_alignment(alignment));
        self
    }

    pub const fn with_width_multiple(mut self, multiple: u32) -> Self {
        self.request = self
            .request
            .with_requirements(self.request.requirements().with_width_multiple(multiple));
        self
    }

    pub const fn with_height_multiple(mut self, multiple: u32) -> Self {
        self.request = self
            .request
            .with_requirements(self.request.requirements().with_height_multiple(multiple));
        self
    }

    pub const fn with_stride_multiple(mut self, multiple: u32) -> Self {
        self.request = self
            .request
            .with_requirements(self.request.requirements().with_stride_multiple(multiple));
        self
    }

    pub const fn with_limits(mut self, limits: mirx::PayloadLimits) -> Self {
        self.limits = limits;
        self
    }

    pub const fn requirements(self) -> mirx::image::SurfaceRequirements {
        self.request.requirements()
    }

    pub const fn decode_request(self) -> mirx::image::DecodeRequest {
        self.request
    }

    pub const fn limits(self) -> mirx::PayloadLimits {
        self.limits
    }
}

impl Default for MirxTextureOptions {
    fn default() -> Self {
        Self::new()
    }
}

// Boxing the encoded plan would make the explicit caller-owned planning path
// allocate. This value is constructed once per load and never lives per frame.
#[allow(clippy::large_enum_variant)]
enum MirxTexturePlanInner<'source, 'groups> {
    Raw(mirx::image::SurfaceView<'source>),
    Encoded(mirx::image::ImageDecodePlan<'source, 'groups>),
}

/// Preflighted MIRX texture decode with exact caller-buffer requirements.
///
/// Group slots, encoded bytes, output, and codec workspace all remain owned by
/// the caller. Planning validates the complete selected image before output is
/// touched and retains the decode request for cache maintenance at the adapter
/// boundary.
pub struct MirxTexturePlan<'source, 'groups> {
    inner: MirxTexturePlanInner<'source, 'groups>,
    memory: mirx::image::SurfaceMemoryPlan,
    request: mirx::image::DecodeRequest,
}

impl MirxTexturePlan<'_, '_> {
    pub const fn output_len(&self) -> usize {
        self.memory.buffer_requirements().byte_len()
    }

    pub const fn output_alignment(&self) -> mirx::types::ByteAlignment {
        self.memory.buffer_requirements().base_alignment()
    }

    pub const fn workspace_len(&self) -> usize {
        match self.inner {
            MirxTexturePlanInner::Raw(_) => 0,
            MirxTexturePlanInner::Encoded(plan) => plan.workspace_requirements().byte_len(),
        }
    }

    pub const fn workspace_alignment(&self) -> mirx::types::ByteAlignment {
        match self.inner {
            MirxTexturePlanInner::Raw(_) => self.request.workspace_alignment(),
            MirxTexturePlanInner::Encoded(plan) => plan.workspace_requirements().base_alignment(),
        }
    }

    pub const fn decode_request(&self) -> mirx::image::DecodeRequest {
        self.request
    }

    pub const fn input_sync(&self) -> mirx::image::CacheSync {
        self.request.input_sync()
    }

    pub const fn output_sync(&self) -> mirx::image::CacheSync {
        self.request.output_sync()
    }

    pub const fn memory_plan(&self) -> mirx::image::SurfaceMemoryPlan {
        self.memory
    }
}

impl<'source, 'groups> MirxTexturePlan<'source, 'groups> {
    /// Reconstructs the preflighted texture into caller-owned storage.
    pub fn decode_into<'output>(
        self,
        output: &'output mut [u8],
        workspace: &mut [u8],
    ) -> Result<Texture<'output>, MirxLoadError>
    where
        'source: 'output,
    {
        let surface = match self.inner {
            MirxTexturePlanInner::Raw(source) => source.copy_into(output, self.memory)?,
            MirxTexturePlanInner::Encoded(plan) => plan.decode_into(output, workspace)?,
        };
        texture_from_surface(surface)
    }
}

fn open_mirx_image(bytes: &[u8]) -> Result<mirx::image::ImageRef<'_>, MirxLoadError> {
    let reader = mirx::Reader::open(bytes)?;
    if let Some(image) = reader.flat_image() {
        return Ok(mirx::image::ImageRef::Raw(image.surface()?));
    }
    let primary = reader.primary()?.ok_or(MirxLoadError::NoImageChunk)?;
    primary.image()?.ok_or(MirxLoadError::NoImageChunk)
}

pub(super) fn texture_from_surface(
    surface: mirx::image::SurfaceView<'_>,
) -> Result<Texture<'_>, MirxLoadError> {
    let descriptor = surface.surface();
    if descriptor.plane_count() != 1 {
        return Err(MirxLoadError::UnsupportedLayout(descriptor.sample_layout()));
    }
    let source_format = descriptor
        .sample_layout()
        .color_format()
        .ok_or(MirxLoadError::UnsupportedLayout(descriptor.sample_layout()))?;
    let format = map_mirx_format(source_format)?;
    let plane = surface
        .plane(0)
        .ok_or(MirxLoadError::UnsupportedLayout(descriptor.sample_layout()))?;
    Ok(Texture {
        buf: TexBuf::Ref(plane.bytes()),
        width: descriptor.width().try_into()?,
        height: descriptor.height().try_into()?,
        format,
        stride: plane.memory().stride() as usize,
        alpha_mode: AlphaMode::Opaque,
        transient: false,
    })
}

impl Texture<'static> {
    /// Preflights a MIRX image into a caller-owned decode plan.
    pub fn plan_mirx<'source, 'groups>(
        bytes: &'source [u8],
        options: MirxTextureOptions,
        group_slots: &'groups mut [Option<mirx::image::UnitGroup<'source>>],
    ) -> Result<MirxTexturePlan<'source, 'groups>, MirxLoadError> {
        let request = options.decode_request();
        request.validate_reconstruction()?;
        let image = open_mirx_image(bytes)?;
        let memory = image.surface().memory_plan(request.requirements())?;
        // Reject unsupported render layouts during planning, before any output
        // or workspace allocation is requested.
        let descriptor = image.surface();
        if descriptor.plane_count() != 1 {
            return Err(MirxLoadError::UnsupportedLayout(descriptor.sample_layout()));
        }
        map_mirx_format(
            descriptor
                .sample_layout()
                .color_format()
                .ok_or(MirxLoadError::UnsupportedLayout(descriptor.sample_layout()))?,
        )?;
        let inner = match image {
            mirx::image::ImageRef::Raw(surface) => MirxTexturePlanInner::Raw(surface),
            mirx::image::ImageRef::Encoded(image) => {
                let mut budget =
                    mirx::image::CoverageBudget::new(options.limits().max_raster_work());
                let groups = image.groups_into(group_slots, &mut budget)?;
                MirxTexturePlanInner::Encoded(groups.decode_plan_for(request, &options.limits())?)
            }
        };
        Ok(MirxTexturePlan {
            inner,
            memory,
            request,
        })
    }

    /// Returns display geometry without allocating or decoding samples.
    pub fn probe_mirx(bytes: &[u8]) -> Result<TextureMeta, MirxLoadError> {
        let surface = open_mirx_image(bytes)?.surface();
        if surface.plane_count() != 1 {
            return Err(MirxLoadError::UnsupportedLayout(surface.sample_layout()));
        }
        let format = map_mirx_format(
            surface
                .sample_layout()
                .color_format()
                .ok_or(MirxLoadError::UnsupportedLayout(surface.sample_layout()))?,
        )?;
        Ok(TextureMeta {
            width: surface.width().try_into()?,
            height: surface.height().try_into()?,
            format,
        })
    }

    /// Opens a MIRX image using embedded limits and tight output storage.
    /// RAW samples stay borrowed; encoded samples become aligned owned storage.
    pub fn from_mirx(bytes: &'static [u8]) -> Result<Self, MirxLoadError> {
        Self::from_mirx_with(bytes, MirxTextureOptions::new())
    }

    /// Opens a MIRX image under explicit decode limits and hardware layout
    /// requirements. RAW storage is copied only when requirements are non-tight.
    pub fn from_mirx_with(
        bytes: &'static [u8],
        options: MirxTextureOptions,
    ) -> Result<Self, MirxLoadError> {
        let request = options.decode_request();
        request.validate_reconstruction()?;
        if request.output() != mirx::image::MemoryPlacement::Cpu
            || request.workspace() != mirx::image::MemoryPlacement::Cpu
        {
            return Err(MirxLoadError::ExternalMemoryRequired);
        }
        let image = open_mirx_image(bytes)?;
        if request == mirx::image::DecodeRequest::default() {
            if let mirx::image::ImageRef::Raw(surface) = image {
                return texture_from_surface(surface);
            }
        }
        let group_count = image.encoded().map_or(0, |image| image.group_count());
        let mut group_slots = alloc::vec![None; group_count];
        let plan = Self::plan_mirx(bytes, options, &mut group_slots)?;
        let output_len = plan.output_len();
        let output_alignment = plan.output_alignment();
        let workspace_len = plan.workspace_len();
        let workspace_alignment = plan.workspace_alignment();
        let memory = plan.memory_plan();
        let mut output = AlignedBytes::zeroed(
            output_len,
            usize::try_from(output_alignment.get()).expect("u32 fits usize"),
        )
        .ok_or(MirxLoadError::AllocationSizeOverflow)?;
        let mut workspace = AlignedBytes::zeroed(
            workspace_len,
            usize::try_from(workspace_alignment.get()).expect("u32 fits usize"),
        )
        .ok_or(MirxLoadError::AllocationSizeOverflow)?;
        let decoded = plan.decode_into(output.as_mut_slice(), workspace.as_mut_slice())?;
        let width = decoded.width;
        let height = decoded.height;
        let format = decoded.format;
        let stride = decoded.stride;
        drop(decoded);
        debug_assert_eq!(memory.plane(0).map(|plane| plane.data_offset()), Some(0));
        Ok(Texture {
            buf: TexBuf::Aligned(output),
            width,
            height,
            format,
            stride,
            alpha_mode: AlphaMode::Opaque,
            transient: false,
        })
    }
}

/// Cheap geometry snapshot used as [`HasProbe::Meta`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureMeta {
    pub width: u16,
    pub height: u16,
    pub format: ColorFormat,
}

impl HasSize for TextureMeta {
    fn cache_size(&self) -> usize {
        1
    }
}

impl crate::core::resource::HasProbe for Texture<'static> {
    type Meta = TextureMeta;

    fn extract_meta(&self) -> TextureMeta {
        TextureMeta {
            width: self.width,
            height: self.height,
            format: self.format,
        }
    }
}

impl From<MirxLoadError> for crate::core::resource::LoadError {
    fn from(err: MirxLoadError) -> Self {
        crate::core::resource::LoadError::Failed(match err {
            MirxLoadError::Read(_) => "mirx read failed",
            MirxLoadError::Image(_) => "mirx image metadata is invalid",
            MirxLoadError::Surface(_) => "mirx flat surface is invalid",
            MirxLoadError::Plan(_) => "mirx output layout is invalid",
            MirxLoadError::Groups(_) => "mirx image groups are invalid",
            MirxLoadError::Request(_) => "mirx decode request is unsupported",
            MirxLoadError::Decode(_) => "mirx image decode failed",
            MirxLoadError::Copy(_) => "mirx image transfer failed",
            MirxLoadError::UnsupportedFormat(_) => "mirx format not supported by mirui",
            MirxLoadError::UnsupportedLayout(_) => "mirx sample layout not supported by mirui",
            MirxLoadError::NoImageChunk => "mirx file has no IMAGE chunk",
            MirxLoadError::DimensionOverflow => "mirx image dimensions exceed u16",
            MirxLoadError::AllocationSizeOverflow => "mirx decode allocation size overflow",
            MirxLoadError::ExternalMemoryRequired => {
                "mirx output or workspace requires caller-owned memory"
            }
        })
    }
}

/// Looks up mirx bytes by token via `fetch`; `None` lets the chain continue.
pub struct MirxLoader<F> {
    fetch: F,
    options: MirxTextureOptions,
}

impl<F> MirxLoader<F>
where
    F: Fn(&str) -> Option<&'static [u8]> + 'static,
{
    pub fn new(fetch: F) -> Self {
        Self {
            fetch,
            options: MirxTextureOptions::new(),
        }
    }

    pub const fn with_options(mut self, options: MirxTextureOptions) -> Self {
        self.options = options;
        self
    }
}

impl<F> crate::core::resource::Loader<Texture<'static>> for MirxLoader<F>
where
    F: Fn(&str) -> Option<&'static [u8]> + 'static,
{
    fn try_load(&self, token: &str) -> Result<Texture<'static>, crate::core::resource::LoadError> {
        let bytes = (self.fetch)(token).ok_or(crate::core::resource::LoadError::NotMine)?;
        Texture::from_mirx_with(bytes, self.options).map_err(Into::into)
    }
}

impl crate::core::resource::ResourceManager<Texture<'static>> {
    /// Register `bytes` under `token`. Peeks meta eagerly so layout queries
    /// skip the full decode path.
    pub fn add_mirx_bytes(
        &self,
        token: impl Into<alloc::borrow::Cow<'static, str>>,
        bytes: &'static [u8],
    ) -> Result<(), MirxLoadError> {
        self.add_mirx_bytes_with(token, bytes, MirxTextureOptions::new())
    }

    /// Registers MIRX bytes with explicit decode limits and hardware layout
    /// requirements. Metadata probing does not decode pixel storage.
    pub fn add_mirx_bytes_with(
        &self,
        token: impl Into<alloc::borrow::Cow<'static, str>>,
        bytes: &'static [u8],
        options: MirxTextureOptions,
    ) -> Result<(), MirxLoadError> {
        let meta = Texture::probe_mirx(bytes)?;
        self.add_probed_factory(token, meta, move || {
            Texture::from_mirx_with(bytes, options).ok()
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb8888_roundtrip() {
        let mut buf = [0u8; 4];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888);
        let c = Color::rgba(100, 200, 50, 255);
        tex.set_pixel(0, 0, &c);
        assert_eq!(tex.get_pixel(0, 0), c);
    }

    #[test]
    fn bgra8888_roundtrip() {
        let mut buf = [0u8; 4];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::BGRA8888);
        let c = Color::rgba(100, 200, 50, 255);
        tex.set_pixel(0, 0, &c);
        assert_eq!(tex.get_pixel(0, 0), c);
        assert_eq!(buf, [c.b, c.g, c.r, c.a]);
    }

    #[test]
    fn bgra8888_pack_byte_order() {
        let c = Color::rgba(0xAA, 0xBB, 0xCC, 0xDD);
        let bgra = ColorFormat::BGRA8888.pack(&c);
        // LE u32: byte 0=B, 1=G, 2=R, 3=A.
        assert_eq!(bgra & 0xFF, c.b as u32);
        assert_eq!((bgra >> 8) & 0xFF, c.g as u32);
        assert_eq!((bgra >> 16) & 0xFF, c.r as u32);
        assert_eq!((bgra >> 24) & 0xFF, c.a as u32);
    }

    #[test]
    fn rgb565_roundtrip() {
        let mut buf = [0u8; 2];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGB565);
        let c = Color::rgb(248, 252, 248); // values that survive 565 truncation
        tex.set_pixel(0, 0, &c);
        let got = tex.get_pixel(0, 0);
        assert_eq!(got.r, c.r);
        assert_eq!(got.g, c.g);
        assert_eq!(got.b, c.b);
    }

    #[test]
    fn blend_50_percent() {
        let mut buf = [0u8; 4];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888);
        tex.set_pixel(0, 0, &Color::rgb(0, 0, 0));
        tex.blend_pixel(Fixed::ZERO, Fixed::ZERO, &Color::rgb(200, 100, 50), 128);
        let got = tex.get_pixel(0, 0);
        assert!((got.r as i32 - 100).abs() <= 1);
        assert!((got.g as i32 - 50).abs() <= 1);
        assert!((got.b as i32 - 25).abs() <= 1);
    }

    #[test]
    fn blend_rgb565() {
        let mut buf = [0u8; 2];
        let mut tex = Texture::new(&mut buf, 1, 1, ColorFormat::RGB565);
        tex.set_pixel(0, 0, &Color::rgb(0, 0, 0));
        tex.blend_pixel(Fixed::ZERO, Fixed::ZERO, &Color::rgb(255, 255, 255), 255);
        let got = tex.get_pixel(0, 0);
        assert_eq!(got.r, 248);
        assert_eq!(got.g, 252);
        assert_eq!(got.b, 248);
    }

    #[test]
    fn blend_subpixel_spreads_to_neighbors() {
        // A point at (0.5, 0.5) should spread to all 4 pixels
        let mut buf = [0u8; 4 * 4]; // 2x2 RGBA8888
        let mut tex = Texture::new(&mut buf, 2, 2, ColorFormat::RGBA8888);
        tex.set_pixel(0, 0, &Color::rgb(0, 0, 0));
        tex.set_pixel(1, 0, &Color::rgb(0, 0, 0));
        tex.set_pixel(0, 1, &Color::rgb(0, 0, 0));
        tex.set_pixel(1, 1, &Color::rgb(0, 0, 0));

        tex.blend_pixel(Fixed::HALF, Fixed::HALF, &Color::rgb(255, 255, 255), 255);

        // Each pixel should get ~25% coverage
        let tl = tex.get_pixel(0, 0);
        let tr = tex.get_pixel(1, 0);
        let bl = tex.get_pixel(0, 1);
        let br = tex.get_pixel(1, 1);
        // All should be non-zero (got some coverage)
        assert!(tl.r > 0, "top-left should have coverage");
        assert!(tr.r > 0, "top-right should have coverage");
        assert!(bl.r > 0, "bottom-left should have coverage");
        assert!(br.r > 0, "bottom-right should have coverage");
        // Sum should be ~255
        // 24.8 fixed-point: 3 multiplications + 1 division per pixel, 4 pixels
        // max accumulated error ≈ 4 * 3/256 * 255 ≈ 12
        let sum = tl.r as u16 + tr.r as u16 + bl.r as u16 + br.r as u16;
        assert!(
            (sum as i32 - 255).abs() <= 12,
            "total coverage sum={sum} should be ~255 (±12 for 24.8 precision)"
        );
    }

    use crate::core::resource::HasProbe;

    fn build_flat_rgb565_2x1() -> &'static [u8] {
        let bytes = mirx::Document::new_flat(mirx::image::ImageAsset::new(
            2,
            1,
            mirx::image::ColorFormat::RGB565,
            4,
            alloc::borrow::Cow::Borrowed(&[0xAA, 0xBB, 0xCC, 0xDD]),
        ))
        .unwrap()
        .finish()
        .unwrap()
        .into_owned();
        Box::leak(bytes.into_boxed_slice())
    }

    #[derive(Clone, Copy)]
    enum TestCoding {
        Pixel,
        Rle,
        Lz4,
        FrequencyReversible,
        FrequencyQuantized,
    }

    fn build_encoded_rgb(coding: TestCoding) -> &'static [u8] {
        use mirx::{
            Document,
            coding::{Frequency, FrequencyGeometry, Lz4, Pixel, Rle},
            image::{ColorDescription, EncodedImageAsset, SampleLayout, SurfaceDescriptor},
        };

        let samples = [17, 42, 91, 111, 7, 203];
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
        let mut encoded = [0; 512];
        let mut frequency_params = [0];
        let (record, len) = match coding {
            TestCoding::Pixel => {
                let codec = Pixel::new(SampleLayout::RGB888).unwrap();
                (
                    codec.record(),
                    codec.encode_into(&samples, &mut encoded).unwrap(),
                )
            }
            TestCoding::Rle => {
                let codec = Rle::new().with_element_size(3).unwrap();
                (
                    codec.record(),
                    codec.encode_into(&samples, &mut encoded).unwrap(),
                )
            }
            TestCoding::Lz4 => {
                let codec = Lz4::new();
                let mut table = [0; Lz4::TABLE_LEN];
                let len = codec
                    .encoder(&mut table)
                    .unwrap()
                    .encode_into(&samples, &mut encoded)
                    .unwrap();
                (codec.record(), len)
            }
            TestCoding::FrequencyReversible => {
                let codec = Frequency::reversible();
                let geometry = FrequencyGeometry::for_plane(SampleLayout::RGB888, 0, 2, 1).unwrap();
                (
                    codec.record_into(&mut frequency_params),
                    codec.encode_into(geometry, &samples, &mut encoded).unwrap(),
                )
            }
            TestCoding::FrequencyQuantized => {
                let codec = Frequency::quantized(50).unwrap();
                let geometry = FrequencyGeometry::for_plane(SampleLayout::RGB888, 0, 2, 1).unwrap();
                (
                    codec.record_into(&mut frequency_params),
                    codec.encode_into(geometry, &samples, &mut encoded).unwrap(),
                )
            }
        };
        let image = EncodedImageAsset::new(surface, record, &encoded[..len]);
        let mut document = Document::new();
        let id = document.push_encoded_image(&image).unwrap();
        document.set_primary(id).unwrap();
        Box::leak(
            document
                .encode(&Default::default())
                .unwrap()
                .into_boxed_slice(),
        )
    }

    #[test]
    fn from_mirx_flat_rgb565_round_trip() {
        let tex = Texture::from_mirx(build_flat_rgb565_2x1()).expect("flat parse");
        assert_eq!(tex.width, 2);
        assert_eq!(tex.height, 1);
        assert_eq!(tex.format, ColorFormat::RGB565);
        assert_eq!(tex.alpha_mode, AlphaMode::Opaque);
        assert_eq!(tex.buf.as_slice(), &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn from_mirx_zero_copy_borrow() {
        let bytes = build_flat_rgb565_2x1();
        let tex = Texture::from_mirx(bytes).unwrap();
        let pix_ptr = tex.buf.as_slice().as_ptr();
        let buf_ptr = bytes.as_ptr();
        assert_eq!(pix_ptr as usize - buf_ptr as usize, 28);
    }

    #[test]
    fn raw_mirx_with_explicit_reconstruction_owns_the_output() {
        let bytes = build_flat_rgb565_2x1();
        let options =
            MirxTextureOptions::new().with_input_memory(mirx::image::MemoryPlacement::Flash);
        let tex = Texture::from_mirx_with(bytes, options).unwrap();
        assert!(matches!(&tex.buf, TexBuf::Aligned(_)));
        assert_eq!(tex.buf.as_slice(), &[0xAA, 0xBB, 0xCC, 0xDD]);
    }

    #[test]
    fn from_mirx_decodes_every_lossless_profile() {
        for coding in [
            TestCoding::Pixel,
            TestCoding::Rle,
            TestCoding::Lz4,
            TestCoding::FrequencyReversible,
        ] {
            let tex = Texture::from_mirx(build_encoded_rgb(coding)).unwrap();
            assert_eq!(tex.width, 2);
            assert_eq!(tex.height, 1);
            assert_eq!(tex.stride, 6);
            assert_eq!(tex.format, ColorFormat::RGB888);
            assert_eq!(tex.buf.as_slice(), &[17, 42, 91, 111, 7, 203]);
            assert!(matches!(tex.buf, TexBuf::Aligned(_)));
        }
    }

    #[test]
    fn from_mirx_decodes_quantized_frequency_pixels() {
        let tex = Texture::from_mirx(build_encoded_rgb(TestCoding::FrequencyQuantized)).unwrap();
        assert_eq!((tex.width, tex.height, tex.stride), (2, 1, 6));
        assert_eq!(tex.format, ColorFormat::RGB888);
        assert_ne!(tex.buf.as_slice(), &[17, 42, 91, 111, 7, 203]);
        for (before, after) in [17u8, 42, 91, 111, 7, 203]
            .into_iter()
            .zip(tex.buf.as_slice().iter().copied())
        {
            assert!(before.abs_diff(after) <= 52);
        }
    }

    #[test]
    fn mirx_plan_applies_gpu_width_stride_and_address_constraints() {
        let bytes = build_encoded_rgb(TestCoding::Rle);
        let requirements = mirx::image::SurfaceRequirements::new()
            .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
            .with_plane_alignment(mirx::types::ByteAlignment::new(64).unwrap())
            .with_width_multiple(64)
            .with_stride_multiple(64);
        let options = MirxTextureOptions::new().with_requirements(requirements);
        let tex = Texture::from_mirx_with(bytes, options).unwrap();
        assert_eq!(tex.width, 2);
        assert_eq!(tex.height, 1);
        assert_eq!(tex.stride, 192);
        assert_eq!(tex.buf.as_slice().as_ptr() as usize % 64, 0);
        assert_eq!(&tex.buf.as_slice()[..6], &[17, 42, 91, 111, 7, 203]);
        assert_eq!(&tex.buf.as_slice()[6..], &[0; 186]);
        let TexBuf::Aligned(storage) = tex.buf else {
            panic!("encoded output must own its aligned allocation")
        };
        assert_eq!(storage.alignment(), 64);
    }

    #[test]
    fn mirx_plan_decodes_into_fixed_caller_storage() {
        #[repr(align(64))]
        struct Output([u8; 256]);

        #[repr(align(64))]
        struct Workspace([u8; 64]);

        let bytes = build_encoded_rgb(TestCoding::Pixel);
        let requirements = mirx::image::SurfaceRequirements::new()
            .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
            .with_stride_multiple(64);
        let options = MirxTextureOptions::new()
            .with_requirements(requirements)
            .with_input_memory(mirx::image::MemoryPlacement::Flash)
            .with_output_memory(mirx::image::MemoryPlacement::SharedNoncoherent)
            .with_workspace_memory(mirx::image::MemoryPlacement::SharedCoherent)
            .with_workspace_alignment(mirx::types::ByteAlignment::new(64).unwrap());
        let mut groups = [None];
        let plan = Texture::plan_mirx(bytes, options, &mut groups).unwrap();
        assert_eq!(plan.output_len(), 64);
        assert_eq!(plan.output_alignment(), 64);
        assert_eq!(plan.workspace_len(), 6);
        assert_eq!(plan.workspace_alignment(), 64);
        assert_eq!(plan.decode_request(), options.decode_request());
        assert_eq!(plan.input_sync(), mirx::image::CacheSync::None);
        assert_eq!(plan.output_sync(), mirx::image::CacheSync::CleanAfterWrite);
        let mut output = Output([0xa5; 256]);
        let mut workspace = Workspace([0x5a; 64]);
        let texture = plan.decode_into(&mut output.0, &mut workspace.0).unwrap();
        assert_eq!(texture.buf.as_slice().as_ptr() as usize % 64, 0);
        assert_eq!(texture.stride, 64);
        assert_eq!(&texture.buf.as_slice()[..6], &[17, 42, 91, 111, 7, 203]);
        assert_eq!(&texture.buf.as_slice()[6..], &[0; 58]);
        assert_eq!(&output.0[64..], &[0xa5; 192]);
        assert_eq!(&workspace.0[6..], &[0x5a; 58]);
    }

    #[test]
    fn managed_mirx_load_rejects_external_memory_and_invalid_execution() {
        let bytes = build_encoded_rgb(TestCoding::Rle);
        let external = MirxTextureOptions::new()
            .with_output_memory(mirx::image::MemoryPlacement::SharedCoherent);
        assert!(matches!(
            Texture::from_mirx_with(bytes, external),
            Err(MirxLoadError::ExternalMemoryRequired)
        ));

        let bad: &'static [u8] = Box::leak(Box::new([0u8; 8]));
        let direct =
            MirxTextureOptions::new().with_execution(mirx::image::DecodeExecution::DirectUpload);
        assert!(matches!(
            Texture::from_mirx_with(bad, direct),
            Err(MirxLoadError::Request(
                mirx::image::DecodeRequestError::UnsupportedExecution(
                    mirx::image::DecodeExecution::DirectUpload
                )
            ))
        ));
    }

    #[test]
    fn from_mirx_bad_magic_propagates_parse_error() {
        let bad: &'static [u8] = Box::leak(Box::new([0u8; 8]));
        assert!(matches!(
            Texture::from_mirx(bad),
            Err(MirxLoadError::Read(mirx::ReadError::BadMagic))
        ));
    }

    #[test]
    fn extract_meta_returns_geometry() {
        let tex = Texture::from_mirx(build_flat_rgb565_2x1()).unwrap();
        let meta = tex.extract_meta();
        assert_eq!(meta.width, 2);
        assert_eq!(meta.height, 1);
        assert_eq!(meta.format, ColorFormat::RGB565);
    }

    use crate::core::cache::MaxSize;
    use crate::core::resource::ResourceManager;

    fn texture_manager() -> ResourceManager<Texture<'static>> {
        let m = ResourceManager::<Texture<'static>>::new(
            MaxSize::Bytes(1024),
            Texture::from_mirx(build_flat_rgb565_2x1()).unwrap(),
        );
        m.enable_probes(
            MaxSize::Count(8),
            TextureMeta {
                width: 0,
                height: 0,
                format: ColorFormat::RGB565,
            },
        );
        m
    }

    #[test]
    fn add_mirx_bytes_populates_probe_and_value() {
        let m = texture_manager();
        let bytes = build_flat_rgb565_2x1();
        m.add_mirx_bytes("logo", bytes).expect("register ok");

        assert_eq!(
            m.probe("logo"),
            Some(TextureMeta {
                width: 2,
                height: 1,
                format: ColorFormat::RGB565,
            })
        );

        let tex = m.resolve("logo");
        assert_eq!(tex.width, 2);
        assert_eq!(tex.format, ColorFormat::RGB565);
    }

    #[test]
    fn add_mirx_bytes_probes_then_caches_decoded_storage() {
        let m = texture_manager();
        let bytes = build_encoded_rgb(TestCoding::Lz4);
        m.add_mirx_bytes("compressed", bytes).unwrap();

        assert_eq!(
            m.probe("compressed"),
            Some(TextureMeta {
                width: 2,
                height: 1,
                format: ColorFormat::RGB888,
            })
        );
        let first = m.resolve("compressed");
        let second = m.resolve("compressed");
        assert!(alloc::rc::Rc::ptr_eq(&first, &second));
        assert_eq!(first.buf.as_slice(), &[17, 42, 91, 111, 7, 203]);
        assert!(matches!(first.buf, TexBuf::Aligned(_)));
    }

    #[test]
    fn add_mirx_bytes_rejects_bad_input() {
        let m = texture_manager();
        let bad: &'static [u8] = Box::leak(Box::new([0u8; 8]));
        let err = m.add_mirx_bytes("bad", bad).unwrap_err();
        assert!(matches!(
            err,
            MirxLoadError::Read(mirx::ReadError::BadMagic)
        ));
    }

    #[test]
    fn mirx_loader_resolves_via_chain() {
        let m = texture_manager();
        let bytes = build_flat_rgb565_2x1();
        m.add_loader(MirxLoader::new(move |t| {
            if t == "via-loader" { Some(bytes) } else { None }
        }));

        let tex = m.resolve("via-loader");
        assert_eq!(tex.width, 2);
        assert_eq!(tex.format, ColorFormat::RGB565);
    }

    #[test]
    fn mirx_loader_returns_not_mine_when_fetch_misses() {
        let m = texture_manager();
        let bytes = build_flat_rgb565_2x1();
        m.add_loader(MirxLoader::new(move |t| {
            if t == "logo" { Some(bytes) } else { None }
        }));

        let tex = m.resolve("nope");
        assert_eq!(tex.width, 2, "unhandled tokens fall through to fallback");
    }
}
