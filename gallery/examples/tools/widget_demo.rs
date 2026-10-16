use mirui::prelude::*;
use mirui::surface::FramebufferAccess;
use mirui::types::{Color, Dimension};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

const W: u32 = 480;
const H: u32 = 320;

#[compose]
fn colored_tiles() {
    ui! {
        walk [Color::rgb(88, 166, 255), Color::rgb(63, 185, 80), Color::rgb(248, 81, 73)] with color {
            View (bg_color: color, width: 120, height: 80)
        }
    };
}

fn main() {
    let mut app = App::headless(W as u16, H as u16);
    app.with_default_widgets();

    let root = app
        .spawn_root()
        .ignore_safe_area()
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Row,
            justify: JustifyContent::SpaceEvenly,
            align: AlignItems::Center,
            width: Dimension::px(W as i32),
            height: Dimension::px(H as i32),
            padding: Padding {
                top: 20.into(),
                right: 20.into(),
                bottom: 20.into(),
                left: 20.into(),
            },
            ..Default::default()
        })
        .id();

    app.compose(root, colored_tiles());
    app.render().unwrap();

    // SDL display
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("mirui - ECS widget", W, H)
        .position_centered()
        .build()
        .unwrap();
    let mut canvas = window.into_canvas().build().unwrap();
    let tc = canvas.texture_creator();
    let mut texture = tc
        .create_texture_streaming(PixelFormatEnum::RGBA32, W, H)
        .unwrap();
    let frame = app.backend.framebuffer();
    texture
        .update(
            None,
            frame.read_pixels().expect("read pixels").bytes(),
            frame.read_pixels().expect("read pixels").stride(),
        )
        .unwrap();
    canvas.copy(&texture, None, None).unwrap();
    canvas.present();

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
