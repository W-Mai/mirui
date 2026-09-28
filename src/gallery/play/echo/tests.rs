use super::types::{HISTORY_CAPACITY, ROOM_COUNT};
use super::*;
use crate::gallery::play::change::ChangeSet;

fn path_to(model: &EchoModel, target: u8) -> [Option<Direction>; BEAT_LIMIT as usize] {
    #[derive(Clone, Copy)]
    struct Node {
        cell: u8,
        beat: u8,
        parent: i16,
        direction: Direction,
    }
    const CAPACITY: usize = CELL_COUNT * (BEAT_LIMIT as usize + 1);
    let empty = Node {
        cell: 0,
        beat: 0,
        parent: -1,
        direction: Direction::Wait,
    };
    let mut nodes = [empty; CAPACITY];
    let mut seen = [false; CAPACITY];
    let mut len = 1usize;
    let mut cursor = 0usize;
    nodes[0].cell = model.position();
    nodes[0].beat = model.tick();
    seen[usize::from(model.tick()) * CELL_COUNT + usize::from(model.position())] = true;
    let mut found = None;
    while cursor < len {
        let node = nodes[cursor];
        if node.cell == target {
            found = Some(cursor);
            break;
        }
        if node.beat < BEAT_LIMIT {
            for direction in [
                Direction::Up,
                Direction::Right,
                Direction::Down,
                Direction::Left,
                Direction::Wait,
            ] {
                let (dx, dy) = direction.delta();
                let x = (node.cell % BOARD_WIDTH as u8) as i8 + dx;
                let y = (node.cell / BOARD_WIDTH as u8) as i8 + dy;
                if x < 0 || x >= BOARD_WIDTH as i8 || y < 0 || y >= BOARD_HEIGHT as i8 {
                    continue;
                }
                let next = (y as usize * BOARD_WIDTH + x as usize) as u8;
                let beat = node.beat + 1;
                if direction != Direction::Wait && !model.can_enter(next, beat, node.cell) {
                    continue;
                }
                let key = usize::from(beat) * CELL_COUNT + usize::from(next);
                if seen[key] {
                    continue;
                }
                seen[key] = true;
                nodes[len] = Node {
                    cell: next,
                    beat,
                    parent: cursor as i16,
                    direction,
                };
                if next == target {
                    found = Some(len);
                    len += 1;
                    break;
                }
                len += 1;
            }
        }
        if found.is_some() {
            break;
        }
        cursor += 1;
    }
    let mut result = [None; BEAT_LIMIT as usize];
    let Some(mut index) = found else {
        return result;
    };
    let mut count = 0usize;
    while nodes[index].parent >= 0 {
        result[count] = Some(nodes[index].direction);
        count += 1;
        index = nodes[index].parent as usize;
    }
    result[..count].reverse();
    result
}

fn walk_to(model: &mut EchoModel, target: u8) {
    for direction in path_to(model, target).into_iter().flatten() {
        assert_ne!(model.step(direction), ChangeSet::NONE);
    }
    assert_eq!(model.position(), target);
}

fn solve_room(model: &mut EchoModel) {
    for gate in 0..usize::from(model.room().gate_count) {
        let plate = model.room().plates[gate];
        walk_to(model, plate);
        assert_ne!(model.rewind(), ChangeSet::NONE);
    }
    for gem in 0..usize::from(model.room().gem_count) {
        let cell = model.room().gems[gem];
        if !model.collected(gem) {
            walk_to(model, cell);
        }
    }
    let end = model.room().end;
    walk_to(model, end);
}

#[test]
fn generator_is_deterministic_and_progresses_gate_count() {
    assert_eq!(generate_room(89, 7), generate_room(89, 7));
    assert_ne!(generate_room(89, 7), generate_room(90, 7));
    assert_eq!(generate_room(9, 0).gate_count, 1);
    assert_eq!(generate_room(9, 2).gate_count, 2);
    assert_eq!(generate_room(9, 6).gate_count, 3);
}

#[test]
fn plates_stay_two_rows_from_their_doors() {
    for seed in 1..=100 {
        let room = generate_room(seed, 8);
        for gate in 0..usize::from(room.gate_count) {
            let door_y = room.doors[gate] as usize / BOARD_WIDTH;
            let plate_y = room.plates[gate] as usize / BOARD_WIDTH;
            assert!(door_y.abs_diff(plate_y) >= 2);
        }
    }
}

#[test]
fn movement_waiting_and_closed_doors_follow_the_next_beat() {
    let mut model = EchoModel::new(9);
    let start = model.position();
    assert_ne!(model.step(Direction::Wait), ChangeSet::NONE);
    assert_eq!(model.position(), start);
    assert_eq!(model.tick(), 1);
    assert_eq!(model.route_len(), 1);
    assert!(!model.can_enter(model.room().doors[0], 1, model.position()));
}

#[test]
fn rewind_records_route_holds_last_cell_and_caps_at_three() {
    let mut model = EchoModel::new(9);
    for ghost in 0..MAX_GHOSTS {
        assert_ne!(model.step(Direction::Wait), ChangeSet::NONE);
        assert_ne!(model.rewind(), ChangeSet::NONE);
        assert_eq!(model.ghost_position(ghost, BEAT_LIMIT), model.room().start);
    }
    assert_ne!(model.step(Direction::Wait), ChangeSet::NONE);
    assert_eq!(model.rewind(), ChangeSet::NONE);
}

#[test]
fn fragments_survive_rewind_and_restart() {
    let mut model = EchoModel::new(9);
    let gem = model.room().gems[0];
    walk_to(&mut model, gem);
    assert_eq!(model.collected_count(), 1);
    assert_ne!(model.rewind(), ChangeSet::NONE);
    assert_eq!(model.collected_count(), 1);
    assert_ne!(model.step(Direction::Wait), ChangeSet::NONE);
    assert_ne!(model.restart(), ChangeSet::NONE);
    assert_eq!(model.collected_count(), 1);
}

#[test]
fn undo_restores_complete_dynamic_state() {
    let mut model = EchoModel::new(9);
    let plate = model.room().plates[0];
    walk_to(&mut model, plate);
    let before = model.state;
    assert_ne!(model.rewind(), ChangeSet::NONE);
    assert_ne!(model.undo(), ChangeSet::NONE);
    assert_eq!(model.state, before);
}

#[test]
fn clear_preserves_geometry_and_resets_room_progress() {
    let mut model = EchoModel::new(9);
    let room = *model.room();
    assert_ne!(model.step(Direction::Wait), ChangeSet::NONE);
    assert_ne!(model.rewind(), ChangeSet::NONE);
    assert_ne!(model.clear_room(), ChangeSet::NONE);
    assert_eq!(*model.room(), room);
    assert_eq!(model.ghost_count(), 0);
    assert_eq!(model.steps(), 0);
}

#[test]
fn clock_gate_cycles_two_open_then_two_closed() {
    let mut model = EchoModel::new(9);
    model.room.clocks[0] = ClockGate { cell: 17, phase: 0 };
    model.room.clock_count = 1;
    assert!(model.clock_open(17, 0));
    assert!(model.clock_open(17, 1));
    assert!(!model.clock_open(17, 2));
    assert!(!model.clock_open(17, 3));
    assert!(model.clock_open(17, 4));
}

#[test]
fn campaign_requires_exit_and_every_fragment() {
    let mut model = EchoModel::new(9);
    solve_room(&mut model);
    assert!(model.won());
    assert_eq!(model.position(), model.room().end);
    assert_eq!(model.collected_count(), model.room().gem_count);
    assert_eq!(model.step(Direction::Wait), ChangeSet::NONE);
    assert_ne!(model.continue_archive(), ChangeSet::NONE);
    assert_eq!(model.level(), 1);
    assert_eq!(model.ghost_count(), 0);
}

#[test]
fn all_twelve_rooms_can_be_completed() {
    let mut model = EchoModel::new(2718);
    for room in 0..ROOM_COUNT {
        solve_room(&mut model);
        assert!(model.won(), "room {room}");
        assert_ne!(model.continue_archive(), ChangeSet::NONE);
    }
    assert!(model.complete());
    assert_eq!(model.result_len(), ROOM_COUNT);
}

#[test]
fn history_and_model_memory_are_bounded() {
    let mut model = EchoModel::new(9);
    for _ in 0..80 {
        let _ = model.step(Direction::Wait);
        let _ = model.restart();
    }
    assert_eq!(model.history_len(), HISTORY_CAPACITY as u8);
    #[cfg(not(feature = "persistence"))]
    assert!(core::mem::size_of::<EchoModel>() <= 8 * 1024);
    #[cfg(feature = "persistence")]
    assert!(core::mem::size_of::<EchoModel>() <= 16 * 1024);
}

#[cfg(feature = "persistence")]
#[test]
fn full_replay_rejects_before_mutating_the_model() {
    use crate::gallery::play::storage::ECHO_REPLAY_CAPACITY;

    let mut model = EchoModel::new(9);
    for _ in 0..ECHO_REPLAY_CAPACITY / 2 {
        assert!(model.step(Direction::Wait).contains(ChangeSet::PERSISTENCE));
        assert!(model.undo().contains(ChangeSet::PERSISTENCE));
    }
    let state = model.state;
    let history_len = model.history_len();
    assert_eq!(usize::from(model.replay_len()), ECHO_REPLAY_CAPACITY);
    assert_eq!(model.step(Direction::Wait), ChangeSet::NONE);
    assert_eq!(model.state, state);
    assert_eq!(model.history_len(), history_len);
}
