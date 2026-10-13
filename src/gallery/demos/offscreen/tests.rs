use super::composition::build_widgets;
use super::state::FpsReadout;
use super::systems::{UPDATE_EVERY, force_dirty_system, fps_readout_system};
use crate::ecs::{FrameTimings, SystemScheduler};
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::dirty::{Dirty, VisualDirty};
use crate::ui::render_system::try_update_layout;
use crate::ui::widgets::Text;
use crate::ui::{Children, ComputedRect};

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(360, 360);
    app.with_default_widgets();
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    assert!(
        app.world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty())
    );
}

#[test]
fn forced_redraw_tracks_the_app_root_without_dirtying_the_cached_panel() {
    let mut app = App::headless(360, 360);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, build_widgets());
    let panel = app.world.find_by_id("offscreen_panel").unwrap();
    let readout = app.world.find_by_id("offscreen_readout").unwrap();
    app.world.remove::<Dirty>(root);
    app.world.remove::<Dirty>(panel);
    app.world.remove::<Dirty>(readout);
    let mut scheduler = SystemScheduler::new();
    scheduler.add(force_dirty_system::system());
    scheduler.run_all(&mut app.world).unwrap();
    assert!(app.world.has::<Dirty>(root));
    assert!(!app.world.has::<Dirty>(panel));
    assert!(!app.world.has::<Dirty>(readout));

    let replacement = app.spawn_root().id();
    app.world.remove::<Dirty>(root);
    app.world.remove::<Dirty>(replacement);
    scheduler.run_all(&mut app.world).unwrap();
    assert!(app.world.has::<Dirty>(replacement));
    assert!(!app.world.has::<Dirty>(root));
    assert!(!app.world.has::<Dirty>(panel));
}

#[test]
fn panel_is_centered_and_contained_across_supported_viewports() {
    for (width, height) in [(320, 320), (320, 568), (1024, 640)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        app.compose(root, build_widgets());
        try_update_layout(
            &mut app.world,
            root,
            &Viewport::new(width, height, Fixed::ONE),
        )
        .unwrap();

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
    let mut app = App::headless(360, 360);
    app.with_default_widgets();
    app.add_resource(FrameTimings {
        render_nanos: 123_000,
        ..FrameTimings::default()
    });
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    let world = &mut app.world;

    let readout = world.find_by_id("offscreen_readout").unwrap();
    let paragraph = world.get::<Text>(readout).unwrap().paragraph().clone();
    let state = world.get_mut::<FpsReadout>(readout).unwrap();
    state.counter = UPDATE_EVERY - 1;
    state.accum_render_ns = 123_000 * (UPDATE_EVERY - 1) as u64;
    let mut scheduler = SystemScheduler::new();
    scheduler.add(fps_readout_system::system());
    scheduler.run_all(world).unwrap();

    let text = world.get::<Text>(readout).unwrap();
    assert!(text.resolve(world.resource()).contains("render avg 123us"));
    assert_eq!(text.paragraph(), &paragraph);
}

#[test]
fn readout_stays_clean_between_sample_windows() {
    let mut app = App::headless(360, 360);
    app.with_default_widgets();
    app.add_resource(FrameTimings {
        render_nanos: 123_000,
        ..FrameTimings::default()
    });
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    let world = &mut app.world;

    let readout = world.find_by_id("offscreen_readout").unwrap();
    world.remove::<Dirty>(readout);
    world.remove::<VisualDirty>(readout);
    let mut scheduler = SystemScheduler::new();
    scheduler.add(fps_readout_system::system());
    scheduler.run_all(world).unwrap();

    assert!(!world.has::<Dirty>(readout));
    assert!(!world.has::<VisualDirty>(readout));
}
