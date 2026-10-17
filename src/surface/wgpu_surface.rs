//! WGPU surfaces for desktop polling loops and native mobile event loops.

use alloc::collections::VecDeque;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use alloc::string::String;
use alloc::string::ToString;
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
use crate::render::RenderError;
use crate::render::texture::ColorFormat;
use crate::types::Fixed;

mod text_input;
use text_input::NativeTextInput;
mod config;
pub use config::{PowerPreference, PresentMode, WgpuConfig, WgpuSurfaceError};

const MAX_NATIVE_TOUCHES: usize = 16;

struct NativeTouchMap<const N: usize> {
    slots: super::pointer_slots::PointerSlots<u64, N>,
}

impl<const N: usize> NativeTouchMap<N> {
    fn new() -> Self {
        Self {
            slots: super::pointer_slots::PointerSlots::new(),
        }
    }

    fn clear(&mut self) {
        self.slots.clear();
    }

    fn input_event(
        &mut self,
        native_id: u64,
        phase: TouchPhase,
        x: Fixed,
        y: Fixed,
    ) -> Option<InputEvent> {
        let id = match phase {
            TouchPhase::Started => {
                if self.slots.get(native_id).is_some() {
                    return None;
                }
                self.slots.insert(native_id)?
            }
            TouchPhase::Moved => self.slots.get(native_id)?,
            TouchPhase::Ended | TouchPhase::Cancelled => self.slots.remove(native_id)?,
        };
        let event = match phase {
            TouchPhase::Started => InputEvent::PointerDown {
                kind: crate::input::event::input::PointerKind::Touch,
                id,
                x,
                y,
            },
            TouchPhase::Moved => InputEvent::PointerMove {
                kind: crate::input::event::input::PointerKind::Touch,
                id,
                x,
                y,
            },
            TouchPhase::Ended => InputEvent::PointerUp {
                kind: crate::input::event::input::PointerKind::Touch,
                id,
                x,
                y,
            },
            TouchPhase::Cancelled => InputEvent::PointerCancel {
                kind: crate::input::event::input::PointerKind::Touch,
                id,
                x,
                y,
            },
        };
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

fn wheel_delta(delta: MouseScrollDelta, scale: f64) -> (Fixed, Fixed) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => {
            let step = crate::input::event::input::WHEEL_LINE_PIXELS.to_f32();
            (Fixed::from_f32(-x * step), Fixed::from_f32(y * step))
        }
        MouseScrollDelta::PixelDelta(p) => physical_to_logical(-p.x, p.y, scale),
    }
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

/// Shared device resources retained by surfaces and native render targets.
pub struct WgpuDeviceContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    invalid: core::sync::atomic::AtomicBool,
}

impl WgpuDeviceContext {
    pub(crate) fn new(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
    ) -> Arc<Self> {
        let context = Arc::new(Self {
            instance,
            adapter,
            device,
            queue,
            invalid: core::sync::atomic::AtomicBool::new(false),
        });
        #[cfg(not(target_arch = "wasm32"))]
        {
            let owner = Arc::downgrade(&context);
            context.device.set_device_lost_callback(move |_, _| {
                if let Some(owner) = owner.upgrade() {
                    owner.invalidate();
                }
            });
        }
        context
    }

    pub(crate) fn check(&self) -> Result<(), RenderError> {
        if self.invalid.load(core::sync::atomic::Ordering::Acquire) {
            Err(RenderError::BackendFailure)
        } else {
            Ok(())
        }
    }

    pub(crate) fn invalidate(&self) {
        self.invalid
            .store(true, core::sync::atomic::Ordering::Release);
    }

    pub(crate) fn format_features(
        &self,
        format: wgpu::TextureFormat,
    ) -> wgpu::TextureFormatFeatures {
        if self
            .device
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            self.adapter.get_texture_format_features(format)
        } else {
            format.guaranteed_format_features(self.device.features())
        }
    }
}

/// Live window attachments, created when the platform resumes the surface.
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

impl WgpuState {
    pub(crate) fn context(&self) -> &Arc<WgpuDeviceContext> {
        &self.context
    }

    /// Actual color attachment sample count shared with render pipelines.
    pub fn samples(&self) -> u32 {
        self.msaa.as_ref().map_or(1, wgpu::Texture::sample_count)
    }
}

/// Surface capability required by the WGPU renderer.
///
/// Event-loop ownership is intentionally outside this trait so desktop and
/// platform-owned mobile loops can share the same renderer.
pub trait WgpuTarget: Surface {
    fn state(&self) -> Option<&WgpuState>;

    fn state_mut(&mut self) -> Option<&mut WgpuState>;

    /// Drawable prepared by `Surface::begin_flush` and owned until acceptance or abort.
    fn frame_texture(&self) -> Option<&wgpu::SurfaceTexture>;
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
struct WgpuHandler {
    title: String,
    requested_size: (u32, u32),
    runtime: WgpuRuntime,
    error: Option<WgpuSurfaceError>,
}

pub(crate) struct WgpuRuntime {
    #[cfg(target_os = "linux")]
    clipboard: super::host_clipboard::linux::Clipboard,
    config: WgpuConfig,
    context: Option<Arc<WgpuDeviceContext>>,
    pub(crate) state: Option<WgpuState>,
    event_queue: VecDeque<InputEvent>,
    /// Last known cursor position, in logical pixels. Updated on
    /// every `CursorMoved` so `MouseInput` (which doesn't carry a
    /// position in winit 0.30) can attach one to the synthetic
    /// `PointerDown`/`PointerUp`.
    pub(crate) last_cursor: (Fixed, Fixed),
    /// Consecutive cursor moves share one pending position until another event or polling.
    pending_move: Option<(Fixed, Fixed)>,
    native_touches: NativeTouchMap<MAX_NATIVE_TOUCHES>,
    modifiers: Option<crate::input::event::input::KeyModifiers>,
    text_input: NativeTextInput,
    reconfigure_before_acquire: bool,
    frame_texture: Option<wgpu::SurfaceTexture>,
    needs_redraw: bool,
}

impl WgpuRuntime {
    fn flush_pointer_move(&mut self) {
        if let Some((x, y)) = self.pending_move.take() {
            self.event_queue.push_back(InputEvent::PointerMove {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x,
                y,
            });
        }
    }

    pub(crate) fn poll_event(&mut self) -> Option<InputEvent> {
        self.flush_pointer_move();
        #[cfg(target_os = "linux")]
        if let Some(response) = self.clipboard.poll() {
            self.event_queue.push_back(InputEvent::Clipboard(response));
        }
        self.event_queue.pop_front()
    }

    pub(crate) fn set_text_input(&mut self, state: Option<crate::ui::text_input::TextInputState>) {
        self.text_input.set(
            state,
            self.state.as_ref().map(|state| state.window.as_ref()),
        );
    }

    pub(crate) fn finish_input_batch(&mut self) {
        self.flush_pointer_move();
        if let Some(event) = self.text_input.finish_batch() {
            self.event_queue.push_back(event);
        }
    }

    fn keyboard_input(
        &mut self,
        key: &Key,
        state: ElementState,
        text: Option<&str>,
        repeat: bool,
    ) -> bool {
        use crate::input::event::input::{
            KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_HOME, KEY_LEFT, KEY_RETURN, KEY_RIGHT,
        };

        let shortcut = match key {
            Key::Character(text) => crate::input::event::input::selection_shortcut_key(
                text,
                self.modifiers.unwrap_or_default(),
            ),
            _ => None,
        };
        if self.text_input.composing() && shortcut.is_none() {
            return false;
        }

        let pressed = state == ElementState::Pressed;
        let code = match key {
            Key::Named(NamedKey::Backspace) => Some(KEY_BACKSPACE),
            Key::Named(NamedKey::Delete) => Some(KEY_DELETE),
            Key::Named(NamedKey::ArrowLeft) => Some(KEY_LEFT),
            Key::Named(NamedKey::ArrowRight) => Some(KEY_RIGHT),
            Key::Named(NamedKey::Home) => Some(KEY_HOME),
            Key::Named(NamedKey::End) => Some(KEY_END),
            Key::Named(NamedKey::Enter) => Some(KEY_RETURN),
            Key::Named(NamedKey::Escape | NamedKey::BrowserBack) => {
                if pressed {
                    self.event_queue.push_back(InputEvent::Quit);
                }
                return pressed;
            }
            _ => shortcut,
        };
        if let Some(code) = code {
            self.event_queue.push_back(InputEvent::Key {
                code,
                pressed,
                modifiers: self.modifiers,
                repeat: Some(repeat),
            });
        }
        if pressed
            && shortcut.is_none()
            && let Some(text) = text
            && !text.chars().all(char::is_control)
        {
            if let Some(event) = self.text_input.keyboard_text(text) {
                self.event_queue.push_back(event);
            }
        }
        false
    }

    pub(crate) fn new(config: WgpuConfig) -> Self {
        Self {
            config,
            context: None,
            state: None,
            event_queue: VecDeque::new(),
            last_cursor: (Fixed::ZERO, Fixed::ZERO),
            pending_move: None,
            native_touches: NativeTouchMap::new(),
            modifiers: None,
            text_input: NativeTextInput::default(),
            #[cfg(target_os = "linux")]
            clipboard: Default::default(),
            reconfigure_before_acquire: false,
            frame_texture: None,
            needs_redraw: true,
        }
    }

    pub(crate) fn resume(&mut self, window: Arc<Window>) -> Result<(), WgpuSurfaceError> {
        if self.state.is_some() {
            return Ok(());
        }
        let state = match self.context.as_ref() {
            Some(context) => create_wgpu_state_with_context(context.clone(), window, self.config),
            None => create_wgpu_state(window, self.config),
        }?;
        self.context = Some(state.context.clone());
        #[cfg(target_os = "linux")]
        // Runtime drops/detaches the clipboard before releasing the native window.
        unsafe {
            self.clipboard.attach(state.window.as_ref());
        }
        self.state = Some(state);
        self.needs_redraw = true;
        Ok(())
    }

    pub(crate) fn suspend(&mut self) {
        self.frame_texture = None;
        self.set_text_input(None);
        self.modifiers = None;
        #[cfg(target_os = "linux")]
        self.clipboard.detach();
        self.state = None;
        self.event_queue
            .retain(|event| matches!(event, InputEvent::Clipboard(_)));
        self.pending_move = None;
        self.native_touches.clear();
        self.reconfigure_before_acquire = false;
    }

    fn reconfigure_surface(&mut self) -> bool {
        let Some(state) = self.state.as_mut() else {
            return false;
        };
        if state.check().is_err() {
            return false;
        }
        let size = mobile_window_drawable_size(&state.window);
        state.config.width = size.width.max(1);
        state.config.height = size.height.max(1);
        state.surface.configure(&state.device, &state.config);
        state.msaa = create_msaa(&state.device, &state.config, self.config.samples);
        self.reconfigure_before_acquire = false;
        true
    }

    fn recreate_surface(&mut self) -> Result<(), crate::render::RenderError> {
        let Some(previous) = self.state.as_ref() else {
            return Err(crate::render::RenderError::SurfaceUnavailable);
        };
        let context = previous.context.clone();
        let window = previous.window.clone();
        let state = create_wgpu_state_with_context(context, window, self.config)
            .map_err(|_| crate::render::RenderError::BackendFailure)?;
        self.state = Some(state);
        self.reconfigure_before_acquire = false;
        Ok(())
    }

    pub(crate) fn acquire_surface_texture(
        &mut self,
    ) -> Result<wgpu::SurfaceTexture, crate::render::RenderError> {
        use crate::render::RenderError;
        let size = mobile_window_drawable_size(
            &self
                .state
                .as_ref()
                .ok_or(RenderError::SurfaceUnavailable)?
                .window,
        );
        if size.width == 0 || size.height == 0 {
            return Err(RenderError::SurfaceUnavailable);
        }
        let size_changed = self.state.as_ref().is_some_and(|state| {
            state.config.width != size.width || state.config.height != size.height
        });
        if (self.reconfigure_before_acquire || size_changed) && !self.reconfigure_surface() {
            return Err(RenderError::SurfaceUnavailable);
        }
        let mut retried = false;
        loop {
            let acquired = self
                .state
                .as_ref()
                .ok_or(RenderError::SurfaceUnavailable)?
                .surface
                .get_current_texture();
            match acquired {
                wgpu::CurrentSurfaceTexture::Success(texture) => return Ok(texture),
                wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                    self.reconfigure_before_acquire = true;
                    return Ok(texture);
                }
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Err(RenderError::SurfaceUnavailable);
                }
                wgpu::CurrentSurfaceTexture::Outdated if !retried => {
                    retried = true;
                    if !self.reconfigure_surface() {
                        return Err(RenderError::SurfaceUnavailable);
                    }
                }
                wgpu::CurrentSurfaceTexture::Lost if !retried => {
                    retried = true;
                    self.recreate_surface()?;
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    return Err(RenderError::SurfaceUnavailable);
                }
                wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::BackendFailure),
            }
        }
    }

    fn begin_frame(&mut self) -> Result<(), crate::render::RenderError> {
        self.frame_texture = None;
        self.state
            .as_ref()
            .ok_or(RenderError::SurfaceUnavailable)?
            .check()?;
        self.frame_texture = Some(self.acquire_surface_texture()?);
        Ok(())
    }

    fn present_frame(&mut self) -> Result<(), crate::render::RenderError> {
        self.state
            .as_ref()
            .ok_or(RenderError::SurfaceUnavailable)?
            .check()?;
        let frame = self
            .frame_texture
            .take()
            .ok_or(crate::render::RenderError::SurfaceUnavailable)?;
        frame.present();
        self.needs_redraw = false;
        Ok(())
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
        if !matches!(event, WindowEvent::CursorMoved { .. }) {
            self.flush_pointer_move();
        }
        match event {
            WindowEvent::RedrawRequested => {
                self.needs_redraw = true;
                false
            }
            WindowEvent::Focused(focused) => {
                if !focused {
                    self.modifiers = None;
                    self.native_touches.clear();
                }
                self.event_queue
                    .push_back(InputEvent::HostFocus { focused });
                false
            }
            WindowEvent::Occluded(occluded) => {
                if !occluded {
                    self.needs_redraw = true;
                }
                if occluded {
                    self.native_touches.clear();
                }
                self.event_queue
                    .push_back(InputEvent::HostVisibility { visible: !occluded });
                false
            }
            WindowEvent::CloseRequested => {
                self.event_queue.push_back(InputEvent::Quit);
                true
            }
            WindowEvent::Resized(new_size) => {
                self.needs_redraw = true;
                if let Some(state) = self.state.as_mut()
                    && state.check().is_ok()
                {
                    state.config.width = new_size.width.max(1);
                    state.config.height = new_size.height.max(1);
                    state.surface.configure(&state.device, &state.config);
                    state.msaa = create_msaa(&state.device, &state.config, self.config.samples);
                }
                self.reconfigure_before_acquire = false;
                false
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.needs_redraw = true;
                if let Some(state) = self.state.as_mut()
                    && state.check().is_ok()
                {
                    let new_size = mobile_window_drawable_size(&state.window);
                    state.config.width = new_size.width.max(1);
                    state.config.height = new_size.height.max(1);
                    state.surface.configure(&state.device, &state.config);
                    state.msaa = create_msaa(&state.device, &state.config, self.config.samples);
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
            WindowEvent::CursorEntered { .. } => {
                self.event_queue.push_back(InputEvent::PointerEnter {
                    kind: crate::surface::PointerKind::Mouse,
                    id: 0,
                });
                false
            }
            WindowEvent::CursorLeft { .. } => {
                self.event_queue.push_back(InputEvent::PointerLeave {
                    kind: crate::surface::PointerKind::Mouse,
                    id: 0,
                });
                false
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    let (x, y) = self.last_cursor;
                    let event = match state {
                        ElementState::Pressed => InputEvent::PointerDown {
                            kind: crate::input::event::input::PointerKind::Mouse,
                            id: 0,
                            x,
                            y,
                        },
                        ElementState::Released => InputEvent::PointerUp {
                            kind: crate::input::event::input::PointerKind::Mouse,
                            id: 0,
                            x,
                            y,
                        },
                    };
                    self.event_queue.push_back(event);
                }
                false
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self
                    .state
                    .as_ref()
                    .map(|s| s.window.scale_factor())
                    .unwrap_or(1.0);
                let (dx, dy) = wheel_delta(delta, scale);
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
                        repeat,
                        ..
                    },
                ..
            } => self.keyboard_input(&logical_key, state, text.as_deref(), repeat),
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = Some(crate::input::event::input::KeyModifiers {
                    shift: state.shift_key(),
                    control: state.control_key(),
                    alt: state.alt_key(),
                    meta: state.super_key(),
                });
                false
            }
            WindowEvent::Ime(event) => {
                if let Some(event) = self.text_input.ime(event) {
                    self.event_queue.push_back(event);
                }
                false
            }
            _ => false,
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl ApplicationHandler for WgpuHandler {
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        self.runtime.finish_input_batch();
    }

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
        let result = event_loop
            .create_window(attrs)
            .map_err(|error| WgpuSurfaceError::Backend(error.to_string()))
            .and_then(|window| self.runtime.resume(Arc::new(window)));
        if let Err(error) = result {
            self.error = Some(error);
            event_loop.exit();
        }
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

fn create_wgpu_state(
    window: Arc<Window>,
    options: WgpuConfig,
) -> Result<WgpuState, WgpuSurfaceError> {
    options.validate()?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let surface = instance
        .create_surface(window.clone())
        .map_err(|error| WgpuSurfaceError::Backend(error.to_string()))?;
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: options.power_preference,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .map_err(|error| WgpuSurfaceError::Backend(error.to_string()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("mirui-wgpu-device"),
        required_features: adapter.features()
            & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
        required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
        ..Default::default()
    }))
    .map_err(|error| WgpuSurfaceError::Backend(error.to_string()))?;
    let context = WgpuDeviceContext::new(instance, adapter, device, queue);
    configure_wgpu_state(context, window, surface, options)
}

fn create_wgpu_state_with_context(
    context: Arc<WgpuDeviceContext>,
    window: Arc<Window>,
    options: WgpuConfig,
) -> Result<WgpuState, WgpuSurfaceError> {
    context
        .check()
        .map_err(|_| WgpuSurfaceError::Backend("device unavailable".into()))?;
    let surface = context
        .instance
        .create_surface(window.clone())
        .map_err(|error| WgpuSurfaceError::Backend(error.to_string()))?;
    configure_wgpu_state(context, window, surface, options)
}

fn configure_wgpu_state(
    context: Arc<WgpuDeviceContext>,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    options: WgpuConfig,
) -> Result<WgpuState, WgpuSurfaceError> {
    options.validate()?;
    let size = mobile_window_drawable_size(&window);
    let surface_caps = surface.get_capabilities(&context.adapter);
    let surface_format = surface_caps
        .formats
        .iter()
        .copied()
        .find(|format| !format.is_srgb())
        .or_else(|| surface_caps.formats.first().copied())
        .ok_or(WgpuSurfaceError::UnsupportedSurface)?;
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST;
    if !surface_caps.usages.contains(usage) {
        return Err(WgpuSurfaceError::UnsupportedSurface);
    }
    let format_features = context.format_features(surface_format);
    if !format_features
        .flags
        .sample_count_supported(options.samples)
        || (options.samples > 1
            && !format_features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE))
    {
        return Err(WgpuSurfaceError::UnsupportedSamples(options.samples));
    }
    let config = wgpu::SurfaceConfiguration {
        usage,
        format: surface_format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: options.select_present_mode(&surface_caps.present_modes)?,
        alpha_mode: *surface_caps
            .alpha_modes
            .first()
            .ok_or(WgpuSurfaceError::UnsupportedSurface)?,
        view_formats: alloc::vec![],
        desired_maximum_frame_latency: options.maximum_frame_latency,
    };
    surface.configure(&context.device, &config);
    let msaa = create_msaa(&context.device, &config, options.samples);
    Ok(WgpuState {
        context,
        window,
        surface,
        config,
        msaa,
    })
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
    /// Current device and window state, absent while the surface is suspended.
    pub fn state(&self) -> Option<&WgpuState> {
        self.handler.runtime.state.as_ref()
    }

    pub fn state_mut(&mut self) -> Option<&mut WgpuState> {
        self.handler.runtime.state.as_mut()
    }

    /// Open a window of the given logical size and stand up the wgpu
    /// device that backs it.
    pub fn new(title: &str, width: u16, height: u16) -> Result<Self, WgpuSurfaceError> {
        Self::with_config(title, width, height, WgpuConfig::default())
    }

    pub fn with_config(
        title: &str,
        width: u16,
        height: u16,
        config: WgpuConfig,
    ) -> Result<Self, WgpuSurfaceError> {
        config.validate()?;
        let event_loop =
            EventLoop::new().map_err(|error| WgpuSurfaceError::Backend(error.to_string()))?;
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut this = Self {
            event_loop,
            handler: WgpuHandler {
                title: title.to_string(),
                requested_size: (width as u32, height as u32),
                runtime: WgpuRuntime::new(config),
                error: None,
            },
            pumped_this_frame: false,
        };

        // winit creates windows from `resumed` only.
        let mut spins = 0;
        while this.handler.runtime.state.is_none() && this.handler.error.is_none() && spins < 100 {
            this.pump_once();
            spins += 1;
        }
        if let Some(error) = this.handler.error.take() {
            return Err(error);
        }
        if this.handler.runtime.state.is_none() {
            return Err(WgpuSurfaceError::Backend(
                "winit did not initialize the surface".into(),
            ));
        }
        Ok(this)
    }

    fn pump_once(&mut self) -> winit::platform::pump_events::PumpStatus {
        self.event_loop
            .pump_app_events(Some(Duration::ZERO), &mut self.handler)
    }
}

fn create_msaa(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    samples: u32,
) -> Option<wgpu::Texture> {
    (samples > 1).then(|| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mirui-wgpu-msaa"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
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

    fn frame_texture(&self) -> Option<&wgpu::SurfaceTexture> {
        self.handler.runtime.frame_texture.as_ref()
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl Surface for WgpuSurface {
    fn needs_redraw(&self) -> bool {
        self.handler.runtime.needs_redraw
    }
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

    fn flush(
        &mut self,
        _area: crate::types::PhysicalRect,
    ) -> Result<(), crate::render::RenderError> {
        Ok(())
    }

    fn begin_flush(&mut self) -> Result<(), crate::render::RenderError> {
        self.handler.runtime.begin_frame()
    }

    fn end_flush(&mut self) -> Result<(), crate::render::RenderError> {
        self.handler.runtime.present_frame()?;
        self.pumped_this_frame = false;
        Ok(())
    }

    fn abort_flush(&mut self) {
        self.handler.runtime.frame_texture = None;
        self.pumped_this_frame = false;
    }

    fn frame_end(&mut self) -> Result<(), crate::render::RenderError> {
        // Failed draws do not reach `flush`, but the next tick must still
        // pump winit so a temporarily unavailable drawable can recover.
        self.pumped_this_frame = false;
        Ok(())
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
        if let Some(e) = self.handler.runtime.poll_event() {
            return Some(e);
        }
        if self.pumped_this_frame {
            return None;
        }
        self.pumped_this_frame = true;

        self.pump_once();
        self.handler.runtime.poll_event()
    }

    fn set_text_input(&mut self, state: Option<crate::ui::text_input::TextInputState>) {
        self.handler.runtime.set_text_input(state);
    }

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    fn write_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardWrite,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        #[cfg(target_os = "macos")]
        {
            super::host_clipboard::write(request)
        }
        #[cfg(target_os = "windows")]
        {
            let Some(state) = self.handler.runtime.state.as_ref() else {
                return crate::ui::clipboard::ClipboardDispatch::Complete(
                    request.complete(Err(crate::ui::clipboard::ClipboardError::Unavailable)),
                );
            };
            super::host_clipboard::write(state.window.as_ref(), request)
        }
        #[cfg(target_os = "linux")]
        {
            let runtime = &mut self.handler.runtime;
            let Some(state) = runtime.state.as_ref() else {
                return crate::ui::clipboard::ClipboardDispatch::Complete(
                    request.complete(Err(crate::ui::clipboard::ClipboardError::Unavailable)),
                );
            };
            runtime.clipboard.write(state.window.as_ref(), request)
        }
    }

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    fn read_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardRead,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            super::host_clipboard::read(request)
        }
        #[cfg(target_os = "linux")]
        {
            let runtime = &mut self.handler.runtime;
            let Some(state) = runtime.state.as_ref() else {
                return crate::ui::clipboard::ClipboardDispatch::Complete(
                    request.complete(Err(crate::ui::clipboard::ClipboardError::Unavailable)),
                );
            };
            runtime.clipboard.read(state.window.as_ref(), request)
        }
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

    /// Finish the native event batch before the platform loop waits.
    fn finish_input_batch(&mut self);

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MobileSurfaceError {
    Allocation,
    BufferSizeOverflow,
    InvalidRenderScale,
    FramebufferBudget { required: usize, budget: usize },
    Gpu(WgpuSurfaceError),
}

impl From<WgpuSurfaceError> for MobileSurfaceError {
    fn from(error: WgpuSurfaceError) -> Self {
        Self::Gpu(error)
    }
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
    pub fn new(window: Arc<Window>) -> Result<Self, MobileSurfaceError> {
        Self::with_config(window, WgpuConfig::default())
    }

    pub fn with_config(
        window: Arc<Window>,
        config: WgpuConfig,
    ) -> Result<Self, MobileSurfaceError> {
        let mut runtime = WgpuRuntime::new(config);
        runtime.resume(window)?;
        Ok(Self { runtime })
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

    fn frame_texture(&self) -> Option<&wgpu::SurfaceTexture> {
        self.runtime.frame_texture.as_ref()
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
impl MobileSurface for MobileWgpuSurface {
    fn finish_input_batch(&mut self) {
        self.runtime.finish_input_batch();
    }

    fn resume_window(&mut self, window: Arc<Window>) -> Result<(), MobileSurfaceError> {
        self.runtime.resume(window)?;
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
    #[cfg(target_os = "ios")]
    fn write_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardWrite,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        super::host_clipboard::write(request)
    }

    #[cfg(target_os = "ios")]
    fn read_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardRead,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        super::host_clipboard::read(request)
    }

    fn needs_redraw(&self) -> bool {
        self.runtime.needs_redraw
    }
    fn begin_flush(&mut self) -> Result<(), crate::render::RenderError> {
        self.runtime.begin_frame()
    }

    fn end_flush(&mut self) -> Result<(), crate::render::RenderError> {
        self.runtime.present_frame()
    }

    fn abort_flush(&mut self) {
        self.runtime.frame_texture = None;
    }
    fn set_text_input(&mut self, state: Option<crate::ui::text_input::TextInputState>) {
        self.runtime.set_text_input(state);
    }

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

    fn flush(
        &mut self,
        _area: crate::types::PhysicalRect,
    ) -> Result<(), crate::render::RenderError> {
        Ok(())
    }

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
        self.runtime.poll_event()
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
    MobileHost::new(title, MobileWgpuSurface::new, build)
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
        let redraw = matches!(event, WindowEvent::RedrawRequested);
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
        if redraw {
            if app.is_suspended() {
                return;
            }
            app.draw_tick();
            app.maintain_tick();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(app) = self.app.as_mut() else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        app.backend.finish_input_batch();
        if app.is_suspended() {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        if app.tick_wait_ns().unwrap_or(0) == 0 {
            if app.update_tick() {
                event_loop.exit();
                return;
            }
            app.finish_tick(false);
        }
        if app.is_suspended() {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        match app.dirty_regions() {
            Ok(plan) if !plan.is_empty() => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            Err(error) => crate::warn!("mobile drawing preparation failed: {:?}", error),
            _ => {}
        }
        event_loop.set_control_flow(match app.tick_wait_ns() {
            Some(wait) if wait > 0 => ControlFlow::WaitUntil(
                std::time::Instant::now() + core::time::Duration::from_nanos(wait),
            ),
            _ => ControlFlow::Poll,
        });
    }
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
fn window_event_is_current<T: Eq>(active: Option<&T>, incoming: &T) -> bool {
    active.is_some_and(|active| active == incoming)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::App, ecs::Entity, input::event::focus::FocusState, ui::widgets::TextInput};

    #[test]
    fn platform_redraw_and_restore_remain_pending_until_acceptance() {
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        for event in [WindowEvent::RedrawRequested, WindowEvent::Occluded(false)] {
            runtime.needs_redraw = false;
            assert!(!runtime.window_event(event));
            assert!(runtime.needs_redraw);
            runtime.finish_input_batch();
            assert!(runtime.needs_redraw);
            assert_eq!(
                runtime.begin_frame(),
                Err(crate::render::RenderError::SurfaceUnavailable)
            );
            assert!(runtime.needs_redraw);
        }
    }

    type TestApp =
        App<crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>>;

    fn native_fixture() -> (TestApp, Entity, WgpuRuntime) {
        #[crate::compose]
        fn editor() -> Entity {
            crate::ui! {
                TextInput (width: 160, height: 36)
            }
        }
        let mut app = App::headless(200, 100);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        let editor = app.compose(root, editor());
        app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        runtime.set_text_input(crate::ui::text_input::native_state(&mut app.world, 0));
        (app, editor, runtime)
    }

    fn deliver(app: &mut TestApp, runtime: &mut WgpuRuntime) {
        while let Some(event) = runtime.poll_event() {
            crate::input::event::deliver_input(&mut app.world, None, &event, 0, 200, 100);
        }
    }

    fn display(app: &TestApp, editor: Entity, expected: &str) {
        app.world
            .get::<crate::ui::widgets::TextEditing>(editor)
            .unwrap()
            .read_display(|text| assert_eq!(text, expected));
    }

    #[test]
    fn host_state_preserves_input_order_without_requesting_suspend() {
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        runtime.pending_move = Some((Fixed::ONE, Fixed::ZERO));
        for event in [
            WindowEvent::Focused(false),
            WindowEvent::Occluded(true),
            WindowEvent::Focused(true),
            WindowEvent::Occluded(false),
        ] {
            assert!(!runtime.window_event(event));
        }
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::PointerMove { .. })
        ));
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::HostFocus { focused: false })
        ));
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::HostVisibility { visible: false })
        ));
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::HostFocus { focused: true })
        ));
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::HostVisibility { visible: true })
        ));
        assert!(runtime.poll_event().is_none());
    }

    #[test]
    fn pointer_moves_only_coalesce_between_ordered_input_boundaries() {
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        let device_id = winit::event::DeviceId::dummy();
        let moved = |x| WindowEvent::CursorMoved {
            device_id,
            position: winit::dpi::PhysicalPosition::new(x, 0.0),
        };
        let button = |state| WindowEvent::MouseInput {
            device_id,
            state,
            button: MouseButton::Left,
        };

        runtime.window_event(moved(1.0));
        runtime.window_event(moved(2.0));
        runtime.window_event(button(ElementState::Pressed));
        runtime.window_event(moved(3.0));
        runtime.window_event(moved(4.0));
        runtime.window_event(button(ElementState::Released));
        runtime.window_event(moved(5.0));
        runtime.window_event(WindowEvent::Focused(true));

        assert!(matches!(runtime.event_queue.pop_front(),
            Some(InputEvent::PointerMove { x, .. }) if x == Fixed::from_int(2)));
        assert!(matches!(runtime.event_queue.pop_front(),
            Some(InputEvent::PointerDown { x, .. }) if x == Fixed::from_int(2)));
        assert!(matches!(runtime.event_queue.pop_front(),
            Some(InputEvent::PointerMove { x, .. }) if x == Fixed::from_int(4)));
        assert!(matches!(runtime.event_queue.pop_front(),
            Some(InputEvent::PointerUp { x, .. }) if x == Fixed::from_int(4)));
        assert!(matches!(runtime.event_queue.pop_front(),
            Some(InputEvent::PointerMove { x, .. }) if x == Fixed::from_int(5)));
        assert!(matches!(
            runtime.event_queue.pop_front(),
            Some(InputEvent::HostFocus { focused: true })
        ));
        assert!(runtime.event_queue.is_empty());
        assert!(runtime.pending_move.is_none());
    }

    #[test]
    fn polling_retains_trailing_moves_and_reuses_queue_storage() {
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        runtime.event_queue.reserve(2);
        let allocations = crate::test_support::tracked_allocations(|| {
            for x in 0..8 {
                runtime.event_queue.push_back(InputEvent::PointerDown {
                    kind: crate::input::event::input::PointerKind::Mouse,
                    id: 0,
                    x: Fixed::from_int(x),
                    y: Fixed::ZERO,
                });
                runtime.pending_move = Some((Fixed::from_int(x + 1), Fixed::ZERO));
                assert!(matches!(
                    runtime.poll_event(),
                    Some(InputEvent::PointerDown { .. })
                ));
                assert!(matches!(runtime.poll_event(),
                    Some(InputEvent::PointerMove { x: next, .. }) if next == Fixed::from_int(x + 1)));
                assert!(runtime.poll_event().is_none());
            }
        });
        assert_eq!(allocations, 0);
    }

    #[test]
    fn cursor_boundaries_preserve_position_and_native_input_order() {
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        let device_id = winit::event::DeviceId::dummy();
        runtime.event_queue.reserve(4);
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                runtime.window_event(WindowEvent::CursorEntered { device_id });
                runtime.window_event(WindowEvent::CursorMoved {
                    device_id,
                    position: winit::dpi::PhysicalPosition::new(18.0, 24.0),
                });
                runtime.window_event(WindowEvent::CursorLeft { device_id });
                runtime.window_event(WindowEvent::MouseInput {
                    device_id,
                    state: ElementState::Released,
                    button: MouseButton::Left,
                });
            }),
            0
        );
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::PointerEnter {
                kind: crate::surface::PointerKind::Mouse,
                id: 0
            })
        ));
        assert!(
            matches!(runtime.poll_event(), Some(InputEvent::PointerMove { x, y, .. }) if x == Fixed::from_int(18) && y == Fixed::from_int(24))
        );
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::PointerLeave {
                kind: crate::surface::PointerKind::Mouse,
                id: 0
            })
        ));
        assert!(
            matches!(runtime.poll_event(), Some(InputEvent::PointerUp { x, y, .. }) if x == Fixed::from_int(18) && y == Fixed::from_int(24))
        );
        assert!(runtime.poll_event().is_none());
        assert_eq!(
            runtime.last_cursor,
            (Fixed::from_int(18), Fixed::from_int(24))
        );
    }

    #[test]
    fn keyboard_translation_preserves_pressed_and_released_states() {
        use crate::input::event::input::{
            KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_HOME, KEY_LEFT, KEY_RETURN, KEY_RIGHT,
        };

        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        for (key, expected) in [
            (NamedKey::Backspace, KEY_BACKSPACE),
            (NamedKey::Delete, KEY_DELETE),
            (NamedKey::ArrowLeft, KEY_LEFT),
            (NamedKey::ArrowRight, KEY_RIGHT),
            (NamedKey::Home, KEY_HOME),
            (NamedKey::End, KEY_END),
            (NamedKey::Enter, KEY_RETURN),
        ] {
            for state in [ElementState::Pressed, ElementState::Released] {
                assert!(!runtime.keyboard_input(&Key::Named(key), state, None, false));
                assert!(matches!(runtime.event_queue.pop_front(),
                    Some(InputEvent::Key { code, pressed, .. })
                    if code == expected && pressed == (state == ElementState::Pressed)));
                assert!(runtime.event_queue.is_empty());
            }
        }
    }

    #[test]
    fn keyboard_translation_does_not_duplicate_quit_or_text() {
        let (mut app, editor, mut runtime) = native_fixture();
        for key in [NamedKey::Escape, NamedKey::BrowserBack] {
            assert!(runtime.keyboard_input(&Key::Named(key), ElementState::Pressed, None, false));
            assert!(matches!(
                runtime.event_queue.pop_front(),
                Some(InputEvent::Quit)
            ));
            assert!(!runtime.keyboard_input(&Key::Named(key), ElementState::Released, None, false));
            assert!(runtime.event_queue.is_empty());
        }
        let key = Key::Character("A中🙂".into());
        assert!(!runtime.keyboard_input(&key, ElementState::Pressed, Some("A中🙂"), false));
        assert_eq!(runtime.event_queue.len(), 1);
        deliver(&mut app, &mut runtime);
        display(&app, editor, "A中🙂");
        assert!(!runtime.keyboard_input(&key, ElementState::Released, Some("A中🙂"), false));
        assert!(runtime.event_queue.is_empty());
    }

    #[test]
    fn keyboard_metadata_is_captured_in_event_order_and_cleared_on_focus_loss() {
        use crate::input::event::input::KeyModifiers;
        use winit::keyboard::ModifiersState;
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        let key = Key::Named(NamedKey::ArrowLeft);
        runtime.keyboard_input(&key, ElementState::Pressed, None, false);
        runtime.window_event(WindowEvent::ModifiersChanged(
            (ModifiersState::SHIFT | ModifiersState::SUPER).into(),
        ));
        runtime.keyboard_input(&key, ElementState::Pressed, None, true);
        runtime.window_event(WindowEvent::ModifiersChanged(
            ModifiersState::empty().into(),
        ));
        runtime.keyboard_input(&key, ElementState::Released, None, false);
        for (expected, repeated) in [
            (None, false),
            (
                Some(KeyModifiers {
                    shift: true,
                    meta: true,
                    ..Default::default()
                }),
                true,
            ),
            (Some(KeyModifiers::default()), false),
        ] {
            let Some(InputEvent::Key {
                modifiers, repeat, ..
            }) = runtime.poll_event()
            else {
                panic!("key")
            };
            assert_eq!(modifiers, expected);
            assert_eq!(repeat, Some(repeated));
        }
        runtime.window_event(WindowEvent::Focused(false));
        assert_eq!(runtime.modifiers, None);
        runtime.window_event(WindowEvent::ModifiersChanged(ModifiersState::ALT.into()));
        runtime.suspend();
        assert_eq!(runtime.modifiers, None);
    }

    #[test]
    fn suspend_discards_input_bound_to_the_old_native_window() {
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        runtime.event_queue.push_back(InputEvent::PointerDown {
            kind: crate::input::event::input::PointerKind::Mouse,
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
    fn suspend_retains_clipboard_receipts_until_the_app_consumes_them() {
        use crate::ui::clipboard::{self, ClipboardData, ClipboardRequest};
        let (mut app, editor, mut runtime) = native_fixture();
        clipboard::paste(&mut app.world, editor);
        let Some(ClipboardRequest::Read(request)) = clipboard::take_request(&mut app.world) else {
            panic!("expected pending clipboard read");
        };
        runtime.event_queue.push_back(InputEvent::Clipboard(
            request.complete(ClipboardData::text("retained")),
        ));
        runtime.suspend();
        let Some(InputEvent::Clipboard(response)) = runtime.poll_event() else {
            panic!("lost clipboard receipt");
        };
        clipboard::complete(&mut app.world, &response);
        display(&app, editor, "retained");
        assert!(runtime.poll_event().is_none());
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
            Some(InputEvent::PointerDown { kind: crate::input::event::input::PointerKind::Touch, id: 1, x: px, y: py }) if px == x && py == y
        ));
        assert!(
            touches
                .input_event(native, TouchPhase::Started, x, y)
                .is_none()
        );
        assert!(matches!(
            touches.input_event(native, TouchPhase::Moved, x, y),
            Some(InputEvent::PointerMove { kind: crate::input::event::input::PointerKind::Touch, id: 1, x: px, y: py }) if px == x && py == y
        ));
        assert!(matches!(
            touches.input_event(native, TouchPhase::Cancelled, x, y),
            Some(InputEvent::PointerCancel { kind: crate::input::event::input::PointerKind::Touch, id: 1, x: px, y: py }) if px == x && py == y
        ));
        assert!(matches!(
            touches.input_event(native + 1, TouchPhase::Started, x, y),
            Some(InputEvent::PointerDown { id: 1, .. })
        ));
    }

    #[test]
    fn native_touch_and_mouse_slots_stay_distinct_until_host_cancellation() {
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        let device_id = winit::event::DeviceId::dummy();
        let touch = |phase| {
            WindowEvent::Touch(winit::event::Touch {
                device_id,
                phase,
                location: winit::dpi::PhysicalPosition::new(7.0, 11.0),
                force: None,
                id: u64::MAX,
            })
        };
        runtime.window_event(touch(TouchPhase::Started));
        runtime.window_event(WindowEvent::MouseInput {
            device_id,
            state: ElementState::Pressed,
            button: MouseButton::Left,
        });
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::PointerDown { id: 1, .. })
        ));
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::PointerDown { id: 0, .. })
        ));
        runtime.window_event(WindowEvent::Focused(false));
        runtime.window_event(touch(TouchPhase::Moved));
        runtime.window_event(touch(TouchPhase::Ended));
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::HostFocus { focused: false })
        ));
        assert!(runtime.poll_event().is_none());
        runtime.window_event(touch(TouchPhase::Started));
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::PointerDown { id: 1, .. })
        ));
        runtime.window_event(WindowEvent::Occluded(true));
        runtime.window_event(touch(TouchPhase::Cancelled));
        assert!(matches!(
            runtime.poll_event(),
            Some(InputEvent::HostVisibility { visible: false })
        ));
        assert!(runtime.poll_event().is_none());
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
            Some(InputEvent::PointerMove { id: 2, .. })
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
    fn wheel_units_preserve_pixels_and_normalize_lines_independently_of_dpi() {
        use winit::dpi::PhysicalPosition;
        assert_eq!(
            wheel_delta(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.75, -0.375)),
                1.5
            ),
            (Fixed::from_ratio(-1, 2), Fixed::from_ratio(-1, 4))
        );
        assert_eq!(
            wheel_delta(MouseScrollDelta::LineDelta(0.125, -0.5), 2.0),
            (Fixed::from_ratio(-5, 2), Fixed::from_int(-10))
        );
    }

    #[test]
    fn selection_shortcuts_do_not_duplicate_reported_keyboard_text() {
        use crate::input::event::input::{KEY_A, KEY_C, KEY_V, KEY_X, KeyModifiers};
        let mut runtime = WgpuRuntime::new(WgpuConfig::default());
        for modifiers in [
            KeyModifiers {
                control: true,
                ..Default::default()
            },
            KeyModifiers {
                meta: true,
                ..Default::default()
            },
        ] {
            runtime.modifiers = Some(modifiers);
            for (text, code) in [("a", KEY_A), ("C", KEY_C), ("v", KEY_V), ("X", KEY_X)] {
                for state in [ElementState::Pressed, ElementState::Released] {
                    runtime.keyboard_input(&Key::Character(text.into()), state, Some(text), true);
                    assert!(
                        matches!(runtime.event_queue.pop_front(), Some(InputEvent::Key { code: actual, pressed, modifiers: Some(actual_modifiers), repeat: Some(true) }) if actual == code && pressed == (state == ElementState::Pressed) && actual_modifiers == modifiers)
                    );
                    assert!(runtime.event_queue.is_empty());
                }
            }
        }
    }

    #[test]
    fn native_preedit_and_commit_share_a_session_without_duplicate_keyboard_text() {
        let (mut app, editor, mut runtime) = native_fixture();
        runtime.window_event(WindowEvent::Ime(Ime::Enabled));
        runtime.window_event(WindowEvent::Ime(Ime::Preedit("ni".into(), Some((2, 2)))));
        deliver(&mut app, &mut runtime);
        display(&app, editor, "ni");
        assert!(!runtime.keyboard_input(
            &Key::Character("n".into()),
            ElementState::Pressed,
            Some("n"),
            false
        ));
        assert!(runtime.event_queue.is_empty());
        runtime.window_event(WindowEvent::Ime(Ime::Preedit("".into(), None)));
        runtime.window_event(WindowEvent::Ime(Ime::Commit("你".into())));
        runtime.finish_input_batch();
        assert_eq!(runtime.event_queue.len(), 2);
        deliver(&mut app, &mut runtime);
        display(&app, editor, "你");
        runtime.window_event(WindowEvent::Ime(Ime::Preedit("hao".into(), Some((3, 3)))));
        deliver(&mut app, &mut runtime);
        display(&app, editor, "你hao");
        runtime.window_event(WindowEvent::Ime(Ime::Preedit("".into(), None)));
        runtime.finish_input_batch();
        assert_eq!(runtime.event_queue.len(), 2);
        deliver(&mut app, &mut runtime);
        display(&app, editor, "你");
        assert!(!runtime.text_input.composing());
        runtime.window_event(WindowEvent::Ime(Ime::Preedit("x".into(), Some((1, 1)))));
        runtime.window_event(WindowEvent::Ime(Ime::Preedit("".into(), None)));
        runtime.keyboard_input(
            &Key::Character("a".into()),
            ElementState::Pressed,
            Some("a"),
            false,
        );
        runtime.finish_input_batch();
        assert_eq!(runtime.event_queue.len(), 3);
        deliver(&mut app, &mut runtime);
        display(&app, editor, "你a");
    }

    #[test]
    fn owned_commit_moves_into_the_prepared_queue_without_copying() {
        let (mut app, editor, mut runtime) = native_fixture();
        runtime.event_queue.reserve(1);
        runtime.window_event(WindowEvent::Ime(Ime::Enabled));
        let text = alloc::string::String::from("A中🙂");
        let allocations = crate::test_support::tracked_allocations(|| {
            runtime.window_event(WindowEvent::Ime(Ime::Commit(text)));
        });
        assert_eq!(allocations, 0);
        deliver(&mut app, &mut runtime);
        display(&app, editor, "A中🙂");
    }

    #[test]
    fn retired_ime_source_is_not_retagged_when_the_editor_session_changes() {
        let (mut app, editor, mut runtime) = native_fixture();
        runtime.window_event(WindowEvent::Ime(Ime::Enabled));
        runtime.window_event(WindowEvent::Ime(Ime::Commit("old".into())));
        app.world.resource_mut::<FocusState>().unwrap().focused = None;
        runtime.set_text_input(crate::ui::text_input::native_state(&mut app.world, 0));
        app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
        runtime.set_text_input(crate::ui::text_input::native_state(&mut app.world, 0));
        runtime.window_event(WindowEvent::Ime(Ime::Commit("late".into())));
        deliver(&mut app, &mut runtime);
        display(&app, editor, "");
        runtime.window_event(WindowEvent::Ime(Ime::Disabled));
        runtime.window_event(WindowEvent::Ime(Ime::Enabled));
        runtime.window_event(WindowEvent::Ime(Ime::Commit("new".into())));
        deliver(&mut app, &mut runtime);
        display(&app, editor, "new");
        runtime.set_text_input(None);
        runtime.keyboard_input(
            &Key::Character("x".into()),
            ElementState::Pressed,
            Some("x"),
            false,
        );
        assert!(runtime.poll_event().is_none());
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
