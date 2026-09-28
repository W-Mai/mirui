use super::setup_app;
use super::state::{FoldNodes, FoldSurface};
use crate::gallery::play::expeditions::ExpeditionUiState;
use crate::gallery::play::fold::FoldModel;
use crate::prelude::*;
use crate::ui::Hidden;

#[test]
fn composition_has_semantic_controls() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert!(app.world.find_by_id("fold_surface").is_some());
    assert!(app.world.find_by_id("fold_next").is_some());
    let map = app.world.find_by_id("fold_map").unwrap();
    assert!(app.world.has::<Hidden>(map));
    let locked = app.world.find_by_id("fold_map_level_1").unwrap();
    assert!(!app.world.has::<HitTarget>(locked));
    FoldNodes::open_map(&mut app.world);
    assert!(!app.world.has::<Hidden>(map));
    FoldNodes::select_map_level(&mut app.world, 1);
    assert!(!app.world.has::<Hidden>(map));
    FoldNodes::select_map_level(&mut app.world, 0);
    assert!(app.world.has::<Hidden>(map));
    let rules = app.world.find_by_id("fold_rules").unwrap();
    FoldNodes::open_rules(&mut app.world);
    assert!(!app.world.has::<Hidden>(rules));
    FoldNodes::close_map(&mut app.world);
    assert!(app.world.has::<Hidden>(rules));
    let solution_len = app
        .world
        .resource::<FoldModel>()
        .unwrap()
        .level()
        .solution_len();
    for step in 0..solution_len {
        let direction = app
            .world
            .resource::<FoldModel>()
            .unwrap()
            .level()
            .solution(step)
            .unwrap();
        app.world
            .resource_mut::<FoldModel>()
            .unwrap()
            .move_direction(direction);
    }
    FoldNodes::sync(&mut app.world);
    let result = app.world.find_by_id("fold_result").unwrap();
    assert!(!app.world.has::<Hidden>(result));
    FoldNodes::update(&mut app.world, FoldModel::restart);
    app.world
        .resource_mut::<ExpeditionUiState>()
        .unwrap()
        .open_briefing();
    FoldNodes::sync(&mut app.world);
    let briefing = app.world.find_by_id("fold_briefing").unwrap();
    assert!(!app.world.has::<Hidden>(briefing));
    app.world
        .resource_mut::<ExpeditionUiState>()
        .unwrap()
        .open_summary();
    FoldNodes::sync(&mut app.world);
    let summary = app.world.find_by_id("fold_summary").unwrap();
    assert!(!app.world.has::<Hidden>(summary));
    assert_eq!(app.world.query::<FoldSurface>().iter().count(), 1);
}
