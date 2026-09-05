use crate::wire::{read_u32_le, write_u32_le};

pub const FRAME_TIMING_HEADER_LEN: usize = 4;
const SPARSE_ENTRY_LEN: usize = 8;

/// Physical representation selected for a nonempty FRAME_TIMING section.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameTimingEncoding {
    Dense,
    Sparse,
}

/// Borrowed canonical per-frame timing overrides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameTiming<'a> {
    body: &'a [u8],
    frame_count: u32,
    default_duration_ticks: u32,
    encoding: FrameTimingEncoding,
}

/// Authoring view that chooses the smaller dense or sparse timing column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameTimingAsset<'a> {
    durations: &'a [u32],
    default_duration_ticks: u32,
    override_count: usize,
    encoding: Option<FrameTimingEncoding>,
}

impl<'a> FrameTiming<'a> {
    pub fn open(
        bytes: &'a [u8],
        frame_count: u32,
        default_duration_ticks: u32,
    ) -> Result<Self, FrameTimingError> {
        if frame_count == 0 {
            return Err(FrameTimingError::EmptySequence);
        }
        if default_duration_ticks == 0 {
            return Err(FrameTimingError::ZeroDefaultDuration);
        }
        let header = bytes
            .get(..FRAME_TIMING_HEADER_LEN)
            .ok_or(FrameTimingError::Truncated {
                needed: FRAME_TIMING_HEADER_LEN,
                available: bytes.len(),
            })?;
        if header[1..] != [0; 3] {
            return Err(FrameTimingError::NonCanonicalHeader);
        }
        let encoding = match header[0] {
            0 => FrameTimingEncoding::Dense,
            1 => FrameTimingEncoding::Sparse,
            value => return Err(FrameTimingError::UnknownEncoding(value)),
        };
        let body = &bytes[FRAME_TIMING_HEADER_LEN..];
        let dense_len = dense_body_len(frame_count)?;
        let override_count = match encoding {
            FrameTimingEncoding::Dense => {
                if body.len() != dense_len {
                    return Err(FrameTimingError::InvalidDenseLength {
                        expected: dense_len,
                        actual: body.len(),
                    });
                }
                let mut count = 0;
                for frame in 0..frame_count {
                    let duration = read_dense(body, frame).ok_or(FrameTimingError::SizeOverflow)?;
                    validate_duration(frame, duration)?;
                    count += usize::from(duration != default_duration_ticks);
                }
                count
            }
            FrameTimingEncoding::Sparse => {
                if body.is_empty() || body.len() % SPARSE_ENTRY_LEN != 0 {
                    return Err(FrameTimingError::InvalidSparseLength(body.len()));
                }
                let mut previous = None;
                for entry in body.chunks_exact(SPARSE_ENTRY_LEN) {
                    let frame = read_u32_le(entry, 0).expect("complete timing entry");
                    let duration = read_u32_le(entry, 4).expect("complete timing entry");
                    if frame >= frame_count {
                        return Err(FrameTimingError::FrameOutOfBounds { frame, frame_count });
                    }
                    if previous.is_some_and(|previous| frame <= previous) {
                        return Err(FrameTimingError::FramesOutOfOrder {
                            previous: previous.unwrap(),
                            next: frame,
                        });
                    }
                    validate_duration(frame, duration)?;
                    if duration == default_duration_ticks {
                        return Err(FrameTimingError::RedundantDefault { frame });
                    }
                    previous = Some(frame);
                }
                body.len() / SPARSE_ENTRY_LEN
            }
        };
        if override_count == 0 {
            return Err(FrameTimingError::NoOverrides);
        }
        let best = preferred_encoding(frame_count, override_count)?;
        if encoding != best {
            return Err(FrameTimingError::NonCanonicalEncoding {
                expected: best,
                actual: encoding,
            });
        }
        Ok(Self {
            body,
            frame_count,
            default_duration_ticks,
            encoding,
        })
    }

    pub const fn frame_count(self) -> u32 {
        self.frame_count
    }

    pub const fn encoding(self) -> FrameTimingEncoding {
        self.encoding
    }

    pub fn duration(self, frame: u32) -> Option<u32> {
        if frame >= self.frame_count {
            return None;
        }
        match self.encoding {
            FrameTimingEncoding::Dense => read_dense(self.body, frame),
            FrameTimingEncoding::Sparse => self
                .sparse_duration(frame)
                .or(Some(self.default_duration_ticks)),
        }
    }

    pub(crate) fn cycle_duration_ticks(self) -> u64 {
        match self.encoding {
            FrameTimingEncoding::Dense => (0..self.frame_count)
                .map(|frame| {
                    u64::from(read_dense(self.body, frame).expect("validated dense duration"))
                })
                .sum(),
            FrameTimingEncoding::Sparse => {
                let mut total =
                    u128::from(self.frame_count) * u128::from(self.default_duration_ticks);
                for entry in self.body.chunks_exact(SPARSE_ENTRY_LEN) {
                    let duration = read_u32_le(entry, 4).expect("validated sparse duration");
                    total = total + u128::from(duration) - u128::from(self.default_duration_ticks);
                }
                u64::try_from(total).expect("u32 frame counts and durations fit u64")
            }
        }
    }

    pub(crate) fn locate(self, elapsed_ticks: u64) -> Option<(u32, u32)> {
        if elapsed_ticks >= self.cycle_duration_ticks() {
            return None;
        }
        match self.encoding {
            FrameTimingEncoding::Dense => {
                let mut remaining = elapsed_ticks;
                for frame in 0..self.frame_count {
                    let duration =
                        u64::from(read_dense(self.body, frame).expect("validated dense duration"));
                    if remaining < duration {
                        return Some((frame, remaining as u32));
                    }
                    remaining -= duration;
                }
                unreachable!("validated durations cover one complete cycle")
            }
            FrameTimingEncoding::Sparse => self.locate_sparse(elapsed_ticks),
        }
    }

    fn locate_sparse(self, mut remaining: u64) -> Option<(u32, u32)> {
        let default = u64::from(self.default_duration_ticks);
        let mut frame = 0u32;
        for entry in self.body.chunks_exact(SPARSE_ENTRY_LEN) {
            let override_frame = read_u32_le(entry, 0).expect("validated sparse frame");
            let default_run = u64::from(override_frame - frame) * default;
            if remaining < default_run {
                let offset = u32::try_from(remaining / default)
                    .expect("default run offset fits frame ordinal");
                return Some((frame + offset, (remaining % default) as u32));
            }
            remaining -= default_run;

            let duration = u64::from(read_u32_le(entry, 4).expect("validated sparse duration"));
            if remaining < duration {
                return Some((override_frame, remaining as u32));
            }
            remaining -= duration;
            frame = override_frame + 1;
        }

        let offset =
            u32::try_from(remaining / default).expect("default tail offset fits frame ordinal");
        Some((frame + offset, (remaining % default) as u32))
    }

    fn sparse_duration(self, frame: u32) -> Option<u32> {
        let mut left = 0usize;
        let mut right = self.body.len() / SPARSE_ENTRY_LEN;
        while left < right {
            let middle = left + (right - left) / 2;
            let entry = &self.body[middle * SPARSE_ENTRY_LEN..][..SPARSE_ENTRY_LEN];
            match read_u32_le(entry, 0)?.cmp(&frame) {
                core::cmp::Ordering::Less => left = middle + 1,
                core::cmp::Ordering::Greater => right = middle,
                core::cmp::Ordering::Equal => return read_u32_le(entry, 4),
            }
        }
        None
    }
}

impl<'a> FrameTimingAsset<'a> {
    pub fn new(
        durations: &'a [u32],
        default_duration_ticks: u32,
    ) -> Result<Self, FrameTimingError> {
        if durations.is_empty() {
            return Err(FrameTimingError::EmptySequence);
        }
        if default_duration_ticks == 0 {
            return Err(FrameTimingError::ZeroDefaultDuration);
        }
        u32::try_from(durations.len()).map_err(|_| FrameTimingError::SizeOverflow)?;
        let mut override_count = 0;
        for (frame, &duration) in durations.iter().enumerate() {
            validate_duration(frame as u32, duration)?;
            override_count += usize::from(duration != default_duration_ticks);
        }
        let encoding = if override_count == 0 {
            None
        } else {
            Some(preferred_encoding(durations.len() as u32, override_count)?)
        };
        Ok(Self {
            durations,
            default_duration_ticks,
            override_count,
            encoding,
        })
    }

    /// Returns true when every frame uses the sequence default and no section is emitted.
    pub const fn is_empty(self) -> bool {
        self.encoding.is_none()
    }

    pub const fn frame_count(self) -> usize {
        self.durations.len()
    }

    pub const fn encoding(self) -> Option<FrameTimingEncoding> {
        self.encoding
    }

    pub(crate) const fn durations(self) -> &'a [u32] {
        self.durations
    }

    pub(crate) const fn default_duration_ticks(self) -> u32 {
        self.default_duration_ticks
    }

    pub fn encoded_len(self) -> usize {
        match self.encoding {
            None => 0,
            Some(FrameTimingEncoding::Dense) => FRAME_TIMING_HEADER_LEN + self.durations.len() * 4,
            Some(FrameTimingEncoding::Sparse) => {
                FRAME_TIMING_HEADER_LEN + self.override_count * SPARSE_ENTRY_LEN
            }
        }
    }

    /// Writes the canonical nonempty section; an omitted table writes zero bytes.
    pub fn encode_into(self, output: &mut [u8]) -> Result<usize, FrameTimingError> {
        let needed = self.encoded_len();
        if output.len() < needed {
            return Err(FrameTimingError::BufferTooSmall {
                needed,
                available: output.len(),
            });
        }
        let Some(encoding) = self.encoding else {
            return Ok(0);
        };
        output[..needed].fill(0);
        output[0] = match encoding {
            FrameTimingEncoding::Dense => 0,
            FrameTimingEncoding::Sparse => 1,
        };
        let mut cursor = FRAME_TIMING_HEADER_LEN;
        for (frame, &duration) in self.durations.iter().enumerate() {
            if encoding == FrameTimingEncoding::Sparse && duration == self.default_duration_ticks {
                continue;
            }
            if encoding == FrameTimingEncoding::Sparse {
                write_u32_le(&mut output[..needed], cursor, frame as u32);
                cursor += 4;
            }
            write_u32_le(&mut output[..needed], cursor, duration);
            cursor += 4;
        }
        debug_assert_eq!(cursor, needed);
        Ok(needed)
    }
}

fn preferred_encoding(
    frame_count: u32,
    override_count: usize,
) -> Result<FrameTimingEncoding, FrameTimingError> {
    let dense = dense_body_len(frame_count)?;
    let sparse = override_count
        .checked_mul(SPARSE_ENTRY_LEN)
        .ok_or(FrameTimingError::SizeOverflow)?;
    Ok(if sparse < dense {
        FrameTimingEncoding::Sparse
    } else {
        FrameTimingEncoding::Dense
    })
}

fn dense_body_len(frame_count: u32) -> Result<usize, FrameTimingError> {
    usize::try_from(frame_count)
        .ok()
        .and_then(|count| count.checked_mul(4))
        .ok_or(FrameTimingError::SizeOverflow)
}

fn read_dense(bytes: &[u8], frame: u32) -> Option<u32> {
    read_u32_le(bytes, usize::try_from(frame).ok()?.checked_mul(4)?)
}

fn validate_duration(frame: u32, duration: u32) -> Result<(), FrameTimingError> {
    if duration == 0 {
        Err(FrameTimingError::ZeroDuration { frame })
    } else {
        Ok(())
    }
}

/// Failure while reading or emitting per-frame timing overrides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameTimingError {
    Truncated {
        needed: usize,
        available: usize,
    },
    EmptySequence,
    ZeroDefaultDuration,
    UnknownEncoding(u8),
    NonCanonicalHeader,
    InvalidDenseLength {
        expected: usize,
        actual: usize,
    },
    InvalidSparseLength(usize),
    FrameOutOfBounds {
        frame: u32,
        frame_count: u32,
    },
    FramesOutOfOrder {
        previous: u32,
        next: u32,
    },
    ZeroDuration {
        frame: u32,
    },
    RedundantDefault {
        frame: u32,
    },
    NoOverrides,
    NonCanonicalEncoding {
        expected: FrameTimingEncoding,
        actual: FrameTimingEncoding,
    },
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    SizeOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_default_durations_omit_the_section() {
        let asset = FrameTimingAsset::new(&[40, 40, 40], 40).unwrap();
        assert!(asset.is_empty());
        assert_eq!(asset.encoded_len(), 0);
        assert_eq!(asset.encode_into(&mut []), Ok(0));
    }

    #[test]
    fn sparse_overrides_roundtrip_and_default_missing_frames() {
        let durations = [40, 80, 40, 40, 120];
        let asset = FrameTimingAsset::new(&durations, 40).unwrap();
        assert_eq!(asset.encoding(), Some(FrameTimingEncoding::Sparse));
        let mut bytes = [0; 20];
        let len = asset.encode_into(&mut bytes).unwrap();
        assert_eq!(len, 20);
        let timing = FrameTiming::open(&bytes[..len], 5, 40).unwrap();
        assert_eq!(timing.encoding(), FrameTimingEncoding::Sparse);
        for (frame, duration) in durations.into_iter().enumerate() {
            assert_eq!(timing.duration(frame as u32), Some(duration));
        }
        assert_eq!(timing.duration(5), None);
    }

    #[test]
    fn sparse_timeline_skips_default_runs_around_overrides() {
        let mut durations = [10; 1_000];
        durations[500] = 25;
        let asset = FrameTimingAsset::new(&durations, 10).unwrap();
        assert_eq!(asset.encoding(), Some(FrameTimingEncoding::Sparse));
        let mut bytes = [0; 12];
        let len = asset.encode_into(&mut bytes).unwrap();
        let timing = FrameTiming::open(&bytes[..len], 1_000, 10).unwrap();

        assert_eq!(timing.cycle_duration_ticks(), 10_015);
        assert_eq!(timing.locate(4_999), Some((499, 9)));
        assert_eq!(timing.locate(5_000), Some((500, 0)));
        assert_eq!(timing.locate(5_024), Some((500, 24)));
        assert_eq!(timing.locate(5_025), Some((501, 0)));
        assert_eq!(timing.locate(10_014), Some((999, 9)));
        assert_eq!(timing.locate(10_015), None);
    }

    #[test]
    fn dense_column_wins_ties_and_roundtrips() {
        let durations = [20, 40, 60, 40];
        let asset = FrameTimingAsset::new(&durations, 40).unwrap();
        assert_eq!(asset.encoding(), Some(FrameTimingEncoding::Dense));
        let mut bytes = [0; 20];
        asset.encode_into(&mut bytes).unwrap();
        let timing = FrameTiming::open(&bytes, 4, 40).unwrap();
        assert_eq!(timing.encoding(), FrameTimingEncoding::Dense);
        assert_eq!(timing.duration(2), Some(60));
    }

    #[test]
    fn nonminimal_and_redundant_wire_forms_are_rejected() {
        let mut dense = [0; 24];
        for (index, duration) in [40, 40, 80, 40, 40].into_iter().enumerate() {
            write_u32_le(&mut dense, FRAME_TIMING_HEADER_LEN + index * 4, duration);
        }
        assert_eq!(
            FrameTiming::open(&dense, 5, 40),
            Err(FrameTimingError::NonCanonicalEncoding {
                expected: FrameTimingEncoding::Sparse,
                actual: FrameTimingEncoding::Dense,
            })
        );

        let mut sparse = [0; 12];
        sparse[0] = 1;
        write_u32_le(&mut sparse, 4, 2);
        write_u32_le(&mut sparse, 8, 40);
        assert_eq!(
            FrameTiming::open(&sparse, 5, 40),
            Err(FrameTimingError::RedundantDefault { frame: 2 })
        );
    }

    #[test]
    fn malformed_tables_and_capacity_failures_preserve_output() {
        let asset = FrameTimingAsset::new(&[40, 80, 40], 40).unwrap();
        let mut short = [0xa5; 11];
        assert_eq!(
            asset.encode_into(&mut short),
            Err(FrameTimingError::BufferTooSmall {
                needed: 12,
                available: 11,
            })
        );
        assert_eq!(short, [0xa5; 11]);

        let mut unordered = [0; 20];
        unordered[0] = 1;
        write_u32_le(&mut unordered, 4, 2);
        write_u32_le(&mut unordered, 8, 80);
        write_u32_le(&mut unordered, 12, 1);
        write_u32_le(&mut unordered, 16, 60);
        assert_eq!(
            FrameTiming::open(&unordered, 5, 40),
            Err(FrameTimingError::FramesOutOfOrder {
                previous: 2,
                next: 1,
            })
        );
    }
}
