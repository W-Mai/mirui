use super::FrameSequence;
use crate::wire::{read_u16_le, read_u32_le, write_u16_le, write_u32_le};

/// Borrowed sorted recovery points for bounded random access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyframeIndex<'a> {
    bytes: &'a [u8],
    sequence: FrameSequence,
    width: IndexWidth,
}

/// Validated native recovery points before canonical wire emission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyframeIndexAsset<'a> {
    frames: &'a [u32],
    sequence: FrameSequence,
    width: IndexWidth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IndexWidth {
    U16,
    U32,
}

impl IndexWidth {
    const fn for_sequence(sequence: FrameSequence) -> Self {
        if sequence.frame_count() <= u16::MAX as u32 + 1 {
            Self::U16
        } else {
            Self::U32
        }
    }

    const fn bytes(self) -> usize {
        match self {
            Self::U16 => 2,
            Self::U32 => 4,
        }
    }
}

impl<'a> KeyframeIndex<'a> {
    pub fn open(bytes: &'a [u8], sequence: FrameSequence) -> Result<Self, KeyframeIndexError> {
        let width = IndexWidth::for_sequence(sequence);
        if bytes.is_empty() || bytes.len() % width.bytes() != 0 {
            return Err(KeyframeIndexError::InvalidLength(bytes.len()));
        }
        let index = Self {
            bytes,
            sequence,
            width,
        };
        validate(index.iter(), sequence)?;
        Ok(index)
    }

    pub const fn len(self) -> usize {
        self.bytes.len() / self.width.bytes()
    }

    pub const fn is_empty(self) -> bool {
        false
    }

    pub fn get(self, index: usize) -> Option<u32> {
        read_frame(self.bytes, index, self.width)
    }

    pub fn iter(self) -> KeyframeIter<'a> {
        KeyframeIter {
            bytes: self.bytes,
            width: self.width,
            front: 0,
            back: self.len(),
        }
    }

    /// Finds the closest indexed recovery point at or before `frame`.
    pub fn previous(self, frame: u32) -> Option<u32> {
        if frame >= self.sequence.frame_count() {
            return None;
        }
        let mut left = 0usize;
        let mut right = self.len();
        while left < right {
            let middle = left + (right - left) / 2;
            if self.get(middle)? <= frame {
                left = middle + 1;
            } else {
                right = middle;
            }
        }
        self.get(left.checked_sub(1)?)
    }
}

impl<'a> IntoIterator for KeyframeIndex<'a> {
    type Item = u32;
    type IntoIter = KeyframeIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> KeyframeIndexAsset<'a> {
    pub fn new(frames: &'a [u32], sequence: FrameSequence) -> Result<Self, KeyframeIndexError> {
        let width = IndexWidth::for_sequence(sequence);
        validate(frames.iter().copied(), sequence)?;
        frames
            .len()
            .checked_mul(width.bytes())
            .ok_or(KeyframeIndexError::SizeOverflow)?;
        Ok(Self {
            frames,
            sequence,
            width,
        })
    }

    pub const fn len(self) -> usize {
        self.frames.len()
    }

    pub const fn is_empty(self) -> bool {
        false
    }

    pub const fn encoded_len(self) -> usize {
        self.frames.len() * self.width.bytes()
    }

    pub(crate) const fn frames(self) -> &'a [u32] {
        self.frames
    }

    pub(crate) const fn entry_bytes(self) -> usize {
        self.width.bytes()
    }

    /// Writes canonical indices after validating capacity; errors preserve output.
    pub fn encode_into(self, output: &mut [u8]) -> Result<usize, KeyframeIndexError> {
        let needed = self.encoded_len();
        if output.len() < needed {
            return Err(KeyframeIndexError::BufferTooSmall {
                needed,
                available: output.len(),
            });
        }
        for (index, &frame) in self.frames.iter().enumerate() {
            let offset = index * self.width.bytes();
            match self.width {
                IndexWidth::U16 => write_u16_le(&mut output[..needed], offset, frame as u16),
                IndexWidth::U32 => write_u32_le(&mut output[..needed], offset, frame),
            }
        }
        Ok(needed)
    }

    pub const fn sequence(self) -> FrameSequence {
        self.sequence
    }
}

#[derive(Clone, Debug)]
pub struct KeyframeIter<'a> {
    bytes: &'a [u8],
    width: IndexWidth,
    front: usize,
    back: usize,
}

impl Iterator for KeyframeIter<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front >= self.back {
            return None;
        }
        let frame = read_frame(self.bytes, self.front, self.width)?;
        self.front += 1;
        Some(frame)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.front = self.front.saturating_add(n).min(self.back);
        self.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.back - self.front;
        (len, Some(len))
    }
}

impl DoubleEndedIterator for KeyframeIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front >= self.back {
            return None;
        }
        self.back -= 1;
        read_frame(self.bytes, self.back, self.width)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.back = self.back.saturating_sub(n).max(self.front);
        self.next_back()
    }
}

impl ExactSizeIterator for KeyframeIter<'_> {}
impl core::iter::FusedIterator for KeyframeIter<'_> {}

fn read_frame(bytes: &[u8], index: usize, width: IndexWidth) -> Option<u32> {
    let offset = index.checked_mul(width.bytes())?;
    match width {
        IndexWidth::U16 => read_u16_le(bytes, offset).map(u32::from),
        IndexWidth::U32 => read_u32_le(bytes, offset),
    }
}

fn validate(
    frames: impl IntoIterator<Item = u32>,
    sequence: FrameSequence,
) -> Result<(), KeyframeIndexError> {
    let mut frames = frames.into_iter();
    let Some(first) = frames.next() else {
        return Err(KeyframeIndexError::Empty);
    };
    if first != 0 {
        return Err(KeyframeIndexError::FirstFrameNotIndexed(first));
    }
    let mut previous = first;
    for frame in frames {
        if frame >= sequence.frame_count() {
            return Err(KeyframeIndexError::FrameOutOfBounds {
                frame,
                frame_count: sequence.frame_count(),
            });
        }
        if frame <= previous {
            return Err(KeyframeIndexError::FramesOutOfOrder {
                previous,
                next: frame,
            });
        }
        let delta_frames = frame - previous - 1;
        if delta_frames > u32::from(sequence.max_delta_frames()) {
            return Err(KeyframeIndexError::DeltaBoundExceeded {
                previous,
                next: frame,
                delta_frames,
                limit: sequence.max_delta_frames(),
            });
        }
        previous = frame;
    }
    let tail_frames = sequence.frame_count() - previous - 1;
    if tail_frames > u32::from(sequence.max_delta_frames()) {
        return Err(KeyframeIndexError::TailBoundExceeded {
            previous,
            tail_frames,
            limit: sequence.max_delta_frames(),
        });
    }
    Ok(())
}

/// Failure while reading or emitting a bounded keyframe index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum KeyframeIndexError {
    Empty,
    InvalidLength(usize),
    FirstFrameNotIndexed(u32),
    FrameOutOfBounds {
        frame: u32,
        frame_count: u32,
    },
    FramesOutOfOrder {
        previous: u32,
        next: u32,
    },
    DeltaBoundExceeded {
        previous: u32,
        next: u32,
        delta_frames: u32,
        limit: u16,
    },
    TailBoundExceeded {
        previous: u32,
        tail_frames: u32,
        limit: u16,
    },
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    SizeOverflow,
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    fn sequence(frames: u32, bound: u16) -> FrameSequence {
        FrameSequence::new(frames, 60, 1)
            .unwrap()
            .with_max_delta_frames(bound)
            .unwrap()
    }

    #[test]
    fn compact_index_roundtrips_and_seeks_backwards() {
        let frames = [0, 4, 8];
        let sequence = sequence(10, 3);
        let asset = KeyframeIndexAsset::new(&frames, sequence).unwrap();
        assert_eq!(asset.encoded_len(), 6);
        let mut bytes = [0xff; 8];
        let len = asset.encode_into(&mut bytes).unwrap();
        assert_eq!(&bytes[len..], &[0xff; 2]);
        let index = KeyframeIndex::open(&bytes[..len], sequence).unwrap();
        assert_eq!(index.iter().collect::<Vec<_>>(), frames);
        assert_eq!(index.previous(0), Some(0));
        assert_eq!(index.previous(7), Some(4));
        assert_eq!(index.previous(9), Some(8));
        assert_eq!(index.previous(10), None);
    }

    #[test]
    fn gaps_and_tail_obey_the_single_sequence_bound() {
        let sequence = sequence(10, 2);
        assert_eq!(
            KeyframeIndexAsset::new(&[0, 4, 7], sequence),
            Err(KeyframeIndexError::DeltaBoundExceeded {
                previous: 0,
                next: 4,
                delta_frames: 3,
                limit: 2,
            })
        );
        assert_eq!(
            KeyframeIndexAsset::new(&[0, 3, 6], sequence),
            Err(KeyframeIndexError::TailBoundExceeded {
                previous: 6,
                tail_frames: 3,
                limit: 2,
            })
        );
        assert!(KeyframeIndexAsset::new(&[0, 3, 6, 9], sequence).is_ok());
    }

    #[test]
    fn roots_order_bounds_and_output_capacity_are_strict() {
        let sequence = sequence(5, 4);
        assert_eq!(
            KeyframeIndexAsset::new(&[], sequence),
            Err(KeyframeIndexError::Empty)
        );
        assert_eq!(
            KeyframeIndexAsset::new(&[1], sequence),
            Err(KeyframeIndexError::FirstFrameNotIndexed(1))
        );
        assert_eq!(
            KeyframeIndexAsset::new(&[0, 2, 2], sequence),
            Err(KeyframeIndexError::FramesOutOfOrder {
                previous: 2,
                next: 2
            })
        );
        assert_eq!(
            KeyframeIndexAsset::new(&[0, 5], sequence),
            Err(KeyframeIndexError::FrameOutOfBounds {
                frame: 5,
                frame_count: 5
            })
        );
        let frames = [0];
        let asset = KeyframeIndexAsset::new(&frames, sequence).unwrap();
        let mut output = [0xa5; 1];
        assert_eq!(
            asset.encode_into(&mut output),
            Err(KeyframeIndexError::BufferTooSmall {
                needed: 2,
                available: 1
            })
        );
        assert_eq!(output, [0xa5]);
    }

    #[test]
    fn large_sequences_use_u32_indices_without_a_width_field() {
        let sequence = FrameSequence::new(70_000, 1, 1)
            .unwrap()
            .with_max_delta_frames(u16::MAX)
            .unwrap();
        let frames = [0, 65_536];
        let asset = KeyframeIndexAsset::new(&frames, sequence).unwrap();
        assert_eq!(asset.encoded_len(), 8);
        let mut bytes = [0; 8];
        asset.encode_into(&mut bytes).unwrap();
        let index = KeyframeIndex::open(&bytes, sequence).unwrap();
        assert_eq!(index.get(1), Some(65_536));
    }
}
