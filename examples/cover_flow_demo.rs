use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::components::assets::IMG_THUMBS_UP;
use mirui::components::image::Image;
use mirui::components::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use mirui::components::transform_3d::WidgetTransform3D;
use mirui::ecs::World;
use mirui::layout::*;
use mirui::types::{Color, Dimension, Fixed, Transform3D};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;

extern crate alloc;

const WINDOW_W: i32 = 640;
const WINDOW_H: i32 = 360;
const CARD_W: i32 = 140;
const CARD_H: i32 = 180;
const CARD_GAP: i32 = 80;
const PERSPECTIVE: i32 = 250;
const CARD_COUNT: i32 = 5;

struct CarouselCard {
    index: usize,
}

struct Carousel;

fn layout_system(world: &mut World) {
    let mut carousels = alloc::vec::Vec::new();
    world.query::<Carousel>().collect_into(&mut carousels);
    let offset = match carousels
        .first()
        .and_then(|&e| world.get::<ScrollOffset>(e))
    {
        Some(s) => s.x,
        None => return,
    };

    let carousel = carousels[0];

    let mut cards = alloc::vec::Vec::new();
    world.query::<CarouselCard>().collect_into(&mut cards);
    if cards.is_empty() {
        return;
    }
    let slot_stride = Fixed::from_int(CARD_W + CARD_GAP);
    let container_center = Fixed::from_int(WINDOW_W / 2);

    for e in cards {
        let idx = match world.get::<CarouselCard>(e) {
            Some(c) => c.index as i32,
            None => continue,
        };
        let tx =
            container_center + Fixed::from_int(idx) * slot_stride - Fixed::from_int(CARD_W / 2);
        let ty = Fixed::from_int((WINDOW_H - CARD_H) / 2);
        mirui::widget::set_position(world, e, tx, ty);

        let relative = Fixed::from_int(idx) - offset / slot_stride;
        let tilt_y = Fixed::ZERO - relative * Fixed::from_int(45);
        let tilt_x = relative.abs() * Fixed::from_int(20) - Fixed::from_int(15);
        let distance = Fixed::from_int(PERSPECTIVE);
        let ty = Transform3D::rotate_y_perspective(tilt_y, distance);
        let tx3d = Transform3D::rotate_x_perspective(tilt_x, distance);
        world.insert(e, WidgetTransform3D(ty.compose(&tx3d)));
        world.insert(e, Dirty);
    }
    world.insert(carousel, Dirty);
}

fn main() {
    let backend = SdlBackend::new(
        "mirui - cover flow (nested 2.5D + drag)",
        WINDOW_W as u16,
        WINDOW_H as u16,
    );
    let mut app = App::new(backend);

    app.add_system(layout_system);

    let card_colors = [
        Color::rgb(255, 107, 107),
        Color::rgb(255, 206, 84),
        Color::rgb(136, 216, 176),
        Color::rgb(118, 209, 244),
        Color::rgb(178, 148, 255),
    ];

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(24, 26, 34))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(WINDOW_W),
            height: Dimension::px(WINDOW_H),
            ..Default::default()
        })
        .id();

    let world = &mut app.world;
    let card_colors_ref = &card_colors;
    mirui_macros::ui! {
        :(
            parent: root
            world: world
        :)

        carousel (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: WINDOW_W,
            height: WINDOW_H
        ) [
            Carousel,
            ScrollOffset {
                x: Fixed::from_int(440),
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Horizontal,
                elastic: true,
                content_width: Fixed::from_int(
                WINDOW_W + (CARD_W + CARD_GAP) * (CARD_COUNT - 1),
            ),
                content_height: Fixed::ZERO,
            },
        ] {
            walk card_colors_ref.iter().enumerate() with item {
                card (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: CARD_W,
                    height: CARD_H,
                    bg_color: *item.1,
                    border_radius: 16
                ) [
                    CarouselCard { index: item.0 },
                ] {
                    if item.0 % 2 == 1 {
                        thumb (
                            position: Position::Absolute,
                            left: (CARD_W - 64) / 2,
                            top: (CARD_H - 64) / 2,
                            width: 64,
                            height: 64,
                            image: Image::new(&IMG_THUMBS_UP)
                        ) {}
                    }
                }
            }
        }
    };

    app.set_root(root);
    app.run();
}
