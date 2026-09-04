use crate::wire::{read_u16_le, read_u32_le, write_u16_le, write_u32_le};

pub const FRAME_SEQUENCE_RECORD_LEN: usize = 20;
const MAX_TIMESCALE_HZ: u32 = 1_000_000;

/// How decoded samples combine with the current presentation canvas.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum BlendMode {
    /// Replace every selected sample.
    #[default]
    Replace = 0,
    /// Composite selected samples over the current canvas.
    SourceOver = 1,
}

impl BlendMode {
    pub(super) fn open(value: u8) -> Result<Self, FrameSequenceError> {
        match value {
            0 => Ok(Self::Replace),
            1 => Ok(Self::SourceOver),
            _ => Err(FrameSequenceError::UnknownBlendMode(value)),
        }
    }
}

/// What happens to the updated presentation region after its duration elapses.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum DisposalMode {
    /// Keep the presented canvas as the next frame's starting state.
    #[default]
    Keep = 0,
    /// Clear the presented region before decoding the next frame.
    Clear = 1,
    /// Restore the canvas state captured before this frame was applied.
    RestorePrevious = 2,
}

impl DisposalMode {
    pub(super) fn open(value: u8) -> Result<Self, FrameSequenceError> {
        match value {
            0 => Ok(Self::Keep),
            1 => Ok(Self::Clear),
            2 => Ok(Self::RestorePrevious),
            _ => Err(FrameSequenceError::UnknownDisposalMode(value)),
        }
    }
}

/// Shared timing, composition, and seek bounds for one FRAMES timeline.
///
/// Per-frame sections only store values that differ from these defaults.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameSequence {
    frame_count: u32,
    timescale_hz: u32,
    default_duration_ticks: u32,
    play_count: u32,
    max_delta_frames: u16,
    default_blend: BlendMode,
    default_disposal: DisposalMode,
}

impl FrameSequence {
    pub fn new(
        frame_count: u32,
        timescale_hz: u32,
        default_duration_ticks: u32,
    ) -> Result<Self, FrameSequenceError> {
        let sequence = Self {
            frame_count,
            timescale_hz,
            default_duration_ticks,
            play_count: 0,
            max_delta_frames: 0,
            default_blend: BlendMode::Replace,
            default_disposal: DisposalMode::Keep,
        };
        sequence.validate()?;
        Ok(sequence)
    }

    pub const fn frame_count(self) -> u32 {
        self.frame_count
    }

    pub const fn timescale_hz(self) -> u32 {
        self.timescale_hz
    }

    pub const fn default_duration_ticks(self) -> u32 {
        self.default_duration_ticks
    }

    /// Zero means unbounded repetition; a nonzero value is the total play count.
    pub const fn play_count(self) -> u32 {
        self.play_count
    }

    /// Maximum number of dependent frames between independently recoverable frames.
    pub const fn max_delta_frames(self) -> u16 {
        self.max_delta_frames
    }

    pub const fn default_blend(self) -> BlendMode {
        self.default_blend
    }

    pub const fn default_disposal(self) -> DisposalMode {
        self.default_disposal
    }

    pub const fn with_play_count(mut self, play_count: u32) -> Self {
        self.play_count = play_count;
        self
    }

    pub fn with_max_delta_frames(
        mut self,
        max_delta_frames: u16,
    ) -> Result<Self, FrameSequenceError> {
        self.max_delta_frames = max_delta_frames;
        self.validate()?;
        Ok(self)
    }

    pub const fn with_default_composition(
        mut self,
        blend: BlendMode,
        disposal: DisposalMode,
    ) -> Self {
        self.default_blend = blend;
        self.default_disposal = disposal;
        self
    }

    /// Reads one complete canonical record without requiring aligned storage.
    pub fn open(bytes: &[u8]) -> Result<Self, FrameSequenceError> {
        let bytes =
            bytes
                .get(..FRAME_SEQUENCE_RECORD_LEN)
                .ok_or(FrameSequenceError::Truncated {
                    needed: FRAME_SEQUENCE_RECORD_LEN,
                    available: bytes.len(),
                })?;
        let sequence = Self {
            frame_count: read_u32_le(bytes, 0).expect("validated sequence record"),
            timescale_hz: read_u32_le(bytes, 4).expect("validated sequence record"),
            default_duration_ticks: read_u32_le(bytes, 8).expect("validated sequence record"),
            play_count: read_u32_le(bytes, 12).expect("validated sequence record"),
            max_delta_frames: read_u16_le(bytes, 16).expect("validated sequence record"),
            default_blend: BlendMode::open(bytes[18])?,
            default_disposal: DisposalMode::open(bytes[19])?,
        };
        sequence.validate()?;
        Ok(sequence)
    }

    pub fn encode_record(self) -> [u8; FRAME_SEQUENCE_RECORD_LEN] {
        let mut bytes = [0; FRAME_SEQUENCE_RECORD_LEN];
        write_u32_le(&mut bytes, 0, self.frame_count);
        write_u32_le(&mut bytes, 4, self.timescale_hz);
        write_u32_le(&mut bytes, 8, self.default_duration_ticks);
        write_u32_le(&mut bytes, 12, self.play_count);
        write_u16_le(&mut bytes, 16, self.max_delta_frames);
        bytes[18] = self.default_blend as u8;
        bytes[19] = self.default_disposal as u8;
        bytes
    }

    fn validate(self) -> Result<(), FrameSequenceError> {
        if self.frame_count == 0 {
            return Err(FrameSequenceError::Empty);
        }
        if !(1..=MAX_TIMESCALE_HZ).contains(&self.timescale_hz) {
            return Err(FrameSequenceError::InvalidTimescale(self.timescale_hz));
        }
        if self.default_duration_ticks == 0 {
            return Err(FrameSequenceError::ZeroDefaultDuration);
        }
        if u32::from(self.max_delta_frames) >= self.frame_count {
            return Err(FrameSequenceError::InvalidDeltaBound {
                frame_count: self.frame_count,
                max_delta_frames: self.max_delta_frames,
            });
        }
        Ok(())
    }
}

/// Failure while constructing or reading shared FRAMES sequence state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameSequenceError {
    Truncated {
        needed: usize,
        available: usize,
    },
    Empty,
    InvalidTimescale(u32),
    ZeroDefaultDuration,
    InvalidDeltaBound {
        frame_count: u32,
        max_delta_frames: u16,
    },
    UnknownBlendMode(u8),
    UnknownDisposalMode(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_record_roundtrips_all_shared_state() {
        let sequence = FrameSequence::new(300, 1_000, 40)
            .unwrap()
            .with_play_count(3)
            .with_max_delta_frames(15)
            .unwrap()
            .with_default_composition(BlendMode::SourceOver, DisposalMode::Clear);
        let bytes = sequence.encode_record();
        assert_eq!(bytes.len(), FRAME_SEQUENCE_RECORD_LEN);
        assert_eq!(FrameSequence::open(&bytes), Ok(sequence));
        assert_eq!(sequence.frame_count(), 300);
        assert_eq!(sequence.timescale_hz(), 1_000);
        assert_eq!(sequence.default_duration_ticks(), 40);
        assert_eq!(sequence.play_count(), 3);
        assert_eq!(sequence.max_delta_frames(), 15);
        assert_eq!(sequence.default_blend(), BlendMode::SourceOver);
        assert_eq!(sequence.default_disposal(), DisposalMode::Clear);
    }

    #[test]
    fn invalid_values_are_rejected_before_emission() {
        assert_eq!(FrameSequence::new(0, 1, 1), Err(FrameSequenceError::Empty));
        assert_eq!(
            FrameSequence::new(1, 0, 1),
            Err(FrameSequenceError::InvalidTimescale(0))
        );
        assert_eq!(
            FrameSequence::new(1, 1, 0),
            Err(FrameSequenceError::ZeroDefaultDuration)
        );
        assert_eq!(
            FrameSequence::new(2, 1, 1)
                .unwrap()
                .with_max_delta_frames(2),
            Err(FrameSequenceError::InvalidDeltaBound {
                frame_count: 2,
                max_delta_frames: 2,
            })
        );
    }

    #[test]
    fn unknown_composition_and_truncation_are_rejected() {
        let bytes = FrameSequence::new(2, 60, 1).unwrap().encode_record();
        for end in 0..FRAME_SEQUENCE_RECORD_LEN {
            assert_eq!(
                FrameSequence::open(&bytes[..end]),
                Err(FrameSequenceError::Truncated {
                    needed: FRAME_SEQUENCE_RECORD_LEN,
                    available: end,
                })
            );
        }
        let mut unknown = bytes;
        unknown[18] = 9;
        assert_eq!(
            FrameSequence::open(&unknown),
            Err(FrameSequenceError::UnknownBlendMode(9))
        );
        unknown = bytes;
        unknown[19] = 9;
        assert_eq!(
            FrameSequence::open(&unknown),
            Err(FrameSequenceError::UnknownDisposalMode(9))
        );
    }
}
