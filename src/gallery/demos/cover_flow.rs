extern crate alloc;

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
use crate::types::{Dimension, Transform3D};
use crate::widget;
use crate::widget::dirty::Dirty;
use crate::widget::root_viewport;

pub const DEFAULT_VIEW: (u16, u16) = (640, 360);
const CARD_COUNT: i32 = 5;

pub struct CarouselCard {
    pub index: usize,
}

pub struct Carousel;

pub struct CoverFlowBounds {
    pub view_w: i32,
    pub view_h: i32,
    pub card_w: i32,
    pub card_h: i32,
    pub card_gap: i32,
    pub perspective: i32,
}

impl CoverFlowBounds {
    fn for_view(view_w: u16, view_h: u16) -> Self {
        Self::for_px(view_w as i32, view_h as i32)
    }

    fn for_px(vw: i32, vh: i32) -> Self {
        Self {
            view_w: vw,
            view_h: vh,
            card_w: vw * 7 / 32,
            card_h: vh / 2,
            card_gap: vw / 8,
            perspective: vw * 25 / 64,
        }
    }

    fn content_width(&self) -> i32 {
        self.view_w + (self.card_w + self.card_gap) * (CARD_COUNT - 1)
    }
}

//~focus-start
#[mirui_macros::system]
pub fn layout_system(world: &mut World) {
    let bounds = match root_viewport(world) {
        Some(rect) => CoverFlowBounds::for_px(rect.w.to_int(), rect.h.to_int()),
        None => match world.resource::<CoverFlowBounds>() {
            Some(b) => CoverFlowBounds::for_px(b.view_w, b.view_h),
            None => return,
        },
    };
    let CoverFlowBounds {
        view_w,
        view_h,
        card_w,
        card_h,
        card_gap,
        perspective,
    } = bounds;

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

    if let Some(style) = world.get_mut::<Style>(carousel) {
        style.layout.width = Dimension::Px(Fixed::from_int(view_w));
        style.layout.height = Dimension::Px(Fixed::from_int(view_h));
    }
    if let Some(cfg) = world.get_mut::<ScrollConfig>(carousel) {
        cfg.content_width = Fixed::from_int(view_w + (card_w + card_gap) * (CARD_COUNT - 1));
    }

    let mut cards = alloc::vec::Vec::new();
    world.query::<CarouselCard>().collect_into(&mut cards);
    if cards.is_empty() {
        return;
    }
    let slot_stride = Fixed::from_int(card_w + card_gap);
    let container_center = Fixed::from_int(view_w / 2);
    let card_top = Fixed::from_int((view_h - card_h) / 2);

    for e in cards {
        let idx = match world.get::<CarouselCard>(e) {
            Some(c) => c.index as i32,
            None => continue,
        };
        let tx =
            container_center + Fixed::from_int(idx) * slot_stride - Fixed::from_int(card_w / 2);
        widget::set_position(world, e, tx, card_top);

        let relative = Fixed::from_int(idx) - offset / slot_stride;
        let tilt_y = Fixed::ZERO - relative * Fixed::from_int(45);
        let tilt_x = relative.abs() * Fixed::from_int(20) - Fixed::from_int(15);
        let distance = Fixed::from_int(perspective);
        let ty3d = Transform3D::rotate_y_perspective(tilt_y, distance);
        let tx3d = Transform3D::rotate_x_perspective(tilt_x, distance);
        world.insert(e, WidgetTransform3D(ty3d.compose(&tx3d)));
        world.insert(e, Dirty);
    }
    world.insert(carousel, Dirty);
}
//~focus-end

pub fn build_widgets(world: &mut World, parent: Entity, view_w: u16, view_h: u16) {
    let bounds = CoverFlowBounds::for_view(view_w, view_h);
    let vw = bounds.view_w;
    let vh = bounds.view_h;
    let card_w = bounds.card_w;
    let card_h = bounds.card_h;
    let content_width = bounds.content_width();
    let initial_offset = (content_width - vw) / 2 - card_w / 4;
    world.insert_resource(bounds);

    let card_colors = [
        Color::rgb(255, 107, 107),
        Color::rgb(255, 206, 84),
        Color::rgb(136, 216, 176),
        Color::rgb(118, 209, 244),
        Color::rgb(178, 148, 255),
    ];

    let card_colors_ref = &card_colors;
    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: vw,
            height: vh
        ) [
            Carousel,
            ScrollOffset {
                x: Fixed::from_int(initial_offset),
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: ScrollAxis::Horizontal,
                elastic: true,
                content_width: Fixed::from_int(content_width),
                content_height: Fixed::ZERO,
            },
        ] {
            walk card_colors_ref.iter().enumerate() with item {
                View (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: card_w,
                    height: card_h,
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
                            left: (card_w - 64) / 2,
                            top: (card_h - 64) / 2,
                            width: 64,
                            height: 64,
                            image: Image::new(&IMG_THUMBS_UP)
                        )
                    }
                }
            }
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    let info = app.backend.display_info();
    app.add_system(layout_system::system())
        .add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    build_widgets(&mut app.world, parent, info.width, info.height);
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
        build_widgets(&mut world, parent, 640, 360);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
