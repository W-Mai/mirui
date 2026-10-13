use super::DEFAULT_VIEW;
use super::state::FlipCard;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::dirty::Dirty;
use crate::ui::widgets::WidgetTransform3D;
use crate::ui::{ComputedRect, Style, Theme, WidgetRoot};

//~focus-start
#[mirui_macros::system(order = Animation)]
pub fn flip_system(
    delta: res!(DeltaTimeMs),
    theme: res!(optional Theme),
    root: res!(optional WidgetRoot),
    rects: query!(Entity, ComputedRect),
    cards: query!(Entity, mut FlipCard, mut Style),
    commands: commands!(),
) {
    // 5/12 and 9/16 reproduce the original 200×180 card in a 480×320 window.
    let viewport = root.and_then(|root| {
        rects
            .into_iter()
            .find_map(|(entity, rect)| (entity == root.0).then_some(rect.0))
    });
    let (vw, vh) = viewport.map_or((DEFAULT_VIEW.0 as i32, DEFAULT_VIEW.1 as i32), |r| {
        (r.w.to_int(), r.h.to_int())
    });
    let card_w = vw * 5 / 12;
    let card_h = vh * 9 / 16;
    let card_left = (vw - card_w) / 2;
    let card_top = (vh - card_h) / 2;
    let dt = delta.0.min(50);

    for (entity, card, style) in cards {
        card.angle_deg += card.speed_deg_per_second * Fixed::from_ratio(i32::from(dt), 1_000);
        if card.angle_deg >= Fixed::from_int(360) {
            card.angle_deg -= Fixed::from_int(360);
        }

        let halfway = Fixed::from_int(90);
        let three_quarters = Fixed::from_int(270);
        let color_token = if card.angle_deg < halfway || card.angle_deg >= three_quarters {
            card.front_color
        } else {
            card.back_color
        };
        let color = theme.as_deref().map_or_else(
            || Theme::default().resolve(color_token),
            |theme| theme.resolve(color_token),
        );
        style.set_bg_color(color);
        style.layout.left = Dimension::px(card_left);
        style.layout.top = Dimension::px(card_top);
        style.layout.width = Dimension::px(card_w);
        style.layout.height = Dimension::px(card_h);

        commands.insert(
            entity,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                card.angle_deg,
                Fixed::from_int(400),
            )),
        );
        commands.insert(card.root, Dirty);
    }
}
//~focus-end
