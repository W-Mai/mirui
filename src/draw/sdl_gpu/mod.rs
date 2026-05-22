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

use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::render::Canvas as SdlCanvas;
use sdl2::video::Window;

use self::label_cache::LabelCache;
use self::tessellation::TessellationCache;
use crate::app::RendererFactory;
use crate::draw::canvas::Canvas;
use crate::draw::command::DrawCommand;
use crate::draw::path::Path;
use crate::draw::renderer::Renderer;
use crate::draw::texture::{ColorFormat, Texture};
use crate::types::{Color, Fixed, Point, Rect, Viewport};

use crate::cache::{CacheInspect, InspectCaches};
use crate::surface::{DisplayInfo, InputEvent, Surface, logical_from_physical};

pub struct SdlGpuSurface {
    canvas: SdlCanvas<Window>,
    label_cache: LabelCache,
    event_pump: EventPump,
    tessellator: TessellationCache,
    width: u16,
    height: u16,
    scale: Fixed,
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
            event_pump,
            tessellator: TessellationCache::new(),
            width: phys_w,
            height: phys_h,
            scale,
        }
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
        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => return Some(InputEvent::Quit),
                Event::KeyDown {
                    keycode: Some(kc), ..
                } => {
                    use crate::event::input::*;
                    let code = match kc {
                        Keycode::Backspace => KEY_BACKSPACE,
                        Keycode::Delete => KEY_DELETE,
                        Keycode::Left => KEY_LEFT,
                        Keycode::Right => KEY_RIGHT,
                        Keycode::Home => KEY_HOME,
                        Keycode::End => KEY_END,
                        Keycode::Return => KEY_RETURN,
                        Keycode::Escape => return Some(InputEvent::Quit),
                        _ => continue,
                    };
                    return Some(InputEvent::Key {
                        code,
                        pressed: true,
                    });
                }
                Event::MouseButtonDown { x, y, .. } => {
                    return Some(InputEvent::PointerDown {
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseButtonUp { x, y, .. } => {
                    return Some(InputEvent::PointerUp {
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseMotion {
                    x, y, mousestate, ..
                } if mousestate.left() => {
                    return Some(InputEvent::PointerMove {
                        id: 0,
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::TextInput { text, .. } => {
                    if let Some(ch) = text.chars().next() {
                        return Some(InputEvent::CharInput { ch });
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn persistence(&self) -> crate::surface::BackbufferPersistence {
        crate::surface::BackbufferPersistence::Transient
    }
}

// NOTE: no `impl FramebufferAccess for SdlGpuSurface` — by design.

type CacheAccessor = fn(&SdlGpuSurface) -> &dyn CacheInspect;

impl InspectCaches for SdlGpuSurface {
    fn inspect_caches(&self) -> impl Iterator<Item = (&'static str, &dyn CacheInspect)> + '_ {
        const ENTRIES: &[CacheAccessor] = &[|s: &SdlGpuSurface| s.label_cache.as_inspect()];
        ENTRIES.iter().map(move |f| {
            let c = f(self);
            (c.cache_name(), c)
        })
    }
}

pub struct SdlGpuFactory;

impl SdlGpuFactory {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SdlGpuFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl RendererFactory<SdlGpuSurface> for SdlGpuFactory {
    type Renderer<'a>
        = SdlGpuRenderer<'a>
    where
        Self: 'a;

    fn make<'a>(
        &'a mut self,
        backend: &'a mut SdlGpuSurface,
        transform: &Viewport,
    ) -> SdlGpuRenderer<'a> {
        let viewport = *transform;
        let (canvas, label_cache, tessellator) = backend.parts_mut();
        SdlGpuRenderer {
            canvas,
            label_cache,
            tessellator,
            viewport,
        }
    }
}

pub struct SdlGpuRenderer<'a> {
    canvas: &'a mut SdlCanvas<Window>,
    label_cache: &'a mut LabelCache,
    tessellator: &'a mut TessellationCache,
    viewport: Viewport,
}

impl SdlGpuRenderer<'_> {
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
}

impl Renderer for SdlGpuRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        use crate::types::TransformClass;

        // Quad fast paths short-circuit before the axis-aligned
        // translate/transform branch: the render_system has already
        // pre-projected any 3D/2D affine into the 4 quad vertices, so
        // the GPU just needs to tessellate / UV-map them.
        match cmd {
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
                ..
            } => {
                self.blit_quad_inner(texture, q, clip);
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
                pos, size, texture, ..
            } => {
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                let pos = offset_point(pos, tx, ty);
                self.blit(texture, &src_rect, pos, *size, clip);
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
            DrawCommand::Label {
                pos,
                text,
                color,
                opa,
                ..
            } => {
                let pos = offset_point(pos, tx, ty);
                self.draw_label(&pos, text, clip, color, *opa)
            }
            DrawCommand::FillPath {
                path, color, opa, ..
            } => {
                if tx == Fixed::ZERO && ty == Fixed::ZERO {
                    self.fill_path_inner(path, clip, color, *opa);
                } else {
                    let translated = translate_path(path, tx, ty);
                    self.fill_path_inner(&translated, clip, color, *opa);
                }
            }
        }
    }

    fn flush(&mut self) {}
}

fn translate_path(path: &crate::draw::path::Path, tx: Fixed, ty: Fixed) -> crate::draw::path::Path {
    use crate::draw::path::PathCmd;
    let cmds = path
        .cmds
        .iter()
        .map(|c| match c {
            PathCmd::MoveTo(p) => PathCmd::MoveTo(Point {
                x: p.x + tx,
                y: p.y + ty,
            }),
            PathCmd::LineTo(p) => PathCmd::LineTo(Point {
                x: p.x + tx,
                y: p.y + ty,
            }),
            PathCmd::QuadTo { ctrl, end } => PathCmd::QuadTo {
                ctrl: Point {
                    x: ctrl.x + tx,
                    y: ctrl.y + ty,
                },
                end: Point {
                    x: end.x + tx,
                    y: end.y + ty,
                },
            },
            PathCmd::CubicTo { ctrl1, ctrl2, end } => PathCmd::CubicTo {
                ctrl1: Point {
                    x: ctrl1.x + tx,
                    y: ctrl1.y + ty,
                },
                ctrl2: Point {
                    x: ctrl2.x + tx,
                    y: ctrl2.y + ty,
                },
                end: Point {
                    x: end.x + tx,
                    y: end.y + ty,
                },
            },
            PathCmd::Close => PathCmd::Close,
        })
        .collect();
    crate::draw::path::Path { cmds }
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

impl Canvas for SdlGpuRenderer<'_> {
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

    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        self.fill_path_inner(path, clip, color, opa);
    }

    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        self.stroke_path_inner(path, clip, width, color, opa);
    }

    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, dst_size: Point, clip: &Rect) {
        self.blit_inner(src, src_rect, dst, dst_size, clip);
    }

    fn clear(&mut self, area: &Rect, color: &Color) {
        self.fill_rect_inner(area, area, color, Fixed::ZERO, 255);
    }

    fn draw_label(&mut self, pos: &Point, text: &[u8], clip: &Rect, color: &Color, opa: u8) {
        self.draw_label_inner(pos, text, clip, color, opa);
    }

    fn flush(&mut self) {}
}
