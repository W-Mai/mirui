//! Dashboard panel (6×9 = 54 rounded tiles). Root is marked `Dirty`
//! every frame so the dirty rect spans the window; the panel subtree
//! itself stays clean, which is what lets the offscreen cache hit on
//! every frame after the first. Mode auto-toggles every 5s; the
//! bottom readout shows `render_nanos` averages for both modes.
//!
//! ```text
//! cargo run -p gallery --example offscreen_demo --release
//! ```

use mirui::components::Text;
use mirui::ecs::FrameTimings;
use mirui::plugins::StdInstantClockPlugin;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::theme::ColorToken;
use mirui::widget::{Children, OffscreenRender, Theme};
use mirui_macros::ui;

extern crate alloc;

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

/// Tag root, not panel: keeps the dirty rect window-sized while
/// leaving panel subtree clean so the offscreen cache stays valid.
struct ForceDirty;

struct PanelTarget;

struct FpsReadout {
    counter: u32,
    accum_render_ns: u64,
}

struct ModeToggle {
    last_flip_ns: u64,
    elapsed_ns: u64,
    offscreen: bool,
}

#[mirui::system(order = ANIMATION)]
fn mode_toggle_system(world: &mut World) {
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

#[mirui::system(order = ANIMATION)]
fn fps_readout_system(world: &mut World) {
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
            world.insert(e, mirui::widget::dirty::Dirty);
        }
    }
}

#[mirui::system(order = ANIMATION)]
fn force_dirty_system(world: &mut World) {
    let mut entities = alloc::vec::Vec::new();
    world.query::<ForceDirty>().collect_into(&mut entities);
    for e in entities {
        world.insert(e, mirui::widget::dirty::Dirty);
    }
}

fn tile_color(idx: i32) -> ColorToken {
    match idx % 3 {
        0 => ColorToken::Primary,
        1 => ColorToken::Secondary,
        _ => ColorToken::Tertiary,
    }
}

fn main() {
    let backend = SdlSurface::new(
        "mirui — OffscreenRender demo (auto-toggle every 5s)",
        WIN_W as u16,
        WIN_H as u16,
    );
    let mut app = App::new(backend);
    app.with_default_widgets()
        .with_default_systems()
        .with_theme(Theme::dark())
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

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 22, 28))
        .layout(LayoutStyle {
            width: Dimension::px(WIN_W),
            height: Dimension::px(WIN_H),
            ..Default::default()
        })
        .id();
    app.world.insert(root, ForceDirty);

    let panel = ui! {
        :(
            parent: root
            world: &mut app.world
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
                ) {}
            }
        }
    };

    let readout = ui! {
        :(
            parent: root
            world: &mut app.world
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
        ] {}
    };

    app.world
        .insert(root, Children(alloc::vec![panel, readout]));
    app.set_root(root);
    app.run();
}
