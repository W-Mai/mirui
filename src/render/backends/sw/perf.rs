pub struct PerfCtx {
    pub clock: fn() -> u64,
    pub fill: u64,
    pub stroke: u64,
    pub blit: u64,
    pub label: u64,
    pub count_fill: u32,
    pub count_stroke: u32,
    pub count_blit: u32,
    pub count_label: u32,
}

impl PerfCtx {
    pub fn new(clock: fn() -> u64) -> Self {
        Self {
            clock,
            fill: 0,
            stroke: 0,
            blit: 0,
            label: 0,
            count_fill: 0,
            count_stroke: 0,
            count_blit: 0,
            count_label: 0,
        }
    }

    pub fn reset(&mut self) {
        self.fill = 0;
        self.stroke = 0;
        self.blit = 0;
        self.label = 0;
        self.count_fill = 0;
        self.count_stroke = 0;
        self.count_blit = 0;
        self.count_label = 0;
    }
}

/// Global perf counters for quad-path drawing. Not thread-safe (plain
/// `static mut`), which matches the single-threaded embedded targets mirui
/// runs on. Off-by-default: zero cost when `perf` feature is off.
pub mod quad_perf {
    pub static mut FILL: u64 = 0;
    pub static mut BLIT: u64 = 0;
    pub static mut FILL_COUNT: u32 = 0;
    pub static mut BLIT_COUNT: u32 = 0;

    /// Per-pixel breakdown for fill_rect_quad bbox scan.
    pub static mut FILL_PIXELS_SCANNED: u64 = 0;
    pub static mut FILL_PIXELS_DRAWN: u64 = 0;
    pub static mut FILL_PIXELS_INSET_HIT: u64 = 0;
    pub static mut FILL_PIXELS_SLOW_HIT: u64 = 0;

    /// Per-pixel breakdown for blit_quad.
    pub static mut BLIT_PIXELS_SCANNED: u64 = 0;
    pub static mut BLIT_PIXELS_DRAWN: u64 = 0;

    /// User supplies a clock reading monotonic ticks. Demo points it at
    /// e.g. ESP systimer (cycles) or std Instant (ns) — caller decides
    /// units and interprets the output accordingly.
    pub static mut CLOCK: fn() -> u64 = || 0;

    pub struct Snapshot {
        pub fill_ticks: u64,
        pub fill_count: u32,
        pub blit_ticks: u64,
        pub blit_count: u32,
        pub fill_scanned: u64,
        pub fill_drawn: u64,
        pub fill_inset_hit: u64,
        pub fill_slow_hit: u64,
        pub blit_scanned: u64,
        pub blit_drawn: u64,
    }

    pub fn take() -> Snapshot {
        unsafe {
            let out = Snapshot {
                fill_ticks: FILL,
                fill_count: FILL_COUNT,
                blit_ticks: BLIT,
                blit_count: BLIT_COUNT,
                fill_scanned: FILL_PIXELS_SCANNED,
                fill_drawn: FILL_PIXELS_DRAWN,
                fill_inset_hit: FILL_PIXELS_INSET_HIT,
                fill_slow_hit: FILL_PIXELS_SLOW_HIT,
                blit_scanned: BLIT_PIXELS_SCANNED,
                blit_drawn: BLIT_PIXELS_DRAWN,
            };
            FILL = 0;
            BLIT = 0;
            FILL_COUNT = 0;
            BLIT_COUNT = 0;
            FILL_PIXELS_SCANNED = 0;
            FILL_PIXELS_DRAWN = 0;
            FILL_PIXELS_INSET_HIT = 0;
            FILL_PIXELS_SLOW_HIT = 0;
            BLIT_PIXELS_SCANNED = 0;
            BLIT_PIXELS_DRAWN = 0;
            out
        }
    }

    #[inline]
    pub fn now() -> u64 {
        unsafe { CLOCK() }
    }

    #[inline]
    pub fn add_fill(dt: u64) {
        unsafe {
            FILL += dt;
            FILL_COUNT += 1;
        }
    }

    #[inline]
    pub fn add_blit(dt: u64) {
        unsafe {
            BLIT += dt;
            BLIT_COUNT += 1;
        }
    }
}
