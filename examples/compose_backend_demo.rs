//! compose_backend! demo. A LoggingBackend wraps a second SwRenderer and
//! counts every method call. Hybrid routes blit + clear through Logging, and
//! everything else (fill_path / stroke_path / draw_line / draw_arc) through
//! the plain sw backend. Every second the counter is printed to stderr so the
//! routing stays visible without flooding the trace per-call.

use std::cell::RefCell;

use mirui::draw::canvas::Canvas;
use mirui::draw::path::Path;
use mirui::draw::sw::SwRenderer;
use mirui::draw::texture::{ColorFormat, Texture};
use mirui::types::{Color, Fixed, Point, Rect};
use mirui_macros::compose_backend;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

const W: u32 = 480;
const H: u32 = 320;

/// Any Canvas wrapped in log lines. Uses RefCell for the counter so the
/// example doesn't need `&mut self` on the outer wrapper just to bump it.
struct Logging<B: Canvas> {
    inner: B,
    calls: RefCell<u32>,
}

impl<B: Canvas> Logging<B> {
    fn new(inner: B) -> Self {
        Self {
            inner,
            calls: RefCell::new(0),
        }
    }
    fn log(&self, _what: &str) {
        *self.calls.borrow_mut() += 1;
    }
}

impl<B: Canvas> Canvas for Logging<B> {
    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        self.log("fill_path");
        self.inner.fill_path(path, clip, color, opa);
    }
    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        self.log("stroke_path");
        self.inner.stroke_path(path, clip, width, color, opa);
    }
    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, dst_size: Point, clip: &Rect) {
        self.log("blit");
        self.inner.blit(src, src_rect, dst, dst_size, clip);
    }
    fn clear(&mut self, area: &Rect, color: &Color) {
        self.log("clear");
        self.inner.clear(area, color);
    }
    fn draw_label(&mut self, pos: &Point, text: &[u8], clip: &Rect, color: &Color, opa: u8) {
        self.log("draw_label");
        self.inner.draw_label(pos, text, clip, color, opa);
    }
    fn flush(&mut self) {
        self.log("flush");
        self.inner.flush();
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
        clear => gpu,
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

    // Each backend owns a separate framebuffer. `clear` routes through the
    // logging backend onto a throwaway buffer so the trace is visible; all
    // the path drawing routes through sw onto `fb`, which is what SDL shows.
    let mut fb = vec![0u8; (W * H * 4) as usize];
    let mut gpu_fb = vec![0u8; (W * H * 4) as usize];

    let sw = SwRenderer::new(Texture::new(
        &mut fb,
        W as u16,
        H as u16,
        ColorFormat::ARGB8888,
    ));
    let gpu_inner = SwRenderer::new(Texture::new(
        &mut gpu_fb,
        W as u16,
        H as u16,
        ColorFormat::ARGB8888,
    ));
    let gpu = Logging::new(gpu_inner);

    let mut hybrid = Hybrid { sw, gpu };

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
    let sprite = Texture::from_ref(&sprite_buf, 16, 16, ColorFormat::ARGB8888);
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

        // Every frame: clear routes through Logging; path + line go sw.
        hybrid.clear(&clip, &Color::rgb(30, 30, 46));
        hybrid.sw.clear(&clip, &Color::rgb(30, 30, 46));

        let x = 40.0 + (t * 1.2).sin() * 160.0 + 160.0;
        let path = Path::rounded_rect(
            Fixed::from_f32(x),
            Fixed::from_int(40),
            Fixed::from_int(200),
            Fixed::from_int(120),
            Fixed::from_int(16),
        );
        hybrid.fill_path(&path, &clip, &Color::rgb(88, 166, 255), 255);
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
