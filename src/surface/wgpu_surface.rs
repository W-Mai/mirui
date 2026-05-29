//! wgpu-backed Surface.

use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use core::time::Duration;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::window::{Window, WindowId};

use super::{BackbufferPersistence, DisplayInfo, InputEvent, Surface, logical_from_physical};
use crate::cache::InspectCaches;
use crate::draw::texture::ColorFormat;
use crate::types::{Fixed, Rect};

/// Live wgpu state — only present after the first `pump_app_events`
/// has driven `ApplicationHandler::resumed`, which is where winit
/// permits `create_window`.
pub struct WgpuState {
    pub window: Arc<Window>,
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
}

struct WgpuHandler {
    title: String,
    requested_size: (u32, u32),
    state: Option<WgpuState>,
    event_queue: VecDeque<InputEvent>,
}

impl ApplicationHandler for WgpuHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(winit::dpi::PhysicalSize::new(
                self.requested_size.0,
                self.requested_size.1,
            ));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("winit create_window failed"),
        );

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .expect("wgpu create_surface failed");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("wgpu request_adapter failed");

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("mirui-wgpu-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
            ..Default::default()
        }))
        .expect("wgpu request_device failed");

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: alloc::vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        self.state = Some(WgpuState {
            window,
            instance,
            surface,
            adapter,
            device,
            queue,
            config,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.event_queue.push_back(InputEvent::Quit);
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(state) = self.state.as_mut() {
                    state.config.width = new_size.width.max(1);
                    state.config.height = new_size.height.max(1);
                    state.surface.configure(&state.device, &state.config);
                }
            }
            // Other window events remain unmapped in this commit.
            _ => {}
        }
    }
}

pub struct WgpuSurface {
    event_loop: EventLoop<()>,
    handler: WgpuHandler,
}

impl WgpuSurface {
    /// Open a window of the given logical size and stand up the wgpu
    /// device that backs it.
    pub fn new(title: &str, width: u16, height: u16) -> Self {
        let event_loop = EventLoop::new().expect("winit EventLoop::new failed");
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut this = Self {
            event_loop,
            handler: WgpuHandler {
                title: title.to_string(),
                requested_size: (width as u32, height as u32),
                state: None,
                event_queue: VecDeque::new(),
            },
        };

        // winit creates windows from `resumed` only.
        let mut spins = 0;
        while this.handler.state.is_none() && spins < 100 {
            this.pump_once();
            spins += 1;
        }
        if this.handler.state.is_none() {
            panic!("WgpuSurface: winit failed to deliver resumed within {spins} pumps");
        }

        this
    }

    fn pump_once(&mut self) -> winit::platform::pump_events::PumpStatus {
        self.event_loop
            .pump_app_events(Some(Duration::ZERO), &mut self.handler)
    }
}

impl InspectCaches for WgpuSurface {}

impl Surface for WgpuSurface {
    fn display_info(&self) -> DisplayInfo {
        let state = self
            .handler
            .state
            .as_ref()
            .expect("WgpuSurface state must be initialised by new()");
        let size = state.window.inner_size();
        let (lw, lh) = logical_from_physical(size.width as u16, size.height as u16, Fixed::ONE);
        DisplayInfo {
            width: lw,
            height: lh,
            scale: Fixed::ONE,
            format: ColorFormat::RGBA8888,
        }
    }

    fn flush(&mut self, _area: &Rect) {
        todo!()
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        self.pump_once();
        self.handler.event_queue.pop_front()
    }

    fn persistence(&self) -> BackbufferPersistence {
        BackbufferPersistence::Transient
    }
}
