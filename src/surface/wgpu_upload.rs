//! Software rasterization presented through a mobile WGPU swapchain.

use alloc::sync::Arc;
use alloc::vec::Vec;

use winit::event::WindowEvent;
use winit::window::Window;

pub use super::wgpu_surface::SoftwareRenderScale;
use super::wgpu_surface::{MobileHost, MobileSurface, MobileSurfaceError, WgpuRuntime, WgpuState};
use super::{
    BackbufferPersistence, DisplayInfo, FramebufferAccess, InputEvent, SafeAreaInsets, Surface,
};
use crate::core::cache::InspectCaches;
use crate::render::factory::{RendererFactory, SwRendererFactory};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Fixed, PhysicalRect};

const PRESENT_SHADER: &str = r#"
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    let uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(1.0, 0.0),
    );
    var out: VertexOut;
    out.position = vec4<f32>(positions[index], 0.0, 1.0);
    out.uv = uvs[index];
    return out;
}

@group(0) @binding(0) var present_texture: texture_2d<f32>;
@group(0) @binding(1) var present_sampler: sampler;

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(present_texture, present_sampler, in.uv);
}
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoftwareUploadConfig {
    /// Resolution policy for the retained software framebuffer.
    pub render_scale: SoftwareRenderScale,
    /// Maximum RGBA framebuffer storage admitted by this surface.
    pub framebuffer_budget_bytes: usize,
}

impl Default for SoftwareUploadConfig {
    fn default() -> Self {
        Self {
            render_scale: SoftwareRenderScale::Device,
            framebuffer_budget_bytes: 32 * 1024 * 1024,
        }
    }
}

struct Presenter {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl Presenter {
    fn new(state: &WgpuState, width: u16, height: u16, exact_size: bool) -> Self {
        let texture = state.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirui-mobile-sw-framebuffer"),
            size: wgpu::Extent3d {
                width: u32::from(width),
                height: u32::from(height),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = state.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mirui-mobile-sw-sampler"),
            mag_filter: if exact_size {
                wgpu::FilterMode::Nearest
            } else {
                wgpu::FilterMode::Linear
            },
            min_filter: if exact_size {
                wgpu::FilterMode::Nearest
            } else {
                wgpu::FilterMode::Linear
            },
            ..Default::default()
        });
        let bind_group_layout =
            state
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("mirui-mobile-sw-bind-group-layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });
        let bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mirui-mobile-sw-bind-group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = state
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("mirui-mobile-sw-present-shader"),
                source: wgpu::ShaderSource::Wgsl(alloc::borrow::Cow::Borrowed(PRESENT_SHADER)),
            });
        let pipeline_layout =
            state
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("mirui-mobile-sw-pipeline-layout"),
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });
        let pipeline = state
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mirui-mobile-sw-pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: state.config.format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        Self {
            texture,
            bind_group,
            pipeline,
        }
    }
}

pub struct SoftwareUploadSurface {
    runtime: WgpuRuntime,
    config: SoftwareUploadConfig,
    buffer: Vec<u8>,
    width: u16,
    height: u16,
    render_scale: Fixed,
    presenter: Option<Presenter>,
    pending_present: bool,
}

impl SoftwareUploadSurface {
    pub fn new(
        window: Arc<Window>,
        config: SoftwareUploadConfig,
    ) -> Result<Self, MobileSurfaceError> {
        let mut runtime = WgpuRuntime::new();
        runtime.resume(window);
        let mut surface = Self {
            runtime,
            config,
            buffer: Vec::new(),
            width: 0,
            height: 0,
            render_scale: Fixed::ONE,
            presenter: None,
            pending_present: false,
        };
        surface.resize_storage()?;
        Ok(surface)
    }

    fn resize_storage(&mut self) -> Result<(), MobileSurfaceError> {
        let state = self
            .runtime
            .state
            .as_ref()
            .expect("software upload surface is not resumed");
        let window_size = state.window.inner_size();
        let device_scale = Fixed::from_f32(state.window.scale_factor() as f32);
        let logical_width =
            Fixed::from_int(i32::try_from(window_size.width).unwrap_or(i32::MAX)) / device_scale;
        let logical_height =
            Fixed::from_int(i32::try_from(window_size.height).unwrap_or(i32::MAX)) / device_scale;
        let (render_scale, width, height, required) =
            super::wgpu_surface::resolve_software_buffer_layout(
                logical_width,
                logical_height,
                device_scale,
                self.config.render_scale,
                self.config.framebuffer_budget_bytes,
            )?;
        self.render_scale = render_scale;
        if super::wgpu_surface::software_presenter_needs_rebuild(
            self.width,
            self.height,
            width,
            height,
            self.presenter.is_some(),
        ) {
            self.buffer.resize(required, 0);
            self.width = width;
            self.height = height;
            let exact_size =
                u32::from(width) == state.config.width && u32::from(height) == state.config.height;
            self.presenter = Some(Presenter::new(state, width, height, exact_size));
            self.pending_present = true;
        }
        Ok(())
    }

    fn upload(&mut self, area: PhysicalRect) {
        let Some(region) =
            super::wgpu_surface::software_upload_region(self.width, self.height, area)
        else {
            return;
        };
        let Some(state) = self.runtime.state.as_ref() else {
            return;
        };
        let Some(presenter) = self.presenter.as_ref() else {
            return;
        };
        state.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &presenter.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: u32::from(area.x()),
                    y: u32::from(area.y()),
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &self.buffer[region.offset..],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(region.bytes_per_row),
                rows_per_image: Some(region.height),
            },
            wgpu::Extent3d {
                width: region.width,
                height: region.height,
                depth_or_array_layers: 1,
            },
        );
        self.pending_present = true;
    }

    fn present(&mut self) {
        if !self.pending_present {
            return;
        }
        let Some(state) = self.runtime.state.as_ref() else {
            return;
        };
        let Some(presenter) = self.presenter.as_ref() else {
            return;
        };
        let surface_texture = match state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            _ => return,
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mirui-mobile-sw-present-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mirui-mobile-sw-present-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
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
            pass.set_pipeline(&presenter.pipeline);
            pass.set_bind_group(0, &presenter.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        state.queue.submit(Some(encoder.finish()));
        surface_texture.present();
        self.pending_present = false;
    }
}

impl InspectCaches for SoftwareUploadSurface {}

impl MobileSurface for SoftwareUploadSurface {
    fn resume_window(&mut self, window: Arc<Window>) -> Result<(), MobileSurfaceError> {
        self.runtime.resume(window);
        self.presenter = None;
        self.resize_storage()
    }

    fn suspend_window(&mut self) {
        self.runtime.suspend();
        self.presenter = None;
        self.pending_present = false;
    }

    fn handle_window_event(&mut self, event: WindowEvent) -> bool {
        let resized = matches!(
            event,
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
        );
        let exit = self.runtime.window_event(event);
        if resized && let Err(error) = self.resize_storage() {
            crate::warn!("software upload resize failed: {:?}", error);
        }
        exit
    }
}

impl Surface for SoftwareUploadSurface {
    fn display_info(&self) -> DisplayInfo {
        let (width, height) =
            super::logical_from_physical(self.width, self.height, self.render_scale);
        DisplayInfo {
            width,
            height,
            scale: self.render_scale,
            format: ColorFormat::RGBA8888,
        }
    }

    fn physical_size(&self) -> (u32, u32) {
        (u32::from(self.width), u32::from(self.height))
    }

    fn safe_area_insets(&self) -> SafeAreaInsets {
        self.runtime
            .state
            .as_ref()
            .map_or_else(SafeAreaInsets::default, |state| {
                super::wgpu_surface::mobile_window_safe_area(&state.window)
            })
    }

    fn flush(&mut self, area: PhysicalRect) {
        self.upload(area);
    }

    fn end_flush(&mut self) {
        self.present();
    }

    fn frame_end(&mut self) {
        self.present();
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if let Some((x, y)) = self.runtime.pending_move.take() {
            self.runtime
                .event_queue
                .push_back(InputEvent::PointerMove { id: 0, x, y });
        }
        self.runtime.event_queue.pop_front()
    }

    fn persistence(&self) -> BackbufferPersistence {
        BackbufferPersistence::Persistent
    }
}

impl FramebufferAccess for SoftwareUploadSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        Texture::new(
            &mut self.buffer,
            self.width,
            self.height,
            ColorFormat::RGBA8888,
        )
    }
}

pub fn software_mobile_host<Build>(
    title: impl Into<alloc::string::String>,
    config: SoftwareUploadConfig,
    build: Build,
) -> MobileHost<
    SoftwareUploadSurface,
    SwRendererFactory,
    impl FnOnce(Arc<Window>) -> Result<SoftwareUploadSurface, MobileSurfaceError>,
    Build,
>
where
    Build: FnOnce(SoftwareUploadSurface) -> crate::app::App<SoftwareUploadSurface> + 'static,
{
    MobileHost::new(
        title,
        move |window| SoftwareUploadSurface::new(window, config),
        build,
    )
}

const _: fn() = || {
    fn assert_factory<B: Surface, F: RendererFactory<B>>() {}
    assert_factory::<SoftwareUploadSurface, SwRendererFactory>();
};
