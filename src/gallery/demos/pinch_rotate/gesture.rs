use super::state::{PinchStatus, PinchTarget};
use crate::prelude::*;
use crate::types::{Fixed64, Transform};
use crate::ui::icons::ICON_PLUS;
use crate::ui::theme::ThemedColor;
use crate::ui::widgets::icon::Icon;

const BASE_W: i32 = 160;
const BASE_H: i32 = 120;

fn refresh(world: &mut World, entity: Entity) {
    let snapshot = world.get::<PinchTarget>(entity).map(|t| {
        (
            t.status.clone(),
            t.mode,
            t.visual_scale,
            t.visual_rotation,
            t.pinch_events,
            t.rotate_events,
        )
    });
    let Some((status, mode, visual_scale, visual_rotation, pinch_events, rotate_events)) = snapshot
    else {
        return;
    };

    let visual_rot_deg = visual_rotation * Fixed::from_int(180) / Fixed::PI;
    let xform = Transform::scale(visual_scale, visual_scale)
        .compose(&Transform::rotate_deg(visual_rot_deg));
    crate::ui::widgets::set_transform(world, entity, xform);

    let visual_scale_pct = (visual_scale * Fixed::from_int(100)).to_int();
    let visual_rot_int = visual_rot_deg.to_int();
    status.set(PinchStatus {
        mode,
        scale_pct: visual_scale_pct,
        rotation_deg: visual_rot_int,
        pinch_events,
        rotate_events,
    });
}

#[compose]
pub(super) fn pinch_target(status: Signal<PinchStatus>) -> Entity {
    //~focus-start
    ui! {
        View (
            id: "pinch_target",
            width: BASE_W,
            height: BASE_H,
            bg_color: ColorToken::Primary,
            border_color: ColorToken::OnPrimary,
            border_width: 2,
            border_radius: 28
        ) [
            PinchTarget {
                status,
                last_pinch: Fixed64::ONE,
                last_rotate: Fixed::ZERO,
                visual_scale: Fixed::ONE,
                visual_scale64: Fixed64::ONE,
                visual_rotation: Fixed::ZERO,
                pinch_events: 0,
                rotate_events: 0,
                mode: "IDLE",
            },
        ] on Pinch {
            if let Some(t) = ctx.world.get_mut::<PinchTarget>(ctx.entity) {
                t.last_pinch = *scale_delta;
                let lo = Fixed64::from_ratio(65, 100);
                let hi = Fixed64::from_ratio(8, 5);
                t.visual_scale64 = (t.visual_scale64 * *scale_delta).clamp(lo, hi);
                t.visual_scale = t.visual_scale64.to_fixed();
                t.pinch_events += 1;
                t.mode = if *scale_delta > Fixed64::ONE {
                    "EXPAND"
                } else if *scale_delta < Fixed64::ONE {
                    "SHRINK"
                } else {
                    "PINCH"
                };
            }
            refresh(ctx.world, ctx.entity);
        } on Rotate {
            if let Some(t) = ctx.world.get_mut::<PinchTarget>(ctx.entity) {
                t.last_rotate = *angle;
                t.visual_rotation += *angle;
                t.rotate_events += 1;
                t.mode = "ROTATE";
            }
            refresh(ctx.world, ctx.entity);
        }
        {
            Icon (
                path: ICON_PLUS,
                color: ThemedColor::Token(ColorToken::OnPrimary),
                size: Dimension::Px(Fixed::from_int(46)),
                grow: 1.0,
                width: Dimension::percent(100)
            )
        }
    }
    //~focus-end
}
