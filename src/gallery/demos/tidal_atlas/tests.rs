use super::input::local_cell;
use super::setup_app;
#[cfg(feature = "persistence")]
use super::state::TideNodes;
use super::state::TideSurface;
#[cfg(feature = "persistence")]
use crate::gallery::play::storage::TidalReplayLog;
#[cfg(feature = "persistence")]
use crate::gallery::play::tidal::{BOARD_SIZE, TideCommand, TideModel};
use crate::prelude::{App, Fixed};

#[test]
fn composition_uses_one_dense_board_and_semantic_controls() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert!(app.world.find_by_id("tide_surface").is_some());
    assert!(app.world.find_by_id("tide_offer_0").is_some());
    assert!(app.world.find_by_id("tide_place").is_some());
    assert_eq!(app.world.query::<TideSurface>().iter().count(), 1);
}

#[test]
fn board_hit_testing_rejects_cell_gaps() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    app.set_root(root);
    app.systems.run_all(&mut app.world);
    app.render().unwrap();
    let surface = app.world.find_by_id("tide_surface").unwrap();
    assert_eq!(
        local_cell(
            &app.world,
            surface,
            Fixed::from_int(14),
            Fixed::from_int(60)
        ),
        Some(0)
    );
    assert_eq!(
        local_cell(
            &app.world,
            surface,
            Fixed::from_int(46),
            Fixed::from_int(60)
        ),
        None
    );
}

#[cfg(feature = "persistence")]
#[test]
fn persistent_commands_append_to_the_replay_log() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    let index = (0..BOARD_SIZE)
        .find(|index| app.world.resource::<TideModel>().unwrap().valid(*index))
        .unwrap() as u8;
    TideNodes::dispatch(&mut app.world, TideCommand::Place { index, choice: 0 });
    assert_eq!(app.world.resource::<TidalReplayLog>().unwrap().len(), 1);
    assert_eq!(app.world.resource::<TideModel>().unwrap().turn(), 1);
}
