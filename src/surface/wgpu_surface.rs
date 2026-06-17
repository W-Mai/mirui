//! wgpu-backed Surface. Wraps a winit window driven by
//! `pump_app_events` so mirui's polling main loop stays untouched.

use alloc::collections::VecDeque;
use alloc::string::{String, ToString};
use core::time::Duration;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, KeyEvent, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::window::{Window, WindowId};

use super::{BackbufferPersistence, DisplayInfo, InputEvent, Surface, logical_from_physical};
use crate::cache::InspectCaches;
use crate::render::texture::ColorFormat;
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
    /// MSAA color attachment; render passes draw here and resolve to
    /// the swapchain texture. Recreated on resize.
    pub msaa: wgpu::Texture,
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
    /// `Some` while a `PointerMove` is queued for emission this pump
    /// cycle. Only the latest position is sent — winit can fire
    /// CursorMoved 100+ times per gesture and dispatch_input is too
    /// expensive to walk that on every event.
    pending_move: Option<(Fixed, Fixed)>,
}

impl WgpuHandler {
    /// winit hands every coordinate as `PhysicalPosition` (device
    /// pixels). mirui hit-tests in logical points; divide by the
    /// integer-rounded `scale_factor` to bridge.
    fn to_logical(&self, x: f64, y: f64) -> (Fixed, Fixed) {
        let scale = self
            .state
            .as_ref()
            .map(|s| s.window.scale_factor().round().max(1.0))
            .unwrap_or(1.0);
        (
            Fixed::from((x / scale) as i32),
            Fixed::from((y / scale) as i32),
        )
    }
}

impl ApplicationHandler for WgpuHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
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
            // Mailbox > Immediate > Fifo: present without vsync stall when
            // the driver supports it; Fifo is the universal fallback.
            present_mode: if surface_caps
                .present_modes
                .contains(&wgpu::PresentMode::Mailbox)
            {
                wgpu::PresentMode::Mailbox
            } else if surface_caps
                .present_modes
                .contains(&wgpu::PresentMode::Immediate)
            {
                wgpu::PresentMode::Immediate
            } else {
                wgpu::PresentMode::Fifo
            },
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: alloc::vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let msaa = create_msaa(&device, &config);

        self.state = Some(WgpuState {
            window,
            instance,
            surface,
            adapter,
            device,
            queue,
            config,
            msaa,
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
                    state.msaa = create_msaa(&state.device, &state.config);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = self.to_logical(position.x, position.y);
                self.last_cursor = (x, y);
                self.pending_move = Some((x, y));
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
                // mirui wheel units are line ticks; PixelDelta is logical points.
                // 16 logical points matches macOS default line height.
                // Keep right-swipe positive in mirui coordinates.
                const PX_PER_LINE: f32 = 16.0;
                let scale = self
                    .state
                    .as_ref()
                    .map(|s| s.window.scale_factor().round().max(1.0))
                    .unwrap_or(1.0) as f32;
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (Fixed::from_f32(-x), Fixed::from_f32(y)),
                    MouseScrollDelta::PixelDelta(p) => {
                        let lx = (p.x as f32) / scale;
                        let ly = (p.y as f32) / scale;
                        (
                            Fixed::from_f32(-lx / PX_PER_LINE),
                            Fixed::from_f32(ly / PX_PER_LINE),
                        )
                    }
                };
                let (x, y) = self.last_cursor;
                self.event_queue
                    .push_back(InputEvent::Wheel { dx, dy, x, y });
            }
            WindowEvent::Touch(touch) => {
                let (x, y) = self.to_logical(touch.location.x, touch.location.y);
                let id = (touch.id & 0xff) as u8;
                let event = match touch.phase {
                    TouchPhase::Started => InputEvent::PointerDown { id, x, y },
                    TouchPhase::Moved => InputEvent::PointerMove { id, x, y },
                    TouchPhase::Ended | TouchPhase::Cancelled => InputEvent::PointerUp { id, x, y },
                };
                self.event_queue.push_back(event);
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        text,
                        state,
                        ..
                    },
                ..
            } => {
                use crate::event::input::*;
                if state != ElementState::Pressed {
                    return;
                }
                let code = match &logical_key {
                    Key::Named(NamedKey::Backspace) => Some(KEY_BACKSPACE),
                    Key::Named(NamedKey::Delete) => Some(KEY_DELETE),
                    Key::Named(NamedKey::ArrowLeft) => Some(KEY_LEFT),
                    Key::Named(NamedKey::ArrowRight) => Some(KEY_RIGHT),
                    Key::Named(NamedKey::Home) => Some(KEY_HOME),
                    Key::Named(NamedKey::End) => Some(KEY_END),
                    Key::Named(NamedKey::Enter) => Some(KEY_RETURN),
                    Key::Named(NamedKey::Escape) => {
                        self.event_queue.push_back(InputEvent::Quit);
                        event_loop.exit();
                        return;
                    }
                    _ => None,
                };
                if let Some(code) = code {
                    self.event_queue.push_back(InputEvent::Key {
                        code,
                        pressed: true,
                    });
                }
                // Printable text input — mirui currently consumes the first produced char.
                if let Some(s) = text.as_ref() {
                    if let Some(ch) = s.chars().next() {
                        if !ch.is_control() {
                            self.event_queue.push_back(InputEvent::CharInput { ch });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub struct WgpuSurface {
    event_loop: EventLoop<()>,
    handler: WgpuHandler,
    /// macOS `pump_app_events(Duration::ZERO)` costs ~6 ms per call
    /// (it spins NSApp internally even with no events). mirui calls
    /// `poll_event` until `None` every frame, so without this flag a
    /// frame with N events would pump N+1 times = 6N ms of overhead.
    /// Pump once per frame; `Surface::flush` resets the latch.
    pumped_this_frame: bool,
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
                pending_move: None,
            },
            pumped_this_frame: false,
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

fn create_msaa(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mirui-wgpu-msaa"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: crate::render::wgpu::MSAA_SAMPLES,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
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
        let scale_int = state.window.scale_factor().round().max(1.0) as u16;
        let scale = Fixed::from(scale_int);
        let (lw, lh) = logical_from_physical(size.width as u16, size.height as u16, scale);
        DisplayInfo {
            width: lw,
            height: lh,
            scale,
            format: ColorFormat::RGBA8888,
        }
    }

    fn flush(&mut self, _area: &Rect) {
        // Frame boundary: re-arm `poll_event` to pump once next tick.
        // Transient backends like wgpu hit `flush` every frame but
        // never `begin_flush`, so the latch lives here.
        //
        // wgpu present itself happens inside `WgpuRenderer::flush`
        // (the SurfaceTexture lives on the renderer's frame state)
        // — this method only owns the per-frame latch reset.
        self.pumped_this_frame = false;
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if let Some(e) = self.handler.event_queue.pop_front() {
            return Some(e);
        }
        if self.pumped_this_frame {
            return None;
        }
        self.pumped_this_frame = true;

        self.pump_once();
        if let Some((x, y)) = self.handler.pending_move.take() {
            self.handler
                .event_queue
                .push_back(InputEvent::PointerMove { id: 0, x, y });
        }
        self.handler.event_queue.pop_front()
    }

    fn persistence(&self) -> BackbufferPersistence {
        BackbufferPersistence::Transient
    }
}
