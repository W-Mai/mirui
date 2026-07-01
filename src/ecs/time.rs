/// Delta time in integer milliseconds — avoids floating point in
/// animation hot paths on targets without an FPU.
pub struct DeltaTimeMs(pub u16);

/// Per-frame timing breakdown written by `App::run` once per loop
/// iteration when a `MonoClock` is installed. All values are nanoseconds
/// for the most recently completed frame; zero when no `MonoClock` is
/// present.
///
/// `frame_nanos` is the full loop iteration; the other fields are
/// disjoint subsets that sum to it. `input_nanos` covers the entire
/// "consume input" phase (backend `poll_event`, plugin `on_event`,
/// `dispatch_input`, long-press scan, gesture bubble dispatch), not
/// just `Surface::poll_event`. `seed_prev_nanos` only advances on the
/// full-render path; dirty-render frames leave it zero.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTimings {
    pub frame_nanos: u64,
    pub input_nanos: u64,
    pub systems_nanos: u64,
    pub layout_nanos: u64,
    pub render_nanos: u64,
    pub flush_nanos: u64,
    pub seed_prev_nanos: u64,
}

/// Sliding window of recent `frame_nanos` values for jitter and tail
/// latency analysis. `App::run` pushes one entry per frame; reading
/// stays cheap (zero allocations after the first 256 frames).
///
/// 256 samples ≈ 3.4 s at 75 fps, ≈ 14 s at 18 fps — plenty to surface
/// stutter without occupying meaningful RAM (2 KB).
pub struct FrameStats {
    samples: [u64; FRAME_STATS_CAP],
    head: usize,
    len: usize,
}

const FRAME_STATS_CAP: usize = 256;

impl Default for FrameStats {
    fn default() -> Self {
        Self {
            samples: [0; FRAME_STATS_CAP],
            head: 0,
            len: 0,
        }
    }
}

impl FrameStats {
    /// Append a frame's `frame_nanos`.
    pub fn push(&mut self, frame_nanos: u64) {
        self.samples[self.head] = frame_nanos;
        self.head = (self.head + 1) % FRAME_STATS_CAP;
        if self.len < FRAME_STATS_CAP {
            self.len += 1;
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        let len = self.len;
        let cap = FRAME_STATS_CAP;
        let start = if len < cap { 0 } else { self.head };
        (0..len).map(move |i| self.samples[(start + i) % cap])
    }

    pub fn avg(&self) -> u64 {
        if self.len == 0 {
            return 0;
        }
        let sum: u64 = self.iter().sum();
        sum / self.len as u64
    }

    pub fn min(&self) -> u64 {
        self.iter().min().unwrap_or(0)
    }

    pub fn max(&self) -> u64 {
        self.iter().max().unwrap_or(0)
    }

    /// Approximate p99: the (n-1)th value of the bottom 99% in a sorted
    /// view. With 256 samples that's the 254th-smallest. Allocates a
    /// `Vec` for sorting; cheap because called by reporter plugins
    /// once per N frames, not per frame.
    pub fn p99(&self) -> u64 {
        if self.len == 0 {
            return 0;
        }
        let mut sorted: alloc::vec::Vec<u64> = self.iter().collect();
        sorted.sort_unstable();
        let idx = ((self.len * 99).div_ceil(100) - 1).min(self.len - 1);
        sorted[idx]
    }

    pub fn jitter(&self) -> u64 {
        self.max().saturating_sub(self.min())
    }
}

/// Global monotonic clock resource. Single time source for the entire
/// App — animation, gesture recognition, simulated input, render
/// timing all read from this.
///
/// `clock` is a fn pointer returning nanoseconds since an arbitrary
/// epoch (typically app init). Plugins set it: `StdInstantClockPlugin`
/// on desktop, `SystimerClockPlugin` on ESP.
pub struct MonoClock {
    pub clock: fn() -> u64,
    pub last_ns: u64,
}

impl MonoClock {
    pub fn new(clock: fn() -> u64) -> Self {
        let now = clock();
        Self {
            clock,
            last_ns: now,
        }
    }

    /// Reads `core::time::clock_now_ns` so ECS timestamps stay in
    /// sync with `mirui::info!` and `trace_span!`.
    pub fn from_time_source() -> Self {
        Self::new(crate::core::time::clock_now_ns)
    }

    pub fn now_ns(&self) -> u64 {
        (self.clock)()
    }

    pub fn now_ms(&self) -> u32 {
        (self.now_ns() / 1_000_000) as u32
    }
}

#[cfg(test)]
mod frame_stats_tests {
    use super::*;

    #[test]
    fn empty_window_reports_zero() {
        let s = FrameStats::default();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
        assert_eq!(s.min(), 0);
        assert_eq!(s.max(), 0);
        assert_eq!(s.p99(), 0);
        assert_eq!(s.jitter(), 0);
    }

    #[test]
    fn small_window_min_max_jitter() {
        let mut s = FrameStats::default();
        s.push(10);
        s.push(20);
        s.push(15);
        assert_eq!(s.len(), 3);
        assert_eq!(s.min(), 10);
        assert_eq!(s.max(), 20);
        assert_eq!(s.jitter(), 10);
        assert_eq!(s.p99(), 20);
    }

    #[test]
    fn p99_with_two_samples_returns_max() {
        let mut s = FrameStats::default();
        s.push(5);
        s.push(50);
        assert_eq!(s.p99(), 50);
    }

    #[test]
    fn p99_with_one_sample_returns_that_sample() {
        let mut s = FrameStats::default();
        s.push(42);
        assert_eq!(s.p99(), 42);
    }

    #[test]
    fn p99_full_window_picks_254th_smallest() {
        let mut s = FrameStats::default();
        for v in 1..=256u64 {
            s.push(v);
        }
        assert_eq!(s.len(), 256);
        assert_eq!(s.p99(), 254);
    }

    #[test]
    fn ring_drops_oldest_after_capacity() {
        let mut s = FrameStats::default();
        for v in 1..=256u64 {
            s.push(v);
        }
        s.push(999);
        assert_eq!(s.len(), 256);
        assert_eq!(s.min(), 2);
        assert_eq!(s.max(), 999);
    }

    #[test]
    fn avg_matches_arithmetic_mean() {
        let mut s = FrameStats::default();
        s.push(10);
        s.push(20);
        s.push(30);
        assert_eq!(s.avg(), 20);
    }
}
