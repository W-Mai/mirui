use super::rules::{moved, won};
use super::types::{FoldState, HullPose};
use crate::gallery::play::expeditions::{Direction4, ExpeditionHintWorkspace, FoldLevelRef};

pub(super) const fn state_id(state: FoldState) -> u16 {
    let pose = state.pose as u16;
    let bridge = if state.bridge { 1 } else { 0 };
    (((state.y as u16 * 10 + state.x as u16) * 3 + pose) * 2 + bridge) * 4 + state.seals as u16
}

const fn decode_state(id: u16) -> FoldState {
    let seals = (id & 3) as u8;
    let value = id / 4;
    let bridge = value & 1 != 0;
    let value = value / 2;
    let pose = match value % 3 {
        1 => HullPose::Horizontal,
        2 => HullPose::Vertical,
        _ => HullPose::Upright,
    };
    let cell = value / 3;
    FoldState {
        x: (cell % 10) as u8,
        y: (cell / 10) as u8,
        pose,
        bridge,
        seals,
    }
}

pub(super) fn solve(
    level: FoldLevelRef,
    start: FoldState,
    workspace: &mut ExpeditionHintWorkspace,
) -> Option<(Direction4, u16)> {
    debug_assert_eq!(level.solution_len(), level.par());
    debug_assert!(level.solution(0).is_some());
    workspace.clear();
    let start_id = state_id(start);
    workspace.visit(start_id, Direction4::Up);
    workspace.push(start_id);
    let mut depth = 0_u16;
    let mut level_end = workspace.tail();
    while let Some(id) = workspace.pop() {
        let state = decode_state(id);
        for direction in Direction4::ALL {
            let Some(next) = moved(level, state, direction) else {
                continue;
            };
            let next_id = state_id(next);
            let first = if id == start_id {
                direction
            } else {
                workspace.first(id)
            };
            if !workspace.visit(next_id, first) {
                continue;
            }
            if won(level, next) {
                return Some((first, depth + 1));
            }
            if !workspace.push(next_id) {
                return None;
            }
        }
        if workspace.head() == level_end {
            depth += 1;
            level_end = workspace.tail();
        }
    }
    None
}
