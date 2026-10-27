use super::state::{FpsReadout, ModeToggle, PanelTarget};
use crate::ecs::{Entity, FrameTimings};
use crate::ui::dirty::Dirty;
use crate::ui::property::prop;
use crate::ui::{OffscreenRender, WidgetRoot};

const TOGGLE_NS: u64 = 5_000_000_000;
pub(super) const UPDATE_EVERY: u32 = 30;

#[mirui_macros::system(order = Animation)]
pub fn mode_toggle_system(
    timings: res!(optional FrameTimings),
    toggle: res!(mut ModeToggle),
    panels: query!(Entity; with PanelTarget;),
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
        for entity in panels {
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

#[mirui_macros::system(order = Animation)]
pub fn force_dirty_system(root: res!(WidgetRoot), commands: commands!()) {
    // Redraw the window without invalidating the cached panel subtree.
    commands.insert(root.0, Dirty);
}
