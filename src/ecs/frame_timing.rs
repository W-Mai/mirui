/// Outermost App operation owning a timing sample.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum FrameOperation {
    #[default]
    Tick,
    Update,
    Draw,
    Maintenance,
}

/// Recorded phases within an App operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FramePhase {
    Input,
    Systems,
    Layout,
    Render,
    Flush,
    SeedPrev,
}

/// A fixed set of recorded phases. Empty and absent phases are distinct from measured zero duration.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FramePhases(u8);

impl FramePhases {
    pub const fn with(self, phase: FramePhase) -> Self {
        Self(self.0 | (1 << phase as u8))
    }

    pub const fn contains(self, phase: FramePhase) -> bool {
        self.0 & (1 << phase as u8) != 0
    }
}

/// Phase wall times for one closed operation, including failed attempts and excluding work outside these six phases. Values include blocking and instrumentation within each phase; their sum is neither CPU time nor a display interval.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTimings {
    values: [u64; 6],
    phases: FramePhases,
    operation: FrameOperation,
    clock_valid: bool,
}

impl FrameTimings {
    pub(crate) fn new(operation: FrameOperation, clock_valid: bool) -> Self {
        Self {
            operation,
            clock_valid,
            ..Self::default()
        }
    }

    pub fn operation(&self) -> FrameOperation {
        self.operation
    }
    pub fn phases(&self) -> FramePhases {
        self.phases
    }
    pub fn clock_valid(&self) -> bool {
        self.clock_valid
    }
    pub fn has_phase(&self, phase: FramePhase) -> bool {
        self.phases.contains(phase)
    }

    pub fn phase_nanos(&self, phase: FramePhase) -> Option<u64> {
        (self.clock_valid && self.has_phase(phase)).then_some(self.values[phase as usize])
    }

    pub fn phase_wall_nanos(&self) -> Option<u64> {
        if !self.clock_valid || self.phases.0 == 0 {
            return None;
        }
        self.values
            .iter()
            .try_fold(0u64, |sum, &value| sum.checked_add(value))
    }

    pub fn input_nanos(&self) -> Option<u64> {
        self.phase_nanos(FramePhase::Input)
    }
    pub fn systems_nanos(&self) -> Option<u64> {
        self.phase_nanos(FramePhase::Systems)
    }
    pub fn layout_nanos(&self) -> Option<u64> {
        self.phase_nanos(FramePhase::Layout)
    }
    pub fn render_nanos(&self) -> Option<u64> {
        self.phase_nanos(FramePhase::Render)
    }
    pub fn flush_nanos(&self) -> Option<u64> {
        self.phase_nanos(FramePhase::Flush)
    }
    pub fn seed_prev_nanos(&self) -> Option<u64> {
        self.phase_nanos(FramePhase::SeedPrev)
    }

    pub(crate) fn invalidate_clock(&mut self) {
        self.clock_valid = false;
    }

    pub(crate) fn record(&mut self, phase: FramePhase, start: Option<u64>, end: Option<u64>) {
        self.phases = self.phases.with(phase);
        let elapsed = start.zip(end).and_then(|(a, b)| b.checked_sub(a));
        let value = &mut self.values[phase as usize];
        if let Some(total) = elapsed.and_then(|elapsed| value.checked_add(elapsed)) {
            *value = total;
        } else {
            self.clock_valid = false;
        }
    }

    fn tag(&self) -> u8 {
        ((self.operation as u8) << 6) | self.phases.0
    }
}

/// Result of an App operation; accepted presentation is independent of physical display completion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum TimingOutcome {
    Completed,
    #[default]
    Idle,
    Suspended,
    Quit,
    Failed,
}

/// Closed operation wall time before reporting and pacing. Instrumentation within the operation remains included. Missing CPU timing is unavailable, not zero.
pub struct TimingSample<'a> {
    pub operation: FrameOperation,
    pub wall_nanos: Option<u64>,
    pub thread_cpu_nanos: Option<u64>,
    pub outcome: TimingOutcome,
    pub phases: &'a FrameTimings,
    /// End timestamp in this App's operation clock domain, not a platform presentation timestamp.
    pub finished_at_ns: Option<u64>,
}

/// Statistics for samples with exactly the same operation and phase set.
#[derive(Clone, Copy, Debug)]
pub struct PhaseStatistics {
    pub count: usize,
    pub avg_nanos: u64,
    pub min_nanos: u64,
    pub max_nanos: u64,
    pub p99_nanos: u64,
}

impl PhaseStatistics {
    pub fn jitter_nanos(&self) -> u64 {
        self.max_nanos - self.min_nanos
    }
}

const CAP: usize = 256;

/// Recent valid, completed phase budgets. One ring retains at most 256 operations; queries select an exact operation and phase set instead of mixing update and draw samples.
pub struct FrameStats {
    samples: [u64; CAP],
    tags: [u8; CAP],
    head: usize,
    len: usize,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self {
            samples: [0; CAP],
            tags: [0; CAP],
            head: 0,
            len: 0,
        }
    }
}

impl FrameStats {
    pub(crate) fn push(&mut self, timings: &FrameTimings) {
        let Some(value) = timings.phase_wall_nanos() else {
            return;
        };
        self.samples[self.head] = value;
        self.tags[self.head] = timings.tag();
        self.head = (self.head + 1) % CAP;
        self.len = (self.len + 1).min(CAP);
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn summary(
        &self,
        operation: FrameOperation,
        phases: FramePhases,
    ) -> Option<PhaseStatistics> {
        let tag = ((operation as u8) << 6) | phases.0;
        let mut count = 0usize;
        let mut sum = 0u128;
        let mut min = u64::MAX;
        // At most 256 samples: nearest-rank p99 is among the largest three.
        let mut largest = [0u64; 3];
        for (&value, &sample_tag) in self.samples[..self.len].iter().zip(&self.tags[..self.len]) {
            if sample_tag != tag {
                continue;
            }
            count += 1;
            sum += u128::from(value);
            min = min.min(value);
            let mut candidate = value;
            for slot in &mut largest {
                if candidate > *slot {
                    core::mem::swap(&mut candidate, slot);
                }
            }
        }
        if count == 0 {
            return None;
        }
        let rank_from_top = count - (count * 99).div_ceil(100);
        Some(PhaseStatistics {
            count,
            avg_nanos: (sum / count as u128) as u64,
            min_nanos: min,
            max_nanos: largest[0],
            p99_nanos: largest[rank_from_top],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(kind: FrameOperation, phase: FramePhase, value: u64) -> FrameTimings {
        let mut sample = FrameTimings::new(kind, true);
        sample.record(phase, Some(0), Some(value));
        sample
    }

    #[test]
    fn absent_zero_and_invalid_are_distinct() {
        let mut t = sample(FrameOperation::Draw, FramePhase::Render, 0);
        assert_eq!(t.render_nanos(), Some(0));
        assert_eq!(t.input_nanos(), None);
        assert_eq!(t.phase_wall_nanos(), Some(0));
        t.record(FramePhase::Flush, Some(2), Some(1));
        assert!(t.has_phase(FramePhase::Flush));
        assert!(!t.clock_valid());
        assert_eq!(t.render_nanos(), None);
        assert_eq!(t.phase_wall_nanos(), None);
    }

    #[test]
    fn ring_filters_scope_and_phase_set_without_allocating() {
        let mut ring = FrameStats::default();
        let allocations = crate::test_support::tracked_allocations(|| {
            for value in 1..=300 {
                ring.push(&sample(FrameOperation::Draw, FramePhase::Render, value));
            }
            ring.summary(
                FrameOperation::Draw,
                FramePhases::default().with(FramePhase::Render),
            );
        });
        assert_eq!(allocations, 0);
        let s = ring
            .summary(
                FrameOperation::Draw,
                FramePhases::default().with(FramePhase::Render),
            )
            .unwrap();
        assert_eq!(
            (s.count, s.min_nanos, s.max_nanos, s.avg_nanos, s.p99_nanos),
            (256, 45, 300, 172, 298)
        );
        assert!(
            ring.summary(
                FrameOperation::Update,
                FramePhases::default().with(FramePhase::Render)
            )
            .is_none()
        );
        assert!(
            ring.summary(
                FrameOperation::Draw,
                FramePhases::default().with(FramePhase::Flush)
            )
            .is_none()
        );
    }

    #[test]
    fn nearest_rank_matches_sorted_reference_for_every_window_length() {
        let phases = FramePhases::default().with(FramePhase::Render);
        let mut ring = FrameStats::default();
        let mut reference = alloc::vec::Vec::new();
        for i in 0..256u64 {
            let value = (i * 47) % 101;
            ring.push(&sample(FrameOperation::Draw, FramePhase::Render, value));
            reference.push(value);
            reference.sort_unstable();
            let rank = (reference.len() * 99).div_ceil(100) - 1;
            assert_eq!(
                ring.summary(FrameOperation::Draw, phases)
                    .unwrap()
                    .p99_nanos,
                reference[rank]
            );
        }
    }
}
