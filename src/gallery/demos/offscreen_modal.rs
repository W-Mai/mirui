extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, FrameTimings, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::types::Transform;
#[cfg(feature = "std")]
use crate::ui::Theme;
use crate::ui::dirty::Dirty;
use crate::ui::theme::ColorToken;
use crate::ui::widgets::Text;
use crate::ui::widgets::WidgetTransform;
use crate::ui::{Children, OffscreenRender};

const WIN_W: i32 = 360;
const WIN_H: i32 = 360;
const MODAL_W: i32 = 200;
const MODAL_H: i32 = 280;
const MODAL_LEFT_FINAL: i32 = (WIN_W - MODAL_W) / 2;
const GRID_COLS: i32 = 4;
const GRID_ROWS: i32 = 9;
const TILE_W: i32 = 40;
const TILE_H: i32 = 24;
const TILE_GAP: i32 = 4;
const TILE_PAD: i32 = 8;

const TOGGLE_NS: u64 = 5_000_000_000;
const UPDATE_EVERY: u32 = 30;

pub const DEFAULT_VIEW: (u16, u16) = (WIN_W as u16, WIN_H as u16);

pub struct ModalAnim {
    pub t: Fixed,
}

pub struct FpsReadout {
    pub counter: u32,
    pub accum_render_ns: u64,
}

pub struct ModeToggle {
    pub last_flip_ns: u64,
    pub elapsed_ns: u64,
    pub offscreen: bool,
}

#[mirui_macros::system(order = ANIMATION)]
pub fn modal_slide_system(world: &mut World) {
    let mut entities = alloc::vec::Vec::new();
    world.query::<ModalAnim>().collect_into(&mut entities);
    for e in entities {
        let next_t = if let Some(a) = world.get_mut::<ModalAnim>(e) {
            a.t += Fixed::ONE / 90;
            if a.t > Fixed::ONE {
                a.t -= Fixed::ONE;
            }
            a.t
        } else {
            continue;
        };

        let bounce = if next_t < Fixed::ONE / 2 {
            next_t * Fixed::from_int(2)
        } else {
            (Fixed::ONE - next_t) * Fixed::from_int(2)
        };
        let off_screen_offset = Fixed::from_int(-MODAL_LEFT_FINAL - MODAL_W);
        let tx = off_screen_offset * (Fixed::ONE - bounce);
        world.insert(e, WidgetTransform(Transform::translate(tx, Fixed::ZERO)));
        world.insert(e, Dirty);
    }
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
        let mut panels = alloc::vec::Vec::new();
        world.query::<ModalAnim>().collect_into(&mut panels);
        for e in panels {
            if now_offscreen {
                world.insert(e, OffscreenRender::default());
            } else {
                world.remove::<OffscreenRender>(e);
            }
        }
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

    let mut entities = alloc::vec::Vec::new();
    world.query::<FpsReadout>().collect_into(&mut entities);
    for e in entities {
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
            world.insert(e, Text(label.into_bytes()));
            world.insert(e, Dirty);
        }
    }
}

fn tile_color(idx: i32) -> ColorToken {
    match idx % 3 {
        0 => ColorToken::Primary,
        1 => ColorToken::Secondary,
        _ => ColorToken::Tertiary,
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    //~focus-start
    let modal = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: ColorToken::Surface,
            border_radius: Fixed::from_int(12),
            position: Position::Absolute,
            left: MODAL_LEFT_FINAL,
            top: 30,
            width: MODAL_W,
            height: MODAL_H
        ) [
            ModalAnim { t: Fixed::ZERO },
        ] {
            walk 0..(GRID_COLS * GRID_ROWS) with i {
                View (
                    bg_color: tile_color(i),
                    border_color: ColorToken::OnSurface,
                    border_width: Fixed::ONE,
                    border_radius: Fixed::from_int(6),
                    position: Position::Absolute,
                    left: TILE_PAD + (i % GRID_COLS) * (TILE_W + TILE_GAP),
                    top: TILE_PAD + (i / GRID_COLS) * (TILE_H + TILE_GAP),
                    width: TILE_W,
                    height: TILE_H
                )
            }
        }
    };
    //~focus-end

    let readout = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            text_color: ColorToken::OnSurface,
            position: Position::Absolute,
            left: 20,
            top: WIN_H - 30,
            width: WIN_W - 40,
            height: 24
        ) [
            Text(b"warming up...".to_vec()),
            FpsReadout {
                counter: 0,
                accum_render_ns: 0,
            },
        ]
    };

    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.clear();
        children.0.push(modal);
        children.0.push(readout);
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    // Modal buffer at RGBA8888 = 200×280×4 = 224 KB; the 256 KiB pool
    // fits one buffer with eviction headroom.
    app.with_theme(Theme::dark())
        .with_offscreen_pool_budget(256 * 1024);
    app.world.insert_resource(ModeToggle {
        last_flip_ns: 0,
        elapsed_ns: 0,
        offscreen: false,
    });
    app.add_system(modal_slide_system::system());
    app.add_system(mode_toggle_system::system());
    app.add_system(fps_readout_system::system());
    app.add_plugin(StdInstantClockPlugin);
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::IdMap;
    use crate::ui::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty())
        );
    }
}
