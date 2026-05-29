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

use self::pipeline::{PipelineCache, PipelineKey, RectUniform, ShaderKind, ViewportUniform};

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
}

impl Renderer for WgpuRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, _clip: &Rect) {
        if let DrawCommand::Fill {
            area,
            color,
            radius,
            opa,
            ..
        } = cmd
        {
            self.fill_rect_inner(area, color, *radius, *opa);
        }
        // Path / blit / label commands route through their own dispatch.
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

    fn blit(
        &mut self,
        _src: &Texture,
        _src_rect: &Rect,
        _dst: Point,
        _dst_size: Point,
        _clip: &Rect,
    ) {
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
