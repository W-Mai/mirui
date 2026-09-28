use super::types::{FoldState, HullPose};
use crate::gallery::play::expeditions::{Direction4, FoldCell, FoldLevelRef};

pub(super) fn initial(level: FoldLevelRef) -> FoldState {
    FoldState {
        x: level.start() % 10,
        y: level.start() / 10,
        pose: HullPose::Upright,
        bridge: false,
        seals: 0,
    }
}

pub(super) const fn occupied(state: FoldState) -> ([u8; 2], u8) {
    let first = state.y * 10 + state.x;
    match state.pose {
        HullPose::Upright => ([first, 0], 1),
        HullPose::Horizontal => ([first, first + 1], 2),
        HullPose::Vertical => ([first, first + 10], 2),
    }
}

fn valid(level: FoldLevelRef, state: FoldState) -> bool {
    if state.x >= 10
        || state.y >= 7
        || (state.pose == HullPose::Horizontal && state.x >= 9)
        || (state.pose == HullPose::Vertical && state.y >= 6)
    {
        return false;
    }
    let (cells, len) = occupied(state);
    for cell in &cells[..usize::from(len)] {
        match level.cell(*cell) {
            FoldCell::Void => return false,
            FoldCell::Bridge if !state.bridge => return false,
            FoldCell::Fragile if state.pose == HullPose::Upright => return false,
            FoldCell::Solid | FoldCell::Fragile | FoldCell::Bridge | FoldCell::Switch => {}
        }
    }
    true
}

pub(super) fn moved(
    level: FoldLevelRef,
    state: FoldState,
    direction: Direction4,
) -> Option<FoldState> {
    let mut x = i16::from(state.x);
    let mut y = i16::from(state.y);
    let mut pose = state.pose;
    match (state.pose, direction) {
        (HullPose::Upright, Direction4::Up) => {
            y -= 2;
            pose = HullPose::Vertical;
        }
        (HullPose::Upright, Direction4::Right) => {
            x += 1;
            pose = HullPose::Horizontal;
        }
        (HullPose::Upright, Direction4::Down) => {
            y += 1;
            pose = HullPose::Vertical;
        }
        (HullPose::Upright, Direction4::Left) => {
            x -= 2;
            pose = HullPose::Horizontal;
        }
        (HullPose::Horizontal, Direction4::Up) => y -= 1,
        (HullPose::Horizontal, Direction4::Right) => {
            x += 2;
            pose = HullPose::Upright;
        }
        (HullPose::Horizontal, Direction4::Down) => y += 1,
        (HullPose::Horizontal, Direction4::Left) => {
            x -= 1;
            pose = HullPose::Upright;
        }
        (HullPose::Vertical, Direction4::Up) => {
            y -= 1;
            pose = HullPose::Upright;
        }
        (HullPose::Vertical, Direction4::Right) => x += 1,
        (HullPose::Vertical, Direction4::Down) => {
            y += 2;
            pose = HullPose::Upright;
        }
        (HullPose::Vertical, Direction4::Left) => x -= 1,
    }
    if x < 0 || y < 0 || x > u8::MAX as i16 || y > u8::MAX as i16 {
        return None;
    }
    let mut next = FoldState {
        x: x as u8,
        y: y as u8,
        pose,
        bridge: state.bridge,
        seals: state.seals,
    };
    if !valid(level, next) {
        return None;
    }
    let (cells, len) = occupied(next);
    if next.pose == HullPose::Upright && level.cell(cells[0]) == FoldCell::Switch {
        next.bridge = !next.bridge;
        if !valid(level, next) {
            return None;
        }
    }
    for cell in &cells[..usize::from(len)] {
        let mut index = 0;
        while index < usize::from(level.seal_count()) {
            if level.seal(index) == Some(*cell) {
                next.seals |= 1 << index;
            }
            index += 1;
        }
    }
    Some(next)
}

pub(super) fn won(level: FoldLevelRef, state: FoldState) -> bool {
    state.pose == HullPose::Upright
        && state.y * 10 + state.x == level.goal()
        && state.seals == (1 << level.seal_count()) - 1
}
