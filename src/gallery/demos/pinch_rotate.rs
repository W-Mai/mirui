extern crate alloc;

#[cfg(feature = "std")]
use crate::anim::ease;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::{Text, WidgetTransform};
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
#[cfg(feature = "std")]
use crate::event::sim::{SimAction, SimTimeline, sim_timeline_system};
#[cfg(feature = "std")]
use crate::plugins::StdInstantClockPlugin;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::{Fixed64, Transform};
use crate::widget::Style;
use crate::widget::dirty::Dirty;
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
    pub status: Entity,
}

fn handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let (
        mode,
        last_pinch,
        last_rotate,
        visual_scale,
        visual_rotation,
        pinch_events,
        rotate_events,
        status,
    ) = {
        let Some(t) = world.get_mut::<PinchTarget>(entity) else {
            return false;
        };
        match event {
            GestureEvent::Pinch { scale_delta, .. } => {
                t.last_pinch = *scale_delta;
                let lo = Fixed64::from_fixed(Fixed::ONE / Fixed::from_int(2));
                let hi = Fixed64::from_fixed(Fixed::from_int(2));
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
            GestureEvent::Rotate { angle, .. } => {
                t.last_rotate = *angle;
                t.visual_rotation += *angle;
                t.rotate_events += 1;
                t.mode = "ROTATE";
            }
            _ => return false,
        }
        (
            t.mode,
            t.last_pinch,
            t.last_rotate,
            t.visual_scale,
            t.visual_rotation,
            t.pinch_events,
            t.rotate_events,
            t.status,
        )
    };

    let rot_deg = last_rotate * Fixed::from_int(180) / Fixed::PI;
    let visual_rot_deg = visual_rotation * Fixed::from_int(180) / Fixed::PI;
    let xform = Transform::scale(visual_scale, visual_scale)
        .compose(&Transform::rotate_deg(visual_rot_deg));
    world.insert(entity, WidgetTransform(xform));
    world.insert(entity, Dirty);

    let scale_pct = (last_pinch * Fixed64::from_int(100)).to_int();
    let visual_scale_pct = (visual_scale * Fixed::from_int(100)).to_int();
    let rot_int = rot_deg.to_int();
    let visual_rot_int = visual_rot_deg.to_int();
    let line = format!(
        "{mode}   delta {scale_pct}%/{} raw   visual {visual_scale_pct}% {visual_rot_int}deg   rotate_delta {rot_int}   counts {pinch_events}/{rotate_events}",
        last_pinch.raw(),
    );
    world.insert(status, Text(line.into_bytes()));
    world.insert(status, Dirty);
    true
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    if let Some(style) = world.get_mut::<Style>(parent) {
        style.bg_color = Some(Color::rgb(30, 30, 46).into());
        style.layout = LayoutStyle {
            width: Dimension::px(W),
            height: Dimension::px(H),
            grow: Fixed::ONE,
            ..Default::default()
        };
    }

    let status = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            position: Position::Absolute,
            left: 16,
            top: 16,
            width: W - 32,
            height: 28,
            text: "scale 100%   rotation 0",
            text_color: Color::rgb(140, 220, 255)
        ) {}
    };

    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            position: Position::Absolute,
            left: CENTER_X - BASE_W / 2,
            top: CENTER_Y - BASE_H / 2,
            width: BASE_W,
            height: BASE_H,
            bg_color: Color::rgb(88, 166, 255)
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
                status,
            },
            GestureHandler {
                on_gesture: handler,
            },
        ] {}
    };
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    build_widgets(&mut app.world, parent);

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
    use crate::widget::Children;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
