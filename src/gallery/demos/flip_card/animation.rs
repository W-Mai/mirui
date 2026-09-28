use super::DEFAULT_VIEW;
use super::state::FlipCard;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::root_viewport;
use crate::ui::widgets::WidgetTransform3D;
use crate::ui::{Style, Theme};

//~focus-start
#[mirui_macros::system(order = ANIMATION)]
pub fn flip_system(world: &mut World) {
    // 5/12 and 9/16 reproduce the original 200×180 card in a 480×320 window.
    let (vw, vh) = root_viewport(world)
        .map_or((DEFAULT_VIEW.0 as i32, DEFAULT_VIEW.1 as i32), |r| {
            (r.w.to_int(), r.h.to_int())
        });
    let card_w = vw * 5 / 12;
    let card_h = vh * 9 / 16;
    let card_left = (vw - card_w) / 2;
    let card_top = (vh - card_h) / 2;
    let dt = world
        .resource::<DeltaTimeMs>()
        .map_or(16, |delta| delta.0)
        .min(50);

    world.for_each_stable::<FlipCard>(|world, e| {
        let (angle, front, back, root) = if let Some(c) = world.get_mut::<FlipCard>(e) {
            c.angle_deg += c.speed_deg_per_second * Fixed::from_ratio(i32::from(dt), 1_000);
            if c.angle_deg >= Fixed::from_int(360) {
                c.angle_deg -= Fixed::from_int(360);
            }
            (c.angle_deg, c.front_color, c.back_color, c.root)
        } else {
            return;
        };

        let halfway = Fixed::from_int(90);
        let three_quarters = Fixed::from_int(270);
        let color_token = if angle < halfway || angle >= three_quarters {
            front
        } else {
            back
        };
        let color = world.resource::<Theme>().map_or_else(
            || Theme::default().resolve(color_token),
            |theme| theme.resolve(color_token),
        );
        if let Some(style) = world.get_mut::<Style>(e) {
            style.set_bg_color(color);
            style.layout.left = Dimension::px(card_left);
            style.layout.top = Dimension::px(card_top);
            style.layout.width = Dimension::px(card_w);
            style.layout.height = Dimension::px(card_h);
        }

        world.insert(
            e,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                angle,
                Fixed::from_int(400),
            )),
        );
        world.invalidate(e);
        world.invalidate(root);
    });
}
//~focus-end
