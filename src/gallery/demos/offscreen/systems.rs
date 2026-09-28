use super::state::{ForceDirty, FpsReadout, ModeToggle, PanelTarget};
use crate::ecs::{FrameTimings, World};
use crate::ui::OffscreenRender;
use crate::ui::widgets::Text;

const TOGGLE_NS: u64 = 5_000_000_000;
pub(super) const UPDATE_EVERY: u32 = 30;

#[mirui_macros::system(order = ANIMATION)]
pub fn mode_toggle_system(world: &mut World) {
    let frame_ns = world
        .resource::<FrameTimings>()
        .map(|t| t.frame_nanos)
        .unwrap_or(0);

    let flip = if let Some(t) = world.resource_mut::<ModeToggle>() {
        t.elapsed_ns += frame_ns;
        if t.elapsed_ns - t.last_flip_ns >= TOGGLE_NS {
            t.last_flip_ns = t.elapsed_ns;
            t.offscreen = !t.offscreen;
            Some(t.offscreen)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(now_offscreen) = flip {
        world.for_each_stable::<PanelTarget>(|world, e| {
            if now_offscreen {
                world.insert(e, OffscreenRender::default());
            } else {
                world.remove::<OffscreenRender>(e);
            }
        });
    }
}

#[mirui_macros::system(order = ANIMATION)]
pub fn fps_readout_system(world: &mut World) {
    let render_ns = world
        .resource::<FrameTimings>()
        .map(|t| t.render_nanos)
        .unwrap_or(0);
    let offscreen = world
        .resource::<ModeToggle>()
        .map(|t| t.offscreen)
        .unwrap_or(false);

    world.for_each_stable::<FpsReadout>(|world, e| {
        let snapshot = if let Some(r) = world.get_mut::<FpsReadout>(e) {
            r.accum_render_ns += render_ns;
            r.counter += 1;
            if r.counter >= UPDATE_EVERY {
                let avg_ns = r.accum_render_ns / r.counter as u64;
                r.counter = 0;
                r.accum_render_ns = 0;
                Some(avg_ns)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(avg_ns) = snapshot {
            let avg_us = avg_ns / 1000;
            let mode = if offscreen { "offscreen" } else { "inline   " };
            let label = alloc::format!("MODE={mode}  render avg {avg_us}us");
            if let Some(text) = world.get_mut::<Text>(e) {
                text.set_content(label);
                world.invalidate_visual(e);
            }
        }
    });
}

#[mirui_macros::system(order = ANIMATION)]
pub fn force_dirty_system(world: &mut World) {
    world.for_each_stable::<ForceDirty>(|world, e| {
        world.invalidate(e);
    });
}
