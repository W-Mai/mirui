use crate::gallery::play::change::ChangeSet;

pub(crate) const EXPEDITION_LEVELS: usize = 36;
pub(crate) const MAX_EXPEDITION_ACTIONS: usize = 2048;
pub(crate) const ACTION_BYTES: usize = MAX_EXPEDITION_ACTIONS / 4;
const HINT_STATES: usize = 2592;
#[cfg(any(feature = "persistence", test))]
pub(crate) const RECORD_BYTES: usize = EXPEDITION_LEVELS * 3;

#[cfg(any(feature = "persistence", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExpeditionSaveError {
    BufferTooSmall,
    InvalidLength,
    InvalidMagic,
    UnsupportedVersion,
    InvalidChecksum,
    InvalidLevel,
    InvalidRecord,
    InvalidState,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Direction4 {
    #[default]
    Up,
    Right,
    Down,
    Left,
}

impl Direction4 {
    pub(crate) const ALL: [Self; 4] = [Self::Up, Self::Right, Self::Down, Self::Left];

    pub(crate) const fn code(self) -> u8 {
        self as u8
    }

    pub(crate) const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Up),
            1 => Some(Self::Right),
            2 => Some(Self::Down),
            3 => Some(Self::Left),
            _ => None,
        }
    }

    pub(crate) const fn delta(self) -> (i8, i8) {
        match self {
            Self::Up => (0, -1),
            Self::Right => (1, 0),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Up => "UP",
            Self::Right => "RIGHT",
            Self::Down => "DOWN",
            Self::Left => "LEFT",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PackedDirections {
    bytes: [u8; ACTION_BYTES],
    len: u16,
}

impl PackedDirections {
    pub(crate) const EMPTY: Self = Self {
        bytes: [0; ACTION_BYTES],
        len: 0,
    };

    pub(crate) const fn len(&self) -> u16 {
        self.len
    }

    #[cfg(test)]
    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) const fn is_full(&self) -> bool {
        self.len as usize == MAX_EXPEDITION_ACTIONS
    }

    pub(crate) fn clear(&mut self) {
        self.bytes = [0; ACTION_BYTES];
        self.len = 0;
    }

    pub(crate) fn push(&mut self, direction: Direction4) -> bool {
        if self.is_full() {
            return false;
        }
        let index = usize::from(self.len);
        let shift = (index & 3) * 2;
        self.bytes[index / 4] |= direction.code() << shift;
        self.len += 1;
        true
    }

    pub(crate) fn pop(&mut self) -> Option<Direction4> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        let index = usize::from(self.len);
        let shift = (index & 3) * 2;
        let direction = Direction4::from_code((self.bytes[index / 4] >> shift) & 3);
        self.bytes[index / 4] &= !(3 << shift);
        direction
    }

    pub(crate) const fn get(&self, index: u16) -> Option<Direction4> {
        if index >= self.len {
            return None;
        }
        let index = index as usize;
        Direction4::from_code((self.bytes[index / 4] >> ((index & 3) * 2)) & 3)
    }

    #[cfg(any(feature = "persistence", test))]
    pub(crate) const fn bytes(&self) -> &[u8; ACTION_BYTES] {
        &self.bytes
    }

    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn from_bytes(bytes: [u8; ACTION_BYTES], len: u16) -> Option<Self> {
        if usize::from(len) > MAX_EXPEDITION_ACTIONS {
            return None;
        }
        let mut value = Self { bytes, len };
        if len & 3 != 0 {
            let index = usize::from(len) / 4;
            value.bytes[index] &= (1 << ((usize::from(len) & 3) * 2)) - 1;
        }
        for byte in &mut value.bytes[usize::from(len).div_ceil(4)..] {
            *byte = 0;
        }
        Some(value)
    }
}

impl Default for PackedDirections {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct LevelRecord {
    best_steps: u16,
    stars: u8,
}

impl LevelRecord {
    pub(crate) const fn completed(self) -> bool {
        self.stars != 0
    }

    #[cfg(any(feature = "persistence", test))]
    pub(crate) const fn best_steps(self) -> u16 {
        self.best_steps
    }

    pub(crate) const fn stars(self) -> u8 {
        self.stars
    }

    pub(crate) fn submit(&mut self, steps: u16, stars: u8) {
        if !self.completed() || steps < self.best_steps {
            self.best_steps = steps;
        }
        self.stars = self.stars.max(stars.min(3));
    }

    #[cfg(any(feature = "persistence", test))]
    const fn from_parts(best_steps: u16, stars: u8) -> Option<Self> {
        if stars > 3 || (stars == 0 && best_steps != 0) {
            None
        } else {
            Some(Self { best_steps, stars })
        }
    }
}

#[cfg(any(feature = "persistence", test))]
pub(crate) fn write_records(
    records: &[LevelRecord; EXPEDITION_LEVELS],
    output: &mut [u8],
) -> Result<(), ExpeditionSaveError> {
    if output.len() < RECORD_BYTES {
        return Err(ExpeditionSaveError::BufferTooSmall);
    }
    for (index, record) in records.iter().copied().enumerate() {
        let offset = index * 3;
        output[offset..offset + 2].copy_from_slice(&record.best_steps().to_le_bytes());
        output[offset + 2] = record.stars();
    }
    Ok(())
}

#[cfg(any(feature = "persistence", test))]
pub(crate) fn read_records(
    input: &[u8],
) -> Result<[LevelRecord; EXPEDITION_LEVELS], ExpeditionSaveError> {
    if input.len() < RECORD_BYTES {
        return Err(ExpeditionSaveError::InvalidLength);
    }
    let mut records = [LevelRecord::default(); EXPEDITION_LEVELS];
    for (index, record) in records.iter_mut().enumerate() {
        let offset = index * 3;
        let steps = u16::from_le_bytes([input[offset], input[offset + 1]]);
        *record = LevelRecord::from_parts(steps, input[offset + 2])
            .ok_or(ExpeditionSaveError::InvalidRecord)?;
    }
    Ok(records)
}

#[cfg(any(feature = "persistence", test))]
pub(crate) fn finish_packet(output: &mut [u8], payload_len: usize) {
    let checksum = checksum(&output[..payload_len]);
    output[payload_len..payload_len + 4].copy_from_slice(&checksum.to_le_bytes());
}

#[cfg(any(feature = "persistence", test))]
pub(crate) fn validate_packet(input: &[u8], payload_len: usize) -> Result<(), ExpeditionSaveError> {
    if input.len() != payload_len + 4 {
        return Err(ExpeditionSaveError::InvalidLength);
    }
    let stored = u32::from_le_bytes(
        input[payload_len..]
            .try_into()
            .map_err(|_| ExpeditionSaveError::InvalidLength)?,
    );
    if checksum(&input[..payload_len]) != stored {
        return Err(ExpeditionSaveError::InvalidChecksum);
    }
    Ok(())
}

#[cfg(any(feature = "persistence", test))]
fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

pub(crate) const fn unlocked_level(records: &[LevelRecord; EXPEDITION_LEVELS]) -> u8 {
    let mut index = 0;
    while index < EXPEDITION_LEVELS && records[index].completed() {
        index += 1;
    }
    if index >= EXPEDITION_LEVELS {
        (EXPEDITION_LEVELS - 1) as u8
    } else {
        index as u8
    }
}

pub(crate) const fn movement_stars(hints: u16, steps: u16, par: u8) -> u8 {
    if hints != 0 {
        1
    } else if steps == par as u16 {
        3
    } else {
        2
    }
}

pub(crate) const fn picture_stars(hints: u16) -> u8 {
    if hints == 0 { 3 } else { 1 }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ExpeditionModal {
    #[default]
    None,
    Result,
    Final,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ExpeditionPanel {
    #[default]
    None,
    Map,
    Rules,
    Briefing,
    Summary,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ExpeditionUiState {
    panel: ExpeditionPanel,
    chapter: u8,
}

impl ExpeditionUiState {
    pub(crate) const fn panel(self) -> ExpeditionPanel {
        self.panel
    }

    pub(crate) const fn chapter(self) -> u8 {
        self.chapter
    }

    pub(crate) fn open(&mut self, level: u8) {
        self.panel = ExpeditionPanel::Map;
        self.chapter = (level / 6).min(5);
    }

    pub(crate) fn open_rules(&mut self) {
        self.panel = ExpeditionPanel::Rules;
    }

    pub(crate) fn open_briefing(&mut self) {
        self.panel = ExpeditionPanel::Briefing;
    }

    pub(crate) fn open_summary(&mut self) {
        self.panel = ExpeditionPanel::Summary;
    }

    pub(crate) fn close(&mut self) {
        self.panel = ExpeditionPanel::None;
    }

    pub(crate) fn select_chapter(&mut self, chapter: u8) {
        self.chapter = chapter.min(5);
    }
}

pub(crate) struct ExpeditionHintWorkspace {
    queue: [u16; HINT_STATES],
    first: [u8; HINT_STATES],
    visited: [u8; HINT_STATES / 8],
    head: u16,
    tail: u16,
}

impl ExpeditionHintWorkspace {
    pub(crate) const fn new() -> Self {
        Self {
            queue: [0; HINT_STATES],
            first: [0; HINT_STATES],
            visited: [0; HINT_STATES / 8],
            head: 0,
            tail: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        self.visited.fill(0);
        self.head = 0;
        self.tail = 0;
    }

    pub(crate) fn visit(&mut self, state: u16, first: Direction4) -> bool {
        let index = usize::from(state);
        debug_assert!(index < HINT_STATES);
        let mask = 1 << (index & 7);
        let byte = &mut self.visited[index / 8];
        if *byte & mask != 0 {
            return false;
        }
        *byte |= mask;
        self.first[index] = first.code();
        true
    }

    pub(crate) fn push(&mut self, state: u16) -> bool {
        if usize::from(self.tail) == self.queue.len() {
            return false;
        }
        self.queue[usize::from(self.tail)] = state;
        self.tail += 1;
        true
    }

    pub(crate) fn pop(&mut self) -> Option<u16> {
        if self.head == self.tail {
            return None;
        }
        let state = self.queue[usize::from(self.head)];
        self.head += 1;
        Some(state)
    }

    pub(crate) const fn tail(&self) -> u16 {
        self.tail
    }

    pub(crate) const fn head(&self) -> u16 {
        self.head
    }

    pub(crate) const fn first(&self, state: u16) -> Direction4 {
        match Direction4::from_code(self.first[state as usize]) {
            Some(direction) => direction,
            None => Direction4::Up,
        }
    }
}

impl Default for ExpeditionHintWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
pub(super) struct TwinLevelData {
    pub(super) mode: u8,
    pub(super) starts: [u8; 2],
    pub(super) goals: [u8; 2],
    pub(super) key: u8,
    pub(super) par: u8,
    pub(super) map_offset: u16,
    pub(super) solution_offset: u16,
    pub(super) solution_len: u8,
}

#[derive(Clone, Copy)]
pub(super) struct FoldLevelData {
    pub(super) start: u8,
    pub(super) goal: u8,
    pub(super) seals: [u8; 2],
    pub(super) seal_count: u8,
    pub(super) par: u8,
    pub(super) map_offset: u16,
    pub(super) solution_offset: u16,
    pub(super) solution_len: u8,
}

#[derive(Clone, Copy)]
pub(super) struct PictureLevelData {
    pub(super) size: u8,
    pub(super) image_offset: u16,
    pub(super) givens: [u8; 2],
    pub(super) given_values: u8,
    pub(super) given_count: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TwinCell {
    Floor,
    Wall,
    Door,
}

#[derive(Clone, Copy)]
pub(crate) struct TwinLevelRef {
    index: u8,
}

impl TwinLevelRef {
    const fn data(self) -> &'static TwinLevelData {
        &super::expeditions_data::TWIN_LEVELS[self.index as usize]
    }

    pub(crate) const fn mode(self) -> u8 {
        self.data().mode
    }

    pub(crate) const fn start(self, station: usize) -> u8 {
        self.data().starts[station]
    }

    pub(crate) const fn goal(self, station: usize) -> u8 {
        self.data().goals[station]
    }

    pub(crate) const fn key(self) -> Option<u8> {
        if self.data().key == u8::MAX {
            None
        } else {
            Some(self.data().key)
        }
    }

    pub(crate) const fn par(self) -> u8 {
        self.data().par
    }

    pub(crate) fn cell(self, station: usize, cell: u8) -> TwinCell {
        let data = self.data();
        let local = station * 36 + usize::from(cell);
        let value = read_bits(
            &super::expeditions_data::TWIN_MAPS,
            usize::from(data.map_offset) * 8 + local * 2,
            2,
        );
        match value {
            1 => TwinCell::Wall,
            2 => TwinCell::Door,
            _ => TwinCell::Floor,
        }
    }

    pub(crate) const fn solution_len(self) -> u8 {
        self.data().solution_len
    }

    pub(crate) fn solution(self, step: u8) -> Option<Direction4> {
        if step >= self.solution_len() {
            return None;
        }
        Direction4::from_code(read_bits(
            &super::expeditions_data::TWIN_SOLUTIONS,
            usize::from(self.data().solution_offset) * 8 + usize::from(step) * 2,
            2,
        ))
    }
}

pub(crate) const fn twin_level(index: u8) -> Option<TwinLevelRef> {
    if index < EXPEDITION_LEVELS as u8 {
        Some(TwinLevelRef { index })
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FoldCell {
    Void,
    Solid,
    Fragile,
    Bridge,
    Switch,
}

#[derive(Clone, Copy)]
pub(crate) struct FoldLevelRef {
    index: u8,
}

impl FoldLevelRef {
    const fn data(self) -> &'static FoldLevelData {
        &super::expeditions_data::FOLD_LEVELS[self.index as usize]
    }

    pub(crate) const fn start(self) -> u8 {
        self.data().start
    }

    pub(crate) const fn goal(self) -> u8 {
        self.data().goal
    }

    pub(crate) const fn seal_count(self) -> u8 {
        self.data().seal_count
    }

    pub(crate) const fn seal(self, index: usize) -> Option<u8> {
        if index < self.data().seal_count as usize {
            Some(self.data().seals[index])
        } else {
            None
        }
    }

    pub(crate) const fn par(self) -> u8 {
        self.data().par
    }

    pub(crate) fn cell(self, cell: u8) -> FoldCell {
        let value = read_bits(
            &super::expeditions_data::FOLD_MAPS,
            usize::from(self.data().map_offset) * 8 + usize::from(cell) * 3,
            3,
        );
        match value {
            1 => FoldCell::Solid,
            2 => FoldCell::Fragile,
            3 => FoldCell::Bridge,
            4 => FoldCell::Switch,
            _ => FoldCell::Void,
        }
    }

    pub(crate) const fn solution_len(self) -> u8 {
        self.data().solution_len
    }

    pub(crate) fn solution(self, step: u8) -> Option<Direction4> {
        if step >= self.solution_len() {
            return None;
        }
        Direction4::from_code(read_bits(
            &super::expeditions_data::FOLD_SOLUTIONS,
            usize::from(self.data().solution_offset) * 8 + usize::from(step) * 2,
            2,
        ))
    }
}

pub(crate) const fn fold_level(index: u8) -> Option<FoldLevelRef> {
    if index < EXPEDITION_LEVELS as u8 {
        Some(FoldLevelRef { index })
    } else {
        None
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PictureLevelRef {
    index: u8,
}

impl PictureLevelRef {
    const fn data(self) -> &'static PictureLevelData {
        &super::expeditions_data::PICTURE_LEVELS[self.index as usize]
    }

    pub(crate) const fn size(self) -> u8 {
        self.data().size
    }

    pub(crate) fn target(self, cell: u8) -> bool {
        if usize::from(cell) >= usize::from(self.size()) * usize::from(self.size()) {
            return false;
        }
        read_bits(
            &super::expeditions_data::PICTURE_IMAGES,
            usize::from(self.data().image_offset) * 8 + usize::from(cell),
            1,
        ) != 0
    }

    pub(crate) const fn given_count(self) -> u8 {
        self.data().given_count
    }

    pub(crate) const fn given(self, index: usize) -> Option<(u8, bool)> {
        if index >= self.data().given_count as usize {
            return None;
        }
        Some((
            self.data().givens[index],
            self.data().given_values & (1 << index) != 0,
        ))
    }

    pub(crate) const fn is_given(self, cell: u8) -> bool {
        let mut index = 0;
        while index < self.data().given_count as usize {
            if self.data().givens[index] == cell {
                return true;
            }
            index += 1;
        }
        false
    }
}

pub(crate) const fn picture_level(index: u8) -> Option<PictureLevelRef> {
    if index < EXPEDITION_LEVELS as u8 {
        Some(PictureLevelRef { index })
    } else {
        None
    }
}

fn read_bits(bytes: &[u8], bit: usize, width: usize) -> u8 {
    let byte = bit / 8;
    let shift = bit & 7;
    let mut value = u16::from(bytes[byte]) >> shift;
    if shift + width > 8 {
        value |= u16::from(bytes[byte + 1]) << (8 - shift);
    }
    (value as u8) & ((1 << width) - 1)
}

pub(crate) const fn accepted_change() -> ChangeSet {
    ChangeSet::MODEL
        .union(ChangeSet::VISUAL)
        .union(ChangeSet::PERSISTENCE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_directions_round_trip_and_bound_capacity() {
        let mut actions = PackedDirections::default();
        for index in 0..MAX_EXPEDITION_ACTIONS {
            let direction = Direction4::ALL[index & 3];
            assert!(actions.push(direction));
            assert_eq!(actions.get(index as u16), Some(direction));
        }
        assert!(!actions.push(Direction4::Up));
        for index in (0..MAX_EXPEDITION_ACTIONS).rev() {
            assert_eq!(actions.pop(), Some(Direction4::ALL[index & 3]));
        }
        assert!(actions.is_empty());
    }

    #[test]
    fn generated_tables_have_expected_bounds() {
        assert_eq!(super::super::expeditions_data::TWIN_LEVELS.len(), 36);
        assert_eq!(super::super::expeditions_data::FOLD_LEVELS.len(), 36);
        assert_eq!(super::super::expeditions_data::PICTURE_LEVELS.len(), 36);
        assert_eq!(super::super::expeditions_data::TWIN_MAPS.len() / 36, 18);
        assert_eq!(super::super::expeditions_data::FOLD_MAPS.len() / 36, 27);
        assert_eq!(picture_level(0).unwrap().size(), 5);
        assert_eq!(picture_level(35).unwrap().size(), 10);
    }

    #[test]
    fn records_unlock_only_the_contiguous_frontier() {
        let mut records = [LevelRecord::default(); EXPEDITION_LEVELS];
        records[1].submit(7, 3);
        assert_eq!(unlocked_level(&records), 0);
        records[0].submit(5, 3);
        assert_eq!(unlocked_level(&records), 2);
    }

    #[test]
    fn hint_workspace_is_fixed_and_reusable() {
        let mut workspace = ExpeditionHintWorkspace::new();
        assert!(workspace.visit(7, Direction4::Left));
        assert!(!workspace.visit(7, Direction4::Right));
        assert!(workspace.push(7));
        assert_eq!(workspace.pop(), Some(7));
        assert_eq!(workspace.first(7), Direction4::Left);
        workspace.clear();
        assert!(workspace.visit(7, Direction4::Right));
    }

    #[test]
    fn expedition_panels_preserve_the_selected_chapter() {
        let mut state = ExpeditionUiState::default();
        state.open(17);
        assert_eq!(state.panel(), ExpeditionPanel::Map);
        assert_eq!(state.chapter(), 2);
        state.select_chapter(5);
        state.open_rules();
        assert_eq!(state.panel(), ExpeditionPanel::Rules);
        assert_eq!(state.chapter(), 5);
        state.open_briefing();
        assert_eq!(state.panel(), ExpeditionPanel::Briefing);
        state.open_summary();
        assert_eq!(state.panel(), ExpeditionPanel::Summary);
        state.close();
        assert_eq!(state.panel(), ExpeditionPanel::None);
    }
}
