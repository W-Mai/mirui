//! wgpu-backed Surface. Wraps a winit window driven by
//! `pump_app_events` so mirui's polling main loop stays untouched.

use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use core::time::Duration;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
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
    /// Last known cursor position, in logical pixels. Updated on
    /// every `CursorMoved` so `MouseInput` (which doesn't carry a
    /// position in winit 0.30) can attach one to the synthetic
    /// `PointerDown`/`PointerUp`.
    last_cursor: (Fixed, Fixed),
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
        // Prefer unorm: mirui Color values are already sRGB-encoded
        // bytes, and the renderer composites in that space. Picking
        // an sRGB swap-chain format would re-encode and wash colours
        // out by ~2× luminance.
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
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
            WindowEvent::CursorMoved { position, .. } => {
                let x = Fixed::from(position.x as i32);
                let y = Fixed::from(position.y as i32);
                self.last_cursor = (x, y);
                self.event_queue
                    .push_back(InputEvent::PointerMove { id: 0, x, y });
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button != MouseButton::Left {
                    return;
                }
                let (x, y) = self.last_cursor;
                let event = match state {
                    ElementState::Pressed => InputEvent::PointerDown { id: 0, x, y },
                    ElementState::Released => InputEvent::PointerUp { id: 0, x, y },
                };
                self.event_queue.push_back(event);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        // 16 logical px per line tick.
                        (
                            Fixed::from((x * 16.0) as i32),
                            Fixed::from((y * 16.0) as i32),
                        )
                    }
                    MouseScrollDelta::PixelDelta(p) => {
                        (Fixed::from(p.x as i32), Fixed::from(p.y as i32))
                    }
                };
                let (x, y) = self.last_cursor;
                self.event_queue
                    .push_back(InputEvent::Wheel { dx, dy, x, y });
            }
            WindowEvent::Touch(touch) => {
                let x = Fixed::from(touch.location.x as i32);
                let y = Fixed::from(touch.location.y as i32);
                let id = (touch.id & 0xff) as u8;
                let event = match touch.phase {
                    TouchPhase::Started => InputEvent::PointerDown { id, x, y },
                    TouchPhase::Moved => InputEvent::PointerMove { id, x, y },
                    TouchPhase::Ended | TouchPhase::Cancelled => InputEvent::PointerUp { id, x, y },
                };
                self.event_queue.push_back(event);
            }
            _ => {}
        }
    }
}

pub struct WgpuSurface {
    event_loop: EventLoop<()>,
    handler: WgpuHandler,
}

impl WgpuSurface {
    /// `None` only between `WgpuSurface::new` constructing the struct
    /// and `resumed` populating the wgpu device.
    pub fn state(&self) -> Option<&WgpuState> {
        self.handler.state.as_ref()
    }

    pub fn state_mut(&mut self) -> Option<&mut WgpuState> {
        self.handler.state.as_mut()
    }

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
                last_cursor: (Fixed::ZERO, Fixed::ZERO),
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
            // The surface format is sRGB BGRA / RGBA — mirui's `Texture`
            format: ColorFormat::RGBA8888,
        }
    }

    fn flush(&mut self, _area: &Rect) {
        // wgpu present happens inside `WgpuRenderer::flush` (the
        // SurfaceTexture lives on the renderer's frame state). The
        // backend-side flush is a no-op so the App tick order stays
        // identical to other backends.
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        // `window_event` already pushes (Quit on CloseRequested).
        self.pump_once();
        self.handler.event_queue.pop_front()
    }

    fn persistence(&self) -> BackbufferPersistence {
        BackbufferPersistence::Transient
    }
}
