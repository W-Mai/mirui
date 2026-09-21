use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    EXPEDITION_LEVELS, ExpeditionModal, LevelRecord, PictureLevelRef, accepted_change,
    picture_level, picture_stars, unlocked_level,
};
#[cfg(any(feature = "persistence", test))]
use crate::gallery::play::expeditions::{
    ExpeditionSaveError, RECORD_BYTES, finish_packet, read_records, validate_packet, write_records,
};

pub(crate) const CHAPTER_NAMES: [&str; 6] = [
    "FIRST MARKS",
    "SIGNAL PUZZLES",
    "DISTANT LETTER",
    "MECHANICAL MAP",
    "STAR VOYAGE",
    "HOME PANORAMA",
];
pub(crate) const CHAPTER_MECHANICS: [&str; 6] = [
    "5×5 · 读懂连续段",
    "6×6 · 学会分隔与留白",
    "7×7 · 行列交叉推理",
    "8×8 · 观测点辅助定位",
    "9×9 · 多段线索组合",
    "10×10 · 完整图纸复原",
];
const PACKED_BYTES: usize = 25;
const HISTORY_CAPACITY: usize = 64;
#[cfg(any(feature = "persistence", test))]
const SAVE_MAGIC: [u8; 4] = *b"ATL1";
#[cfg(any(feature = "persistence", test))]
const SAVE_VERSION: u8 = 1;
#[cfg(any(feature = "persistence", test))]
const SAVE_PAYLOAD: usize = 12 + RECORD_BYTES + PACKED_BYTES * (HISTORY_CAPACITY + 1);
#[cfg(any(feature = "persistence", test))]
pub(crate) const SAVE_LEN: usize = SAVE_PAYLOAD + 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum PictureCell {
    #[default]
    Unknown = 0,
    Filled = 1,
    EmptyMark = 2,
}

impl PictureCell {
    const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Filled,
            2 => Self::EmptyMark,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum PictureTool {
    #[default]
    Fill,
    Mark,
}

impl PictureTool {
    const fn cell(self) -> PictureCell {
        match self {
            Self::Fill => PictureCell::Filled,
            Self::Mark => PictureCell::EmptyMark,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PictureMessage {
    Ready,
    Observed,
    Undone,
    Hint(u8),
    Checked(u8),
    Complete,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PackedPicture {
    bytes: [u8; PACKED_BYTES],
}

impl PackedPicture {
    pub(crate) const fn get(self, cell: u8) -> PictureCell {
        let index = cell as usize;
        PictureCell::from_code((self.bytes[index / 4] >> ((index & 3) * 2)) & 3)
    }

    pub(crate) fn set(&mut self, cell: u8, value: PictureCell) {
        let index = usize::from(cell);
        let shift = (index & 3) * 2;
        self.bytes[index / 4] = (self.bytes[index / 4] & !(3 << shift)) | ((value as u8) << shift);
    }

    #[cfg(any(feature = "persistence", test))]
    pub(crate) const fn bytes(&self) -> &[u8; PACKED_BYTES] {
        &self.bytes
    }

    #[cfg(any(feature = "persistence", test))]
    fn from_bytes(bytes: [u8; PACKED_BYTES]) -> Option<Self> {
        for cell in 0..100_usize {
            let code = (bytes[cell / 4] >> ((cell & 3) * 2)) & 3;
            if code > PictureCell::EmptyMark as u8 {
                return None;
            }
        }
        Some(Self { bytes })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PictureModel {
    level: u8,
    cells: PackedPicture,
    stroke_origin: PackedPicture,
    history: [PackedPicture; HISTORY_CAPACITY],
    history_start: u8,
    history_len: u8,
    stroke_last: u8,
    stroke_value: PictureCell,
    stroke_active: bool,
    tool: PictureTool,
    cursor: u8,
    records: [LevelRecord; EXPEDITION_LEVELS],
    hints: u16,
    modal: ExpeditionModal,
    message: PictureMessage,
}

impl Default for PictureModel {
    fn default() -> Self {
        let mut model = Self {
            level: 0,
            cells: PackedPicture::default(),
            stroke_origin: PackedPicture::default(),
            history: [PackedPicture::default(); HISTORY_CAPACITY],
            history_start: 0,
            history_len: 0,
            stroke_last: 0,
            stroke_value: PictureCell::Unknown,
            stroke_active: false,
            tool: PictureTool::Fill,
            cursor: 0,
            records: [LevelRecord::default(); EXPEDITION_LEVELS],
            hints: 0,
            modal: ExpeditionModal::None,
            message: PictureMessage::Ready,
        };
        model.reset_board();
        model
    }
}

impl PictureModel {
    pub(crate) const fn level_index(&self) -> u8 {
        self.level
    }

    pub(crate) fn level(&self) -> PictureLevelRef {
        picture_level(self.level).expect("validated Picture level")
    }

    pub(crate) const fn cell(&self, cell: u8) -> PictureCell {
        self.cells.get(cell)
    }

    pub(crate) const fn tool(&self) -> PictureTool {
        self.tool
    }

    pub(crate) const fn cursor(&self) -> u8 {
        self.cursor
    }

    pub(crate) const fn hints(&self) -> u16 {
        self.hints
    }

    pub(crate) const fn history_len(&self) -> u8 {
        self.history_len
    }

    pub(crate) const fn modal(&self) -> ExpeditionModal {
        self.modal
    }

    pub(crate) const fn message(&self) -> PictureMessage {
        self.message
    }

    pub(crate) const fn unlocked(&self) -> u8 {
        unlocked_level(&self.records)
    }

    pub(crate) const fn record(&self, level: u8) -> LevelRecord {
        self.records[level as usize]
    }

    pub(crate) fn set_tool(&mut self, tool: PictureTool) -> ChangeSet {
        if self.tool == tool || self.modal != ExpeditionModal::None {
            return ChangeSet::NONE;
        }
        self.tool = tool;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn begin_stroke(&mut self, cell: u8) -> ChangeSet {
        let level = self.level();
        if self.modal != ExpeditionModal::None
            || usize::from(cell) >= usize::from(level.size()).pow(2)
            || level.is_given(cell)
        {
            self.message = PictureMessage::Observed;
            return ChangeSet::MODEL | ChangeSet::VISUAL;
        }
        self.stroke_origin = self.cells;
        self.stroke_last = cell;
        let selected = self.tool.cell();
        self.stroke_value = if self.cells.get(cell) == selected {
            PictureCell::Unknown
        } else {
            selected
        };
        self.stroke_active = true;
        self.cells.set(cell, self.stroke_value);
        ChangeSet::VISUAL
    }

    pub(crate) fn continue_stroke(&mut self, cell: u8) -> ChangeSet {
        if !self.stroke_active || cell == self.stroke_last {
            return ChangeSet::NONE;
        }
        let size = self.level().size();
        if usize::from(cell) >= usize::from(size).pow(2) {
            return ChangeSet::NONE;
        }
        let from_x = i16::from(self.stroke_last % size);
        let from_y = i16::from(self.stroke_last / size);
        let to_x = i16::from(cell % size);
        let to_y = i16::from(cell / size);
        let mut x = from_x;
        let mut y = from_y;
        let dx = (to_x - from_x).abs();
        let sx = if from_x < to_x { 1 } else { -1 };
        let dy = -(to_y - from_y).abs();
        let sy = if from_y < to_y { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            let index = (y * i16::from(size) + x) as u8;
            if !self.level().is_given(index) {
                self.cells.set(index, self.stroke_value);
            }
            if x == to_x && y == to_y {
                break;
            }
            let twice = error * 2;
            if twice >= dy {
                error += dy;
                x += sx;
            }
            if twice <= dx {
                error += dx;
                y += sy;
            }
        }
        self.stroke_last = cell;
        ChangeSet::VISUAL
    }

    pub(crate) fn end_stroke(&mut self, cancelled: bool) -> ChangeSet {
        if !self.stroke_active {
            return ChangeSet::NONE;
        }
        self.stroke_active = false;
        if cancelled {
            self.cells = self.stroke_origin;
            return ChangeSet::VISUAL;
        }
        if self.cells == self.stroke_origin {
            return ChangeSet::NONE;
        }
        self.push_history(self.stroke_origin);
        self.finish_action()
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.modal != ExpeditionModal::None || self.history_len == 0 {
            return ChangeSet::NONE;
        }
        self.history_len -= 1;
        let index = (self.history_start + self.history_len) % HISTORY_CAPACITY as u8;
        self.cells = self.history[usize::from(index)];
        self.message = PictureMessage::Undone;
        accepted_change()
    }

    pub(crate) fn reveal_hint(&mut self) -> ChangeSet {
        if self.modal != ExpeditionModal::None || self.complete() {
            return ChangeSet::NONE;
        }
        let level = self.level();
        let cell_count = level.size() * level.size();
        for cell in 0..cell_count {
            let expected = if level.target(cell) {
                PictureCell::Filled
            } else {
                PictureCell::EmptyMark
            };
            if self.cells.get(cell) != expected && !level.is_given(cell) {
                let previous = self.cells;
                self.cells.set(cell, expected);
                self.push_history(previous);
                self.hints = self.hints.saturating_add(1);
                let changes = self.finish_action();
                if self.modal == ExpeditionModal::None {
                    self.message = PictureMessage::Hint(cell);
                }
                return changes;
            }
        }
        ChangeSet::NONE
    }

    pub(crate) fn check(&mut self) -> ChangeSet {
        let level = self.level();
        let mut errors = 0;
        for cell in 0..level.size() * level.size() {
            let value = self.cells.get(cell);
            if (value == PictureCell::Filled && !level.target(cell))
                || (value == PictureCell::EmptyMark && level.target(cell))
            {
                errors += 1;
            }
        }
        self.message = PictureMessage::Checked(errors);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn move_cursor(
        &mut self,
        direction: crate::gallery::play::expeditions::Direction4,
    ) -> ChangeSet {
        let size = self.level().size();
        let x = self.cursor % size;
        let y = self.cursor / size;
        self.cursor = match direction {
            crate::gallery::play::expeditions::Direction4::Up if y > 0 => self.cursor - size,
            crate::gallery::play::expeditions::Direction4::Right if x + 1 < size => self.cursor + 1,
            crate::gallery::play::expeditions::Direction4::Down if y + 1 < size => {
                self.cursor + size
            }
            crate::gallery::play::expeditions::Direction4::Left if x > 0 => self.cursor - 1,
            _ => return ChangeSet::NONE,
        };
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn apply_cursor(&mut self, tool: Option<PictureTool>) -> ChangeSet {
        if let Some(tool) = tool {
            self.tool = tool;
        }
        let cell = self.cursor;
        let changes = self.begin_stroke(cell);
        if !self.stroke_active {
            return changes;
        }
        changes | self.end_stroke(false)
    }

    pub(crate) fn restart(&mut self) -> ChangeSet {
        self.reset_board();
        self.hints = 0;
        self.modal = ExpeditionModal::None;
        self.message = PictureMessage::Ready;
        accepted_change()
    }

    pub(crate) fn select_level(&mut self, level: u8) -> ChangeSet {
        if level > self.unlocked() || picture_level(level).is_none() {
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

    pub(crate) fn complete(&self) -> bool {
        let level = self.level();
        (0..level.size() * level.size())
            .all(|cell| (self.cells.get(cell) == PictureCell::Filled) == level.target(cell))
    }

    pub(crate) fn clues(&self, row: bool, line: u8, output: &mut [u8; 5]) -> usize {
        let level = self.level();
        let size = level.size();
        let mut len = 0;
        let mut run = 0;
        for offset in 0..=size {
            let filled = if offset == size {
                false
            } else {
                let cell = if row {
                    line * size + offset
                } else {
                    offset * size + line
                };
                level.target(cell)
            };
            if filled {
                run += 1;
            } else if run != 0 {
                output[len] = run;
                len += 1;
                run = 0;
            }
        }
        if len == 0 {
            output[0] = 0;
            1
        } else {
            len
        }
    }

    fn finish_action(&mut self) -> ChangeSet {
        if self.complete() {
            let stars = picture_stars(self.hints);
            self.records[usize::from(self.level)].submit(0, stars);
            self.modal = if self.level as usize + 1 == EXPEDITION_LEVELS {
                ExpeditionModal::Final
            } else {
                ExpeditionModal::Result
            };
            self.message = PictureMessage::Complete;
        } else {
            self.message = PictureMessage::Ready;
        }
        accepted_change()
    }

    fn reset_board(&mut self) {
        self.cells = PackedPicture::default();
        let level = self.level();
        for index in 0..usize::from(level.given_count()) {
            let (cell, filled) = level.given(index).expect("given count is validated");
            self.cells.set(
                cell,
                if filled {
                    PictureCell::Filled
                } else {
                    PictureCell::EmptyMark
                },
            );
        }
        self.stroke_origin = self.cells;
        self.history_start = 0;
        self.history_len = 0;
        self.stroke_active = false;
        self.cursor = 0;
    }

    fn push_history(&mut self, picture: PackedPicture) {
        if usize::from(self.history_len) == HISTORY_CAPACITY {
            self.history[self.history_start as usize] = picture;
            self.history_start = (self.history_start + 1) % HISTORY_CAPACITY as u8;
        } else {
            let index = (self.history_start + self.history_len) % HISTORY_CAPACITY as u8;
            self.history[usize::from(index)] = picture;
            self.history_len += 1;
        }
    }

    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn encode_into(&self, output: &mut [u8]) -> Result<usize, ExpeditionSaveError> {
        if output.len() < SAVE_LEN {
            return Err(ExpeditionSaveError::BufferTooSmall);
        }
        output[..4].copy_from_slice(&SAVE_MAGIC);
        output[4] = SAVE_VERSION;
        output[5] = self.level;
        output[6] = self.tool as u8;
        output[7] = self.cursor;
        output[8..10].copy_from_slice(&self.hints.to_le_bytes());
        output[10] = self.history_start;
        output[11] = self.history_len;
        write_records(&self.records, &mut output[12..12 + RECORD_BYTES])?;
        let mut offset = 12 + RECORD_BYTES;
        output[offset..offset + PACKED_BYTES].copy_from_slice(self.cells.bytes());
        offset += PACKED_BYTES;
        for snapshot in &self.history {
            output[offset..offset + PACKED_BYTES].copy_from_slice(snapshot.bytes());
            offset += PACKED_BYTES;
        }
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
        let level = picture_level(level_index).ok_or(ExpeditionSaveError::InvalidLevel)?;
        let tool = match input[6] {
            0 => PictureTool::Fill,
            1 => PictureTool::Mark,
            _ => return Err(ExpeditionSaveError::InvalidState),
        };
        let cursor = input[7];
        if cursor >= level.size() * level.size()
            || usize::from(input[10]) >= HISTORY_CAPACITY
            || usize::from(input[11]) > HISTORY_CAPACITY
        {
            return Err(ExpeditionSaveError::InvalidState);
        }
        let hints = u16::from_le_bytes([input[8], input[9]]);
        let records = read_records(&input[12..12 + RECORD_BYTES])?;
        let mut offset = 12 + RECORD_BYTES;
        let mut current = [0; PACKED_BYTES];
        current.copy_from_slice(&input[offset..offset + PACKED_BYTES]);
        let cells = PackedPicture::from_bytes(current).ok_or(ExpeditionSaveError::InvalidState)?;
        offset += PACKED_BYTES;
        let mut history = [PackedPicture::default(); HISTORY_CAPACITY];
        for snapshot in &mut history {
            let mut bytes = [0; PACKED_BYTES];
            bytes.copy_from_slice(&input[offset..offset + PACKED_BYTES]);
            *snapshot =
                PackedPicture::from_bytes(bytes).ok_or(ExpeditionSaveError::InvalidState)?;
            offset += PACKED_BYTES;
        }
        for index in 0..usize::from(level.given_count()) {
            let (cell, filled) = level
                .given(index)
                .ok_or(ExpeditionSaveError::InvalidState)?;
            let expected = if filled {
                PictureCell::Filled
            } else {
                PictureCell::EmptyMark
            };
            if cells.get(cell) != expected {
                return Err(ExpeditionSaveError::InvalidState);
            }
        }
        let mut model = Self {
            level: level_index,
            cells,
            stroke_origin: cells,
            history,
            history_start: input[10],
            history_len: input[11],
            stroke_last: 0,
            stroke_value: PictureCell::Unknown,
            stroke_active: false,
            tool,
            cursor,
            records,
            hints,
            modal: ExpeditionModal::None,
            message: PictureMessage::Ready,
        };
        if model.complete() {
            model.modal = if level_index as usize + 1 == EXPEDITION_LEVELS {
                ExpeditionModal::Final
            } else {
                ExpeditionModal::Result
            };
            model.message = PictureMessage::Complete;
        }
        Ok(model)
    }

    #[cfg(feature = "persistence")]
    pub(crate) fn encode_vec(&self) -> alloc::vec::Vec<u8> {
        let mut output = alloc::vec![0; SAVE_LEN];
        self.encode_into(&mut output)
            .expect("exact Atlas save buffer");
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_clues_match_every_target() {
        for level_index in 0..EXPEDITION_LEVELS as u8 {
            let mut model = PictureModel::default();
            model.level = level_index;
            model.reset_board();
            for row in 0..model.level().size() {
                let mut clues = [0; 5];
                assert!(model.clues(true, row, &mut clues) > 0);
                assert!(model.clues(false, row, &mut clues) > 0);
            }
        }
    }

    #[test]
    fn cancellation_restores_the_entire_stroke() {
        let mut model = PictureModel::default();
        let before = model.cells;
        model.begin_stroke(0);
        model.continue_stroke(24);
        model.end_stroke(true);
        assert_eq!(model.cells, before);
        assert_eq!(model.history_len(), 0);
    }

    #[test]
    fn completion_ignores_unknown_empty_cells() {
        let mut model = PictureModel::default();
        let level = model.level();
        for cell in 0..level.size() * level.size() {
            if level.target(cell) {
                model.cells.set(cell, PictureCell::Filled);
            }
        }
        assert!(model.complete());
    }

    #[test]
    fn history_is_bounded_to_sixty_four_strokes() {
        let mut model = PictureModel::default();
        for _ in 0..80 {
            model.begin_stroke(0);
            model.end_stroke(false);
        }
        assert_eq!(model.history_len(), HISTORY_CAPACITY as u8);
    }

    #[test]
    fn observed_cells_cannot_be_changed() {
        let mut model = PictureModel::default();
        model.level = 12;
        model.reset_board();
        let (cell, _) = model.level().given(0).unwrap();
        let before = model.cell(cell);
        model.begin_stroke(cell);
        assert_eq!(model.cell(cell), before);
        assert_eq!(model.history_len(), 0);
    }

    #[test]
    fn model_memory_is_bounded() {
        assert!(core::mem::size_of::<PictureModel>() <= 2_048);
    }

    #[test]
    fn save_round_trip_preserves_board_history_and_rejects_corruption() {
        let mut model = PictureModel::default();
        model.begin_stroke(0);
        model.end_stroke(false);
        model.cursor = 3;
        let mut bytes = [0; SAVE_LEN];
        model.encode_into(&mut bytes).unwrap();
        let restored = PictureModel::decode(&bytes).unwrap();
        assert_eq!(restored.level, model.level);
        assert_eq!(restored.cells, model.cells);
        assert_eq!(restored.history_len, model.history_len);
        assert_eq!(restored.cursor, model.cursor);
        bytes[20] ^= 1;
        assert!(matches!(
            PictureModel::decode(&bytes),
            Err(ExpeditionSaveError::InvalidChecksum)
        ));
    }
}
