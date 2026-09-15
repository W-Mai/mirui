//! `compose_backend!` demo with one software target and a routed blit engine.

use mirui::prelude::*;
use std::cell::RefCell;

use mirui::render::canvas::{Canvas, Paint};
use mirui::render::command::CompositeMode;
use mirui::render::engine::RenderEngine;
use mirui::render::path::Path;
use mirui::render::renderer::{DrawRequest, RenderError, Renderer};
use mirui::render::sw::SwRenderer;
use mirui::render::texture::{ColorFormat, Texture};
use mirui_macros::compose_backend;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

const W: u32 = 480;
const H: u32 = 320;

struct Logging {
    calls: RefCell<u32>,
}

impl Logging {
    fn new() -> Self {
        Self {
            calls: RefCell::new(0),
        }
    }
}

impl<T: Renderer> RenderEngine<T> for Logging {
    fn submit(&mut self, target: &mut T, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        *self.calls.borrow_mut() += 1;
        target.submit(request)
    }
}

compose_backend! {
    pub struct Hybrid {
        sw: SwRenderer,
        gpu: Logging,
    }
    route {
        default => sw,
        blit => gpu,
    }
}

fn main() {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("mirui - compose_backend demo", W, H)
        .position_centered()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas().build().unwrap();
    let texture_creator = canvas.texture_creator();
    let mut sdl_texture = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, W, H)
        .unwrap();

    let mut fb = vec![0u8; (W * H * 4) as usize];

    let sw = SwRenderer::new(Texture::new(
        &mut fb,
        W as u16,
        H as u16,
        ColorFormat::RGBA8888,
    ));
    let gpu = Logging::new();
    let mut hybrid = Hybrid::new(sw, gpu);

    let clip = Rect::new(0, 0, W as u16, H as u16);

    // A small solid-colour sprite we blit each frame. blit is routed to the
    // logging backend, so the stderr trace records every blit.
    let mut sprite_buf = vec![0u8; 16 * 16 * 4];
    for px in sprite_buf.chunks_exact_mut(4) {
        px[0] = 255;
        px[1] = 200;
        px[2] = 80;
        px[3] = 255;
    }
    let sprite = Texture::from_ref(&sprite_buf, 16, 16, ColorFormat::RGBA8888);
    let sprite_rect = Rect::new(0, 0, 16, 16);

    let mut event_pump = sdl.event_pump().unwrap();
    let start = std::time::Instant::now();
    let mut frame: u64 = 0;

    'running: loop {
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                _ => {}
            }
        }

        let t = start.elapsed().as_secs_f32();

        hybrid.clear(&clip, &Color::rgb(30, 30, 46));

        let x = 40.0 + (t * 1.2).sin() * 160.0 + 160.0;
        let path = Path::rounded_rect(
            Fixed::from_f32(x),
            Fixed::from_int(40),
            Fixed::from_int(200),
            Fixed::from_int(120),
            Fixed::from_int(16),
        );
        let path_paint = Paint::Color(Color::rgb(88, 166, 255).into());
        hybrid.fill_path(
            &path,
            &clip,
            &path_paint,
            255,
            ::mirui::render::raster::FillRule::EvenOdd,
        );
        hybrid.draw_line(
            Point {
                x: Fixed::from_int(40),
                y: Fixed::from_int(200),
            },
            Point {
                x: Fixed::from_int(440),
                y: Fixed::from_int(200),
            },
            &clip,
            Fixed::from_int(3),
            &Color::rgb(248, 81, 73),
            255,
        );

        // Sprite train — five blits marching across the bottom, each goes
        // through Logging.
        for i in 0..5 {
            let dx = ((t * 60.0) as i32 + i * 48) % (W as i32 + 32) - 16;
            hybrid.blit(
                &sprite,
                &sprite_rect,
                Point {
                    x: Fixed::from_int(dx),
                    y: Fixed::from_int(260),
                },
                Point {
                    x: Fixed::from(sprite.width),
                    y: Fixed::from(sprite.height),
                },
                &clip,
                255,
                Fixed::ZERO,
                CompositeMode::SourceOver,
            );
        }

        sdl_texture
            .update(None, hybrid.sw.target.buf.as_slice(), (W * 4) as usize)
            .unwrap();
        canvas.copy(&sdl_texture, None, None).unwrap();
        canvas.present();

        frame += 1;
        if frame % 60 == 0 {
            eprintln!(
                "[summary] frame {frame}, Logging invocations so far: {}",
                hybrid.gpu.calls.borrow()
            );
        }

        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
