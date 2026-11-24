//! WGPU surfaces for desktop polling loops and native mobile event loops.

use alloc::collections::VecDeque;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use alloc::string::String;
use alloc::string::ToString;
use core::ops::Deref;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use core::time::Duration;
use std::sync::Arc;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::window::Window;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use winit::window::WindowId;

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
#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "ios")]
pub(crate) use ios::prepare_window;

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
    resize_revision: u64,
    #[cfg(target_os = "macos")]
    // Detach native responders before Runtime releases its window.
    menu: Option<super::native_menu::Registration>,
    title: String,
    requested_size: (u32, u32),
    runtime: WgpuRuntime,
    error: Option<WgpuSurfaceError>,
    #[cfg(target_os = "macos")]
    menu_generation: u64,
    #[cfg(target_os = "macos")]
    proxy: winit::event_loop::EventLoopProxy<HostEvent>,
}

#[cfg(target_os = "macos")]
struct HostEvent {
    generation: u64,
    action: super::native_menu::Action,
}
#[cfg(not(any(target_os = "macos", target_os = "android", target_os = "ios")))]
type HostEvent = ();

#[cfg(target_os = "macos")]
impl WgpuHandler {
    fn attach_menu(&mut self) -> Result<(), WgpuSurfaceError> {
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let error = || WgpuSurfaceError::Backend("native menu window unavailable".into());
        let mtm = objc2::MainThreadMarker::new().ok_or_else(error)?;
        let window = &self.runtime.state.as_ref().ok_or_else(error)?.window;
        let handle = window.window_handle().map_err(|_| error())?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err(error());
        };
        // The window owns this live NSView; Registration retains and detaches it before teardown.
        let view = unsafe {
            objc2::rc::Retained::<objc2_app_kit::NSResponder>::retain(
                handle.ns_view.as_ptr().cast(),
            )
        }
        .ok_or_else(error)?;
        self.menu_generation = self.menu_generation.checked_add(1).ok_or_else(error)?;
        let generation = self.menu_generation;
        let proxy = self.proxy.clone();
        self.menu = Some(super::native_menu::Registration::new(
            &view,
            move |action| {
                if proxy.send_event(HostEvent { generation, action }).is_err() {
                    crate::warn!(target: "mirui::clipboard", "native menu event loop unavailable");
                }
            },
            mtm,
        ));
        Ok(())
    }
}

pub(crate) struct WgpuRuntime {
    #[cfg(target_os = "android")]
    pub(crate) clipboard: super::host_clipboard::android::Clipboard,
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
        #[cfg(any(target_os = "linux", target_os = "android"))]
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
        physical: Option<crate::input::event::input::PhysicalKey>,
    ) -> bool {
        use crate::input::event::input::{
            KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_ESCAPE, KEY_HOME, KEY_LEFT, KEY_RETURN,
            KEY_RIGHT, KEY_UNKNOWN,
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
            Key::Named(NamedKey::Escape) => Some(KEY_ESCAPE),
            Key::Named(NamedKey::BrowserBack) => {
                if pressed {
                    self.event_queue.push_back(InputEvent::Quit);
                }
                return pressed;
            }
            _ => shortcut,
        };
        if code.is_some() || physical.is_some() {
            self.event_queue.push_back(InputEvent::Key {
                physical,
                code: code.unwrap_or(KEY_UNKNOWN),
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
            #[cfg(any(target_os = "linux", target_os = "android"))]
            clipboard: Default::default(),
            reconfigure_before_acquire: false,
            frame_texture: None,
            needs_redraw: true,
        }
    }

    pub(crate) fn resume(&mut self, window: Arc<Window>) -> Result<(), WgpuSurfaceError> {
        if self.state.is_some() {
            self.needs_redraw = true;
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

    pub(crate) fn suspend(&mut self, retain_window: bool) {
        self.frame_texture = None;
        self.set_text_input(None);
        self.modifiers = None;
        #[cfg(any(target_os = "linux", target_os = "android"))]
        self.clipboard.detach();
        if !retain_window {
            self.state = None;
            self.reconfigure_before_acquire = false;
        }
        self.event_queue.retain(|event| {
            matches!(event, InputEvent::Clipboard(_))
                || (retain_window
                    && matches!(
                        event,
                        InputEvent::HostFocus { .. } | InputEvent::HostVisibility { .. }
                    ))
        });
        self.pending_move = None;
        self.native_touches.clear();
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
            let state = self.state.as_ref().ok_or(RenderError::SurfaceUnavailable)?;
            let acquired = {
                // Acquisition does not expose the native request that may block it.
                let _wait = crate::core::perf::wait("wgpu.surface.acquire", None);
                state.surface.get_current_texture()
            };
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
        let submission = crate::core::perf::submission("wgpu.present");
        frame.present();
        submission.finish(crate::core::perf::SubmissionOutcome::Issued);
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
                        physical_key,
                        text,
                        state,
                        repeat,
                        ..
                    },
                ..
            } => self.keyboard_input(
                &logical_key,
                state,
                text.as_deref(),
                repeat,
                super::keyboard::from_winit(physical_key),
            ),
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
impl ApplicationHandler<HostEvent> for WgpuHandler {
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
        #[cfg(target_os = "macos")]
        let result = result.and_then(|()| self.attach_menu());
        if result.is_ok() {
            self.resize_revision = self.resize_revision.wrapping_add(1);
        }
        if let Err(error) = result {
            self.error = Some(error);
            event_loop.exit();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.runtime.state {
            let size = state
                .window
                .inner_size()
                .to_logical::<u32>(state.window.scale_factor());
            self.requested_size = (size.width, size.height);
        }
        #[cfg(target_os = "macos")]
        self.menu.take();
        self.runtime.suspend(false);
    }

    #[cfg(target_os = "macos")]
    fn user_event(&mut self, _: &ActiveEventLoop, event: HostEvent) {
        if self.menu.is_some() && event.generation == self.menu_generation {
            self.runtime.flush_pointer_move();
            self.runtime
                .event_queue
                .push_back(InputEvent::ClipboardInput(event.action.input()));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(
            event,
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
        ) {
            self.resize_revision = self.resize_revision.wrapping_add(1);
        }
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
    size_host: Option<super::SurfaceSizeHost>,
    pending_size: Option<(super::SurfaceSizeRequest, u64, WindowId)>,
    observed_size_revision: u64,
    event_loop: EventLoop<HostEvent>,
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
        let event_loop = EventLoop::<HostEvent>::with_user_event()
            .build()
            .map_err(|error| WgpuSurfaceError::Backend(error.to_string()))?;
        event_loop.set_control_flow(ControlFlow::Poll);
        #[cfg(target_os = "macos")]
        let proxy = event_loop.create_proxy();

        let mut this = Self {
            size_host: None,
            pending_size: None,
            observed_size_revision: 0,
            event_loop,
            handler: WgpuHandler {
                resize_revision: 0,
                title: title.to_string(),
                requested_size: (width as u32, height as u32),
                runtime: WgpuRuntime::new(config),
                error: None,
                #[cfg(target_os = "macos")]
                menu: None,
                #[cfg(target_os = "macos")]
                menu_generation: 0,
                #[cfg(target_os = "macos")]
                proxy,
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
        let result = self
            .event_loop
            .pump_app_events(Some(Duration::ZERO), &mut self.handler);
        self.observe_size_feedback();
        result
    }

    fn observe_size_feedback(&mut self) {
        if self.size_host.is_none() {
            return;
        }
        if self.pending_size.is_none()
            && self.observed_size_revision == self.handler.resize_revision
        {
            return;
        }
        let Some(state) = self.state() else {
            return;
        };
        let window_id = state.window.id();
        let physical = mobile_window_drawable_size(&state.window);
        let reconfigure = physical.width > 0
            && physical.height > 0
            && (state.config.width != physical.width || state.config.height != physical.height);
        if reconfigure {
            self.handler
                .runtime
                .window_event(WindowEvent::Resized(physical));
        }
        let info = self.display_info();
        self.observed_size_revision = self.handler.resize_revision;
        let host = self.size_host.as_ref().unwrap();
        host.observe_size(info.width, info.height);
        if let Some((request, revision, original_window)) = self.pending_size {
            if original_window != window_id {
                self.pending_size = None;
                host.defer(request);
            } else if request.size() == (info.width, info.height)
                || revision != self.handler.resize_revision
            {
                self.pending_size = None;
                host.complete(request, Ok((info.width, info.height)));
            }
        }
    }

    fn service_size_request(&mut self) {
        if self.size_host.is_none() {
            return;
        }
        if self.pending_size.is_some() {
            return;
        }
        let Some(state) = self.state() else {
            return;
        };
        if state.window.is_minimized() == Some(true) {
            return;
        }
        let Some(host) = &self.size_host else {
            return;
        };
        let Some(request) = host.take_request() else {
            return;
        };
        if state.window.fullscreen().is_some() || state.window.is_maximized() {
            host.complete(request, Err(super::SurfaceSizeError::Restricted));
            return;
        }
        let window_id = state.window.id();
        let scale = self.display_info().scale;
        let size = request.size();
        let factor = scale.to_int().max(1) as u32;
        let physical =
            winit::dpi::PhysicalSize::new(u32::from(size.0) * factor, u32::from(size.1) * factor);
        let limit = state
            .device
            .limits()
            .max_texture_dimension_2d
            .min(u32::from(u16::MAX));
        if physical.width > limit || physical.height > limit {
            host.complete(request, Err(super::SurfaceSizeError::Restricted));
            return;
        }
        let actual = state.window.request_inner_size(physical);
        if let Some(actual) = actual {
            self.handler
                .runtime
                .window_event(WindowEvent::Resized(actual));
            let info = self.display_info();
            self.size_host
                .as_ref()
                .unwrap()
                .complete(request, Ok((info.width, info.height)));
        } else {
            self.pending_size = Some((request, self.handler.resize_revision, window_id));
        }
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
    fn size_control(&mut self) -> Option<super::SurfaceSizeControl> {
        let state = self.state()?;
        if state.window.fullscreen().is_some() {
            return None;
        }
        let info = self.display_info();
        Some(
            self.size_host
                .get_or_insert_with(|| super::SurfaceSizeHost::new(info.width, info.height))
                .control(),
        )
    }
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
        self.service_size_request();
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
    /// Resumes presentation on a new or retained window and refreshes its drawable geometry.
    fn resume_window(&mut self, window: Arc<Window>) -> Result<(), MobileSurfaceError>;

    /// Pauses native input and presentation. iOS windows remain valid during inactivity; Android native surfaces must be released before returning.
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
        self.runtime.suspend(cfg!(target_os = "ios"));
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
    #[cfg(target_os = "android")]
    fn write_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardWrite,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        self.runtime.clipboard.write(request)
    }

    #[cfg(target_os = "android")]
    fn read_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardRead,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        self.runtime.clipboard.read(request)
    }

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
pub use crate::app::mobile_host::{MobileHost, wgpu_mobile_host};

#[cfg(any(target_os = "android", target_os = "ios", test))]
pub(crate) fn window_event_is_deliverable<T: Eq>(
    active: Option<&T>,
    incoming: &T,
    suspended: bool,
    event: &WindowEvent,
) -> bool {
    active.is_some_and(|active| active == incoming)
        && (!suspended || matches!(event, WindowEvent::Focused(_) | WindowEvent::Occluded(_)))
}

#[cfg(test)]
#[path = "../../tests/unit/surface/wgpu_surface/tests.rs"]
mod tests;
