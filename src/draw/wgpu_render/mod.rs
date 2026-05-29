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
use crate::surface::Surface;
use crate::surface::wgpu_surface::WgpuSurface;
use crate::types::{Color, Fixed, Point, Rect, Viewport};

use self::label_atlas::GlyphAtlas;
use self::path::PathTessellator;
use self::pipeline::{
    BlitUniform, LabelVertex, PathTintUniform, PipelineCache, PipelineKey, RectUniform, ShaderKind,
    ViewportUniform,
};
use self::texture_pool::{CachedTexture, TextureKey, TexturePool, new_pool};

pub use self::pipeline::MSAA_SAMPLES;

pub struct WgpuRendererFactory {
    cache: Option<PipelineCache>,
    glyph_atlas: Option<GlyphAtlas>,
    tessellator: PathTessellator,
    texture_pool: TexturePool,
}

impl WgpuRendererFactory {
    pub fn new() -> Self {
        Self {
            cache: None,
            glyph_atlas: None,
            tessellator: PathTessellator::new(),
            texture_pool: new_pool(),
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
        if self.cache.is_none() || self.glyph_atlas.is_none() {
            let state = backend
                .state()
                .expect("WgpuSurface must be initialised before make()");
            if self.cache.is_none() {
                self.cache = Some(PipelineCache::new(&state.device));
            }
            if self.glyph_atlas.is_none() {
                self.glyph_atlas = Some(GlyphAtlas::new(&state.device, &state.queue));
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
    cleared: bool,
    /// Logical viewport size — mirui hands the renderer logical
    /// coordinates and we let NDC do the scaling onto the physical
    /// swapchain texture, so the shader uniforms stay logical too.
    logical_w: f32,
    logical_h: f32,
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
        self.frame = Some(Frame {
            surface_texture,
            swapchain_view,
            msaa_view,
            encoder,
            cleared: false,
            logical_w: state.config.width as f32 / scale,
            logical_h: state.config.height as f32 / scale,
        });
        true
    }

    fn fill_rect_inner(&mut self, area: &Rect, color: &Color, radius: Fixed, opa: u8) {
        if !self.begin_frame() {
            return;
        }
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

        let viewport_uniform = ViewportUniform {
            size: [frame.logical_w, frame.logical_h],
            _pad: [0.0, 0.0],
        };
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

        let viewport_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-fill-viewport"),
                contents: bytemuck::bytes_of(&viewport_uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let rect_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-fill-rect"),
                contents: bytemuck::bytes_of(&rect_uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-fill-bind-group"),
            layout: &cache.fill_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: rect_buf.as_entire_binding(),
                },
            ],
        });

        let pipeline = cache
            .get_or_build(
                &state.device,
                PipelineKey {
                    shader: ShaderKind::Fill,
                    format: state.config.format,
                },
            )
            .clone();

        let load = if frame.cleared {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
        };

        {
            let mut pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mirui-fill-pass"),
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
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..4, 0..1);
        }

        frame.cleared = true;
    }

    fn blit_inner(&mut self, src: &Texture, src_rect: &Rect, dst_pos: Point, dst_size: Point) {
        if !self.begin_frame() {
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
        let tex_view = tex_handle
            .0
            .create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = state.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mirui-blit-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let viewport_uniform = ViewportUniform {
            size: [frame.logical_w, frame.logical_h],
            _pad: [0.0, 0.0],
        };
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

        let viewport_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-blit-viewport"),
                contents: bytemuck::bytes_of(&viewport_uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });
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
                    resource: viewport_buf.as_entire_binding(),
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
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline = cache
            .get_or_build(
                &state.device,
                PipelineKey {
                    shader: ShaderKind::Blit,
                    format: state.config.format,
                },
            )
            .clone();

        let load = if frame.cleared {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
        };

        {
            let mut pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mirui-blit-pass"),
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
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..4, 0..1);
        }

        frame.cleared = true;
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

impl WgpuRenderer<'_> {
    fn fill_path_inner(&mut self, path: &Path, color: &Color, opa: u8) {
        let (verts, indices) = {
            let (v, i) = self.factory.tessellator.fill(path);
            (v.to_vec(), i.to_vec())
        };
        self.draw_path_mesh(&verts, &indices, color, opa);
    }

    fn stroke_path_inner(&mut self, path: &Path, width: Fixed, color: &Color, opa: u8) {
        let (verts, indices) = {
            let (v, i) = self
                .factory
                .tessellator
                .stroke(path, width.to_f32().max(1.0));
            (v.to_vec(), i.to_vec())
        };
        self.draw_path_mesh(&verts, &indices, color, opa);
    }

    fn draw_path_mesh(
        &mut self,
        verts: &[lyon::math::Point],
        indices: &[u32],
        color: &Color,
        opa: u8,
    ) {
        if verts.is_empty() || indices.is_empty() {
            return;
        }
        if !self.begin_frame() {
            return;
        }
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

        let viewport_uniform = ViewportUniform {
            size: [frame.logical_w, frame.logical_h],
            _pad: [0.0, 0.0],
        };
        let tint_uniform = PathTintUniform {
            color: [
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0 * opa as f32 / 255.0,
            ],
        };

        // lyon::math::Point is repr(C) over (f32, f32) — same wire layout
        // as `[f32; 2]`, so cast straight into the vertex buffer without
        // an intermediate copy.
        let vertex_bytes: &[u8] = bytemuck::cast_slice(unsafe {
            core::slice::from_raw_parts(verts.as_ptr() as *const [f32; 2], verts.len())
        });

        let viewport_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-path-viewport"),
                contents: bytemuck::bytes_of(&viewport_uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let tint_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-path-tint"),
                contents: bytemuck::bytes_of(&tint_uniform),
                usage: wgpu::BufferUsages::UNIFORM,
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

        let bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-path-bind-group"),
            layout: &cache.path_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: tint_buf.as_entire_binding(),
                },
            ],
        });

        let pipeline = cache
            .get_or_build(
                &state.device,
                PipelineKey {
                    shader: ShaderKind::Path,
                    format: state.config.format,
                },
            )
            .clone();

        let load = if frame.cleared {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
        };

        {
            let mut pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mirui-path-pass"),
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
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buf.slice(..));
            pass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }

        frame.cleared = true;
    }

    fn draw_label_inner(&mut self, pos: &Point, text: &[u8], color: &Color, opa: u8) {
        if text.is_empty() {
            return;
        }
        if !self.begin_frame() {
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

        let viewport_uniform = ViewportUniform {
            size: [frame.logical_w, frame.logical_h],
            _pad: [0.0, 0.0],
        };
        let tint_uniform = PathTintUniform {
            color: [
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0 * opa as f32 / 255.0,
            ],
        };

        let viewport_buf = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mirui-label-viewport"),
                contents: bytemuck::bytes_of(&viewport_uniform),
                usage: wgpu::BufferUsages::UNIFORM,
            });
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
        let sampler = state.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mirui-label-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-label-bind-group"),
            layout: &cache.label_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buf.as_entire_binding(),
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
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline = cache
            .get_or_build(
                &state.device,
                PipelineKey {
                    shader: ShaderKind::Label,
                    format: state.config.format,
                },
            )
            .clone();

        let load = if frame.cleared {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
        };

        {
            let mut pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mirui-label-pass"),
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
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buf.slice(..));
            pass.set_index_buffer(index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }

        frame.cleared = true;
    }
}

impl Renderer for WgpuRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, _clip: &Rect) {
        match cmd {
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => self.fill_rect_inner(area, color, *radius, *opa),
            DrawCommand::Blit {
                pos, size, texture, ..
            } => {
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                self.blit_inner(texture, &src_rect, *pos, *size);
            }
            DrawCommand::Border {
                area,
                width,
                radius,
                color,
                opa,
                ..
            } => {
                let half = *width / 2;
                let path = Path::rounded_rect(
                    area.x + half,
                    area.y + half,
                    area.w - *width,
                    area.h - *width,
                    *radius,
                );
                self.stroke_path_inner(&path, *width, color, *opa);
            }
            DrawCommand::Line {
                p1,
                p2,
                width,
                color,
                opa,
                ..
            } => {
                let mut path = Path::new();
                path.move_to(*p1).line_to(*p2);
                self.stroke_path_inner(&path, *width, color, *opa);
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
                let path = Path::arc(*center, *radius, *start_angle, *end_angle);
                self.stroke_path_inner(&path, *width, color, *opa);
            }
            DrawCommand::FillPath {
                path, color, opa, ..
            } => {
                self.fill_path_inner(path, color, *opa);
            }
            DrawCommand::Label {
                pos,
                text,
                color,
                opa,
                ..
            } => {
                self.draw_label_inner(pos, text, color, *opa);
            }
        }
    }

    fn flush(&mut self) {
        let Some(mut frame) = self.frame.take() else {
            return;
        };
        // Force a clear-only pass when nothing was drawn so the
        // swapchain image isn't garbage on transient backbuffers.
        if !frame.cleared {
            let _ = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mirui-empty-clear-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame.msaa_view,
                        resolve_target: Some(&frame.swapchain_view),
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
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
    fn fill_rect(&mut self, area: &Rect, _clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        self.fill_rect_inner(area, color, radius, opa);
    }

    fn fill_path(&mut self, path: &Path, _clip: &Rect, color: &Color, opa: u8) {
        self.fill_path_inner(path, color, opa);
    }

    fn stroke_path(&mut self, path: &Path, _clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        self.stroke_path_inner(path, width, color, opa);
    }

    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, dst_size: Point, _clip: &Rect) {
        self.blit_inner(src, src_rect, dst, dst_size);
    }

    fn clear(&mut self, _area: &Rect, color: &Color) {
        let info = self.surface.display_info();
        let area = Rect::new(0, 0, info.width, info.height);
        self.fill_rect_inner(&area, color, Fixed::ZERO, 255);
    }

    fn draw_label(&mut self, pos: &Point, text: &[u8], _clip: &Rect, color: &Color, opa: u8) {
        self.draw_label_inner(pos, text, color, opa);
    }

    fn flush(&mut self) {
        Renderer::flush(self)
    }
}
