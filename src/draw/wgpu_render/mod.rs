//! wgpu-backed Renderer + Canvas.

mod pipeline;

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

use self::pipeline::{
    BlitUniform, PipelineCache, PipelineKey, RectUniform, ShaderKind, ViewportUniform,
};

pub struct WgpuRendererFactory {
    cache: Option<PipelineCache>,
}

impl WgpuRendererFactory {
    pub fn new() -> Self {
        Self { cache: None }
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
        _transform: &Viewport,
    ) -> WgpuRenderer<'a> {
        if self.cache.is_none() {
            let state = backend
                .state()
                .expect("WgpuSurface must be initialised before make()");
            self.cache = Some(PipelineCache::new(&state.device));
        }
        WgpuRenderer {
            factory: self,
            surface: backend,
            frame: None,
        }
    }
}

pub struct WgpuRenderer<'a> {
    factory: &'a mut WgpuRendererFactory,
    surface: &'a mut WgpuSurface,
    frame: Option<Frame>,
}

struct Frame {
    surface_texture: wgpu::SurfaceTexture,
    view: wgpu::TextureView,
    encoder: wgpu::CommandEncoder,
    cleared: bool,
    width: f32,
    height: f32,
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
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mirui-wgpu-encoder"),
            });
        self.frame = Some(Frame {
            surface_texture,
            view,
            encoder,
            cleared: false,
            width: state.config.width as f32,
            height: state.config.height as f32,
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
            size: [frame.width, frame.height],
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
                        view: &frame.view,
                        depth_slice: None,
                        resolve_target: None,
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

        let rgba = match texture_to_rgba8(src) {
            Some(buf) => buf,
            None => return,
        };

        let tex = state.device.create_texture_with_data(
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
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &rgba,
        );
        let tex_view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = state.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mirui-blit-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let viewport_uniform = ViewportUniform {
            size: [frame.width, frame.height],
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
                        view: &frame.view,
                        depth_slice: None,
                        resolve_target: None,
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
            // Path / label commands route through their own dispatch.
            _ => {}
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
                        view: &frame.view,
                        depth_slice: None,
                        resolve_target: None,
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

    fn fill_path(&mut self, _path: &Path, _clip: &Rect, _color: &Color, _opa: u8) {}

    fn stroke_path(&mut self, _path: &Path, _clip: &Rect, _width: Fixed, _color: &Color, _opa: u8) {
    }

    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, dst_size: Point, _clip: &Rect) {
        self.blit_inner(src, src_rect, dst, dst_size);
    }

    fn clear(&mut self, _area: &Rect, color: &Color) {
        let info = self.surface.display_info();
        let area = Rect::new(0, 0, info.width, info.height);
        self.fill_rect_inner(&area, color, Fixed::ZERO, 255);
    }

    fn draw_label(&mut self, _pos: &Point, _text: &[u8], _clip: &Rect, _color: &Color, _opa: u8) {}

    fn flush(&mut self) {
        Renderer::flush(self)
    }
}
