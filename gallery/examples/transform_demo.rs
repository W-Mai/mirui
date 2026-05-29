use gallery::prelude::*;
use mirui::components::assets::*;
use mirui::components::{Image, WidgetTransform};
use mirui::types::Transform;
use mirui::widget::{Children, Parent};

extern crate alloc;

struct Spinner {
    angle: Fixed,
    speed: Fixed,
}

#[mirui::system(order = ANIMATION)]
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
    gallery::run("mirui - transform demo", 480, 320, |setup| {
        setup.app.add_system(spin_system::system());
        let world = &mut setup.app.world;

        let root = WidgetBuilder::new(world)
            .bg_color(Color::rgb(30, 30, 46))
            .layout(LayoutStyle {
                direction: FlexDirection::Column,
                width: Dimension::px(480),
                height: Dimension::px(320),
                ..Default::default()
            })
            .id();

        let spinning_box = WidgetBuilder::new(world)
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
        world.insert(
            spinning_box,
            Spinner {
                angle: Fixed::ZERO,
                speed: Fixed::from_int(2),
            },
        );

        let spinning_img = WidgetBuilder::new(world)
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
        world.insert(spinning_img, Image::new(&IMG_THUMBS_UP));
        world.insert(
            spinning_img,
            Spinner {
                angle: Fixed::ZERO,
                speed: Fixed::from_int(3),
            },
        );

        for &entity in &[spinning_box, spinning_img] {
            world.insert(entity, Parent(root));
            if let Some(children) = world.get_mut::<Children>(root) {
                children.0.push(entity);
            }
        }

        root
    });
}
