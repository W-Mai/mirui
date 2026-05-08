use mirui::draw::{Renderer, SwRenderer};
use mirui::ecs::World;
use mirui::layout::*;
use mirui::types::{Color, Dimension};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::render_system;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

const W: u32 = 480;
const H: u32 = 320;

fn main() {
    let mut world = World::new();

    // Create child widgets
    let c1 = WidgetBuilder::new(&mut world)
        .bg_color(Color::rgb(88, 166, 255))
        .layout(LayoutStyle {
            width: Dimension::px(120),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let c2 = WidgetBuilder::new(&mut world)
        .bg_color(Color::rgb(63, 185, 80))
        .layout(LayoutStyle {
            width: Dimension::px(120),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    let c3 = WidgetBuilder::new(&mut world)
        .bg_color(Color::rgb(248, 81, 73))
        .layout(LayoutStyle {
            width: Dimension::px(120),
            height: Dimension::px(80),
            ..Default::default()
        })
        .id();

    // Create root container
    let root = WidgetBuilder::new(&mut world)
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
        .child(c1)
        .child(c2)
        .child(c3)
        .id();

    // Render via ECS
    let mut buf = vec![0u8; (W * H * 4) as usize];
    let mut renderer = SwRenderer::new(&mut buf, W, H);
    render_system::render(&world, root, W as u16, H as u16, 1, &mut renderer);
    renderer.flush();

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
    texture.update(None, &buf, (W * 4) as usize).unwrap();
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
