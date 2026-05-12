//! GPU-accelerated SDL backend.
//!
//! Where [`super::sdl::SdlBackend`] keeps a CPU byte buffer and uploads it
//! each frame, `SdlGpuBackend` drives the SDL2 accelerated renderer
//! directly: `canvas.fill_rect`, `canvas.copy`, and ultimately
//! `SDL_RenderGeometry` (unsafe FFI) for tessellated paths. No CPU
//! framebuffer, no `FramebufferAccess` impl.

mod label_cache;
mod tessellation;

use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::render::Canvas;
use sdl2::video::Window;

use self::label_cache::LabelCache;
use self::tessellation::TessellationCache;
use crate::app::RendererFactory;
use crate::draw::backend::DrawBackend;
use crate::draw::command::DrawCommand;
use crate::draw::path::Path;
use crate::draw::renderer::Renderer;
use crate::draw::texture::{ColorFormat, Texture};
use crate::types::{Color, Fixed, Point, Rect, Viewport};

use super::{Backend, DisplayInfo, InputEvent, logical_from_physical};

pub struct SdlGpuBackend {
    canvas: Canvas<Window>,
    label_cache: LabelCache,
    event_pump: EventPump,
    tessellator: TessellationCache,
    width: u16,
    height: u16,
    scale: Fixed,
}

impl SdlGpuBackend {
    pub fn new(title: &str, width: u16, height: u16) -> Self {
        Self::new_with_vsync(title, width, height, true)
    }

    pub fn new_with_vsync(title: &str, width: u16, height: u16, vsync: bool) -> Self {
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");

        let sdl = sdl2::init().expect("SDL2 init failed");
        let video = sdl.video().expect("SDL2 video init failed");
        let window = video
            .window(title, width as u32, height as u32)
            .position_centered()
            .allow_highdpi()
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
    ) -> (&mut Canvas<Window>, &mut LabelCache, &mut TessellationCache) {
        (
            &mut self.canvas,
            &mut self.label_cache,
            &mut self.tessellator,
        )
    }
}

impl Backend for SdlGpuBackend {
    fn display_info(&self) -> DisplayInfo {
        let (lw, lh) = logical_from_physical(self.width, self.height, self.scale);
        DisplayInfo {
            width: lw,
            height: lh,
            scale: self.scale,
            format: ColorFormat::ARGB8888,
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
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => return Some(InputEvent::Quit),
                Event::MouseButtonDown { x, y, .. } => {
                    return Some(InputEvent::Touch {
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseButtonUp { x, y, .. } => {
                    return Some(InputEvent::Release {
                        x: x.into(),
                        y: y.into(),
                    });
                }
                Event::MouseMotion {
                    x, y, mousestate, ..
                } if mousestate.left() => {
                    return Some(InputEvent::TouchMove {
                        x: x.into(),
                        y: y.into(),
                    });
                }
                _ => {}
            }
        }
        None
    }

    fn persistence(&self) -> super::BackbufferPersistence {
        super::BackbufferPersistence::Transient
    }
}

// NOTE: no `impl FramebufferAccess for SdlGpuBackend` — by design.

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

impl RendererFactory<SdlGpuBackend> for SdlGpuFactory {
    type Renderer<'a>
        = SdlGpuRenderer<'a>
    where
        Self: 'a;

    fn make<'a>(
        &'a mut self,
        backend: &'a mut SdlGpuBackend,
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
    canvas: &'a mut Canvas<Window>,
    label_cache: &'a mut LabelCache,
    tessellator: &'a mut TessellationCache,
    viewport: Viewport,
}

impl SdlGpuRenderer<'_> {
    fn submit_geometry(&mut self, phys_clip: &Rect, needs_blend: bool) {
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

    /// Path-scale helper to feed the tessellator (which produces physical
    /// vertex positions) the path already in physical coords.
    fn scale_path(&self, path: &Path) -> Path {
        let s = self.viewport.scale();
        let cmds = path
            .cmds
            .iter()
            .map(|c| match c {
                crate::draw::path::PathCmd::MoveTo(p) => {
                    crate::draw::path::PathCmd::MoveTo(Point {
                        x: p.x * s,
                        y: p.y * s,
                    })
                }
                crate::draw::path::PathCmd::LineTo(p) => {
                    crate::draw::path::PathCmd::LineTo(Point {
                        x: p.x * s,
                        y: p.y * s,
                    })
                }
                crate::draw::path::PathCmd::QuadTo { ctrl, end } => {
                    crate::draw::path::PathCmd::QuadTo {
                        ctrl: Point {
                            x: ctrl.x * s,
                            y: ctrl.y * s,
                        },
                        end: Point {
                            x: end.x * s,
                            y: end.y * s,
                        },
                    }
                }
                crate::draw::path::PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                    crate::draw::path::PathCmd::CubicTo {
                        ctrl1: Point {
                            x: ctrl1.x * s,
                            y: ctrl1.y * s,
                        },
                        ctrl2: Point {
                            x: ctrl2.x * s,
                            y: ctrl2.y * s,
                        },
                        end: Point {
                            x: end.x * s,
                            y: end.y * s,
                        },
                    }
                }
                crate::draw::path::PathCmd::Close => crate::draw::path::PathCmd::Close,
            })
            .collect();
        Path { cmds }
    }
}

impl Renderer for SdlGpuRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        use crate::types::TransformClass;

        if matches!(
            cmd,
            DrawCommand::Fill { quad: Some(_), .. } | DrawCommand::Blit { quad: Some(_), .. }
        ) {
            unimplemented!("sdl_gpu: 3D quad rendering not yet supported");
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
        }
    }

    fn flush(&mut self) {}
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
fn sdl_pixel_rect(area: &Rect, clip: &Rect) -> Option<sdl2::rect::Rect> {
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
fn apply_solid_color(canvas: &mut Canvas<Window>, color: &Color, opa: u8) {
    let a = ((color.a as u16) * (opa as u16) / 255) as u8;
    canvas.set_blend_mode(if a == 255 {
        sdl2::render::BlendMode::None
    } else {
        sdl2::render::BlendMode::Blend
    });
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(color.r, color.g, color.b, a));
}

impl DrawBackend for SdlGpuRenderer<'_> {
    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        let phys_area = self.viewport.rect_to_physical(*area);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        if radius == Fixed::ZERO {
            if let Some(sdl_rect) = sdl_pixel_rect(&phys_area, &phys_clip) {
                apply_solid_color(self.canvas, color, opa);
                let _ = self.canvas.fill_rect(sdl_rect);
            }
            return;
        }
        let path = Path::rounded_rect(area.x, area.y, area.w, area.h, radius);
        self.fill_path(&path, clip, color, opa);
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
        if radius == Fixed::ZERO && width == Fixed::ONE {
            let phys_area = self.viewport.rect_to_physical(*area);
            let phys_clip = self.viewport.rect_to_physical(*clip);
            if let Some(sdl_rect) = sdl_pixel_rect(&phys_area, &phys_clip) {
                apply_solid_color(self.canvas, color, opa);
                let _ = self.canvas.draw_rect(sdl_rect);
            }
            return;
        }
        let path = Path::rounded_rect(
            area.x + width / 2,
            area.y + width / 2,
            area.w - width,
            area.h - width,
            (radius - width / 2).max(Fixed::ZERO),
        );
        self.stroke_path(&path, clip, width, color, opa);
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
        if width == Fixed::ONE {
            let phys_clip = self.viewport.rect_to_physical(*clip);
            let phys_p1 = self.viewport.point_to_physical(p1);
            let phys_p2 = self.viewport.point_to_physical(p2);
            let Some(clip_rect) = sdl_pixel_rect(&phys_clip, &phys_clip) else {
                return;
            };
            self.canvas.set_clip_rect(clip_rect);
            apply_solid_color(self.canvas, color, opa);
            let _ = self.canvas.draw_line(
                sdl2::rect::Point::new(phys_p1.x.to_int(), phys_p1.y.to_int()),
                sdl2::rect::Point::new(phys_p2.x.to_int(), phys_p2.y.to_int()),
            );
            self.canvas.set_clip_rect(None);
            return;
        }
        let mut path = Path::new();
        path.move_to(p1).line_to(p2);
        self.stroke_path(&path, clip, width, color, opa);
    }

    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        let phys_path = self.scale_path(path);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.tessellator.fill(&phys_path, color, opa);
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }

    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        let phys_path = self.scale_path(path);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let physical_width = (width * self.viewport.scale()).to_f32().max(1.0);
        self.tessellator
            .stroke(&phys_path, physical_width, color, opa);
        self.submit_geometry(&phys_clip, opa != 255 || color.a != 255);
    }

    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, dst_size: Point, clip: &Rect) {
        let sdl_fmt = match src.format {
            ColorFormat::ARGB8888 => sdl2::pixels::PixelFormatEnum::RGBA32,
            ColorFormat::RGB888 => sdl2::pixels::PixelFormatEnum::RGB24,
            ColorFormat::RGB565 => sdl2::pixels::PixelFormatEnum::RGB565,
            ColorFormat::RGB565Swapped => {
                // SDL has no BGR565 variant; round-trip via RGB565 would swap
                // channels. Punt until we need it in practice.
                return;
            }
        };

        let phys_dst = self.viewport.point_to_physical(dst);
        let phys_dst_size = self.viewport.point_to_physical(dst_size);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let dx = phys_dst.x.to_int();
        let dy = phys_dst.y.to_int();
        let dw = phys_dst_size.x.to_int().max(0) as u32;
        let dh = phys_dst_size.y.to_int().max(0) as u32;
        if dw == 0 || dh == 0 {
            return;
        }
        let (sx0, sy0, sx1, sy1) = src_rect.pixel_bounds();
        let src_sdl = sdl2::rect::Rect::new(
            sx0.max(0),
            sy0.max(0),
            (sx1 - sx0) as u32,
            (sy1 - sy0) as u32,
        );
        let dst_sdl = sdl2::rect::Rect::new(dx, dy, dw, dh);

        let Some(sdl_clip) = sdl_pixel_rect(&phys_clip, &phys_clip) else {
            return;
        };

        let src_slice = src.buf.as_slice();
        let stride = src.stride;
        let src_width = src.width as u32;
        let src_height = src.height as u32;

        let canvas = &mut *self.canvas;
        self.label_cache.with_creator(|creator| {
            let mut tex = match creator.create_texture_streaming(sdl_fmt, src_width, src_height) {
                Ok(t) => t,
                Err(_) => return,
            };
            if tex.update(None, src_slice, stride).is_err() {
                return;
            }
            tex.set_blend_mode(sdl2::render::BlendMode::Blend);

            canvas.set_clip_rect(sdl_clip);
            let _ = canvas.copy(&tex, Some(src_sdl), Some(dst_sdl));
            canvas.set_clip_rect(None);
        });
    }

    fn clear(&mut self, area: &Rect, color: &Color) {
        self.fill_rect(area, area, color, Fixed::ZERO, 255);
    }

    fn draw_label(&mut self, pos: &Point, text: &[u8], clip: &Rect, color: &Color, opa: u8) {
        let phys_pos = self.viewport.point_to_physical(*pos);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        self.label_cache.draw(
            self.canvas,
            &phys_pos,
            text,
            &phys_clip,
            color,
            opa,
            self.viewport.scale(),
        );
    }

    fn flush(&mut self) {}
}
