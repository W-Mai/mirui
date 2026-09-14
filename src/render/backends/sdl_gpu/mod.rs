//! GPU-accelerated SDL backend.
//!
//! Where [`crate::surface::sdl::SdlSurface`] keeps a CPU byte buffer and uploads it
//! each frame, `SdlGpuSurface` drives the SDL2 accelerated renderer
//! directly: `canvas.fill_rect`, `canvas.copy`, and ultimately
//! `SDL_RenderGeometry` (unsafe FFI) for tessellated paths. No CPU
//! framebuffer, no `FramebufferAccess` impl.

mod blit;
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
use sdl2::keyboard::Keycode;
use sdl2::render::Canvas as SdlCanvas;
use sdl2::video::Window;

use self::label_cache::LabelCache;
use self::tessellation::TessellationCache;
use crate::render::canvas::{Canvas, Paint};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::factory::RendererFactory;
use crate::render::path::Path;
use crate::render::projective_fallback::{ProjectiveFallback, ProjectiveFallbackPlan};
use crate::render::raster::{StrokeScratch, StrokeSpec};
use crate::render::renderer::{
    DrawRequest, ProjectiveDrawError, RenderError, RenderFeature, RenderRoute, Renderer,
};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Color, Fixed, Point, Rect, Transform, Transform3D, Viewport};

use crate::core::cache::{CacheInspect, InspectCaches};
use crate::surface::{DisplayInfo, InputEvent, Surface, logical_from_physical};

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
    tessellator: TessellationCache,
    width: u16,
    height: u16,
    scale: Fixed,
    last_mouse_x: i32,
    last_mouse_y: i32,
    multi: MultiGestureState,
    pending: alloc::collections::VecDeque<InputEvent>,
}

impl SdlGpuSurface {
    pub fn new(title: &str, width: u16, height: u16) -> Self {
        Self::new_with_vsync(title, width, height, true)
    }

    pub fn new_with_vsync(title: &str, width: u16, height: u16, vsync: bool) -> Self {
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");
        // Ask SDL's renderer driver to use OpenGL so GL MSAA attributes
        // take effect; on macOS the default would be Metal, which
        // ignores those attributes and leaves triangle edges aliased.
        sdl2::hint::set("SDL_RENDER_DRIVER", "opengl");

        let sdl = sdl2::init().expect("SDL2 init failed");
        let video = sdl.video().expect("SDL2 video init failed");

        // Request 4× MSAA on the GL context before the window is built;
        // the renderer's SDL_RenderGeometry calls then antialias triangle
        // edges natively, matching the SW backend's SDF coverage.
        {
            let gl = video.gl_attr();
            gl.set_multisample_buffers(1);
            gl.set_multisample_samples(4);
        }

        let window = video
            .window(title, width as u32, height as u32)
            .position_centered()
            .allow_highdpi()
            .opengl()
            .build()
            .expect("SDL2 window creation failed");
        let mut canvas_builder = window.into_canvas().accelerated();
        if vsync {
            canvas_builder = canvas_builder.present_vsync();
        }
        let canvas = canvas_builder.build().expect("SDL2 canvas failed");
        let texture_creator = canvas.texture_creator();
        let event_pump = sdl.event_pump().expect("SDL2 event pump failed");

        let (draw_w, _) = canvas.output_size().unwrap();
        let scale_int = (draw_w as u16) / width;
        let scale_int = if scale_int == 0 { 1 } else { scale_int };
        let scale = Fixed::from(scale_int);

        let phys_w = width * scale_int;
        let phys_h = height * scale_int;

        Self {
            canvas,
            label_cache: LabelCache::new(texture_creator),
            _event_pump: event_pump,
            tessellator: TessellationCache::new(),
            width: phys_w,
            height: phys_h,
            scale,
            last_mouse_x: 0,
            last_mouse_y: 0,
            multi: MultiGestureState::default(),
            pending: alloc::collections::VecDeque::new(),
        }
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
                id: VIRT_FINGER_A,
                x: Fixed::from(ax as i32),
                y: Fixed::from(ay as i32),
            });
            self.pending.push_back(InputEvent::PointerDown {
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
            id: VIRT_FINGER_A,
            x: Fixed::from(ax as i32),
            y: Fixed::from(ay as i32),
        });
        self.pending.push_back(InputEvent::PointerMove {
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
            id: VIRT_FINGER_A,
            x: Fixed::from(ax as i32),
            y: Fixed::from(ay as i32),
        });
        self.pending.push_back(InputEvent::PointerUp {
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
        let (lw, lh) = logical_from_physical(self.width, self.height, self.scale);
        DisplayInfo {
            width: lw,
            height: lh,
            scale: self.scale,
            format: ColorFormat::RGBA8888,
        }
    }

    fn physical_size(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    fn flush(&mut self, _area: &Rect) {
        self.canvas.present();
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
                Event::KeyDown {
                    keycode: Some(kc), ..
                } => {
                    use crate::input::event::input::*;
                    let code = match kc {
                        Keycode::Backspace => KEY_BACKSPACE,
                        Keycode::Delete => KEY_DELETE,
                        Keycode::Left => KEY_LEFT,
                        Keycode::Right => KEY_RIGHT,
                        Keycode::Home => KEY_HOME,
                        Keycode::End => KEY_END,
                        Keycode::Return => KEY_RETURN,
                        Keycode::Escape => {
                            self.pending.push_back(InputEvent::Quit);
                            continue;
                        }
                        _ => continue,
                    };
                    self.pending.push_back(InputEvent::Key {
                        code,
                        pressed: true,
                    });
                }
                Event::MouseButtonDown { x, y, .. } => {
                    self.pending.push_back(InputEvent::PointerDown {
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseButtonUp { x, y, .. } => {
                    self.pending.push_back(InputEvent::PointerUp {
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseMotion { x, y, .. } => {
                    self.last_mouse_x = x;
                    self.last_mouse_y = y;
                    self.pending.push_back(InputEvent::PointerMove {
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseWheel { x, y, .. } => {
                    self.pending.push_back(InputEvent::Wheel {
                        dx: Fixed::from(x),
                        dy: Fixed::from(y),
                        x: Fixed::from(self.last_mouse_x),
                        y: Fixed::from(self.last_mouse_y),
                    });
                }
                Event::MultiGesture {
                    d_theta,
                    d_dist,
                    x,
                    y,
                    ..
                } => {
                    let win_w = self.width as f32 / self.scale.to_f32();
                    let win_h = self.height as f32 / self.scale.to_f32();
                    let cx = x * win_w;
                    let cy = y * win_h;
                    self.handle_multi_gesture(cx, cy, d_theta, d_dist, win_w, win_h);
                }
                Event::TextInput { text, .. } => {
                    if let Some(ch) = text.chars().next() {
                        self.pending.push_back(InputEvent::CharInput { ch });
                    }
                }
                Event::Window {
                    win_event: sdl2::event::WindowEvent::Leave,
                    ..
                } => {
                    const OFF: i32 = i16::MIN as i32;
                    self.pending.push_back(InputEvent::PointerMove {
                        id: 0,
                        x: Fixed::from_int(OFF),
                        y: Fixed::from_int(OFF),
                    });
                }
                _ => {}
            }
        }
        self.pending.pop_front()
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
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    fn physical_clip_rect(&self, src: &Rect) -> Option<Rect> {
        let phys = self.viewport.rect_to_physical(*src);
        let (pw, ph) = self.viewport.physical_size();
        let target = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(pw as i32),
            h: Fixed::from_int(ph as i32),
        };
        let clipped = phys.intersect(&target)?;
        if clipped.w <= Fixed::ZERO || clipped.h <= Fixed::ZERO {
            return None;
        }
        Some(clipped)
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
        unsafe {
            sdl2_sys::SDL_RenderGeometry(
                self.canvas.raw(),
                core::ptr::null_mut(),
                verts.as_ptr(),
                verts.len() as _,
                indices.as_ptr(),
                indices.len() as _,
            );
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
            x: plan.x,
            y: plan.y,
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
                i32::from(plan.width()) * 4,
            )
        };
        if read_result != 0 {
            return Err(ProjectiveDrawError::Unsupported);
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
                .update(None, target, usize::from(plan.width()) * 4)
                .is_err()
            {
                return;
            }
            texture.set_blend_mode(sdl2::render::BlendMode::None);
            let dst = sdl2::rect::Rect::new(
                plan.x,
                plan.y,
                u32::from(plan.width()),
                u32::from(plan.height()),
            );
            uploaded = canvas.copy(&texture, None, Some(dst)).is_ok();
        });
        if uploaded {
            Ok(())
        } else {
            Err(ProjectiveDrawError::Unsupported)
        }
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    fn classify_request(request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        use crate::types::TransformClass;

        request.validate_texture()?;
        let projected = !request.projective.is_identity();
        match request.command {
            DrawCommand::PushClip { .. } | DrawCommand::PopClip => {
                return Err(RenderError::Unsupported(RenderFeature::PathClip));
            }
            DrawCommand::ApplyBlur { .. } => {
                return Err(RenderError::Unsupported(RenderFeature::Blur));
            }
            DrawCommand::StrokePath { paint, .. } => {
                if !matches!(paint, Paint::Color(_)) {
                    return Err(RenderError::Unsupported(RenderFeature::GradientPaint));
                }
            }
            DrawCommand::FillPath { paint, .. } if !projected => {
                if !matches!(paint, Paint::Color(_)) {
                    return Err(RenderError::Unsupported(RenderFeature::GradientPaint));
                }
            }
            DrawCommand::Blit {
                quad,
                texture,
                radius,
                composite,
                ..
            } if !projected => {
                if *radius != Fixed::ZERO {
                    return Err(RenderError::Unsupported(RenderFeature::RoundedBlit));
                }
                let supported = if quad.is_some() {
                    *composite == CompositeMode::SourceOver
                } else {
                    matches!(
                        composite,
                        CompositeMode::SourceOver | CompositeMode::Add | CompositeMode::Multiply
                    )
                };
                if !supported {
                    return Err(RenderError::Unsupported(RenderFeature::Composite(
                        *composite,
                    )));
                }
                if texture.format == ColorFormat::RGB565Swapped {
                    return Err(RenderError::Unsupported(RenderFeature::TextureFormat(
                        texture.format,
                    )));
                }
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
mod route_tests {
    use super::*;

    #[test]
    fn solid_stroke_with_full_style_has_a_native_route() {
        let path = Path::rect(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(16),
            Fixed::from_int(12),
        );
        let paint = Paint::Color(Color::rgb(20, 30, 40).into());
        let dash = [Fixed::from_int(3), Fixed::from_int(2)];
        let stroke = DrawCommand::StrokePath {
            path: &path,
            transform: Transform::rotate_deg(Fixed::from_int(20)),
            paint: &paint,
            width: Fixed::from_int(2),
            opa: 255,
            line_cap: crate::render::raster::LineCap::Round,
            line_join: crate::render::raster::LineJoin::Bevel,
            miter_limit: Fixed::from_int(4),
            dash: &dash,
        };
        assert_eq!(
            SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(
                &stroke,
                Rect::new(0, 0, 32, 32),
            )),
            Ok(())
        );
    }

    #[test]
    fn borrowed_fallback_factory_keeps_the_caller_budget() {
        let mut bytes = [0u8; 256];
        let factory =
            SdlGpuFactory::new().with_projective_fallback(ProjectiveFallback::borrowed(&mut bytes));
        assert_eq!(factory.projective_fallback.unwrap().capacity(), 256);
    }

    #[test]
    fn rejects_ignored_sdl_gpu_commands_and_texture_format() {
        let clip = Rect::new(0, 0, 16, 16);
        let path = Path::new();
        let push = DrawCommand::PushClip {
            path: &path,
            transform: Transform::IDENTITY,
            fill_rule: crate::render::raster::FillRule::EvenOdd,
        };
        assert_eq!(
            SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&push, clip)),
            Err(RenderError::Unsupported(RenderFeature::PathClip))
        );

        let paint = Paint::Color(Color::rgb(20, 30, 40).into());
        let fill = DrawCommand::FillPath {
            path: &path,
            transform: Transform::IDENTITY,
            paint: &paint,
            opa: 255,
            fill_rule: crate::render::raster::FillRule::NonZero,
        };
        assert_eq!(
            SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&fill, clip)),
            Ok(())
        );

        let texture = Texture::owned(2, 2, ColorFormat::RGB565Swapped);
        let blit = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::new(2, 2),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        };
        assert_eq!(
            SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&blit, clip)),
            Err(RenderError::Unsupported(RenderFeature::TextureFormat(
                ColorFormat::RGB565Swapped
            )))
        );

        let short = Texture::from_ref(&[0u8; 1], 2, 2, ColorFormat::RGBA8888);
        let invalid = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::new(2, 2),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &short,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        };
        assert_eq!(
            SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&invalid, clip)),
            Err(RenderError::InvalidTexture)
        );
    }

    #[test]
    fn distinguishes_sdl_gpu_quad_composite_and_affine_support() {
        let clip = Rect::new(0, 0, 16, 16);
        let texture = Texture::owned(2, 2, ColorFormat::RGBA8888);
        let quad = [
            Point::ZERO,
            Point::new(2, 0),
            Point::new(2, 2),
            Point::new(0, 2),
        ];
        let blit = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::new(2, 2),
            transform: Transform::IDENTITY,
            quad: Some(quad),
            texture: &texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::Add,
        };
        assert_eq!(
            SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&blit, clip)),
            Err(RenderError::Unsupported(RenderFeature::Composite(
                CompositeMode::Add
            )))
        );

        let line = DrawCommand::Line {
            p1: Point::ZERO,
            p2: Point::new(10, 10),
            transform: Transform::rotate_deg(Fixed::from_int(20)),
            color: Color::rgb(20, 30, 40),
            width: Fixed::ONE,
            opa: 255,
        };
        assert_eq!(
            SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&line, clip)),
            Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
        );
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> Renderer for SdlGpuRenderer<'_, S> {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate_projection()?;
        Self::classify_request(request)?;
        if request.projective.is_identity() {
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
        Ok(RenderRoute::ExactFallback {
            required_bytes: plan.required_bytes(),
        })
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        request.validate_projection()?;
        Self::classify_request(request)?;
        if request.projective.is_identity() {
            self.draw(request.command, &request.clip);
            return Ok(());
        }
        let plan = self
            .projective_fallback
            .as_deref()
            .ok_or(RenderError::MissingWorkspace)?
            .plan(
                request.command,
                &request.clip,
                &request.projective,
                self.viewport,
            )
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

    fn draw_projective(
        &mut self,
        command: &DrawCommand,
        clip: &Rect,
        projective: &Transform3D,
    ) -> Result<(), ProjectiveDrawError> {
        if projective.is_identity() {
            self.draw(command, clip);
            return Ok(());
        }
        let plan = self
            .projective_fallback
            .as_deref()
            .ok_or(ProjectiveDrawError::MissingFallbackStorage)?
            .plan(command, clip, projective, self.viewport)?;
        self.draw_projective_plan(plan, command, projective)
    }

    fn preflight_projective(
        &self,
        command: &DrawCommand,
        clip: &Rect,
        projective: &Transform3D,
    ) -> Result<(), ProjectiveDrawError> {
        if projective.is_identity() {
            return Ok(());
        }
        let fallback = self
            .projective_fallback
            .as_deref()
            .ok_or(ProjectiveDrawError::MissingFallbackStorage)?;
        fallback
            .plan(command, clip, projective, self.viewport)
            .map(|_| ())
    }

    fn flush(&mut self) {}

    fn supports_offscreen(&self) -> bool {
        true
    }

    fn offscreen_format(&self) -> Option<crate::render::texture::ColorFormat> {
        Some(crate::render::texture::ColorFormat::RGBA8888)
    }

    fn sample_target_region(&self, src: &Rect) -> Option<crate::render::texture::Texture<'static>> {
        let phys = self.physical_clip_rect(src)?;
        let sdl_rect = sdl2::rect::Rect::new(
            phys.x.to_int(),
            phys.y.to_int(),
            phys.w.to_int() as u32,
            phys.h.to_int() as u32,
        );
        let bytes = self
            .canvas
            .read_pixels(Some(sdl_rect), sdl2::pixels::PixelFormatEnum::RGBA32)
            .ok()?;
        let w = u16::try_from(phys.w.to_int()).ok()?;
        let h = u16::try_from(phys.h.to_int()).ok()?;
        crate::render::texture::Texture::from_vec(
            bytes,
            w,
            h,
            crate::render::texture::ColorFormat::RGBA8888,
        )
    }

    fn read_target_region(&self, src: &Rect, dst: &mut crate::render::texture::Texture) {
        let Some(phys) = self.physical_clip_rect(src) else {
            return;
        };
        let sdl_rect = sdl2::rect::Rect::new(
            phys.x.to_int(),
            phys.y.to_int(),
            phys.w.to_int() as u32,
            phys.h.to_int() as u32,
        );
        let Ok(bytes) = self
            .canvas
            .read_pixels(Some(sdl_rect), sdl2::pixels::PixelFormatEnum::RGBA32)
        else {
            return;
        };
        if let crate::render::texture::TexBuf::Owned(ref mut buf) = dst.buf {
            let copy = buf.len().min(bytes.len());
            buf[..copy].copy_from_slice(&bytes[..copy]);
        }
    }

    fn modify_target_region(
        &mut self,
        src: &Rect,
        f: &mut dyn FnMut(&mut crate::render::texture::Texture),
    ) -> bool {
        let Some(mut tex) = self.sample_target_region(src) else {
            return false;
        };
        f(&mut tex);
        let Some(phys) = self.physical_clip_rect(src) else {
            return false;
        };
        let w = tex.width as u32;
        let h = tex.height as u32;
        if !tex.valid_storage() {
            return false;
        }
        let bytes = tex.buf.as_slice();
        let stride = tex.stride;
        let canvas = &mut *self.canvas;
        let mut ok = true;
        self.label_cache.with_creator(|creator| {
            let mut sdl_tex =
                match creator.create_texture_streaming(sdl2::pixels::PixelFormatEnum::RGBA32, w, h)
                {
                    Ok(t) => t,
                    Err(_) => {
                        ok = false;
                        return;
                    }
                };
            if sdl_tex.update(None, bytes, stride).is_err() {
                ok = false;
                return;
            }
            sdl_tex.set_blend_mode(sdl2::render::BlendMode::None);
            let dst_rect = sdl2::rect::Rect::new(phys.x.to_int(), phys.y.to_int(), w, h);
            if canvas.copy(&sdl_tex, None, Some(dst_rect)).is_err() {
                ok = false;
            }
        });
        ok
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
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(sdl2::rect::Rect::new(
        x0,
        y0,
        (x1 - x0) as u32,
        (y1 - y0) as u32,
    ))
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
