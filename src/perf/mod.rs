//! Span-based perf tracing. Use [`crate::trace_span!`] /
//! `#[crate::trace_fn]`; this module is the storage layer.
//!
//! On `std` each [`enter`] guard records `(name, start_ns, end_ns,
//! depth)` into a thread-local `Vec` for chrome-JSON or per-name
//! aggregation. On `no_std` the API still compiles but is a no-op
//! pending count / total / last accumulators.

#[cfg(feature = "std")]
mod imp {
    extern crate std;
    use std::cell::RefCell;
    use std::time::Instant;
    use std::vec::Vec;

    thread_local! {
        static EVENTS: RefCell<Vec<PerfEvent>> = RefCell::new(Vec::with_capacity(2048));
        static DEPTH: RefCell<u8> = const { RefCell::new(0) };
        static EPOCH: RefCell<Option<Instant>> = const { RefCell::new(None) };
    }

    fn now_ns() -> u64 {
        EPOCH.with(|e| {
            let mut slot = e.borrow_mut();
            let inst = slot.get_or_insert_with(Instant::now);
            inst.elapsed().as_nanos() as u64
        })
    }

    #[derive(Clone, Copy)]
    pub struct PerfEvent {
        pub name: &'static str,
        pub start_ns: u64,
        pub end_ns: u64,
        pub depth: u8,
    }

    pub struct Guard {
        name: &'static str,
        start_ns: u64,
        depth: u8,
    }

    impl Guard {
        fn new(name: &'static str) -> Self {
            let depth = DEPTH.with(|d| {
                let cur = *d.borrow();
                *d.borrow_mut() = cur.saturating_add(1);
                cur
            });
            Self {
                name,
                start_ns: now_ns(),
                depth,
            }
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            let end_ns = now_ns();
            DEPTH.with(|d| {
                let cur = *d.borrow();
                *d.borrow_mut() = cur.saturating_sub(1);
            });
            EVENTS.with(|e| {
                e.borrow_mut().push(PerfEvent {
                    name: self.name,
                    start_ns: self.start_ns,
                    end_ns,
                    depth: self.depth,
                });
            });
        }
    }

    pub fn enter(name: &'static str) -> Guard {
        Guard::new(name)
    }

    pub fn drain_events() -> Vec<PerfEvent> {
        EVENTS.with(|e| core::mem::take(&mut *e.borrow_mut()))
    }
}

#[cfg(not(feature = "std"))]
mod imp {
    pub struct PerfEvent {
        pub name: &'static str,
        pub start_ns: u64,
        pub end_ns: u64,
        pub depth: u8,
    }

    pub struct Guard;

    pub fn enter(_name: &'static str) -> Guard {
        Guard
    }

    pub fn drain_events() -> alloc::vec::Vec<PerfEvent> {
        alloc::vec::Vec::new()
    }
}

pub use imp::{Guard, PerfEvent, drain_events, enter};

/// Span macros — re-exported from `mirui-macros` so callers can
/// write `mirui::trace_span!("...")` / `#[mirui::trace_fn("...")]`
/// without depending on the macro crate directly.
///
/// - `trace_span!("name")` — RAII statement form, guard lives until
///   the end of the enclosing scope. Multiple calls in the same
///   scope each get a unique mangled binding so they don't shadow.
/// - `trace_span!("name", { ... })` — block-expression form,
///   evaluates to the block's value.
/// - `#[trace_fn("name")]` — wraps an entire fn body in a guard.
pub use mirui_macros::{trace_fn, trace_span};

/// Aggregate of a single span name across one or more events. Useful
/// for console summaries; chrome JSON consumers want the raw
/// [`PerfEvent`]s instead.
#[derive(Clone, Copy, Default)]
pub struct StageStat {
    pub name: &'static str,
    pub count: u32,
    pub total_ns: u64,
    pub last_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
}

/// Aggregate a slice of events by `name`. O(n²) in distinct names but
/// distinct names are bounded by the number of `trace_span!` call
/// sites, which stays small.
#[cfg(feature = "std")]
pub fn aggregate(events: &[PerfEvent]) -> alloc::vec::Vec<StageStat> {
    let mut out: alloc::vec::Vec<StageStat> = alloc::vec::Vec::new();
    for ev in events {
        let dur = ev.end_ns.saturating_sub(ev.start_ns);
        if let Some(s) = out.iter_mut().find(|s| s.name == ev.name) {
            s.count += 1;
            s.total_ns += dur;
            s.last_ns = dur;
            if dur < s.min_ns {
                s.min_ns = dur;
            }
            if dur > s.max_ns {
                s.max_ns = dur;
            }
        } else {
            out.push(StageStat {
                name: ev.name,
                count: 1,
                total_ns: dur,
                last_ns: dur,
                min_ns: dur,
                max_ns: dur,
            });
        }
    }
    out
}

#[cfg(not(feature = "std"))]
pub fn aggregate(_events: &[PerfEvent]) -> alloc::vec::Vec<StageStat> {
    alloc::vec::Vec::new()
}
