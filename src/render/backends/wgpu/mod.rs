//! wgpu-backed Renderer + Canvas.

mod pipeline;
mod texture_pool;

use wgpu::util::DeviceExt;

use crate::render::PlaneRequirements;
use crate::render::PosedGlyphs;
use crate::render::canvas::{Canvas, Paint};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::factory::RendererFactory;
use crate::render::font::Font;
use crate::render::path::Path;
use crate::render::projective_fallback::{ProjectiveFallback, ProjectiveFallbackPlan};
use crate::render::raster::{FillRule, StrokeScratch, StrokeSpec};
use crate::render::renderer::{
    DrawRequest, FallbackRegion, RenderError, RenderFeature, RenderRoute, Renderer,
};
use crate::render::texture::Texture;
use crate::surface::wgpu_surface::WgpuTarget;
use crate::types::{Color, Fixed, PhysicalRect, Point, Rect, Transform, Transform3D, Viewport};

use self::pipeline::{
    BlitQuadVertex, BlitUniform, GlyphInstance, GlyphUniform, PathTintUniform, PipelineCache,
    PipelineKey, QuadSdfUniform, QuadSdfVertex, RectUniform, ShaderKind, ViewportUniform,
};
use self::texture_pool::{
    CachedScalarSurface, CachedTexture, ScalarSurfaceKey, ScalarSurfacePool, TextureKey,
    TexturePool, new_pool, new_scalar_surface_pool,
};
use crate::render::backends::tessellation::{FillTessellator, StrokeTessellator};

pub use self::pipeline::MSAA_SAMPLES;

fn paint_color(paint: &Paint) -> Color {
    match paint {
        Paint::Color(color) => (*color).into(),
        Paint::LinearGradient(gradient) => gradient
            .stops
            .first()
            .map(|stop| stop.color.into())
            .unwrap_or(Color::rgba(0, 0, 0, 0)),
        Paint::RadialGradient(gradient) => gradient
            .stops
            .first()
            .map(|stop| stop.color.into())
            .unwrap_or(Color::rgba(0, 0, 0, 0)),
    }
}

fn quad_projection_valid(width: Fixed, height: Fixed, quad: &[Point; 4]) -> bool {
    if width <= Fixed::ZERO || height <= Fixed::ZERO {
        return false;
    }
    let Some(forward) = Transform3D::from_quad(Rect::new(0, 0, width, height), quad) else {
        return false;
    };
    let width = width.to_f32();
    let height = height.to_f32();
    [(0.0, 0.0), (width, 0.0), (width, height), (0.0, height)]
        .into_iter()
        .all(|(x, y)| {
            let w = forward.m20.to_f32() * x + forward.m21.to_f32() * y + forward.m22.to_f32();
            w.is_finite() && w > 0.0
        })
}

fn glyph_uniform(color: Color, opacity: u8, spread: u16, transform: Transform3D) -> GlyphUniform {
    GlyphUniform {
        color: [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            color.a as f32 / 255.0 * opacity as f32 / 255.0,
        ],
        spread_pad: [f32::from(spread), 0.0, 0.0, 0.0],
        projective_row_0: [
            transform.m00.to_f32(),
            transform.m01.to_f32(),
            transform.m02.to_f32(),
            0.0,
        ],
        projective_row_1: [
            transform.m10.to_f32(),
            transform.m11.to_f32(),
            transform.m12.to_f32(),
            0.0,
        ],
        projective_row_2: [
            transform.m20.to_f32(),
            transform.m21.to_f32(),
            transform.m22.to_f32(),
            0.0,
        ],
    }
}

pub struct WgpuRendererFactory {
    target_edit_budget_bytes: Option<usize>,
    cache: Option<PipelineCache>,
    tessellator: FillTessellator,
    stroke_tessellator: StrokeTessellator,
    stroke_scratch: StrokeScratch,
    texture_pool: TexturePool,
    scalar_surface_pool: ScalarSurfacePool,
    scalar_samples: alloc::vec::Vec<u8>,
    glyph_instances: alloc::vec::Vec<GlyphInstance>,
    glyph_buffers: GlyphBufferArena,
    /// Samplers are immutable; one instance covers every frame.
    linear_sampler: Option<wgpu::Sampler>,
    nearest_sampler: Option<wgpu::Sampler>,
}

impl WgpuRendererFactory {
    pub fn new() -> Self {
        Self {
            target_edit_budget_bytes: None,
            cache: None,
            tessellator: FillTessellator::new(),
            stroke_tessellator: StrokeTessellator::new(),
            stroke_scratch: StrokeScratch::new(),
            texture_pool: new_pool(),
            scalar_surface_pool: new_scalar_surface_pool(),
            scalar_samples: alloc::vec::Vec::new(),
            glyph_instances: alloc::vec::Vec::new(),
            glyph_buffers: GlyphBufferArena::new(),
            linear_sampler: None,
            nearest_sampler: None,
        }
    }

    /// Limit checked target edits to this many physical RGBA bytes.
    pub fn with_target_edit_budget(mut self, bytes: usize) -> Self {
        self.target_edit_budget_bytes = Some(bytes);
        self
    }
}

impl Default for WgpuRendererFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: WgpuTarget> RendererFactory<B> for WgpuRendererFactory {
    type Renderer<'a>
        = WgpuRenderer<'a, B>
    where
        Self: 'a,
        B: 'a;

    fn make<'a>(&'a mut self, backend: &'a mut B, transform: &Viewport) -> WgpuRenderer<'a, B> {
        self.glyph_buffers.reset();
        if self.cache.is_none() || self.linear_sampler.is_none() || self.nearest_sampler.is_none() {
            let state = backend
                .state()
                .expect("WgpuSurface must be initialised before make()");
            if self.cache.is_none() {
                self.cache = Some(PipelineCache::new(&state.device));
            }
            if self.linear_sampler.is_none() {
                self.linear_sampler = Some(state.device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("mirui-linear-sampler"),
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Linear,
                    ..Default::default()
                }));
            }
            if self.nearest_sampler.is_none() {
                self.nearest_sampler =
                    Some(state.device.create_sampler(&wgpu::SamplerDescriptor {
                        label: Some("mirui-nearest-sampler"),
                        mag_filter: wgpu::FilterMode::Nearest,
                        min_filter: wgpu::FilterMode::Nearest,
                        ..Default::default()
                    }));
            }
        }
        WgpuRenderer {
            factory: self,
            surface: backend,
            viewport: *transform,
            frame: None,
            draw_failed: false,
        }
    }
}

pub struct WgpuRenderer<'a, B: WgpuTarget> {
    factory: &'a mut WgpuRendererFactory,
    surface: &'a mut B,
    viewport: Viewport,
    frame: Option<Frame>,
    draw_failed: bool,
}

struct Frame {
    surface_texture: wgpu::SurfaceTexture,
    swapchain_view: wgpu::TextureView,
    msaa_view: Option<wgpu::TextureView>,
    encoder: wgpu::CommandEncoder,
    /// One viewport uniform shared across the frame's draws.
    viewport_buf: wgpu::Buffer,
    /// Per-draw uniforms (rect / tint) packed back-to-back. `set_bind_group`
    /// dynamic offsets index into this single buffer.
    uniform_arena: wgpu::Buffer,
    /// Bytes already written; advances by `UNIFORM_ALIGN` per draw.
    uniform_cursor: u32,
    /// Cached fill / path bind groups — same `(viewport_buf, uniform_arena)`
    /// pair every draw, so one bind group covers the whole frame.
    fill_bind_group: Option<wgpu::BindGroup>,
    path_bind_group: Option<wgpu::BindGroup>,
    /// Encoded into one render pass on `flush`.
    ops: alloc::vec::Vec<DrawOp>,
    /// Tracks whether a render pass has already cleared/written the
    /// swapchain target so the next partial flush picks `LoadOp::Load`
    /// instead of clearing the already-written pixels.
    has_committed_pass: bool,
}

/// A full arena starts another ordered pass before its offsets are reused.
const UNIFORM_ARENA_SIZE: u64 = 1024 * 1024;

/// Most desktop / mobile GPUs require 256-byte alignment for dynamic
/// uniform offsets. `RectUniform` is 48 B, `PathTintUniform` is 16 B —
/// align up to the limit so any device accepts the offset.
const UNIFORM_ALIGN: u32 = 256;
const GLYPHS_PER_BATCH: usize = 2_048;
const GLYPH_BUFFER_CAPACITY: usize = GLYPHS_PER_BATCH * 2;

const fn uniform_arena_full(cursor: u32) -> bool {
    cursor as u64 + UNIFORM_ALIGN as u64 > UNIFORM_ARENA_SIZE
}

struct GlyphBufferArena {
    instance: Option<wgpu::Buffer>,
    glyph_cursor: usize,
}

struct GlyphBufferUpload {
    instance: wgpu::Buffer,
    instance_range: core::ops::Range<u64>,
}

impl GlyphBufferArena {
    const fn new() -> Self {
        Self {
            instance: None,
            glyph_cursor: 0,
        }
    }

    fn reset(&mut self) {
        self.glyph_cursor = 0;
    }

    fn can_fit(&self, glyph_count: usize) -> bool {
        self.glyph_cursor
            .checked_add(glyph_count)
            .is_some_and(|end| end <= GLYPH_BUFFER_CAPACITY)
    }

    fn range(glyph_start: usize, glyph_count: usize) -> Option<core::ops::Range<u64>> {
        let start = glyph_start.checked_mul(core::mem::size_of::<GlyphInstance>())? as u64;
        let end = start
            .checked_add(glyph_count.checked_mul(core::mem::size_of::<GlyphInstance>())? as u64)?;
        Some(start..end)
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[GlyphInstance],
    ) -> Option<GlyphBufferUpload> {
        let glyph_count = instances.len();
        if !self.can_fit(glyph_count) {
            return None;
        }
        if self.instance.is_none() {
            self.instance = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mirui-glyph-instance-arena"),
                size: (GLYPH_BUFFER_CAPACITY * core::mem::size_of::<GlyphInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        let instance_range = Self::range(self.glyph_cursor, glyph_count)?;
        let instance = self.instance.as_ref()?;
        queue.write_buffer(
            instance,
            instance_range.start,
            bytemuck::cast_slice(instances),
        );
        self.glyph_cursor += glyph_count;

        Some(GlyphBufferUpload {
            instance: instance.clone(),
            instance_range,
        })
    }
}

/// wgpu pipelines / buffers / bind groups are `Arc`-backed clones, so
/// owning them in the `Vec<DrawOp>` keeps them alive until
/// `queue.submit` consumes the encoder.
struct DrawOp {
    pipeline: wgpu::RenderPipeline,
    /// `None` when the op uses the frame-cached fill/path bind group;
    /// `Some` for textured blits and cached glyph runs.
    bind_group: BindGroupRef,
    vertex_buf: Option<wgpu::Buffer>,
    vertex_range: Option<core::ops::Range<u64>>,
    index_buf: Option<wgpu::Buffer>,
    index_range: Option<core::ops::Range<u64>>,
    index_format: wgpu::IndexFormat,
    /// `draw_indexed(0..count)` when `index_buf.is_some()`, else `draw(0..count)`.
    count: u32,
    instance_count: u32,
    /// Physical-pixel scissor `(x, y, w, h)`; clamped to swapchain extent.
    scissor: [u32; 4],
    dynamic_offset: Option<u32>,
}

enum BindGroupRef {
    /// Owned per-draw — blit / label use this because the texture view
    /// changes between draws.
    Owned(wgpu::BindGroup),
    /// Index into a frame-shared bind group. 0 = fill, 1 = path.
    Shared(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GlyphBatchKey {
    surface: ScalarSurfaceKey,
    shader: ShaderKind,
    spread: u16,
}

#[derive(Clone, Copy)]
struct GlyphBatch<'a> {
    key: GlyphBatchKey,
    surface: crate::render::font::GlyphSurface<'a>,
}

struct GlyphRunDraw<'a> {
    pos: &'a Point,
    glyphs: &'a [textflow::shaping::PositionedGlyph],
    font: &'a crate::render::font::Font,
    transform: &'a crate::types::Transform,
    clip: &'a Rect,
    color: &'a Color,
    opacity: u8,
    projective: Transform3D,
}

struct PosedGlyphRunDraw<'a> {
    pos: &'a Point,
    glyphs: PosedGlyphs<'a>,
    font: &'a Font,
    transform: &'a Transform,
    clip: &'a Rect,
    color: &'a Color,
    opacity: u8,
    projective: Transform3D,
}

impl<B: WgpuTarget> WgpuRenderer<'_, B> {
    fn draw_projective_validated(
        &mut self,
        cmd: &DrawCommand,
        clip: &Rect,
        projective: &Transform3D,
    ) -> Result<(), crate::render::ProjectiveDrawError> {
        use crate::render::ProjectiveDrawError;

        let transform = projective.compose(&Transform3D::from_affine(cmd.transform()));
        match cmd {
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => {
                let quad = transform
                    .apply_rect(*area)
                    .ok_or(ProjectiveDrawError::InvalidProjection)?;
                self.fill_quad_inner(area, &quad, *radius, clip, color, *opa);
            }
            DrawCommand::Border {
                area,
                width,
                radius,
                color,
                opa,
                ..
            } => {
                let quad = transform
                    .apply_rect(*area)
                    .ok_or(ProjectiveDrawError::InvalidProjection)?;
                self.stroke_quad_inner(area, &quad, *width, *radius, clip, color, *opa);
            }
            DrawCommand::Blit {
                pos,
                size,
                texture,
                opa,
                radius,
                composite,
                ..
            } => {
                let quad = transform
                    .apply_rect(Rect::new(pos.x, pos.y, size.x, size.y))
                    .ok_or(ProjectiveDrawError::InvalidProjection)?;
                self.blit_quad_inner(
                    texture,
                    &quad,
                    BlitMask {
                        size: *size,
                        radius: *radius,
                    },
                    clip,
                    *opa,
                    *composite,
                );
            }
            DrawCommand::GlyphRun {
                pos,
                transform,
                glyphs,
                font,
                color,
                opa,
            } => self.draw_glyph_run_inner(GlyphRunDraw {
                pos,
                glyphs,
                font,
                transform,
                clip,
                color,
                opacity: *opa,
                projective: *projective,
            }),
            DrawCommand::PosedGlyphRun {
                pos,
                transform,
                glyphs,
                font,
                color,
                opa,
            } => self.draw_posed_glyph_run_inner(PosedGlyphRunDraw {
                pos,
                glyphs: *glyphs,
                font,
                transform,
                clip,
                color,
                opacity: *opa,
                projective: *projective,
            }),
            _ => return Err(ProjectiveDrawError::Unsupported),
        }
        if self.draw_failed {
            Err(ProjectiveDrawError::BackendFailure)
        } else {
            Ok(())
        }
    }

    fn non_native_fallback_plan(
        &self,
        request: &DrawRequest<'_, '_>,
    ) -> Result<Option<ProjectiveFallbackPlan>, RenderError> {
        if !Self::needs_non_native_fallback(request)? {
            return Ok(None);
        }
        #[cfg(target_arch = "wasm32")]
        return Err(RenderError::Unsupported(RenderFeature::Readback));
        #[cfg(not(target_arch = "wasm32"))]
        {
            let capacity = self
                .factory
                .target_edit_budget_bytes
                .ok_or(RenderError::MissingWorkspace)?;
            ProjectiveFallback::<&mut [u8]>::measure(
                capacity,
                request.command,
                &request.clip,
                &request.projective,
                self.viewport,
                PlaneRequirements::CPU,
            )
            .map(Some)
            .map_err(RenderError::from)
        }
    }

    fn needs_non_native_fallback(request: &DrawRequest<'_, '_>) -> Result<bool, RenderError> {
        let needs_fallback = match request.command {
            DrawCommand::Blit { composite, .. } => matches!(
                composite,
                CompositeMode::Darken | CompositeMode::Lighten | CompositeMode::Difference
            ),
            DrawCommand::FillPath { paint, .. } | DrawCommand::StrokePath { paint, .. } => {
                if !matches!(paint, Paint::LinearGradient(_) | Paint::RadialGradient(_)) {
                    false
                } else if !request.projective.is_identity() {
                    return Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry));
                } else {
                    true
                }
            }
            _ => false,
        };
        Ok(needs_fallback)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn draw_composite_fallback(
        &mut self,
        plan: ProjectiveFallbackPlan,
        request: &DrawRequest<'_, '_>,
    ) -> Result<(), RenderError> {
        if plan.required_bytes() == 0 {
            return Ok(());
        }
        self.prepare_readback(&request.clip)?;
        let state = self.surface.state().ok_or(RenderError::BackendFailure)?;
        let frame = self.frame.as_ref().ok_or(RenderError::BackendFailure)?;
        let mut pixels = wgpu_readback_rgba8(
            &state.device,
            &state.queue,
            &frame.surface_texture.texture,
            state.config.format,
            u32::from(plan.x()),
            u32::from(plan.y()),
            u32::from(plan.width()),
            u32::from(plan.height()),
        )
        .ok_or(RenderError::BackendFailure)?;
        if pixels.len() != plan.required_bytes() {
            return Err(RenderError::BackendFailure);
        }
        ProjectiveFallback::borrowed(&mut pixels)
            .render(plan, request.command, &request.projective, self.viewport)
            .map_err(RenderError::from)?;
        let texture = Texture::from_ref(
            &pixels,
            plan.width(),
            plan.height(),
            crate::render::texture::ColorFormat::RGBA8888,
        );
        let view = self
            .blit_source_view(&texture)
            .ok_or(RenderError::BackendFailure)?;
        let scale = self.viewport.scale();
        self.blit_view_inner(
            view,
            texture.width,
            texture.height,
            &Rect::new(0, 0, texture.width, texture.height),
            Point::new(Fixed::from(plan.x()) / scale, Fixed::from(plan.y()) / scale),
            Point::new(
                Fixed::from(plan.width()) / scale,
                Fixed::from(plan.height()) / scale,
            ),
            255,
            Fixed::ZERO,
            CompositeMode::SourceOver,
            ShaderKind::BlitReplace,
            [
                u32::from(plan.x()),
                u32::from(plan.y()),
                u32::from(plan.width()),
                u32::from(plan.height()),
            ],
        );
        if self.draw_failed {
            Err(RenderError::BackendFailure)
        } else {
            Ok(())
        }
    }

    fn classify_request(request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        use crate::types::TransformClass;

        request.validate_texture()?;

        match request.command {
            DrawCommand::PushClip { .. } | DrawCommand::PopClip => {
                return Err(RenderError::Unsupported(RenderFeature::PathClip));
            }
            DrawCommand::StrokePath { paint, .. } => {
                if !matches!(paint, Paint::Color(_)) {
                    return Err(RenderError::Unsupported(RenderFeature::GradientPaint));
                }
            }
            DrawCommand::FillPath { paint, .. } => {
                if !matches!(paint, Paint::Color(_)) {
                    return Err(RenderError::Unsupported(RenderFeature::GradientPaint));
                }
            }
            DrawCommand::Blit { composite, .. } => {
                if matches!(
                    composite,
                    CompositeMode::Darken | CompositeMode::Lighten | CompositeMode::Difference
                ) {
                    return Err(RenderError::Unsupported(RenderFeature::Composite(
                        *composite,
                    )));
                }
            }
            _ => {}
        }

        let explicit_quad = match request.command {
            DrawCommand::Fill {
                area,
                quad: Some(quad),
                ..
            }
            | DrawCommand::Border {
                area,
                quad: Some(quad),
                ..
            } => Some((area.w, area.h, quad)),
            DrawCommand::Blit {
                texture,
                quad: Some(quad),
                ..
            } => Some((
                Fixed::from_int(i32::from(texture.width)),
                Fixed::from_int(i32::from(texture.height)),
                quad,
            )),
            _ => None,
        };
        if explicit_quad
            .is_some_and(|(width, height, quad)| !quad_projection_valid(width, height, quad))
        {
            return Err(RenderError::InvalidGeometry);
        }

        if !request.projective.is_identity() {
            return match request.command {
                DrawCommand::Fill { .. }
                | DrawCommand::Border { .. }
                | DrawCommand::Blit { .. }
                | DrawCommand::GlyphRun { .. }
                | DrawCommand::PosedGlyphRun { .. } => Ok(RenderRoute::Native),
                _ => Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry)),
            };
        }

        let supports_affine = match request.command {
            DrawCommand::Fill { quad, .. }
            | DrawCommand::Border { quad, .. }
            | DrawCommand::Blit { quad, .. } => quad.is_some(),
            DrawCommand::GlyphRun { .. }
            | DrawCommand::PosedGlyphRun { .. }
            | DrawCommand::FillPath { .. }
            | DrawCommand::StrokePath { .. } => true,
            _ => false,
        };
        if !supports_affine
            && !matches!(
                request.command.transform().classify(),
                TransformClass::Identity | TransformClass::Translate
            )
        {
            return Err(RenderError::Unsupported(RenderFeature::AffineGeometry));
        }
        Ok(RenderRoute::Native)
    }

    /// `false` on swapchain Outdated/Lost/Validation; caller drops the
    /// frame, next tick retries (Resized triggers a reconfigure).
    fn begin_frame(&mut self) -> bool {
        if self.frame.is_some() {
            return true;
        }
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing in begin_frame");
        let surface_texture = match state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => return false,
        };
        let swapchain_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let msaa_view = state
            .msaa
            .as_ref()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default()));
        let encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mirui-wgpu-encoder"),
            });
        let scale = self.viewport.scale().to_f32().max(1.0);
        // Logical pixels — NDC scales onto the physical swapchain.
        let viewport_uniform = ViewportUniform {
            size: [
                state.config.width as f32 / scale,
                state.config.height as f32 / scale,
            ],
            _pad: [0.0, 0.0],
        };
        let viewport_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-frame-viewport"),
                contents: bytemuck::bytes_of(&viewport_uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let uniform_arena = state.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mirui-frame-uniform-arena"),
            size: UNIFORM_ARENA_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.frame = Some(Frame {
            surface_texture,
            swapchain_view,
            msaa_view,
            encoder,
            viewport_buf,
            uniform_arena,
            uniform_cursor: 0,
            fill_bind_group: None,
            path_bind_group: None,
            ops: alloc::vec::Vec::new(),
            has_committed_pass: false,
        });
        true
    }

    /// `present=false` submits the recorded ops but keeps the swapchain
    /// texture so a follow-up `copy_texture_to_buffer` can read this
    /// frame's pixels mid-walk.
    fn flush_ops_to_swapchain(&mut self, present: bool) -> Result<(), RenderError> {
        let Some(frame) = self.frame.as_mut() else {
            return Ok(());
        };
        if frame.ops.is_empty() && !present && frame.has_committed_pass {
            return Ok(());
        }
        let state = self.surface.state().ok_or(RenderError::BackendFailure)?;
        let load = if frame.has_committed_pass {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(if frame.ops.is_empty() {
                wgpu::Color::BLACK
            } else {
                wgpu::Color::TRANSPARENT
            })
        };
        {
            let (view, resolve_target) = match frame.msaa_view.as_ref() {
                Some(msaa_view) => (msaa_view, Some(&frame.swapchain_view)),
                None => (&frame.swapchain_view, None),
            };
            let mut pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mirui-frame-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            for op in &frame.ops {
                pass.set_scissor_rect(op.scissor[0], op.scissor[1], op.scissor[2], op.scissor[3]);
                pass.set_pipeline(&op.pipeline);
                let bg: &wgpu::BindGroup = match &op.bind_group {
                    BindGroupRef::Owned(bg) => bg,
                    BindGroupRef::Shared(0) => frame
                        .fill_bind_group
                        .as_ref()
                        .expect("fill bind group must be created before any Shared(0) op"),
                    BindGroupRef::Shared(1) => frame
                        .path_bind_group
                        .as_ref()
                        .expect("path bind group must be created before any Shared(1) op"),
                    BindGroupRef::Shared(other) => panic!("unknown shared bind group id {}", other),
                };
                match op.dynamic_offset {
                    Some(o) => pass.set_bind_group(0, bg, &[o]),
                    None => pass.set_bind_group(0, bg, &[]),
                }
                if let Some(vb) = &op.vertex_buf {
                    match &op.vertex_range {
                        Some(range) => pass.set_vertex_buffer(0, vb.slice(range.clone())),
                        None => pass.set_vertex_buffer(0, vb.slice(..)),
                    }
                }
                if let Some(ib) = &op.index_buf {
                    match &op.index_range {
                        Some(range) => {
                            pass.set_index_buffer(ib.slice(range.clone()), op.index_format)
                        }
                        None => pass.set_index_buffer(ib.slice(..), op.index_format),
                    }
                    pass.draw_indexed(0..op.count, 0, 0..op.instance_count);
                } else {
                    pass.draw(0..op.count, 0..op.instance_count);
                }
            }
        }
        let new_encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mirui-wgpu-encoder"),
            });
        let old_encoder = core::mem::replace(&mut frame.encoder, new_encoder);
        state.queue.submit(Some(old_encoder.finish()));
        frame.ops.clear();
        frame.has_committed_pass = true;

        if present {
            let frame = self.frame.take().expect("frame present taken");
            frame.surface_texture.present();
        }
        Ok(())
    }

    /// Append a uniform to the frame's arena. Earlier draws are submitted
    /// before their offsets are reused by a later pass.
    fn push_uniform<T: bytemuck::Pod>(&mut self, value: &T) -> Option<u32> {
        if uniform_arena_full(self.frame.as_ref()?.uniform_cursor) {
            if !self.frame.as_ref()?.ops.is_empty() && self.flush_ops_to_swapchain(false).is_err() {
                self.draw_failed = true;
                return None;
            }
            self.frame.as_mut()?.uniform_cursor = 0;
        }
        let frame = self.frame.as_mut()?;
        let offset = frame.uniform_cursor;
        let state = self.surface.state()?;
        state.queue.write_buffer(
            &frame.uniform_arena,
            offset as u64,
            bytemuck::bytes_of(value),
        );
        frame.uniform_cursor += UNIFORM_ALIGN;
        Some(offset)
    }

    fn fill_rect_inner(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        if !self.begin_frame() {
            return;
        }
        let scissor = self.scissor_from_clip(clip);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }

        let rect_uniform = RectUniform {
            pos: [area.x.to_f32(), area.y.to_f32()],
            size: [area.w.to_f32(), area.h.to_f32()],
            color: [
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0 * opa as f32 / 255.0,
            ],
            radius_pad: [radius.to_f32(), 0.0, 0.0, 0.0],
        };
        let Some(offset) = self.push_uniform(&rect_uniform) else {
            self.draw_failed = true;
            return;
        };

        let frame = self.frame.as_mut().expect("frame just initialised");
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing in fill_rect");
        let cache = self
            .factory
            .cache
            .as_mut()
            .expect("PipelineCache must be initialised before fill_rect");

        if frame.fill_bind_group.is_none() {
            frame.fill_bind_group =
                Some(state.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("mirui-fill-bind-group"),
                    layout: &cache.fill_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: frame.viewport_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &frame.uniform_arena,
                                offset: 0,
                                size: core::num::NonZeroU64::new(
                                    core::mem::size_of::<RectUniform>() as u64,
                                ),
                            }),
                        },
                    ],
                }));
        }

        let pipeline = cache.get_or_build(
            &state.device,
            PipelineKey {
                shader: ShaderKind::Fill,
                format: state.config.format,
                composite: CompositeMode::SourceOver,
            },
        );

        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Shared(0),
            vertex_buf: None,
            vertex_range: None,
            index_buf: None,
            index_range: None,
            index_format: wgpu::IndexFormat::Uint32,
            count: 4,
            instance_count: 1,
            scissor,
            dynamic_offset: Some(offset),
        });
    }

    fn blit_source_view(&mut self, src: &Texture) -> Option<wgpu::TextureView> {
        let view = self.blit_source_view_checked(src);
        self.draw_failed |= view.is_none();
        view
    }

    fn blit_source_view_checked(&mut self, src: &Texture) -> Option<wgpu::TextureView> {
        let state = self.surface.state()?;
        let Some(key) = TextureKey::cacheable(src) else {
            let rgba = src.rgba8_pixels()?;
            let texture = upload_blit_source(&state.device, &state.queue, src, &rgba);
            return Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        };

        let handle = self
            .factory
            .texture_pool
            .entry(key)
            .or_try_insert_with::<_, ()>(|| {
                let rgba = src.rgba8_pixels().ok_or(())?;
                Ok(CachedTexture(upload_blit_source(
                    &state.device,
                    &state.queue,
                    src,
                    &rgba,
                )))
            })
            .ok()?;
        Some(
            handle
                .0
                .create_view(&wgpu::TextureViewDescriptor::default()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_inner(
        &mut self,
        src: &Texture,
        src_rect: &Rect,
        dst_pos: Point,
        dst_size: Point,
        clip: &Rect,
        opa: u8,
        radius: Fixed,
        composite: CompositeMode,
    ) {
        if opa == 0 {
            return;
        }
        if !self.begin_frame() {
            return;
        }
        let scissor = self.scissor_from_clip(clip);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }

        let Some(tex_view) = self.blit_source_view(src) else {
            return;
        };

        self.blit_view_inner(
            tex_view,
            src.width,
            src.height,
            src_rect,
            dst_pos,
            dst_size,
            opa,
            radius,
            composite,
            ShaderKind::Blit,
            scissor,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_view_inner(
        &mut self,
        tex_view: wgpu::TextureView,
        texture_width: u16,
        texture_height: u16,
        src_rect: &Rect,
        dst_pos: Point,
        dst_size: Point,
        opa: u8,
        radius: Fixed,
        composite: CompositeMode,
        shader: ShaderKind,
        scissor: [u32; 4],
    ) {
        let frame = self.frame.as_mut().expect("frame just initialised");
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing in blit");
        let cache = self
            .factory
            .cache
            .as_mut()
            .expect("PipelineCache must be initialised before blit");
        let sampler = self
            .factory
            .nearest_sampler
            .as_ref()
            .expect("nearest sampler must be initialised before blit");

        let tw = texture_width as f32;
        let th = texture_height as f32;
        let blit_uniform = BlitUniform {
            dst_pos: [dst_pos.x.to_f32(), dst_pos.y.to_f32()],
            dst_size: [dst_size.x.to_f32(), dst_size.y.to_f32()],
            uv: [
                src_rect.x.to_f32() / tw,
                src_rect.y.to_f32() / th,
                (src_rect.x.to_f32() + src_rect.w.to_f32()) / tw,
                (src_rect.y.to_f32() + src_rect.h.to_f32()) / th,
            ],
            alpha: [opa as f32 / 255.0, radius.to_f32().max(0.0), 0.0, 0.0],
        };

        let blit_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-blit-uniform"),
                contents: bytemuck::bytes_of(&blit_uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-blit-bind-group"),
            layout: &cache.blit_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame.viewport_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: blit_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        let pipeline = cache.get_or_build(
            &state.device,
            PipelineKey {
                shader,
                format: state.config.format,
                composite,
            },
        );

        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Owned(bind_group),
            vertex_buf: None,
            vertex_range: None,
            index_buf: None,
            index_range: None,
            index_format: wgpu::IndexFormat::Uint32,
            count: 4,
            instance_count: 1,
            scissor,
            dynamic_offset: None,
        });
    }
}

impl<B: WgpuTarget> WgpuRenderer<'_, B> {
    fn scissor_from_clip(&self, clip: &Rect) -> [u32; 4] {
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing for scissor");
        let Some(rect) =
            self.viewport
                .physical_rect_in(*clip, state.config.width, state.config.height)
        else {
            return [0, 0, 0, 0];
        };
        [
            u32::from(rect.x()),
            u32::from(rect.y()),
            u32::from(rect.width()),
            u32::from(rect.height()),
        ]
    }
}

fn upload_blit_source(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src: &Texture,
    rgba: &[u8],
) -> wgpu::Texture {
    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("mirui-blit-source"),
            size: wgpu::Extent3d {
                width: src.width as u32,
                height: src.height as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    )
}

struct BlitMask {
    size: Point,
    radius: Fixed,
}

fn offset_rect(r: &Rect, tx: Fixed, ty: Fixed) -> Rect {
    Rect {
        x: r.x + tx,
        y: r.y + ty,
        w: r.w,
        h: r.h,
    }
}

fn offset_point(p: &Point, tx: Fixed, ty: Fixed) -> Point {
    Point {
        x: p.x + tx,
        y: p.y + ty,
    }
}

impl<B: WgpuTarget> WgpuRenderer<'_, B> {
    fn fill_path_transformed_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        cmd_tf: &crate::types::Transform,
        paint: &Paint,
        opa: u8,
        fill_rule: FillRule,
    ) {
        let color = paint_color(paint);
        self.factory.tessellator.fill(path, Some(cmd_tf), fill_rule);
        let mesh = self.factory.tessellator.take_mesh();
        self.draw_path_mesh(&mesh.vertices, &mesh.indices, clip, &color, opa);
        self.factory.tessellator.restore_mesh(mesh);
    }
}

impl<B: WgpuTarget> WgpuRenderer<'_, B> {
    fn physical_clip_rect(&self, src: &Rect) -> Option<PhysicalRect> {
        let state = self.surface.state()?;
        self.viewport
            .physical_rect_in(*src, state.config.width, state.config.height)
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn swapchain_is_bgra(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    )
}

/// wgpu's `bytes_per_row` for `copy_texture_to_buffer` must be aligned
/// to 256 bytes (`COPY_BYTES_PER_ROW_ALIGNMENT`). The caller's rect has
/// the natural `w * 4` row size; the staging buffer needs the padded
/// stride, and the per-row copy strips the padding back out.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
fn wgpu_readback_rgba8(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    format: wgpu::TextureFormat,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Option<alloc::vec::Vec<u8>> {
    if w == 0 || h == 0 {
        return None;
    }
    const ALIGN: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let unpadded = w * 4;
    let padded = unpadded.div_ceil(ALIGN) * ALIGN;
    let buf_size = (padded as u64) * (h as u64);

    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mirui-wgpu-readback"),
        size: buf_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mirui-wgpu-readback-encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x, y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = sender.send(r);
    });
    let _ = device.poll(wgpu::PollType::Wait {
        timeout: None,
        submission_index: None,
    });
    receiver.recv().ok()?.ok()?;

    let data = slice.get_mapped_range();
    let mut out = alloc::vec::Vec::with_capacity((unpadded as usize) * (h as usize));
    let swap_rb = swapchain_is_bgra(format);
    for row in 0..h {
        let start = (row * padded) as usize;
        let end = start + unpadded as usize;
        if swap_rb {
            let src = &data[start..end];
            for px in src.chunks_exact(4) {
                out.extend_from_slice(&straight_rgba8([px[2], px[1], px[0], px[3]]));
            }
        } else {
            for px in data[start..end].chunks_exact(4) {
                out.extend_from_slice(&straight_rgba8([px[0], px[1], px[2], px[3]]));
            }
        }
    }
    drop(data);
    staging.unmap();
    Some(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn straight_rgba8(pixel: [u8; 4]) -> [u8; 4] {
    let alpha = u32::from(pixel[3]);
    if alpha == 0 {
        return [0; 4];
    }
    let channel = |value: u8| ((u32::from(value) * 255 + alpha / 2) / alpha).min(255) as u8;
    [
        channel(pixel[0]),
        channel(pixel[1]),
        channel(pixel[2]),
        pixel[3],
    ]
}

impl<B: WgpuTarget> WgpuRenderer<'_, B> {
    fn fill_path_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        color: &Color,
        opa: u8,
        fill_rule: FillRule,
    ) {
        self.factory.tessellator.fill(path, None, fill_rule);
        let mesh = self.factory.tessellator.take_mesh();
        self.draw_path_mesh(&mesh.vertices, &mesh.indices, clip, color, opa);
        self.factory.tessellator.restore_mesh(mesh);
    }

    fn stroke_path_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        self.stroke_path_styled_inner(
            path,
            clip,
            None,
            StrokeSpec {
                width,
                cap: crate::render::raster::LineCap::Butt,
                join: crate::render::raster::LineJoin::Miter,
                miter_limit: Fixed::from_int(4),
                dash: &[],
                dash_scale: Fixed::ONE,
            },
            color,
            opa,
        );
    }

    fn stroke_path_styled_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        transform: Option<&Transform>,
        spec: StrokeSpec<'_>,
        color: &Color,
        opa: u8,
    ) {
        if spec.width <= Fixed::ZERO || opa == 0 {
            return;
        }
        if spec.dash.is_empty() {
            self.factory
                .stroke_tessellator
                .stroke(path, transform, spec);
            let mesh = self.factory.stroke_tessellator.take_mesh();
            self.draw_path_mesh(&mesh.vertices, &mesh.indices, clip, color, opa);
            self.factory.stroke_tessellator.restore_mesh(mesh);
            return;
        }
        let outline = self.factory.stroke_scratch.outline(path, transform, spec);
        self.factory
            .tessellator
            .fill(outline, None, FillRule::NonZero);
        let mesh = self.factory.tessellator.take_mesh();
        self.draw_path_mesh(&mesh.vertices, &mesh.indices, clip, color, opa);
        self.factory.tessellator.restore_mesh(mesh);
    }

    fn draw_path_mesh(
        &mut self,
        verts: &[lyon::math::Point],
        indices: &[u32],
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        if verts.is_empty() || indices.is_empty() {
            return;
        }
        if !self.begin_frame() {
            return;
        }
        let scissor = self.scissor_from_clip(clip);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }

        let tint_uniform = PathTintUniform {
            color: [
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0 * opa as f32 / 255.0,
            ],
        };
        let Some(offset) = self.push_uniform(&tint_uniform) else {
            self.draw_failed = true;
            return;
        };

        let frame = self.frame.as_mut().expect("frame just initialised");
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing in path");
        let cache = self
            .factory
            .cache
            .as_mut()
            .expect("PipelineCache must be initialised before path");

        // lyon::math::Point is repr(C) over (f32, f32) — same wire layout
        // as `[f32; 2]`, so cast straight into the vertex buffer without
        // an intermediate copy.
        let vertex_bytes: &[u8] = bytemuck::cast_slice(unsafe {
            core::slice::from_raw_parts(verts.as_ptr() as *const [f32; 2], verts.len())
        });

        let vertex_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-path-vertices"),
                contents: vertex_bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-path-indices"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        if frame.path_bind_group.is_none() {
            frame.path_bind_group =
                Some(state.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("mirui-path-bind-group"),
                    layout: &cache.path_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: frame.viewport_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &frame.uniform_arena,
                                offset: 0,
                                size: core::num::NonZeroU64::new(
                                    core::mem::size_of::<PathTintUniform>() as u64,
                                ),
                            }),
                        },
                    ],
                }));
        }

        let pipeline = cache.get_or_build(
            &state.device,
            PipelineKey {
                shader: ShaderKind::Path,
                format: state.config.format,
                composite: CompositeMode::SourceOver,
            },
        );

        let count = indices.len() as u32;
        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Shared(1),
            vertex_buf: Some(vertex_buf),
            vertex_range: None,
            index_buf: Some(index_buf),
            index_range: None,
            index_format: wgpu::IndexFormat::Uint32,
            count,
            instance_count: 1,
            scissor,
            dynamic_offset: Some(offset),
        });
    }

    fn fill_quad_inner(
        &mut self,
        area: &Rect,
        q: &[Point; 4],
        radius: Fixed,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        self.quad_sdf_inner(area, q, radius, Fixed::ZERO, clip, color, opa);
    }

    #[allow(clippy::too_many_arguments)]
    fn stroke_quad_inner(
        &mut self,
        area: &Rect,
        q: &[Point; 4],
        width: Fixed,
        radius: Fixed,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        self.quad_sdf_inner(area, q, radius, width, clip, color, opa);
    }

    /// `stroke_width = 0` paints a fill; `> 0` paints a ring of that
    /// width centred on the outline.
    #[allow(clippy::too_many_arguments)]
    fn quad_sdf_inner(
        &mut self,
        area: &Rect,
        q: &[Point; 4],
        radius: Fixed,
        stroke_width: Fixed,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        if !self.begin_frame() {
            return;
        }
        let scissor = self.scissor_from_clip(clip);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }

        // The homography's bottom row gives each corner's projective `w`.
        let widget_w = area.w.to_f32();
        let widget_h = area.h.to_f32();
        if widget_w <= 0.0 || widget_h <= 0.0 {
            return;
        }
        let src_rect = Rect::new(0, 0, area.w, area.h);
        let Some(forward) = crate::types::Transform3D::from_quad(src_rect, q) else {
            return;
        };
        let m20 = forward.m20.to_f32();
        let m21 = forward.m21.to_f32();
        let m22 = forward.m22.to_f32();
        let corners = [
            (0.0_f32, 0.0_f32),
            (widget_w, 0.0),
            (widget_w, widget_h),
            (0.0, widget_h),
        ];
        let mut verts = [QuadSdfVertex::default(); 4];
        for (i, (lx, ly)) in corners.iter().enumerate() {
            let w = m20 * lx + m21 * ly + m22;
            if w <= 0.0 {
                return;
            }
            let inv_w = 1.0 / w;
            verts[i] = QuadSdfVertex {
                pos: [q[i].x.to_f32(), q[i].y.to_f32()],
                local_uvw: [lx * inv_w, ly * inv_w, inv_w],
            };
        }
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

        let uniform = QuadSdfUniform {
            size: [widget_w, widget_h],
            _pad0: [0.0, 0.0],
            color: [
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0 * opa as f32 / 255.0,
            ],
            radius_stroke: [radius.to_f32(), stroke_width.to_f32(), 0.0, 0.0],
        };
        let Some(offset) = self.push_uniform(&uniform) else {
            self.draw_failed = true;
            return;
        };

        let frame = self.frame.as_mut().expect("frame just initialised");
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing in quad_sdf");
        let cache = self
            .factory
            .cache
            .as_mut()
            .expect("PipelineCache must be initialised before quad_sdf");

        if frame.fill_bind_group.is_none() {
            frame.fill_bind_group =
                Some(state.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("mirui-fill-bind-group"),
                    layout: &cache.fill_bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: frame.viewport_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &frame.uniform_arena,
                                offset: 0,
                                size: core::num::NonZeroU64::new(
                                    core::mem::size_of::<RectUniform>() as u64,
                                ),
                            }),
                        },
                    ],
                }));
        }

        let vertex_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-quad-sdf-vertex"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-quad-sdf-index"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        let pipeline = cache.get_or_build(
            &state.device,
            PipelineKey {
                shader: ShaderKind::QuadSdf,
                format: state.config.format,
                composite: CompositeMode::SourceOver,
            },
        );

        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Shared(0),
            vertex_buf: Some(vertex_buf),
            vertex_range: None,
            index_buf: Some(index_buf),
            index_range: None,
            index_format: wgpu::IndexFormat::Uint16,
            count: 6,
            instance_count: 1,
            scissor,
            dynamic_offset: Some(offset),
        });
    }

    /// Perspective-correct quad blit via `Transform3D::from_quad`.
    fn blit_quad_inner(
        &mut self,
        src: &Texture,
        q: &[Point; 4],
        mask: BlitMask,
        clip: &Rect,
        opa: u8,
        composite: CompositeMode,
    ) {
        if opa == 0 {
            return;
        }
        if !self.begin_frame() {
            return;
        }
        let scissor = self.scissor_from_clip(clip);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }

        let src_rect = Rect::new(0, 0, src.width, src.height);
        let Some(forward) = crate::types::Transform3D::from_quad(src_rect, q) else {
            return;
        };

        let corners = [(0.0_f32, 0.0_f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let m20 = forward.m20.to_f32();
        let m21 = forward.m21.to_f32();
        let m22 = forward.m22.to_f32();
        // `from_quad` takes a pixel-space src rect, so unit corners need
        // source-size scaling before plugging into the bottom row.
        let sw = src.width as f32;
        let sh = src.height as f32;

        let params = [
            opa as f32 / 255.0,
            mask.radius.to_f32().max(0.0),
            mask.size.x.to_f32(),
            mask.size.y.to_f32(),
        ];
        let mut verts = [BlitQuadVertex::default(); 4];
        for (i, (u, v)) in corners.iter().enumerate() {
            let pixel_u = u * sw;
            let pixel_v = v * sh;
            let w = m20 * pixel_u + m21 * pixel_v + m22;
            if w <= 0.0 {
                // Corner behind the near plane — drop rather than emit NaN UVs.
                return;
            }
            // Encode `(u/w, v/w, 1/w)`: linear interpolation of these
            // across screen-space gives perspective-correct attributes
            // when the fragment shader divides `xy / z`. Encoding
            // `(u·w, v·w, w)` would also satisfy `xy / z = (u, v)` at
            // each vertex but only stays correct under uniform `w`.
            let inv_w = 1.0 / w;
            verts[i] = BlitQuadVertex {
                pos: [q[i].x.to_f32(), q[i].y.to_f32()],
                uvw: [u * inv_w, v * inv_w, inv_w],
                params,
            };
        }
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

        let Some(tex_view) = self.blit_source_view(src) else {
            return;
        };

        let frame = self.frame.as_mut().expect("frame just initialised");
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing in blit_quad");
        let cache = self
            .factory
            .cache
            .as_mut()
            .expect("PipelineCache must be initialised before blit_quad");
        let sampler = self
            .factory
            .nearest_sampler
            .as_ref()
            .expect("nearest sampler must be initialised before blit_quad");
        let vertex_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-blit-quad-vertex"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-blit-quad-index"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        let bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-blit-quad-bind-group"),
            layout: &cache.blit_quad_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame.viewport_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        let pipeline = cache.get_or_build(
            &state.device,
            PipelineKey {
                shader: ShaderKind::BlitQuad,
                format: state.config.format,
                composite,
            },
        );

        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Owned(bind_group),
            vertex_buf: Some(vertex_buf),
            vertex_range: None,
            index_buf: Some(index_buf),
            index_range: None,
            index_format: wgpu::IndexFormat::Uint16,
            count: 6,
            instance_count: 1,
            scissor,
            dynamic_offset: None,
        });
    }

    fn draw_glyph_run_inner(&mut self, draw: GlyphRunDraw<'_>) {
        let GlyphRunDraw {
            pos,
            glyphs,
            font,
            transform,
            clip,
            color,
            opacity,
            projective,
        } = draw;
        let Some(first) = glyphs.first().copied() else {
            return;
        };
        let requested_size = font.size.max(1);
        let raster_scale = self.viewport.scale() * transform.raster_scale();
        let output_ppem = crate::render::font::output_ppem(requested_size, raster_scale);
        let metrics = font.metrics(requested_size);
        self.draw_glyph_quads(
            glyphs,
            font,
            output_ppem,
            clip,
            color,
            opacity,
            projective,
            |_, positioned, raster, region| {
                let dx = positioned
                    .origin
                    .x
                    .checked_sub(first.origin.x)?
                    .checked_add(positioned.offset.x)?;
                let dy = positioned
                    .origin
                    .y
                    .checked_sub(first.origin.y)?
                    .checked_add(positioned.offset.y)?;
                let scale = Fixed::from_int(i32::from(requested_size))
                    / Fixed::from_int(i32::from(raster.representation.design_ppem().max(1)));
                Some((
                    Rect {
                        x: pos.x + crate::types::fixed::from_textflow(dx) + raster.offset_x,
                        y: pos.y + metrics.ascender + crate::types::fixed::from_textflow(dy)
                            - raster.offset_y,
                        w: Fixed::from_int(region.width() as i32) * scale,
                        h: Fixed::from_int(region.height() as i32) * scale,
                    },
                    *transform,
                ))
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_glyph_quads(
        &mut self,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &Font,
        output_ppem: u16,
        clip: &Rect,
        color: &Color,
        opacity: u8,
        projective: Transform3D,
        mut geometry: impl FnMut(
            usize,
            &textflow::shaping::PositionedGlyph,
            crate::render::font::RasterGlyph<'_>,
            mirx::image::Region,
        ) -> Option<(Rect, Transform)>,
    ) {
        if !self.begin_frame() {
            return;
        }
        let scissor = self.scissor_from_clip(clip);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }
        let requested_size = font.size.max(1);
        let mut active: Option<GlyphBatch<'_>> = None;
        self.factory.glyph_instances.clear();

        for (index, positioned) in glyphs.iter().enumerate() {
            let Some(raster) =
                font.raster_for_output(positioned.glyph_id(), requested_size, output_ppem)
            else {
                continue;
            };
            let Some(region) = raster
                .region
                .filter(|region| region.width() > 0 && region.height() > 0)
            else {
                continue;
            };
            let (shader, spread, bits) = match raster.representation.kind() {
                mirx::font::FontRepresentationKind::Coverage { bits } => {
                    (ShaderKind::GlyphCoverage, 0, bits)
                }
                mirx::font::FontRepresentationKind::SignedDistance { bits, spread } => {
                    (ShaderKind::GlyphSdf, spread, bits)
                }
                _ => continue,
            };
            if crate::render::font::scalar::alpha_bits(raster.surface.sample_layout()) != Some(bits)
            {
                continue;
            }
            let Some((rect, transform)) = geometry(index, positioned, raster, region) else {
                continue;
            };
            let key = GlyphBatchKey {
                surface: ScalarSurfaceKey::new(font.face_id(), font.revision(), raster.surface),
                shader,
                spread,
            };
            if active.map(|batch| batch.key) != Some(key)
                || self.factory.glyph_instances.len() >= GLYPHS_PER_BATCH
            {
                if let Some(batch) = active.take() {
                    self.submit_glyph_batch(batch, scissor, color, opacity, projective);
                }
                active = Some(GlyphBatch {
                    key,
                    surface: raster.surface,
                });
            }

            append_glyph_instance(
                &mut self.factory.glyph_instances,
                rect,
                region,
                raster.surface,
                &transform,
            );
        }
        if let Some(batch) = active {
            self.submit_glyph_batch(batch, scissor, color, opacity, projective);
        }
    }

    fn draw_posed_glyph_run_inner(&mut self, draw: PosedGlyphRunDraw<'_>) {
        let PosedGlyphRunDraw {
            pos,
            glyphs,
            font,
            transform,
            clip,
            color,
            opacity,
            projective,
        } = draw;
        let visible = glyphs
            .ink_bounds(font, *pos, *transform, self.viewport.scale())
            .and_then(|bounds| {
                if projective.is_identity() {
                    bounds.intersect(clip)
                } else {
                    projective
                        .apply_rect(bounds)
                        .map(|quad| Rect::bounding_quad(&quad))
                        .and_then(|bounds| bounds.intersect(clip))
                }
            });
        if visible.is_none() {
            return;
        }
        let requested_size = font.size.max(1);
        let output_ppem = crate::render::font::output_ppem(
            requested_size,
            self.viewport.scale() * transform.raster_scale(),
        );
        self.draw_glyph_quads(
            glyphs.glyphs(),
            font,
            output_ppem,
            clip,
            color,
            opacity,
            projective,
            |index, _, raster, region| {
                let frame = glyphs.frames().get(index)?;
                let quad = raster.posed_quad(*pos, *frame, requested_size, *transform)?;
                debug_assert_eq!(raster.region, Some(region));
                Some((quad.rect, quad.transform))
            },
        );
    }

    fn submit_glyph_batch(
        &mut self,
        batch: GlyphBatch<'_>,
        scissor: [u32; 4],
        color: &Color,
        opa: u8,
        projective: Transform3D,
    ) {
        if self.factory.glyph_instances.is_empty() {
            return;
        }
        let glyph_count = self.factory.glyph_instances.len();
        if !self.factory.glyph_buffers.can_fit(glyph_count) {
            if self.flush_ops_to_swapchain(false).is_err() {
                self.factory.glyph_instances.clear();
                self.draw_failed = true;
                return;
            }
            self.factory.glyph_buffers.reset();
        }
        if !self.factory.glyph_buffers.can_fit(glyph_count) {
            self.factory.glyph_instances.clear();
            self.draw_failed = true;
            return;
        }
        let uniform = glyph_uniform(*color, opa, batch.key.spread, projective);
        let Some(offset) = self.push_uniform(&uniform) else {
            self.factory.glyph_instances.clear();
            self.draw_failed = true;
            return;
        };
        let texture_view = {
            let state = self
                .surface
                .state()
                .expect("WgpuSurface state missing in glyph batch");
            let samples = &mut self.factory.scalar_samples;
            let handle = match self
                .factory
                .scalar_surface_pool
                .entry(batch.key.surface)
                .or_try_insert_with::<_, ()>(|| {
                    crate::render::font::scalar::unpack_surface(batch.surface, samples).ok_or(())?;
                    let texture = state.device.create_texture_with_data(
                        &state.queue,
                        &wgpu::TextureDescriptor {
                            label: Some("mirui-glyph-surface"),
                            size: wgpu::Extent3d {
                                width: batch.surface.width(),
                                height: batch.surface.height(),
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::R8Unorm,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        },
                        wgpu::util::TextureDataOrder::LayerMajor,
                        samples,
                    );
                    Ok(CachedScalarSurface(texture))
                }) {
                Ok(handle) => handle,
                Err(_) => {
                    self.factory.glyph_instances.clear();
                    self.draw_failed = true;
                    return;
                }
            };
            handle
                .0
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing in glyph batch");
        let Some(upload) = self.factory.glyph_buffers.upload(
            &state.device,
            &state.queue,
            &self.factory.glyph_instances,
        ) else {
            self.factory.glyph_instances.clear();
            self.draw_failed = true;
            return;
        };
        let frame = self
            .frame
            .as_mut()
            .expect("frame initialised for glyph batch");
        let cache = self
            .factory
            .cache
            .as_mut()
            .expect("PipelineCache must be initialised before glyph batch");
        let sampler = self
            .factory
            .linear_sampler
            .as_ref()
            .expect("linear sampler must be initialised before glyph batch");
        let bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-glyph-bind-group"),
            layout: &cache.glyph_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame.viewport_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &frame.uniform_arena,
                        offset: 0,
                        size: core::num::NonZeroU64::new(
                            core::mem::size_of::<GlyphUniform>() as u64
                        ),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        let pipeline = cache.get_or_build(
            &state.device,
            PipelineKey {
                shader: batch.key.shader,
                format: state.config.format,
                composite: CompositeMode::SourceOver,
            },
        );
        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Owned(bind_group),
            vertex_buf: Some(upload.instance),
            vertex_range: Some(upload.instance_range),
            index_buf: None,
            index_range: None,
            index_format: wgpu::IndexFormat::Uint16,
            count: 6,
            instance_count: glyph_count as u32,
            scissor,
            dynamic_offset: Some(offset),
        });
        self.factory.glyph_instances.clear();
    }
}

fn append_glyph_instance(
    instances: &mut alloc::vec::Vec<GlyphInstance>,
    rect: Rect,
    region: mirx::image::Region,
    surface: crate::render::font::GlyphSurface<'_>,
    transform: &crate::types::Transform,
) {
    let points = transform.apply_rect(rect);
    let width = surface.width() as f32;
    let height = surface.height() as f32;
    let x0 = region.x() as f32 / width;
    let y0 = region.y() as f32 / height;
    let x1 = region.x().saturating_add(region.width()) as f32 / width;
    let y1 = region.y().saturating_add(region.height()) as f32 / height;
    let origin = [points[0].x.to_f32(), points[0].y.to_f32()];
    instances.push(GlyphInstance {
        origin,
        axis_x: [
            points[1].x.to_f32() - origin[0],
            points[1].y.to_f32() - origin[1],
        ],
        axis_y: [
            points[3].x.to_f32() - origin[0],
            points[3].y.to_f32() - origin[1],
        ],
        uv_bounds: [x0, y0, x1, y1],
    });
}

#[cfg(test)]
mod route_tests {
    use super::*;
    use crate::render::texture::ColorFormat;
    use mirx::scene::{GradientUnits, LinearGradient, SpreadMode};

    #[test]
    fn readback_restores_straight_alpha_and_target_edits_replace_pixels() {
        assert_eq!(straight_rgba8([64, 32, 0, 128]), [128, 64, 0, 128]);
        assert_eq!(straight_rgba8([255, 20, 10, 0]), [0, 0, 0, 0]);
        assert_eq!(straight_rgba8([12, 34, 56, 255]), [12, 34, 56, 255]);
        assert_eq!(
            crate::render::backends::wgpu::pipeline::blend_state_for(
                ShaderKind::BlitReplace,
                CompositeMode::SourceOver,
            ),
            wgpu::BlendState::REPLACE
        );
    }

    #[test]
    fn texture_upload_accepts_both_rgb565_orders_and_row_padding() {
        let mut native =
            Texture::from_ref(&[0x00, 0xf8, 0, 0, 0xe0, 0x07], 1, 2, ColorFormat::RGB565);
        native.stride = 4;
        assert_eq!(
            native.rgba8_pixels(),
            Some(alloc::vec![255, 0, 0, 255, 0, 255, 0, 255])
        );

        let swapped = Texture::from_ref(&[0xf8, 0x00], 1, 1, ColorFormat::RGB565Swapped);
        assert_eq!(swapped.rgba8_pixels(), Some(alloc::vec![255, 0, 0, 255]));

        let short = Texture::from_ref(&[0xf8], 1, 1, ColorFormat::RGB565Swapped);
        assert!(short.rgba8_pixels().is_none());
        let blit = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::new(1, 1),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &short,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&blit, Rect::new(0, 0, 1, 1)),
            ),
            Err(RenderError::InvalidTexture)
        );
    }

    #[test]
    fn projected_quads_are_checked_before_gpu_submission() {
        let area = Rect::new(8, 8, 40, 24);
        let projection =
            Transform3D::rotate_y_perspective(Fixed::from_int(65), Fixed::from_int(320));
        let quad = projection.apply_rect(area).expect("visible projected quad");
        assert!(quad_projection_valid(area.w, area.h, &quad));

        let collapsed = [Point::new(8, 8); 4];
        assert!(!quad_projection_valid(area.w, area.h, &collapsed));
        let fill = DrawCommand::Fill {
            area,
            transform: Transform::IDENTITY,
            quad: Some(collapsed),
            color: Color::rgb(20, 30, 40),
            radius: Fixed::ZERO,
            opa: 255,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&fill, area),
            ),
            Err(RenderError::InvalidGeometry)
        );
    }

    #[test]
    fn solid_stroke_with_full_style_has_a_native_route() {
        let path = Path::rect(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(16),
            Fixed::from_int(12),
        );
        let paint = Paint::Color(Color::rgb(20, 30, 40).into());
        let dash = [Fixed::from_int(3), Fixed::from_int(2)];
        let stroke = DrawCommand::StrokePath {
            path: &path,
            transform: Transform::rotate_deg(Fixed::from_int(20)),
            paint: &paint,
            width: Fixed::from_int(2),
            opa: 255,
            line_cap: crate::render::raster::LineCap::Round,
            line_join: crate::render::raster::LineJoin::Bevel,
            miter_limit: Fixed::from_int(4),
            dash: &dash,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&stroke, Rect::new(0, 0, 32, 32)),
            ),
            Ok(RenderRoute::Native)
        );
    }

    #[test]
    fn shared_stroke_mesh_keeps_round_caps_inside_local_bounds() {
        let mut path = Path::new();
        path.move_to(Point::new(0, 0)).line_to(Point::new(40, 0));
        let mut scratch = StrokeScratch::new();
        let outline = scratch.outline(
            &path,
            None,
            StrokeSpec {
                width: Fixed::from_int(8),
                cap: crate::render::raster::LineCap::Round,
                join: crate::render::raster::LineJoin::Round,
                miter_limit: Fixed::from_int(4),
                dash: &[],
                dash_scale: Fixed::ONE,
            },
        );
        let mut tessellator = FillTessellator::new();
        let (vertices, indices) = tessellator.fill(outline, None, FillRule::NonZero);
        assert!(!indices.is_empty());
        assert!(
            vertices.iter().all(|point| {
                (-8.0..=48.0).contains(&point.x) && (-8.0..=8.0).contains(&point.y)
            })
        );
    }

    #[test]
    fn shared_stroke_mesh_keeps_the_center_of_a_closed_path_empty() {
        let path = Path::rect(
            Fixed::from_int(10),
            Fixed::from_int(10),
            Fixed::from_int(30),
            Fixed::from_int(20),
        );
        let mut scratch = StrokeScratch::new();
        let outline = scratch.outline(
            &path,
            None,
            StrokeSpec {
                width: Fixed::from_int(4),
                cap: crate::render::raster::LineCap::Butt,
                join: crate::render::raster::LineJoin::Miter,
                miter_limit: Fixed::from_int(4),
                dash: &[],
                dash_scale: Fixed::ONE,
            },
        );
        let mut tessellator = FillTessellator::new();
        let (vertices, indices) = tessellator.fill(outline, None, FillRule::NonZero);
        let covers = |x: f32, y: f32| {
            indices.chunks_exact(3).any(|triangle| {
                let points = [
                    vertices[triangle[0] as usize],
                    vertices[triangle[1] as usize],
                    vertices[triangle[2] as usize],
                ];
                let edge = |a: lyon::math::Point, b: lyon::math::Point| {
                    (x - a.x) * (b.y - a.y) - (y - a.y) * (b.x - a.x)
                };
                let signs = [
                    edge(points[0], points[1]),
                    edge(points[1], points[2]),
                    edge(points[2], points[0]),
                ];
                signs.iter().all(|value| *value >= 0.0) || signs.iter().all(|value| *value <= 0.0)
            })
        };
        assert!(covers(10.0, 20.0));
        assert!(!covers(25.0, 20.0));
    }

    #[test]
    fn rejects_commands_that_current_gpu_dispatch_ignores_or_reduces() {
        let clip = Rect::new(0, 0, 32, 32);
        let path = Path::new();
        let paint = Paint::Color(Color::rgb(20, 30, 40).into());
        let push = DrawCommand::PushClip {
            path: &path,
            transform: Transform::IDENTITY,
            fill_rule: FillRule::EvenOdd,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&push, clip),
            ),
            Err(RenderError::Unsupported(RenderFeature::PathClip))
        );

        let fill = DrawCommand::FillPath {
            path: &path,
            transform: Transform::IDENTITY,
            paint: &paint,
            opa: 255,
            fill_rule: FillRule::NonZero,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&fill, clip),
            ),
            Ok(RenderRoute::Native)
        );

        let gradient = Paint::LinearGradient(LinearGradient {
            start: Point::ZERO.into(),
            end: Point::new(10, 0).into(),
            stops: alloc::borrow::Cow::Borrowed(&[]),
            spread: SpreadMode::Pad,
            units: GradientUnits::UserSpaceOnUse,
            transform: Transform::IDENTITY.into(),
        });
        let gradient_fill = DrawCommand::FillPath {
            path: &path,
            transform: Transform::IDENTITY,
            paint: &gradient,
            opa: 255,
            fill_rule: FillRule::EvenOdd,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&gradient_fill, clip),
            ),
            Err(RenderError::Unsupported(RenderFeature::GradientPaint))
        );

        let texture = Texture::owned(2, 2, ColorFormat::RGBA8888);
        let blit = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::new(2, 2),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &texture,
            opa: 255,
            radius: Fixed::ONE,
            composite: CompositeMode::SourceOver,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&blit, clip),
            ),
            Ok(RenderRoute::Native)
        );
        let projected = DrawRequest::new(&blit, clip).with_projective(
            Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(120)),
        );
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(&projected),
            Ok(RenderRoute::Native)
        );

        let rounded_quad = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::new(2, 2),
            transform: Transform::IDENTITY,
            quad: Some([
                Point::new(0, 0),
                Point::new(2, 0),
                Point::new(2, 2),
                Point::new(0, 2),
            ]),
            texture: &texture,
            opa: 255,
            radius: Fixed::ONE,
            composite: CompositeMode::SourceOver,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&rounded_quad, clip),
            ),
            Ok(RenderRoute::Native)
        );

        let difference = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::new(2, 2),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::Difference,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&difference, clip),
            ),
            Err(RenderError::Unsupported(RenderFeature::Composite(
                CompositeMode::Difference
            )))
        );
    }

    #[test]
    fn accepts_native_fill_and_rejects_unrouted_affine_line() {
        let clip = Rect::new(0, 0, 32, 32);
        let fill = DrawCommand::Fill {
            area: clip,
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(20, 30, 40),
            radius: Fixed::ZERO,
            opa: 255,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&fill, clip),
            ),
            Ok(RenderRoute::Native)
        );

        let line = DrawCommand::Line {
            p1: Point::ZERO,
            p2: Point::new(10, 10),
            transform: Transform::rotate_deg(Fixed::from_int(20)),
            color: Color::rgb(20, 30, 40),
            width: Fixed::ONE,
            opa: 255,
        };
        assert_eq!(
            WgpuRenderer::<crate::surface::wgpu_surface::WgpuSurface>::classify_request(
                &DrawRequest::new(&line, clip),
            ),
            Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
        );
    }
}

#[cfg(test)]
mod blit_tests {
    use super::*;

    #[test]
    fn blit_paths_mask_corners_and_premultiply_alpha() {
        const SIZE: u32 = 32;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
        else {
            return;
        };
        let Ok((device, queue)) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("mirui-rounded-blit-test-device"),
                required_features: wgpu::Features::empty(),
                required_limits:
                    wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
                ..Default::default()
            }))
        else {
            return;
        };
        let source = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("mirui-rounded-blit-test-source"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[255, 0, 0, 128],
        );
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirui-rounded-blit-test-target"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let msaa = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirui-rounded-blit-test-msaa"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let viewport = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-rounded-blit-test-viewport"),
            contents: bytemuck::bytes_of(&ViewportUniform {
                size: [SIZE as f32, SIZE as f32],
                _pad: [0.0; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-rounded-blit-test-uniform"),
            contents: bytemuck::bytes_of(&BlitUniform {
                dst_pos: [0.0, 0.0],
                dst_size: [SIZE as f32, SIZE as f32],
                uv: [0.0, 0.0, 1.0, 1.0],
                alpha: [1.0, 10.0, 0.0, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let mut cache = PipelineCache::new(&device);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-rounded-blit-test-bind-group"),
            layout: &cache.blit_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &source.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline = cache.get_or_build(
            &device,
            PipelineKey {
                shader: ShaderKind::Blit,
                format: wgpu::TextureFormat::Rgba8Unorm,
                composite: CompositeMode::SourceOver,
            },
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mirui-rounded-blit-test-encoder"),
        });
        {
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mirui-rounded-blit-test-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: Some(&view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        let Some(bytes) = wgpu_readback_rgba8(
            &device,
            &queue,
            &target,
            wgpu::TextureFormat::Rgba8Unorm,
            0,
            0,
            SIZE,
            SIZE,
        ) else {
            panic!("rounded blit target could not be read");
        };
        let alpha = |x: usize, y: usize| bytes[(y * SIZE as usize + x) * 4 + 3];
        assert_eq!(alpha(0, 0), 0);
        assert!(alpha(16, 0) > 120);
        assert!(alpha(0, 16) > 120);
        assert!(alpha(16, 16) > 120);

        let replace_pipeline = cache.get_or_build(
            &device,
            PipelineKey {
                shader: ShaderKind::BlitReplace,
                format: wgpu::TextureFormat::Rgba8Unorm,
                composite: CompositeMode::SourceOver,
            },
        );
        let mut replace_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mirui-target-replace-test-encoder"),
        });
        {
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = replace_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mirui-target-replace-test-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: Some(&view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&replace_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..4, 0..1);
        }
        queue.submit(Some(replace_encoder.finish()));
        let replaced = wgpu_readback_rgba8(
            &device,
            &queue,
            &target,
            wgpu::TextureFormat::Rgba8Unorm,
            16,
            16,
            1,
            1,
        )
        .expect("replacement pixel should be readable");
        assert!((125..=131).contains(&replaced[3]));

        let vertices = [
            BlitQuadVertex {
                pos: [0.0, 0.0],
                uvw: [0.0, 0.0, 1.0],
                params: [1.0, 10.0, 32.0, 32.0],
            },
            BlitQuadVertex {
                pos: [32.0, 0.0],
                uvw: [0.5, 0.0, 0.5],
                params: [1.0, 10.0, 32.0, 32.0],
            },
            BlitQuadVertex {
                pos: [32.0, 32.0],
                uvw: [0.5, 0.5, 0.5],
                params: [1.0, 10.0, 32.0, 32.0],
            },
            BlitQuadVertex {
                pos: [0.0, 32.0],
                uvw: [0.0, 1.0, 1.0],
                params: [1.0, 10.0, 32.0, 32.0],
            },
        ];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-translucent-quad-test-vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = [0u16, 1, 2, 0, 2, 3];
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-translucent-quad-test-indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let quad_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-translucent-quad-test-bind-group"),
            layout: &cache.blit_quad_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &source.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let quad_pipeline = cache.get_or_build(
            &device,
            PipelineKey {
                shader: ShaderKind::BlitQuad,
                format: wgpu::TextureFormat::Rgba8Unorm,
                composite: CompositeMode::SourceOver,
            },
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mirui-translucent-quad-test-encoder"),
        });
        {
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mirui-translucent-quad-test-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: Some(&view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&quad_pipeline);
            pass.set_bind_group(0, &quad_bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        let bytes = wgpu_readback_rgba8(
            &device,
            &queue,
            &target,
            wgpu::TextureFormat::Rgba8Unorm,
            0,
            0,
            SIZE,
            SIZE,
        )
        .expect("translucent quad target could not be read");
        let center = &bytes[(16 * SIZE as usize + 16) * 4..][..4];
        assert_eq!(bytes[3], 0);
        assert!((i16::from(center[0]) - 255).abs() <= 1);
        assert!((i16::from(center[3]) - 128).abs() <= 1);
    }
}

#[cfg(test)]
mod glyph_tests {
    use super::*;
    use crate::render::font::{FontSurfaceId, GlyphSurface};

    static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn uniform_arena_rollover_starts_after_the_last_slot() {
        assert!(!uniform_arena_full(
            UNIFORM_ARENA_SIZE as u32 - UNIFORM_ALIGN
        ));
        assert!(uniform_arena_full(UNIFORM_ARENA_SIZE as u32));
    }

    fn render_scalar_field(
        shader: ShaderKind,
        spread: u16,
        projective: Transform3D,
    ) -> Option<alloc::vec::Vec<u8>> {
        const WIDTH: u32 = 32;
        const HEIGHT: u32 = 16;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("mirui-glyph-parity-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
            ..Default::default()
        }))
        .ok()?;
        let atlas = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("mirui-glyph-parity-atlas"),
                size: wgpu::Extent3d {
                    width: 4,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[0, 64, 192, 255],
        );
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirui-glyph-parity-target"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let msaa = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirui-glyph-parity-msaa"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let viewport = ViewportUniform {
            size: [WIDTH as f32, HEIGHT as f32],
            _pad: [0.0; 2],
        };
        let glyph = glyph_uniform(Color::rgb(255, 255, 255), u8::MAX, spread, projective);
        let viewport_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-glyph-parity-viewport"),
            contents: bytemuck::bytes_of(&viewport),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut glyph_uniforms = [0u8; UNIFORM_ALIGN as usize];
        glyph_uniforms[..core::mem::size_of::<GlyphUniform>()]
            .copy_from_slice(bytemuck::bytes_of(&glyph));
        let glyph_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-glyph-parity-uniform"),
            contents: &glyph_uniforms,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let instances = [GlyphInstance {
            origin: [0.0, 0.0],
            axis_x: [WIDTH as f32, 0.0],
            axis_y: [0.0, HEIGHT as f32],
            uv_bounds: [0.0, 0.0, 1.0, 1.0],
        }];
        let instance_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-glyph-parity-instance"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut pipelines = PipelineCache::new(&device);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-glyph-parity-bind-group"),
            layout: &pipelines.glyph_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &glyph_buf,
                        offset: 0,
                        size: core::num::NonZeroU64::new(
                            core::mem::size_of::<GlyphUniform>() as u64
                        ),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        &atlas.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&device.create_sampler(
                        &wgpu::SamplerDescriptor {
                            mag_filter: wgpu::FilterMode::Linear,
                            min_filter: wgpu::FilterMode::Linear,
                            ..Default::default()
                        },
                    )),
                },
            ],
        });
        let pipeline = pipelines.get_or_build(
            &device,
            PipelineKey {
                shader,
                format: wgpu::TextureFormat::Rgba8Unorm,
                composite: CompositeMode::SourceOver,
            },
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mirui-glyph-parity-encoder"),
        });
        {
            let target_view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mirui-glyph-parity-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: Some(&target_view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[0]);
            pass.set_vertex_buffer(0, instance_buf.slice(..));
            pass.draw(0..6, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        wgpu_readback_rgba8(
            &device,
            &queue,
            &target,
            wgpu::TextureFormat::Rgba8Unorm,
            0,
            0,
            WIDTH,
            HEIGHT,
        )
    }

    fn alpha_at(bytes: &[u8], x: usize, y: usize) -> u8 {
        bytes[(y * 32 + x) * 4 + 3]
    }

    #[test]
    fn projected_quad_and_reused_uniforms_stay_bounded() {
        let _gpu = GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        const SIZE: u32 = 64;
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
        else {
            return;
        };
        let Ok((device, queue)) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("mirui-quad-parity-device"),
                required_features: wgpu::Features::empty(),
                required_limits:
                    wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
                ..Default::default()
            }))
        else {
            return;
        };
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirui-quad-parity-target"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let msaa = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirui-quad-parity-msaa"),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let source = Rect::new(0, 0, 32, 32);
        let quad = [
            Point::new(18, 16),
            Point::new(48, 20),
            Point::new(44, 44),
            Point::new(20, 48),
        ];
        let forward = Transform3D::from_quad(source, &quad).unwrap();
        let mut vertices = [QuadSdfVertex::default(); 4];
        for (index, (x, y)) in [(0.0, 0.0), (32.0, 0.0), (32.0, 32.0), (0.0, 32.0)]
            .into_iter()
            .enumerate()
        {
            let inverse_w =
                1.0 / (forward.m20.to_f32() * x + forward.m21.to_f32() * y + forward.m22.to_f32());
            vertices[index] = QuadSdfVertex {
                pos: [quad[index].x.to_f32(), quad[index].y.to_f32()],
                local_uvw: [x * inverse_w, y * inverse_w, inverse_w],
            };
        }
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-quad-parity-vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = [0u16, 1, 2, 0, 2, 3];
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-quad-parity-indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let viewport_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-quad-parity-viewport"),
            contents: bytemuck::bytes_of(&ViewportUniform {
                size: [SIZE as f32, SIZE as f32],
                _pad: [0.0; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mirui-quad-parity-uniform"),
            contents: bytemuck::bytes_of(&QuadSdfUniform {
                size: [32.0, 32.0],
                _pad0: [0.0; 2],
                color: [1.0, 0.0, 0.0, 1.0],
                radius_stroke: [0.0; 4],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let mut pipelines = PipelineCache::new(&device);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-quad-parity-bind-group"),
            layout: &pipelines.fill_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });
        let pipeline = pipelines.get_or_build(
            &device,
            PipelineKey {
                shader: ShaderKind::QuadSdf,
                format: wgpu::TextureFormat::Rgba8Unorm,
                composite: CompositeMode::SourceOver,
            },
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mirui-quad-parity-encoder"),
        });
        {
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mirui-quad-parity-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: Some(&view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[0]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.set_scissor_rect(0, 0, SIZE / 2, SIZE);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        queue.write_buffer(
            &uniform_buffer,
            0,
            bytemuck::bytes_of(&QuadSdfUniform {
                size: [32.0, 32.0],
                _pad0: [0.0; 2],
                color: [0.0, 0.0, 1.0, 1.0],
                radius_stroke: [0.0; 4],
            }),
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mirui-quad-reused-uniform-encoder"),
        });
        {
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mirui-quad-reused-uniform-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    resolve_target: Some(&view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[0]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.set_scissor_rect(SIZE / 2, 0, SIZE / 2, SIZE);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        let pixels = wgpu_readback_rgba8(
            &device,
            &queue,
            &target,
            wgpu::TextureFormat::Rgba8Unorm,
            0,
            0,
            SIZE,
            SIZE,
        )
        .unwrap();
        let alpha = |x: usize, y: usize| pixels[(y * SIZE as usize + x) * 4 + 3];
        let color =
            |x: usize, y: usize, channel: usize| pixels[(y * SIZE as usize + x) * 4 + channel];
        assert!(color(24, 32, 0) > 220);
        assert!(color(24, 32, 2) < 32);
        assert!(color(40, 32, 2) > 220);
        assert!(color(40, 32, 0) < 32);
        assert!(alpha(32, 32) > 220);
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(63, 0), 0);
        assert_eq!(alpha(0, 63), 0);
        assert_eq!(alpha(63, 63), 0);
        for y in 0..SIZE as usize {
            for x in 0..SIZE as usize {
                if alpha(x, y) != 0 {
                    assert!(
                        (16..=50).contains(&x) && (14..=50).contains(&y),
                        "({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn glyph_buffer_ranges_are_disjoint_and_capacity_is_bounded() {
        let first = GlyphBufferArena::range(0, 1).unwrap();
        let next = GlyphBufferArena::range(1, 2).unwrap();
        assert_eq!(first.end, next.start);
        assert_eq!(
            next.end - next.start,
            2 * core::mem::size_of::<GlyphInstance>() as u64
        );
        assert_eq!(core::mem::size_of::<GlyphInstance>(), 40);

        let mut arena = GlyphBufferArena::new();
        arena.glyph_cursor = GLYPH_BUFFER_CAPACITY - 1;
        assert!(arena.can_fit(1));
        assert!(!arena.can_fit(2));
        arena.reset();
        assert!(arena.can_fit(GLYPH_BUFFER_CAPACITY));
    }

    #[test]
    fn packed_surface_upload_removes_stride_and_expands_alpha() {
        let bytes = [0b1010_0000, 0, 0b0100_0000, 0];
        let surface = GlyphSurface::new(
            &bytes,
            3,
            2,
            2,
            mirx::image::SampleLayout::A1,
            mirx::types::ByteAlignment::ONE,
            FontSurfaceId::new(7),
        )
        .unwrap();
        let mut output = alloc::vec::Vec::new();

        crate::render::font::scalar::unpack_surface(surface, &mut output).unwrap();

        assert_eq!(output, [255, 0, 255, 0, 255, 0]);
    }

    #[test]
    fn glyph_instance_keeps_region_bounds_under_transform() {
        let surface = GlyphSurface::new(
            &[0; 64],
            8,
            8,
            8,
            mirx::image::SampleLayout::A8,
            mirx::types::ByteAlignment::ONE,
            FontSurfaceId::new(8),
        )
        .unwrap();
        let region = mirx::image::Region::new(2, 1, 4, 3).unwrap();
        let mut instances = alloc::vec::Vec::new();

        append_glyph_instance(
            &mut instances,
            Rect::new(1, 2, 4, 3),
            region,
            surface,
            &crate::types::Transform::translate(Fixed::from_int(5), Fixed::from_int(7)),
        );

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].origin, [6.0, 9.0]);
        assert_eq!(instances[0].axis_x, [4.0, 0.0]);
        assert_eq!(instances[0].axis_y, [0.0, 3.0]);
        assert_eq!(instances[0].uv_bounds, [0.25, 0.125, 0.75, 0.5]);
    }

    #[test]
    fn glyph_instance_rotates_about_its_pose_origin() {
        let surface = GlyphSurface::new(
            &[0; 64],
            8,
            8,
            8,
            mirx::image::SampleLayout::A8,
            mirx::types::ByteAlignment::ONE,
            FontSurfaceId::new(9),
        )
        .unwrap();
        let region = mirx::image::Region::new(0, 0, 8, 8).unwrap();
        let mut instances = alloc::vec::Vec::new();
        let pose = Transform {
            m00: Fixed::ZERO,
            m01: -Fixed::ONE,
            tx: Fixed::from_int(12),
            m10: Fixed::ONE,
            m11: Fixed::ZERO,
            ty: Fixed::from_int(1),
        };

        append_glyph_instance(
            &mut instances,
            Rect::new(0, -7, 8, 8),
            region,
            surface,
            &pose,
        );

        assert_eq!(instances[0].origin, [19.0, 1.0]);
        assert_eq!(instances[0].axis_x, [0.0, 8.0]);
        assert_eq!(instances[0].axis_y, [-8.0, 0.0]);
        assert_eq!(
            [
                instances[0].origin[0] + instances[0].axis_x[0] + instances[0].axis_y[0],
                instances[0].origin[1] + instances[0].axis_x[1] + instances[0].axis_y[1],
            ],
            [11.0, 9.0]
        );
    }

    #[test]
    fn coverage_and_sdf_shaders_preserve_scalar_edges() {
        let _gpu = GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for (shader, spread) in [(ShaderKind::GlyphCoverage, 0), (ShaderKind::GlyphSdf, 4)] {
            let Some(pixels) = render_scalar_field(shader, spread, Transform3D::IDENTITY) else {
                return;
            };
            let samples = [
                alpha_at(&pixels, 2, 8),
                alpha_at(&pixels, 10, 8),
                alpha_at(&pixels, 20, 8),
                alpha_at(&pixels, 29, 8),
            ];
            assert!(samples.windows(2).all(|pair| pair[0] <= pair[1]));
            assert!(samples[0] < 32, "{shader:?} left edge {samples:?}");
            assert!(samples[3] > 223, "{shader:?} right edge {samples:?}");
        }
    }

    #[test]
    fn glyph_shader_applies_projective_transform() {
        let _gpu = GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = Rect::new(0, 0, 32, 16);
        let target = [
            Point::new(0, 0),
            Point::new(24, 2),
            Point::new(20, 14),
            Point::new(0, 16),
        ];
        let projective = Transform3D::from_quad(source, &target).unwrap();
        let Some(pixels) = render_scalar_field(ShaderKind::GlyphCoverage, 0, projective) else {
            return;
        };

        assert!(alpha_at(&pixels, 18, 8) > 160);
        assert_eq!(alpha_at(&pixels, 28, 8), 0);
    }
}

impl<B: WgpuTarget> WgpuRenderer<'_, B> {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate_projection()?;
        request.validate_texture()?;
        if let Some(plan) = self.non_native_fallback_plan(request)? {
            return Ok(RenderRoute::ExactFallback(plan.region()));
        }
        if let DrawCommand::ApplyBlur { alpha, region } = request.command {
            if !request.projective.is_identity() {
                return Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry));
            }
            if *alpha <= Fixed::ZERO || *alpha >= Fixed::ONE {
                return Ok(RenderRoute::Native);
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = region;
                return Err(RenderError::Unsupported(RenderFeature::Readback));
            }
            #[cfg(not(target_arch = "wasm32"))]
            return RenderRoute::target_readback(
                self.physical_clip_rect(region),
                self.factory.target_edit_budget_bytes,
                PlaneRequirements::CPU,
            );
        }
        let route = Self::classify_request(request)?;
        if !request.projective.is_identity() {
            self.preflight_projective(request.command, &request.clip, &request.projective)
                .map_err(RenderError::from)?;
        }
        Ok(route)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        let route = self.route(request)?;
        self.submit_with_route(request, route)
    }

    fn submit_with_route(
        &mut self,
        request: &DrawRequest<'_, '_>,
        route: RenderRoute,
    ) -> Result<(), RenderError> {
        self.draw_failed = false;
        request.validate_projection()?;
        request.validate_texture()?;
        if Self::needs_non_native_fallback(request)? {
            let RenderRoute::ExactFallback(region) = route else {
                return Err(RenderError::InvalidGeometry);
            };
            let capacity_bytes = self
                .factory
                .target_edit_budget_bytes
                .ok_or(RenderError::MissingWorkspace)?;
            if region.required_bytes() > capacity_bytes {
                return Err(RenderError::InsufficientWorkspace {
                    required_bytes: region.required_bytes(),
                    capacity_bytes,
                });
            }
            let plan =
                ProjectiveFallbackPlan::from_region(region, self.viewport, PlaneRequirements::CPU)
                    .map_err(RenderError::from)?;
            if plan.required_bytes() == 0 {
                return Ok(());
            }
            if !self.begin_frame() {
                return Err(RenderError::BackendFailure);
            }
            #[cfg(not(target_arch = "wasm32"))]
            return self.draw_composite_fallback(plan, request);
            #[cfg(target_arch = "wasm32")]
            return Err(RenderError::Unsupported(RenderFeature::Readback));
        }
        if let DrawCommand::ApplyBlur { alpha, region } = request.command {
            if *alpha <= Fixed::ZERO
                || *alpha >= Fixed::ONE
                || self.physical_clip_rect(region).is_none()
            {
                return Ok(());
            }
            if !self.begin_frame() {
                return Err(RenderError::BackendFailure);
            }
            return self.blur_target_region(*alpha, region);
        }
        if route != RenderRoute::Native {
            return Err(RenderError::InvalidGeometry);
        }
        Self::classify_request(request)?;
        if !self.begin_frame() {
            return Err(RenderError::BackendFailure);
        }
        if request.projective.is_identity() {
            self.draw(request.command, &request.clip);
            return if self.draw_failed {
                Err(RenderError::BackendFailure)
            } else {
                Ok(())
            };
        }
        self.draw_projective_validated(request.command, &request.clip, &request.projective)
            .map_err(RenderError::from)
    }

    fn output_scale(&self) -> Fixed {
        self.viewport.scale()
    }

    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        use crate::types::TransformClass;

        // Explicit leaf quads short-circuit before affine dispatch.
        match cmd {
            DrawCommand::PushClip { .. } | DrawCommand::PopClip | DrawCommand::ApplyBlur { .. } => {
            }
            DrawCommand::Fill {
                area,
                quad: Some(q),
                color,
                radius,
                opa,
                ..
            } => {
                self.fill_quad_inner(area, q, *radius, clip, color, *opa);
                return;
            }
            DrawCommand::Border {
                area,
                quad: Some(q),
                width,
                radius,
                color,
                opa,
                ..
            } => {
                self.stroke_quad_inner(area, q, *width, *radius, clip, color, *opa);
                return;
            }
            DrawCommand::Blit {
                quad: Some(q),
                texture,
                opa,
                radius,
                composite,
                size,
                ..
            } => {
                self.blit_quad_inner(
                    texture,
                    q,
                    BlitMask {
                        size: *size,
                        radius: *radius,
                    },
                    clip,
                    *opa,
                    *composite,
                );
                return;
            }
            DrawCommand::FillPath {
                path,
                transform,
                paint,
                opa,
                fill_rule,
                ..
            } if !matches!(
                transform.classify(),
                TransformClass::Identity | TransformClass::Translate
            ) =>
            {
                self.fill_path_transformed_inner(path, clip, transform, paint, *opa, *fill_rule);
                return;
            }
            DrawCommand::StrokePath {
                path,
                transform,
                paint,
                width,
                opa,
                line_cap,
                line_join,
                miter_limit,
                dash,
            } => {
                let color = paint_color(paint);
                self.stroke_path_styled_inner(
                    path,
                    clip,
                    Some(transform),
                    StrokeSpec {
                        width: *width,
                        cap: *line_cap,
                        join: *line_join,
                        miter_limit: *miter_limit,
                        dash,
                        dash_scale: Fixed::ONE,
                    },
                    &color,
                    *opa,
                );
                return;
            }
            DrawCommand::GlyphRun {
                pos,
                transform,
                glyphs,
                font,
                color,
                opa,
            } => {
                self.draw_glyph_run_inner(GlyphRunDraw {
                    pos,
                    glyphs,
                    font,
                    transform,
                    clip,
                    color,
                    opacity: *opa,
                    projective: Transform3D::IDENTITY,
                });
                return;
            }
            DrawCommand::PosedGlyphRun {
                pos,
                transform,
                glyphs,
                font,
                color,
                opa,
            } => {
                self.draw_posed_glyph_run_inner(PosedGlyphRunDraw {
                    pos,
                    glyphs: *glyphs,
                    font,
                    transform,
                    clip,
                    color,
                    opacity: *opa,
                    projective: Transform3D::IDENTITY,
                });
                return;
            }
            _ => {}
        }

        let tf = cmd.transform();
        let (tx, ty) = match tf.classify() {
            TransformClass::Identity => (Fixed::ZERO, Fixed::ZERO),
            TransformClass::Translate => (tf.tx, tf.ty),
            other => unimplemented!(
                "wgpu backend: transform class {:?} not yet handled — render_system should pre-project to a quad",
                other
            ),
        };

        match cmd {
            DrawCommand::PushClip { .. } | DrawCommand::PopClip | DrawCommand::ApplyBlur { .. } => {
            }
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => {
                let area = offset_rect(area, tx, ty);
                self.fill_rect_inner(&area, clip, color, *radius, *opa);
            }
            DrawCommand::Blit {
                pos,
                size,
                texture,
                opa,
                radius,
                composite,
                ..
            } => {
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                let pos = offset_point(pos, tx, ty);
                self.blit_inner(
                    texture, &src_rect, pos, *size, clip, *opa, *radius, *composite,
                );
            }
            DrawCommand::Border {
                area,
                width,
                radius,
                color,
                opa,
                ..
            } => {
                let area = offset_rect(area, tx, ty);
                let half = *width / 2;
                let path = Path::rounded_rect(
                    area.x + half,
                    area.y + half,
                    area.w - *width,
                    area.h - *width,
                    *radius,
                );
                self.stroke_path_inner(&path, clip, *width, color, *opa);
            }
            DrawCommand::Line {
                p1,
                p2,
                width,
                color,
                opa,
                ..
            } => {
                let p1 = offset_point(p1, tx, ty);
                let p2 = offset_point(p2, tx, ty);
                let mut path = Path::new();
                path.move_to(p1).line_to(p2);
                self.stroke_path_inner(&path, clip, *width, color, *opa);
            }
            DrawCommand::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                width,
                color,
                opa,
                ..
            } => {
                let center = offset_point(center, tx, ty);
                let path = Path::arc(center, *radius, *start_angle, *end_angle);
                self.stroke_path_inner(&path, clip, *width, color, *opa);
            }
            DrawCommand::FillPath {
                path,
                paint,
                opa,
                fill_rule,
                ..
            } => {
                let color = paint_color(paint);
                if tx == Fixed::ZERO && ty == Fixed::ZERO {
                    self.fill_path_inner(path, clip, &color, *opa, *fill_rule);
                } else {
                    let translate = crate::types::Transform::translate(tx, ty);
                    self.fill_path_transformed_inner(
                        path, clip, &translate, paint, *opa, *fill_rule,
                    );
                }
            }
            DrawCommand::StrokePath { .. } => unreachable!("stroke path returns before dispatch"),
            DrawCommand::GlyphRun { .. } | DrawCommand::PosedGlyphRun { .. } => {
                unreachable!("glyph runs return before transform dispatch")
            }
        }
    }

    fn preflight_projective(
        &self,
        command: &DrawCommand,
        _clip: &Rect,
        projective: &Transform3D,
    ) -> Result<(), crate::render::ProjectiveDrawError> {
        use crate::render::ProjectiveDrawError;

        if projective.is_identity() {
            return Ok(());
        }
        let logical = projective.compose(&Transform3D::from_affine(command.transform()));
        if logical.inverse().is_none() {
            return Err(ProjectiveDrawError::InvalidProjection);
        }
        let supported = matches!(
            command,
            DrawCommand::Fill { .. }
                | DrawCommand::Border { .. }
                | DrawCommand::Blit { .. }
                | DrawCommand::GlyphRun { .. }
                | DrawCommand::PosedGlyphRun { .. }
        );
        if !supported {
            return Err(ProjectiveDrawError::Unsupported);
        }
        let valid_geometry = match command {
            DrawCommand::Fill { area, .. } | DrawCommand::Border { area, .. } => logical
                .apply_rect(*area)
                .is_some_and(|quad| quad_projection_valid(area.w, area.h, &quad)),
            DrawCommand::Blit {
                pos, size, texture, ..
            } => logical
                .apply_rect(Rect::new(pos.x, pos.y, size.x, size.y))
                .is_some_and(|quad| {
                    quad_projection_valid(
                        Fixed::from_int(i32::from(texture.width)),
                        Fixed::from_int(i32::from(texture.height)),
                        &quad,
                    )
                }),
            DrawCommand::PosedGlyphRun {
                pos,
                glyphs,
                font,
                transform,
                ..
            } => glyphs
                .ink_bounds(font, *pos, *transform, self.viewport.scale())
                .is_none_or(|bounds| projective.apply_rect(bounds).is_some()),
            DrawCommand::GlyphRun {
                pos,
                transform,
                glyphs,
                font,
                ..
            } => {
                let output_ppem = crate::render::font::output_ppem(
                    font.size.max(1),
                    self.viewport.scale() * transform.raster_scale(),
                );
                font.glyph_run_ink_bounds(glyphs, *pos, *transform, output_ppem)
                    .is_none_or(|bounds| projective.apply_rect(bounds).is_some())
            }
            DrawCommand::Line { .. }
            | DrawCommand::Arc { .. }
            | DrawCommand::FillPath { .. }
            | DrawCommand::StrokePath { .. }
            | DrawCommand::PushClip { .. }
            | DrawCommand::PopClip
            | DrawCommand::ApplyBlur { .. } => true,
        };
        if !valid_geometry {
            return Err(ProjectiveDrawError::InvalidProjection);
        }
        Ok(())
    }

    fn flush(&mut self) {
        if self.frame.is_none() {
            return;
        }
        if self.flush_ops_to_swapchain(true).is_err() {
            self.draw_failed = true;
        }
    }

    fn supports_offscreen(&self) -> bool {
        true
    }

    fn offscreen_format(&self) -> Option<crate::render::texture::ColorFormat> {
        Some(crate::render::texture::ColorFormat::RGBA8888)
    }

    fn prepare_readback(&mut self, _src: &Rect) -> Result<(), RenderError> {
        if self.frame.is_some() {
            self.flush_ops_to_swapchain(false)?;
        }
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn sample_target_region(
        &self,
        src: &Rect,
    ) -> Result<Option<crate::render::texture::Texture<'static>>, RenderError> {
        let Some(phys) = self.physical_clip_rect(src) else {
            return Ok(None);
        };
        let w = phys.width();
        let h = phys.height();
        let frame = self.frame.as_ref().ok_or(RenderError::BackendFailure)?;
        let state = self.surface.state().ok_or(RenderError::BackendFailure)?;
        let bytes = wgpu_readback_rgba8(
            &state.device,
            &state.queue,
            &frame.surface_texture.texture,
            state.config.format,
            u32::from(phys.x()),
            u32::from(phys.y()),
            u32::from(w),
            u32::from(h),
        )
        .ok_or(RenderError::BackendFailure)?;
        crate::render::texture::Texture::from_vec(
            bytes,
            w,
            h,
            crate::render::texture::ColorFormat::RGBA8888,
        )
        .map(Some)
        .ok_or(RenderError::BackendFailure)
    }

    #[cfg(target_arch = "wasm32")]
    fn sample_target_region(
        &self,
        _src: &Rect,
    ) -> Result<Option<crate::render::texture::Texture<'static>>, RenderError> {
        Err(RenderError::Unsupported(RenderFeature::Readback))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read_target_region(
        &self,
        src: &Rect,
        dst: &mut crate::render::texture::Texture,
    ) -> Result<(), RenderError> {
        let Some(phys) = self.physical_clip_rect(src) else {
            return Ok(());
        };
        let w = u32::from(phys.width());
        let h = u32::from(phys.height());
        let Some(frame) = self.frame.as_ref() else {
            return Err(RenderError::BackendFailure);
        };
        let Some(state) = self.surface.state() else {
            return Err(RenderError::BackendFailure);
        };
        let bytes = wgpu_readback_rgba8(
            &state.device,
            &state.queue,
            &frame.surface_texture.texture,
            state.config.format,
            u32::from(phys.x()),
            u32::from(phys.y()),
            w,
            h,
        )
        .ok_or(RenderError::BackendFailure)?;
        let (x, y, _, _) = self.viewport.rect_to_physical_pixel_bounds(*src);
        crate::render::renderer::copy_packed_rgba8(&bytes, phys, (x, y), dst)
    }

    #[cfg(target_arch = "wasm32")]
    fn read_target_region(
        &self,
        _src: &Rect,
        _dst: &mut crate::render::texture::Texture,
    ) -> Result<(), RenderError> {
        Err(RenderError::Unsupported(RenderFeature::Readback))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn modify_target_region(
        &mut self,
        src: &Rect,
        f: &mut dyn FnMut(&mut crate::render::texture::Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        self.prepare_readback(src)?;
        let Some(mut tex) = self.sample_target_region(src)? else {
            return Ok(false);
        };
        f(&mut tex)?;
        let phys = self
            .physical_clip_rect(src)
            .ok_or(RenderError::InvalidGeometry)?;
        let scissor = [
            u32::from(phys.x()),
            u32::from(phys.y()),
            u32::from(phys.width()),
            u32::from(phys.height()),
        ];
        let view = self
            .blit_source_view(&tex)
            .ok_or(RenderError::BackendFailure)?;
        let scale = self.viewport.scale();
        self.blit_view_inner(
            view,
            tex.width,
            tex.height,
            &Rect::new(0, 0, tex.width, tex.height),
            Point {
                x: Fixed::from(phys.x()) / scale,
                y: Fixed::from(phys.y()) / scale,
            },
            Point {
                x: Fixed::from(phys.width()) / scale,
                y: Fixed::from(phys.height()) / scale,
            },
            255,
            Fixed::ZERO,
            CompositeMode::SourceOver,
            ShaderKind::BlitReplace,
            scissor,
        );
        Ok(true)
    }

    #[cfg(target_arch = "wasm32")]
    fn modify_target_region(
        &mut self,
        _src: &Rect,
        _f: &mut dyn FnMut(&mut crate::render::texture::Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        Err(RenderError::Unsupported(RenderFeature::Readback))
    }
}

impl<B: WgpuTarget> Renderer for WgpuRenderer<'_, B> {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        WgpuRenderer::route(self, request)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        WgpuRenderer::submit(self, request)
    }

    fn submit_with_route(
        &mut self,
        request: &DrawRequest<'_, '_>,
        route: RenderRoute,
    ) -> Result<(), RenderError> {
        WgpuRenderer::submit_with_route(self, request, route)
    }

    fn flush(&mut self) {
        WgpuRenderer::flush(self)
    }

    fn output_scale(&self) -> Fixed {
        WgpuRenderer::output_scale(self)
    }

    fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
        FallbackRegion::from_logical_bounds(
            *bounds,
            self.viewport,
            self.factory.target_edit_budget_bytes,
        )
    }

    fn supports_offscreen(&self) -> bool {
        WgpuRenderer::supports_offscreen(self)
    }

    fn offscreen_format(&self) -> Option<crate::render::texture::ColorFormat> {
        WgpuRenderer::offscreen_format(self)
    }

    fn prepare_readback(&mut self, src: &Rect) -> Result<(), RenderError> {
        WgpuRenderer::prepare_readback(self, src)
    }

    fn sample_target_region(&self, src: &Rect) -> Result<Option<Texture<'static>>, RenderError> {
        WgpuRenderer::sample_target_region(self, src)
    }

    fn read_target_region(&self, src: &Rect, dst: &mut Texture) -> Result<(), RenderError> {
        WgpuRenderer::read_target_region(self, src, dst)
    }

    fn modify_target_region(
        &mut self,
        src: &Rect,
        f: &mut dyn FnMut(&mut Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        WgpuRenderer::modify_target_region(self, src, f)
    }
}

impl<B: WgpuTarget> Drop for WgpuRenderer<'_, B> {
    // App drops the renderer without explicitly calling flush; submit
    // + present here so the frame reaches the screen.
    fn drop(&mut self) {
        if self.frame.is_some() {
            Renderer::flush(self);
        }
    }
}

impl<B: WgpuTarget> Canvas for WgpuRenderer<'_, B> {
    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        self.fill_rect_inner(area, clip, color, radius, opa);
    }

    fn fill_path(
        &mut self,
        path: &Path,
        clip: &Rect,
        paint: &Paint,
        opa: u8,
        fill_rule: crate::render::raster::FillRule,
    ) {
        let color = paint_color(paint);
        self.fill_path_inner(path, clip, &color, opa, fill_rule);
    }

    fn stroke_path(
        &mut self,
        path: &Path,
        clip: &Rect,
        width: Fixed,
        paint: &Paint,
        opa: u8,
        cap: crate::render::raster::LineCap,
        join: crate::render::raster::LineJoin,
        miter_limit: Fixed,
        dash: &[Fixed],
    ) {
        let color = paint_color(paint);
        self.stroke_path_styled_inner(
            path,
            clip,
            None,
            StrokeSpec {
                width,
                cap,
                join,
                miter_limit,
                dash,
                dash_scale: Fixed::ONE,
            },
            &color,
            opa,
        );
    }

    fn blit(
        &mut self,
        src: &Texture,
        src_rect: &Rect,
        dst: Point,
        dst_size: Point,
        clip: &Rect,
        opa: u8,
        radius: Fixed,
        composite: CompositeMode,
    ) {
        self.blit_inner(src, src_rect, dst, dst_size, clip, opa, radius, composite);
    }

    fn clear(&mut self, area: &Rect, color: &Color) {
        self.fill_rect_inner(area, area, color, Fixed::ZERO, 255);
    }

    fn draw_glyph_run(
        &mut self,
        pos: &Point,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &crate::render::font::Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        self.draw_glyph_run_inner(GlyphRunDraw {
            pos,
            glyphs,
            font,
            transform: &crate::types::Transform::IDENTITY,
            clip,
            color,
            opacity: opa,
            projective: Transform3D::IDENTITY,
        });
    }

    fn draw_posed_glyph_run(
        &mut self,
        pos: &Point,
        glyphs: PosedGlyphs<'_>,
        font: &crate::render::font::Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        self.draw_posed_glyph_run_inner(PosedGlyphRunDraw {
            pos,
            glyphs,
            font,
            transform: &Transform::IDENTITY,
            clip,
            color,
            opacity: opa,
            projective: Transform3D::IDENTITY,
        });
    }

    fn flush(&mut self) {
        Renderer::flush(self)
    }
}
