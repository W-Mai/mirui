use super::state::{CARD_COUNT, Carousel, CarouselCard, CoverFlowBounds, LayoutCache};
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::types::{Dimension, Transform3D};
use crate::ui;
use crate::ui::root_viewport;
use crate::ui::widgets::WidgetTransform3D;

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

    let Some(carousel) = world
        .query::<Carousel>()
        .iter()
        .next()
        .map(|(entity, _)| entity)
    else {
        return;
    };
    let offset = match world.get::<ScrollOffset>(carousel) {
        Some(s) => s.x,
        None => return,
    };

    let frame = LayoutCache {
        view_w,
        view_h,
        offset,
    };
    if world.resource::<LayoutCache>() == Some(&frame) {
        return;
    }
    world.insert_resource(frame);

    if let Some(style) = world.get_mut::<Style>(carousel) {
        style.layout.width = Dimension::Px(Fixed::from_int(view_w));
        style.layout.height = Dimension::Px(Fixed::from_int(view_h));
    }
    if let Some(cfg) = world.get_mut::<ScrollConfig>(carousel) {
        cfg.content_width = Fixed::from_int(view_w + (card_w + card_gap) * (CARD_COUNT - 1));
    }

    if world
        .storage::<CarouselCard>()
        .is_none_or(|storage| storage.entities().is_empty())
    {
        return;
    }
    let slot_stride = Fixed::from_int(card_w + card_gap);
    let container_center = Fixed::from_int(view_w / 2);
    let card_top = Fixed::from_int((view_h - card_h) / 2);

    world.for_each_stable::<CarouselCard>(|world, e| {
        let idx = match world.get::<CarouselCard>(e) {
            Some(c) => c.index as i32,
            None => return,
        };
        let tx =
            container_center + Fixed::from_int(idx) * slot_stride - Fixed::from_int(card_w / 2);
        ui::set_position(world, e, tx, card_top);
        if let Some(style) = world.get_mut::<Style>(e) {
            style.layout.width = Dimension::px(card_w);
            style.layout.height = Dimension::px(card_h);
        }

        let relative = Fixed::from_int(idx) - offset / slot_stride;
        let tilt_y = Fixed::ZERO - relative * Fixed::from_int(45);
        let tilt_x = relative.abs() * Fixed::from_int(20) - Fixed::from_int(15);
        let distance = Fixed::from_int(perspective);
        let ty3d = Transform3D::rotate_y_perspective(tilt_y, distance);
        let tx3d = Transform3D::rotate_x_perspective(tilt_x, distance);
        world.insert(e, WidgetTransform3D(ty3d.compose(&tx3d)));
        world.invalidate(e);
    });
    world.invalidate(carousel);
}
//~focus-end
