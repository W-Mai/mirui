use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::components::transform::WidgetTransform;
use mirui::ecs::World;
use mirui::layout::*;
use mirui::types::{Color, Dimension, Fixed, Transform};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::{Children, Parent};

extern crate alloc;

struct Spinner {
    angle: Fixed,
    speed: Fixed,
}

fn spin_system(world: &mut World) {
    let mut entities = alloc::vec::Vec::new();
    world.query::<Spinner>().collect_into(&mut entities);
    for e in entities {
        let next = if let Some(s) = world.get_mut::<Spinner>(e) {
            s.angle += s.speed;
            s.angle
        } else {
            continue;
        };
        world.insert(e, WidgetTransform(Transform::rotate_deg(next)));
    }
}

fn main() {
    let backend = SdlBackend::new("mirui - transform demo", 480, 320);
    let mut app = App::new(backend);

    app.add_system(spin_system);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let spinning_box = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(248, 81, 73))
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(100),
            top: Dimension::px(100),
            width: Dimension::px(80),
            height: Dimension::px(80),
            ..Default::default()
        })
        .rotate(0)
        .id();
    app.world.insert(
        spinning_box,
        Spinner {
            angle: Fixed::ZERO,
            speed: Fixed::from_int(2),
        },
    );

    let spinning_img = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(300),
            top: Dimension::px(120),
            width: Dimension::px(64),
            height: Dimension::px(64),
            ..Default::default()
        })
        .rotate(0)
        .id();
    app.world.insert(spinning_img, Image::new(&IMG_THUMBS_UP));
    app.world.insert(
        spinning_img,
        Spinner {
            angle: Fixed::ZERO,
            speed: Fixed::from_int(3),
        },
    );

    for &entity in &[spinning_box, spinning_img] {
        app.world.insert(entity, Parent(root));
        if let Some(children) = app.world.get_mut::<Children>(root) {
            children.0.push(entity);
        }
    }

    app.set_root(root);
    app.run();
}
