//! GPU-accelerated SDL backend.
//!
//! Where [`super::sdl::SdlBackend`] keeps a CPU byte buffer and uploads it
//! each frame, `SdlGpuBackend` drives the SDL2 accelerated renderer
//! directly: `canvas.fill_rect`, `canvas.copy`, and ultimately
//! `SDL_RenderGeometry` (unsafe FFI) for tessellated paths. No CPU
//! framebuffer, no `FramebufferAccess` impl.

use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

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
    #[allow(dead_code)] // held for future label-cache texture uploads
    texture_creator: TextureCreator<WindowContext>,
    event_pump: EventPump,
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
            width: phys_w,
            height: phys_h,
            scale,
        }
    }

    pub(crate) fn canvas_mut(&mut self) -> &mut Canvas<Window> {
        &mut self.canvas
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
        SdlGpuRenderer {
            canvas: backend.canvas_mut(),
            scale,
        }
    }
}

pub struct SdlGpuRenderer<'a> {
    canvas: &'a mut Canvas<Window>,
    #[allow(dead_code)] // used by physical-pixel math once non-rect primitives land
    scale: Fixed,
}

impl Renderer for SdlGpuRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        if let DrawCommand::Fill {
            area,
            color,
            radius,
            opa,
        } = cmd
        {
            self.fill_rect(area, clip, color, *radius, *opa);
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

    fn fill_path(&mut self, _path: &Path, _clip: &Rect, _color: &Color, _opa: u8) {
        todo!("fill_path needs tessellation + SDL_RenderGeometry")
    }

    fn stroke_path(&mut self, _path: &Path, _clip: &Rect, _width: Fixed, _color: &Color, _opa: u8) {
        todo!("stroke_path needs tessellation + SDL_RenderGeometry")
    }

    fn blit(&mut self, _src: &Texture, _src_rect: &Rect, _dst: Point, _clip: &Rect) {
        todo!("blit needs SdlTexture upload + canvas.copy")
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
