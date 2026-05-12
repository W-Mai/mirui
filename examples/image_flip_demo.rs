use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::components::transform_3d::WidgetTransform3D;
use mirui::ecs::{Entity, World};
use mirui::layout::*;
use mirui::types::{Color, Dimension, Fixed, Transform3D};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui::widget::{Children, Parent};

extern crate alloc;

struct Spinner {
    angle: Fixed,
    speed: Fixed,
    root: Entity,
}

fn spin_system(world: &mut World) {
    let mut entities = alloc::vec::Vec::new();
    world.query::<Spinner>().collect_into(&mut entities);
    for e in entities {
        let (angle, root) = if let Some(s) = world.get_mut::<Spinner>(e) {
            s.angle += s.speed;
            if s.angle >= Fixed::from_int(360) {
                s.angle -= Fixed::from_int(360);
            }
            (s.angle, s.root)
        } else {
            continue;
        };
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
    let backend = SdlBackend::new("mirui - 2.5D image flip demo", 480, 320);
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

    let img_widget = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px(160),
            top: Dimension::px(80),
            width: Dimension::px(160),
            height: Dimension::px(160),
            ..Default::default()
        })
        .id();
    app.world.insert(img_widget, Image::new(&IMG_THUMBS_UP));
    app.world.insert(
        img_widget,
        Spinner {
            angle: Fixed::ZERO,
            speed: Fixed::ONE,
            root,
        },
    );
    app.world.insert(img_widget, Parent(root));
    if let Some(children) = app.world.get_mut::<Children>(root) {
        children.0.push(img_widget);
    }

    app.set_root(root);
    app.run();
}
