use super::cells::{GLIDER, GRID_HEIGHT, GRID_WIDTH, MossCells};
use super::model::MossModel;

fn empty() -> MossCells {
    MossCells::EMPTY
}

#[test]
fn empty_and_isolated_cells_die_without_wraparound() {
    assert_eq!(empty().evolve(), empty());
    let mut cells = empty();
    cells.set(0, 0, 1);
    assert_eq!(cells.evolve(), empty());
}

#[test]
fn block_is_stable_and_blinker_returns_after_two_steps() {
    let mut block = empty();
    for (x, y) in [(4, 4), (5, 4), (4, 5), (5, 5)] {
        block.set(x, y, 1);
    }
    let evolved = block.evolve();
    for (x, y) in [(4, 4), (5, 4), (4, 5), (5, 5)] {
        assert_eq!(evolved.get(x, y), 2);
    }

    let mut blinker = empty();
    for x in 4..=6 {
        blinker.set(x, 5, 1);
    }
    let second = blinker.evolve().evolve();
    for x in 4..=6 {
        assert_ne!(second.get(x, 5), 0);
    }
    assert_eq!(second.live_count(), 3);
}

#[test]
fn glider_moves_one_cell_after_four_generations() {
    let mut cells = empty();
    for (x, y) in GLIDER {
        cells.set((x + 4) as u8, (y + 3) as u8, 1);
    }
    for _ in 0..4 {
        cells = cells.evolve();
    }
    let mut expected = empty();
    for (x, y) in GLIDER {
        expected.set((x + 5) as u8, (y + 4) as u8, 1);
    }
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            assert_eq!(cells.get(x, y) != 0, expected.get(x, y) != 0);
        }
    }
}

#[test]
fn ages_saturate_and_do_not_change_survival_rules() {
    let mut cells = empty();
    for (x, y) in [(4, 4), (5, 4), (4, 5), (5, 5)] {
        cells.set(x, y, u8::MAX);
    }
    let next = cells.evolve();
    assert_eq!(next.get(4, 4), u8::MAX);

    let mut parents = empty();
    for (x, y) in [(7, 6), (8, 6), (9, 6)] {
        parents.set(x, y, 3);
    }
    assert_eq!(parents.evolve().get(8, 5), 1);
}

#[test]
fn drag_commit_and_cancel_are_atomic() {
    let mut model = MossModel::default();
    let initial = *model.cells();
    model.begin_stroke(0, 0);
    model.continue_stroke(19, 11);
    model.end_stroke(true);
    assert_eq!(*model.cells(), initial);
    assert_eq!(model.history_len(), 0);
    model.begin_stroke(0, 0);
    model.continue_stroke(19, 11);
    model.end_stroke(false);
    assert_eq!(model.history_len(), 1);
    model.undo();
    assert_eq!(*model.cells(), initial);
}

#[test]
fn automatic_evolution_does_not_accumulate_history() {
    let mut model = MossModel::default();
    model.toggle_running();
    for _ in 0..32 {
        model.advance_ms(125);
    }
    assert_eq!(model.history_len(), 0);
    assert!(model.generation() > 0);
}

#[test]
fn manual_step_seed_and_clear_are_undoable() {
    let mut model = MossModel::default();
    let initial = *model.cells();
    model.step();
    assert_eq!(model.history_len(), 1);
    model.undo();
    assert_eq!(*model.cells(), initial);
    model.open_seeds();
    model.load_seed(1);
    assert_eq!(model.seed_id(), 1);
    model.undo();
    assert_eq!(*model.cells(), initial);
    model.open_clear();
    model.confirm_clear();
    assert_eq!(model.live_count(), 0);
    model.undo();
    assert_eq!(*model.cells(), initial);
}

#[test]
fn random_seed_is_fixed_and_history_is_bounded() {
    assert_eq!(MossCells::seed(2), MossCells::seed(2));
    let mut model = MossModel::default();
    for index in 0..40 {
        model.open_seeds();
        model.load_seed((index % 3) as u8);
    }
    assert_eq!(model.history_len(), 16);
}

#[test]
fn model_storage_is_bounded() {
    assert_eq!(core::mem::size_of::<MossCells>(), 240);
    assert!(core::mem::size_of::<MossModel>() <= 8_800);
}
