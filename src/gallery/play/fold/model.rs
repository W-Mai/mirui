use super::rules::{initial, moved, occupied, won};
use super::solver::{solve, state_id};
use super::types::{FoldMessage, FoldState, HullPose};
use crate::gallery::play::change::ChangeSet;
#[cfg(any(feature = "persistence", test))]
use crate::gallery::play::expeditions::{
    ACTION_BYTES, ExpeditionSaveError, RECORD_BYTES, finish_packet, read_records, validate_packet,
    write_records,
};
use crate::gallery::play::expeditions::{
    Direction4, EXPEDITION_LEVELS, ExpeditionHintWorkspace, ExpeditionModal, FoldLevelRef,
    LevelRecord, PackedDirections, accepted_change, fold_level, movement_stars, unlocked_level,
};

#[cfg(any(feature = "persistence", test))]
const SAVE_MAGIC: [u8; 4] = *b"FLD1";
#[cfg(any(feature = "persistence", test))]
const SAVE_VERSION: u8 = 1;
#[cfg(any(feature = "persistence", test))]
const SAVE_PAYLOAD: usize = 10 + RECORD_BYTES + ACTION_BYTES;
#[cfg(any(feature = "persistence", test))]
pub(crate) const SAVE_LEN: usize = SAVE_PAYLOAD + 4;

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
#[derive(Clone, Copy)]
pub(crate) struct FoldModel {
    pub(super) level: u8,
    pub(super) state: FoldState,
    pub(super) actions: PackedDirections,
    records: [LevelRecord; EXPEDITION_LEVELS],
    pub(super) hints: u16,
    last_hint: Option<u16>,
    modal: ExpeditionModal,
    message: FoldMessage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FoldProgress {
    stars: [u8; EXPEDITION_LEVELS],
}

impl FoldProgress {
    pub(crate) const fn stars(self, level: u8) -> u8 {
        self.stars[level as usize]
    }

    pub(crate) const fn completed(self, level: u8) -> bool {
        self.stars(level) != 0
    }

    pub(crate) fn completed_count(self) -> u8 {
        self.stars.iter().filter(|stars| **stars != 0).count() as u8
    }
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

#[crate::model]
impl FoldModel {
    #[observe]
    pub(crate) fn level_index(&self) -> u8 {
        self.level
    }

    #[model(local)]
    pub(crate) fn level(&self) -> FoldLevelRef {
        fold_level(self.level).expect("validated Fold level")
    }

    #[observe]
    pub(crate) fn pose(&self) -> HullPose {
        self.state.pose
    }

    #[model(local)]
    pub(crate) fn bridge_on(&self) -> bool {
        self.state.bridge
    }

    #[model(local)]
    pub(crate) fn seal_bits(&self) -> u8 {
        self.state.seals
    }

    #[observe]
    pub(crate) fn steps(&self) -> u16 {
        self.actions.len()
    }

    #[observe]
    pub(crate) fn modal(&self) -> ExpeditionModal {
        self.modal
    }

    #[observe]
    pub(crate) fn message(&self) -> FoldMessage {
        self.message
    }

    #[observe]
    pub(crate) fn unlocked(&self) -> u8 {
        unlocked_level(&self.records)
    }

    #[observe]
    pub(crate) fn progress(&self) -> FoldProgress {
        FoldProgress {
            stars: self.records.map(LevelRecord::stars),
        }
    }

    #[model(local)]
    pub(crate) fn occupied(&self) -> ([u8; 2], u8) {
        occupied(self.state)
    }

    #[model(local)]
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

    #[model(local)]
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
    #[model(local)]
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
    #[model(local)]
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
    #[model(local)]
    pub(crate) fn encode_vec(&self) -> alloc::vec::Vec<u8> {
        let mut output = alloc::vec![0; SAVE_LEN];
        self.encode_into(&mut output)
            .expect("exact Fold save buffer");
        output
    }

    #[cfg(feature = "persistence")]
    pub(crate) fn restore(&mut self, restored: FoldModel) -> ChangeSet {
        *self = restored;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }
}
