use crate::ecs::World;
use crate::event::input::{InputEvent, KEY_ROTARY_PRESS};
use crate::feedback::{InputFeedback, InputFeedbackInput};

/// Record a raw input event into [`InputFeedbackInput`] so the rotary
/// feedback system can consume it on its next run. Called from the
/// plugin's `on_event`; framework `event::dispatch_input` does not
/// reach into this module.
pub(crate) fn record_input(world: &mut World, event: &InputEvent) {
    if world.resource::<InputFeedback>().is_none() {
        return;
    }
    let mut input = world
        .resource::<InputFeedbackInput>()
        .copied()
        .unwrap_or_default();
    match event {
        InputEvent::Rotary { delta, .. } => {
            input.rotary_delta = input.rotary_delta.saturating_add(*delta);
            input.event_seq = input.event_seq.wrapping_add(1);
        }
        InputEvent::Wheel { dy, .. } => {
            input.wheel_delta_y += *dy;
            input.event_seq = input.event_seq.wrapping_add(1);
        }
        InputEvent::Key { code, pressed } if *code == KEY_ROTARY_PRESS && *pressed => {
            input.click_pulse = true;
            input.event_seq = input.event_seq.wrapping_add(1);
        }
        _ => return,
    }
    world.insert_resource(input);
}
