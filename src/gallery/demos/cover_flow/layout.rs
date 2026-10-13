use super::state::{
    CARD_COUNT, Carousel, CarouselCard, CoverFlowBounds, CoverFlowLayoutItem, LayoutCache,
};
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::types::{Dimension, Transform3D};
use crate::ui::dirty::Dirty;
use crate::ui::widgets::WidgetTransform3D;
use crate::ui::{ComputedRect, WidgetRoot};

//~focus-start
#[mirui_macros::system]
pub fn layout_system(
    fallback: res!(optional CoverFlowBounds),
    cache: res!(optional LayoutCache),
    root: res!(optional WidgetRoot),
    rects: query!(Entity, ComputedRect),
    offsets: query!(Entity, ScrollOffset; with Carousel;),
    styles: query!(Entity, mut Style, optional CarouselCard; with CoverFlowLayoutItem;),
    configs: query!(mut ScrollConfig; with Carousel;),
    commands: commands!(),
) {
    let viewport = root.and_then(|root| {
        rects
            .into_iter()
            .find_map(|(entity, rect)| (entity == root.0).then_some(rect.0))
    });
    let bounds = viewport
        .map(|rect| CoverFlowBounds::for_px(rect.w.to_int(), rect.h.to_int()))
        .or_else(|| fallback.map(|b| CoverFlowBounds::for_px(b.view_w, b.view_h)));
    let Some(bounds) = bounds else { return };
    let CoverFlowBounds {
        view_w,
        view_h,
        card_w,
        card_h,
        card_gap,
        perspective,
    } = bounds;

    let Some((carousel, offset)) = offsets.into_iter().next() else {
        return;
    };
    let offset = offset.x;

    let frame = LayoutCache {
        view_w,
        view_h,
        offset,
    };
    if cache.as_deref() == Some(&frame) {
        return;
    }
    commands.insert_resource(frame);

    if let Some(scroll_config) = configs.into_iter().next() {
        scroll_config.content_width =
            Fixed::from_int(view_w + (card_w + card_gap) * (CARD_COUNT - 1));
    }

    let slot_stride = Fixed::from_int(card_w + card_gap);
    let container_center = Fixed::from_int(view_w / 2);
    let card_top = Fixed::from_int((view_h - card_h) / 2);

    for (entity, style, card) in styles {
        if entity == carousel {
            style.layout.width = Dimension::Px(Fixed::from_int(view_w));
            style.layout.height = Dimension::Px(Fixed::from_int(view_h));
            continue;
        }
        let Some(card) = card else { continue };
        let idx = card.index as i32;
        let tx =
            container_center + Fixed::from_int(idx) * slot_stride - Fixed::from_int(card_w / 2);
        style.layout.left = Dimension::Px(tx);
        style.layout.top = Dimension::Px(card_top);
        style.layout.width = Dimension::px(card_w);
        style.layout.height = Dimension::px(card_h);

        let relative = Fixed::from_int(idx) - offset / slot_stride;
        let tilt_y = Fixed::ZERO - relative * Fixed::from_int(45);
        let tilt_x = relative.abs() * Fixed::from_int(20) - Fixed::from_int(15);
        let distance = Fixed::from_int(perspective);
        let ty3d = Transform3D::rotate_y_perspective(tilt_y, distance);
        let tx3d = Transform3D::rotate_x_perspective(tilt_x, distance);
        commands.insert(entity, WidgetTransform3D(ty3d.compose(&tx3d)));
        commands.insert(entity, Dirty);
    }
    commands.insert(carousel, Dirty);
}
//~focus-end
