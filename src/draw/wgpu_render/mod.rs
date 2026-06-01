//! wgpu-backed Renderer + Canvas.

mod label_atlas;
mod path;
mod pipeline;
mod texture_pool;

use wgpu::util::DeviceExt;

use crate::app::RendererFactory;
use crate::draw::canvas::Canvas;
use crate::draw::command::DrawCommand;
use crate::draw::path::Path;
use crate::draw::renderer::Renderer;
use crate::draw::texture::Texture;
use crate::surface::wgpu_surface::WgpuSurface;
use crate::types::{Color, Fixed, Point, Rect, Viewport};

use self::label_atlas::GlyphAtlas;
use self::path::PathTessellator;
use self::pipeline::{
    BlitQuadVertex, BlitUniform, LabelVertex, PathTintUniform, PipelineCache, PipelineKey,
    QuadSdfUniform, QuadSdfVertex, RectUniform, ShaderKind, ViewportUniform,
};
use self::texture_pool::{CachedTexture, TextureKey, TexturePool, new_pool};

pub use self::pipeline::MSAA_SAMPLES;

pub struct WgpuRendererFactory {
    cache: Option<PipelineCache>,
    glyph_atlas: Option<GlyphAtlas>,
    tessellator: PathTessellator,
    texture_pool: TexturePool,
    /// Samplers are immutable; one instance covers every frame.
    linear_sampler: Option<wgpu::Sampler>,
    nearest_sampler: Option<wgpu::Sampler>,
}

impl WgpuRendererFactory {
    pub fn new() -> Self {
        Self {
            cache: None,
            glyph_atlas: None,
            tessellator: PathTessellator::new(),
            texture_pool: new_pool(),
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
        if self.cache.is_none()
            || self.glyph_atlas.is_none()
            || self.linear_sampler.is_none()
            || self.nearest_sampler.is_none()
        {
            let state = backend
                .state()
                .expect("WgpuSurface must be initialised before make()");
            if self.cache.is_none() {
                self.cache = Some(PipelineCache::new(&state.device));
            }
            if self.glyph_atlas.is_none() {
                self.glyph_atlas = Some(GlyphAtlas::new(&state.device, &state.queue));
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
}

/// 1 MiB / 256 B = 4096 draws per frame before the arena overflows.
/// Past that the overflowing draw is silently dropped — pick a size
/// large enough that real workloads never hit the cap.
const UNIFORM_ARENA_SIZE: u64 = 1024 * 1024;

/// Most desktop / mobile GPUs require 256-byte alignment for dynamic
/// uniform offsets. `RectUniform` is 48 B, `PathTintUniform` is 16 B —
/// align up to the limit so any device accepts the offset.
const UNIFORM_ALIGN: u32 = 256;

/// wgpu pipelines / buffers / bind groups are `Arc`-backed clones, so
/// owning them in the `Vec<DrawOp>` keeps them alive until
/// `queue.submit` consumes the encoder.
struct DrawOp {
    pipeline: wgpu::RenderPipeline,
    /// `None` when the op uses the frame-cached fill/path bind group;
    /// `Some` for blit / blit_quad / draw_label which carry textures.
    bind_group: BindGroupRef,
    vertex_buf: Option<wgpu::Buffer>,
    index_buf: Option<wgpu::Buffer>,
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
        });
        true
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
            },
        );

        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Shared(0),
            vertex_buf: None,
            index_buf: None,
            index_format: wgpu::IndexFormat::Uint32,
            count: 4,
            scissor,
            dynamic_offset: Some(offset),
        });
    }

    fn blit_inner(
        &mut self,
        src: &Texture,
        src_rect: &Rect,
        dst_pos: Point,
        dst_size: Point,
        clip: &Rect,
    ) {
        if !self.begin_frame() {
            return;
        }
        let scissor = self.scissor_from_clip(clip);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }

        let key = TextureKey::from(src);
        let tex_handle: crate::cache::Handle<CachedTexture> = {
            let state = self
                .surface
                .state()
                .expect("WgpuSurface state missing in blit");
            match self
                .factory
                .texture_pool
                .entry(key)
                .or_try_insert_with::<_, ()>(|| {
                    let rgba = texture_to_rgba8(src).ok_or(())?;
                    Ok(CachedTexture(state.device.create_texture_with_data(
                        &state.queue,
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
        let tex_view = tex_handle
            .0
            .create_view(&wgpu::TextureViewDescriptor::default());

        let tw = src.width as f32;
        let th = src.height as f32;
        let blit_uniform = BlitUniform {
            dst_pos: [dst_pos.x.to_f32(), dst_pos.y.to_f32()],
            dst_size: [dst_size.x.to_f32(), dst_size.y.to_f32()],
            uv: [
                src_rect.x.to_f32() / tw,
                src_rect.y.to_f32() / th,
                (src_rect.x.to_f32() + src_rect.w.to_f32()) / tw,
                (src_rect.y.to_f32() + src_rect.h.to_f32()) / th,
            ],
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
            },
        );

        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Owned(bind_group),
            vertex_buf: None,
            index_buf: None,
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

/// RGB565 formats return `None`; this upload path only handles
/// byte-aligned RGB/RGBA.
fn texture_to_rgba8(src: &Texture) -> Option<alloc::vec::Vec<u8>> {
    use crate::draw::texture::ColorFormat;
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

fn translate_path(path: &Path, tx: Fixed, ty: Fixed) -> Path {
    use crate::draw::path::PathCmd;
    let cmds = path
        .cmds
        .iter()
        .map(|c| match c {
            PathCmd::MoveTo(p) => PathCmd::MoveTo(Point {
                x: p.x + tx,
                y: p.y + ty,
            }),
            PathCmd::LineTo(p) => PathCmd::LineTo(Point {
                x: p.x + tx,
                y: p.y + ty,
            }),
            PathCmd::QuadTo { ctrl, end } => PathCmd::QuadTo {
                ctrl: Point {
                    x: ctrl.x + tx,
                    y: ctrl.y + ty,
                },
                end: Point {
                    x: end.x + tx,
                    y: end.y + ty,
                },
            },
            PathCmd::CubicTo { ctrl1, ctrl2, end } => PathCmd::CubicTo {
                ctrl1: Point {
                    x: ctrl1.x + tx,
                    y: ctrl1.y + ty,
                },
                ctrl2: Point {
                    x: ctrl2.x + tx,
                    y: ctrl2.y + ty,
                },
                end: Point {
                    x: end.x + tx,
                    y: end.y + ty,
                },
            },
            PathCmd::Close => PathCmd::Close,
        })
        .collect();
    Path { cmds }
}

impl WgpuRenderer<'_> {
    fn fill_path_inner(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        let (verts, indices) = {
            let (v, i) = self.factory.tessellator.fill(path);
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
                .stroke(path, width.to_f32().max(1.0));
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
            },
        );

        let count = indices.len() as u32;
        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Shared(1),
            vertex_buf: Some(vertex_buf),
            index_buf: Some(index_buf),
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
            },
        );

        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Shared(0),
            vertex_buf: Some(vertex_buf),
            index_buf: Some(index_buf),
            index_format: wgpu::IndexFormat::Uint16,
            count: 6,
            scissor,
            dynamic_offset: Some(offset),
        });
    }

    /// Perspective-correct quad blit via `Transform3D::from_quad`.
    fn blit_quad_inner(&mut self, src: &Texture, q: &[Point; 4], clip: &Rect) {
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
            };
        }
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

        let key = TextureKey::from(src);
        let tex_handle: crate::cache::Handle<CachedTexture> = {
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
            },
        );

        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Owned(bind_group),
            vertex_buf: Some(vertex_buf),
            index_buf: Some(index_buf),
            index_format: wgpu::IndexFormat::Uint16,
            count: 6,
            scissor,
            dynamic_offset: None,
        });
    }

    fn draw_label_inner(&mut self, pos: &Point, text: &[u8], clip: &Rect, color: &Color, opa: u8) {
        if text.is_empty() {
            return;
        }
        if !self.begin_frame() {
            return;
        }
        let scissor = self.scissor_from_clip(clip);
        if scissor[2] == 0 || scissor[3] == 0 {
            return;
        }
        let frame = self.frame.as_mut().expect("frame just initialised");
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing in draw_label");
        let cache = self
            .factory
            .cache
            .as_mut()
            .expect("PipelineCache must be initialised before draw_label");
        let atlas = self
            .factory
            .glyph_atlas
            .as_ref()
            .expect("GlyphAtlas must be initialised before draw_label");

        let cell_w = crate::draw::font::CHAR_W as f32;
        let cell_h = crate::draw::font::CHAR_H as f32;
        let mut verts = alloc::vec::Vec::with_capacity(text.len() * 4);
        let mut indices = alloc::vec::Vec::with_capacity(text.len() * 6);
        let base_x = pos.x.to_f32();
        let base_y = pos.y.to_f32();
        for (i, &ch) in text.iter().enumerate() {
            let x0 = base_x + i as f32 * cell_w;
            let y0 = base_y;
            let x1 = x0 + cell_w;
            let y1 = y0 + cell_h;
            let [u0, v0, u1, v1] = GlyphAtlas::uv_for(ch);
            let v_base = verts.len() as u32;
            verts.push(LabelVertex {
                pos: [x0, y0],
                uv: [u0, v0],
            });
            verts.push(LabelVertex {
                pos: [x1, y0],
                uv: [u1, v0],
            });
            verts.push(LabelVertex {
                pos: [x0, y1],
                uv: [u0, v1],
            });
            verts.push(LabelVertex {
                pos: [x1, y1],
                uv: [u1, v1],
            });
            indices.extend_from_slice(&[
                v_base,
                v_base + 1,
                v_base + 2,
                v_base + 1,
                v_base + 3,
                v_base + 2,
            ]);
        }

        let tint_uniform = PathTintUniform {
            color: [
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0 * opa as f32 / 255.0,
            ],
        };

        let tint_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-label-tint"),
                contents: bytemuck::bytes_of(&tint_uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let vertex_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-label-vertices"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-label-indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        let atlas_view = atlas
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = self
            .factory
            .nearest_sampler
            .as_ref()
            .expect("nearest sampler must be initialised before draw_label");

        let bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-label-bind-group"),
            layout: &cache.label_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: frame.viewport_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: tint_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
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
                shader: ShaderKind::Label,
                format: state.config.format,
            },
        );

        let count = indices.len() as u32;
        frame.ops.push(DrawOp {
            pipeline,
            bind_group: BindGroupRef::Owned(bind_group),
            vertex_buf: Some(vertex_buf),
            index_buf: Some(index_buf),
            index_format: wgpu::IndexFormat::Uint32,
            count,
            scissor,
            dynamic_offset: None,
        });
    }
}

impl Renderer for WgpuRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        use crate::types::TransformClass;

        // Quad short-circuits: render_system already pre-projected the
        // 3D / non-affine widget into 4 corner points, so the GPU only
        // has to draw the resulting quad.
        match cmd {
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
                ..
            } => {
                self.blit_quad_inner(texture, q, clip);
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
                pos, size, texture, ..
            } => {
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                let pos = offset_point(pos, tx, ty);
                self.blit_inner(texture, &src_rect, pos, *size, clip);
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
                path, color, opa, ..
            } => {
                if tx == Fixed::ZERO && ty == Fixed::ZERO {
                    self.fill_path_inner(path, clip, color, *opa);
                } else {
                    let translated = translate_path(path, tx, ty);
                    self.fill_path_inner(&translated, clip, color, *opa);
                }
            }
            DrawCommand::Label {
                pos,
                text,
                color,
                opa,
                ..
            } => {
                let pos = offset_point(pos, tx, ty);
                self.draw_label_inner(&pos, text, clip, color, *opa);
            }
        }
    }

    fn flush(&mut self) {
        let Some(mut frame) = self.frame.take() else {
            return;
        };
        // Empty frames still need a clear or transient backbuffers
        // show stale pixels from the previous swap.
        let load = wgpu::LoadOp::Clear(if frame.ops.is_empty() {
            wgpu::Color::BLACK
        } else {
            wgpu::Color::TRANSPARENT
        });
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
                    pass.set_vertex_buffer(0, vb.slice(..));
                }
                if let Some(ib) = &op.index_buf {
                    pass.set_index_buffer(ib.slice(..), op.index_format);
                    pass.draw_indexed(0..op.count, 0, 0..1);
                } else {
                    pass.draw(0..op.count, 0..1);
                }
            }
        }
        let state = self
            .surface
            .state()
            .expect("WgpuSurface state missing in flush");
        state.queue.submit(Some(frame.encoder.finish()));
        frame.surface_texture.present();
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

    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        self.fill_path_inner(path, clip, color, opa);
    }

    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        self.stroke_path_inner(path, clip, width, color, opa);
    }

    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, dst_size: Point, clip: &Rect) {
        self.blit_inner(src, src_rect, dst, dst_size, clip);
    }

    fn clear(&mut self, area: &Rect, color: &Color) {
        self.fill_rect_inner(area, area, color, Fixed::ZERO, 255);
    }

    fn draw_label(&mut self, pos: &Point, text: &[u8], clip: &Rect, color: &Color, opa: u8) {
        self.draw_label_inner(pos, text, clip, color, opa);
    }

    fn flush(&mut self) {
        Renderer::flush(self)
    }
}
