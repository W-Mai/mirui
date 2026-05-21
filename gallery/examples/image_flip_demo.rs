use mirui::app::App;
use mirui::components::assets::*;
use mirui::components::image::Image;
use mirui::components::transform_3d::WidgetTransform3D;
use mirui::ecs::World;
use mirui::layout::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed, Transform3D};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui::widget::{Children, Parent};

extern crate alloc;

struct Spinner {
    angle: Fixed,
    speed: Fixed,
    bounce_phase: Fixed,
}

#[mirui::system(order = ANIMATION)]
fn spin_system(world: &mut World) {
    let mut entities = alloc::vec::Vec::new();
    world.query::<Spinner>().collect_into(&mut entities);
    for e in entities {
        let (angle, bounce) = if let Some(s) = world.get_mut::<Spinner>(e) {
            s.angle += s.speed;
            if s.angle >= Fixed::from_int(360) {
                s.angle -= Fixed::from_int(360);
            }
            s.bounce_phase += s.speed;
            if s.bounce_phase >= Fixed::from_int(360) {
                s.bounce_phase -= Fixed::from_int(360);
            }
            (s.angle, s.bounce_phase)
        } else {
            continue;
        };

        let t_num = bounce.to_int() % 180;
        let t = Fixed::from_int(t_num) / Fixed::from_int(180);
        let two_t_minus_1 = t * Fixed::from_int(2) - Fixed::ONE;
        let h = Fixed::ONE - two_t_minus_1 * two_t_minus_1;

        let bounce_y = Fixed::ZERO - h * Fixed::from_int(100);
        let squash = Fixed::ONE - (Fixed::ONE - h) / Fixed::from_int(4);
        let stretch = Fixed::ONE + h / Fixed::from_int(8);

        let rot = Transform3D::rotate_y_perspective(angle, Fixed::from_int(400));
        let scale = Transform3D::scale(squash, stretch);
        let translate = Transform3D::translate(Fixed::ZERO, bounce_y);
        world.insert(
            e,
            WidgetTransform3D(translate.compose(&rot).compose(&scale)),
        );
        world.insert(e, Dirty);
    }
}

fn main() {
    let backend = SdlSurface::new("mirui - 2.5D image flip demo", 480, 320);
    let mut app = App::new(backend);
    app.with_default_widgets();

    app.add_system(spin_system::system());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let side = 120;
    let img_widget = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            position: Position::Absolute,
            left: Dimension::px((480 - side) / 2),
            top: Dimension::px(320 - side - 20),
            width: Dimension::px(side),
            height: Dimension::px(side),
            ..Default::default()
        })
        .id();
    app.world.insert(img_widget, Image::new(&IMG_THUMBS_UP));
    app.world.insert(
        img_widget,
        Spinner {
            angle: Fixed::ZERO,
            speed: Fixed::from_int(3),
            bounce_phase: Fixed::ZERO,
        },
    );
    app.world.insert(img_widget, Parent(root));
    if let Some(children) = app.world.get_mut::<Children>(root) {
        children.0.push(img_widget);
    }

    app.set_root(root);
    app.run();
}
