use super::composition::build_widgets;
use super::render::life_view;
use super::runtime::dims_from_px;
use super::state::{GLIDER, GOSPER_GUN, LifeBoard, MAX_GRID_EDGE};
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::ComputedRect;
use crate::ui::render_system::update_layout;

#[test]
fn dims_track_viewport() {
    assert_eq!(dims_from_px(10, 10), (48, 48), "tiny viewport floors at 48");
    let huge = dims_from_px(10_000, 10_000);
    assert_eq!(huge.0, huge.1);
    assert!(huge.0 <= MAX_GRID_EDGE, "huge viewport stays bounded");
    let mid = dims_from_px(200, 100);
    assert!(
        mid.0 > mid.1,
        "wider-than-tall viewport: more cols than rows"
    );
    assert_eq!(dims_from_px(480, 240), (160, 80));
}

#[test]
fn responsive_shell_contains_the_board_at_supported_viewports() {
    for (width, height) in [(320, 568), (480, 320), (1024, 640)] {
        let mut app = App::headless(width, height);
        app.with_default_widgets()
            .with_default_systems()
            .with_widget(life_view());
        let root = app.spawn_root().id();
        app.compose(root, |cx| build_widgets(cx, width, height));
        app.set_root(root);
        update_layout(
            &mut app.world,
            root,
            &Viewport::new(width, height, Fixed::ONE),
        );

        for id in ["life_header", "life_stage", "life_board"] {
            let entity = app.world.find_by_id(id).unwrap();
            let rect = app.world.get::<ComputedRect>(entity).unwrap().0;
            assert!(rect.x >= Fixed::ZERO, "{id} starts before the viewport");
            assert!(rect.y >= Fixed::ZERO, "{id} starts above the viewport");
            assert!(
                rect.x + rect.w <= Fixed::from_int(width as i32),
                "{id} exceeds {width}x{height} horizontally",
            );
            assert!(
                rect.y + rect.h <= Fixed::from_int(height as i32),
                "{id} exceeds {width}x{height} vertically",
            );
        }
    }
}

#[test]
fn stepping_reuses_the_existing_cell_buffers() {
    let mut board = LifeBoard::new(96, 64);
    board.seed((3, 2), GOSPER_GUN);
    let cell_capacity = board.cell.capacity();
    let scratch_capacity = board.scratch.capacity();

    for _ in 0..120 {
        board.step();
    }

    assert_eq!(board.cell.capacity(), cell_capacity);
    assert_eq!(board.scratch.capacity(), scratch_capacity);
}

#[test]
fn blinker_oscillates_period_2() {
    let mut b = LifeBoard::new(32, 32);
    b.seed((10, 5), &[(0, 0), (0, 1), (0, 2)]);
    let gen0 = b.cell.clone();
    b.step();
    let gen1 = b.cell.clone();
    b.step();
    let gen2 = b.cell.clone();
    assert_ne!(gen0, gen1, "blinker must change on step 1");
    assert_eq!(gen0, gen2, "blinker returns to itself after 2 steps");
}

#[test]
fn glider_wraps_toroidal_after_one_period() {
    let mut b = LifeBoard::new(32, 32);
    let glider = [(0, 1), (1, 2), (2, 0), (2, 1), (2, 2)];
    b.seed((0, 0), &glider);
    let count0 = b.alive_count();
    for _ in 0..4 {
        b.step();
    }
    let mut expected = LifeBoard::new(32, 32);
    for &(r, c) in &glider {
        expected.set(r + 1, c + 1);
    }
    assert_eq!(b.alive_count(), count0, "glider preserves cell count");
    assert_eq!(b.cell, expected.cell, "glider shifts (1,1) after 4 steps");
}

#[test]
fn gosper_gun_stays_active() {
    let mut b = LifeBoard::new(64, 64);
    b.seed((3, 2), GOSPER_GUN);
    let start = b.alive_count();
    assert_eq!(start, GOSPER_GUN.len(), "gun seeds all 36 cells");
    for _ in 0..120 {
        b.step();
    }
    assert!(
        b.alive_count() > start,
        "gun keeps the board active: {start} -> {}",
        b.alive_count(),
    );
}

#[test]
fn random_glider_drops_keep_an_empty_board_alive() {
    // No seed pattern; only the periodic glider injection runs. The board
    // must gain life from the drops rather than staying empty.
    let mut b = LifeBoard::new(64, 64);
    assert_eq!(b.alive_count(), 0);
    let mut max_seen = 0;
    for _ in 0..400 {
        b.advance();
        max_seen = max_seen.max(b.alive_count());
    }
    assert!(
        max_seen >= GLIDER.len(),
        "drops must inject gliders: {max_seen}"
    );
}
