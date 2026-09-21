use crate::gallery::play::change::ChangeSet;
#[cfg(any(feature = "persistence", test))]
use crate::gallery::play::expeditions::{
    ACTION_BYTES, ExpeditionSaveError, RECORD_BYTES, finish_packet, read_records, validate_packet,
    write_records,
};
use crate::gallery::play::expeditions::{
    Direction4, EXPEDITION_LEVELS, ExpeditionHintWorkspace, ExpeditionModal, FoldCell,
    FoldLevelRef, LevelRecord, PackedDirections, accepted_change, fold_level, movement_stars,
    unlocked_level,
};

#[cfg(any(feature = "persistence", test))]
const SAVE_MAGIC: [u8; 4] = *b"FLD1";
#[cfg(any(feature = "persistence", test))]
const SAVE_VERSION: u8 = 1;
#[cfg(any(feature = "persistence", test))]
const SAVE_PAYLOAD: usize = 10 + RECORD_BYTES + ACTION_BYTES;
#[cfg(any(feature = "persistence", test))]
pub(crate) const SAVE_LEN: usize = SAVE_PAYLOAD + 4;

pub(crate) const CHAPTER_NAMES: [&str; 6] = [
    "AWAKEN HULL",
    "THIN ICE",
    "LOST SEALS",
    "BRIDGE SWITCH",
    "CROSSED REEFS",
    "FINAL HARBOR",
];

pub(crate) const CHAPTER_MECHANICS: [&str; 6] = [
    "直立占一格，平躺占两格",
    "裂纹地板不能直立承重",
    "经过全部徽记后才可归港",
    "直立压开关，切换桥梁",
    "薄冰、徽记与桥梁交错",
    "全部机制组合 · 规划折返",
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum HullPose {
    #[default]
    Upright,
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FoldMessage {
    Ready,
    Unsupported,
    Seal,
    Bridge(bool),
    Undone,
    Hint(Direction4, u16),
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FoldState {
    x: u8,
    y: u8,
    pose: HullPose,
    bridge: bool,
    seals: u8,
}

#[derive(Clone, Copy)]
pub(crate) struct FoldModel {
    level: u8,
    state: FoldState,
    actions: PackedDirections,
    records: [LevelRecord; EXPEDITION_LEVELS],
    hints: u16,
    last_hint: Option<u16>,
    modal: ExpeditionModal,
    message: FoldMessage,
}

impl Default for FoldModel {
    fn default() -> Self {
        let level = fold_level(0).expect("first Fold level");
        Self {
            level: 0,
            state: initial(level),
            actions: PackedDirections::default(),
            records: [LevelRecord::default(); EXPEDITION_LEVELS],
            hints: 0,
            last_hint: None,
            modal: ExpeditionModal::None,
            message: FoldMessage::Ready,
        }
    }
}

impl FoldModel {
    pub(crate) const fn level_index(&self) -> u8 {
        self.level
    }

    pub(crate) fn level(&self) -> FoldLevelRef {
        fold_level(self.level).expect("validated Fold level")
    }

    pub(crate) const fn pose(&self) -> HullPose {
        self.state.pose
    }

    pub(crate) const fn bridge_on(&self) -> bool {
        self.state.bridge
    }

    pub(crate) const fn seal_bits(&self) -> u8 {
        self.state.seals
    }

    pub(crate) const fn steps(&self) -> u16 {
        self.actions.len()
    }

    pub(crate) const fn modal(&self) -> ExpeditionModal {
        self.modal
    }

    pub(crate) const fn message(&self) -> FoldMessage {
        self.message
    }

    pub(crate) const fn unlocked(&self) -> u8 {
        unlocked_level(&self.records)
    }

    pub(crate) const fn record(&self, level: u8) -> LevelRecord {
        self.records[level as usize]
    }

    pub(crate) const fn occupied(&self) -> ([u8; 2], u8) {
        occupied(self.state)
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
            self.message = FoldMessage::Unsupported;
            return ChangeSet::MODEL | ChangeSet::VISUAL;
        };
        let seal_changed = next.seals != self.state.seals;
        let bridge_changed = next.bridge != self.state.bridge;
        self.state = next;
        let pushed = self.actions.push(direction);
        debug_assert!(pushed);
        self.last_hint = None;
        self.message = if seal_changed {
            FoldMessage::Seal
        } else if bridge_changed {
            FoldMessage::Bridge(next.bridge)
        } else {
            FoldMessage::Ready
        };
        if self.won() {
            let stars = movement_stars(self.hints, self.steps(), level.par());
            self.records[usize::from(self.level)].submit(self.steps(), stars);
            self.modal = if self.level as usize + 1 == EXPEDITION_LEVELS {
                ExpeditionModal::Final
            } else {
                ExpeditionModal::Result
            };
            self.message = FoldMessage::Complete;
        }
        accepted_change()
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.modal != ExpeditionModal::None || self.actions.pop().is_none() {
            return ChangeSet::NONE;
        }
        self.replay();
        self.last_hint = None;
        self.message = FoldMessage::Undone;
        accepted_change()
    }

    pub(crate) fn restart(&mut self) -> ChangeSet {
        self.actions.clear();
        self.state = initial(self.level());
        self.hints = 0;
        self.last_hint = None;
        self.modal = ExpeditionModal::None;
        self.message = FoldMessage::Ready;
        accepted_change()
    }

    pub(crate) fn select_level(&mut self, level: u8) -> ChangeSet {
        if level > self.unlocked() || fold_level(level).is_none() {
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
        let signature = state_id(self.state);
        let Some((direction, remaining)) = solve(self.level(), self.state, workspace) else {
            return ChangeSet::NONE;
        };
        if self.last_hint != Some(signature) {
            self.hints = self.hints.saturating_add(1);
            self.last_hint = Some(signature);
        }
        self.message = FoldMessage::Hint(direction, remaining);
        accepted_change()
    }

    fn replay(&mut self) {
        let level = self.level();
        let mut state = initial(level);
        let mut index = 0;
        while let Some(direction) = self.actions.get(index) {
            state = moved(level, state, direction).expect("stored Fold action remains valid");
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
        let level = fold_level(level_index).ok_or(ExpeditionSaveError::InvalidLevel)?;
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
                FoldMessage::Ready
            } else {
                FoldMessage::Complete
            },
        })
    }

    #[cfg(feature = "persistence")]
    pub(crate) fn encode_vec(&self) -> alloc::vec::Vec<u8> {
        let mut output = alloc::vec![0; SAVE_LEN];
        self.encode_into(&mut output)
            .expect("exact Fold save buffer");
        output
    }
}

fn initial(level: FoldLevelRef) -> FoldState {
    FoldState {
        x: level.start() % 10,
        y: level.start() / 10,
        pose: HullPose::Upright,
        bridge: false,
        seals: 0,
    }
}

const fn occupied(state: FoldState) -> ([u8; 2], u8) {
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

fn moved(level: FoldLevelRef, state: FoldState, direction: Direction4) -> Option<FoldState> {
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

fn won(level: FoldLevelRef, state: FoldState) -> bool {
    state.pose == HullPose::Upright
        && state.y * 10 + state.x == level.goal()
        && state.seals == (1 << level.seal_count()) - 1
}

const fn state_id(state: FoldState) -> u16 {
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

fn solve(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_reference_solutions_complete_at_par() {
        for index in 0..EXPEDITION_LEVELS as u8 {
            let level = fold_level(index).unwrap();
            let mut state = initial(level);
            for step in 0..level.solution_len() {
                state = moved(level, state, level.solution(step).unwrap()).unwrap();
            }
            assert!(won(level, state), "Fold level {index}");
            assert_eq!(level.solution_len(), level.par());
        }
    }

    #[test]
    fn exact_hint_solves_every_initial_state() {
        let mut workspace = ExpeditionHintWorkspace::new();
        for index in 0..EXPEDITION_LEVELS as u8 {
            let level = fold_level(index).unwrap();
            let (_, remaining) = solve(level, initial(level), &mut workspace).unwrap();
            assert_eq!(remaining, u16::from(level.par()), "Fold level {index}");
        }
    }

    #[test]
    fn invalid_moves_do_not_enter_history() {
        let mut model = FoldModel::default();
        let steps = model.steps();
        model.move_direction(Direction4::Left);
        assert_eq!(model.steps(), steps);
    }

    #[test]
    fn model_memory_is_bounded() {
        assert!(core::mem::size_of::<FoldModel>() <= 768);
    }

    #[test]
    fn save_round_trip_is_atomic_and_checksummed() {
        let mut model = FoldModel::default();
        model.move_direction(model.level().solution(0).unwrap());
        model.request_hint(&mut ExpeditionHintWorkspace::new());
        let mut bytes = [0; SAVE_LEN];
        model.encode_into(&mut bytes).unwrap();
        let restored = FoldModel::decode(&bytes).unwrap();
        assert_eq!(restored.level, model.level);
        assert_eq!(restored.state, model.state);
        assert_eq!(restored.actions.len(), model.actions.len());
        assert_eq!(restored.hints, model.hints);
        bytes[20] ^= 1;
        assert!(matches!(
            FoldModel::decode(&bytes),
            Err(ExpeditionSaveError::InvalidChecksum)
        ));
    }
}
