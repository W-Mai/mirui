extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "std")]
use crate::ecs::Entity;
use crate::ecs::{FrameTimings, World};
use crate::prelude::*;
use crate::ui::OffscreenRender;
use crate::ui::widgets::{ParagraphStyle, Text};

const WIN_W: i32 = 360;
const WIN_H: i32 = 360;
const PANEL_W: i32 = 280;
const PANEL_H: i32 = 260;
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

fn tile_color(idx: i32) -> ColorToken {
    match idx % 3 {
        0 => ColorToken::Primary,
        1 => ColorToken::Secondary,
        _ => ColorToken::Tertiary,
    }
}

#[compose]
pub fn build_widgets() {
    let root = cx.parent();
    cx.world_mut().insert(root, ForceDirty);

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            row_gap: 6,
            padding: Padding::all(12),
            bg_color: ColorToken::SurfaceVariant
        ) {
            View (
                id: "offscreen_panel",
                bg_color: ColorToken::Surface,
                border_radius: Fixed::from_int(12),
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
            Text (
                id: "offscreen_readout",
                "warming up...",
                width: Dimension::percent(100),
                max_width: 320,
                height: 24,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            ) [
                FpsReadout {
                    counter: 0,
                    accum_render_ns: 0,
                },
            ]
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_offscreen_pool_budget(512 * 1024);
    app.world.insert_resource(ModeToggle {
        last_flip_ns: 0,
        elapsed_ns: 0,
        offscreen: false,
    });
    app.add_system(mode_toggle_system::system());
    app.add_system(force_dirty_system::system());
    app.add_system(fps_readout_system::system());
    app.add_plugin(StdInstantClockPlugin);
    app.compose(parent, build_widgets);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(360, 360);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Viewport;
    use crate::ui::render_system::update_layout;
    use crate::ui::{Children, ComputedRect, IdMap, UiScope};

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty())
        );
    }

    #[test]
    fn panel_is_centered_and_contained_across_supported_viewports() {
        for (width, height) in [(320, 320), (320, 568), (1024, 640)] {
            let mut app = App::headless(width, height);
            app.with_default_widgets().with_default_systems();
            let root = app.spawn_root().id();
            app.compose(root, build_widgets);
            app.set_root(root);
            update_layout(
                &mut app.world,
                root,
                &Viewport::new(width, height, Fixed::ONE),
            );

            let panel = app.world.find_by_id("offscreen_panel").unwrap();
            let rect = app.world.get::<ComputedRect>(panel).unwrap().0;
            assert!(rect.x >= Fixed::ZERO);
            assert!(rect.y >= Fixed::ZERO);
            assert!(rect.x + rect.w <= Fixed::from_int(width as i32));
            assert!(rect.y + rect.h <= Fixed::from_int(height as i32));
        }
    }

    #[test]
    fn readout_updates_content_without_replacing_paragraph_style() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(FrameTimings {
            render_nanos: 123_000,
            ..FrameTimings::default()
        });
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);

        let readout = world.find_by_id("offscreen_readout").unwrap();
        let paragraph = world.get::<Text>(readout).unwrap().paragraph().clone();
        let state = world.get_mut::<FpsReadout>(readout).unwrap();
        state.counter = UPDATE_EVERY - 1;
        state.accum_render_ns = 123_000 * (UPDATE_EVERY - 1) as u64;
        fps_readout_system(&mut world);

        let text = world.get::<Text>(readout).unwrap();
        assert!(text.resolve(&world).contains("render avg 123us"));
        assert_eq!(text.paragraph(), &paragraph);
    }
}
