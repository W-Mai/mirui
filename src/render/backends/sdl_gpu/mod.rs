//! GPU-accelerated SDL backend.
//!
//! Where [`crate::surface::sdl::SdlSurface`] keeps a CPU byte buffer and uploads it
//! each frame, `SdlGpuSurface` drives the SDL2 accelerated renderer
//! directly: `canvas.fill_rect`, `canvas.copy`, and ultimately
//! `SDL_RenderGeometry` (unsafe FFI) for tessellated paths. No CPU
//! framebuffer, no `FramebufferAccess` impl.

mod blit;
mod config;
mod label;
mod label_cache;
mod line;
mod path;
mod quad;
mod rect_fill;
mod rect_stroke;
mod tessellation;

use std::time::Instant;

use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::render::Canvas as SdlCanvas;
use sdl2::video::Window;

pub use self::config::{SdlGpuConfig, SdlGpuDriver, SdlGpuError, SdlWindowMode};
use self::label_cache::LabelCache;
use self::tessellation::TessellationCache;
use crate::render::PlaneRequirements;
use crate::render::canvas::{Canvas, Paint};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::factory::RendererFactory;
use crate::render::path::Path;
use crate::render::projective_fallback::{ProjectiveFallback, ProjectiveFallbackPlan};
use crate::render::raster::{StrokeScratch, StrokeSpec};
use crate::render::renderer::{
    DrawRequest, FallbackRegion, ProjectiveDrawError, RenderError, RenderFeature, RenderRoute,
    Renderer,
};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{
    Color, Fixed, Fixed64, PhysicalRect, Point, Rect, Transform, Transform3D, Viewport,
};

use crate::core::cache::{CacheInspect, InspectCaches};
use crate::surface::{DisplayInfo, InputEvent, Surface};

fn paint_color(paint: &Paint) -> Color {
    match paint {
        Paint::Color(color) => (*color).into(),
        Paint::LinearGradient(gradient) => gradient
            .stops
            .first()
            .map(|stop| stop.color.into())
            .unwrap_or(Color::rgba(0, 0, 0, 0)),
        Paint::RadialGradient(gradient) => gradient
            .stops
            .first()
            .map(|stop| stop.color.into())
            .unwrap_or(Color::rgba(0, 0, 0, 0)),
    }
}

/// macOS trackpad pinch / rotate is delivered by SDL as `MultiGesture`,
/// which has no "end" sentinel; if no `MultiGesture` arrives within
/// this window we synthesize PointerUp for the two virtual fingers.
const MULTI_GESTURE_TIMEOUT_MS: u128 = 50;
/// Half-distance between the two virtual fingers when the gesture
/// starts. The recognizer only sees relative scale, so the absolute
/// value doesn't matter for correctness.
const INITIAL_DIST_FRAC: f32 = 0.05;
const VIRT_FINGER_A: u8 = 1;
const VIRT_FINGER_B: u8 = 2;

#[derive(Default)]
struct MultiGestureState {
    active: bool,
    f_a: (f32, f32),
    f_b: (f32, f32),
    last_event: Option<Instant>,
}

pub struct SdlGpuSurface {
    canvas: SdlCanvas<Window>,
    label_cache: LabelCache,
    _event_pump: EventPump,
    text_input: crate::surface::sdl_events::NativeTextInput,
    tessellator: TessellationCache,
    viewport: Viewport,
    last_mouse_x: i32,
    last_mouse_y: i32,
    multi: MultiGestureState,
    pending: alloc::collections::VecDeque<InputEvent>,
}

impl SdlGpuSurface {
    /// Creates a windowed OpenGL surface with 4× MSAA and vsync.
    pub fn new(title: &str, width: u16, height: u16) -> Result<Self, SdlGpuError> {
        Self::with_config(title, width, height, SdlGpuConfig::default())
    }

    /// Creates a surface using the requested accelerated driver and actual window geometry.
    pub fn with_config(
        title: &str,
        width: u16,
        height: u16,
        config: SdlGpuConfig,
    ) -> Result<Self, SdlGpuError> {
        use self::config::backend_error;
        if width == 0 || height == 0 {
            return Err(SdlGpuError::InvalidConfig(
                "window dimensions must be nonzero",
            ));
        }
        config.driver.validate()?;
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");
        let sdl = crate::surface::sdl_events::init().map_err(|e| backend_error("SDL init", e))?;
        let video = sdl.video().map_err(|e| backend_error("SDL video", e))?;
        let driver = config
            .driver
            .select(sdl2::render::drivers().map(|driver| driver.name))?;
        config.driver.configure_gl()?;
        let mut window = video.window(title, u32::from(width), u32::from(height));
        window.position_centered().allow_highdpi();
        if driver.is_some() {
            window.opengl();
        }
        match config.window {
            SdlWindowMode::Windowed { resizable: true } => {
                window.resizable();
            }
            SdlWindowMode::Windowed { resizable: false } => {}
            SdlWindowMode::FullscreenDesktop => {
                window.fullscreen_desktop();
            }
        }
        let window = window.build().map_err(|e| backend_error("SDL window", e))?;
        let mut canvas_builder = window.into_canvas().accelerated();
        if let Some(index) = driver {
            canvas_builder = canvas_builder.index(index);
        }
        if config.vsync {
            canvas_builder = canvas_builder.present_vsync();
        }
        let canvas = canvas_builder
            .build()
            .map_err(|e| backend_error("SDL renderer", e))?;
        if config
            .driver
            .name()
            .is_some_and(|name| canvas.info().name != name)
        {
            return Err(SdlGpuError::UnsupportedDriver(config.driver));
        }
        config.driver.verify_gl()?;
        let viewport = self::config::viewport(
            canvas.window().size(),
            canvas
                .output_size()
                .map_err(|e| backend_error("SDL output size", e))?,
        )?;
        let texture_creator = canvas.texture_creator();
        let event_pump = sdl
            .event_pump()
            .map_err(|e| backend_error("SDL event pump", e))?;
        let text_input = crate::surface::sdl_events::NativeTextInput::new(
            &video,
            sdl.event().map_err(|e| backend_error("SDL events", e))?,
        );
        Ok(Self {
            canvas,
            label_cache: LabelCache::new(texture_creator),
            _event_pump: event_pump,
            text_input,
            tessellator: TessellationCache::new(),
            viewport,
            last_mouse_x: 0,
            last_mouse_y: 0,
            multi: MultiGestureState::default(),
            pending: alloc::collections::VecDeque::new(),
        })
    }

    /// The accelerated renderer selected by SDL.
    pub fn renderer_name(&self) -> &'static str {
        self.canvas.info().name
    }

    fn refresh_viewport(&mut self) -> Result<(), SdlGpuError> {
        let logical = self.canvas.window().size();
        let physical = self
            .canvas
            .output_size()
            .map_err(|e| config::backend_error("SDL output size", e))?;
        if logical.0 == 0 || logical.1 == 0 || physical.0 == 0 || physical.1 == 0 {
            return Ok(());
        }
        self.viewport = config::viewport(logical, physical)?;
        Ok(())
    }

    fn handle_multi_gesture(
        &mut self,
        cx: f32,
        cy: f32,
        d_theta: f32,
        d_dist: f32,
        win_w: f32,
        win_h: f32,
    ) {
        if !self.multi.active {
            let half = INITIAL_DIST_FRAC * win_w.min(win_h) / 2.0;
            self.multi.f_a = (cx - half, cy);
            self.multi.f_b = (cx + half, cy);
            self.multi.active = true;
            self.multi.last_event = Some(Instant::now());

            let (ax, ay) = self.multi.f_a;
            let (bx, by) = self.multi.f_b;
            self.pending.push_back(InputEvent::PointerDown {
                kind: crate::input::event::input::PointerKind::Touch,
                id: VIRT_FINGER_A,
                x: Fixed::from(ax as i32),
                y: Fixed::from(ay as i32),
            });
            self.pending.push_back(InputEvent::PointerDown {
                kind: crate::input::event::input::PointerKind::Touch,
                id: VIRT_FINGER_B,
                x: Fixed::from(bx as i32),
                y: Fixed::from(by as i32),
            });
            return;
        }

        rotate_scale_around(&mut self.multi.f_a, cx, cy, d_theta, 1.0 + d_dist);
        rotate_scale_around(&mut self.multi.f_b, cx, cy, d_theta, 1.0 + d_dist);
        let mid = (
            (self.multi.f_a.0 + self.multi.f_b.0) / 2.0,
            (self.multi.f_a.1 + self.multi.f_b.1) / 2.0,
        );
        let dx = cx - mid.0;
        let dy = cy - mid.1;
        self.multi.f_a.0 += dx;
        self.multi.f_a.1 += dy;
        self.multi.f_b.0 += dx;
        self.multi.f_b.1 += dy;

        self.multi.last_event = Some(Instant::now());

        let (ax, ay) = self.multi.f_a;
        let (bx, by) = self.multi.f_b;
        self.pending.push_back(InputEvent::PointerMove {
            kind: crate::input::event::input::PointerKind::Touch,
            id: VIRT_FINGER_A,
            x: Fixed::from(ax as i32),
            y: Fixed::from(ay as i32),
        });
        self.pending.push_back(InputEvent::PointerMove {
            kind: crate::input::event::input::PointerKind::Touch,
            id: VIRT_FINGER_B,
            x: Fixed::from(bx as i32),
            y: Fixed::from(by as i32),
        });
    }

    fn end_multi_gesture(&mut self) {
        if !self.multi.active {
            return;
        }
        let (ax, ay) = self.multi.f_a;
        let (bx, by) = self.multi.f_b;
        self.pending.push_back(InputEvent::PointerUp {
            kind: crate::input::event::input::PointerKind::Touch,
            id: VIRT_FINGER_A,
            x: Fixed::from(ax as i32),
            y: Fixed::from(ay as i32),
        });
        self.pending.push_back(InputEvent::PointerUp {
            kind: crate::input::event::input::PointerKind::Touch,
            id: VIRT_FINGER_B,
            x: Fixed::from(bx as i32),
            y: Fixed::from(by as i32),
        });
        self.multi.active = false;
        self.multi.last_event = None;
    }

    pub(crate) fn parts_mut(
        &mut self,
    ) -> (
        &mut SdlCanvas<Window>,
        &mut LabelCache,
        &mut TessellationCache,
    ) {
        (
            &mut self.canvas,
            &mut self.label_cache,
            &mut self.tessellator,
        )
    }
}

fn rotate_scale_around(p: &mut (f32, f32), cx: f32, cy: f32, theta: f32, scale: f32) {
    let dx = p.0 - cx;
    let dy = p.1 - cy;
    let s = theta.sin();
    let c = theta.cos();
    let rx = (dx * c - dy * s) * scale;
    let ry = (dx * s + dy * c) * scale;
    p.0 = cx + rx;
    p.1 = cy + ry;
}

impl Surface for SdlGpuSurface {
    fn display_info(&self) -> DisplayInfo {
        let (lw, lh) = self.viewport.logical_size();
        DisplayInfo {
            width: lw,
            height: lh,
            scale: self.viewport.scale(),
            format: ColorFormat::RGBA8888,
        }
    }

    fn physical_size(&self) -> (u32, u32) {
        let (width, height) = self.viewport.physical_size();
        (u32::from(width), u32::from(height))
    }

    fn viewport(&self) -> Viewport {
        self.viewport
    }

    fn flush(&mut self, _area: PhysicalRect) {}

    fn end_flush(&mut self) {
        self.canvas.present();
    }

    fn set_text_input(&mut self, state: Option<crate::ui::text_input::TextInputState>) {
        self.text_input.set(state);
    }

    fn write_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardWrite,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        crate::surface::sdl_events::write_clipboard(self.canvas.window().subsystem(), request)
    }

    fn read_clipboard(
        &mut self,
        request: crate::ui::clipboard::ClipboardRead,
    ) -> crate::ui::clipboard::ClipboardDispatch {
        crate::surface::sdl_events::read_clipboard(self.canvas.window().subsystem(), request)
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        if let Some(e) = self.pending.pop_front() {
            return Some(e);
        }
        if self.multi.active {
            if let Some(t) = self.multi.last_event {
                if t.elapsed().as_millis() > MULTI_GESTURE_TIMEOUT_MS {
                    self.end_multi_gesture();
                    return self.pending.pop_front();
                }
            }
        }
        while let Some(event) = crate::surface::sdl_events::poll() {
            match event {
                Event::Quit { .. } => self.pending.push_back(InputEvent::Quit),
                event @ (Event::KeyDown { .. } | Event::KeyUp { .. }) => {
                    if let Some(input) = crate::surface::sdl_events::keyboard_input(&event) {
                        self.pending.push_back(input);
                    }
                }
                Event::MouseButtonDown { x, y, .. } => {
                    self.pending.push_back(InputEvent::PointerDown {
                        kind: crate::input::event::input::PointerKind::Mouse,
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseButtonUp { x, y, .. } => {
                    self.pending.push_back(InputEvent::PointerUp {
                        kind: crate::input::event::input::PointerKind::Mouse,
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseMotion { x, y, .. } => {
                    self.last_mouse_x = x;
                    self.last_mouse_y = y;
                    self.pending.push_back(InputEvent::PointerMove {
                        kind: crate::input::event::input::PointerKind::Mouse,
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseWheel {
                    precise_x,
                    precise_y,
                    ..
                } => {
                    self.pending
                        .push_back(crate::surface::sdl_events::wheel_input(
                            precise_x,
                            precise_y,
                            self.last_mouse_x,
                            self.last_mouse_y,
                        ));
                }
                Event::MultiGesture {
                    d_theta,
                    d_dist,
                    x,
                    y,
                    ..
                } => {
                    let (width, height) = self.viewport.logical_size();
                    let win_w = f32::from(width);
                    let win_h = f32::from(height);
                    let cx = x * win_w;
                    let cy = y * win_h;
                    self.handle_multi_gesture(cx, cy, d_theta, d_dist, win_w, win_h);
                }
                Event::TextInput { text, .. } => {
                    if let Some(input) = self.text_input.commit(text) {
                        self.pending.push_back(input);
                    }
                }
                Event::TextEditing {
                    text,
                    start,
                    length,
                    ..
                } => {
                    if let Some(input) = self.text_input.preedit(text, start, length) {
                        self.pending.push_back(input);
                    }
                }
                Event::Window { win_event, .. } => {
                    use sdl2::event::WindowEvent;
                    if matches!(
                        win_event,
                        WindowEvent::SizeChanged(..)
                            | WindowEvent::Resized(..)
                            | WindowEvent::DisplayChanged(..)
                            | WindowEvent::Restored
                            | WindowEvent::Shown
                    ) && let Err(error) = self.refresh_viewport()
                    {
                        crate::warn!("SDL GPU geometry update failed: {error}");
                        return Some(InputEvent::Quit);
                    }
                    if let Some(input) = crate::surface::sdl_events::pointer_window_input(win_event)
                        .or_else(|| crate::surface::sdl_events::host_window_input(win_event))
                    {
                        self.pending.push_back(input);
                    }
                }
                _ => {}
            }
            if let Some(input) = self.pending.pop_front() {
                return Some(input);
            }
        }
        self.text_input.idle()
    }

    fn persistence(&self) -> crate::surface::BackbufferPersistence {
        crate::surface::BackbufferPersistence::Transient
    }
}

// NOTE: no `impl FramebufferAccess for SdlGpuSurface` — by design.

type CacheAccessor = fn(&SdlGpuSurface) -> &dyn CacheInspect;

impl InspectCaches for SdlGpuSurface {
    fn inspect_caches(&self) -> impl Iterator<Item = (&'static str, &dyn CacheInspect)> + '_ {
        const ENTRIES: &[CacheAccessor] = &[
            |s: &SdlGpuSurface| s.label_cache.as_inspect(),
            |s: &SdlGpuSurface| s.label_cache.scalar_inspect(),
        ];
        ENTRIES.iter().map(move |f| {
            let c = f(self);
            (c.cache_name(), c)
        })
    }
}

pub struct SdlGpuFactory<S = Box<[u8]>> {
    projective_fallback: Option<ProjectiveFallback<S>>,
    stroke_scratch: StrokeScratch,
}

impl SdlGpuFactory<Box<[u8]>> {
    pub fn new() -> Self {
        Self {
            projective_fallback: None,
            stroke_scratch: StrokeScratch::new(),
        }
    }

    pub fn with_projective_fallback<S>(self, fallback: ProjectiveFallback<S>) -> SdlGpuFactory<S> {
        SdlGpuFactory {
            projective_fallback: Some(fallback),
            stroke_scratch: self.stroke_scratch,
        }
    }
}

impl Default for SdlGpuFactory<Box<[u8]>> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> RendererFactory<SdlGpuSurface> for SdlGpuFactory<S> {
    type Renderer<'a>
        = SdlGpuRenderer<'a, S>
    where
        Self: 'a;

    fn make<'a>(
        &'a mut self,
        backend: &'a mut SdlGpuSurface,
        transform: &Viewport,
    ) -> SdlGpuRenderer<'a, S> {
        let viewport = *transform;
        let (canvas, label_cache, tessellator) = backend.parts_mut();
        SdlGpuRenderer {
            canvas,
            label_cache,
            tessellator,
            projective_fallback: self.projective_fallback.as_mut(),
            stroke_scratch: &mut self.stroke_scratch,
            viewport,
            draw_failed: false,
        }
    }
}

pub struct SdlGpuRenderer<'a, S = Box<[u8]>> {
    canvas: &'a mut SdlCanvas<Window>,
    label_cache: &'a mut LabelCache,
    tessellator: &'a mut TessellationCache,
    projective_fallback: Option<&'a mut ProjectiveFallback<S>>,
    stroke_scratch: &'a mut StrokeScratch,
    viewport: Viewport,
    draw_failed: bool,
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    fn physical_clip_rect(&self, src: &Rect) -> Option<PhysicalRect> {
        let (pw, ph) = self.viewport.physical_size();
        self.viewport
            .physical_rect_in(*src, u32::from(pw), u32::from(ph))
    }

    pub(super) fn submit_geometry(&mut self, phys_clip: &Rect, needs_blend: bool) {
        if self.tessellator.indices.is_empty() {
            return;
        }
        let Some(clip_rect) = sdl_pixel_rect(phys_clip, phys_clip) else {
            return;
        };
        self.canvas.set_clip_rect(clip_rect);
        self.canvas.set_blend_mode(if needs_blend {
            sdl2::render::BlendMode::Blend
        } else {
            sdl2::render::BlendMode::None
        });

        let verts = &self.tessellator.verts;
        let indices = &self.tessellator.indices;
        let result = unsafe {
            sdl2_sys::SDL_RenderGeometry(
                self.canvas.raw(),
                core::ptr::null_mut(),
                verts.as_ptr(),
                verts.len() as _,
                indices.as_ptr(),
                indices.len() as _,
            )
        };
        if result != 0 {
            self.draw_failed = true;
        }
        self.canvas.set_clip_rect(None);
    }

    fn draw_projective_plan(
        &mut self,
        plan: ProjectiveFallbackPlan,
        command: &DrawCommand,
        projective: &Transform3D,
    ) -> Result<(), ProjectiveDrawError> {
        if plan.width() == 0 || plan.height() == 0 {
            return Ok(());
        }
        let fallback = self
            .projective_fallback
            .as_deref_mut()
            .ok_or(ProjectiveDrawError::MissingFallbackStorage)?;
        let read_rect = sdl2_sys::SDL_Rect {
            x: i32::from(plan.x()),
            y: i32::from(plan.y()),
            w: i32::from(plan.width()),
            h: i32::from(plan.height()),
        };
        let target = fallback.target_mut(plan);
        let read_result = unsafe {
            sdl2_sys::SDL_RenderReadPixels(
                self.canvas.raw(),
                &read_rect,
                sdl2_sys::SDL_PixelFormatEnum::SDL_PIXELFORMAT_RGBA32 as u32,
                target.as_mut_ptr().cast(),
                i32::try_from(plan.region().stride_bytes())
                    .map_err(|_| ProjectiveDrawError::InvalidProjection)?,
            )
        };
        if read_result != 0 {
            return Err(ProjectiveDrawError::BackendFailure);
        }

        fallback.render(plan, command, projective, self.viewport)?;
        let target = fallback.target(plan);
        let canvas = &mut *self.canvas;
        let mut uploaded = false;
        self.label_cache.with_creator(|creator| {
            let Ok(mut texture) = creator.create_texture_streaming(
                sdl2::pixels::PixelFormatEnum::RGBA32,
                u32::from(plan.width()),
                u32::from(plan.height()),
            ) else {
                return;
            };
            if texture
                .update(None, target, plan.region().stride_bytes())
                .is_err()
            {
                return;
            }
            texture.set_blend_mode(sdl2::render::BlendMode::None);
            let dst = sdl2::rect::Rect::new(
                i32::from(plan.x()),
                i32::from(plan.y()),
                u32::from(plan.width()),
                u32::from(plan.height()),
            );
            uploaded = canvas.copy(&texture, None, Some(dst)).is_ok();
        });
        if uploaded {
            Ok(())
        } else {
            Err(ProjectiveDrawError::BackendFailure)
        }
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    fn needs_blit_fallback(request: &DrawRequest<'_, '_>) -> bool {
        match request.command {
            DrawCommand::Blit {
                quad,
                radius,
                composite,
                transform,
                ..
            } => {
                if *radius > Fixed::ZERO {
                    return true;
                }
                match quad {
                    Some(q) => {
                        let x0 = Fixed64::from_fixed(q[0].x) + Fixed64::from_fixed(q[2].x);
                        let x1 = Fixed64::from_fixed(q[1].x) + Fixed64::from_fixed(q[3].x);
                        let y0 = Fixed64::from_fixed(q[0].y) + Fixed64::from_fixed(q[2].y);
                        let y1 = Fixed64::from_fixed(q[1].y) + Fixed64::from_fixed(q[3].y);
                        *composite != CompositeMode::SourceOver || x0 != x1 || y0 != y1
                    }
                    None => {
                        !matches!(
                            composite,
                            CompositeMode::SourceOver
                                | CompositeMode::Add
                                | CompositeMode::Multiply
                        ) || !matches!(
                            transform.classify(),
                            crate::types::TransformClass::Identity
                                | crate::types::TransformClass::Translate
                        )
                    }
                }
            }
            _ => false,
        }
    }

    fn needs_paint_fallback(request: &DrawRequest<'_, '_>) -> bool {
        matches!(
            request.command,
            DrawCommand::FillPath {
                paint: Paint::LinearGradient(_) | Paint::RadialGradient(_),
                ..
            } | DrawCommand::StrokePath {
                paint: Paint::LinearGradient(_) | Paint::RadialGradient(_),
                ..
            }
        )
    }

    fn classify_request(request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        use crate::types::TransformClass;

        request.validate_texture()?;
        let projected = !request.projective.is_identity();
        match request.command {
            DrawCommand::PushClip { .. } | DrawCommand::PopClip => {
                return Err(RenderError::Unsupported(RenderFeature::PathClip));
            }
            _ => {}
        }
        if projected {
            return Ok(());
        }
        let supports_affine = match request.command {
            DrawCommand::Fill { quad, .. }
            | DrawCommand::Border { quad, .. }
            | DrawCommand::Blit { quad, .. } => quad.is_some(),
            DrawCommand::FillPath { .. }
            | DrawCommand::StrokePath { .. }
            | DrawCommand::GlyphRun { .. }
            | DrawCommand::PosedGlyphRun { .. } => true,
            _ => false,
        };
        if !supports_affine
            && !Self::needs_blit_fallback(request)
            && !matches!(
                request.command.transform().classify(),
                TransformClass::Identity | TransformClass::Translate
            )
        {
            return Err(RenderError::Unsupported(RenderFeature::AffineGeometry));
        }
        Ok(())
    }
}

#[cfg(test)]
mod route_tests;

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate_projection()?;
        request.validate_texture()?;
        if let DrawCommand::ApplyBlur { alpha, region } = request.command {
            if !request.projective.is_identity() {
                return Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry));
            }
            if *alpha <= Fixed::ZERO || *alpha >= Fixed::ONE {
                return Ok(RenderRoute::Native);
            }
            let fallback = self.projective_fallback.as_ref();
            return RenderRoute::target_readback(
                self.physical_clip_rect(region),
                fallback.as_ref().map(|f| f.capacity()),
                PlaneRequirements::CPU,
            );
        }
        Self::classify_request(request)?;
        let paint_fallback = Self::needs_paint_fallback(request);
        if paint_fallback && !request.projective.is_identity() {
            return Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry));
        }
        if request.projective.is_identity()
            && !Self::needs_blit_fallback(request)
            && !paint_fallback
        {
            return Ok(RenderRoute::Native);
        }
        let fallback = self
            .projective_fallback
            .as_deref()
            .ok_or(RenderError::MissingWorkspace)?;
        let plan = fallback
            .plan(
                request.command,
                &request.clip,
                &request.projective,
                self.viewport,
            )
            .map_err(RenderError::from)?;
        Ok(RenderRoute::ExactFallback(plan.region()))
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        let route = self.route(request)?;
        self.submit_with_route(request, route)
    }

    fn submit_with_route(
        &mut self,
        request: &DrawRequest<'_, '_>,
        route: RenderRoute,
    ) -> Result<(), RenderError> {
        request.validate_projection()?;
        request.validate_texture()?;
        if let DrawCommand::ApplyBlur { alpha, region } = request.command {
            return self.blur_target_region(*alpha, region);
        }
        Self::classify_request(request)?;
        let paint_fallback = Self::needs_paint_fallback(request);
        if paint_fallback && !request.projective.is_identity() {
            return Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry));
        }
        let needs_fallback = !request.projective.is_identity()
            || Self::needs_blit_fallback(request)
            || paint_fallback;
        if !needs_fallback {
            if route != RenderRoute::Native {
                return Err(RenderError::InvalidGeometry);
            }
            self.draw_failed = false;
            self.draw(request.command, &request.clip);
            return if self.draw_failed {
                Err(RenderError::BackendFailure)
            } else {
                Ok(())
            };
        }
        let RenderRoute::ExactFallback(region) = route else {
            return Err(RenderError::InvalidGeometry);
        };
        let fallback = self
            .projective_fallback
            .as_deref()
            .ok_or(RenderError::MissingWorkspace)?;
        if region.required_bytes() > fallback.capacity() {
            return Err(RenderError::InsufficientWorkspace {
                required_bytes: region.required_bytes(),
                capacity_bytes: fallback.capacity(),
            });
        }
        let requirements = self
            .projective_fallback
            .as_ref()
            .map(|fallback| fallback.requirements())
            .ok_or(RenderError::MissingWorkspace)?;
        let plan = ProjectiveFallbackPlan::from_region(region, self.viewport, requirements)
            .map_err(RenderError::from)?;
        self.draw_projective_plan(plan, request.command, &request.projective)
            .map_err(RenderError::from)
    }

    fn output_scale(&self) -> Fixed {
        self.viewport.scale()
    }

    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        use crate::types::TransformClass;

        // Explicit leaf quads short-circuit before affine dispatch.
        match cmd {
            DrawCommand::PushClip { .. } | DrawCommand::PopClip | DrawCommand::ApplyBlur { .. } => {
            }
            DrawCommand::Fill {
                quad: Some(q),
                radius,
                color,
                opa,
                ..
            } => {
                self.fill_quad_inner(q, *radius, color, *opa, clip);
                return;
            }
            DrawCommand::Border {
                quad: Some(q),
                width,
                radius,
                color,
                opa,
                ..
            } => {
                self.stroke_quad_inner(q, *width, *radius, color, *opa, clip);
                return;
            }
            DrawCommand::Blit {
                quad: Some(q),
                texture,
                opa,
                ..
            } => {
                self.blit_quad_inner(texture, q, clip, *opa);
                return;
            }
            DrawCommand::FillPath {
                path,
                transform,
                paint,
                opa,
                fill_rule,
                ..
            } => {
                let color = paint_color(paint);
                self.fill_path_transformed_inner(path, clip, transform, &color, *opa, *fill_rule);
                return;
            }
            DrawCommand::GlyphRun {
                pos,
                glyphs,
                font,
                transform,
                color,
                opa,
            } => {
                self.draw_glyph_run_inner(label::GlyphRunDraw {
                    pos,
                    glyphs,
                    font,
                    transform,
                    clip,
                    color,
                    opacity: *opa,
                });
                return;
            }
            DrawCommand::PosedGlyphRun {
                pos,
                glyphs,
                font,
                transform,
                color,
                opa,
            } => {
                self.draw_posed_glyph_run_inner(label::PosedGlyphRunDraw {
                    pos,
                    glyphs: glyphs.glyphs(),
                    frames: glyphs.frames(),
                    font,
                    transform,
                    clip,
                    color,
                    opacity: *opa,
                });
                return;
            }
            DrawCommand::StrokePath {
                path,
                transform,
                paint,
                width,
                opa,
                line_cap,
                line_join,
                miter_limit,
                dash,
            } => {
                let color = paint_color(paint);
                self.stroke_path_styled_inner(
                    path,
                    clip,
                    transform,
                    StrokeSpec {
                        width: *width,
                        cap: *line_cap,
                        join: *line_join,
                        miter_limit: *miter_limit,
                        dash,
                        dash_scale: Fixed::ONE,
                    },
                    &color,
                    *opa,
                );
                return;
            }
            _ => {}
        }

        let tf = cmd.transform();
        let (tx, ty) = match tf.classify() {
            TransformClass::Identity => (Fixed::ZERO, Fixed::ZERO),
            TransformClass::Translate => (tf.tx, tf.ty),
            _ => unimplemented!(
                "sdl_gpu backend: transform class {:?} not yet handled",
                tf.classify()
            ),
        };
        match cmd {
            DrawCommand::PushClip { .. } | DrawCommand::PopClip | DrawCommand::ApplyBlur { .. } => {
            }
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => {
                let area = offset_rect(area, tx, ty);
                self.fill_rect(&area, clip, color, *radius, *opa)
            }
            DrawCommand::Border {
                area,
                color,
                width,
                radius,
                opa,
                ..
            } => {
                let area = offset_rect(area, tx, ty);
                self.stroke_rect(&area, clip, *width, color, *radius, *opa)
            }
            DrawCommand::Line {
                p1,
                p2,
                color,
                width,
                opa,
                ..
            } => {
                let p1 = offset_point(p1, tx, ty);
                let p2 = offset_point(p2, tx, ty);
                self.draw_line(p1, p2, clip, *width, color, *opa)
            }
            DrawCommand::Blit {
                pos,
                size,
                texture,
                opa,
                radius,
                composite,
                ..
            } => {
                if *radius != Fixed::ZERO {
                    unimplemented!(
                        "sdl_gpu backend: Blit.radius mask not implemented; use SwRenderer",
                    );
                }
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                let pos = offset_point(pos, tx, ty);
                self.blit(
                    texture,
                    &src_rect,
                    pos,
                    *size,
                    clip,
                    *opa,
                    Fixed::ZERO,
                    *composite,
                );
            }
            DrawCommand::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
                ..
            } => {
                let center = offset_point(center, tx, ty);
                self.draw_arc(
                    center,
                    *radius,
                    *start_angle,
                    *end_angle,
                    clip,
                    *width,
                    color,
                    *opa,
                )
            }
            DrawCommand::GlyphRun { .. } | DrawCommand::PosedGlyphRun { .. } => {
                unreachable!("glyph runs return before dispatch")
            }
            DrawCommand::FillPath {
                path,
                paint,
                opa,
                fill_rule,
                ..
            } => {
                let color = paint_color(paint);
                if tx == Fixed::ZERO && ty == Fixed::ZERO {
                    self.fill_path_inner(path, clip, &color, *opa, *fill_rule);
                } else {
                    let translate = Transform::translate(tx, ty);
                    self.fill_path_transformed_inner(
                        path, clip, &translate, &color, *opa, *fill_rule,
                    );
                }
            }
            DrawCommand::StrokePath { .. } => unreachable!("stroke path returns before dispatch"),
        }
    }

    fn flush(&mut self) {}

    fn supports_offscreen(&self) -> bool {
        true
    }

    fn offscreen_format(&self) -> Option<crate::render::texture::ColorFormat> {
        Some(crate::render::texture::ColorFormat::RGBA8888)
    }

    fn sample_target_region(
        &self,
        src: &Rect,
    ) -> Result<Option<crate::render::texture::Texture<'static>>, RenderError> {
        let Some(phys) = self.physical_clip_rect(src) else {
            return Ok(None);
        };
        let sdl_rect = sdl2::rect::Rect::new(
            i32::from(phys.x()),
            i32::from(phys.y()),
            u32::from(phys.width()),
            u32::from(phys.height()),
        );
        let bytes = self
            .canvas
            .read_pixels(Some(sdl_rect), sdl2::pixels::PixelFormatEnum::RGBA32)
            .map_err(|_| RenderError::BackendFailure)?;
        let w = phys.width();
        let h = phys.height();
        crate::render::texture::Texture::from_vec(
            bytes,
            w,
            h,
            crate::render::texture::ColorFormat::RGBA8888,
        )
        .map(Some)
        .ok_or(RenderError::BackendFailure)
    }

    fn read_target_region(
        &self,
        src: &Rect,
        dst: &mut crate::render::texture::Texture,
    ) -> Result<(), RenderError> {
        let Some(phys) = self.physical_clip_rect(src) else {
            return Ok(());
        };
        let sdl_rect = sdl2::rect::Rect::new(
            i32::from(phys.x()),
            i32::from(phys.y()),
            u32::from(phys.width()),
            u32::from(phys.height()),
        );
        let bytes = self
            .canvas
            .read_pixels(Some(sdl_rect), sdl2::pixels::PixelFormatEnum::RGBA32)
            .map_err(|_| RenderError::BackendFailure)?;
        let (x, y, _, _) = self.viewport.rect_to_physical_pixel_bounds(*src);
        crate::render::renderer::copy_packed_rgba8(&bytes, phys, (x, y), dst)
    }

    fn modify_target_region(
        &mut self,
        src: &Rect,
        f: &mut dyn FnMut(&mut crate::render::texture::Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        let Some(mut tex) = self.sample_target_region(src)? else {
            return Ok(false);
        };
        f(&mut tex)?;
        let Some(phys) = self.physical_clip_rect(src) else {
            return Ok(false);
        };
        let w = tex.width as u32;
        let h = tex.height as u32;
        if !tex.valid_storage() {
            return Err(RenderError::InvalidTexture);
        }
        let bytes = tex.buf.as_slice();
        let stride = tex.stride;
        let canvas = &mut *self.canvas;
        self.label_cache
            .with_creator(|creator| -> Result<(), RenderError> {
                let mut sdl_tex = creator
                    .create_texture_streaming(sdl2::pixels::PixelFormatEnum::RGBA32, w, h)
                    .map_err(|_| RenderError::BackendFailure)?;
                sdl_tex
                    .update(None, bytes, stride)
                    .map_err(|_| RenderError::BackendFailure)?;
                sdl_tex.set_blend_mode(sdl2::render::BlendMode::None);
                let dst_rect =
                    sdl2::rect::Rect::new(i32::from(phys.x()), i32::from(phys.y()), w, h);
                canvas
                    .copy(&sdl_tex, None, Some(dst_rect))
                    .map_err(|_| RenderError::BackendFailure)
            })?;
        Ok(true)
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> Renderer for SdlGpuRenderer<'_, S> {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        SdlGpuRenderer::route(self, request)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        SdlGpuRenderer::submit(self, request)
    }

    fn submit_with_route(
        &mut self,
        request: &DrawRequest<'_, '_>,
        route: RenderRoute,
    ) -> Result<(), RenderError> {
        SdlGpuRenderer::submit_with_route(self, request, route)
    }

    fn flush(&mut self) {
        SdlGpuRenderer::flush(self)
    }

    fn output_scale(&self) -> Fixed {
        SdlGpuRenderer::output_scale(self)
    }

    fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
        FallbackRegion::from_logical_bounds(
            *bounds,
            self.viewport,
            self.projective_fallback
                .as_ref()
                .map(|fallback| fallback.capacity()),
        )
    }

    fn supports_offscreen(&self) -> bool {
        SdlGpuRenderer::supports_offscreen(self)
    }

    fn offscreen_format(&self) -> Option<ColorFormat> {
        SdlGpuRenderer::offscreen_format(self)
    }

    fn sample_target_region(&self, src: &Rect) -> Result<Option<Texture<'static>>, RenderError> {
        SdlGpuRenderer::sample_target_region(self, src)
    }

    fn read_target_region(&self, src: &Rect, dst: &mut Texture) -> Result<(), RenderError> {
        SdlGpuRenderer::read_target_region(self, src, dst)
    }

    fn modify_target_region(
        &mut self,
        src: &Rect,
        f: &mut dyn FnMut(&mut Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        SdlGpuRenderer::modify_target_region(self, src, f)
    }
}

#[inline]
fn offset_rect(r: &Rect, tx: Fixed, ty: Fixed) -> Rect {
    if tx == Fixed::ZERO && ty == Fixed::ZERO {
        return *r;
    }
    Rect {
        x: r.x + tx,
        y: r.y + ty,
        w: r.w,
        h: r.h,
    }
}

#[inline]
fn offset_point(p: &crate::types::Point, tx: Fixed, ty: Fixed) -> crate::types::Point {
    crate::types::Point {
        x: p.x + tx,
        y: p.y + ty,
    }
}

/// Intersect `area` with `clip` and convert to an integer-pixel
/// `sdl2::rect::Rect`. Both inputs are assumed to be in physical pixels
/// (render_system runs `scale_rects` before calling into the renderer).
/// Returns `None` if the intersection is empty.
pub(super) fn sdl_pixel_rect(area: &Rect, clip: &Rect) -> Option<sdl2::rect::Rect> {
    let inter = area.intersect(clip)?;
    let (x0, y0, x1, y1) = inter.pixel_bounds();
    let (Ok(width), Ok(height)) = (u32::try_from(x1 - x0), u32::try_from(y1 - y0)) else {
        return None;
    };
    if width == 0 || height == 0 {
        return None;
    }
    Some(sdl2::rect::Rect::new(x0, y0, width, height))
}

/// Configure the canvas draw colour + blend mode for a solid primitive
/// using `(color, opa)`. Blend off when fully opaque to skip the blend
/// shader path.
pub(super) fn apply_solid_color(canvas: &mut SdlCanvas<Window>, color: &Color, opa: u8) {
    let a = ((color.a as u16) * (opa as u16) / 255) as u8;
    canvas.set_blend_mode(if a == 255 {
        sdl2::render::BlendMode::None
    } else {
        sdl2::render::BlendMode::Blend
    });
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(color.r, color.g, color.b, a));
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> Canvas for SdlGpuRenderer<'_, S> {
    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        self.fill_rect_inner(area, clip, color, radius, opa);
    }

    fn stroke_rect(
        &mut self,
        area: &Rect,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        self.stroke_rect_inner(area, clip, width, color, radius, opa);
    }

    fn draw_line(
        &mut self,
        p1: Point,
        p2: Point,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        self.draw_line_inner(p1, p2, clip, width, color, opa);
    }

    fn fill_path(
        &mut self,
        path: &Path,
        clip: &Rect,
        paint: &Paint,
        opa: u8,
        fill_rule: crate::render::raster::FillRule,
    ) {
        let color = paint_color(paint);
        self.fill_path_inner(path, clip, &color, opa, fill_rule);
    }

    fn stroke_path(
        &mut self,
        path: &Path,
        clip: &Rect,
        width: Fixed,
        paint: &Paint,
        opa: u8,
        cap: crate::render::raster::LineCap,
        join: crate::render::raster::LineJoin,
        miter_limit: Fixed,
        dash: &[Fixed],
    ) {
        let color = paint_color(paint);
        self.stroke_path_styled_inner(
            path,
            clip,
            &Transform::IDENTITY,
            StrokeSpec {
                width,
                cap,
                join,
                miter_limit,
                dash,
                dash_scale: Fixed::ONE,
            },
            &color,
            opa,
        );
    }

    fn blit(
        &mut self,
        src: &Texture,
        src_rect: &Rect,
        dst: Point,
        dst_size: Point,
        clip: &Rect,
        opa: u8,
        radius: Fixed,
        composite: CompositeMode,
    ) {
        if radius != Fixed::ZERO {
            unimplemented!("sdl_gpu backend: Blit.radius mask not implemented; use SwRenderer");
        }
        self.blit_inner(src, src_rect, dst, dst_size, clip, opa, composite);
    }

    fn clear(&mut self, area: &Rect, color: &Color) {
        self.fill_rect_inner(area, area, color, Fixed::ZERO, 255);
    }

    fn draw_glyph_run(
        &mut self,
        pos: &Point,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &crate::render::font::Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        self.draw_glyph_run_inner(label::GlyphRunDraw {
            pos,
            glyphs,
            font,
            transform: &Transform::IDENTITY,
            clip,
            color,
            opacity: opa,
        });
    }

    fn draw_posed_glyph_run(
        &mut self,
        pos: &Point,
        glyphs: crate::render::PosedGlyphs<'_>,
        font: &crate::render::font::Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        self.draw_posed_glyph_run_inner(label::PosedGlyphRunDraw {
            pos,
            glyphs: glyphs.glyphs(),
            frames: glyphs.frames(),
            font,
            transform: &Transform::IDENTITY,
            clip,
            color,
            opacity: opa,
        });
    }

    fn flush(&mut self) {}
}
