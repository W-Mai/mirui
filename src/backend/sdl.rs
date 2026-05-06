use alloc::vec;
use alloc::vec::Vec;
use sdl2::EventPump;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use super::{Backend, DisplayInfo, InputEvent};

pub struct SdlBackend {
    canvas: Canvas<Window>,
    texture_creator: TextureCreator<WindowContext>,
    event_pump: EventPump,
    buf: Vec<u8>,
    width: u16,
    height: u16,
}

impl SdlBackend {
    pub fn new(title: &str, width: u16, height: u16) -> Self {
        let sdl = sdl2::init().expect("SDL2 init failed");
        let video = sdl.video().expect("SDL2 video init failed");
        let window = video
            .window(title, width as u32, height as u32)
            .position_centered()
            .build()
            .expect("SDL2 window creation failed");
        let canvas = window.into_canvas().build().expect("SDL2 canvas failed");
        let texture_creator = canvas.texture_creator();
        let event_pump = sdl.event_pump().expect("SDL2 event pump failed");
        let buf = vec![0u8; width as usize * height as usize * 4];

        Self {
            canvas,
            texture_creator,
            event_pump,
            buf,
            width,
            height,
        }
    }
}

impl Backend for SdlBackend {
    fn display_info(&self) -> DisplayInfo {
        DisplayInfo {
            width: self.width,
            height: self.height,
        }
    }

    fn framebuffer(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    fn flush(&mut self) {
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
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => return Some(InputEvent::Quit),
                Event::MouseButtonDown { x, y, .. } => {
                    return Some(InputEvent::Touch { x, y });
                }
                Event::MouseButtonUp { x, y, .. } => {
                    return Some(InputEvent::Release { x, y });
                }
                _ => {}
            }
        }
        None
    }
}
