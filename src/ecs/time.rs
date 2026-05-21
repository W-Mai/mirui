/// Delta time in integer milliseconds — avoids floating point in
/// animation hot paths on targets without an FPU.
pub struct DeltaTimeMs(pub u16);

/// Per-frame timing breakdown written by `App::run` once per loop
/// iteration when a `MonoClock` is installed. All values are nanoseconds
/// for the most recently completed frame; zero when no `MonoClock` is
/// present.
///
/// Stages cover the entire `App::run` iteration:
///
/// ```text
/// frame_nanos = event_poll + systems + layout + render + flush + seed_prev
/// ```
///
/// `seed_prev` only advances on the full-render path; on dirty-render
/// frames it stays zero. Plugins read this via `world.resource()`.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameTimings {
    pub frame_nanos: u64,
    pub event_poll_nanos: u64,
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
    /// Most recent frames first index (head). Underlying storage is a
    /// fixed-size array so we don't allocate on the hot path.
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

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
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
        let idx = (self.len * 99 / 100).saturating_sub(1).min(self.len - 1);
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

    pub fn now_ns(&self) -> u64 {
        (self.clock)()
    }

    pub fn now_ms(&self) -> u32 {
        (self.now_ns() / 1_000_000) as u32
    }
}

/// Test-only fake clock. Drives `MonoClock` from a global mutex so a
/// `fn() -> u64` clock pointer can read it. Tests must run serially
/// when using this — pass `--test-threads=1` to `cargo test` for any
/// suite that uses sim_timeline / mock-clock-driven tests.
///
/// Usage:
///   `world.insert_resource(MonoClock::new(mock::clock_fn));`
///   `mock::set_ns(0);` ... drive system ... `mock::advance_ms(800);`
#[cfg(any(test, feature = "std"))]
pub mod mock {
    extern crate std;
    use std::sync::Mutex;

    static MOCK_NS: Mutex<u64> = Mutex::new(0);

    pub fn clock_fn() -> u64 {
        *MOCK_NS.lock().expect("mock clock poisoned")
    }

    pub fn set_ns(ns: u64) {
        *MOCK_NS.lock().expect("mock clock poisoned") = ns;
    }

    pub fn set_ms(ms: u64) {
        set_ns(ms.saturating_mul(1_000_000));
    }

    pub fn advance_ms(ms: u64) {
        let mut guard = MOCK_NS.lock().expect("mock clock poisoned");
        *guard = guard.saturating_add(ms.saturating_mul(1_000_000));
    }

    /// Acquire a guard that any test using the mock clock should hold
    /// for its full duration. Tests using this guard run serially
    /// without `--test-threads=1` because they all contend on the
    /// same mutex.
    pub fn lock() -> std::sync::MutexGuard<'static, ()> {
        static SERIAL: Mutex<()> = Mutex::new(());
        SERIAL.lock().unwrap_or_else(|p| p.into_inner())
    }
}
