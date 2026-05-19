use alloc::vec;
use alloc::vec::Vec;
use sdl2::EventPump;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use super::{DisplayInfo, FramebufferAccess, InputEvent, Surface, logical_from_physical};
use crate::draw::texture::{ColorFormat, Texture};
use crate::types::{Fixed, Rect};

pub struct SdlSurface {
    canvas: Canvas<Window>,
    texture_creator: TextureCreator<WindowContext>,
    event_pump: EventPump,
    buf: Vec<u8>,
    width: u16,
    height: u16,
    scale: Fixed,
}

impl SdlSurface {
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
        let mut canvas_builder = window.into_canvas();
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

        // Physical pixel framebuffer
        let phys_w = width * scale_int;
        let phys_h = height * scale_int;
        let buf = vec![0u8; phys_w as usize * phys_h as usize * 4];

        Self {
            canvas,
            texture_creator,
            event_pump,
            buf,
            width: phys_w,
            height: phys_h,
            scale,
        }
    }

    pub fn scale_factor(&self) -> Fixed {
        self.scale
    }
}

impl Surface for SdlSurface {
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
        sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");
        let mut texture = self
            .texture_creator
            .create_texture_streaming(
                PixelFormatEnum::RGBA32,
                self.width as u32,
                self.height as u32,
            )
            .expect("texture creation failed");
        texture
            .update(None, &self.buf, self.width as usize * 4)
            .expect("texture update failed");
        self.canvas.copy(&texture, None, None).expect("copy failed");
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
                Event::MouseMotion { x, y, .. } => {
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
}

impl FramebufferAccess for SdlSurface {
    fn framebuffer(&mut self) -> Texture<'_> {
        Texture::new(
            &mut self.buf,
            self.width,
            self.height,
            ColorFormat::RGBA8888,
        )
    }
}
