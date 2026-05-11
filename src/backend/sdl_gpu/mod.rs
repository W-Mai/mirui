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
use crate::types::{Color, CoordTransform, Fixed, Point, Rect};

use super::{Backend, DisplayInfo, InputEvent};

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
        DisplayInfo {
            width: self.width,
            height: self.height,
            scale: self.scale,
            format: ColorFormat::ARGB8888,
        }
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
        transform: &CoordTransform,
    ) -> SdlGpuRenderer<'a> {
        let scale = transform.scale();
        let (canvas, label_cache, tessellator) = backend.parts_mut();
        SdlGpuRenderer {
            canvas,
            label_cache,
            tessellator,
            scale,
        }
    }
}

pub struct SdlGpuRenderer<'a> {
    canvas: &'a mut Canvas<Window>,
    label_cache: &'a mut LabelCache,
    tessellator: &'a mut TessellationCache,
    scale: Fixed,
}

impl SdlGpuRenderer<'_> {
    fn submit_geometry(&mut self, clip: &Rect, needs_blend: bool) {
        if self.tessellator.indices.is_empty() {
            return;
        }
        let Some(clip_rect) = sdl_pixel_rect(clip, clip) else {
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
        match cmd {
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
            } => self.fill_rect(area, clip, color, *radius, *opa),
            DrawCommand::Border {
                area,
                color,
                width,
                radius,
                opa,
            } => self.stroke_rect(area, clip, *width, color, *radius, *opa),
            DrawCommand::Line {
                p1,
                p2,
                color,
                width,
                opa,
            } => self.draw_line(*p1, *p2, clip, *width, color, *opa),
            DrawCommand::Blit { pos, texture } => {
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                self.blit(texture, &src_rect, *pos, clip);
            }
            DrawCommand::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
            } => self.draw_arc(
                *center,
                *radius,
                *start_angle,
                *end_angle,
                clip,
                *width,
                color,
                *opa,
            ),
            DrawCommand::Label {
                pos,
                text,
                color,
                opa,
            } => self.draw_label(pos, text, clip, color, *opa),
        }
    }

    fn flush(&mut self) {}
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
        if radius == Fixed::ZERO {
            if let Some(sdl_rect) = sdl_pixel_rect(area, clip) {
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
            if let Some(sdl_rect) = sdl_pixel_rect(area, clip) {
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
            let Some(clip_rect) = sdl_pixel_rect(clip, clip) else {
                return;
            };
            self.canvas.set_clip_rect(clip_rect);
            apply_solid_color(self.canvas, color, opa);
            let _ = self.canvas.draw_line(
                sdl2::rect::Point::new(p1.x.to_int(), p1.y.to_int()),
                sdl2::rect::Point::new(p2.x.to_int(), p2.y.to_int()),
            );
            self.canvas.set_clip_rect(None);
            return;
        }
        let mut path = Path::new();
        path.move_to(p1).line_to(p2);
        self.stroke_path(&path, clip, width, color, opa);
    }

    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        self.tessellator.fill(path, color, opa);
        self.submit_geometry(clip, opa != 255 || color.a != 255);
    }

    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        let physical_width = width.to_f32().max(1.0);
        self.tessellator.stroke(path, physical_width, color, opa);
        self.submit_geometry(clip, opa != 255 || color.a != 255);
    }

    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, clip: &Rect) {
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

        let dx = dst.x.to_int();
        let dy = dst.y.to_int();
        let dw = src_rect.w.to_int() as u32;
        let dh = src_rect.h.to_int() as u32;
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

        let Some(sdl_clip) = sdl_pixel_rect(clip, clip) else {
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
        self.label_cache
            .draw(self.canvas, pos, text, clip, color, opa, self.scale);
    }

    fn flush(&mut self) {}
}
