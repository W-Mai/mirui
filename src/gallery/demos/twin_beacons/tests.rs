use super::setup_app;
use super::state::{TwinNodes, TwinSurface};
use crate::gallery::play::expeditions::ExpeditionUiState;
use crate::gallery::play::twin::TwinModel;
use crate::prelude::*;
use crate::ui::Hidden;

#[test]
fn composition_has_semantic_controls() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert!(app.world.find_by_id("twin_surface").is_some());
    assert!(app.world.find_by_id("twin_next").is_some());
    let map = app.world.find_by_id("twin_map").unwrap();
    assert!(app.world.has::<Hidden>(map));
    let locked = app.world.find_by_id("twin_map_level_1").unwrap();
    assert!(!app.world.has::<HitTarget>(locked));
    TwinNodes::open_map(&mut app.world);
    assert!(!app.world.has::<Hidden>(map));
    TwinNodes::select_map_level(&mut app.world, 1);
    assert!(!app.world.has::<Hidden>(map));
    TwinNodes::select_map_level(&mut app.world, 0);
    assert!(app.world.has::<Hidden>(map));
    let rules = app.world.find_by_id("twin_rules").unwrap();
    TwinNodes::open_rules(&mut app.world);
    assert!(!app.world.has::<Hidden>(rules));
    TwinNodes::close_map(&mut app.world);
    assert!(app.world.has::<Hidden>(rules));
    let solution_len = app
        .world
        .resource::<TwinModel>()
        .unwrap()
        .level()
        .solution_len();
    for step in 0..solution_len {
        let direction = app
            .world
            .resource::<TwinModel>()
            .unwrap()
            .level()
            .solution(step)
            .unwrap();
        app.world
            .resource_mut::<TwinModel>()
            .unwrap()
            .move_direction(direction);
    }
    TwinNodes::sync(&mut app.world);
    let result = app.world.find_by_id("twin_result").unwrap();
    assert!(!app.world.has::<Hidden>(result));
    TwinNodes::update(&mut app.world, TwinModel::restart);
    app.world
        .resource_mut::<ExpeditionUiState>()
        .unwrap()
        .open_briefing();
    TwinNodes::sync(&mut app.world);
    let briefing = app.world.find_by_id("twin_briefing").unwrap();
    assert!(!app.world.has::<Hidden>(briefing));
    app.world
        .resource_mut::<ExpeditionUiState>()
        .unwrap()
        .open_summary();
    TwinNodes::sync(&mut app.world);
    let summary = app.world.find_by_id("twin_summary").unwrap();
    assert!(!app.world.has::<Hidden>(summary));
    assert_eq!(app.world.query::<TwinSurface>().iter().count(), 1);
}
