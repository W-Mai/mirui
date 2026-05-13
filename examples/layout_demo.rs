use mirui::draw::texture::{ColorFormat, Texture};
use mirui::draw::{DrawCommand, Renderer, SwRenderer};
use mirui::layout::*;
use mirui::types::{Color, Dimension, Fixed, Rect, Transform};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

const W: u32 = 480;
const H: u32 = 320;

const COLORS: &[Color] = &[
    Color {
        r: 88,
        g: 166,
        b: 255,
        a: 255,
    },
    Color {
        r: 63,
        g: 185,
        b: 80,
        a: 255,
    },
    Color {
        r: 248,
        g: 81,
        b: 73,
        a: 255,
    },
    Color {
        r: 210,
        g: 168,
        b: 255,
        a: 255,
    },
    Color {
        r: 227,
        g: 179,
        b: 65,
        a: 255,
    },
];

fn draw_node(renderer: &mut SwRenderer, node: &LayoutNode, clip: &Rect, depth: usize) {
    if depth > 0 {
        let color = COLORS[(depth - 1) % COLORS.len()];
        renderer.draw(
            &DrawCommand::Fill {
                area: node.rect,
                transform: Transform::IDENTITY,
                quad: None,
                color,
                radius: Fixed::ZERO,
                opa: 220,
            },
            clip,
        );
    }
    for child in &node.children {
        draw_node(renderer, child, clip, depth + 1);
    }
}

fn main() {
    // Build layout tree
    let mut root = LayoutNode::new(LayoutStyle {
        direction: FlexDirection::Column,
        justify: JustifyContent::SpaceBetween,
        padding: Padding {
            top: 20.into(),
            right: 20.into(),
            bottom: 20.into(),
            left: 20.into(),
        },
        width: Dimension::px(W as i32),
        height: Dimension::px(H as i32),
        ..Default::default()
    });

    // Top row: 3 equal boxes
    let mut top_row = LayoutNode::new(LayoutStyle {
        direction: FlexDirection::Row,
        justify: JustifyContent::SpaceBetween,
        height: Dimension::px(80),
        ..Default::default()
    });
    for _ in 0..3 {
        top_row.add_child(LayoutNode::new(LayoutStyle {
            width: Dimension::px(130),
            height: Dimension::px(80),
            ..Default::default()
        }));
    }

    // Bottom row: grow
    let mut bottom_row = LayoutNode::new(LayoutStyle {
        direction: FlexDirection::Row,
        justify: JustifyContent::FlexStart,
        height: Dimension::px(160),
        ..Default::default()
    });
    bottom_row.add_child(LayoutNode::new(LayoutStyle {
        grow: Fixed::from_f32(1.0),
        height: Dimension::px(160),
        ..Default::default()
    }));
    bottom_row.add_child(LayoutNode::new(LayoutStyle {
        grow: Fixed::from_f32(2.0),
        height: Dimension::px(160),
        ..Default::default()
    }));

    root.add_child(top_row);
    root.add_child(bottom_row);

    compute_layout(
        &mut root,
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(W as i32),
        Fixed::from_int(H as i32),
    );

    // Render
    let mut buf = vec![0u8; (W * H * 4) as usize];
    let clip = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: Fixed::from_int(W as i32),
        h: Fixed::from_int(H as i32),
    };
    let mut renderer = SwRenderer::new(Texture::new(
        &mut buf,
        W as u16,
        H as u16,
        ColorFormat::ARGB8888,
    ));

    // Background
    renderer.draw(
        &DrawCommand::Fill {
            area: clip,
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(30, 30, 46),
            radius: Fixed::ZERO,
            opa: 255,
        },
        &clip,
    );

    draw_node(&mut renderer, &root, &clip, 0);
    renderer.flush();

    // SDL window
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video
        .window("mirui - layout", W, H)
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
