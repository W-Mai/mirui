use core::ops::Range;

use crate::wire::{read_u16_le, read_u32_le, write_u16_le, write_u32_le};

/// Borrowed frame-to-group ranges stored as cumulative u16 or u32 endpoints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameMap<'a> {
    bytes: &'a [u8],
    frame_count: u32,
    group_count: u32,
    width: MapWidth,
}

/// Authoring view over cumulative group endpoints, one value per frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameMapAsset<'a> {
    ends: &'a [u32],
    group_count: u32,
    width: MapWidth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MapWidth {
    U16,
    U32,
}

impl MapWidth {
    const fn bytes(self) -> usize {
        match self {
            Self::U16 => 2,
            Self::U32 => 4,
        }
    }
}

impl<'a> FrameMap<'a> {
    /// Opens a compact map using the frame count from the SEQUENCE record.
    ///
    /// Equal neighboring endpoints represent a no-op frame. The first frame
    /// must select at least one group so playback always has a recoverable root.
    pub fn open(bytes: &'a [u8], frame_count: u32) -> Result<Self, FrameMapError> {
        if frame_count == 0 {
            return Err(FrameMapError::Empty);
        }
        let narrow_len = table_len(frame_count, MapWidth::U16)?;
        let wide_len = table_len(frame_count, MapWidth::U32)?;
        let width = if bytes.len() == narrow_len {
            MapWidth::U16
        } else if bytes.len() == wide_len {
            MapWidth::U32
        } else {
            return Err(FrameMapError::InvalidLength {
                frame_count,
                actual: bytes.len(),
            });
        };
        let mut previous = 0;
        for frame in 0..frame_count {
            let end = read_end(bytes, frame, width).ok_or(FrameMapError::SizeOverflow)?;
            if end < previous {
                return Err(FrameMapError::GroupsOutOfOrder {
                    frame,
                    previous,
                    next: end,
                });
            }
            previous = end;
        }
        if previous == 0 {
            return Err(FrameMapError::FirstFrameEmpty);
        }
        if width == MapWidth::U32 && previous <= u32::from(u16::MAX) {
            return Err(FrameMapError::NonCanonicalWide {
                group_count: previous,
            });
        }
        Ok(Self {
            bytes,
            frame_count,
            group_count: previous,
            width,
        })
    }

    pub const fn frame_count(self) -> u32 {
        self.frame_count
    }

    pub const fn group_count(self) -> u32 {
        self.group_count
    }

    pub const fn encoded_len(self) -> usize {
        self.bytes.len()
    }

    pub fn get(self, frame: u32) -> Option<Range<u32>> {
        if frame >= self.frame_count {
            return None;
        }
        let end = read_end(self.bytes, frame, self.width)?;
        let start = if frame == 0 {
            0
        } else {
            read_end(self.bytes, frame - 1, self.width)?
        };
        Some(start..end)
    }

    pub fn iter(self) -> FrameMapIter<'a> {
        FrameMapIter {
            map: self,
            front: 0,
            back: self.frame_count,
        }
    }
}

impl<'a> IntoIterator for FrameMap<'a> {
    type Item = Range<u32>;
    type IntoIter = FrameMapIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> FrameMapAsset<'a> {
    pub fn new(ends: &'a [u32]) -> Result<Self, FrameMapError> {
        if ends.is_empty() {
            return Err(FrameMapError::Empty);
        }
        u32::try_from(ends.len()).map_err(|_| FrameMapError::SizeOverflow)?;
        let mut previous = 0;
        for (frame, &end) in ends.iter().enumerate() {
            if end < previous {
                return Err(FrameMapError::GroupsOutOfOrder {
                    frame: frame as u32,
                    previous,
                    next: end,
                });
            }
            previous = end;
        }
        if previous == 0 {
            return Err(FrameMapError::FirstFrameEmpty);
        }
        let width = if previous <= u32::from(u16::MAX) {
            MapWidth::U16
        } else {
            MapWidth::U32
        };
        table_len(ends.len() as u32, width)?;
        Ok(Self {
            ends,
            group_count: previous,
            width,
        })
    }

    pub const fn frame_count(self) -> usize {
        self.ends.len()
    }

    pub const fn group_count(self) -> u32 {
        self.group_count
    }

    pub const fn encoded_len(self) -> usize {
        self.ends.len() * self.width.bytes()
    }

    /// Writes canonical cumulative endpoints; errors preserve the output.
    pub fn encode_into(self, output: &mut [u8]) -> Result<usize, FrameMapError> {
        let needed = self.encoded_len();
        if output.len() < needed {
            return Err(FrameMapError::BufferTooSmall {
                needed,
                available: output.len(),
            });
        }
        for (index, &end) in self.ends.iter().enumerate() {
            let offset = index * self.width.bytes();
            match self.width {
                MapWidth::U16 => write_u16_le(&mut output[..needed], offset, end as u16),
                MapWidth::U32 => write_u32_le(&mut output[..needed], offset, end),
            }
        }
        Ok(needed)
    }
}

/// Validated per-frame group counts that can be accumulated into a map stream.
///
/// This form avoids requiring callers to build another endpoint array.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameCounts<'a> {
    counts: &'a [u32],
    group_count: u32,
    width: MapWidth,
}

impl<'a> FrameCounts<'a> {
    pub fn new(counts: &'a [u32]) -> Result<Self, FrameMapError> {
        if counts.is_empty() {
            return Err(FrameMapError::Empty);
        }
        u32::try_from(counts.len()).map_err(|_| FrameMapError::SizeOverflow)?;
        if counts[0] == 0 {
            return Err(FrameMapError::FirstFrameEmpty);
        }
        let mut group_count = 0u32;
        for &count in counts {
            group_count = group_count
                .checked_add(count)
                .ok_or(FrameMapError::SizeOverflow)?;
        }
        let width = if group_count <= u32::from(u16::MAX) {
            MapWidth::U16
        } else {
            MapWidth::U32
        };
        table_len(counts.len() as u32, width)?;
        Ok(Self {
            counts,
            group_count,
            width,
        })
    }

    pub const fn frame_count(self) -> usize {
        self.counts.len()
    }

    pub const fn group_count(self) -> u32 {
        self.group_count
    }

    pub const fn encoded_len(self) -> usize {
        self.counts.len() * self.width.bytes()
    }

    /// Writes accumulated endpoints directly from counts without allocating.
    pub fn encode_into(self, output: &mut [u8]) -> Result<usize, FrameMapError> {
        let needed = self.encoded_len();
        if output.len() < needed {
            return Err(FrameMapError::BufferTooSmall {
                needed,
                available: output.len(),
            });
        }
        let mut end = 0u32;
        for (index, &count) in self.counts.iter().enumerate() {
            end = end.checked_add(count).ok_or(FrameMapError::SizeOverflow)?;
            let offset = index * self.width.bytes();
            match self.width {
                MapWidth::U16 => write_u16_le(&mut output[..needed], offset, end as u16),
                MapWidth::U32 => write_u32_le(&mut output[..needed], offset, end),
            }
        }
        Ok(needed)
    }
}

#[derive(Clone, Debug)]
pub struct FrameMapIter<'a> {
    map: FrameMap<'a>,
    front: u32,
    back: u32,
}

impl Iterator for FrameMapIter<'_> {
    type Item = Range<u32>;

    fn next(&mut self) -> Option<Self::Item> {
        let frame = self.front;
        let range = self.map.get(frame)?;
        self.front += 1;
        Some(range)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let skip = u32::try_from(n).ok()?;
        self.front = self.front.saturating_add(skip).min(self.back);
        self.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = (self.back - self.front) as usize;
        (len, Some(len))
    }
}

impl DoubleEndedIterator for FrameMapIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front >= self.back {
            return None;
        }
        self.back -= 1;
        self.map.get(self.back)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        let skip = u32::try_from(n).ok()?;
        self.back = self.back.saturating_sub(skip).max(self.front);
        self.next_back()
    }
}

impl ExactSizeIterator for FrameMapIter<'_> {}
impl core::iter::FusedIterator for FrameMapIter<'_> {}

fn table_len(frame_count: u32, width: MapWidth) -> Result<usize, FrameMapError> {
    usize::try_from(frame_count)
        .ok()
        .and_then(|count| count.checked_mul(width.bytes()))
        .ok_or(FrameMapError::SizeOverflow)
}

fn read_end(bytes: &[u8], frame: u32, width: MapWidth) -> Option<u32> {
    let offset = usize::try_from(frame).ok()?.checked_mul(width.bytes())?;
    match width {
        MapWidth::U16 => read_u16_le(bytes, offset).map(u32::from),
        MapWidth::U32 => read_u32_le(bytes, offset),
    }
}

/// Failure while reading or emitting a compact frame-to-group map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameMapError {
    Empty,
    InvalidLength {
        frame_count: u32,
        actual: usize,
    },
    FirstFrameEmpty,
    GroupsOutOfOrder {
        frame: u32,
        previous: u32,
        next: u32,
    },
    NonCanonicalWide {
        group_count: u32,
    },
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    SizeOverflow,
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    #[test]
    fn narrow_map_roundtrips_noop_frames_without_allocating_records() {
        let ends = [2, 3, 3, 5];
        let asset = FrameMapAsset::new(&ends).unwrap();
        assert_eq!(asset.encoded_len(), 8);
        let mut bytes = [0xff; 12];
        let len = asset.encode_into(&mut bytes).unwrap();
        assert_eq!(len, 8);
        assert_eq!(&bytes[len..], &[0xff; 4]);
        let map = FrameMap::open(&bytes[..len], 4).unwrap();
        assert_eq!(map.group_count(), 5);
        assert_eq!(
            map.iter().collect::<vec::Vec<_>>(),
            [0..2, 2..3, 3..3, 3..5]
        );
        assert_eq!(map.get(4), None);
    }

    #[test]
    fn counts_write_the_same_canonical_map_without_endpoint_storage() {
        let counts = [2, 1, 0, 2];
        let counted = FrameCounts::new(&counts).unwrap();
        let mut bytes = [0; 8];
        assert_eq!(counted.encode_into(&mut bytes), Ok(8));
        let map = FrameMap::open(&bytes, 4).unwrap();
        assert_eq!(
            map.iter().collect::<vec::Vec<_>>(),
            [0..2, 2..3, 3..3, 3..5]
        );
    }

    #[test]
    fn wide_map_is_used_only_above_the_u16_group_limit() {
        let ends = [1, u16::MAX as u32 + 1];
        let asset = FrameMapAsset::new(&ends).unwrap();
        assert_eq!(asset.encoded_len(), 8);
        let mut bytes = [0; 8];
        asset.encode_into(&mut bytes).unwrap();
        let map = FrameMap::open(&bytes, 2).unwrap();
        assert_eq!(map.get(1), Some(1..65_536));

        let mut redundant = [0; 8];
        write_u32_le(&mut redundant, 0, 1);
        write_u32_le(&mut redundant, 4, 2);
        assert_eq!(
            FrameMap::open(&redundant, 2),
            Err(FrameMapError::NonCanonicalWide { group_count: 2 })
        );
    }

    #[test]
    fn malformed_maps_and_short_outputs_are_rejected() {
        assert_eq!(FrameMapAsset::new(&[]), Err(FrameMapError::Empty));
        assert_eq!(
            FrameMapAsset::new(&[0]),
            Err(FrameMapError::FirstFrameEmpty)
        );
        assert_eq!(
            FrameMapAsset::new(&[2, 1]),
            Err(FrameMapError::GroupsOutOfOrder {
                frame: 1,
                previous: 2,
                next: 1,
            })
        );
        assert_eq!(
            FrameMap::open(&[0, 1, 2], 2),
            Err(FrameMapError::InvalidLength {
                frame_count: 2,
                actual: 3,
            })
        );
        let asset = FrameMapAsset::new(&[1, 2]).unwrap();
        let mut short = [0; 3];
        assert_eq!(
            asset.encode_into(&mut short),
            Err(FrameMapError::BufferTooSmall {
                needed: 4,
                available: 3,
            })
        );
    }

    #[test]
    fn iterator_skips_in_constant_time_from_both_ends() {
        let ends = [1, 3, 3, 7, 8];
        let asset = FrameMapAsset::new(&ends).unwrap();
        let mut bytes = [0; 10];
        asset.encode_into(&mut bytes).unwrap();
        let map = FrameMap::open(&bytes, 5).unwrap();
        let mut iter = map.iter();
        assert_eq!(iter.nth(2), Some(3..3));
        assert_eq!(iter.nth_back(1), Some(3..7));
        assert_eq!(iter.len(), 0);
    }
}
