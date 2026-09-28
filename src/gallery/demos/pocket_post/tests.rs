use super::input::surface_gesture;
use super::setup_app;
use super::state::PostSurface;
use crate::gallery::play::post::PostModel;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::ComputedRect;

#[test]
fn composition_uses_one_dense_route_surface_and_semantic_controls() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert!(app.world.find_by_id("post_surface").is_some());
    assert!(app.world.find_by_id("post_run").is_some());
    assert!(app.world.find_by_id("post_manifest_2").is_some());
    assert_eq!(app.world.query::<PostSurface>().iter().count(), 1);
}

#[test]
fn switch_hit_targets_map_through_the_surface_geometry() {
    let mut app = App::headless(960, 640);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.systems.run_all(&mut app.world);
    let surface = app.world.find_by_id("post_surface").unwrap();
    app.world
        .insert(surface, ComputedRect(Rect::new(20, 30, 720, 480)));
    let rect = app.world.get::<ComputedRect>(surface).unwrap().0;
    let x = rect.x + rect.w * Fixed::from_int(151) / Fixed::from_int(480);
    let y = rect.y + rect.h * Fixed::from_int(157) / Fixed::from_int(320);
    assert_eq!(app.world.resource::<PostModel>().unwrap().switch(0), 0);
    assert!(surface_gesture(
        &mut app.world,
        surface,
        &GestureEvent::Tap {
            x,
            y,
            target: surface,
        },
    ));
    assert_eq!(app.world.resource::<PostModel>().unwrap().switch(0), 1);
}
