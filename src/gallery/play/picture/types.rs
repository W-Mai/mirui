#[cfg(any(feature = "persistence", test))]
use crate::gallery::play::expeditions::RECORD_BYTES;

pub(super) const PACKED_BYTES: usize = 25;
pub(super) const HISTORY_CAPACITY: usize = 64;
#[cfg(any(feature = "persistence", test))]
pub(super) const SAVE_MAGIC: [u8; 4] = *b"ATL1";
#[cfg(any(feature = "persistence", test))]
pub(super) const SAVE_VERSION: u8 = 1;
#[cfg(any(feature = "persistence", test))]
pub(super) const SAVE_PAYLOAD: usize = 12 + RECORD_BYTES + PACKED_BYTES * (HISTORY_CAPACITY + 1);
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
    pub(super) const fn cell(self) -> PictureCell {
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
    pub(super) fn from_bytes(bytes: [u8; PACKED_BYTES]) -> Option<Self> {
        for cell in 0..100_usize {
            let code = (bytes[cell / 4] >> ((cell & 3) * 2)) & 3;
            if code > PictureCell::EmptyMark as u8 {
                return None;
            }
        }
        Some(Self { bytes })
    }
}
