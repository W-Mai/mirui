use super::model::{SAVE_LEN, rating};
use super::*;
use crate::gallery::play::change::ChangeSet;
use crate::types::Fixed;
use alloc::vec::Vec;

fn orientations_from_mask(level: &Level, mask: usize) -> [MirrorOrientation; MAX_MIRRORS] {
    let mut orientations = [MirrorOrientation::Slash; MAX_MIRRORS];
    for (index, slot) in orientations
        .iter_mut()
        .enumerate()
        .take(level.mirrors.len())
    {
        if mask & (1 << index) != 0 {
            *slot = MirrorOrientation::Backslash;
        }
    }
    orientations
}

fn solved_orientations(level: &Level) -> [MirrorOrientation; MAX_MIRRORS] {
    let mut orientations = [MirrorOrientation::Slash; MAX_MIRRORS];
    for (slot, mirror) in orientations.iter_mut().zip(level.mirrors) {
        *slot = mirror.solution;
    }
    orientations
}

fn initial_orientations(level: &Level) -> [MirrorOrientation; MAX_MIRRORS] {
    let mut orientations = [MirrorOrientation::Slash; MAX_MIRRORS];
    for (slot, orientation) in orientations.iter_mut().zip(level.initial) {
        *slot = *orientation;
    }
    orientations
}

fn combinations(level: &Level) -> impl Iterator<Item = [MirrorOrientation; MAX_MIRRORS]> + '_ {
    (0..1_usize << level.mirrors.len()).map(|mask| orientations_from_mask(level, mask))
}

#[test]
fn every_level_starts_unsolved_and_reference_solution_reaches_receiver() {
    for level in &LEVELS {
        assert_eq!(level.initial.len(), level.mirrors.len(), "{}", level.name);
        assert!(
            !trace(level, &initial_orientations(level)).solved,
            "{}",
            level.name
        );
        let solved = trace(level, &solved_orientations(level));
        assert!(solved.solved, "{}", level.name);
        assert_eq!(solved.stop, TraceStop::Receiver);
        assert!(solved.points().len() <= MAX_TRACE_POINTS);
    }
}

#[test]
fn every_level_has_exactly_one_solution() {
    for level in &LEVELS {
        let solutions: Vec<_> = combinations(level)
            .filter(|orientations| trace(level, orientations).solved)
            .collect();
        assert_eq!(solutions, [solved_orientations(level)], "{}", level.name);
    }
}

#[test]
fn the_solution_uses_every_mirror_on_the_board() {
    for level in &LEVELS {
        let solved = trace(level, &solved_orientations(level));
        let all = (1_u16 << level.mirrors.len()) - 1;
        assert_eq!(u16::from(solved.hit_mirrors), all, "{}", level.name);
        for fixed in level.fixed {
            assert!(
                solved.points().contains(&fixed.position),
                "{} leaves a fixed mirror unused",
                level.name
            );
        }
    }
}

#[test]
fn every_lamp_rules_out_another_way_to_the_receiver() {
    for level in &LEVELS {
        for lamp in 0..level.lamps.len() {
            let without = level.all_lamps() & !(1 << lamp);
            let reaching = combinations(level)
                .map(|orientations| trace(level, &orientations))
                .filter(|beam| {
                    beam.stop == TraceStop::Receiver && beam.lit_lamps & without == without
                })
                .count();
            assert!(reaching > 1, "{} lamp {lamp} decides nothing", level.name);
        }
    }
}

#[test]
fn hints_name_a_wrong_mirror_on_the_beam_and_lead_to_the_solution() {
    for (index, level) in LEVELS.iter().enumerate() {
        let solution = solved_orientations(level);
        for start in combinations(level) {
            let mut model = LumenModel::new();
            model.select_level(index);
            model.orientations = start;
            model.evaluate();
            let mut hints = 0;
            while !model.solved() {
                model.reveal_hint();
                let Some(hinted) = model.hint() else {
                    panic!("{}: no hint while unsolved", level.name);
                };
                let position = level.mirrors[hinted].position;
                assert!(model.trace().points().contains(&position), "{}", level.name);
                assert_ne!(
                    model.orientations[hinted], solution[hinted],
                    "{}",
                    level.name
                );
                model.rotate(hinted);
                hints += 1;
                assert!(hints <= level.mirrors.len(), "{}", level.name);
            }
            assert_eq!(model.orientations, solution, "{}", level.name);
            model.reveal_hint();
            assert_eq!(model.hint(), None);
        }
    }
}

#[test]
fn all_orientation_combinations_terminate_within_the_budget() {
    for level in &LEVELS {
        for orientations in combinations(level) {
            let result = trace(level, &orientations);
            assert!(result.steps <= 140);
            assert!(result.points().len() <= MAX_TRACE_POINTS);
        }
    }
}

#[test]
fn boards_are_well_formed() {
    let inside = |point: GridPoint| {
        (0..GRID_COLUMNS).contains(&point.x) && (0..GRID_ROWS).contains(&point.y)
    };
    for level in &LEVELS {
        assert!(!level.mirrors.is_empty() && level.mirrors.len() <= MAX_MIRRORS);
        assert!(level.lamps.len() <= 8);
        assert!((0..GRID_ROWS).contains(&level.source_row));
        let mut cells = Vec::new();
        cells.push(level.target);
        cells.extend(level.mirrors.iter().map(|mirror| mirror.position));
        cells.extend(level.fixed.iter().map(|mirror| mirror.position));
        cells.extend_from_slice(level.walls);
        cells.extend_from_slice(level.lamps);
        for (index, cell) in cells.iter().enumerate() {
            assert!(inside(*cell), "{} {cell:?}", level.name);
            assert!(!cells[..index].contains(cell), "{} {cell:?}", level.name);
        }
        let reading = |point: GridPoint| (point.y, point.x);
        assert!(
            level
                .mirrors
                .windows(2)
                .all(|pair| reading(pair[0].position) < reading(pair[1].position)),
            "{} numbers its mirrors out of reading order",
            level.name
        );
    }
}

#[test]
fn each_set_adds_one_idea_and_grows_within_itself() {
    assert_eq!(SET_COUNT * SET_SIZE, LEVEL_COUNT);
    for (set, levels) in LEVELS.chunks(SET_SIZE).enumerate() {
        for phrase in [SETS[set].name, SETS[set].rule] {
            assert!(!phrase.en.is_empty() && !phrase.zh.is_empty());
        }
        for level in levels {
            match set {
                0 => assert!(level.fixed.is_empty() && level.lamps.is_empty()),
                1 => assert!(!level.fixed.is_empty() && level.lamps.is_empty()),
                _ => assert!(!level.lamps.is_empty()),
            }
        }
        assert!(
            levels
                .windows(2)
                .all(|pair| pair[0].mirrors.len() <= pair[1].mirrors.len())
        );
    }
}

#[test]
fn playing_every_level_through_its_mirrors_lights_all_fifteen() {
    let mut model = LumenModel::new();
    for (index, level) in LEVELS.iter().enumerate() {
        model.select_level(index);
        assert_eq!(model.level_index(), index);
        assert!(!model.solved(), "{}", level.name);
        for mirror in level.mirrors {
            let slot = level.mirror_at(mirror.position).unwrap();
            if model.orientations()[slot] != mirror.solution {
                assert_ne!(
                    model.rotate_cell(mirror.position.x, mirror.position.y),
                    ChangeSet::NONE
                );
            }
        }
        assert!(model.solved(), "{}", level.name);
        assert_eq!(model.beam_status(), BeamStatus::Lit);
        assert!(model.is_completed(index));
        assert_eq!(usize::from(model.completed_count()), index + 1);
    }
    assert_eq!(model.completion_mask(), (1 << LEVEL_COUNT) - 1);
    model.next_level();
    assert!(model.levels_open(), "the last puzzle leads to the list");
    assert_eq!(model.level_index(), LEVEL_COUNT - 1);
}

#[test]
fn rotate_and_undo_restore_orientation_and_move_count() {
    let mut model = LumenModel::new();
    let before = model.orientations;
    assert!(model.rotate(1).contains(ChangeSet::MODEL));
    assert_eq!(model.moves, 1);
    assert!(model.trace.solved);
    assert!(model.undo().contains(ChangeSet::MODEL));
    assert_eq!(model.orientations, before);
    assert_eq!(model.moves, 0);
}

#[test]
fn successful_visit_remains_recorded_after_undo() {
    let mut model = LumenModel::new();
    model.rotate(1);
    model.undo();
    assert!(model.is_completed(0));
    assert!(!model.trace.solved);
}

#[test]
fn invalid_mirror_or_level_does_not_mutate() {
    let mut model = LumenModel::new();
    let before = model.clone();
    assert_eq!(model.rotate(88), ChangeSet::NONE);
    assert_eq!(model.select_level(LEVEL_COUNT), ChangeSet::NONE);
    assert_eq!(model.rotate_cell(0, 0), ChangeSet::NONE);
    assert_eq!(model.orientations, before.orientations);
    assert_eq!(model.level_index, before.level_index);
    assert_eq!(model.moves, before.moves);
}

#[test]
fn hint_identifies_mismatch_without_rotating() {
    let mut model = LumenModel::new();
    let before = model.orientations;
    model.reveal_hint();
    assert_eq!(model.hint(), Some(1));
    assert_eq!(model.note(), LumenNote::Hint(1));
    assert_eq!(model.orientations, before);
}

#[test]
fn tapping_a_fixed_mirror_explains_itself_without_a_move() {
    let mut model = LumenModel::new();
    model.select_level(SET_SIZE);
    let fixed = model.level().fixed[0].position;
    let before = model.orientations;
    assert!(
        model
            .rotate_cell(fixed.x, fixed.y)
            .contains(ChangeSet::MODEL)
    );
    assert_eq!(model.note(), LumenNote::Fixed);
    assert_eq!(model.orientations, before);
    assert_eq!(model.moves(), 0);
    assert!(!model.can_undo());
    let mirror = model.level().mirrors[0].position;
    model.rotate_cell(mirror.x, mirror.y);
    assert_eq!(model.note(), LumenNote::Tip);
}

#[test]
fn the_first_lamp_level_opens_on_a_beam_that_reaches_a_dark_receiver() {
    let mut model = LumenModel::new();
    model.select_level(2 * SET_SIZE);
    assert_eq!(model.beam_status(), BeamStatus::LampsDark);
    assert_eq!(model.lamp_count(), (0, 1));
    assert!(!model.is_completed(2 * SET_SIZE));
}

#[test]
fn undo_is_bounded_to_the_latest_thirty_two_actions() {
    let mut model = LumenModel::new();
    for _ in 0..100 {
        model.rotate(0);
    }
    assert_eq!(model.history_len, 32);
    for _ in 0..32 {
        assert_ne!(model.undo(), ChangeSet::NONE);
    }
    assert_eq!(model.undo(), ChangeSet::NONE);
}

#[test]
fn modal_blocks_board_mutation_and_animation() {
    let mut model = LumenModel::new();
    model.open_levels();
    let before = model.orientations;
    assert_eq!(model.rotate_cell(2, 3), ChangeSet::NONE);
    assert_eq!(model.advance_ms(16), ChangeSet::NONE);
    assert_eq!(model.orientations, before);
}

fn settle(model: &mut LumenModel) {
    for _ in 0..200 {
        model.advance_ms(16);
    }
}

#[test]
fn light_flows_only_along_a_beam_that_is_drawn() {
    let mut model = LumenModel::new();
    assert!(
        !model.motion().beam_complete(),
        "a puzzle opens drawing its beam"
    );
    let start = model.scan_phase;
    assert!(model.advance_ms(16).contains(ChangeSet::VISUAL));
    assert_eq!(model.scan_phase, start);
    settle(&mut model);
    assert!(model.motion().beam_complete());
    let drawn = model.scan_phase;
    assert!(model.advance_ms(16).contains(ChangeSet::VISUAL));
    assert_ne!(model.scan_phase, drawn);
    assert_eq!(model.advance_ms(0), ChangeSet::NONE);
}

#[test]
fn par_counts_the_mirrors_that_start_the_wrong_way() {
    for level in &LEVELS {
        let wrong = level
            .mirrors
            .iter()
            .zip(level.initial)
            .filter(|(mirror, start)| mirror.solution != **start)
            .count();
        assert_eq!(usize::from(level.par()), wrong, "{}", level.name);
        assert!(level.par() >= 1, "{}", level.name);
    }
}

/// Turns every wrong mirror of the current puzzle once.
fn solve_at_par(model: &mut LumenModel) {
    let level = model.level();
    for (index, mirror) in level.mirrors.iter().enumerate() {
        if model.orientations()[index] != mirror.solution {
            model.rotate(index);
        }
    }
}

#[test]
fn stars_reward_par_and_count_slips_and_hints() {
    assert_eq!(rating(2, 2, false), 3);
    assert_eq!(rating(4, 2, false), 2);
    assert_eq!(rating(5, 2, false), 1);
    assert_eq!(rating(2, 2, true), 2);
    assert_eq!(rating(9, 2, true), 1);

    let mut model = LumenModel::new();
    solve_at_par(&mut model);
    assert!(model.solved());
    assert_eq!(model.earned(), MAX_STARS);
    assert_eq!(model.stars()[0], 3);

    model.select_level(1);
    model.rotate(0);
    model.rotate(0);
    solve_at_par(&mut model);
    assert_eq!(model.earned(), 2, "one slip, there and back");

    model.reset();
    model.reveal_hint();
    solve_at_par(&mut model);
    assert_eq!(model.earned(), 2, "a hint caps the solve at two");
    assert_eq!(model.stars()[1], 2);

    model.select_level(0);
    for _ in 0..4 {
        model.rotate(0);
    }
    solve_at_par(&mut model);
    assert_eq!(model.earned(), 1);
    assert_eq!(model.stars()[0], 3, "the best solve is kept");
    assert_eq!(model.total_stars(), 5);
    assert_eq!(model.completed_count(), 2);
}

#[test]
fn a_turn_redraws_the_beam_from_the_mirror_that_moved() {
    let mut model = LumenModel::new();
    settle(&mut model);
    let position = model.level().mirrors[1].position;
    model.rotate(1);
    let from = model
        .trace()
        .points()
        .iter()
        .position(|point| *point == position)
        .unwrap();
    assert_eq!(model.motion().front(), Fixed::from_int(from as i32));
    assert_eq!(model.motion().turn_of(1), Some(Fixed::ZERO));
    assert!(!model.won(), "the result waits for the beam");
    settle(&mut model);
    let segments = model.trace().points().len() as i32 - 1;
    assert_eq!(model.motion().front(), Fixed::from_int(segments));
    assert_eq!(model.motion().turn_of(1), None);

    model.select_level(1);
    settle(&mut model);
    let off_beam = (0..model.level().mirrors.len())
        .find(|index| {
            let position = model.level().mirrors[*index].position;
            !model.trace().points().contains(&position)
        })
        .expect("the second puzzle opens with a mirror off the beam");
    let before = *model.trace();
    model.rotate(off_beam);
    assert_eq!(*model.trace(), before);
    assert!(model.motion().beam_complete(), "nothing to redraw");
}

#[test]
fn a_solve_lights_up_then_pops_its_stars_one_by_one() {
    let mut model = LumenModel::new();
    settle(&mut model);
    model.rotate(1);
    assert!(model.solved());
    while !model.motion().beam_complete() {
        model.advance_ms(16);
        assert!(!model.won());
    }
    model.advance_ms(16);
    assert!(model.motion().glow().is_some(), "the receiver glows first");
    let mut first = None;
    let mut elapsed = 0;
    while model.motion().star(2) == Fixed::ZERO {
        model.advance_ms(10);
        elapsed += 10;
        if first.is_none() && model.motion().star(0) > Fixed::ZERO {
            first = Some(elapsed);
            assert!(model.won(), "the card opens with the first star");
            assert_eq!(model.motion().star(1), Fixed::ZERO);
        }
        assert!(elapsed < 1_000);
    }
    assert!(first.unwrap() < elapsed);
    settle(&mut model);
    for star in 0..3 {
        assert_eq!(model.motion().star(star), Fixed::ONE);
    }
}

#[test]
fn the_next_input_cuts_motion_short() {
    let mut model = LumenModel::new();
    model.rotate(1);
    assert_ne!(model.finish_motion(), ChangeSet::NONE);
    assert!(model.won(), "a skipped solve shows its result at once");
    assert_eq!(model.motion().star(0), Fixed::ONE);
    assert_eq!(model.finish_motion(), ChangeSet::NONE);

    model.undo();
    assert!(!model.won());
    assert_eq!(model.motion().turn_of(1), Some(Fixed::ZERO));
}

#[test]
fn a_refused_tap_shakes_the_fixed_mirror_every_time() {
    let mut model = LumenModel::new();
    model.select_level(SET_SIZE);
    let fixed = model.level().fixed[0].position;
    for _ in 0..2 {
        assert!(
            model
                .rotate_cell(fixed.x, fixed.y)
                .contains(ChangeSet::VISUAL)
        );
        model.advance_ms(16);
        assert_ne!(model.motion().shake_of(0), Fixed::ZERO);
        settle(&mut model);
        assert_eq!(model.motion().shake_of(0), Fixed::ZERO);
    }
}

#[test]
fn progress_saves_the_puzzle_in_play_and_every_rating() {
    let mut source = LumenModel::new();
    solve_at_par(&mut source);
    source.select_level(3);
    source.rotate(0);
    let mut bytes = [0_u8; SAVE_LEN];
    assert_eq!(source.encode_into(&mut bytes), Ok(SAVE_LEN));
    assert_eq!(&bytes[..4], b"LUM1");

    let progress = LumenModel::decode_progress(&bytes).unwrap();
    let mut restored = LumenModel::new();
    restored.open_levels();
    restored.restore(progress);
    assert_eq!(restored.level_index(), 3);
    assert_eq!(restored.moves(), 0, "the puzzle opens fresh");
    assert_eq!(restored.stars(), source.stars());
    assert!(!restored.levels_open());

    let mut corrupt = bytes;
    corrupt[6] ^= 1;
    assert!(LumenModel::decode_progress(&corrupt).is_err());
    assert!(LumenModel::decode_progress(&bytes[..SAVE_LEN - 1]).is_err());
}

#[test]
fn model_storage_stays_within_embedded_budget() {
    // The beam trace is the bulk; the board motion (a tween for the beam and
    // one for the receiver, springs for a turn and the three result stars)
    // adds about 250 bytes.
    assert!(core::mem::size_of::<LumenModel>() <= 768);
}
