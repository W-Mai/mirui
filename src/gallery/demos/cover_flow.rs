extern crate alloc;

use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::assets::IMG_THUMBS_UP;
use crate::components::{Image, WidgetTransform3D};
use crate::ecs::{Entity, World};
use crate::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
#[cfg(feature = "std")]
use crate::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::Transform3D;
use crate::widget;
use crate::widget::dirty::Dirty;

const WINDOW_W: i32 = 640;
const WINDOW_H: i32 = 360;
const CARD_W: i32 = 140;
const CARD_H: i32 = 180;
const CARD_GAP: i32 = 80;
const PERSPECTIVE: i32 = 250;
const CARD_COUNT: i32 = 5;

pub const DEFAULT_VIEW: (u16, u16) = (WINDOW_W as u16, WINDOW_H as u16);

pub struct CarouselCard {
    pub index: usize,
}

pub struct Carousel;

#[mirui_macros::system]
pub fn layout_system(world: &mut World) {
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
        widget::set_position(world, e, tx, ty);

        let relative = Fixed::from_int(idx) - offset / slot_stride;
        let tilt_y = Fixed::ZERO - relative * Fixed::from_int(45);
        let tilt_x = relative.abs() * Fixed::from_int(20) - Fixed::from_int(15);
        let distance = Fixed::from_int(PERSPECTIVE);
        let ty3d = Transform3D::rotate_y_perspective(tilt_y, distance);
        let tx3d = Transform3D::rotate_x_perspective(tilt_x, distance);
        world.insert(e, WidgetTransform3D(ty3d.compose(&tx3d)));
        world.insert(e, Dirty);
    }
    world.insert(carousel, Dirty);
}

pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
    let card_colors = [
        Color::rgb(255, 107, 107),
        Color::rgb(255, 206, 84),
        Color::rgb(136, 216, 176),
        Color::rgb(118, 209, 244),
        Color::rgb(178, 148, 255),
    ];

    let root = WidgetBuilder::new(world)
        .bg_color(Color::rgb(24, 26, 34))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(WINDOW_W),
            height: Dimension::px(WINDOW_H),
            ..Default::default()
        })
        .id();

    let card_colors_ref = &card_colors;
    ui! {
        :(
            parent: root
            world: world
        :)

        View (
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
                View (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: CARD_W,
                    height: CARD_H,
                    bg_color: *item.1,
                    border_radius: 8,
                    border_color: Color::rgb(0, 0, 0),
                    border_width: 5
                ) [
                    CarouselCard { index: item.0 },
                ] {
                    if item.0 % 2 == 1 {
                        View (
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

    attach_to_parent(world, parent, root);
    root
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_system(layout_system::system())
        .add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    build_widgets(&mut app.world, parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Children;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let root = build_widgets(&mut world, parent);
        assert_ne!(root, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| c.0.contains(&root)),
        );
    }
}
