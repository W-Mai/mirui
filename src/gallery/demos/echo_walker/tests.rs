use super::input::{handle_key, local_cell};
use super::render::{cell_origin, map_box};
use super::setup_app;
use super::state::EchoSurface;
use crate::gallery::play::echo::EchoModel;
#[cfg(feature = "persistence")]
use crate::gallery::play::storage::EchoReplayLog;
use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::widgets::Button;

#[test]
fn composition_uses_one_dense_board_and_semantic_controls() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert!(app.world.find_by_id("echo_surface").is_some());
    assert!(app.world.find_by_id("echo_rewind").is_some());
    assert_eq!(app.world.query::<EchoSurface>().iter().count(), 1);
    assert!(app.world.query::<Button>().iter().count() >= 15);
}

#[test]
fn board_hit_testing_maps_the_player_cell() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.systems.run_all(&mut app.world);
    app.render().unwrap();
    let surface = app.world.find_by_id("echo_surface").unwrap();
    let model = app.world.resource::<EchoModel>().unwrap();
    let map = map_box(model);
    let (x, y) = cell_origin(model.position(), map);
    let rect = app.world.get::<ComputedRect>(surface).unwrap().0;
    let screen_x = rect.x + Fixed::from_int(x + map.size / 2) * rect.w / Fixed::from_int(480);
    let screen_y = rect.y + Fixed::from_int(y + map.size / 2) * rect.h / Fixed::from_int(320);
    assert_eq!(
        local_cell(&app.world, surface, screen_x, screen_y),
        Some(model.position())
    );
}

#[test]
fn keyboard_wait_advances_exactly_one_beat() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert!(handle_key(&mut app.world, ' '));
    assert_eq!(app.world.resource::<EchoModel>().unwrap().tick(), 1);
    #[cfg(feature = "persistence")]
    assert_eq!(app.world.resource::<EchoReplayLog>().unwrap().len(), 1);
}
