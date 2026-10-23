use super::renderer::{RenderError, RenderResource};
use super::scratch::PlaneLayout;
use crate::core::cache::HasSize;

mod access;
#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32", test))]
mod coverage;
mod native;
mod pixels;
mod region;
#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
pub(crate) use coverage::UploadCoverage;
#[cfg(any(feature = "wgpu", target_arch = "wasm32", test))]
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
    Mut(&'a mut [u8]),
    Owned(Vec<u8>),
    Aligned(AlignedBytes),
    Native(alloc::boxed::Box<native::NativeTexture>),
}

impl TexBuf<'_> {
    fn cpu_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Ref(s) => Some(s),
            Self::Mut(s) => Some(s),
            Self::Owned(v) => Some(v),
            Self::Aligned(v) => Some(v.as_slice()),
            Self::Native(_) => None,
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

#[inline(always)]
pub(crate) fn blend_straight_source_over(source: &Color, destination: &Color, alpha: u8) -> Color {
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
/// Native storage retains its source device and prepares CPU pixels through [`Texture::read_pixels`]. Textures and their read-only pixel guards are thread-local; CPU constructors do not allocate native storage or readback caches.
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
            TexBuf::Mut(bytes) => bytes.len(),
            TexBuf::Owned(bytes) => bytes.capacity(),
            TexBuf::Aligned(bytes) => bytes.storage.capacity(),
            TexBuf::Native(native) => native.cache_size(),
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
    Texture::from_ref(
        plane.bytes(),
        descriptor.width().try_into()?,
        descriptor.height().try_into()?,
        format,
    )
    .and_then(|texture| texture.with_stride(plane.memory().stride() as usize))
    .map(|texture| texture.with_transient(false))
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
        let layout = decoded.pixel_layout().map_err(MirxLoadError::Texture)?;
        drop(decoded);
        debug_assert_eq!(memory.plane(0).map(|plane| plane.data_offset()), Some(0));
        Ok(Self::from_layout(TexBuf::Aligned(output), layout, false))
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
    fn try_load(&self, token: &str) -> Result<Texture<'static>, crate::core::resource::LoadError> {
        let bytes = (self.fetch)(token).ok_or(crate::core::resource::LoadError::NotMine)?;
        Texture::from_mirx_with(bytes, self.options).map_err(Into::into)
    }
}

impl<F> crate::core::resource::ProbeLoader<Texture<'static>> for MirxLoader<F>
where
    F: Fn(&str) -> Option<&'static [u8]> + 'static,
{
    fn try_probe(&self, token: &str) -> Result<TextureMeta, crate::core::resource::LoadError> {
        let bytes = (self.fetch)(token).ok_or(crate::core::resource::LoadError::NotMine)?;
        Texture::probe_mirx(bytes).map_err(Into::into)
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
    fn cache_charges_owning_capacity_including_alignment_storage() {
        let mut bytes = Vec::with_capacity(64);
        bytes.resize(4, 0);
        let capacity = bytes.capacity();
        let texture = Texture::from_vec(bytes, 1, 1, ColorFormat::RGBA8888).unwrap();
        assert_eq!(texture.cache_size(), capacity);
        assert_eq!(texture.read_pixels().unwrap().bytes().len(), 4);

        let bytes = AlignedBytes::zeroed(4, 64).unwrap();
        let capacity = bytes.storage.capacity();
        let layout = PlaneLayout::packed(1, 1, ColorFormat::RGBA8888).unwrap();
        let texture = Texture::from_layout(TexBuf::Aligned(bytes), layout, false);
        assert_eq!(texture.cache_size(), capacity);
        assert!(capacity >= 67);
        assert_eq!(texture.read_pixels().unwrap().bytes().len(), 4);
    }

    #[test]
    fn static_assets_share_checked_layout_without_allocating() {
        static BYTES: [u8; 16] = [12; 16];
        const ASSET: Texture<'static> = Texture::expect_static(&BYTES, 2, 2, ColorFormat::RGBA8888);
        let allocations = crate::test_support::tracked_allocations(|| {
            let asset = ASSET;
            assert_eq!(
                asset.read_pixels().unwrap().bytes().as_ptr(),
                BYTES.as_ptr()
            );
            assert_eq!(asset.width(), 2);
            assert_eq!(asset.height(), 2);
            assert!(!asset.transient);
            for format in [
                ColorFormat::RGB565,
                ColorFormat::RGB565Swapped,
                ColorFormat::RGB888,
                ColorFormat::RGBA8888,
                ColorFormat::BGRA8888,
            ] {
                let asset = Texture::expect_static(&BYTES, 2, 2, format);
                let checked = Texture::from_static(&BYTES, 2, 2, format).unwrap();
                assert_eq!(asset.pixel_layout(), checked.pixel_layout());
                assert_eq!(
                    asset.read_pixels().unwrap().bytes().as_ptr(),
                    BYTES.as_ptr()
                );
            }
        });
        assert_eq!(allocations, 0);
        assert!(
            std::panic::catch_unwind(|| {
                Texture::expect_static(&BYTES, 0, 2, ColorFormat::RGBA8888)
            })
            .is_err()
        );
        assert!(
            std::panic::catch_unwind(|| {
                Texture::expect_static(&BYTES, 3, 2, ColorFormat::RGBA8888)
            })
            .is_err()
        );
    }

    #[test]
    fn constructors_reject_invalid_storage_before_exposing_pixels() {
        const SHORT: Result<Texture<'static>, RenderError> =
            Texture::from_static(&[0; 3], 1, 1, ColorFormat::RGBA8888);
        const VALID: Result<Texture<'static>, RenderError> =
            Texture::from_static(&[10, 20, 30, 40], 1, 1, ColorFormat::RGBA8888);
        assert!(matches!(SHORT, Err(RenderError::InvalidTexture)));
        assert_eq!(
            VALID.unwrap().read_pixels().unwrap().get_pixel(0, 0),
            Color::rgba(10, 20, 30, 40)
        );
        for format in [
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
            ColorFormat::RGB888,
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
        ] {
            let mut bytes = [0; 4];
            let short = format.bytes_per_pixel() - 1;
            assert!(matches!(
                Texture::new(&mut bytes[..short], 1, 1, format),
                Err(RenderError::InvalidTexture)
            ));
            assert!(matches!(
                Texture::from_ref(&bytes[..short], 1, 1, format),
                Err(RenderError::InvalidTexture)
            ));
            assert!(matches!(
                Texture::from_vec(bytes[..short].to_vec(), 1, 1, format),
                Err(RenderError::InvalidTexture)
            ));
            assert!(matches!(
                Texture::owned(0, 1, format),
                Err(RenderError::InvalidTexture)
            ));
            assert!(matches!(
                Texture::owned(1, 0, format),
                Err(RenderError::InvalidTexture)
            ));
            let owned = Texture::owned(1, 1, format).unwrap();
            assert!(owned.buf.as_slice().iter().all(|byte| *byte == 0));
            let borrowed = Texture::from_ref(&bytes, 1, 1, format).unwrap();
            assert_eq!(borrowed.buf.as_slice().as_ptr(), bytes.as_ptr());
        }
    }

    #[test]
    fn storage_validation_uses_the_last_pixel_of_padded_rows() {
        let mut texture =
            Texture::from_ref(&[0u8; 12], 2, 2, ColorFormat::RGB565).expect("valid pixel storage");
        texture.stride = 8;
        assert!(texture.valid_storage());
        texture.stride = 9;
        assert!(!texture.valid_storage());
        assert!(matches!(
            Texture::from_ref(&[], 0, 0, ColorFormat::RGBA8888),
            Err(RenderError::InvalidTexture)
        ));
    }

    #[test]
    fn owned_pixel_bytes_move_into_texture_without_copying() {
        let bytes = alloc::vec![7u8; 16];
        let address = bytes.as_ptr();
        let texture = Texture::from_vec(bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
        assert_eq!(texture.buf.as_slice().as_ptr(), address);
        assert_eq!(texture.stride, 8);
        assert!(texture.transient);
        assert!(Texture::from_vec(alloc::vec![0u8; 15], 2, 2, ColorFormat::RGBA8888).is_err());
    }

    #[test]
    fn argb8888_roundtrip() {
        let mut buf = [0u8; 4];
        let mut tex =
            Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
        let c = Color::rgba(100, 200, 50, 255);
        tex.pixels_mut().unwrap().set_pixel(0, 0, &c);
        assert_eq!(tex.read_pixels().unwrap().get_pixel(0, 0), c);
    }

    #[test]
    fn bgra8888_roundtrip() {
        let mut buf = [0u8; 4];
        let mut tex =
            Texture::new(&mut buf, 1, 1, ColorFormat::BGRA8888).expect("valid pixel storage");
        let c = Color::rgba(100, 200, 50, 255);
        tex.pixels_mut().unwrap().set_pixel(0, 0, &c);
        assert_eq!(tex.read_pixels().unwrap().get_pixel(0, 0), c);
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
        let mut tex =
            Texture::new(&mut buf, 1, 1, ColorFormat::RGB565).expect("valid pixel storage");
        let c = Color::rgb(255, 255, 255);
        tex.pixels_mut().unwrap().set_pixel(0, 0, &c);
        let got = tex.read_pixels().unwrap().get_pixel(0, 0);
        assert_eq!(got.r, c.r);
        assert_eq!(got.g, c.g);
        assert_eq!(got.b, c.b);
    }

    #[test]
    fn blend_50_percent() {
        let mut buf = [0u8; 4];
        let mut tex =
            Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
        tex.pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &Color::rgb(0, 0, 0));
        tex.pixels_mut().unwrap().blend_pixel(
            Fixed::ZERO,
            Fixed::ZERO,
            &Color::rgb(200, 100, 50),
            128,
        );
        let got = tex.read_pixels().unwrap().get_pixel(0, 0);
        assert!((got.r as i32 - 100).abs() <= 1);
        assert!((got.g as i32 - 50).abs() <= 1);
        assert!((got.b as i32 - 25).abs() <= 1);
    }

    #[test]
    fn blend_mode_keeps_straight_rgb_over_a_transparent_target() {
        let mut buf = [0u8; 4];
        let mut tex =
            Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
        tex.alpha_mode = AlphaMode::Blend;
        tex.pixels_mut()
            .unwrap()
            .blend_pixel_int(0, 0, &Color::rgba(0, 0, 255, 128), 128);
        assert_eq!(
            tex.read_pixels().unwrap().get_pixel(0, 0),
            Color::rgba(0, 0, 255, 128)
        );
        tex.pixels_mut()
            .unwrap()
            .blend_pixel_int(0, 0, &Color::rgba(255, 0, 0, 128), 128);
        let pixel = tex.read_pixels().unwrap().get_pixel(0, 0);
        assert!((pixel.r as i32 - 170).abs() <= 1);
        assert_eq!(pixel.g, 0);
        assert!((pixel.b as i32 - 85).abs() <= 1);
        assert!((pixel.a as i32 - 192).abs() <= 1);
    }

    #[test]
    fn rgba8_upload_conversion_ignores_stride_padding_in_each_format() {
        let mut rgba = Texture::from_ref(
            &[1, 2, 3, 4, 99, 99, 99, 99, 5, 6, 7, 8],
            1,
            2,
            ColorFormat::RGBA8888,
        )
        .expect("valid pixel storage");
        rgba.stride = 8;
        assert_eq!(
            rgba.read_pixels()
                .unwrap()
                .rgba8_upload_pixels()
                .unwrap()
                .as_ref(),
            &[1, 2, 3, 4, 5, 6, 7, 8]
        );

        let mut bgra = Texture::from_ref(
            &[3, 2, 1, 4, 99, 99, 99, 99, 7, 6, 5, 8],
            1,
            2,
            ColorFormat::BGRA8888,
        )
        .expect("valid pixel storage");
        bgra.stride = 8;
        assert_eq!(
            bgra.read_pixels().unwrap().rgba8_upload_pixels(),
            rgba.read_pixels().unwrap().rgba8_upload_pixels()
        );

        let mut rgb = Texture::from_ref(&[1, 2, 3, 99, 5, 6, 7], 1, 2, ColorFormat::RGB888)
            .expect("valid pixel storage");
        rgb.stride = 4;
        assert_eq!(
            rgb.read_pixels()
                .unwrap()
                .rgba8_upload_pixels()
                .unwrap()
                .as_ref(),
            &[1, 2, 3, 255, 5, 6, 7, 255]
        );

        rgb.stride = 2;
        assert!(matches!(
            rgb.read_pixels(),
            Err(RenderError::InvalidTexture)
        ));
    }

    #[test]
    fn rgba8_upload_borrows_packed_pixels_and_converts_padded_rows() {
        let pixels = [1, 2, 3, 4, 5, 6, 7, 8];
        let packed =
            Texture::from_ref(&pixels, 1, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        let allocations = crate::test_support::tracked_allocations(|| {
            let source = packed.read_pixels().unwrap();
            assert!(matches!(
                source.rgba8_upload_pixels(),
                Ok(Cow::Borrowed(bytes)) if bytes.as_ptr() == pixels.as_ptr()
            ));
        });
        assert_eq!(allocations, 0);

        let mut padded = Texture::from_ref(
            &[1, 2, 3, 4, 99, 99, 99, 99, 5, 6, 7, 8],
            1,
            2,
            ColorFormat::RGBA8888,
        )
        .expect("valid pixel storage");
        padded.stride = 8;
        assert!(matches!(
            padded.read_pixels().unwrap().rgba8_upload_pixels(),
            Ok(Cow::Owned(bytes)) if bytes == pixels
        ));
    }

    #[test]
    fn composite_modes_keep_source_color_on_transparent_targets() {
        use crate::render::command::CompositeMode;

        for mode in [
            CompositeMode::Add,
            CompositeMode::Screen,
            CompositeMode::Multiply,
            CompositeMode::Darken,
            CompositeMode::Lighten,
            CompositeMode::Difference,
        ] {
            let mut buf = [0u8; 4];
            let mut tex =
                Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
            tex.alpha_mode = AlphaMode::Blend;
            tex.pixels_mut().unwrap().composite_pixel_int(
                0,
                0,
                &Color::rgba(200, 80, 40, 128),
                128,
                mode,
            );
            assert_eq!(
                tex.read_pixels().unwrap().get_pixel(0, 0),
                Color::rgba(200, 80, 40, 128),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn composite_mode_uses_destination_alpha_in_blend_result() {
        use crate::render::command::CompositeMode;

        let mut buf = [0u8; 4];
        let mut tex =
            Texture::new(&mut buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
        tex.alpha_mode = AlphaMode::Blend;
        tex.pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &Color::rgba(255, 0, 0, 128));
        tex.pixels_mut().unwrap().composite_pixel_int(
            0,
            0,
            &Color::rgba(0, 0, 255, 128),
            128,
            CompositeMode::Multiply,
        );
        let pixel = tex.read_pixels().unwrap().get_pixel(0, 0);
        assert!((pixel.r as i32 - 85).abs() <= 1);
        assert_eq!(pixel.g, 0);
        assert!((pixel.b as i32 - 85).abs() <= 1);
        assert!((pixel.a as i32 - 192).abs() <= 1);
    }

    #[test]
    fn blend_rgb565() {
        let mut buf = [0u8; 2];
        let mut tex =
            Texture::new(&mut buf, 1, 1, ColorFormat::RGB565).expect("valid pixel storage");
        tex.pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &Color::rgb(0, 0, 0));
        tex.pixels_mut().unwrap().blend_pixel(
            Fixed::ZERO,
            Fixed::ZERO,
            &Color::rgb(255, 255, 255),
            255,
        );
        let got = tex.read_pixels().unwrap().get_pixel(0, 0);
        assert_eq!(got.r, 255);
        assert_eq!(got.g, 255);
        assert_eq!(got.b, 255);
    }

    #[test]
    fn blend_subpixel_spreads_to_neighbors() {
        // A point at (0.5, 0.5) should spread to all 4 pixels
        let mut buf = [0u8; 4 * 4]; // 2x2 RGBA8888
        let mut tex =
            Texture::new(&mut buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        tex.pixels_mut()
            .unwrap()
            .set_pixel(0, 0, &Color::rgb(0, 0, 0));
        tex.pixels_mut()
            .unwrap()
            .set_pixel(1, 0, &Color::rgb(0, 0, 0));
        tex.pixels_mut()
            .unwrap()
            .set_pixel(0, 1, &Color::rgb(0, 0, 0));
        tex.pixels_mut()
            .unwrap()
            .set_pixel(1, 1, &Color::rgb(0, 0, 0));

        tex.pixels_mut().unwrap().blend_pixel(
            Fixed::HALF,
            Fixed::HALF,
            &Color::rgb(255, 255, 255),
            255,
        );

        // Each pixel should get ~25% coverage
        let tl = tex.read_pixels().unwrap().get_pixel(0, 0);
        let tr = tex.read_pixels().unwrap().get_pixel(1, 0);
        let bl = tex.read_pixels().unwrap().get_pixel(0, 1);
        let br = tex.read_pixels().unwrap().get_pixel(1, 1);
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
            Err(MirxLoadError::Read(mirx::reader::ReadError::BadMagic))
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
            MirxLoadError::Read(mirx::reader::ReadError::BadMagic)
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
