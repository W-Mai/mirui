// Pinch / Rotate verification — drives the recogniser via `SimTimeline`
// since SDL2 macOS does not deliver trackpad gestures (issue #6137).
// Stage 1 (this revision): pure text readout. Rect stays put; only the
// status line updates. If text is wrong, the bug is in the handler /
// sim path, not in transform / layout.

use mirui::anim::ease;
use mirui::app::App;
use mirui::components::text::Text;
use mirui::components::transform::WidgetTransform;
use mirui::ecs::{Entity, World};
use mirui::event::GestureHandler;
use mirui::event::gesture::GestureEvent;
use mirui::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use mirui::layout::*;
use mirui::plugins::StdInstantClockPlugin;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed, Fixed64, Point, Transform};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui_macros::ui;

extern crate alloc;
use alloc::format;

const W: i32 = 480;
const H: i32 = 360;

// Rect base size + center (matches sim gesture center below).
const BASE_W: i32 = 160;
const BASE_H: i32 = 120;
const CENTER_X: i32 = 240;
const CENTER_Y: i32 = 220;

struct PinchTarget {
    last_pinch: Fixed64,
    last_rotate: Fixed,
    visual_scale: Fixed,
    visual_scale64: Fixed64,
    visual_rotation: Fixed,
    pinch_events: u32,
    rotate_events: u32,
    mode: &'static str,
    status: Entity,
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
            GestureEvent::Pinch { scale, .. } => {
                t.last_pinch = *scale;
                t.visual_scale64 = t.visual_scale64 * *scale;
                t.visual_scale = t
                    .visual_scale64
                    .clamp(
                        Fixed64::from_fixed(Fixed::ONE / Fixed::from_int(2)),
                        Fixed64::from_fixed(Fixed::from_int(2)),
                    )
                    .to_fixed();
                t.pinch_events += 1;
                t.mode = if *scale > Fixed64::ONE {
                    "EXPAND"
                } else if *scale < Fixed64::ONE {
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

fn main() {
    let backend = SdlSurface::new("mirui — pinch / rotate demo", W as u16, H as u16);
    let mut app = App::new(backend)
        .with_default_widgets()
        .with_default_systems();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            width: Dimension::px(W),
            height: Dimension::px(H),
            ..Default::default()
        })
        .id();

    let status = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        status (
            position: Position::Absolute,
            left: 16,
            top: 16,
            width: W - 32,
            height: 28,
            text: "scale 100%   rotation 0",
            text_color: Color::rgb(140, 220, 255)
        ) {}
    };

    let rect = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        rect (
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
    let _ = rect;

    let center = Point {
        x: Fixed::from_int(CENTER_X),
        y: Fixed::from_int(CENTER_Y),
    };
    // Keep both virtual fingers inside the rect on PointerDown. Hit-test
    // target is captured from the first finger's Down; if a wide pinch
    // starts outside the target, that whole round is intentionally ignored.
    let small = Fixed::from_int(40);
    let large = Fixed::from_int(80);
    let radius = Fixed::from_int(50);
    let timeline = SimTimeline::new(alloc::vec![
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

    app.set_root(root);
    app.add_system(sim_timeline_system::system());
    app.add_plugin(StdInstantClockPlugin::default());
    app.run();
}
