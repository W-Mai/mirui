use super::rules::{initial, moved, won};
use super::solver::solve;
use super::types::{TwinMessage, TwinState};
use crate::gallery::play::change::ChangeSet;
#[cfg(any(feature = "persistence", test))]
use crate::gallery::play::expeditions::{
    ACTION_BYTES, ExpeditionSaveError, RECORD_BYTES, finish_packet, read_records, validate_packet,
    write_records,
};
use crate::gallery::play::expeditions::{
    Direction4, EXPEDITION_LEVELS, ExpeditionHintWorkspace, ExpeditionModal, LevelRecord,
    PackedDirections, TwinLevelRef, accepted_change, movement_stars, twin_level, unlocked_level,
};

#[cfg(any(feature = "persistence", test))]
const SAVE_MAGIC: [u8; 4] = *b"TWB1";
#[cfg(any(feature = "persistence", test))]
const SAVE_VERSION: u8 = 1;
#[cfg(any(feature = "persistence", test))]
const SAVE_PAYLOAD: usize = 10 + RECORD_BYTES + ACTION_BYTES;
#[cfg(any(feature = "persistence", test))]
pub(crate) const SAVE_LEN: usize = SAVE_PAYLOAD + 4;

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TwinModel {
    pub(super) level: u8,
    pub(super) state: TwinState,
    pub(super) actions: PackedDirections,
    pub(super) records: [LevelRecord; EXPEDITION_LEVELS],
    pub(super) hints: u16,
    pub(super) last_hint: Option<(u8, u16, bool)>,
    pub(super) modal: ExpeditionModal,
    pub(super) message: TwinMessage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TwinProgress {
    stars: [u8; EXPEDITION_LEVELS],
}

impl TwinProgress {
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

#[crate::model]
impl TwinModel {
    #[observe]
    pub(crate) fn level_index(&self) -> u8 {
        self.level
    }

    #[model(local)]
    pub(crate) fn level(&self) -> TwinLevelRef {
        twin_level(self.level).expect("validated Twin level")
    }

    #[model(local)]
    pub(crate) fn position(&self, station: usize) -> u8 {
        if station == 0 {
            self.state.a
        } else {
            self.state.b
        }
    }

    #[model(local)]
    pub(crate) fn has_key(&self) -> bool {
        self.state.key
    }

    #[observe]
    pub(crate) fn steps(&self) -> u16 {
        self.actions.len()
    }

    #[observe]
    pub(crate) fn hints(&self) -> u16 {
        self.hints
    }

    #[observe]
    pub(crate) fn modal(&self) -> ExpeditionModal {
        self.modal
    }

    #[observe]
    pub(crate) fn message(&self) -> TwinMessage {
        self.message
    }

    #[observe]
    pub(crate) fn unlocked(&self) -> u8 {
        unlocked_level(&self.records)
    }

    #[observe]
    pub(crate) fn progress(&self) -> TwinProgress {
        TwinProgress {
            stars: core::array::from_fn(|index| self.records[index].stars()),
        }
    }

    #[observe]
    pub(crate) fn par(&self) -> u8 {
        self.level().par()
    }

    #[observe]
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

    #[model(local)]
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
    pub(crate) fn restore(&mut self, restored: TwinModel) -> ChangeSet {
        if *self == restored {
            return ChangeSet::NONE;
        }
        *self = restored;
        accepted_change()
    }

    #[cfg(feature = "persistence")]
    #[model(local)]
    pub(crate) fn encode_vec(&self) -> alloc::vec::Vec<u8> {
        let mut output = alloc::vec![0; SAVE_LEN];
        self.encode_into(&mut output)
            .expect("exact Twin save buffer");
        output
    }
}
