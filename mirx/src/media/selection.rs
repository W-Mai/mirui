use core::{iter::FusedIterator, ops::Range};

use crate::wire::{read_u32_le, write_u32_le};

/// Grid cells between bitmap population checkpoints.
pub const SELECTION_CHECKPOINT_INTERVAL: usize = 256;
const CHECKPOINT_BYTES: usize = SELECTION_CHECKPOINT_INTERVAL / 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Storage<'a> {
    All,
    List(&'a [u8]),
    Bitmap {
        checkpoints: &'a [u8],
        bits: &'a [u8],
    },
}

/// Borrowed mapping from stored-unit ordinals to selected grid cells.
///
/// Selection does not establish complete image coverage, frame inheritance,
/// or decoder independence. Those are group and profile constraints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitSelection<'a> {
    storage: Storage<'a>,
    cell_count: u32,
    count: usize,
}

impl<'a> UnitSelection<'a> {
    /// Selects every grid cell without storing a map.
    pub fn all(cell_count: u32) -> Result<Self, UnitSelectionError> {
        Ok(Self {
            storage: Storage::All,
            cell_count,
            count: usize::try_from(cell_count).map_err(|_| UnitSelectionError::SizeOverflow)?,
        })
    }

    /// Borrows strictly increasing little-endian u32 grid-cell ordinals.
    pub fn list(cell_count: u32, bytes: &'a [u8]) -> Result<Self, UnitSelectionError> {
        u32::try_from(bytes.len()).map_err(|_| UnitSelectionError::SizeOverflow)?;
        if bytes.len() % 4 != 0 {
            return Err(UnitSelectionError::Truncated);
        }
        let mut previous = None;
        for index in 0..bytes.len() / 4 {
            let cell = read_u32_le(bytes, index * 4).unwrap();
            Self::validate_cell(cell_count, cell, previous)?;
            previous = Some(cell);
        }
        Ok(Self {
            storage: Storage::List(bytes),
            cell_count,
            count: bytes.len() / 4,
        })
    }

    /// Borrows population checkpoints followed by low-bit-first presence bits.
    ///
    /// There is one u32 checkpoint per 256 cells. Opening checks the entire
    /// map; lookup examines at most 32 bitmap bytes after finding a checkpoint.
    pub fn bitmap(cell_count: u32, bytes: &'a [u8]) -> Result<Self, UnitSelectionError> {
        let needed = UnitSelectionEncoding::Bitmap.table_len(cell_count, 0)?;
        if bytes.len() != needed {
            return Err(UnitSelectionError::LengthMismatch {
                expected: needed,
                actual: bytes.len(),
            });
        }
        let cells = usize::try_from(cell_count).map_err(|_| UnitSelectionError::SizeOverflow)?;
        let (checkpoints, bits) = bytes.split_at(cells.div_ceil(SELECTION_CHECKPOINT_INTERVAL) * 4);
        if cell_count % 8 != 0
            && bits
                .last()
                .is_some_and(|last| *last >> (cell_count % 8) != 0)
        {
            return Err(UnitSelectionError::NonZeroPadding);
        }
        let mut count = 0u32;
        for (block, chunk) in bits.chunks(CHECKPOINT_BYTES).enumerate() {
            let actual = read_u32_le(checkpoints, block * 4).unwrap();
            if actual != count {
                return Err(UnitSelectionError::CheckpointMismatch {
                    block: block as u32,
                    expected: count,
                    actual,
                });
            }
            count += chunk.iter().map(|byte| byte.count_ones()).sum::<u32>();
        }
        Ok(Self {
            storage: Storage::Bitmap { checkpoints, bits },
            cell_count,
            count: usize::try_from(count).map_err(|_| UnitSelectionError::SizeOverflow)?,
        })
    }

    pub const fn cell_count(self) -> u32 {
        self.cell_count
    }
    pub const fn len(self) -> usize {
        self.count
    }
    pub const fn is_empty(self) -> bool {
        self.count == 0
    }

    /// Finds the grid cell for a selected ordinal without scanning earlier cells.
    pub fn get(self, index: usize) -> Option<u32> {
        if index >= self.count {
            return None;
        }
        match self.storage {
            Storage::All => Some(index as u32),
            Storage::List(bytes) => read_u32_le(bytes, index * 4),
            Storage::Bitmap { checkpoints, bits } => {
                let mut low = 0;
                let mut high = checkpoints.len() / 4;
                while low < high {
                    let middle = low + (high - low) / 2;
                    if read_u32_le(checkpoints, middle * 4).unwrap() <= index as u32 {
                        low = middle + 1;
                    } else {
                        high = middle;
                    }
                }
                let block = low - 1;
                let start = block * CHECKPOINT_BYTES;
                let mut remaining = index as u32 - read_u32_le(checkpoints, block * 4).unwrap();
                for (offset, &byte) in bits[start..].iter().take(CHECKPOINT_BYTES).enumerate() {
                    let count = byte.count_ones();
                    if remaining < count {
                        let mut selected = byte;
                        for _ in 0..remaining {
                            selected &= selected - 1;
                        }
                        return Some((start + offset) as u32 * 8 + selected.trailing_zeros());
                    }
                    remaining -= count;
                }
                unreachable!("validated bitmap population contains this ordinal")
            }
        }
    }

    /// Finds the compact selected ordinal for a grid cell, or None if absent.
    pub fn position(self, cell: u32) -> Option<usize> {
        if cell >= self.cell_count {
            return None;
        }
        if let Storage::Bitmap { bits, .. } = self.storage {
            return (bits[cell as usize / 8] & (1 << (cell % 8)) != 0).then(|| self.rank(cell));
        }
        let ordinal = self.rank(cell);
        (self.get(ordinal) == Some(cell)).then_some(ordinal)
    }

    /// Selects a half-open grid-cell interval without scanning earlier cells.
    /// Returns None for reversed or out-of-bounds intervals.
    pub fn range(self, cells: Range<u32>) -> Option<SelectedUnits<'a>> {
        if cells.start > cells.end || cells.end > self.cell_count {
            return None;
        }
        Some(SelectedUnits {
            selection: self,
            front: self.rank(cells.start),
            back: self.rank(cells.end),
        })
    }

    fn rank(self, cell: u32) -> usize {
        if cell == self.cell_count {
            return self.count;
        }
        match self.storage {
            Storage::All => cell as usize,
            Storage::List(bytes) => {
                let mut low = 0;
                let mut high = self.count;
                while low < high {
                    let middle = low + (high - low) / 2;
                    if read_u32_le(bytes, middle * 4).unwrap() < cell {
                        low = middle + 1;
                    } else {
                        high = middle;
                    }
                }
                low
            }
            Storage::Bitmap { checkpoints, bits } => {
                let byte_index = cell as usize / 8;
                let block = cell as usize / SELECTION_CHECKPOINT_INTERVAL;
                let prefix = read_u32_le(checkpoints, block * 4).unwrap();
                let preceding = bits[block * CHECKPOINT_BYTES..byte_index]
                    .iter()
                    .map(|byte| byte.count_ones())
                    .sum::<u32>();
                let mask = (1u8 << (cell % 8)) - 1;
                (prefix + preceding + (bits[byte_index] & mask).count_ones()) as usize
            }
        }
    }

    pub fn contains(self, cell: u32) -> bool {
        self.position(cell).is_some()
    }

    pub fn iter(self) -> SelectedUnits<'a> {
        SelectedUnits {
            selection: self,
            front: 0,
            back: self.count,
        }
    }

    fn validate_cell(
        cell_count: u32,
        cell: u32,
        previous: Option<u32>,
    ) -> Result<(), UnitSelectionError> {
        if cell >= cell_count {
            return Err(UnitSelectionError::CellOutOfBounds { cell, cell_count });
        }
        if previous.is_some_and(|previous| cell <= previous) {
            return Err(UnitSelectionError::CellsNotIncreasing { cell });
        }
        Ok(())
    }
}

/// Double-ended selection iteration with direct ordinal skips.
#[derive(Clone, Debug)]
pub struct SelectedUnits<'a> {
    selection: UnitSelection<'a>,
    front: usize,
    back: usize,
}

impl Iterator for SelectedUnits<'_> {
    type Item = u32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.selection.get(index)
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.front += n;
        self.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len(), Some(self.len()))
    }
    fn count(self) -> usize {
        self.len()
    }
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}
impl DoubleEndedIterator for SelectedUnits<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.selection.get(self.back)
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.back -= n;
        self.next_back()
    }
}
impl ExactSizeIterator for SelectedUnits<'_> {
    fn len(&self) -> usize {
        self.back - self.front
    }
}
impl FusedIterator for SelectedUnits<'_> {}

/// Explicit sparse-map encoding; full selections may omit the map entirely.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitSelectionEncoding {
    List,
    Bitmap,
}

impl UnitSelectionEncoding {
    pub(crate) fn table_len(
        self,
        cell_count: u32,
        count: usize,
    ) -> Result<usize, UnitSelectionError> {
        let cells = usize::try_from(cell_count).map_err(|_| UnitSelectionError::SizeOverflow)?;
        let needed = match self {
            Self::List => count
                .checked_mul(4)
                .ok_or(UnitSelectionError::SizeOverflow)?,
            Self::Bitmap => cells.div_ceil(SELECTION_CHECKPOINT_INTERVAL) * 4 + cells.div_ceil(8),
        };
        u32::try_from(needed).map_err(|_| UnitSelectionError::SizeOverflow)?;
        Ok(needed)
    }

    pub fn encoded_len(self, cell_count: u32, cells: &[u32]) -> Result<usize, UnitSelectionError> {
        let needed = self.table_len(cell_count, cells.len())?;
        let mut previous = None;
        for &cell in cells {
            UnitSelection::validate_cell(cell_count, cell, previous)?;
            previous = Some(cell);
        }
        Ok(needed)
    }

    /// Writes a validated selection; errors leave the entire output unchanged.
    pub fn encode_into(
        self,
        cell_count: u32,
        cells: &[u32],
        out: &mut [u8],
    ) -> Result<usize, UnitSelectionError> {
        let needed = self.encoded_len(cell_count, cells)?;
        if out.len() < needed {
            return Err(UnitSelectionError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        match self {
            Self::List => {
                for (index, &cell) in cells.iter().enumerate() {
                    write_u32_le(out, index * 4, cell);
                }
            }
            Self::Bitmap => {
                let checkpoint_count =
                    (cell_count as usize).div_ceil(SELECTION_CHECKPOINT_INTERVAL);
                let (checkpoints, bits) = out[..needed].split_at_mut(checkpoint_count * 4);
                bits.fill(0);
                let mut next_checkpoint = 0;
                for (index, &cell) in cells.iter().enumerate() {
                    let block = cell as usize / SELECTION_CHECKPOINT_INTERVAL;
                    while next_checkpoint <= block {
                        write_u32_le(checkpoints, next_checkpoint * 4, index as u32);
                        next_checkpoint += 1;
                    }
                    bits[cell as usize / 8] |= 1 << (cell % 8);
                }
                while next_checkpoint < checkpoint_count {
                    write_u32_le(checkpoints, next_checkpoint * 4, cells.len() as u32);
                    next_checkpoint += 1;
                }
            }
        }
        Ok(needed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UnitSelectionError {
    Truncated,
    SizeOverflow,
    LengthMismatch {
        expected: usize,
        actual: usize,
    },
    CellOutOfBounds {
        cell: u32,
        cell_count: u32,
    },
    CellsNotIncreasing {
        cell: u32,
    },
    CheckpointMismatch {
        block: u32,
        expected: u32,
        actual: u32,
    },
    NonZeroPadding,
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn selection_forms_share_rank_select_and_iterator_semantics() {
        for cell_count in [0, 1, 7, 8, 9, 255, 256, 257, 511, 512, 513, 2049] {
            for step in [1, 2, 7, 257, 800] {
                let cells: alloc::vec::Vec<u32> = (0..cell_count).step_by(step).collect();
                for encoding in [UnitSelectionEncoding::List, UnitSelectionEncoding::Bitmap] {
                    let size = encoding.encoded_len(cell_count, &cells).unwrap();
                    let mut bytes = vec![0xa5; size + 2];
                    assert_eq!(
                        encoding.encode_into(cell_count, &cells, &mut bytes[1..]),
                        Ok(size)
                    );
                    assert_eq!(bytes[0], 0xa5);
                    assert_eq!(bytes[size + 1], 0xa5);
                    let selection = match encoding {
                        UnitSelectionEncoding::List => {
                            UnitSelection::list(cell_count, &bytes[1..size + 1])
                        }
                        UnitSelectionEncoding::Bitmap => {
                            UnitSelection::bitmap(cell_count, &bytes[1..size + 1])
                        }
                    }
                    .unwrap();
                    assert_eq!(selection.len(), cells.len());
                    assert!(selection.iter().eq(cells.iter().copied()));
                    assert!(selection.iter().rev().eq(cells.iter().rev().copied()));
                    for cell in 0..cell_count {
                        assert_eq!(selection.position(cell), cells.binary_search(&cell).ok());
                    }
                    for (index, &cell) in cells.iter().enumerate() {
                        assert_eq!(selection.get(index), Some(cell));
                        assert_eq!(selection.iter().nth(index), Some(cell));
                        assert_eq!(
                            selection.iter().nth_back(cells.len() - index - 1),
                            Some(cell)
                        );
                    }
                    assert_eq!(selection.get(usize::MAX), None);
                    assert_eq!(selection.position(cell_count), None);
                    let mut exhausted = selection.iter();
                    assert_eq!(exhausted.nth(usize::MAX), None);
                    assert_eq!(exhausted.next_back(), None);
                }
            }
        }
        let all = UnitSelection::all(u32::MAX).unwrap();
        assert_eq!(all.get(u32::MAX as usize - 1), Some(u32::MAX - 1));
        assert_eq!(all.iter().nth(u32::MAX as usize - 1), Some(u32::MAX - 1));
        assert_eq!(all.iter().count(), u32::MAX as usize);
        assert_eq!(all.iter().last(), Some(u32::MAX - 1));
    }

    #[test]
    fn independent_bitmap_packet_uses_lsb_cells_and_prefix_populations() {
        let mut bytes = [0u8; 41];
        bytes[4] = 2;
        bytes[8] = 0x81;
        bytes[40] = 1;
        let selection = UnitSelection::bitmap(257, &bytes).unwrap();
        assert!(selection.iter().eq([0, 7, 256]));
        assert_eq!(selection.position(256), Some(2));
        assert_eq!(selection.position(8), None);
        let mut out = [0; 41];
        UnitSelectionEncoding::Bitmap
            .encode_into(257, &[0, 7, 256], &mut out)
            .unwrap();
        assert_eq!(out, bytes);
        assert!(
            UnitSelection::list(8, &[0, 0, 0, 0, 7, 0, 0, 0])
                .unwrap()
                .iter()
                .eq([0, 7])
        );
    }

    #[test]
    fn invalid_maps_and_encoder_errors_never_change_output() {
        assert_eq!(
            UnitSelection::list(10, &[0]),
            Err(UnitSelectionError::Truncated)
        );
        assert!(matches!(
            UnitSelection::list(1, &[1, 0, 0, 0]),
            Err(UnitSelectionError::CellOutOfBounds { .. })
        ));
        assert!(matches!(
            UnitSelection::list(1, &[0; 8]),
            Err(UnitSelectionError::CellsNotIncreasing { .. })
        ));
        assert_eq!(
            UnitSelection::bitmap(1, &[0, 0, 0, 0, 2]),
            Err(UnitSelectionError::NonZeroPadding)
        );
        assert!(matches!(
            UnitSelection::bitmap(1, &[1, 0, 0, 0, 1]),
            Err(UnitSelectionError::CheckpointMismatch { .. })
        ));
        assert!(matches!(
            UnitSelection::bitmap(1, &[]),
            Err(UnitSelectionError::LengthMismatch { .. })
        ));
        assert!(UnitSelection::bitmap(0, &[]).unwrap().is_empty());
        assert!(UnitSelection::bitmap(257, &[0; 41]).unwrap().is_empty());
        for encoding in [UnitSelectionEncoding::List, UnitSelectionEncoding::Bitmap] {
            let mut out = [0xa5; 20];
            for cells in [&[2, 1][..], &[1, 1], &[10]] {
                assert!(encoding.encode_into(10, cells, &mut out).is_err());
                assert_eq!(out, [0xa5; 20]);
            }
            assert!(encoding.encode_into(10, &[1], &mut out[..1]).is_err());
            assert_eq!(out, [0xa5; 20]);
        }
    }

    #[test]
    fn selected_cell_ranges_share_rank_boundaries_across_forms() {
        let cells = [0, 7, 8, 255, 256, 511, 512];
        for encoding in [UnitSelectionEncoding::List, UnitSelectionEncoding::Bitmap] {
            let mut bytes = vec![0; encoding.encoded_len(513, &cells).unwrap()];
            encoding.encode_into(513, &cells, &mut bytes).unwrap();
            let selection = match encoding {
                UnitSelectionEncoding::List => UnitSelection::list(513, &bytes),
                UnitSelectionEncoding::Bitmap => UnitSelection::bitmap(513, &bytes),
            }
            .unwrap();
            for start in [0, 1, 7, 8, 9, 254, 255, 256, 257, 511, 512, 513] {
                for end in [0, 1, 7, 8, 9, 254, 255, 256, 257, 511, 512, 513] {
                    if start > end {
                        assert!(selection.range(start..end).is_none());
                        continue;
                    }
                    let expected: alloc::vec::Vec<_> = cells
                        .iter()
                        .copied()
                        .filter(|cell| *cell >= start && *cell < end)
                        .collect();
                    let range = selection.range(start..end).unwrap();
                    assert_eq!(range.len(), expected.len());
                    assert!(range.clone().eq(expected.iter().copied()));
                    assert!(range.rev().eq(expected.iter().rev().copied()));
                }
            }
            assert!(selection.range(0..514).is_none());
        }
        let all = UnitSelection::all(u32::MAX).unwrap();
        assert!(
            all.range(u32::MAX - 2..u32::MAX)
                .unwrap()
                .eq([u32::MAX - 2, u32::MAX - 1])
        );
        assert_eq!(all.range(0..u32::MAX).unwrap().count(), u32::MAX as usize);
    }
}
