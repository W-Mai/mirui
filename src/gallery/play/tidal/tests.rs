use super::model::TideModel;
use super::types::{
    BOARD_SIZE, Forecast, HISTORY_CAPACITY, ISLAND_COUNT, TideLevel, Tile, Weather,
};
use crate::gallery::play::change::ChangeSet;

/// The placement that raises the next harvest the most, nudged towards
/// the island's goal tile.
pub(crate) fn best_move(model: &TideModel) -> (u8, u8) {
    let mut best = None;
    for index in 0..BOARD_SIZE {
        for choice in 0..3 {
            if let Some(preview) = model.preview(index, choice) {
                let delta = preview.delta;
                let goal_bonus = i16::from(model.offer(choice) == model.goal().tile) * 2;
                if best.is_none_or(|(_, value, _, _)| delta + goal_bonus > value) {
                    best = Some((index as u8, delta + goal_bonus, choice as u8, delta));
                }
            }
        }
    }
    let (index, _, choice, _) = best.expect("legal placement");
    (index, choice)
}

fn solve_island(model: &mut TideModel) {
    while !model.settled() {
        let (index, choice) = best_move(model);
        assert!(model.place(index, choice).contains(ChangeSet::MODEL));
    }
}

#[test]
fn reference_seed_matches_exported_initial_state() {
    let model = TideModel::default();
    assert_eq!(model.seed(), 4096);
    assert_eq!(model.terrain(0), 1);
    assert_eq!(model.terrain(1), 0);
    assert_eq!(model.tile(14), Tile::Beacon);
    assert_eq!(model.tile(15), Tile::Grove);
    assert_eq!(model.tile(20), Tile::Lagoon);
    assert_eq!(model.offer(0), Tile::Beacon);
    assert_eq!(model.offer(1), Tile::Lens);
    assert_eq!(model.offer(2), Tile::Harbor);
    assert_eq!(model.goal().name.zh, "邻里计划");
    assert_eq!(model.goal().name.en, "Village plan");
}

#[test]
fn a_second_tap_places_and_arrows_walk_the_legal_spots() {
    use super::types::TideMessage;

    let mut model = TideModel::new(8);
    assert!(model.valid(13));
    model.tap_cell(13);
    assert_eq!(model.pending(), Some(13));
    model.tap_cell(13);
    assert_eq!(model.turn(), 1);
    assert_ne!(model.tile(13), Tile::Sea);

    model.step_pending(1, 0);
    let from = model.pending().expect("a legal spot to the right");
    assert!(model.valid(usize::from(from)));
    assert!(from % 6 > 13 % 6);
    for (dx, dy) in [(0, 1), (-1, 0), (0, -1), (1, 0)] {
        let before = model.pending().unwrap();
        model.step_pending(dx, dy);
        let after = model.pending().unwrap();
        assert!(model.valid(usize::from(after)));
        let moved = (i16::from(after % 6) - i16::from(before % 6)) * i16::from(dx)
            + (i16::from(after / 6) - i16::from(before / 6)) * i16::from(dy);
        assert!(after == before || moved > 0, "{before} -> {after}");
    }

    model.cancel_preview();
    let far = (0..BOARD_SIZE)
        .find(|index| model.tile(*index) == Tile::Sea && !model.valid(*index))
        .unwrap() as u8;
    assert!(model.select_cell(far).contains(ChangeSet::MODEL));
    assert_eq!(model.message(), TideMessage::Shore);
    assert_eq!(model.select_cell(far), ChangeSet::NONE);
    assert!(model.place_pending().contains(ChangeSet::MODEL));
    assert_eq!(model.message(), TideMessage::Spot);
    assert_eq!(model.turn(), 1);
}

#[test]
fn placing_drops_the_tile_and_a_harvest_pops_its_scores() {
    let mut model = TideModel::new(9);
    assert!(!model.animating());
    assert_eq!(model.advance_ms(16), ChangeSet::NONE);
    for _ in 0..5 {
        let (index, choice) = best_move(&model);
        model.place(index, choice);
        assert!(model.gains().iter().all(|gain| *gain == 0));
    }
    assert!(model.dropping().is_some());
    for _ in 0..60 {
        model.advance_ms(16);
    }
    assert!(!model.animating());
    assert!(model.dropping().is_none());

    let (index, choice) = best_move(&model);
    model.place(index, choice);
    assert_eq!(
        model.dropping().map(|(cell, _)| cell),
        Some(usize::from(index))
    );
    let total: u16 = model.gains().iter().map(|gain| u16::from(*gain)).sum();
    assert_eq!(total, model.harvests()[0]);
    assert_eq!(model.gain_lift(), 0);
    assert!(model.advance_ms(16).contains(ChangeSet::VISUAL));
    for _ in 0..5 {
        model.advance_ms(16);
    }
    assert!(model.gain_lift() > 0);
    assert!(model.gain_alpha() > 0);

    // The next placement clears the scores at once.
    let (index, choice) = best_move(&model);
    model.place(index, choice);
    assert_eq!(model.gain_lift(), 0);
    assert!(model.gains().iter().all(|gain| *gain == 0));
    model.undo();
    assert!(!model.animating());
}

#[test]
fn harvest_scores_fade_on_their_own_and_a_new_voyage_starts_clean() {
    let mut model = TideModel::new(12);
    for _ in 0..6 {
        let (index, choice) = best_move(&model);
        model.place(index, choice);
    }
    for _ in 0..200 {
        model.advance_ms(16);
    }
    assert!(!model.animating());
    assert_eq!(model.gain_lift(), 0);
    assert!(model.gains().iter().all(|gain| *gain == 0));

    assert_eq!(model.new_voyage(), ChangeSet::NONE);
    for island in 0..ISLAND_COUNT {
        solve_island(&mut model);
        let perk = (island < 3).then(|| model.perk_offer(0));
        model.continue_voyage(perk);
    }
    assert!(model.complete());
    let seed = model.seed();
    assert!(model.new_voyage().contains(ChangeSet::PERSISTENCE));
    assert!(!model.complete());
    assert_eq!(model.turn(), 0);
    assert_eq!(model.chapter(), 0);
    assert_eq!(model.cumulative_score(), 0);
    assert_ne!(model.seed(), seed);
}

#[test]
fn preview_is_pure_and_adjacency_is_cardinal() {
    let model = TideModel::new(8);
    assert!(!model.valid(0));
    assert!(model.valid(13));
    assert!(!model.valid(14));
    let before_state = model.state;
    let before_history_len = model.history.len;
    let preview = model.preview(13, 0).expect("legal preview");
    assert_eq!(preview.terrain, model.terrain(13));
    assert_eq!(model.state, before_state);
    assert_eq!(model.history.len, before_history_len);
}

#[test]
fn repeated_selections_are_no_ops() {
    let mut model = TideModel::new(8);
    assert_eq!(model.select_offer(0), ChangeSet::NONE);

    let index = (0..BOARD_SIZE)
        .find(|index| model.valid(*index))
        .expect("legal placement") as u8;
    assert!(model.select_cell(index).contains(ChangeSet::VISUAL));
    assert_eq!(model.select_cell(index), ChangeSet::NONE);
}

#[test]
fn cached_scores_follow_place_and_undo() {
    let mut model = TideModel::new(9);
    let expected_forecast = (0..BOARD_SIZE)
        .map(|index| model.tile_score(index, model.forecast()))
        .sum::<u16>();
    assert_eq!(model.forecast_score(), expected_forecast);

    let initial_forecast = model.forecast_score();
    let initial_progress = model.goal_progress();
    let (index, choice) = best_move(&model);
    assert!(model.place(index, choice).contains(ChangeSet::MODEL));
    let expected_forecast = (0..BOARD_SIZE)
        .map(|cell| model.tile_score(cell, model.forecast()))
        .sum::<u16>();
    assert_eq!(model.forecast_score(), expected_forecast);
    assert!(model.goal_progress() >= initial_progress);

    assert!(model.undo().contains(ChangeSet::MODEL));
    assert_eq!(model.forecast_score(), initial_forecast);
    assert_eq!(model.goal_progress(), initial_progress);
}

#[cfg(feature = "persistence")]
#[test]
fn full_replay_rejects_before_mutating_the_model() {
    use crate::gallery::play::storage::TIDAL_REPLAY_CAPACITY;

    let mut model = TideModel::new(9);
    let (index, choice) = best_move(&model);
    for _ in 0..TIDAL_REPLAY_CAPACITY / 2 {
        assert!(model.place(index, choice).contains(ChangeSet::PERSISTENCE));
        assert!(model.undo().contains(ChangeSet::PERSISTENCE));
    }
    let state = model.state;
    let history_len = model.history_len();
    assert_eq!(usize::from(model.replay_len()), TIDAL_REPLAY_CAPACITY);
    assert_eq!(model.place(index, choice), ChangeSet::NONE);
    assert_eq!(model.state, state);
    assert_eq!(model.history_len(), history_len);
}

#[test]
fn scoring_preserves_water_flood_and_distinct_neighbour_rules() {
    let mut model = TideModel::new(4);
    model.state.board.fill(Tile::Sea);
    model.state.terrain.fill(1);
    model.state.board[14] = Tile::Field;
    assert_eq!(model.tile_score(14, Forecast::default()), 10);
    model.state.terrain[14] = 0;
    assert_eq!(
        model.tile_score(
            14,
            Forecast {
                tide: TideLevel::High,
                weather: Weather::Clear
            }
        ),
        0
    );
    model.state.board[15] = Tile::Dike;
    assert!(
        model.tile_score(
            14,
            Forecast {
                tide: TideLevel::High,
                weather: Weather::Clear
            }
        ) > 0
    );
    model.state.board.fill(Tile::Sea);
    model.state.board[14] = Tile::Hamlet;
    model.state.board[13] = Tile::Grove;
    model.state.board[15] = Tile::Grove;
    assert_eq!(model.tile_score(14, Forecast::default()), 5);
}

#[test]
fn placement_harvest_and_undo_are_atomic() {
    let mut model = TideModel::new(9);
    let initial = model.state;
    let (index, choice) = best_move(&model);
    model.place(index, choice);
    assert_eq!(model.turn(), 1);
    model.undo();
    assert_eq!(model.state, initial);
    for _ in 0..6 {
        let (index, choice) = best_move(&model);
        model.place(index, choice);
    }
    assert_eq!(model.harvest_len(), 1);
    assert!(model.score() > 0);
}

#[test]
fn full_campaign_completes_without_unbounded_state() {
    let mut model = TideModel::new(12);
    for island in 0..ISLAND_COUNT {
        solve_island(&mut model);
        assert_eq!(model.turn(), 24);
        assert_eq!(model.harvest_len(), 4);
        let perk = (island < 3).then(|| model.perk_offer(0));
        model.continue_voyage(perk);
    }
    assert!(model.complete());
    assert_eq!(model.state.result_len, 4);
    assert_eq!(model.cumulative_score(), model.total());
    assert_eq!(
        model.total(),
        model
            .state
            .results
            .iter()
            .map(|result| result.score)
            .sum::<u16>()
    );
    assert!(core::mem::size_of::<TideModel>() <= 8 * 1024);
}

#[test]
fn reroll_and_history_are_bounded() {
    let mut model = TideModel::new(15);
    for _ in 0..3 {
        assert!(model.reroll().contains(ChangeSet::MODEL));
    }
    assert!(!model.reroll().contains(ChangeSet::MODEL));
    while model.history_len() < HISTORY_CAPACITY as u8 {
        let (index, choice) = best_move(&model);
        model.place(index, choice);
    }
    assert_eq!(model.history_len(), HISTORY_CAPACITY as u8);
}
