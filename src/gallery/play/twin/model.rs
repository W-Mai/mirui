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

#[derive(Clone, Copy)]
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
