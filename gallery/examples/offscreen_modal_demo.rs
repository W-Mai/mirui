//! Modal slides in from the left with `WidgetTransform` translate.
//! The 36-tile subtree never changes pixels — only the outer entity's
//! transform animates. Mode auto-toggles every 5s; the readout shows
//! the per-frame raster cost in both paths.
//!
//! ```text
//! cargo run -p gallery --example offscreen_modal_demo --release
//! ```

use mirui::components::Text;
use mirui::components::WidgetTransform;
use mirui::ecs::FrameTimings;
use mirui::plugins::StdInstantClockPlugin;
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed, Transform};
use mirui::widget::theme::ColorToken;
use mirui::widget::{Children, OffscreenRender, Theme};
use mirui_macros::ui;

extern crate alloc;

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

struct ModalAnim {
    t: Fixed,
}

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
fn modal_slide_system(world: &mut World) {
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
        world.insert(e, mirui::widget::dirty::Dirty);
    }
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

fn tile_color(idx: i32) -> ColorToken {
    match idx % 3 {
        0 => ColorToken::Primary,
        1 => ColorToken::Secondary,
        _ => ColorToken::Tertiary,
    }
}

fn main() {
    let backend = SdlSurface::new(
        "mirui — OffscreenRender + WidgetTransform animation (auto-toggle 5s)",
        WIN_W as u16,
        WIN_H as u16,
    );
    let mut app = App::new(backend);
    app.with_default_widgets()
        .with_default_systems()
        .with_theme(Theme::dark())
        // Modal buffer at RGBA8888 = 200×280×4 = 224 KB; 256 KiB
        // pool fits one buffer with eviction headroom.
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

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 22, 28))
        .layout(LayoutStyle {
            width: Dimension::px(WIN_W),
            height: Dimension::px(WIN_H),
            ..Default::default()
        })
        .id();

    let modal = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        modal (
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
                tile (
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

        readout (
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
        .insert(root, Children(alloc::vec![modal, readout]));
    app.set_root(root);
    app.run();
}
