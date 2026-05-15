/// Delta time in integer milliseconds — avoids floating point in
/// animation hot paths on targets without an FPU.
pub struct DeltaTimeMs(pub u16);

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
