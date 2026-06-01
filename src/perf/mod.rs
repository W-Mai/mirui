//! Span-based perf tracing. Use [`crate::trace_span!`] /
//! `#[crate::trace_fn]`; this module is the storage layer.
//!
//! On `std` each [`enter`] guard records `(name, start_ns, end_ns,
//! depth)` into a thread-local `Vec` for chrome-JSON or per-name
//! aggregation.
//!
//! On `no_std` the recorder uses a global ring buffer (256 events,
//! drops oldest) protected by a `critical_section`. Time source is
//! injected via [`set_clock`]; clock plugins (e.g. `SystimerClockPlugin`
//! on ESP) call it during `build`. Without a clock set, `enter` is a
//! no-op — same as before.

#[cfg(feature = "std")]
mod imp {
    extern crate std;
    use core::sync::atomic::{AtomicBool, Ordering};
    use std::cell::RefCell;
    use std::vec::Vec;
    use web_time::Instant;

    // Off by default; PerfReportPlugin::build flips it on.
    static ENABLED: AtomicBool = AtomicBool::new(false);

    pub fn set_enabled(on: bool) {
        ENABLED.store(on, Ordering::Relaxed);
    }

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
            if !ENABLED.load(Ordering::Relaxed) {
                return;
            }
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
        if !ENABLED.load(Ordering::Relaxed) {
            // Stand-in: skip now_ns and the EVENTS push in Drop.
            return Guard {
                name,
                start_ns: 0,
                depth: 0,
            };
        }
        Guard::new(name)
    }

    pub fn drain_events() -> Vec<PerfEvent> {
        EVENTS.with(|e| core::mem::take(&mut *e.borrow_mut()))
    }

    /// std imp uses Instant directly; clock injection is a no-op.
    /// Provided for API symmetry with the no_std imp.
    pub fn set_clock(_f: fn() -> u64) {}
}

#[cfg(not(feature = "std"))]
mod imp {
    /// Ring buffer capacity. 256 × 32B ≈ 8 KB; fits ESP-C3 with room
    /// to spare. Tunable if a target gets memory-tight.
    const CAP: usize = 256;

    #[derive(Clone, Copy)]
    pub struct PerfEvent {
        pub name: &'static str,
        pub start_ns: u64,
        pub end_ns: u64,
        pub depth: u8,
    }

    /// All recorder state lives here; a single critical_section
    /// guards every read/write because RV32IMC (ESP32-C3) lacks the
    /// A extension required for hardware atomics on `AtomicUsize`.
    /// One-target-at-a-time MCUs make this cheap enough.
    struct State {
        clock: usize, // fn() -> u64 stored as usize; 0 = unset
        depth: u8,
        ring: Ring,
    }

    struct Ring {
        events: [PerfEvent; CAP],
        head: usize,
        len: usize,
    }

    static mut STATE: State = State {
        clock: 0,
        depth: 0,
        ring: Ring {
            events: [PerfEvent {
                name: "",
                start_ns: 0,
                end_ns: 0,
                depth: 0,
            }; CAP],
            head: 0,
            len: 0,
        },
    };

    fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
        critical_section::with(|_| {
            #[allow(static_mut_refs)]
            unsafe {
                f(&mut STATE)
            }
        })
    }

    pub fn set_clock(f: fn() -> u64) {
        with_state(|s| s.clock = f as usize);
    }

    /// No-op on `no_std`: the ring buffer already drops oldest, and
    /// `read_clock` short-circuits when no clock was injected.
    pub fn set_enabled(_on: bool) {}

    pub struct Guard {
        name: &'static str,
        start_ns: u64,
        depth: u8,
    }

    fn read_clock() -> Option<fn() -> u64> {
        let raw = with_state(|s| s.clock);
        if raw == 0 {
            None
        } else {
            // SAFETY: only `set_clock` writes this slot, only with a
            // valid `fn() -> u64` cast to usize.
            Some(unsafe { core::mem::transmute::<usize, fn() -> u64>(raw) })
        }
    }

    impl Guard {
        fn new(name: &'static str) -> Self {
            // Clock fn runs outside the critical section.
            let start_ns = read_clock().map(|f| f()).unwrap_or(0);
            let depth = with_state(|s| {
                let d = s.depth;
                s.depth = s.depth.saturating_add(1);
                d
            });
            Guard {
                name,
                start_ns,
                depth,
            }
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            let end_ns = read_clock().map(|f| f()).unwrap_or(0);
            with_state(|s| {
                s.depth = s.depth.saturating_sub(1);
                if s.clock == 0 {
                    return;
                }
                let r = &mut s.ring;
                r.events[r.head] = PerfEvent {
                    name: self.name,
                    start_ns: self.start_ns,
                    end_ns,
                    depth: self.depth,
                };
                r.head = (r.head + 1) % CAP;
                if r.len < CAP {
                    r.len += 1;
                }
            });
        }
    }

    pub fn enter(name: &'static str) -> Guard {
        Guard::new(name)
    }

    pub fn drain_events() -> alloc::vec::Vec<PerfEvent> {
        // One critical section: copy + reset atomically so a concurrent
        // trace_span! between the reset and the read can't tear or
        // mix events into the snapshot. Stack copy is 8 KB (256 × 32B);
        // alloc happens after the lock is released.
        let zero = PerfEvent {
            name: "",
            start_ns: 0,
            end_ns: 0,
            depth: 0,
        };
        let mut buf = [zero; CAP];
        let (head, len) = with_state(|s| {
            let r = &mut s.ring;
            let h = r.head;
            let n = r.len;
            buf.copy_from_slice(&r.events);
            r.head = 0;
            r.len = 0;
            (h, n)
        });
        if len == 0 {
            return alloc::vec::Vec::new();
        }
        let mut out = alloc::vec::Vec::with_capacity(len);
        let start = if len < CAP { 0 } else { head };
        for i in 0..len {
            out.push(buf[(start + i) % CAP]);
        }
        out
    }
}

pub use imp::{Guard, PerfEvent, drain_events, enter, set_clock, set_enabled};

/// One Chrome trace event as JSON, no trailing newline. Wrap the
/// stream as `[...]` or NDJSON for <https://ui.perfetto.dev>.
pub fn format_chrome_event(ev: &PerfEvent, w: &mut impl core::fmt::Write) -> core::fmt::Result {
    let dur_us = ev.end_ns.saturating_sub(ev.start_ns) / 1_000;
    let ts_us = ev.start_ns / 1_000;
    w.write_str(r#"{"name":""#)?;
    write_json_escaped(ev.name, w)?;
    write!(
        w,
        r#"","cat":"mirui","ph":"X","pid":1,"tid":1,"ts":{ts_us},"dur":{dur_us}}}"#,
    )
}

fn write_json_escaped(s: &str, w: &mut impl core::fmt::Write) -> core::fmt::Result {
    for c in s.chars() {
        match c {
            '"' => w.write_str(r#"\""#)?,
            '\\' => w.write_str(r"\\")?,
            '\n' => w.write_str(r"\n")?,
            '\r' => w.write_str(r"\r")?,
            '\t' => w.write_str(r"\t")?,
            c if (c as u32) < 0x20 => write!(w, "\\u{:04x}", c as u32)?,
            c => w.write_char(c)?,
        }
    }
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    fn ev(name: &'static str) -> PerfEvent {
        PerfEvent {
            name,
            start_ns: 1_000,
            end_ns: 5_000,
            depth: 0,
        }
    }

    #[test]
    fn chrome_event_basic_shape() {
        let mut s = String::new();
        format_chrome_event(&ev("frame.render"), &mut s).unwrap();
        assert!(s.starts_with(r#"{"name":"frame.render""#));
        assert!(s.ends_with(r#""ts":1,"dur":4}"#));
    }

    #[test]
    fn chrome_event_escapes_quote_backslash_newline() {
        let mut s = String::new();
        format_chrome_event(&ev("a\"b\\c\nd"), &mut s).unwrap();
        assert!(s.contains(r#"\""#));
        assert!(s.contains(r"\\"));
        assert!(s.contains(r"\n"));
        assert!(!s.contains('\n'));
    }

    #[test]
    fn chrome_event_escapes_low_control_chars() {
        let mut s = String::new();
        format_chrome_event(&ev("a\x01b"), &mut s).unwrap();
        assert!(s.contains(r"\u0001"));
    }
}
