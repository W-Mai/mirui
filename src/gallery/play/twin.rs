use crate::gallery::play::change::ChangeSet;
#[cfg(any(feature = "persistence", test))]
use crate::gallery::play::expeditions::{
    ACTION_BYTES, ExpeditionSaveError, RECORD_BYTES, finish_packet, read_records, validate_packet,
    write_records,
};
use crate::gallery::play::expeditions::{
    Direction4, EXPEDITION_LEVELS, ExpeditionHintWorkspace, ExpeditionModal, LevelRecord,
    PackedDirections, TwinCell, TwinLevelRef, accepted_change, movement_stars, twin_level,
    unlocked_level,
};

#[cfg(any(feature = "persistence", test))]
const SAVE_MAGIC: [u8; 4] = *b"TWB1";
#[cfg(any(feature = "persistence", test))]
const SAVE_VERSION: u8 = 1;
#[cfg(any(feature = "persistence", test))]
const SAVE_PAYLOAD: usize = 10 + RECORD_BYTES + ACTION_BYTES;
#[cfg(any(feature = "persistence", test))]
pub(crate) const SAVE_LEN: usize = SAVE_PAYLOAD + 4;

pub(crate) const CHAPTER_NAMES: [&str; 6] = [
    "FIRST RESPONSE",
    "MIRROR ROUTE",
    "REVERSE TIDE",
    "KEY PROTOCOL",
    "RIGHT ANGLE",
    "TWIN FINALE",
];

pub(crate) const CHAPTER_MECHANICS: [&str; 6] = [
    "同向联动 · 撞墙的一位停住",
    "水平镜像 · 上下仍然同向",
    "完全反向 · 借障碍分离路线",
    "收齐钥片 · 打开两站闸门",
    "右侧旋转 90° · 重新理解方向",
    "钥片与变向 · 组合使用所有规则",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TwinMessage {
    Ready,
    Blocked,
    KeyCollected,
    Undone,
    Hint(Direction4, u16),
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TwinState {
    a: u8,
    b: u8,
    key: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct TwinModel {
    level: u8,
    state: TwinState,
    actions: PackedDirections,
    records: [LevelRecord; EXPEDITION_LEVELS],
    hints: u16,
    last_hint: Option<(u8, u16, bool)>,
    modal: ExpeditionModal,
    message: TwinMessage,
}

impl Default for TwinModel {
    fn default() -> Self {
        let level = twin_level(0).expect("first Twin level");
        Self {
            level: 0,
            state: initial(level),
            actions: PackedDirections::default(),
            records: [LevelRecord::default(); EXPEDITION_LEVELS],
            hints: 0,
            last_hint: None,
            modal: ExpeditionModal::None,
            message: TwinMessage::Ready,
        }
    }
}

impl TwinModel {
    pub(crate) const fn level_index(&self) -> u8 {
        self.level
    }

    pub(crate) fn level(&self) -> TwinLevelRef {
        twin_level(self.level).expect("validated Twin level")
    }

    pub(crate) const fn position(&self, station: usize) -> u8 {
        if station == 0 {
            self.state.a
        } else {
            self.state.b
        }
    }

    pub(crate) const fn has_key(&self) -> bool {
        self.state.key
    }

    pub(crate) const fn steps(&self) -> u16 {
        self.actions.len()
    }

    pub(crate) const fn hints(&self) -> u16 {
        self.hints
    }

    pub(crate) const fn modal(&self) -> ExpeditionModal {
        self.modal
    }

    pub(crate) const fn message(&self) -> TwinMessage {
        self.message
    }

    pub(crate) const fn unlocked(&self) -> u8 {
        unlocked_level(&self.records)
    }

    pub(crate) const fn record(&self, level: u8) -> LevelRecord {
        self.records[level as usize]
    }

    pub(crate) fn won(&self) -> bool {
        won(self.level(), self.state)
    }

    pub(crate) fn move_direction(&mut self, direction: Direction4) -> ChangeSet {
        if self.modal != ExpeditionModal::None || self.won() || self.actions.is_full() {
            return ChangeSet::NONE;
        }
        let level = self.level();
        let Some(next) = moved(level, self.state, direction) else {
            self.message = TwinMessage::Blocked;
            return ChangeSet::MODEL | ChangeSet::VISUAL;
        };
        let collected = !self.state.key && next.key;
        self.state = next;
        let pushed = self.actions.push(direction);
        debug_assert!(pushed);
        self.last_hint = None;
        self.message = if collected {
            TwinMessage::KeyCollected
        } else {
            TwinMessage::Ready
        };
        if self.won() {
            let stars = movement_stars(self.hints, self.steps(), level.par());
            self.records[usize::from(self.level)].submit(self.steps(), stars);
            self.modal = if self.level as usize + 1 == EXPEDITION_LEVELS {
                ExpeditionModal::Final
            } else {
                ExpeditionModal::Result
            };
            self.message = TwinMessage::Complete;
        }
        accepted_change()
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.modal != ExpeditionModal::None || self.actions.pop().is_none() {
            return ChangeSet::NONE;
        }
        self.replay();
        self.last_hint = None;
        self.message = TwinMessage::Undone;
        accepted_change()
    }

    pub(crate) fn restart(&mut self) -> ChangeSet {
        self.actions.clear();
        self.state = initial(self.level());
        self.hints = 0;
        self.last_hint = None;
        self.modal = ExpeditionModal::None;
        self.message = TwinMessage::Ready;
        accepted_change()
    }

    pub(crate) fn select_level(&mut self, level: u8) -> ChangeSet {
        if level > self.unlocked() || twin_level(level).is_none() {
            return ChangeSet::NONE;
        }
        self.level = level;
        self.restart()
    }

    pub(crate) fn continue_campaign(&mut self) -> ChangeSet {
        if self.modal != ExpeditionModal::Result || self.level as usize + 1 >= EXPEDITION_LEVELS {
            return ChangeSet::NONE;
        }
        self.level += 1;
        self.restart()
    }

    pub(crate) fn request_hint(&mut self, workspace: &mut ExpeditionHintWorkspace) -> ChangeSet {
        if self.modal != ExpeditionModal::None || self.won() {
            return ChangeSet::NONE;
        }
        let signature = (self.state.a, u16::from(self.state.b), self.state.key);
        let Some((direction, remaining)) = solve(self.level(), self.state, workspace) else {
            return ChangeSet::NONE;
        };
        if self.last_hint != Some(signature) {
            self.hints = self.hints.saturating_add(1);
            self.last_hint = Some(signature);
        }
        self.message = TwinMessage::Hint(direction, remaining);
        accepted_change()
    }

    fn replay(&mut self) {
        let level = self.level();
        let mut state = initial(level);
        let mut index = 0;
        while let Some(direction) = self.actions.get(index) {
            state = moved(level, state, direction).expect("stored Twin action remains valid");
            index += 1;
        }
        self.state = state;
    }

    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn encode_into(&self, output: &mut [u8]) -> Result<usize, ExpeditionSaveError> {
        if output.len() < SAVE_LEN {
            return Err(ExpeditionSaveError::BufferTooSmall);
        }
        output[..4].copy_from_slice(&SAVE_MAGIC);
        output[4] = SAVE_VERSION;
        output[5] = self.level;
        output[6..8].copy_from_slice(&self.actions.len().to_le_bytes());
        output[8..10].copy_from_slice(&self.hints.to_le_bytes());
        write_records(&self.records, &mut output[10..10 + RECORD_BYTES])?;
        output[10 + RECORD_BYTES..SAVE_PAYLOAD].copy_from_slice(self.actions.bytes());
        finish_packet(output, SAVE_PAYLOAD);
        Ok(SAVE_LEN)
    }

    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn decode(input: &[u8]) -> Result<Self, ExpeditionSaveError> {
        validate_packet(input, SAVE_PAYLOAD)?;
        if input[..4] != SAVE_MAGIC {
            return Err(ExpeditionSaveError::InvalidMagic);
        }
        if input[4] != SAVE_VERSION {
            return Err(ExpeditionSaveError::UnsupportedVersion);
        }
        let level_index = input[5];
        let level = twin_level(level_index).ok_or(ExpeditionSaveError::InvalidLevel)?;
        let action_len = u16::from_le_bytes([input[6], input[7]]);
        let hints = u16::from_le_bytes([input[8], input[9]]);
        let records = read_records(&input[10..10 + RECORD_BYTES])?;
        let mut bytes = [0; ACTION_BYTES];
        bytes.copy_from_slice(&input[10 + RECORD_BYTES..SAVE_PAYLOAD]);
        let actions = PackedDirections::from_bytes(bytes, action_len)
            .ok_or(ExpeditionSaveError::InvalidState)?;
        let mut state = initial(level);
        let mut index = 0;
        while let Some(direction) = actions.get(index) {
            state = moved(level, state, direction).ok_or(ExpeditionSaveError::InvalidState)?;
            index += 1;
        }
        let modal = if won(level, state) {
            if level_index as usize + 1 == EXPEDITION_LEVELS {
                ExpeditionModal::Final
            } else {
                ExpeditionModal::Result
            }
        } else {
            ExpeditionModal::None
        };
        Ok(Self {
            level: level_index,
            state,
            actions,
            records,
            hints,
            last_hint: None,
            modal,
            message: if modal == ExpeditionModal::None {
                TwinMessage::Ready
            } else {
                TwinMessage::Complete
            },
        })
    }

    #[cfg(feature = "persistence")]
    pub(crate) fn encode_vec(&self) -> alloc::vec::Vec<u8> {
        let mut output = alloc::vec![0; SAVE_LEN];
        self.encode_into(&mut output)
            .expect("exact Twin save buffer");
        output
    }
}

fn initial(level: TwinLevelRef) -> TwinState {
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

fn moved(level: TwinLevelRef, state: TwinState, direction: Direction4) -> Option<TwinState> {
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

const fn won(level: TwinLevelRef, state: TwinState) -> bool {
    state.a == level.goal(0) && state.b == level.goal(1) && state.key
}

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

fn solve(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_reference_solutions_complete_at_par() {
        for index in 0..EXPEDITION_LEVELS as u8 {
            let level = twin_level(index).unwrap();
            let mut state = initial(level);
            for step in 0..level.solution_len() {
                state = moved(level, state, level.solution(step).unwrap()).unwrap();
            }
            assert!(won(level, state), "Twin level {index}");
            assert_eq!(level.solution_len(), level.par());
        }
    }

    #[test]
    fn key_opens_doors_only_after_the_collecting_move() {
        for index in 0..EXPEDITION_LEVELS as u8 {
            let level = twin_level(index).unwrap();
            if level.key().is_none() {
                continue;
            }
            let mut state = initial(level);
            for step in 0..level.solution_len() {
                let before = state;
                state = moved(level, state, level.solution(step).unwrap()).unwrap();
                if !before.key && state.key {
                    assert_eq!(level.key(), Some(state.a));
                    return;
                }
            }
        }
        panic!("no key was collected");
    }

    #[test]
    fn exact_hint_solves_every_initial_state() {
        let mut workspace = ExpeditionHintWorkspace::new();
        for index in 0..EXPEDITION_LEVELS as u8 {
            let level = twin_level(index).unwrap();
            let (_, remaining) = solve(level, initial(level), &mut workspace).unwrap();
            assert_eq!(remaining, u16::from(level.par()), "Twin level {index}");
        }
    }

    #[test]
    fn undo_replays_without_restoring_hint_usage() {
        let mut model = TwinModel::default();
        let direction = model.level().solution(0).unwrap();
        model.move_direction(direction);
        let before = model.state;
        model.request_hint(&mut ExpeditionHintWorkspace::new());
        assert_eq!(model.hints(), 1);
        model.undo();
        assert_ne!(model.state, before);
        assert_eq!(model.hints(), 1);
    }

    #[test]
    fn model_memory_is_bounded() {
        assert!(core::mem::size_of::<TwinModel>() <= 768);
        assert!(core::mem::size_of::<ExpeditionHintWorkspace>() <= 8_128);
    }

    #[test]
    fn save_round_trip_is_atomic_and_checksummed() {
        let mut model = TwinModel::default();
        model.move_direction(model.level().solution(0).unwrap());
        model.request_hint(&mut ExpeditionHintWorkspace::new());
        let mut bytes = [0; SAVE_LEN];
        model.encode_into(&mut bytes).unwrap();
        let restored = TwinModel::decode(&bytes).unwrap();
        assert_eq!(restored.level, model.level);
        assert_eq!(restored.state, model.state);
        assert_eq!(restored.actions.len(), model.actions.len());
        assert_eq!(restored.hints, model.hints);
        bytes[20] ^= 1;
        assert!(matches!(
            TwinModel::decode(&bytes),
            Err(ExpeditionSaveError::InvalidChecksum)
        ));
    }
}
