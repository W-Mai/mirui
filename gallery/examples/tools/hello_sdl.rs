use mirui::render::texture::{ColorFormat, Texture};
use mirui::render::{DrawCommand, Renderer, SwRenderer};
use mirui::types::{Color, Fixed, Rect, Transform};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

const W: u32 = 480;
const H: u32 = 320;

fn main() {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("mirui - hello", W, H)
        .position_centered()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas().build().unwrap();
    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGBA32, W, H)
        .unwrap();

    let mut buf = vec![0u8; (W * H * 4) as usize];
    let clip = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: Fixed::from_int(W as i32),
        h: Fixed::from_int(H as i32),
    };

    // Draw with mirui's SwRenderer
    let mut renderer = SwRenderer::new(Texture::new(
        &mut buf,
        W as u16,
        H as u16,
        ColorFormat::RGBA8888,
    ));

    // Background
    renderer.draw(
        &DrawCommand::Fill {
            area: Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(W as i32),
                h: Fixed::from_int(H as i32),
            },
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(30, 30, 46),
            radius: Fixed::ZERO,
            opa: 255,
        },
        &clip,
    );

    // Blue rectangle
    renderer.draw(
        &DrawCommand::Fill {
            area: Rect {
                x: Fixed::from_int(40),
                y: Fixed::from_int(40),
                w: Fixed::from_int(200),
                h: Fixed::from_int(120),
            },
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(88, 166, 255),
            radius: Fixed::ZERO,
            opa: 255,
        },
        &clip,
    );

    // Green rectangle
    renderer.draw(
        &DrawCommand::Fill {
            area: Rect {
                x: Fixed::from_int(140),
                y: Fixed::from_int(100),
                w: Fixed::from_int(200),
                h: Fixed::from_int(120),
            },
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(63, 185, 80),
            radius: Fixed::ZERO,
            opa: 200,
        },
        &clip,
    );

    // Red rectangle
    renderer.draw(
        &DrawCommand::Fill {
            area: Rect {
                x: Fixed::from_int(240),
                y: Fixed::from_int(160),
                w: Fixed::from_int(200),
                h: Fixed::from_int(120),
            },
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(248, 81, 73),
            radius: Fixed::ZERO,
            opa: 180,
        },
        &clip,
    );

    renderer.flush();

    // Blit to SDL
    texture.update(None, &buf, (W * 4) as usize).unwrap();
    canvas.copy(&texture, None, None).unwrap();
    canvas.present();

    // Event loop
    let mut event_pump = sdl.event_pump().unwrap();
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
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}
