extern crate std;
use super::*;
use crate::render::sw::SwRenderer;
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Color, Dimension, Viewport};
use crate::ui::layout::LayoutStyle;
use crate::ui::{Children, Hidden, Parent, Style, Widget};

fn spawn(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
    let e = world.spawn_empty();
    world.insert(e, Widget);
    world.insert(e, style);
    if let Some(p) = parent {
        world.insert(e, Parent(p));
        if let Some(c) = world.get_mut::<Children>(p) {
            c.0.push(e);
        } else {
            world.insert(p, Children(std::vec![e]));
        }
    }
    e
}

fn make_world() -> World {
    {
        let mut app = crate::app::App::headless(64, 64);
        app.with_default_widgets();
        app.world
    }
}

#[test]
fn hidden_widget_does_not_paint() {
    let mut world = make_world();
    let root = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let child = spawn(
        &mut world,
        Some(root),
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(child, Hidden);

    let mut buf = std::vec![0u8; 32 * 32 * 4];
    let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(32, 32, Fixed::ONE);
    super::render(&world, root, &viewport, &mut renderer).unwrap();

    for x in 0..32 {
        let c = renderer.target.read_pixels().unwrap().get_pixel(x, 16);
        assert_eq!(
            c.r, 0,
            "x={x} should be root bg (black), Hidden child leaked red ({})",
            c.r
        );
    }
}
