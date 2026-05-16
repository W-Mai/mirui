use mirui::app::App;
use mirui::components::transform_3d::WidgetTransform3D;
use mirui::ecs::World;
use mirui::layout::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed, Transform3D};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui::widget::{Children, Parent, Style};

extern crate alloc;

struct FlipCard {
    angle_deg: Fixed,
    speed_deg: Fixed,
    front_color: Color,
    back_color: Color,
    root: mirui::ecs::Entity,
}

fn flip_system(world: &mut World) {
    let mut cards = alloc::vec::Vec::new();
    world.query::<FlipCard>().collect_into(&mut cards);
    for e in cards {
        let (angle, front, back, root) = if let Some(c) = world.get_mut::<FlipCard>(e) {
            c.angle_deg += c.speed_deg;
            if c.angle_deg >= Fixed::from_int(360) {
                c.angle_deg -= Fixed::from_int(360);
            }
            (c.angle_deg, c.front_color, c.back_color, c.root)
        } else {
            continue;
        };

        let halfway = Fixed::from_int(90);
        let three_quarters = Fixed::from_int(270);
        let color = if angle < halfway || angle >= three_quarters {
            front
        } else {
            back
        };
        if let Some(style) = world.get_mut::<Style>(e) {
            style.bg_color = Some(color);
        }

        world.insert(
            e,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                angle,
                Fixed::from_int(400),
            )),
        );
        world.insert(root, Dirty);
    }
}

fn main() {
    let backend = SdlSurface::new("mirui - 2.5D flip card demo", 480, 320);
    let mut app = App::new(backend);

    app.add_system(flip_system);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let card = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(88, 166, 255))
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(140),
            top: Dimension::px(70),
            width: Dimension::px(200),
            height: Dimension::px(180),
            ..Default::default()
        })
        .id();
    app.world.insert(
        card,
        FlipCard {
            angle_deg: Fixed::ZERO,
            speed_deg: Fixed::ONE,
            front_color: Color::rgb(88, 166, 255),
            back_color: Color::rgb(248, 81, 73),
            root,
        },
    );

    app.world.insert(card, Parent(root));
    if let Some(children) = app.world.get_mut::<Children>(root) {
        children.0.push(card);
    }

    app.set_root(root);
    app.run();
}
