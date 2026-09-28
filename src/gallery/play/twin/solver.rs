use super::rules::{moved, won};
use super::types::TwinState;
use crate::gallery::play::expeditions::{Direction4, ExpeditionHintWorkspace, TwinLevelRef};

const fn state_id(state: TwinState) -> u16 {
    state.a as u16 + state.b as u16 * 36 + if state.key { 1296 } else { 0 }
}

const fn decode_state(id: u16) -> TwinState {
    TwinState {
        a: (id % 36) as u8,
        b: (id / 36 % 36) as u8,
        key: id >= 1296,
    }
}

pub(super) fn solve(
    level: TwinLevelRef,
    start: TwinState,
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
