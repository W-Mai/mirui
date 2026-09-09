//! wgpu-backed Renderer + Canvas.

mod path;
mod pipeline;
mod texture_pool;

use wgpu::util::DeviceExt;

use crate::render::canvas::{Canvas, Paint};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::factory::RendererFactory;
use crate::render::path::Path;
use crate::render::renderer::Renderer;
use crate::render::texture::Texture;
use crate::surface::wgpu_surface::WgpuSurface;
use crate::types::{Color, Fixed, Point, Rect, Viewport};

use self::path::PathTessellator;
use self::pipeline::{
    BlitQuadVertex, BlitUniform, GlyphUniform, GlyphVertex, PathTintUniform, PipelineCache,
    PipelineKey, QuadSdfUniform, QuadSdfVertex, RectUniform, ShaderKind, ViewportUniform,
};
use self::texture_pool::{
    CachedScalarSurface, CachedTexture, ScalarSurfaceKey, ScalarSurfacePool, TextureKey,
    TexturePool, new_pool, new_scalar_surface_pool,
};

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

pub struct WgpuRendererFactory {
    cache: Option<PipelineCache>,
    tessellator: PathTessellator,
    texture_pool: TexturePool,
    scalar_surface_pool: ScalarSurfacePool,
    scalar_samples: alloc::vec::Vec<u8>,
    glyph_vertices: alloc::vec::Vec<GlyphVertex>,
    glyph_indices: alloc::vec::Vec<u16>,
    glyph_buffers: GlyphBufferArena,
    /// Samplers are immutable; one instance covers every frame.
    linear_sampler: Option<wgpu::Sampler>,
    nearest_sampler: Option<wgpu::Sampler>,
}

impl WgpuRendererFactory {
    pub fn new() -> Self {
        Self {
            cache: None,
            tessellator: PathTessellator::new(),
            texture_pool: new_pool(),
            scalar_surface_pool: new_scalar_surface_pool(),
            scalar_samples: alloc::vec::Vec::new(),
            glyph_vertices: alloc::vec::Vec::new(),
            glyph_indices: alloc::vec::Vec::new(),
            glyph_buffers: GlyphBufferArena::new(),
            linear_sampler: None,
            nearest_sampler: None,
        }
    }
}

impl Default for WgpuRendererFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl RendererFactory<WgpuSurface> for WgpuRendererFactory {
    type Renderer<'a>
        = WgpuRenderer<'a>
    where
        Self: 'a;

    fn make<'a>(
        &'a mut self,
        backend: &'a mut WgpuSurface,
        transform: &Viewport,
    ) -> WgpuRenderer<'a> {
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
        }
    }
}

pub struct WgpuRenderer<'a> {
    factory: &'a mut WgpuRendererFactory,
    surface: &'a mut WgpuSurface,
    viewport: Viewport,
    frame: Option<Frame>,
}

struct Frame {
    surface_texture: wgpu::SurfaceTexture,
    swapchain_view: wgpu::TextureView,
    msaa_view: wgpu::TextureView,
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

/// 1 MiB / 256 B = 4096 draws per frame before the arena overflows.
/// Past that the overflowing draw is silently dropped — pick a size
/// large enough that real workloads never hit the cap.
const UNIFORM_ARENA_SIZE: u64 = 1024 * 1024;

/// Most desktop / mobile GPUs require 256-byte alignment for dynamic
/// uniform offsets. `RectUniform` is 48 B, `PathTintUniform` is 16 B —
/// align up to the limit so any device accepts the offset.
const UNIFORM_ALIGN: u32 = 256;
const GLYPHS_PER_BATCH: usize = 2_048;
const GLYPH_BUFFER_CAPACITY: usize = GLYPHS_PER_BATCH * 2;

struct GlyphBufferArena {
    vertex: Option<wgpu::Buffer>,
    index: Option<wgpu::Buffer>,
    glyph_cursor: usize,
}

struct GlyphBufferUpload {
    vertex: wgpu::Buffer,
    vertex_range: core::ops::Range<u64>,
    index: wgpu::Buffer,
    index_range: core::ops::Range<u64>,
}

impl GlyphBufferArena {
    const fn new() -> Self {
        Self {
            vertex: None,
            index: None,
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

    fn ranges(
        glyph_start: usize,
        glyph_count: usize,
    ) -> Option<(core::ops::Range<u64>, core::ops::Range<u64>)> {
        let vertex_start = glyph_start
            .checked_mul(4)?
            .checked_mul(core::mem::size_of::<GlyphVertex>())? as u64;
        let vertex_end = vertex_start.checked_add(
            glyph_count
                .checked_mul(4)?
                .checked_mul(core::mem::size_of::<GlyphVertex>())? as u64,
        )?;
        let index_start = glyph_start
            .checked_mul(6)?
            .checked_mul(core::mem::size_of::<u16>())? as u64;
        let index_end = index_start.checked_add(
            glyph_count
                .checked_mul(6)?
                .checked_mul(core::mem::size_of::<u16>())? as u64,
        )?;
        Some((vertex_start..vertex_end, index_start..index_end))
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[GlyphVertex],
        indices: &[u16],
    ) -> Option<GlyphBufferUpload> {
        let glyph_count = vertices.len().checked_div(4)?;
        if glyph_count * 4 != vertices.len()
            || glyph_count * 6 != indices.len()
            || !self.can_fit(glyph_count)
        {
            return None;
        }
        if self.vertex.is_none() {
            self.vertex = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mirui-glyph-vertex-arena"),
                size: (GLYPH_BUFFER_CAPACITY * 4 * core::mem::size_of::<GlyphVertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if self.index.is_none() {
            self.index = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mirui-glyph-index-arena"),
                size: (GLYPH_BUFFER_CAPACITY * 6 * core::mem::size_of::<u16>()) as u64,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }

        let (vertex_range, index_range) = Self::ranges(self.glyph_cursor, glyph_count)?;
        let vertex = self.vertex.as_ref()?;
        let index = self.index.as_ref()?;
        queue.write_buffer(vertex, vertex_range.start, bytemuck::cast_slice(vertices));
        queue.write_buffer(index, index_range.start, bytemuck::cast_slice(indices));
        self.glyph_cursor += glyph_count;

        Some(GlyphBufferUpload {
            vertex: vertex.clone(),
            vertex_range,
            index: index.clone(),
            index_range,
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
}

impl WgpuRenderer<'_> {
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
            .create_view(&wgpu::TextureViewDescriptor::default());
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
    fn flush_ops_to_swapchain(&mut self, present: bool) {
        let Some(frame) = self.frame.as_mut() else {
            return;
        };
        if frame.ops.is_empty() && !present && frame.has_committed_pass {
            return;
        }
        let state = match self.surface.state() {
            Some(s) => s,
            None => return,
        };
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
            let mut pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mirui-frame-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame.msaa_view,
                        resolve_target: Some(&frame.swapchain_view),
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
                    pass.draw_indexed(0..op.count, 0, 0..1);
                } else {
                    pass.draw(0..op.count, 0..1);
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
    }

    /// Append a uniform to the frame's arena. Returns the dynamic
    /// offset for `set_bind_group`, or `None` when the arena is full;
    /// callers drop the draw on `None`.
    fn push_uniform<T: bytemuck::Pod>(&mut self, value: &T) -> Option<u32> {
        let frame = self.frame.as_mut()?;
        let offset = frame.uniform_cursor;
        if (offset as u64) + UNIFORM_ALIGN as u64 > UNIFORM_ARENA_SIZE {
            return None;
        }
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
            scissor,
            dynamic_offset: Some(offset),
        });
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

        let tex_view = {
            let state = self
                .surface
                .state()
                .expect("WgpuSurface state missing in blit");
            if src.transient {
                let Some(rgba) = texture_to_rgba8(src) else {
                    return;
                };
                let tex = upload_blit_source(&state.device, &state.queue, src, &rgba);
                tex.create_view(&wgpu::TextureViewDescriptor::default())
            } else {
                let key = TextureKey::from(src);
                let handle = match self
                    .factory
                    .texture_pool
                    .entry(key)
                    .or_try_insert_with::<_, ()>(|| {
                        let rgba = texture_to_rgba8(src).ok_or(())?;
                        Ok(CachedTexture(upload_blit_source(
                            &state.device,
                            &state.queue,
                            src,
                            &rgba,
                        )))
                    }) {
                    Ok(h) => h,
                    Err(_) => return,
                };
                handle
                    .0
                    .create_view(&wgpu::TextureViewDescriptor::default())
            }
        };

        self.blit_view_inner(
            tex_view, src.width, src.height, src_rect, dst_pos, dst_size, opa, composite, scissor,
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
        composite: CompositeMode,
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
            .linear_sampler
            .as_ref()
            .expect("linear sampler must be initialised before blit");

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
            alpha: [opa as f32 / 255.0, 0.0, 0.0, 0.0],
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
                shader: ShaderKind::Blit,
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
            scissor,
            dynamic_offset: None,
        });
    }
}

/// Logical clip → physical scissor, clamped to the swapchain extent.
/// wgpu validates `x + w <= extent` and `y + h <= extent` so any clip
/// extending past the surface must be cropped before reaching the pass.
fn clip_to_scissor(clip: &Rect, scale: f32, surface_w: u32, surface_h: u32) -> [u32; 4] {
    let x0 = (clip.x.to_f32() * scale).max(0.0).min(surface_w as f32) as u32;
    let y0 = (clip.y.to_f32() * scale).max(0.0).min(surface_h as f32) as u32;
    let x1 = ((clip.x.to_f32() + clip.w.to_f32()) * scale)
        .max(0.0)
        .min(surface_w as f32) as u32;
    let y1 = ((clip.y.to_f32() + clip.h.to_f32()) * scale)
        .max(0.0)
        .min(surface_h as f32) as u32;
    [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
}

impl WgpuRenderer<'_> {
    fn scissor_from_clip(&self, clip: &Rect) -> [u32; 4] {
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing for scissor");
        let scale = self.viewport.scale().to_f32().max(1.0);
        clip_to_scissor(clip, scale, state.config.width, state.config.height)
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

/// RGB565 formats return `None`; this upload path only handles
/// byte-aligned RGB/RGBA.
fn texture_to_rgba8(src: &Texture) -> Option<alloc::vec::Vec<u8>> {
    use crate::render::texture::ColorFormat;
    let buf = src.buf.as_slice();
    let bpp = src.format.bytes_per_pixel();
    let w = src.width as usize;
    let h = src.height as usize;
    match src.format {
        ColorFormat::RGBA8888 => {
            let mut out = alloc::vec::Vec::with_capacity(w * h * 4);
            for y in 0..h {
                let row = &buf[y * src.stride..y * src.stride + w * bpp];
                out.extend_from_slice(row);
            }
            Some(out)
        }
        ColorFormat::BGRA8888 => {
            let mut out = alloc::vec::Vec::with_capacity(w * h * 4);
            for y in 0..h {
                for x in 0..w {
                    let i = y * src.stride + x * bpp;
                    out.extend_from_slice(&[buf[i + 2], buf[i + 1], buf[i], buf[i + 3]]);
                }
            }
            Some(out)
        }
        ColorFormat::RGB888 => {
            let mut out = alloc::vec::Vec::with_capacity(w * h * 4);
            for y in 0..h {
                for x in 0..w {
                    let i = y * src.stride + x * bpp;
                    out.extend_from_slice(&[buf[i], buf[i + 1], buf[i + 2], 255]);
                }
            }
            Some(out)
        }
        ColorFormat::RGB565 | ColorFormat::RGB565Swapped => None,
    }
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

impl WgpuRenderer<'_> {
    fn fill_path_transformed_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        cmd_tf: &crate::types::Transform,
        paint: &Paint,
        opa: u8,
    ) {
        let color = paint_color(paint);
        let (verts, indices) = {
            let (v, i) = self.factory.tessellator.fill(path, Some(cmd_tf));
            (v.to_vec(), i.to_vec())
        };
        self.draw_path_mesh(&verts, &indices, clip, &color, opa);
    }
}

impl WgpuRenderer<'_> {
    fn physical_clip_rect(&self, src: &Rect) -> Option<Rect> {
        let phys = self.viewport.rect_to_physical(*src);
        let state = self.surface.state()?;
        let target = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(state.config.width as i32),
            h: Fixed::from_int(state.config.height as i32),
        };
        let clipped = phys.intersect(&target)?;
        if clipped.w <= Fixed::ZERO || clipped.h <= Fixed::ZERO {
            return None;
        }
        Some(clipped)
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
                out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        } else {
            out.extend_from_slice(&data[start..end]);
        }
    }
    drop(data);
    staging.unmap();
    Some(out)
}

impl WgpuRenderer<'_> {
    fn fill_path_inner(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        let (verts, indices) = {
            let (v, i) = self.factory.tessellator.fill(path, None);
            (v.to_vec(), i.to_vec())
        };
        self.draw_path_mesh(&verts, &indices, clip, color, opa);
    }

    fn stroke_path_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        let (verts, indices) = {
            let (v, i) = self
                .factory
                .tessellator
                .stroke(path, None, width.to_f32().max(1.0));
            (v.to_vec(), i.to_vec())
        };
        self.draw_path_mesh(&verts, &indices, clip, color, opa);
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
            scissor,
            dynamic_offset: Some(offset),
        });
    }

    /// Perspective-correct quad blit via `Transform3D::from_quad`.
    fn blit_quad_inner(
        &mut self,
        src: &Texture,
        q: &[Point; 4],
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
            // Degenerate quad — AABB fallback keeps the widget on screen.
            return self.blit_inner(
                src,
                &src_rect,
                Point {
                    x: q[0].x,
                    y: q[0].y,
                },
                Point {
                    x: q[2].x - q[0].x,
                    y: q[2].y - q[0].y,
                },
                clip,
                opa,
                composite,
            );
        };

        let corners = [(0.0_f32, 0.0_f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let m20 = forward.m20.to_f32();
        let m21 = forward.m21.to_f32();
        let m22 = forward.m22.to_f32();
        // `from_quad` takes a pixel-space src rect, so unit corners need
        // source-size scaling before plugging into the bottom row.
        let sw = src.width as f32;
        let sh = src.height as f32;

        let alpha_f = opa as f32 / 255.0;
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
                alpha: alpha_f,
            };
        }
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

        let key = TextureKey::from(src);
        let tex_handle: crate::core::cache::Handle<CachedTexture> = {
            let state = self
                .surface
                .state()
                .expect("WgpuSurface state missing in blit_quad");
            match self
                .factory
                .texture_pool
                .entry(key)
                .or_try_insert_with::<_, ()>(|| {
                    let rgba = texture_to_rgba8(src).ok_or(())?;
                    Ok(CachedTexture(state.device.create_texture_with_data(
                        &state.queue,
                        &wgpu::TextureDescriptor {
                            label: Some("mirui-blit-quad-source"),
                            size: wgpu::Extent3d {
                                width: src.width as u32,
                                height: src.height as u32,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        },
                        wgpu::util::TextureDataOrder::LayerMajor,
                        &rgba,
                    )))
                }) {
                Ok(h) => h,
                Err(_) => return,
            }
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
            .linear_sampler
            .as_ref()
            .expect("linear sampler must be initialised before blit_quad");
        let tex_view = tex_handle
            .0
            .create_view(&wgpu::TextureViewDescriptor::default());

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
        } = draw;
        let Some(first) = glyphs.first() else {
            return;
        };
        if !self.begin_frame() {
            return;
        }
        let scissor = self.scissor_from_clip(clip);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }
        let requested_size = font.size.max(1);
        let raster_scale = glyph_raster_scale(transform, self.viewport.scale());
        let output_ppem = crate::render::font::output_ppem(requested_size, raster_scale);
        let metrics = font.metrics(requested_size);
        let mut active: Option<GlyphBatch<'_>> = None;
        self.factory.glyph_vertices.clear();
        self.factory.glyph_indices.clear();

        for positioned in glyphs {
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
            if alpha_bits(raster.surface.sample_layout()) != Some(bits) {
                continue;
            }
            let key = GlyphBatchKey {
                surface: ScalarSurfaceKey::new(font.face_id(), font.revision(), raster.surface),
                shader,
                spread,
            };
            if active.map(|batch| batch.key) != Some(key)
                || self.factory.glyph_vertices.len() >= GLYPHS_PER_BATCH * 4
            {
                if let Some(batch) = active.take() {
                    self.submit_glyph_batch(batch, scissor, color, opacity);
                }
                active = Some(GlyphBatch {
                    key,
                    surface: raster.surface,
                });
            }

            let Some(dx) = positioned
                .origin
                .x
                .checked_sub(first.origin.x)
                .and_then(|value| value.checked_add(positioned.offset.x))
            else {
                continue;
            };
            let Some(dy) = positioned
                .origin
                .y
                .checked_sub(first.origin.y)
                .and_then(|value| value.checked_add(positioned.offset.y))
            else {
                continue;
            };
            let scale = Fixed::from_int(i32::from(requested_size))
                / Fixed::from_int(i32::from(raster.representation.design_ppem().max(1)));
            let rect = Rect {
                x: pos.x + crate::types::fixed::from_textflow(dx) + raster.offset_x,
                y: pos.y + metrics.ascender + crate::types::fixed::from_textflow(dy)
                    - raster.offset_y,
                w: Fixed::from_int(region.width() as i32) * scale,
                h: Fixed::from_int(region.height() as i32) * scale,
            };
            append_glyph_quad(
                &mut self.factory.glyph_vertices,
                &mut self.factory.glyph_indices,
                rect,
                region,
                raster.surface,
                transform,
            );
        }
        if let Some(batch) = active {
            self.submit_glyph_batch(batch, scissor, color, opacity);
        }
    }

    fn submit_glyph_batch(
        &mut self,
        batch: GlyphBatch<'_>,
        scissor: [u32; 4],
        color: &Color,
        opa: u8,
    ) {
        if self.factory.glyph_indices.is_empty() {
            return;
        }
        let glyph_count = self.factory.glyph_indices.len() / 6;
        if !self.factory.glyph_buffers.can_fit(glyph_count) {
            self.flush_ops_to_swapchain(false);
            self.factory.glyph_buffers.reset();
        }
        if !self.factory.glyph_buffers.can_fit(glyph_count) {
            self.factory.glyph_vertices.clear();
            self.factory.glyph_indices.clear();
            return;
        }
        let uniform = GlyphUniform {
            color: [
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0 * opa as f32 / 255.0,
            ],
            spread_pad: [f32::from(batch.key.spread), 0.0, 0.0, 0.0],
        };
        let Some(offset) = self.push_uniform(&uniform) else {
            self.factory.glyph_vertices.clear();
            self.factory.glyph_indices.clear();
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
                    unpack_scalar_surface(batch.surface, samples).ok_or(())?;
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
                    self.factory.glyph_vertices.clear();
                    self.factory.glyph_indices.clear();
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
            &self.factory.glyph_vertices,
            &self.factory.glyph_indices,
        ) else {
            self.factory.glyph_vertices.clear();
            self.factory.glyph_indices.clear();
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
            vertex_buf: Some(upload.vertex),
            vertex_range: Some(upload.vertex_range),
            index_buf: Some(upload.index),
            index_range: Some(upload.index_range),
            index_format: wgpu::IndexFormat::Uint16,
            count: self.factory.glyph_indices.len() as u32,
            scissor,
            dynamic_offset: Some(offset),
        });
        self.factory.glyph_vertices.clear();
        self.factory.glyph_indices.clear();
    }
}

fn alpha_bits(layout: mirx::image::SampleLayout) -> Option<u8> {
    match layout {
        mirx::image::SampleLayout::A1 => Some(1),
        mirx::image::SampleLayout::A2 => Some(2),
        mirx::image::SampleLayout::A4 => Some(4),
        mirx::image::SampleLayout::A8 => Some(8),
        _ => None,
    }
}

fn unpack_scalar_surface(
    surface: crate::render::font::GlyphSurface<'_>,
    output: &mut alloc::vec::Vec<u8>,
) -> Option<()> {
    let bits = alpha_bits(surface.sample_layout())?;
    let width = usize::try_from(surface.width()).ok()?;
    let height = usize::try_from(surface.height()).ok()?;
    let stride = usize::try_from(surface.stride()).ok()?;
    let len = width.checked_mul(height)?;
    output.clear();
    output.resize(len, 0);
    let max = (1u16 << bits) - 1;
    for y in 0..height {
        let row = surface
            .samples()
            .get(y.checked_mul(stride)?..)?
            .get(..stride)?;
        for x in 0..width {
            let bit = x.checked_mul(usize::from(bits))?;
            let byte = *row.get(bit / 8)?;
            let shift = 8 - bits - (bit % 8) as u8;
            let value = u16::from((byte >> shift) & max as u8);
            output[y * width + x] = ((value * 255 + max / 2) / max) as u8;
        }
    }
    Some(())
}

fn glyph_raster_scale(transform: &crate::types::Transform, viewport_scale: Fixed) -> Fixed {
    let x = (transform.m00 * transform.m00 + transform.m10 * transform.m10).sqrt();
    let y = (transform.m01 * transform.m01 + transform.m11 * transform.m11).sqrt();
    viewport_scale * x.max(y).max(Fixed::ONE)
}

fn append_glyph_quad(
    vertices: &mut alloc::vec::Vec<GlyphVertex>,
    indices: &mut alloc::vec::Vec<u16>,
    rect: Rect,
    region: mirx::image::Region,
    surface: crate::render::font::GlyphSurface<'_>,
    transform: &crate::types::Transform,
) {
    let Ok(base) = u16::try_from(vertices.len()) else {
        return;
    };
    let points = transform.apply_rect(rect);
    let width = surface.width() as f32;
    let height = surface.height() as f32;
    let x0 = region.x() as f32 / width;
    let y0 = region.y() as f32 / height;
    let x1 = region.x().saturating_add(region.width()) as f32 / width;
    let y1 = region.y().saturating_add(region.height()) as f32 / height;
    let bounds = [x0, y0, x1, y1];
    for (point, uv) in points
        .into_iter()
        .zip([[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
    {
        vertices.push(GlyphVertex {
            pos: [point.x.to_f32(), point.y.to_f32()],
            uv,
            uv_bounds: bounds,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[cfg(test)]
mod glyph_tests {
    use super::*;
    use crate::render::font::{FontSurfaceId, GlyphSurface};

    #[test]
    fn glyph_buffer_ranges_are_disjoint_and_capacity_is_bounded() {
        let (first_vertices, first_indices) = GlyphBufferArena::ranges(0, 1).unwrap();
        let (next_vertices, next_indices) = GlyphBufferArena::ranges(1, 2).unwrap();
        assert_eq!(first_vertices.end, next_vertices.start);
        assert_eq!(first_indices.end, next_indices.start);
        assert_eq!(
            next_vertices.end - next_vertices.start,
            8 * core::mem::size_of::<GlyphVertex>() as u64
        );
        assert_eq!(
            next_indices.end - next_indices.start,
            12 * core::mem::size_of::<u16>() as u64
        );

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

        unpack_scalar_surface(surface, &mut output).unwrap();

        assert_eq!(output, [255, 0, 255, 0, 255, 0]);
    }

    #[test]
    fn glyph_quad_keeps_region_bounds_under_transform() {
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
        let mut vertices = alloc::vec::Vec::new();
        let mut indices = alloc::vec::Vec::new();

        append_glyph_quad(
            &mut vertices,
            &mut indices,
            Rect::new(1, 2, 4, 3),
            region,
            surface,
            &crate::types::Transform::translate(Fixed::from_int(5), Fixed::from_int(7)),
        );

        assert_eq!(vertices.len(), 4);
        assert_eq!(indices, [0, 1, 2, 0, 2, 3]);
        assert_eq!(vertices[0].pos, [6.0, 9.0]);
        assert_eq!(vertices[2].pos, [10.0, 12.0]);
        assert_eq!(vertices[0].uv_bounds, [0.25, 0.125, 0.75, 0.5]);
    }
}

impl Renderer for WgpuRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        use crate::types::TransformClass;

        // Quad short-circuits: render_system already pre-projected the
        // 3D / non-affine widget into 4 corner points, so the GPU only
        // has to draw the resulting quad.
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
                ..
            } => {
                if *radius != Fixed::ZERO {
                    unimplemented!(
                        "wgpu backend: Blit.radius mask not implemented; use SwRenderer",
                    );
                }
                self.blit_quad_inner(texture, q, clip, *opa, *composite);
                return;
            }
            DrawCommand::FillPath {
                path,
                transform,
                paint,
                opa,
                ..
            } if !matches!(
                transform.classify(),
                TransformClass::Identity | TransformClass::Translate
            ) =>
            {
                self.fill_path_transformed_inner(path, clip, transform, paint, *opa);
                return;
            }
            DrawCommand::StrokePath { .. } => {
                unimplemented!("wgpu backend: StrokePath not yet implemented");
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
                if *radius != Fixed::ZERO {
                    unimplemented!(
                        "wgpu backend: Blit.radius mask not implemented; use SwRenderer",
                    );
                }
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                let pos = offset_point(pos, tx, ty);
                self.blit_inner(texture, &src_rect, pos, *size, clip, *opa, *composite);
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
                path, paint, opa, ..
            } => {
                let color = paint_color(paint);
                if tx == Fixed::ZERO && ty == Fixed::ZERO {
                    self.fill_path_inner(path, clip, &color, *opa);
                } else {
                    let translate = crate::types::Transform::translate(tx, ty);
                    self.fill_path_transformed_inner(path, clip, &translate, paint, *opa);
                }
            }
            DrawCommand::StrokePath {
                path,
                width,
                paint,
                opa,
                ..
            } => {
                let color = paint_color(paint);
                if tx == Fixed::ZERO && ty == Fixed::ZERO {
                    self.stroke_path_inner(path, clip, *width, &color, *opa);
                } else {
                    unimplemented!("wgpu backend: StrokePath under translate not yet implemented");
                }
            }
            DrawCommand::GlyphRun { .. } => {
                unreachable!("glyph runs return before transform dispatch")
            }
        }
    }

    fn flush(&mut self) {
        if self.frame.is_none() {
            return;
        }
        self.flush_ops_to_swapchain(true);
    }

    fn supports_offscreen(&self) -> bool {
        true
    }

    fn offscreen_format(&self) -> Option<crate::render::texture::ColorFormat> {
        Some(crate::render::texture::ColorFormat::RGBA8888)
    }

    fn prepare_readback(&mut self, _src: &Rect) {
        if self.frame.is_some() {
            self.flush_ops_to_swapchain(false);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn sample_target_region(&self, src: &Rect) -> Option<crate::render::texture::Texture<'static>> {
        let phys = self.physical_clip_rect(src)?;
        let w = phys.w.to_int() as u32;
        let h = phys.h.to_int() as u32;
        let frame = self.frame.as_ref()?;
        let state = self.surface.state()?;
        let bytes = wgpu_readback_rgba8(
            &state.device,
            &state.queue,
            &frame.surface_texture.texture,
            state.config.format,
            phys.x.to_int() as u32,
            phys.y.to_int() as u32,
            w,
            h,
        )?;
        let mut tex = crate::render::texture::Texture::owned(
            w as u16,
            h as u16,
            crate::render::texture::ColorFormat::RGBA8888,
        );
        if let crate::render::texture::TexBuf::Owned(ref mut dst) = tex.buf {
            let copy = dst.len().min(bytes.len());
            dst[..copy].copy_from_slice(&bytes[..copy]);
        }
        Some(tex.with_transient(true))
    }

    #[cfg(target_arch = "wasm32")]
    fn sample_target_region(
        &self,
        _src: &Rect,
    ) -> Option<crate::render::texture::Texture<'static>> {
        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn read_target_region(&self, src: &Rect, dst: &mut crate::render::texture::Texture) {
        let Some(phys) = self.physical_clip_rect(src) else {
            return;
        };
        let w = phys.w.to_int() as u32;
        let h = phys.h.to_int() as u32;
        let Some(frame) = self.frame.as_ref() else {
            return;
        };
        let Some(state) = self.surface.state() else {
            return;
        };
        let Some(bytes) = wgpu_readback_rgba8(
            &state.device,
            &state.queue,
            &frame.surface_texture.texture,
            state.config.format,
            phys.x.to_int() as u32,
            phys.y.to_int() as u32,
            w,
            h,
        ) else {
            return;
        };
        if let crate::render::texture::TexBuf::Owned(ref mut buf) = dst.buf {
            let copy = buf.len().min(bytes.len());
            buf[..copy].copy_from_slice(&bytes[..copy]);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn read_target_region(&self, _src: &Rect, _dst: &mut crate::render::texture::Texture) {}

    #[cfg(not(target_arch = "wasm32"))]
    fn modify_target_region(
        &mut self,
        src: &Rect,
        f: &mut dyn FnMut(&mut crate::render::texture::Texture),
    ) -> bool {
        let Some(mut tex) = self.sample_target_region(src) else {
            return false;
        };
        f(&mut tex);
        let tw = tex.width;
        let th = tex.height;
        let src_rect = Rect::new(0, 0, tw, th);
        let dst_pos = Point { x: src.x, y: src.y };
        let dst_size = Point { x: src.w, y: src.h };
        // Blit through the render pipeline, not queue.write_texture:
        // the next pass's LoadOp::Load on the MSAA attachment would
        // otherwise resolve stale multisample contents over these pixels.
        self.blit_inner(
            &tex,
            &src_rect,
            dst_pos,
            dst_size,
            src,
            255,
            CompositeMode::SourceOver,
        );
        true
    }

    #[cfg(target_arch = "wasm32")]
    fn modify_target_region(
        &mut self,
        _src: &Rect,
        _f: &mut dyn FnMut(&mut crate::render::texture::Texture),
    ) -> bool {
        false
    }
}

impl Drop for WgpuRenderer<'_> {
    // App drops the renderer without explicitly calling flush; submit
    // + present here so the frame reaches the screen.
    fn drop(&mut self) {
        if self.frame.is_some() {
            Renderer::flush(self);
        }
    }
}

impl Canvas for WgpuRenderer<'_> {
    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        self.fill_rect_inner(area, clip, color, radius, opa);
    }

    fn fill_path(
        &mut self,
        path: &Path,
        clip: &Rect,
        paint: &Paint,
        opa: u8,
        _fill_rule: crate::render::raster::FillRule,
    ) {
        let color = paint_color(paint);
        self.fill_path_inner(path, clip, &color, opa);
    }

    fn stroke_path(
        &mut self,
        path: &Path,
        clip: &Rect,
        width: Fixed,
        paint: &Paint,
        opa: u8,
        _cap: crate::render::raster::LineCap,
        _join: crate::render::raster::LineJoin,
        _miter_limit: Fixed,
        _dash: &[Fixed],
    ) {
        let color = paint_color(paint);
        self.stroke_path_inner(path, clip, width, &color, opa);
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
        if radius != Fixed::ZERO {
            unimplemented!("wgpu backend: Blit.radius mask not implemented yet; use SwRenderer");
        }
        self.blit_inner(src, src_rect, dst, dst_size, clip, opa, composite);
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
        });
    }

    fn flush(&mut self) {
        Renderer::flush(self)
    }
}
