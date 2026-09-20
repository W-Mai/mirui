//! WGPU surfaces for desktop polling loops and native mobile event loops.

use alloc::collections::VecDeque;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use alloc::string::{String, ToString};
use core::ops::Deref;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use core::time::Duration;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::window::{Window, WindowId};

#[cfg(any(target_os = "android", target_os = "ios", test))]
use super::SafeAreaInsets;
use super::{BackbufferPersistence, DisplayInfo, InputEvent, Surface, logical_from_physical};
use crate::core::cache::InspectCaches;
use crate::render::texture::ColorFormat;
use crate::types::Fixed;

const MAX_NATIVE_TOUCHES: usize = 16;

struct NativeTouchMap<const N: usize> {
    native_ids: [u64; N],
    occupied: u32,
}

impl<const N: usize> NativeTouchMap<N> {
    const fn new() -> Self {
        Self {
            native_ids: [0; N],
            occupied: 0,
        }
    }

    fn clear(&mut self) {
        self.occupied = 0;
    }

    fn resolve(&self, native_id: u64) -> Option<usize> {
        self.native_ids
            .iter()
            .enumerate()
            .position(|(slot, candidate)| self.is_occupied(slot) && *candidate == native_id)
    }

    fn start(&mut self, native_id: u64) -> Option<usize> {
        if let Some(slot) = self.resolve(native_id) {
            return Some(slot);
        }
        let slot = (0..N).find(|slot| !self.is_occupied(*slot))?;
        self.native_ids[slot] = native_id;
        self.occupied |= 1_u32.checked_shl(slot as u32)?;
        Some(slot)
    }

    fn is_occupied(&self, slot: usize) -> bool {
        1_u32
            .checked_shl(slot as u32)
            .is_some_and(|mask| self.occupied & mask != 0)
    }

    fn input_event(
        &mut self,
        native_id: u64,
        phase: TouchPhase,
        x: Fixed,
        y: Fixed,
    ) -> Option<InputEvent> {
        let slot = match phase {
            TouchPhase::Started => self.start(native_id)?,
            TouchPhase::Moved | TouchPhase::Ended | TouchPhase::Cancelled => {
                self.resolve(native_id)?
            }
        };
        let id = u8::try_from(slot).ok()?;
        let event = match phase {
            TouchPhase::Started => InputEvent::PointerDown { id, x, y },
            TouchPhase::Moved => InputEvent::PointerMove { id, x, y },
            TouchPhase::Ended | TouchPhase::Cancelled => InputEvent::PointerUp { id, x, y },
        };
        if matches!(phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            self.occupied &= !1_u32.checked_shl(slot as u32).unwrap_or(0);
        }
        Some(event)
    }
}

fn physical_to_logical(x: f64, y: f64, scale: f64) -> (Fixed, Fixed) {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    (
        Fixed::from_f32((x / scale) as f32),
        Fixed::from_f32((y / scale) as f32),
    )
}

fn enqueue_text(queue: &mut VecDeque<InputEvent>, text: &str) {
    queue.extend(
        text.chars()
            .filter(|ch| !ch.is_control())
            .map(|ch| InputEvent::CharInput { ch }),
    );
}

#[cfg(target_os = "android")]
pub(crate) fn mobile_window_safe_area(window: &Window) -> SafeAreaInsets {
    use winit::platform::android::WindowExtAndroid;

    let content = window.content_rect();
    if content.right <= content.left || content.bottom <= content.top {
        return SafeAreaInsets::default();
    }
    let size = mobile_window_drawable_size(window);
    safe_area_from_physical(
        size.width,
        size.height,
        Fixed::from_f32(window.scale_factor() as f32),
        content.left,
        content.top,
        content.right,
        content.bottom,
    )
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
fn safe_area_from_physical(
    width: u32,
    height: u32,
    scale: Fixed,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) -> SafeAreaInsets {
    let width = i32::try_from(width).unwrap_or(i32::MAX);
    let height = i32::try_from(height).unwrap_or(i32::MAX);
    let scale = scale.max(Fixed::from_ratio(1, 4));
    SafeAreaInsets {
        top: Fixed::from_int(top.max(0)) / scale,
        right: Fixed::from_int(width.saturating_sub(right).max(0)) / scale,
        bottom: Fixed::from_int(height.saturating_sub(bottom).max(0)) / scale,
        left: Fixed::from_int(left.max(0)) / scale,
    }
}

#[cfg(target_os = "ios")]
pub(crate) fn mobile_window_safe_area(window: &Window) -> SafeAreaInsets {
    let outer = mobile_window_drawable_size(window);
    let inner = window.inner_size();
    let position = window.inner_position().unwrap_or_default();
    safe_area_from_physical(
        outer.width,
        outer.height,
        Fixed::from_f32(window.scale_factor() as f32),
        position.x,
        position.y,
        position
            .x
            .saturating_add(i32::try_from(inner.width).unwrap_or(i32::MAX)),
        position
            .y
            .saturating_add(i32::try_from(inner.height).unwrap_or(i32::MAX)),
    )
}

pub(crate) fn mobile_window_drawable_size(window: &Window) -> winit::dpi::PhysicalSize<u32> {
    #[cfg(target_os = "ios")]
    {
        window.outer_size()
    }
    #[cfg(not(target_os = "ios"))]
    {
        window.inner_size()
    }
}

/// Live wgpu state — only present after the first `pump_app_events`
/// has driven `ApplicationHandler::resumed`, which is where winit
/// permits `create_window`.
pub struct WgpuDeviceContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

pub struct WgpuState {
    context: Arc<WgpuDeviceContext>,
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    /// Multisampled color attachment. Mobile targets omit it when their
    /// render pipelines use a single sample.
    pub msaa: Option<wgpu::Texture>,
}

impl Deref for WgpuState {
    type Target = WgpuDeviceContext;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

/// Surface capability required by the WGPU renderer.
///
/// Event-loop ownership is intentionally outside this trait so desktop and
/// platform-owned mobile loops can share the same renderer.
pub trait WgpuTarget: Surface {
    fn state(&self) -> Option<&WgpuState>;

    fn state_mut(&mut self) -> Option<&mut WgpuState>;

    fn acquire_surface_texture(&mut self) -> Option<wgpu::SurfaceTexture> {
        match self.state()?.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Some(texture),
            _ => None,
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
struct WgpuHandler {
    title: String,
    requested_size: (u32, u32),
    runtime: WgpuRuntime,
}

pub(crate) struct WgpuRuntime {
    context: Option<Arc<WgpuDeviceContext>>,
    pub(crate) state: Option<WgpuState>,
    pub(crate) event_queue: VecDeque<InputEvent>,
    /// Last known cursor position, in logical pixels. Updated on
    /// every `CursorMoved` so `MouseInput` (which doesn't carry a
    /// position in winit 0.30) can attach one to the synthetic
    /// `PointerDown`/`PointerUp`.
    pub(crate) last_cursor: (Fixed, Fixed),
    /// `Some` while a `PointerMove` is queued for emission this pump
    /// cycle. Only the latest position is sent — winit can fire
    /// CursorMoved 100+ times per gesture and dispatch_input is too
    /// expensive to walk that on every event.
    pub(crate) pending_move: Option<(Fixed, Fixed)>,
    native_touches: NativeTouchMap<MAX_NATIVE_TOUCHES>,
    reconfigure_before_acquire: bool,
}

impl WgpuRuntime {
    pub(crate) fn new() -> Self {
        Self {
            context: None,
            state: None,
            event_queue: VecDeque::new(),
            last_cursor: (Fixed::ZERO, Fixed::ZERO),
            pending_move: None,
            native_touches: NativeTouchMap::new(),
            reconfigure_before_acquire: false,
        }
    }

    pub(crate) fn resume(&mut self, window: Arc<Window>) {
        if self.state.is_some() {
            return;
        }
        let state = match self.context.as_ref() {
            Some(context) => create_wgpu_state_with_context(context.clone(), window),
            None => create_wgpu_state(window),
        };
        self.context = Some(state.context.clone());
        self.state = Some(state);
    }

    pub(crate) fn suspend(&mut self) {
        self.state = None;
        self.event_queue.clear();
        self.pending_move = None;
        self.native_touches.clear();
        self.reconfigure_before_acquire = false;
    }

    fn reconfigure_surface(&mut self) -> bool {
        let Some(state) = self.state.as_mut() else {
            return false;
        };
        let size = mobile_window_drawable_size(&state.window);
        state.config.width = size.width.max(1);
        state.config.height = size.height.max(1);
        state.surface.configure(&state.device, &state.config);
        state.msaa = create_msaa(&state.device, &state.config);
        self.reconfigure_before_acquire = false;
        true
    }

    fn recreate_surface(&mut self) -> bool {
        let Some(previous) = self.state.take() else {
            return false;
        };
        let context = previous.context.clone();
        let window = previous.window.clone();
        drop(previous);
        self.state = Some(create_wgpu_state_with_context(context, window));
        self.reconfigure_before_acquire = false;
        true
    }

    pub(crate) fn acquire_surface_texture(&mut self) -> Option<wgpu::SurfaceTexture> {
        if self.reconfigure_before_acquire && !self.reconfigure_surface() {
            return None;
        }
        let mut retried = false;
        loop {
            let acquired = self.state.as_ref()?.surface.get_current_texture();
            match acquired {
                wgpu::CurrentSurfaceTexture::Success(texture) => return Some(texture),
                wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                    self.reconfigure_before_acquire = true;
                    return Some(texture);
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return None;
                }
                wgpu::CurrentSurfaceTexture::Outdated if !retried => {
                    retried = true;
                    if !self.reconfigure_surface() {
                        return None;
                    }
                }
                wgpu::CurrentSurfaceTexture::Lost if !retried => {
                    retried = true;
                    if !self.recreate_surface() {
                        return None;
                    }
                }
                wgpu::CurrentSurfaceTexture::Outdated
                | wgpu::CurrentSurfaceTexture::Lost
                | wgpu::CurrentSurfaceTexture::Validation => return None,
            }
        }
    }

    /// winit hands every coordinate as `PhysicalPosition` (device
    /// pixels). mirui hit-tests in logical points.
    fn to_logical(&self, x: f64, y: f64) -> (Fixed, Fixed) {
        let scale = self
            .state
            .as_ref()
            .map(|s| s.window.scale_factor())
            .unwrap_or(1.0);
        physical_to_logical(x, y, scale)
    }

    pub(crate) fn window_event(&mut self, event: WindowEvent) -> bool {
        match event {
            WindowEvent::CloseRequested => {
                self.event_queue.push_back(InputEvent::Quit);
                true
            }
            WindowEvent::Resized(new_size) => {
                if let Some(state) = self.state.as_mut() {
                    state.config.width = new_size.width.max(1);
                    state.config.height = new_size.height.max(1);
                    state.surface.configure(&state.device, &state.config);
                    state.msaa = create_msaa(&state.device, &state.config);
                }
                self.reconfigure_before_acquire = false;
                false
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(state) = self.state.as_mut() {
                    let new_size = mobile_window_drawable_size(&state.window);
                    state.config.width = new_size.width.max(1);
                    state.config.height = new_size.height.max(1);
                    state.surface.configure(&state.device, &state.config);
                    state.msaa = create_msaa(&state.device, &state.config);
                }
                self.reconfigure_before_acquire = false;
                false
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = self.to_logical(position.x, position.y);
                self.last_cursor = (x, y);
                self.pending_move = Some((x, y));
                false
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    let (x, y) = self.last_cursor;
                    let event = match state {
                        ElementState::Pressed => InputEvent::PointerDown { id: 0, x, y },
                        ElementState::Released => InputEvent::PointerUp { id: 0, x, y },
                    };
                    self.event_queue.push_back(event);
                }
                false
            }
            WindowEvent::MouseWheel { delta, .. } => {
                const PX_PER_LINE: f32 = 16.0;
                let scale = self
                    .state
                    .as_ref()
                    .map(|s| s.window.scale_factor())
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
                false
            }
            WindowEvent::Touch(touch) => {
                let (x, y) = self.to_logical(touch.location.x, touch.location.y);
                if let Some(event) = self.native_touches.input_event(touch.id, touch.phase, x, y) {
                    self.event_queue.push_back(event);
                }
                false
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
                use crate::input::event::input::*;
                if state != ElementState::Pressed {
                    return false;
                }
                let code = match &logical_key {
                    Key::Named(NamedKey::Backspace) => Some(KEY_BACKSPACE),
                    Key::Named(NamedKey::Delete) => Some(KEY_DELETE),
                    Key::Named(NamedKey::ArrowLeft) => Some(KEY_LEFT),
                    Key::Named(NamedKey::ArrowRight) => Some(KEY_RIGHT),
                    Key::Named(NamedKey::Home) => Some(KEY_HOME),
                    Key::Named(NamedKey::End) => Some(KEY_END),
                    Key::Named(NamedKey::Enter) => Some(KEY_RETURN),
                    Key::Named(NamedKey::Escape | NamedKey::BrowserBack) => {
                        self.event_queue.push_back(InputEvent::Quit);
                        return true;
                    }
                    _ => None,
                };
                if let Some(code) = code {
                    self.event_queue.push_back(InputEvent::Key {
                        code,
                        pressed: true,
                    });
                }
                if let Some(text) = text.as_ref() {
                    enqueue_text(&mut self.event_queue, text);
                }
                false
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                enqueue_text(&mut self.event_queue, &text);
                false
            }
            _ => false,
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl ApplicationHandler for WgpuHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.state.is_some() {
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

        self.runtime.resume(window);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.runtime.suspend();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.runtime.window_event(event) {
            event_loop.exit();
        }
    }
}

pub(crate) fn create_wgpu_state(window: Arc<Window>) -> WgpuState {
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
    let context = Arc::new(WgpuDeviceContext {
        instance,
        adapter,
        device,
        queue,
    });
    configure_wgpu_state(context, window, surface)
}

fn create_wgpu_state_with_context(
    context: Arc<WgpuDeviceContext>,
    window: Arc<Window>,
) -> WgpuState {
    let surface = context
        .instance
        .create_surface(window.clone())
        .expect("wgpu create_surface failed");
    configure_wgpu_state(context, window, surface)
}

fn configure_wgpu_state(
    context: Arc<WgpuDeviceContext>,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
) -> WgpuState {
    let size = mobile_window_drawable_size(&window);
    let surface_caps = surface.get_capabilities(&context.adapter);
    let surface_format = surface_caps
        .formats
        .iter()
        .copied()
        .find(|format| !format.is_srgb())
        .unwrap_or(surface_caps.formats[0]);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        format: surface_format,
        width: size.width.max(1),
        height: size.height.max(1),
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
    surface.configure(&context.device, &config);
    let msaa = create_msaa(&context.device, &config);
    WgpuState {
        context,
        window,
        surface,
        config,
        msaa,
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
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

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl WgpuSurface {
    /// `None` only between `WgpuSurface::new` constructing the struct
    /// and `resumed` populating the wgpu device.
    pub fn state(&self) -> Option<&WgpuState> {
        self.handler.runtime.state.as_ref()
    }

    pub fn state_mut(&mut self) -> Option<&mut WgpuState> {
        self.handler.runtime.state.as_mut()
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
                runtime: WgpuRuntime::new(),
            },
            pumped_this_frame: false,
        };

        // winit creates windows from `resumed` only.
        let mut spins = 0;
        while this.handler.runtime.state.is_none() && spins < 100 {
            this.pump_once();
            spins += 1;
        }
        if this.handler.runtime.state.is_none() {
            panic!("WgpuSurface: winit failed to deliver resumed within {spins} pumps");
        }

        this
    }

    fn pump_once(&mut self) -> winit::platform::pump_events::PumpStatus {
        self.event_loop
            .pump_app_events(Some(Duration::ZERO), &mut self.handler)
    }
}

fn create_msaa(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> Option<wgpu::Texture> {
    (crate::render::wgpu::MSAA_SAMPLES > 1).then(|| {
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
    })
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl InspectCaches for WgpuSurface {}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl WgpuTarget for WgpuSurface {
    fn state(&self) -> Option<&WgpuState> {
        self.state()
    }

    fn state_mut(&mut self) -> Option<&mut WgpuState> {
        self.state_mut()
    }

    fn acquire_surface_texture(&mut self) -> Option<wgpu::SurfaceTexture> {
        self.handler.runtime.acquire_surface_texture()
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl Surface for WgpuSurface {
    fn display_info(&self) -> DisplayInfo {
        let state = self
            .handler
            .runtime
            .state
            .as_ref()
            .expect("WgpuSurface state must be initialised by new()");
        let size = mobile_window_drawable_size(&state.window);
        let scale_int = state
            .window
            .scale_factor()
            .round()
            .clamp(1.0, u16::MAX as f64) as u16;
        let scale = Fixed::from(scale_int);
        let physical_width = u16::try_from(size.width).unwrap_or(u16::MAX);
        let physical_height = u16::try_from(size.height).unwrap_or(u16::MAX);
        let (lw, lh) = logical_from_physical(physical_width, physical_height, scale);
        DisplayInfo {
            width: lw,
            height: lh,
            scale,
            format: ColorFormat::RGBA8888,
        }
    }

    fn flush(&mut self, _area: crate::types::PhysicalRect) {
        // wgpu present itself happens inside `WgpuRenderer::flush`
        // (the SurfaceTexture lives on the renderer's frame state)
        // — this method only re-arms direct `App::render` callers.
        self.pumped_this_frame = false;
    }

    fn frame_end(&mut self) {
        // Failed draws do not reach `flush`, but the next tick must still
        // pump winit so a temporarily unavailable drawable can recover.
        self.pumped_this_frame = false;
    }

    fn physical_size(&self) -> (u32, u32) {
        let state = self
            .handler
            .runtime
            .state
            .as_ref()
            .expect("WgpuSurface state must be initialised by new()");
        let size = mobile_window_drawable_size(&state.window);
        (size.width, size.height)
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if let Some(e) = self.handler.runtime.event_queue.pop_front() {
            return Some(e);
        }
        if self.pumped_this_frame {
            return None;
        }
        self.pumped_this_frame = true;

        self.pump_once();
        if let Some((x, y)) = self.handler.runtime.pending_move.take() {
            self.handler
                .runtime
                .event_queue
                .push_back(InputEvent::PointerMove { id: 0, x, y });
        }
        self.handler.runtime.event_queue.pop_front()
    }

    fn persistence(&self) -> BackbufferPersistence {
        BackbufferPersistence::Transient
    }
}

/// Surface lifecycle required by platform-owned mobile event loops.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub trait MobileSurface: Surface {
    fn resume_window(&mut self, window: Arc<Window>) -> Result<(), MobileSurfaceError>;

    fn suspend_window(&mut self);

    fn handle_window_event(
        &mut self,
        event: WindowEvent,
    ) -> Result<MobileEventFlow, MobileSurfaceError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MobileEventFlow {
    Continue,
    Exit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MobileSurfaceError {
    Allocation,
    BufferSizeOverflow,
    InvalidRenderScale,
    FramebufferBudget { required: usize, budget: usize },
}

/// Resolution policy for the software framebuffer uploaded to a mobile WGPU surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SoftwareRenderScale {
    /// Use native device density, reduced only when required by the framebuffer budget.
    #[default]
    Device,
    /// Require an explicit logical-to-physical scale.
    Fixed(Fixed),
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
pub(crate) fn software_buffer_layout(
    logical_width: Fixed,
    logical_height: Fixed,
    render_scale: Fixed,
    budget: usize,
) -> Result<(u16, u16, usize), MobileSurfaceError> {
    if !render_scale.is_positive() {
        return Err(MobileSurfaceError::InvalidRenderScale);
    }
    let width = (logical_width * render_scale)
        .round()
        .to_int()
        .clamp(1, i32::from(u16::MAX)) as u16;
    let height = (logical_height * render_scale)
        .round()
        .to_int()
        .clamp(1, i32::from(u16::MAX)) as u16;
    let required = usize::from(width)
        .checked_mul(usize::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(MobileSurfaceError::BufferSizeOverflow)?;
    if required > budget {
        return Err(MobileSurfaceError::FramebufferBudget { required, budget });
    }
    Ok((width, height, required))
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
pub(crate) fn resolve_software_buffer_layout(
    logical_width: Fixed,
    logical_height: Fixed,
    native_scale: Fixed,
    render_scale: SoftwareRenderScale,
    budget: usize,
) -> Result<(Fixed, u16, u16, usize), MobileSurfaceError> {
    match render_scale {
        SoftwareRenderScale::Fixed(scale) => {
            software_buffer_layout(logical_width, logical_height, scale, budget)
                .map(|(width, height, required)| (scale, width, height, required))
        }
        SoftwareRenderScale::Device => {
            if !native_scale.is_positive() {
                return Err(MobileSurfaceError::InvalidRenderScale);
            }
            if let Ok((width, height, required)) =
                software_buffer_layout(logical_width, logical_height, native_scale, budget)
            {
                return Ok((native_scale, width, height, required));
            }

            let mut low = 64_i32;
            let mut high = (native_scale.to_f32() * 256.0).floor() as i32;
            let mut best = None;
            while low <= high {
                let middle = low + (high - low) / 2;
                let scale = Fixed::from_ratio(middle, 256);
                match software_buffer_layout(logical_width, logical_height, scale, budget) {
                    Ok((width, height, required)) => {
                        best = Some((scale, width, height, required));
                        low = middle + 1;
                    }
                    Err(MobileSurfaceError::FramebufferBudget { .. }) => {
                        high = middle - 1;
                    }
                    Err(error) => return Err(error),
                }
            }
            best.ok_or_else(|| {
                software_buffer_layout(
                    logical_width,
                    logical_height,
                    Fixed::from_ratio(1, 4),
                    budget,
                )
                .expect_err("minimum software scale must exceed the framebuffer budget")
            })
        }
    }
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
pub(crate) fn software_presenter_needs_rebuild(
    current_width: u16,
    current_height: u16,
    next_width: u16,
    next_height: u16,
    has_presenter: bool,
) -> bool {
    !has_presenter || current_width != next_width || current_height != next_height
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(target_os = "android", target_os = "ios", test))]
pub(crate) struct SoftwareUploadRegion {
    pub offset: usize,
    pub bytes_per_row: u32,
    pub width: u32,
    pub height: u32,
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
pub(crate) fn software_upload_region(
    buffer_width: u16,
    buffer_height: u16,
    area: crate::types::PhysicalRect,
) -> Option<SoftwareUploadRegion> {
    if area.is_empty() || area.right() > buffer_width || area.bottom() > buffer_height {
        return None;
    }
    let bytes_per_row = u32::from(buffer_width).checked_mul(4)?;
    let offset = usize::from(area.y())
        .checked_mul(bytes_per_row as usize)?
        .checked_add(usize::from(area.x()).checked_mul(4)?)?;
    Some(SoftwareUploadRegion {
        offset,
        bytes_per_row,
        width: u32::from(area.width()),
        height: u32::from(area.height()),
    })
}

/// Direct-WGPU surface driven by the native mobile application loop.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct MobileWgpuSurface {
    runtime: WgpuRuntime,
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl MobileWgpuSurface {
    pub fn new(window: Arc<Window>) -> Self {
        let mut runtime = WgpuRuntime::new();
        runtime.resume(window);
        Self { runtime }
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl InspectCaches for MobileWgpuSurface {}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl WgpuTarget for MobileWgpuSurface {
    fn state(&self) -> Option<&WgpuState> {
        self.runtime.state.as_ref()
    }

    fn state_mut(&mut self) -> Option<&mut WgpuState> {
        self.runtime.state.as_mut()
    }

    fn acquire_surface_texture(&mut self) -> Option<wgpu::SurfaceTexture> {
        self.runtime.acquire_surface_texture()
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl MobileSurface for MobileWgpuSurface {
    fn resume_window(&mut self, window: Arc<Window>) -> Result<(), MobileSurfaceError> {
        self.runtime.resume(window);
        Ok(())
    }

    fn suspend_window(&mut self) {
        self.runtime.suspend();
    }

    fn handle_window_event(
        &mut self,
        event: WindowEvent,
    ) -> Result<MobileEventFlow, MobileSurfaceError> {
        Ok(if self.runtime.window_event(event) {
            MobileEventFlow::Exit
        } else {
            MobileEventFlow::Continue
        })
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl Surface for MobileWgpuSurface {
    fn display_info(&self) -> DisplayInfo {
        let state = self
            .runtime
            .state
            .as_ref()
            .expect("mobile WGPU surface is not resumed");
        let size = mobile_window_drawable_size(&state.window);
        let scale = Fixed::from_f32(state.window.scale_factor() as f32);
        let physical_width = u16::try_from(size.width).unwrap_or(u16::MAX);
        let physical_height = u16::try_from(size.height).unwrap_or(u16::MAX);
        let (width, height) = logical_from_physical(physical_width, physical_height, scale);
        DisplayInfo {
            width,
            height,
            scale,
            format: ColorFormat::RGBA8888,
        }
    }

    fn flush(&mut self, _area: crate::types::PhysicalRect) {}

    fn safe_area_insets(&self) -> SafeAreaInsets {
        self.runtime
            .state
            .as_ref()
            .map_or_else(SafeAreaInsets::default, |state| {
                mobile_window_safe_area(&state.window)
            })
    }

    fn physical_size(&self) -> (u32, u32) {
        let state = self
            .runtime
            .state
            .as_ref()
            .expect("mobile WGPU surface is not resumed");
        let size = mobile_window_drawable_size(&state.window);
        (size.width, size.height)
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
        BackbufferPersistence::Transient
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn wgpu_mobile_host<F, Build>(
    title: impl Into<alloc::string::String>,
    build: Build,
) -> MobileHost<
    MobileWgpuSurface,
    F,
    impl FnOnce(Arc<Window>) -> Result<MobileWgpuSurface, MobileSurfaceError>,
    Build,
>
where
    F: crate::render::factory::RendererFactory<MobileWgpuSurface> + 'static,
    Build: FnOnce(MobileWgpuSurface) -> crate::app::App<MobileWgpuSurface, F> + 'static,
{
    MobileHost::new(title, |window| Ok(MobileWgpuSurface::new(window)), build)
}

/// Owns a mirui app inside the platform event loop.
#[cfg(any(target_os = "android", target_os = "ios"))]
pub struct MobileHost<B, F, Create, Build>
where
    B: MobileSurface,
    F: crate::render::factory::RendererFactory<B>,
    Create: FnOnce(Arc<Window>) -> Result<B, MobileSurfaceError>,
    Build: FnOnce(B) -> crate::app::App<B, F>,
{
    title: alloc::string::String,
    create: Option<Create>,
    build: Option<Build>,
    app: Option<crate::app::App<B, F>>,
    window: Option<Arc<Window>>,
    ime_allowed: bool,
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl<B, F, Create, Build> MobileHost<B, F, Create, Build>
where
    B: MobileSurface + 'static,
    F: crate::render::factory::RendererFactory<B> + 'static,
    Create: FnOnce(Arc<Window>) -> Result<B, MobileSurfaceError> + 'static,
    Build: FnOnce(B) -> crate::app::App<B, F> + 'static,
{
    pub fn new(title: impl Into<alloc::string::String>, create: Create, build: Build) -> Self {
        Self {
            title: title.into(),
            create: Some(create),
            build: Some(build),
            app: None,
            window: None,
            ime_allowed: false,
        }
    }

    fn sync_text_input(&mut self) {
        let requested = self
            .app
            .as_ref()
            .is_some_and(|app| crate::input::event::focus::text_input_requested(&app.world));
        if requested == self.ime_allowed {
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.set_ime_allowed(requested);
            self.ime_allowed = requested;
        }
    }

    #[cfg(target_os = "android")]
    pub fn run_android(mut self, android_app: winit::platform::android::activity::AndroidApp) -> ! {
        use winit::platform::android::EventLoopBuilderExtAndroid;

        let mut builder = EventLoop::builder();
        builder.with_android_app(android_app);
        let event_loop = builder.build().expect("winit Android event loop");
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop
            .run_app(&mut self)
            .expect("winit Android application loop");
        unreachable!("mobile event loop returned")
    }

    #[cfg(target_os = "ios")]
    pub fn run_ios(mut self) -> ! {
        let event_loop = EventLoop::new().expect("winit iOS event loop");
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop
            .run_app(&mut self)
            .expect("winit iOS application loop");
        unreachable!("mobile event loop returned")
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl<B, F, Create, Build> ApplicationHandler for MobileHost<B, F, Create, Build>
where
    B: MobileSurface + 'static,
    F: crate::render::factory::RendererFactory<B> + 'static,
    Create: FnOnce(Arc<Window>) -> Result<B, MobileSurfaceError> + 'static,
    Build: FnOnce(B) -> crate::app::App<B, F> + 'static,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Poll);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title(self.title.clone()))
                .expect("winit mobile window"),
        );

        if let Some(app) = self.app.as_mut() {
            if let Err(error) = app.backend.resume_window(window.clone()) {
                crate::warn!("mobile surface resume failed: {:?}", error);
                event_loop.exit();
                return;
            }
            app.resume();
        } else {
            let create = self
                .create
                .take()
                .expect("mobile surface builder consumed once");
            let surface = match create(window.clone()) {
                Ok(surface) => surface,
                Err(error) => {
                    crate::warn!("mobile surface creation failed: {:?}", error);
                    event_loop.exit();
                    return;
                }
            };
            let build = self.build.take().expect("mobile app builder consumed once");
            self.app = Some(build(surface));
        }
        self.window = Some(window.clone());
        window.request_redraw();
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        if self.ime_allowed {
            if let Some(window) = self.window.as_ref() {
                window.set_ime_allowed(false);
            }
            self.ime_allowed = false;
        }
        if let Some(app) = self.app.as_mut() {
            app.suspend();
            app.backend.suspend_window();
        }
        self.window = None;
        event_loop.set_control_flow(ControlFlow::Wait);
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(app) = self.app.as_mut() {
            app.handle_memory_warning();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if !window_event_is_current(Some(&window.id()), &window_id) {
            return;
        }
        let Some(app) = self.app.as_mut() else {
            return;
        };
        if matches!(event, WindowEvent::RedrawRequested) {
            if app.is_suspended() {
                return;
            }
            if app.tick() {
                event_loop.exit();
                return;
            }
            self.sync_text_input();
        } else {
            match app.backend.handle_window_event(event) {
                Ok(MobileEventFlow::Continue) => {}
                Ok(MobileEventFlow::Exit) => {
                    event_loop.exit();
                    return;
                }
                Err(error) => {
                    crate::warn!("mobile surface event failed: {:?}", error);
                    event_loop.exit();
                    return;
                }
            }
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.app.as_ref().is_some_and(|app| !app.is_suspended())
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
    }
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
fn window_event_is_current<T: Eq>(active: Option<&T>, incoming: &T) -> bool {
    active.is_some_and(|active| active == incoming)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspend_discards_input_bound_to_the_old_native_window() {
        let mut runtime = WgpuRuntime::new();
        runtime.event_queue.push_back(InputEvent::PointerDown {
            id: 0,
            x: Fixed::from_int(4),
            y: Fixed::from_int(8),
        });
        runtime.pending_move = Some((Fixed::from_int(10), Fixed::from_int(12)));

        runtime.suspend();

        assert!(runtime.event_queue.is_empty());
        assert!(runtime.pending_move.is_none());
    }

    #[test]
    fn only_the_active_native_window_dispatches_events() {
        assert!(window_event_is_current(Some(&7), &7));
        assert!(!window_event_is_current(Some(&7), &8));
        assert!(!window_event_is_current::<u8>(None, &7));
    }

    #[test]
    fn physical_system_insets_are_reported_in_logical_pixels() {
        assert_eq!(
            safe_area_from_physical(1080, 2400, Fixed::from_int(3), 0, 72, 1080, 2256),
            SafeAreaInsets {
                top: Fixed::from_int(24),
                right: Fixed::ZERO,
                bottom: Fixed::from_int(48),
                left: Fixed::ZERO,
            }
        );
    }

    #[test]
    fn opaque_touch_ids_map_to_stable_compact_slots() {
        let mut touches = NativeTouchMap::<2>::new();
        let x = Fixed::from_int(7);
        let y = Fixed::from_int(11);
        let native = 0x0000_7fff_1234_5678;
        assert!(matches!(
            touches.input_event(native, TouchPhase::Started, x, y),
            Some(InputEvent::PointerDown { id: 0, x: px, y: py }) if px == x && py == y
        ));
        assert!(matches!(
            touches.input_event(native, TouchPhase::Moved, x, y),
            Some(InputEvent::PointerMove { id: 0, x: px, y: py }) if px == x && py == y
        ));
        assert!(matches!(
            touches.input_event(native, TouchPhase::Cancelled, x, y),
            Some(InputEvent::PointerUp { id: 0, x: px, y: py }) if px == x && py == y
        ));
        assert!(matches!(
            touches.input_event(native + 1, TouchPhase::Started, x, y),
            Some(InputEvent::PointerDown { id: 0, .. })
        ));
    }

    #[test]
    fn touch_map_preserves_established_contacts_when_full() {
        let mut touches = NativeTouchMap::<2>::new();
        let x = Fixed::ZERO;
        let y = Fixed::ZERO;
        assert!(touches.input_event(10, TouchPhase::Started, x, y).is_some());
        assert!(touches.input_event(20, TouchPhase::Started, x, y).is_some());
        assert!(touches.input_event(30, TouchPhase::Started, x, y).is_none());
        assert!(matches!(
            touches.input_event(20, TouchPhase::Moved, x, y),
            Some(InputEvent::PointerMove { id: 1, .. })
        ));
        touches.clear();
        assert!(touches.input_event(20, TouchPhase::Moved, x, y).is_none());
    }

    #[test]
    fn fractional_device_scale_preserves_logical_coordinates() {
        assert_eq!(
            physical_to_logical(3.0, 6.0, 1.5),
            (Fixed::from_int(2), Fixed::from_int(4))
        );
        assert_eq!(
            physical_to_logical(12.0, 20.0, 0.0),
            (Fixed::from_int(12), Fixed::from_int(20))
        );
    }

    #[test]
    fn text_event_translation_keeps_every_committed_scalar() {
        let mut queue = VecDeque::new();
        enqueue_text(&mut queue, "A中\n🙂");
        assert!(matches!(
            queue.pop_front(),
            Some(InputEvent::CharInput { ch: 'A' })
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(InputEvent::CharInput { ch: '中' })
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(InputEvent::CharInput { ch: '🙂' })
        ));
        assert!(queue.is_empty());
    }

    #[test]
    fn software_buffer_layout_applies_scale_and_budget() {
        assert_eq!(
            software_buffer_layout(
                Fixed::from_int(390),
                Fixed::from_int(844),
                Fixed::ONE,
                16 * 1024 * 1024,
            ),
            Ok((390, 844, 390 * 844 * 4))
        );
        assert_eq!(
            software_buffer_layout(
                Fixed::from_int(480),
                Fixed::from_int(1024),
                Fixed::from_int(3),
                16 * 1024 * 1024,
            ),
            Err(MobileSurfaceError::FramebufferBudget {
                required: 1440 * 3072 * 4,
                budget: 16 * 1024 * 1024,
            })
        );
    }

    #[test]
    fn software_buffer_layout_rejects_non_positive_scale() {
        assert_eq!(
            software_buffer_layout(
                Fixed::from_int(400),
                Fixed::from_int(800),
                Fixed::ZERO,
                1024 * 1024,
            ),
            Err(MobileSurfaceError::InvalidRenderScale)
        );
    }

    #[test]
    fn device_scale_uses_native_pixels_when_the_budget_allows() {
        assert_eq!(
            resolve_software_buffer_layout(
                Fixed::from_int(360),
                Fixed::from_int(800),
                Fixed::from_int(3),
                SoftwareRenderScale::Device,
                32 * 1024 * 1024,
            ),
            Ok((Fixed::from_int(3), 1080, 2400, 1080 * 2400 * 4))
        );
    }

    #[test]
    fn device_scale_chooses_the_highest_q24_8_scale_inside_the_budget() {
        let budget = 360 * 800 * 4;
        let (scale, width, height, required) = resolve_software_buffer_layout(
            Fixed::from_int(360),
            Fixed::from_int(800),
            Fixed::from_int(3),
            SoftwareRenderScale::Device,
            budget,
        )
        .unwrap();
        assert_eq!(scale, Fixed::ONE);
        assert_eq!((width, height, required), (360, 800, budget));
    }

    #[test]
    fn resume_rebuilds_a_dropped_presenter_at_the_same_size() {
        assert!(software_presenter_needs_rebuild(
            1080, 2400, 1080, 2400, false
        ));
        assert!(!software_presenter_needs_rebuild(
            1080, 2400, 1080, 2400, true
        ));
        assert!(software_presenter_needs_rebuild(
            1080, 2400, 1200, 2400, true
        ));
    }

    #[test]
    fn software_upload_region_uses_full_stride_without_copying_rows() {
        let area = crate::types::PhysicalRect::new(7, 5, 20, 9).unwrap();
        assert_eq!(
            software_upload_region(64, 32, area),
            Some(SoftwareUploadRegion {
                offset: 5 * 64 * 4 + 7 * 4,
                bytes_per_row: 64 * 4,
                width: 20,
                height: 9,
            })
        );
        assert!(
            software_upload_region(
                64,
                32,
                crate::types::PhysicalRect::new(60, 0, 8, 1).unwrap()
            )
            .is_none()
        );
    }
}
