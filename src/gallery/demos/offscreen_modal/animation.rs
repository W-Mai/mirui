use super::state::{FpsReadout, MODAL_W, ModalAnim, ModeToggle, TOGGLE_NS, UPDATE_EVERY};
use crate::ecs::{FrameTimings, World};
use crate::prelude::Fixed;
use crate::types::Transform;
use crate::ui::widgets::{Text, WidgetTransform};
use crate::ui::{ComputedRect, OffscreenRender};

#[mirui_macros::system(order = ANIMATION)]
pub fn modal_slide_system(world: &mut World) {
    world.for_each_stable::<ModalAnim>(|world, e| {
        let next_t = if let Some(a) = world.get_mut::<ModalAnim>(e) {
            a.t += Fixed::ONE / 90;
            if a.t > Fixed::ONE {
                a.t -= Fixed::ONE;
            }
            a.t
        } else {
            return;
        };

        let bounce = if next_t < Fixed::ONE / 2 {
            next_t * Fixed::from_int(2)
        } else {
            (Fixed::ONE - next_t) * Fixed::from_int(2)
        };
        let off_screen_offset = world
            .get::<ComputedRect>(e)
            .map_or(Fixed::from_int(-MODAL_W), |rect| {
                Fixed::ZERO - rect.0.x - rect.0.w
            });
        let tx = off_screen_offset * (Fixed::ONE - bounce);
        world.insert(e, WidgetTransform(Transform::translate(tx, Fixed::ZERO)));
        world.invalidate(e);
    });
}

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
        world.for_each_stable::<ModalAnim>(|world, e| {
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
