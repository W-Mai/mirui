use super::renderer::{RenderError, RenderResource};
use super::scratch::PlaneLayout;
use crate::core::cache::HasSize;

fn state_error(error: crate::core::threading::AccessError) -> RenderError {
    use crate::core::threading::AccessError;
    match error {
        AccessError::Reentrant => RenderError::InvalidScope,
        AccessError::Busy => {
            #[cfg(feature = "sync")]
            {
                RenderError::ResourcePending
            }
            #[cfg(not(feature = "sync"))]
            {
                RenderError::InvalidScope
            }
        }
        #[cfg(feature = "sync")]
        AccessError::Capacity => RenderError::ResourceLimit(RenderResource::Texture),
    }
}

mod access;
#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32", test))]
mod coverage;
mod decoded;
mod indexed;
mod native;
mod pixel_cache;
mod pixels;
mod region;
mod source;
#[cfg(test)]
#[path = "../../tests/unit/render/texture/thread_tests.rs"]
mod thread_tests;
#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
pub(crate) use coverage::UploadCoverage;
pub(crate) use source::SourceMirxLoader;
mod upload;
pub use access::{TexturePixels, TexturePixelsMut};
pub use mirx::image::Region;
pub use region::TextureRegion;

#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
mod cache_key;
use crate::types::Color;
#[cfg(test)]
use crate::types::Fixed;
#[cfg(test)]
use alloc::borrow::Cow;
use alloc::vec::Vec;
#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
pub(crate) use cache_key::TextureKey;

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
    pub(crate) fn zeroed(len: usize, alignment: usize) -> Option<Self> {
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

impl AsRef<[u8]> for AlignedBytes {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsMut<[u8]> for AlignedBytes {
    fn as_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
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

enum TexBuf<'a> {
    Ref(&'a [u8]),
    Indexed(&'a [u8], indexed::IndexPlane<'a>),
    IndexedAligned(alloc::boxed::Box<(AlignedBytes, indexed::IndexPlane<'a>)>),
    Mut(&'a mut [u8]),
    Owned(Vec<u8>),
    BudgetedOwned {
        bytes: Vec<u8>,
        _allocation: crate::render::budget::Allocation,
    },
    Aligned(AlignedBytes),
    Native(alloc::boxed::Box<native::NativeTexture>),
    Mapped(alloc::boxed::Box<source::MappedTexture>),
    Decoded(alloc::boxed::Box<decoded::DecodedTexture>),
}

impl TexBuf<'_> {
    fn cpu_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Ref(s) => Some(s),
            Self::Mut(s) => Some(s),
            Self::Owned(v) | Self::BudgetedOwned { bytes: v, .. } => Some(v),
            Self::Aligned(v) => Some(v.as_slice()),
            Self::Native(_) | Self::Decoded(_) | Self::Indexed(..) | Self::IndexedAligned(_) => {
                None
            }
            Self::Mapped(source) => source.pixels(),
        }
    }

    #[cfg(test)]
    fn as_slice(&self) -> &[u8] {
        self.cpu_bytes().expect("CPU test storage")
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

/// Composites straight RGB over a straight RGBA destination. `alpha` is the effective source alpha, including any opacity or coverage already applied to `source.a`.
#[inline(always)]
pub fn blend_straight_source_over(source: &Color, destination: &Color, alpha: u8) -> Color {
    if alpha == 0 {
        return *destination;
    }
    if alpha == 255 {
        return Color { a: 255, ..*source };
    }
    let source_alpha = u32::from(alpha);
    let inverse = 255 - source_alpha;
    let destination_alpha = u32::from(destination.a);
    let denominator = source_alpha * 255 + destination_alpha * inverse;
    let channel = |source: u8, destination: u8| {
        ((u32::from(source) * source_alpha * 255
            + u32::from(destination) * destination_alpha * inverse
            + denominator / 2)
            / denominator) as u8
    };
    Color {
        r: channel(source.r, destination.r),
        g: channel(source.g, destination.g),
        b: channel(source.b, destination.b),
        a: ((denominator + 127) / 255) as u8,
    }
}

/// Pixel storage sampled with nearest-neighbor filtering when blitted.
/// Glyph coverage and SDF atlases use their own interpolating samplers.
///
/// Pixel constructors return [`RenderError::InvalidTexture`] for empty dimensions, overflowing layouts, or insufficient storage. [`Texture::owned`] reports allocation failure as [`RenderError::ResourceLimit`].
///
/// A texture is not implicitly duplicated. Cache handles can share an existing resource; a snapshot owns an explicit pixel copy.
/// Native storage prepares CPU pixels through [`Texture::read_pixels`]. With the `sync` feature, textures, pixel guards and their cache value handles support `Send + Sync` while preserving their borrowed lifetimes. Thread-bound native reads still require their creating thread; closing that owner invalidates native access but preserves prepared CPU regions. CPU constructors do not allocate native storage or readback caches. Without `sync`, textures retain local ownership.
///
/// ```compile_fail,E0277
/// use mirui::render::texture::Texture;
/// fn requires_clone<T: Clone>() {}
/// requires_clone::<Texture<'static>>();
/// ```
pub struct Texture<'a> {
    buf: TexBuf<'a>,
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) format: ColorFormat,
    pub(crate) stride: usize,
    pub alpha_mode: AlphaMode,
    /// Increment when reusing a buffer with new pixels and retaining GPU upload caching.
    /// Borrowed and mutable constructors bypass the cache until a revision is supplied.
    pub cache_revision: u64,
    /// Bypass GPU-side upload caches keyed by source identity and revision.
    pub transient: bool,
}

impl HasSize for Texture<'_> {
    fn cache_size(&self) -> usize {
        match &self.buf {
            TexBuf::Ref(bytes) => bytes.len(),
            TexBuf::Indexed(bytes, plane) => bytes.len() + plane.palette.as_bytes().len(),
            TexBuf::IndexedAligned(source) => {
                source.0.storage.capacity()
                    + source.1.palette.as_bytes().len()
                    + core::mem::size_of_val(&**source)
            }
            TexBuf::Mut(bytes) => bytes.len(),
            TexBuf::Owned(bytes) | TexBuf::BudgetedOwned { bytes, .. } => bytes.capacity(),
            TexBuf::Aligned(bytes) => bytes.storage.capacity(),
            TexBuf::Native(native) => native.cache_size(),
            TexBuf::Mapped(source) => source.cache_size(),
            TexBuf::Decoded(source) => source.cache_size(),
        }
    }
}

impl Texture<'static> {
    pub(crate) const fn expect_static(
        buf: &'static [u8],
        width: u16,
        height: u16,
        format: ColorFormat,
    ) -> Self {
        let layout = match Self::checked_layout(buf.len(), width, height, format) {
            Ok(layout) => layout,
            Err(_) => panic!("invalid static texture layout"),
        };
        Self::from_layout(TexBuf::Ref(buf), layout, false)
    }

    /// Cacheable pixels with static storage.
    pub const fn from_static(
        buf: &'static [u8],
        width: u16,
        height: u16,
        format: ColorFormat,
    ) -> Result<Self, RenderError> {
        let layout = match Self::checked_layout(buf.len(), width, height, format) {
            Ok(layout) => layout,
            Err(error) => return Err(error),
        };
        Ok(Self::from_layout(TexBuf::Ref(buf), layout, false))
    }
}

impl<'a> Texture<'a> {
    pub(super) fn from_plane(plane: &'a mut crate::render::scratch::AlignedPlane<'_>) -> Self {
        let layout = plane.layout();
        Self::from_layout(TexBuf::Mut(plane.bytes_mut()), layout, true)
    }

    /// Mutable pixels; GPU uploads bypass pointer-keyed caches by default.
    pub fn new(
        buf: &'a mut [u8],
        width: u16,
        height: u16,
        format: ColorFormat,
    ) -> Result<Self, RenderError> {
        let layout = Self::checked_layout(buf.len(), width, height, format)?;
        Ok(Self::from_layout(TexBuf::Mut(buf), layout, true))
    }

    /// Borrowed pixels; GPU uploads bypass pointer-keyed caches by default.
    pub fn from_ref(
        buf: &'a [u8],
        width: u16,
        height: u16,
        format: ColorFormat,
    ) -> Result<Self, RenderError> {
        let layout = Self::checked_layout(buf.len(), width, height, format)?;
        Ok(Self::from_layout(TexBuf::Ref(buf), layout, true))
    }
}

impl Texture<'static> {
    pub(crate) fn with_allocation(mut self, allocation: crate::render::budget::Allocation) -> Self {
        let TexBuf::Owned(bytes) = self.buf else {
            panic!("output allocation requires owned CPU pixels");
        };
        assert_eq!(allocation.bytes(), bytes.capacity());
        self.buf = TexBuf::BudgetedOwned {
            bytes,
            _allocation: allocation,
        };
        self
    }

    /// Owned mutable pixels; GPU uploads bypass pointer-keyed caches by default.
    pub fn owned(width: u16, height: u16, format: ColorFormat) -> Result<Self, RenderError> {
        let layout =
            PlaneLayout::packed(width, height, format).map_err(|_| RenderError::InvalidTexture)?;
        let mut buf = Vec::new();
        buf.try_reserve_exact(layout.required_bytes())
            .map_err(|_| RenderError::ResourceLimit(RenderResource::Texture))?;
        buf.resize(layout.required_bytes(), 0);
        Ok(Self::from_layout(TexBuf::Owned(buf), layout, true))
    }

    /// Take ownership of packed pixel bytes without copying them.
    pub fn from_vec(
        buf: Vec<u8>,
        width: u16,
        height: u16,
        format: ColorFormat,
    ) -> Result<Self, RenderError> {
        let layout = Self::checked_layout(buf.len(), width, height, format)?;
        Ok(Self::from_layout(TexBuf::Owned(buf), layout, true))
    }
}

impl<'a> Texture<'a> {
    const fn checked_layout(
        len: usize,
        width: u16,
        height: u16,
        format: ColorFormat,
    ) -> Result<PlaneLayout, RenderError> {
        let layout = match PlaneLayout::packed(width, height, format) {
            Ok(layout) => layout,
            Err(_) => return Err(RenderError::InvalidTexture),
        };
        if len < layout.minimum_bytes() {
            return Err(RenderError::InvalidTexture);
        }
        Ok(layout)
    }

    const fn from_layout(buf: TexBuf<'a>, layout: PlaneLayout, transient: bool) -> Self {
        Self {
            buf,
            width: layout.width(),
            height: layout.height(),
            format: layout.format(),
            stride: layout.stride_bytes(),
            alpha_mode: AlphaMode::Opaque,
            cache_revision: 0,
            transient,
        }
    }

    pub fn with_transient(mut self, transient: bool) -> Self {
        self.transient = transient;
        self
    }

    /// Opt into GPU upload caching with an explicit content revision.
    pub fn with_cache_revision(mut self, revision: u64) -> Self {
        self.cache_revision = revision;
        self.transient = false;
        self
    }

    pub(crate) fn valid_storage(&self) -> bool {
        self.pixel_layout().is_ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirxLoadError {
    Texture(RenderError),
    Read(mirx::reader::ReadError),
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

impl From<mirx::reader::ReadError> for MirxLoadError {
    fn from(err: mirx::reader::ReadError) -> Self {
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
        indexed if indexed.indexed_bit_depth().is_some() => Ok(ColorFormat::RGBA8888),
        other => Err(MirxLoadError::UnsupportedFormat(other)),
    }
}

/// Decode execution, memory placement, physical layout, and limits for a MIRX texture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MirxTextureOptions {
    request: mirx::image::DecodeRequest,
    limits: mirx::reader::PayloadLimits,
}

impl MirxTextureOptions {
    pub const fn new() -> Self {
        Self {
            request: mirx::image::DecodeRequest::new(mirx::image::SurfaceRequirements::new()),
            limits: mirx::reader::PayloadLimits::EMBEDDED,
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

    pub const fn with_limits(mut self, limits: mirx::reader::PayloadLimits) -> Self {
        self.limits = limits;
        self
    }

    pub const fn requirements(self) -> mirx::image::SurfaceRequirements {
        self.request.requirements()
    }

    pub const fn decode_request(self) -> mirx::image::DecodeRequest {
        self.request
    }

    pub const fn limits(self) -> mirx::reader::PayloadLimits {
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
    if source_format.indexed_bit_depth().is_some() {
        if plane.memory().flags().bits() != 0 {
            return Err(MirxLoadError::UnsupportedLayout(descriptor.sample_layout()));
        }
        let logical = PlaneLayout::packed(
            descriptor.width().try_into()?,
            descriptor.height().try_into()?,
            format,
        )
        .map_err(|_| MirxLoadError::Texture(RenderError::InvalidTexture))?;
        let palette = surface
            .color_table()
            .ok_or(MirxLoadError::UnsupportedFormat(source_format))?;
        let index = indexed::IndexPlane::new(
            source_format,
            palette,
            plane.memory().stride() as usize,
            logical,
            plane.bytes(),
        )
        .map_err(MirxLoadError::Texture)?;
        return Ok(Texture::from_layout(
            TexBuf::Indexed(plane.bytes(), index),
            logical,
            true,
        ));
    }
    Texture::from_ref(
        plane.bytes(),
        descriptor.width().try_into()?,
        descriptor.height().try_into()?,
        format,
    )
    .and_then(|texture| texture.with_stride(plane.memory().stride() as usize))
    .map_err(MirxLoadError::Texture)
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
        Self::plan_image(image, options, group_slots)
    }

    fn plan_image<'source, 'groups>(
        image: mirx::image::ImageRef<'source>,
        options: MirxTextureOptions,
        group_slots: &'groups mut [Option<mirx::image::UnitGroup<'source>>],
    ) -> Result<MirxTexturePlan<'source, 'groups>, MirxLoadError> {
        let request = options.decode_request();
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
        Self::load_image(options, || open_mirx_image(bytes))
    }

    /// Creates a texture from an image in an already open MIRX container.
    pub fn from_mirx_image(
        image: mirx::image::ImageRef<'static>,
        options: MirxTextureOptions,
    ) -> Result<Self, MirxLoadError> {
        Self::load_image(options, || Ok(image))
    }

    fn load_image(
        options: MirxTextureOptions,
        image: impl FnOnce() -> Result<mirx::image::ImageRef<'static>, MirxLoadError>,
    ) -> Result<Self, MirxLoadError> {
        let request = options.decode_request();
        request.validate_reconstruction()?;
        if request.output() != mirx::image::MemoryPlacement::Cpu
            || request.workspace() != mirx::image::MemoryPlacement::Cpu
        {
            return Err(MirxLoadError::ExternalMemoryRequired);
        }
        let image = image()?;
        if request == mirx::image::DecodeRequest::default() {
            if let mirx::image::ImageRef::Raw(surface) = image {
                return texture_from_surface(surface).map(|texture| texture.with_transient(false));
            }
        }
        let group_count = image.encoded().map_or(0, |image| image.group_count());
        let mut group_slots = alloc::vec![None; group_count];
        let plan = Self::plan_image(image, options, &mut group_slots)?;
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
        let layout = decoded.pixel_layout().map_err(MirxLoadError::Texture)?;
        let index = if let TexBuf::Indexed(_, index) = decoded.buf {
            Some(indexed::IndexPlane {
                layout: index.layout,
                palette: image.color_table().expect("validated indexed palette"),
            })
        } else {
            None
        };
        debug_assert_eq!(memory.plane(0).map(|plane| plane.data_offset()), Some(0));
        let storage = match index {
            Some(index) => TexBuf::IndexedAligned(alloc::boxed::Box::new((output, index))),
            None => TexBuf::Aligned(output),
        };
        Ok(Self::from_layout(storage, layout, false))
    }
}

/// Cheap geometry snapshot used as [`HasProbe::Meta`](crate::core::resource::HasProbe::Meta).
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
            MirxLoadError::Texture(_) => "mirx texture storage is invalid",
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
    type State = ();

    fn recognize(
        &self,
        input: crate::core::resource::LoadInput<'_>,
    ) -> crate::core::resource::FormatMatch {
        use crate::core::resource::{FormatMatch, LoadInput};
        match input {
            LoadInput::Token(_) => FormatMatch::Match,
            LoadInput::Bytes { .. } => FormatMatch::NoMatch,
        }
    }

    fn poll_load(
        &self,
        _: &mut (),
        cx: &mut crate::core::resource::LoadContext<'_>,
    ) -> core::task::Poll<Result<Texture<'static>, crate::core::resource::LoadError>> {
        let bytes = (self.fetch)(cx.token()).ok_or(crate::core::resource::LoadError::NotMine)?;
        core::task::Poll::Ready(Texture::from_mirx_with(bytes, self.options).map_err(Into::into))
    }
}

impl<F> crate::core::resource::ProbeLoader<Texture<'static>> for MirxLoader<F>
where
    F: Fn(&str) -> Option<&'static [u8]> + 'static,
{
    fn poll_probe(
        &self,
        _: &mut (),
        cx: &mut crate::core::resource::LoadContext<'_>,
    ) -> core::task::Poll<Result<TextureMeta, crate::core::resource::LoadError>> {
        let bytes = (self.fetch)(cx.token()).ok_or(crate::core::resource::LoadError::NotMine)?;
        core::task::Poll::Ready(Texture::probe_mirx(bytes).map_err(Into::into))
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
#[path = "../../tests/unit/render/texture.rs"]
mod tests;
