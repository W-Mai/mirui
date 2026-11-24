extern crate std;
use super::*;
use crate::render::sw::SwRenderer;
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Color, Dimension, Viewport};
use crate::ui::layout::LayoutStyle;
use crate::ui::theme::{Theme, WidgetState};
use crate::ui::{Children, Parent, Style, UserState, Widget};

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
        let mut app = crate::app::App::headless(32, 32);
        app.with_default_widgets();
        app.world
    }
}

#[test]
fn disabled_subtree_paints_blended_raw_bg() {
    let mut world = make_world();
    let theme = world
        .resource::<Theme>()
        .expect("Theme present after with_default_widgets")
        .clone();
    let red = Color::rgb(248, 81, 73);
    let root = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(red.into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(root, UserState::Disabled);

    let mut buf = std::vec![0u8; 32 * 32 * 4];
    let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(32, 32, Fixed::ONE);
    super::render(&world, root, &viewport, &mut renderer).unwrap();

    let expected = theme.blend_color_in(red, WidgetState::Disabled);
    let actual = renderer.target.read_pixels().unwrap().get_pixel(16, 16);
    assert_eq!(
        actual.r, expected.r,
        "r mismatch: got {} want {}",
        actual.r, expected.r
    );
    assert_eq!(
        actual.g, expected.g,
        "g mismatch: got {} want {}",
        actual.g, expected.g
    );
    assert_eq!(
        actual.b, expected.b,
        "b mismatch: got {} want {}",
        actual.b, expected.b
    );
}
