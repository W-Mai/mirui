use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::components::transform_3d::{TransformOrigin, WidgetTransform3D};
use mirui::ecs::World;
use mirui::layout::*;
use mirui::types::{Color, Dimension, Fixed, Transform3D};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;

extern crate alloc;

struct Page {
    angle_deg: Fixed,
    speed_deg: Fixed,
}

fn flip_system(world: &mut World) {
    let mut pages = alloc::vec::Vec::new();
    world.query::<Page>().collect_into(&mut pages);
    for e in pages {
        let angle = if let Some(p) = world.get_mut::<Page>(e) {
            p.angle_deg += p.speed_deg;
            // Bounce 0..120..0 rather than full 0..180, so the flipping
            // right page never swings past the spine and covers the left.
            if p.angle_deg > Fixed::from_int(120) || p.angle_deg < Fixed::ZERO {
                p.speed_deg = -p.speed_deg;
                p.angle_deg += p.speed_deg;
            }
            p.angle_deg
        } else {
            continue;
        };
        world.insert(
            e,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                Fixed::ZERO - angle,
                Fixed::from_int(500),
            )),
        );
        world.insert(e, Dirty);
    }
}

fn main() {
    let backend = SdlBackend::new("mirui - book flip (transform-origin)", 640, 360);
    let mut app = App::new(backend);
    app.add_system(flip_system);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(24, 26, 34))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(640),
            height: Dimension::px(360),
            ..Default::default()
        })
        .id();

    let world = &mut app.world;
    mirui_macros::ui! {
        :(
            parent: root
            world: world
        :)

        spread (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 640,
            height: 360
        ) {
            left_page (
                position: Position::Absolute,
                left: 140,
                top: 60,
                width: 180,
                height: 240,
                bg_color: Color::rgb(220, 210, 180),
                border_radius: 4,
                border_color: Color::rgb(255, 255, 255),
                border_width: 3
            ) {}
            right_page (
                position: Position::Absolute,
                left: 320,
                top: 60,
                width: 180,
                height: 240,
                bg_color: Color::rgb(200, 230, 200),
                border_radius: 4,
                border_color: Color::rgb(255, 255, 255),
                border_width: 3
            ) [
                TransformOrigin {
                    x: Fixed::ZERO,
                    y: Fixed::ONE / 2,
                },
                Page {
                    angle_deg: Fixed::ZERO,
                    speed_deg: Fixed::ONE / 2,
                },
            ] {}
        }
    };

    app.set_root(root);
    app.run();
}
