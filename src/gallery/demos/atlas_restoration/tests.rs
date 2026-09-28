use super::geometry::board_geometry;
use super::setup_app;
use super::state::{PictureNodes, PictureSurface};
use crate::gallery::play::expeditions::ExpeditionUiState;
use crate::gallery::play::picture::PictureModel;
use crate::prelude::*;
use crate::ui::Hidden;

#[test]
fn composition_has_semantic_controls_and_clues() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    setup_app(&mut app, root);
    assert!(app.world.find_by_id("picture_surface").is_some());
    assert!(app.world.find_by_id("picture_fill").is_some());
    assert!(app.world.find_by_id("pic_col_9").is_some());
    let map = app.world.find_by_id("picture_map").unwrap();
    assert!(app.world.has::<Hidden>(map));
    let locked = app.world.find_by_id("picture_map_level_1").unwrap();
    assert!(!app.world.has::<HitTarget>(locked));
    PictureNodes::open_map(&mut app.world);
    assert!(!app.world.has::<Hidden>(map));
    PictureNodes::select_map_level(&mut app.world, 1);
    assert!(!app.world.has::<Hidden>(map));
    PictureNodes::select_map_level(&mut app.world, 0);
    assert!(app.world.has::<Hidden>(map));
    let rules = app.world.find_by_id("picture_rules").unwrap();
    PictureNodes::open_rules(&mut app.world);
    assert!(!app.world.has::<Hidden>(rules));
    PictureNodes::close_map(&mut app.world);
    assert!(app.world.has::<Hidden>(rules));
    for _ in 0..25 {
        app.world
            .resource_mut::<PictureModel>()
            .unwrap()
            .reveal_hint();
    }
    PictureNodes::sync(&mut app.world);
    let result = app.world.find_by_id("picture_result").unwrap();
    assert!(!app.world.has::<Hidden>(result));
    PictureNodes::update(&mut app.world, PictureModel::restart);
    app.world
        .resource_mut::<ExpeditionUiState>()
        .unwrap()
        .open_briefing();
    PictureNodes::sync(&mut app.world);
    let briefing = app.world.find_by_id("picture_briefing").unwrap();
    assert!(!app.world.has::<Hidden>(briefing));
    app.world
        .resource_mut::<ExpeditionUiState>()
        .unwrap()
        .open_summary();
    PictureNodes::sync(&mut app.world);
    let summary = app.world.find_by_id("picture_summary").unwrap();
    assert!(!app.world.has::<Hidden>(summary));
    assert_eq!(app.world.query::<PictureSurface>().iter().count(), 1);
}

#[test]
fn first_picture_matches_reference_geometry() {
    let model = PictureModel::default();
    let geometry = board_geometry(&model);
    assert_eq!(
        (geometry.x, geometry.y, geometry.cell, geometry.size),
        (112, 81, 26, 130)
    );
}
