use super::state::{FpsReadout, MODAL_W, ModalAnim, ModeToggle, TOGGLE_NS, UPDATE_EVERY};
use crate::ecs::{Entity, FrameTimings};
use crate::prelude::Fixed;
use crate::types::Transform;
use crate::ui::dirty::Dirty;
use crate::ui::property::prop;
use crate::ui::widgets::WidgetTransform;
use crate::ui::{ComputedRect, OffscreenRender};

#[mirui_macros::system(order = Animation)]
pub fn modal_slide_system(
    modals: query!(Entity, mut ModalAnim, optional ComputedRect),
    commands: commands!(),
) {
    for (entity, animation, rect) in modals {
        animation.t += Fixed::ONE / 90;
        if animation.t > Fixed::ONE {
            animation.t -= Fixed::ONE;
        }
        let next_t = animation.t;

        let bounce = if next_t < Fixed::ONE / 2 {
            next_t * Fixed::from_int(2)
        } else {
            (Fixed::ONE - next_t) * Fixed::from_int(2)
        };
        let off_screen_offset = rect.map_or(Fixed::from_int(-MODAL_W), |rect| {
            Fixed::ZERO - rect.0.x - rect.0.w
        });
        let tx = off_screen_offset * (Fixed::ONE - bounce);
        commands.insert(
            entity,
            WidgetTransform(Transform::translate(tx, Fixed::ZERO)),
        );
        commands.insert(entity, Dirty);
    }
}

#[mirui_macros::system(order = Animation)]
pub fn mode_toggle_system(
    timings: res!(optional FrameTimings),
    toggle: res!(mut ModeToggle),
    modals: query!(Entity; with ModalAnim;),
    commands: commands!(),
) {
    let frame_ns = timings.map_or(0, |timings| timings.phase_wall_nanos);

    toggle.elapsed_ns += frame_ns;
    let flip = if toggle.elapsed_ns - toggle.last_flip_ns >= TOGGLE_NS {
        toggle.last_flip_ns = toggle.elapsed_ns;
        toggle.offscreen = !toggle.offscreen;
        Some(toggle.offscreen)
    } else {
        None
    };

    if let Some(now_offscreen) = flip {
        for entity in modals {
            let entity: Entity = entity;
            if now_offscreen {
                commands.insert(entity, OffscreenRender::default());
            } else {
                commands.remove::<OffscreenRender>(entity);
            }
        }
    }
}

#[mirui_macros::system(order = Animation)]
pub fn fps_readout_system(
    timings: res!(optional FrameTimings),
    toggle: res!(optional ModeToggle),
    readouts: query!(Entity, mut FpsReadout),
    commands: commands!(),
) {
    let render_ns = timings.map_or(0, |timings| timings.render_nanos);
    let offscreen = toggle.is_some_and(|toggle| toggle.offscreen);

    for (entity, readout) in readouts {
        readout.accum_render_ns += render_ns;
        readout.counter += 1;
        let snapshot = if readout.counter >= UPDATE_EVERY {
            let avg_ns = readout.accum_render_ns / readout.counter as u64;
            readout.counter = 0;
            readout.accum_render_ns = 0;
            Some(avg_ns)
        } else {
            None
        };
        if let Some(avg_ns) = snapshot {
            let avg_us = avg_ns / 1000;
            let mode = if offscreen { "offscreen" } else { "inline   " };
            let label = alloc::format!("MODE={mode}  render avg {avg_us}us");
            commands.set::<prop::TextContent>(entity, label);
        }
    }
}
