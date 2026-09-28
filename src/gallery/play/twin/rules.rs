use super::types::TwinState;
use crate::gallery::play::expeditions::{Direction4, TwinCell, TwinLevelRef};

pub(super) fn initial(level: TwinLevelRef) -> TwinState {
    TwinState {
        a: level.start(0),
        b: level.start(1),
        key: level.key().is_none(),
    }
}

const fn mapped(direction: Direction4, mode: u8) -> Direction4 {
    match mode {
        1 => match direction {
            Direction4::Left => Direction4::Right,
            Direction4::Right => Direction4::Left,
            other => other,
        },
        2 => match direction {
            Direction4::Up => Direction4::Down,
            Direction4::Right => Direction4::Left,
            Direction4::Down => Direction4::Up,
            Direction4::Left => Direction4::Right,
        },
        3 => match direction {
            Direction4::Up => Direction4::Right,
            Direction4::Right => Direction4::Down,
            Direction4::Down => Direction4::Left,
            Direction4::Left => Direction4::Up,
        },
        _ => direction,
    }
}

fn moved_cell(
    level: TwinLevelRef,
    station: usize,
    position: u8,
    direction: Direction4,
    key: bool,
) -> u8 {
    let (dx, dy) = direction.delta();
    let x = (position % 6) as i8 + dx;
    let y = (position / 6) as i8 + dy;
    if x < 0 || y < 0 || x >= 6 || y >= 6 {
        return position;
    }
    let next = (y * 6 + x) as u8;
    match level.cell(station, next) {
        TwinCell::Wall => position,
        TwinCell::Door if !key => position,
        TwinCell::Floor | TwinCell::Door => next,
    }
}

pub(super) fn moved(
    level: TwinLevelRef,
    state: TwinState,
    direction: Direction4,
) -> Option<TwinState> {
    let a = moved_cell(level, 0, state.a, direction, state.key);
    let b = moved_cell(
        level,
        1,
        state.b,
        mapped(direction, level.mode()),
        state.key,
    );
    if a == state.a && b == state.b {
        return None;
    }
    Some(TwinState {
        a,
        b,
        key: state.key || level.key() == Some(a),
    })
}

pub(super) const fn won(level: TwinLevelRef, state: TwinState) -> bool {
    state.a == level.goal(0) && state.b == level.goal(1) && state.key
}
