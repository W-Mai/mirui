//! GPU-accelerated SDL backend.
//!
//! Where [`super::sdl::SdlBackend`] keeps a CPU byte buffer and uploads it
//! each frame, `SdlGpuBackend` drives the SDL2 accelerated renderer
//! directly: `canvas.fill_rect`, `canvas.copy`, and ultimately
//! `SDL_RenderGeometry` (unsafe FFI) for tessellated paths. No CPU
//! framebuffer, no `FramebufferAccess` impl.

mod tessellation;

use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

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
    texture_creator: TextureCreator<WindowContext>,
    event_pump: EventPump,
    tessellator: TessellationCache,
    width: u16,
    height: u16,
    scale: Fixed,
}

impl SdlGpuBackend {
    pub fn new(title: &str, width: u16, height: u16) -> Self {
        let sdl = sdl2::init().expect("SDL2 init failed");
        let video = sdl.video().expect("SDL2 video init failed");
        let window = video
            .window(title, width as u32, height as u32)
            .position_centered()
            .allow_highdpi()
            .build()
            .expect("SDL2 window creation failed");
        let canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .expect("SDL2 canvas failed");
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
            texture_creator,
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
        &mut Canvas<Window>,
        &TextureCreator<WindowContext>,
        &mut TessellationCache,
    ) {
        (
            &mut self.canvas,
            &self.texture_creator,
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
        let (canvas, texture_creator, tessellator) = backend.parts_mut();
        SdlGpuRenderer {
            canvas,
            texture_creator,
            tessellator,
            scale,
        }
    }
}

pub struct SdlGpuRenderer<'a> {
    canvas: &'a mut Canvas<Window>,
    texture_creator: &'a TextureCreator<WindowContext>,
    tessellator: &'a mut TessellationCache,
    scale: Fixed,
}

impl SdlGpuRenderer<'_> {
    fn submit_geometry(&mut self, clip: &Rect, needs_blend: bool) {
        if self.tessellator.indices.is_empty() {
            return;
        }
        let (cx0, cy0, cx1, cy1) = physical_clip_rect(clip, clip, self.scale);
        let cw = (cx1 - cx0).max(0) as u32;
        let ch = (cy1 - cy0).max(0) as u32;
        if cw == 0 || ch == 0 {
            return;
        }
        self.canvas
            .set_clip_rect(sdl2::rect::Rect::new(cx0, cy0, cw, ch));
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
            DrawCommand::Label { .. } => {}
        }
    }

    fn flush(&mut self) {}
}

/// Convert a logical `area` + `clip` to the integer physical-pixel
/// intersection, suitable for SDL's integer-rect API. Returns
/// `(x0, y0, x1, y1)` in physical pixels; caller checks non-empty.
fn physical_clip_rect(area: &Rect, clip: &Rect, scale: Fixed) -> (i32, i32, i32, i32) {
    let ax0 = (area.x * scale).to_int();
    let ay0 = (area.y * scale).to_int();
    let ax1 = ((area.x + area.w) * scale).ceil().to_int();
    let ay1 = ((area.y + area.h) * scale).ceil().to_int();
    let cx0 = (clip.x * scale).to_int();
    let cy0 = (clip.y * scale).to_int();
    let cx1 = ((clip.x + clip.w) * scale).ceil().to_int();
    let cy1 = ((clip.y + clip.h) * scale).ceil().to_int();
    (ax0.max(cx0), ay0.max(cy0), ax1.min(cx1), ay1.min(cy1))
}

impl DrawBackend for SdlGpuRenderer<'_> {
    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        if radius == Fixed::ZERO {
            let (x0, y0, x1, y1) = physical_clip_rect(area, clip, self.scale);
            if x1 <= x0 || y1 <= y0 {
                return;
            }
            let sdl_rect = sdl2::rect::Rect::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32);
            let a = ((color.a as u16) * (opa as u16) / 255) as u8;
            self.canvas.set_blend_mode(if a == 255 {
                sdl2::render::BlendMode::None
            } else {
                sdl2::render::BlendMode::Blend
            });
            self.canvas
                .set_draw_color(sdl2::pixels::Color::RGBA(color.r, color.g, color.b, a));
            let _ = self.canvas.fill_rect(sdl_rect);
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
        let one = Fixed::ONE;
        if radius == Fixed::ZERO && width == one {
            let (x0, y0, x1, y1) = physical_clip_rect(area, clip, self.scale);
            if x1 <= x0 || y1 <= y0 {
                return;
            }
            let sdl_rect = sdl2::rect::Rect::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32);
            let a = ((color.a as u16) * (opa as u16) / 255) as u8;
            self.canvas.set_blend_mode(if a == 255 {
                sdl2::render::BlendMode::None
            } else {
                sdl2::render::BlendMode::Blend
            });
            self.canvas
                .set_draw_color(sdl2::pixels::Color::RGBA(color.r, color.g, color.b, a));
            let _ = self.canvas.draw_rect(sdl_rect);
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
            let scale = self.scale;
            let x0 = (p1.x * scale).to_int();
            let y0 = (p1.y * scale).to_int();
            let x1 = (p2.x * scale).to_int();
            let y1 = (p2.y * scale).to_int();
            let (cx0, cy0, cx1, cy1) = physical_clip_rect(clip, clip, scale);
            self.canvas.set_clip_rect(sdl2::rect::Rect::new(
                cx0,
                cy0,
                (cx1 - cx0).max(0) as u32,
                (cy1 - cy0).max(0) as u32,
            ));
            let a = ((color.a as u16) * (opa as u16) / 255) as u8;
            self.canvas.set_blend_mode(if a == 255 {
                sdl2::render::BlendMode::None
            } else {
                sdl2::render::BlendMode::Blend
            });
            self.canvas
                .set_draw_color(sdl2::pixels::Color::RGBA(color.r, color.g, color.b, a));
            let _ = self.canvas.draw_line(
                sdl2::rect::Point::new(x0, y0),
                sdl2::rect::Point::new(x1, y1),
            );
            self.canvas.set_clip_rect(None);
            return;
        }
        let mut path = Path::new();
        path.move_to(p1).line_to(p2);
        self.stroke_path(&path, clip, width, color, opa);
    }

    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        self.tessellator.fill(path, self.scale, color, opa);
        self.submit_geometry(clip, opa != 255 || color.a != 255);
    }

    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        let physical_width = (width * self.scale).to_f32().max(1.0);
        self.tessellator
            .stroke(path, self.scale, physical_width, color, opa);
        self.submit_geometry(clip, opa != 255 || color.a != 255);
    }

    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, clip: &Rect) {
        let scale = self.scale;

        let dx = (dst.x * scale).to_int();
        let dy = (dst.y * scale).to_int();
        let dw = (src_rect.w * scale).to_int() as u32;
        let dh = (src_rect.h * scale).to_int() as u32;
        if dw == 0 || dh == 0 {
            return;
        }

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

        let src_slice = src.buf.as_slice();
        let stride = src.stride;

        let mut tex = match self.texture_creator.create_texture_streaming(
            sdl_fmt,
            src.width as u32,
            src.height as u32,
        ) {
            Ok(t) => t,
            Err(_) => return,
        };
        if tex.update(None, src_slice, stride).is_err() {
            return;
        }
        tex.set_blend_mode(sdl2::render::BlendMode::Blend);

        let sx = (src_rect.x * scale).to_int().max(0);
        let sy = (src_rect.y * scale).to_int().max(0);
        let sw = (src_rect.w * scale).to_int() as u32;
        let sh = (src_rect.h * scale).to_int() as u32;
        let src_sdl = sdl2::rect::Rect::new(sx, sy, sw, sh);
        let dst_sdl = sdl2::rect::Rect::new(dx, dy, dw, dh);

        let (cx0, cy0, cx1, cy1) = physical_clip_rect(clip, clip, scale);
        self.canvas.set_clip_rect(sdl2::rect::Rect::new(
            cx0,
            cy0,
            (cx1 - cx0).max(0) as u32,
            (cy1 - cy0).max(0) as u32,
        ));

        let _ = self.canvas.copy(&tex, Some(src_sdl), Some(dst_sdl));
        self.canvas.set_clip_rect(None);
    }

    fn clear(&mut self, area: &Rect, color: &Color) {
        let full = (area.w * self.scale).to_int() as u32;
        let full_h = (area.h * self.scale).to_int() as u32;
        if area.x == Fixed::ZERO && area.y == Fixed::ZERO && full > 0 && full_h > 0 {
            self.canvas.set_draw_color(sdl2::pixels::Color::RGBA(
                color.r, color.g, color.b, color.a,
            ));
            self.canvas.clear();
            return;
        }
        self.fill_rect(area, area, color, Fixed::ZERO, 255);
    }

    fn draw_label(&mut self, _pos: &Point, _text: &[u8], _clip: &Rect, _color: &Color, _opa: u8) {
        todo!("draw_label needs CPU raster + texture cache")
    }

    fn flush(&mut self) {}
}
