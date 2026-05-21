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
