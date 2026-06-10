extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::Text;
use crate::ecs::{Entity, FrameTimings, World};
#[cfg(feature = "std")]
use crate::plugins::StdInstantClockPlugin;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
#[cfg(feature = "std")]
use crate::widget::Theme;
use crate::widget::dirty::Dirty;
use crate::widget::theme::ColorToken;
use crate::widget::{Children, OffscreenRender};

const WIN_W: i32 = 360;
const WIN_H: i32 = 360;
const PANEL_W: i32 = 280;
const PANEL_H: i32 = 260;
const PANEL_LEFT: i32 = (WIN_W - PANEL_W) / 2;
const PANEL_TOP: i32 = 20;
const GRID_COLS: i32 = 6;
const GRID_ROWS: i32 = 9;
const TILE_W: i32 = 36;
const TILE_H: i32 = 22;
const TILE_GAP: i32 = 4;
const TILE_PAD: i32 = 8;

const TOGGLE_NS: u64 = 5_000_000_000;
const UPDATE_EVERY: u32 = 30;

pub const DEFAULT_VIEW: (u16, u16) = (WIN_W as u16, WIN_H as u16);

// Tag root, not panel: the dirty rect spans the window so the panel
// subtree stays clean and the offscreen cache hit holds.
pub struct ForceDirty;

pub struct PanelTarget;

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
        world.query::<PanelTarget>().collect_into(&mut panels);
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

#[mirui_macros::system(order = ANIMATION)]
pub fn force_dirty_system(world: &mut World) {
    let mut entities = alloc::vec::Vec::new();
    world.query::<ForceDirty>().collect_into(&mut entities);
    for e in entities {
        world.insert(e, Dirty);
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
    world.insert(parent, ForceDirty);

    let panel = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: ColorToken::Surface,
            border_radius: Fixed::from_int(12),
            position: Position::Absolute,
            left: PANEL_LEFT,
            top: PANEL_TOP,
            width: PANEL_W,
            height: PANEL_H
        ) [
            PanelTarget,
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
        children.0.push(panel);
        children.0.push(readout);
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_theme(Theme::dark())
        .with_offscreen_pool_budget(512 * 1024);
    app.world.insert_resource(ModeToggle {
        last_flip_ns: 0,
        elapsed_ns: 0,
        offscreen: false,
    });
    app.add_system(mode_toggle_system::system());
    app.add_system(force_dirty_system::system());
    app.add_system(fps_readout_system::system());
    app.add_plugin(StdInstantClockPlugin);
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
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
                .is_some_and(|c| !c.0.is_empty())
        );
    }
}
