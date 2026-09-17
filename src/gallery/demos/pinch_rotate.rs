extern crate alloc;

#[cfg(feature = "std")]
use crate::anim::ease;
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::input::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use crate::prelude::*;
use crate::types::{Fixed64, Transform};
use crate::ui::icons::ICON_PLUS;
use crate::ui::theme::ThemedColor;
use crate::ui::widgets::icon::Icon;
use crate::ui::widgets::{ParagraphStyle, Text};
use alloc::format;
#[cfg(feature = "std")]
use alloc::vec;

const W: i32 = 480;
const H: i32 = 360;

pub const DEFAULT_VIEW: (u16, u16) = (W as u16, H as u16);

const BASE_W: i32 = 160;
const BASE_H: i32 = 120;
const CENTER_X: i32 = 240;
const CENTER_Y: i32 = 220;

pub struct PinchTarget {
    pub last_pinch: Fixed64,
    pub last_rotate: Fixed,
    pub visual_scale: Fixed,
    pub visual_scale64: Fixed64,
    pub visual_rotation: Fixed,
    pub pinch_events: u32,
    pub rotate_events: u32,
    pub mode: &'static str,
}

fn refresh(world: &mut World, entity: Entity) {
    let snapshot = world.get::<PinchTarget>(entity).map(|t| {
        (
            t.mode,
            t.visual_scale,
            t.visual_rotation,
            t.pinch_events,
            t.rotate_events,
        )
    });
    let Some((mode, visual_scale, visual_rotation, pinch_events, rotate_events)) = snapshot else {
        return;
    };

    let visual_rot_deg = visual_rotation * Fixed::from_int(180) / Fixed::PI;
    let xform = Transform::scale(visual_scale, visual_scale)
        .compose(&Transform::rotate_deg(visual_rot_deg));
    crate::ui::widgets::set_transform(world, entity, xform);

    let visual_scale_pct = (visual_scale * Fixed::from_int(100)).to_int();
    let visual_rot_int = visual_rot_deg.to_int();
    let line = format!(
        "{mode}  ·  SCALE {visual_scale_pct}%  ·  ROT {visual_rot_int}°  ·  P{pinch_events} R{rotate_events}",
    );
    if let Some(status) = world.find_by_id("pinch_status") {
        world.insert(status, Text::from(line));
        world.invalidate(status);
    }
}

#[compose]
pub fn build_widgets() {
    ui! {
        View (grow: 1.0, bg_color: ColorToken::Surface) {
            View (
                position: Position::Absolute,
                left: 16,
                top: 16,
                width: W - 32,
                height: 32,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 16
            )
            Text (
                "IDLE  ·  SCALE 100%  ·  ROT 0°  ·  P0 R0",
                position: Position::Absolute,
                left: 28,
                top: 16,
                width: W - 56,
                height: 32,
                font_size: 10,
                text_color: ColorToken::Secondary,
                paragraph: ParagraphStyle::label(),
                id: "pinch_status"
            )
            Text (
                "TWO-POINTER GESTURE · LIVE TRANSFORM",
                position: Position::Absolute,
                left: 16,
                top: 324,
                width: W - 32,
                height: 20,
                font_size: 9,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
        }
    };

    //~focus-start
    ui! {
        View (
            id: "pinch_target",
            position: Position::Absolute,
            left: CENTER_X - BASE_W / 2,
            top: CENTER_Y - BASE_H / 2,
            width: BASE_W,
            height: BASE_H,
            bg_color: ColorToken::Primary,
            border_color: ColorToken::OnPrimary,
            border_width: 2,
            border_radius: 28
        ) [
            PinchTarget {
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
                path: ICON_PLUS.clone(),
                color: ThemedColor::Token(ColorToken::OnPrimary),
                size: Dimension::Px(Fixed::from_int(46)),
                grow: 1.0,
                width: Dimension::percent(100)
            )
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
    app.compose(parent, build_widgets);

    let center = Point {
        x: Fixed::from_int(CENTER_X),
        y: Fixed::from_int(CENTER_Y),
    };
    let small = Fixed::from_int(40);
    let large = Fixed::from_int(80);
    let radius = Fixed::from_int(50);
    let timeline = SimTimeline::new(vec![
        SimAction::pinch(center, small, large, 1500, ease::ease_in_out_cubic),
        SimAction::wait(800),
        SimAction::pinch(center, large, small, 1500, ease::ease_in_out_cubic),
        SimAction::wait(800),
        SimAction::pinch(center, small, large, 1500, ease::ease_in_out_cubic),
        SimAction::wait(800),
        SimAction::rotate_gesture(
            center,
            radius,
            Fixed::ZERO,
            Fixed::PI / Fixed::from_int(2),
            1500,
            ease::ease_in_out_cubic,
        ),
        SimAction::wait(800),
        SimAction::rotate_gesture(
            center,
            radius,
            Fixed::PI / Fixed::from_int(2),
            Fixed::ZERO,
            1500,
            ease::ease_in_out_cubic,
        ),
        SimAction::wait(800),
        SimAction::pinch(center, large, small, 1500, ease::ease_in_out_cubic),
        SimAction::wait(800),
    ])
    .looping(true);
    app.world.insert_resource(timeline);
    app.add_system(sim_timeline_system::system());
    app.add_plugin(StdInstantClockPlugin);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;
    use crate::ui::widgets::WidgetTransform;

    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }

    #[test]
    fn pinch_updates_target_and_status() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        let target = world.find_by_id("pinch_target").expect("target id");
        let status = world.find_by_id("pinch_status").expect("status id");

        assert_eq!(
            world.get::<PinchTarget>(target).map(|t| t.pinch_events),
            Some(0)
        );
        GestureHandler::trigger(
            &mut world,
            target,
            &GestureEvent::Pinch {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                scale_delta: Fixed64::from_int(2),
                target,
            },
        );
        assert_eq!(
            world.get::<PinchTarget>(target).map(|t| t.pinch_events),
            Some(1)
        );
        assert_eq!(
            world.get::<PinchTarget>(target).map(|t| t.mode),
            Some("EXPAND")
        );
        assert!(world.has::<WidgetTransform>(target));
        assert!(world.get::<Text>(status).is_some());
    }
}
