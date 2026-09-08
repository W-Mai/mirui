use super::FramesView;

/// One resolved presentation position inside a frame timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramePosition {
    frame: u32,
    play: u64,
    elapsed_ticks: u32,
}

impl FramePosition {
    pub const fn frame(self) -> u32 {
        self.frame
    }

    /// Zero-based completed play count before this position.
    pub const fn play(self) -> u64 {
        self.play
    }

    pub const fn elapsed_ticks(self) -> u32 {
        self.elapsed_ticks
    }
}

/// Allocation-free time lookup over canonical frame durations and play count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameTimeline<'a> {
    frames: FramesView<'a>,
}

impl<'a> FramesView<'a> {
    pub const fn timeline(self) -> FrameTimeline<'a> {
        FrameTimeline { frames: self }
    }
}

impl FrameTimeline<'_> {
    pub const fn timescale_hz(self) -> u32 {
        self.frames.sequence().timescale_hz()
    }

    pub const fn frame_count(self) -> u32 {
        self.frames.sequence().frame_count()
    }

    pub const fn play_count(self) -> u32 {
        self.frames.sequence().play_count()
    }

    pub fn duration_ticks(self, frame: u32) -> Option<u32> {
        if frame >= self.frame_count() {
            return None;
        }
        self.frames.timing().map_or_else(
            || Some(self.frames.sequence().default_duration_ticks()),
            |timing| timing.duration(frame),
        )
    }

    /// Duration of one complete play through every frame.
    pub fn cycle_duration_ticks(self) -> u64 {
        self.frames.timing().map_or_else(
            || {
                u64::from(self.frame_count())
                    * u64::from(self.frames.sequence().default_duration_ticks())
            },
            |timing| timing.cycle_duration_ticks(),
        )
    }

    /// Total finite duration; `None` denotes unbounded repetition.
    pub fn total_duration_ticks(self) -> Option<u128> {
        let plays = self.play_count();
        (plays != 0).then(|| u128::from(self.cycle_duration_ticks()) * u128::from(plays))
    }

    /// Locates one absolute tick without allocating or materializing a table.
    ///
    /// The start is inclusive and the end of a finite timeline is exclusive.
    /// Unbounded timelines retain the complete play ordinal as `u64`.
    pub fn locate(self, elapsed_ticks: u64) -> Option<FramePosition> {
        let cycle_ticks = self.cycle_duration_ticks();
        let play = elapsed_ticks / cycle_ticks;
        if self.play_count() != 0 && play >= u64::from(self.play_count()) {
            return None;
        }
        let within_cycle = elapsed_ticks % cycle_ticks;
        let (frame, elapsed_ticks) = self.frames.timing().map_or_else(
            || {
                let duration = u64::from(self.frames.sequence().default_duration_ticks());
                Some((
                    u32::try_from(within_cycle / duration)
                        .expect("cycle position fits frame ordinal"),
                    (within_cycle % duration) as u32,
                ))
            },
            |timing| timing.locate(within_cycle),
        )?;
        Some(FramePosition {
            frame,
            play,
            elapsed_ticks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::{FrameEncodingSet, FrameSequence, FramesEncoder};
    use crate::{
        PayloadLimits,
        image::{ColorDescription, SampleLayout, SurfaceDescriptor},
    };

    fn timeline(play_count: u32) -> alloc::vec::Vec<u8> {
        let sequence = FrameSequence::new(3, 1_000, 40)
            .unwrap()
            .with_play_count(play_count);
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(FrameEncodingSet::lossless().with_delta(false))
            .unwrap();
        encoder.push_with_duration(&[1], 40).unwrap();
        encoder.push_with_duration(&[2], 75).unwrap();
        encoder.push_with_duration(&[3], 20).unwrap();
        encoder.finish().unwrap().into_payload()
    }

    #[test]
    fn finite_variable_timeline_uses_half_open_frame_and_play_boundaries() {
        let bytes = timeline(2);
        let timeline = FramesView::open(&bytes, &PayloadLimits::HOST)
            .unwrap()
            .timeline();
        assert_eq!(timeline.timescale_hz(), 1_000);
        assert_eq!(timeline.cycle_duration_ticks(), 135);
        assert_eq!(timeline.total_duration_ticks(), Some(270));
        assert_eq!(
            timeline.locate(0),
            Some(FramePosition {
                frame: 0,
                play: 0,
                elapsed_ticks: 0,
            })
        );
        assert_eq!(timeline.locate(39).unwrap().frame(), 0);
        assert_eq!(
            timeline.locate(40),
            Some(FramePosition {
                frame: 1,
                play: 0,
                elapsed_ticks: 0,
            })
        );
        assert_eq!(timeline.locate(114).unwrap().elapsed_ticks(), 74);
        assert_eq!(timeline.locate(115).unwrap().frame(), 2);
        assert_eq!(timeline.locate(134).unwrap().elapsed_ticks(), 19);
        assert_eq!(
            timeline.locate(135),
            Some(FramePosition {
                frame: 0,
                play: 1,
                elapsed_ticks: 0,
            })
        );
        assert_eq!(timeline.locate(269).unwrap().frame(), 2);
        assert_eq!(timeline.locate(270), None);
    }

    #[test]
    fn unbounded_timeline_keeps_large_play_ordinals() {
        let bytes = timeline(0);
        let timeline = FramesView::open(&bytes, &PayloadLimits::HOST)
            .unwrap()
            .timeline();
        assert_eq!(timeline.total_duration_ticks(), None);
        let position = timeline.locate(u64::MAX).unwrap();
        assert_eq!(position.play(), u64::MAX / 135);
        assert_eq!(position.elapsed_ticks(), 65);
        assert_eq!(position.frame(), 1);
    }
}
