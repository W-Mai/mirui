//! Bounded span recording and export. A capture has one consumer; additional producer threads register before tracing. All timestamps use `core::time`, which need not share the epoch of an application's custom `MonoClock`.

mod buffer;
pub(crate) mod cpu;
mod observer;
mod record;
mod scope;
pub(crate) use observer::{report as observe_report, report_context};
pub use record::{
    ClockDomain, FeedbackKind, PlatformTimestamp, Record, RecordData, ScopeKind, SubmissionId,
    SubmissionOutcome, format_record,
};
pub(crate) use scope::{accepted, operation};
pub use scope::{display_presented, feedback, submission, wait};

fn invalid_scope(epoch: u64) {
    imp::invalid_scope(epoch);
}
#[cfg(not(feature = "std"))]
#[path = "embedded.rs"]
mod imp;
#[cfg(feature = "std")]
#[path = "host.rs"]
mod imp;

use alloc::{string::String, vec::Vec};
pub use buffer::TraceStatus;
use core::fmt::{self, Write};
pub use imp::{is_enabled, register_thread};
pub use mirui_macros::{trace_fn, trace_span};

/// Default bytes per registered producer, including export and aggregation workspace. The workspace is shared by the capture, so additional producers conservatively leave the same allowance unused.
pub const DEFAULT_TRACE_BUDGET: usize = if cfg!(feature = "std") {
    128 * 1024
} else {
    8 * 1024
};
const BATCH: usize = 8;

#[derive(Clone, Copy, Debug)]
pub struct PerfEvent {
    pub name: &'static str,
    pub start_ns: u64,
    pub end_ns: u64,
    pub depth: u8,
}
impl PerfEvent {
    const EMPTY: Self = Self {
        name: "",
        start_ns: 0,
        end_ns: 0,
        depth: 0,
    };
}

/// Capture-local logical track. Empty drain batches carry the track's status exactly once per drain. A logical track is not an operating-system thread identifier.
#[derive(Clone, Copy, Debug)]
pub struct BatchInfo {
    pub capture: u64,
    pub track: u64,
    pub process: u32,
    pub os_thread: Option<u64>,
    pub registered_tracks: u64,
    pub status: TraceStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureError {
    InUse,
    Capacity,
    Inactive,
    Exhausted,
}

/// One span. Disabled guards never become active after a capture is started.
pub struct Guard {
    ticket: Option<imp::Ticket>,
    name: &'static str,
    start_ns: u64,
    depth: u8,
    valid_clock: bool,
    // A scope must close on its originating thread.
    _thread: core::marker::PhantomData<*mut ()>,
}

pub fn enter(name: &'static str) -> Guard {
    let entry = imp::enter();
    let start = entry
        .as_ref()
        .and_then(|_| crate::core::time::try_clock_now_ns());
    let (ticket, depth) = entry.map_or((None, 0), |(ticket, depth)| (Some(ticket), depth));
    Guard {
        ticket,
        name,
        start_ns: start.unwrap_or(0),
        depth,
        valid_clock: start.is_some(),
        _thread: core::marker::PhantomData,
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let Some(ticket) = self.ticket.take() else {
            return;
        };
        let end = crate::core::time::try_clock_now_ns();
        let event = PerfEvent {
            name: self.name,
            start_ns: self.start_ns,
            end_ns: end.unwrap_or(0),
            depth: self.depth,
        };
        if imp::exit(ticket, event, self.valid_clock && end.is_some()) {
            crate::core::log::dispatch_span(&crate::core::log::SpanRecord {
                name: event.name,
                start_ns: event.start_ns,
                end_ns: event.end_ns,
                depth: event.depth,
            });
        }
    }
}

pub fn clock_now_ns() -> u64 {
    crate::core::time::clock_now_ns()
}

/// Inclusive wall totals. Parent and child totals overlap and must not be added together.
#[derive(Clone, Copy, Default)]
pub struct StageStat {
    pub name: &'static str,
    pub count: u32,
    pub total_ns: u64,
    pub last_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
}

/// Cumulative capture losses. Open scopes can legitimately cross drain boundaries; they prevent a drain batch from being described as a complete tree.
#[derive(Clone, Copy, Debug, Default)]
pub struct CaptureStatus {
    pub dropped: u64,
    pub invalid: u64,
    pub aggregation_dropped: u64,
    pub encoding_dropped: u64,
    pub open_scopes: u64,
    pub registered_tracks: u64,
}

/// Sole consumer of the existing trace stream. Storage is prepared once, recording drops on capacity, and draining reuses workspace without holding a producer lock across sinks.
pub struct Capture {
    epoch: u64,
    scratch: [Record; BATCH],
    encoded: String,
    stats: Vec<StageStat>,
    status: CaptureStatus,
    allocated_bytes: usize,
    capacity: usize,
    _thread: core::marker::PhantomData<*mut ()>,
}

impl Capture {
    /// Start a capture and register this thread. Fails rather than taking an existing consumer's stream. Budgets cover recorder payloads, scratch, encoding, aggregation and registration overhead; allocator headers and user-owned sinks are excluded.
    pub fn new(budget: usize) -> Result<Self, CaptureError> {
        let fixed = core::mem::size_of::<Self>() + imp::OVERHEAD;
        let available = budget.checked_sub(fixed).ok_or(CaptureError::Capacity)?;
        let encoding = (available / 8).min(4096);
        let aggregates = (available / 8 / core::mem::size_of::<StageStat>()).min(128);
        if encoding < 512 || aggregates == 0 {
            return Err(CaptureError::Capacity);
        }
        let mut encoded = String::new();
        encoded
            .try_reserve_exact(encoding)
            .map_err(|_| CaptureError::Capacity)?;
        let mut stats: Vec<StageStat> = Vec::new();
        stats
            .try_reserve_exact(aggregates)
            .map_err(|_| CaptureError::Capacity)?;
        let overhead =
            fixed + encoded.capacity() + stats.capacity() * core::mem::size_of::<StageStat>();
        let capacity = budget.checked_sub(overhead).ok_or(CaptureError::Capacity)?
            / core::mem::size_of::<Record>();
        if capacity == 0 {
            return Err(CaptureError::Capacity);
        }
        let epoch = imp::start(capacity)?;
        Ok(Self {
            epoch,
            scratch: [Record::EMPTY; BATCH],
            encoded,
            stats,
            status: CaptureStatus::default(),
            allocated_bytes: overhead + capacity * core::mem::size_of::<Record>(),
            capacity,
            _thread: core::marker::PhantomData,
        })
    }

    pub fn id(&self) -> u64 {
        self.epoch
    }

    /// Enable CPU reads at coarse diagnostic scopes. Unsupported platforms keep the measurement unavailable.
    pub fn set_thread_cpu_time(&mut self, enabled: bool) {
        imp::set_thread_cpu_time(self.epoch, enabled);
    }
    pub fn accounted_bytes_per_thread(&self) -> usize {
        self.allocated_bytes
    }
    pub fn event_capacity(&self) -> usize {
        self.capacity
    }
    pub fn status(&self) -> CaptureStatus {
        self.status
    }
    pub fn stage_stats(&self) -> &[StageStat] {
        &self.stats
    }
    pub fn sort_stage_stats(&mut self) {
        self.stats
            .sort_unstable_by_key(|s| core::cmp::Reverse(s.total_ns));
    }
    pub fn clear_stage_stats(&mut self) {
        self.stats.clear();
    }

    /// Stop new spans without discarding buffered records. Outstanding guards become incomplete; they cannot write into a subsequent capture. Drain before dropping to export the tail.
    pub fn stop(&mut self) {
        imp::stop(self.epoch);
    }

    /// Drain a bounded snapshot from each registered producer. The borrowed slice is valid only during the callback. User-retained copies and custom sink memory are outside the recorder budget.
    pub fn drain_events(&mut self, mut visitor: impl FnMut(BatchInfo, &[Record])) {
        self.status.open_scopes = 0;
        let status = &mut self.status;
        imp::drain(self.epoch, &mut self.scratch, &mut |info, events| {
            if events.is_empty() {
                record_status(status, info);
            }
            visitor(info, events);
        });
    }

    /// Aggregate inclusive spans and optionally export bounded newline-delimited JSON batches. Raw records, aggregation and encoding share the configured budget. A sink can receive multiple batches per operation.
    pub fn drain(&mut self, mut sink: Option<&mut dyn FnMut(&str)>) {
        let start = crate::core::time::try_clock_now_ns();
        let mut aggregate_cost = Some(0);
        let mut encode_cost = Some(0);
        let sink_cost = core::cell::Cell::new(Some(0));
        let has_sink = sink.is_some();
        let mut timed_sink = |batch: &str| {
            let mut cost = sink_cost.get();
            observer::measure(&mut cost, || {
                if let Some(sink) = &mut sink {
                    sink(batch);
                }
            });
            sink_cost.set(cost);
        };
        let mut sink = has_sink.then_some(&mut timed_sink as &mut dyn FnMut(&str));
        let mut business_records = false;
        self.status.open_scopes = 0;
        let status = &mut self.status;
        let stats = &mut self.stats;
        let encoded = &mut self.encoded;
        imp::drain(self.epoch, &mut self.scratch, &mut |info, events| {
            business_records |= events
                .iter()
                .any(|event| !matches!(event.data, RecordData::Observation { .. }));
            if events.is_empty() {
                record_status(status, info);
                if sink.is_some() {
                    observer::measure(&mut encode_cost, || {
                        encode_record(encoded, &mut sink, status, |w| format_status(info, w))
                    });
                }
            }
            observer::measure(&mut aggregate_cost, || {
                for event in events {
                    if let Some(span) = event.span() {
                        accumulate(stats, span, status);
                    }
                }
            });
            if sink.is_some() {
                observer::measure(&mut encode_cost, || {
                    for event in events {
                        encode_record(encoded, &mut sink, status, |w| {
                            format_record(info, event, w)
                        });
                    }
                });
            }
        });
        observer::measure(&mut encode_cost, || {
            if sink.is_some() {
                let snapshot = *status;
                encode_record(encoded, &mut sink, status, |w| {
                    write!(
                        w,
                        r#"{{"name":"trace.capture","ph":"M","pid":{},"tid":0,"args":{{"capture":{},"bytes_per_thread":{},"registered_tracks":{},"dropped":{},"invalid":{},"aggregation_dropped":{},"encoding_dropped":{},"open_scopes":{},"observer_delivery":"next-drain; final sink uncovered"}}}}"#,
                        process_id(),
                        self.epoch,
                        self.allocated_bytes,
                        snapshot.registered_tracks,
                        snapshot.dropped,
                        snapshot.invalid,
                        snapshot.aggregation_dropped,
                        snapshot.encoding_dropped,
                        snapshot.open_scopes
                    )
                });
            }
            flush_encoded(encoded, &mut sink);
        });
        // A drain of observer records alone must terminate rather than recursively trace its own export.
        if business_records && imp::is_enabled() {
            let total = start
                .zip(crate::core::time::try_clock_now_ns())
                .and_then(|(a, b)| b.checked_sub(a));
            let encoding = encode_cost
                .zip(sink_cost.get())
                .and_then(|(a, b)| a.checked_sub(b));
            let draining = total
                .zip(aggregate_cost)
                .zip(encode_cost)
                .and_then(|((a, b), c)| a.checked_sub(b)?.checked_sub(c));
            for (name, cost) in [
                ("probe.drain", draining),
                ("probe.aggregate", aggregate_cost),
                ("probe.encode", encoding),
                ("probe.sink", sink_cost.get()),
            ] {
                observer::record(name, cost);
            }
        }
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        imp::release(self.epoch);
    }
}

fn record_status(status: &mut CaptureStatus, info: BatchInfo) {
    status.dropped = status.dropped.saturating_add(info.status.dropped);
    status.invalid = status.invalid.saturating_add(info.status.invalid);
    status.open_scopes += u64::from(info.status.open_scopes);
    status.registered_tracks = info.registered_tracks;
}

fn accumulate(stats: &mut Vec<StageStat>, event: &PerfEvent, status: &mut CaptureStatus) {
    let duration = event.end_ns - event.start_ns;
    if let Some(stat) = stats.iter_mut().find(|s| s.name == event.name) {
        let Some(count) = stat.count.checked_add(1) else {
            status.aggregation_dropped += 1;
            return;
        };
        let Some(total) = stat.total_ns.checked_add(duration) else {
            status.aggregation_dropped += 1;
            return;
        };
        stat.count = count;
        stat.total_ns = total;
        stat.last_ns = duration;
        stat.min_ns = stat.min_ns.min(duration);
        stat.max_ns = stat.max_ns.max(duration);
    } else if stats.len() < stats.capacity() {
        stats.push(StageStat {
            name: event.name,
            count: 1,
            total_ns: duration,
            last_ns: duration,
            min_ns: duration,
            max_ns: duration,
        });
    } else {
        status.aggregation_dropped = status.aggregation_dropped.saturating_add(1);
    }
}

struct Bounded<'a>(&'a mut String);
impl Write for Bounded<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() > self.0.capacity() - self.0.len() {
            return Err(fmt::Error);
        }
        self.0.push_str(s);
        Ok(())
    }
}
fn flush_encoded(encoded: &mut String, sink: &mut Option<&mut dyn FnMut(&str)>) {
    if !encoded.is_empty() {
        if let Some(sink) = sink {
            sink(encoded);
        }
        encoded.clear();
    }
}
fn encode_record(
    encoded: &mut String,
    sink: &mut Option<&mut dyn FnMut(&str)>,
    status: &mut CaptureStatus,
    write: impl Fn(&mut Bounded<'_>) -> fmt::Result,
) {
    let start = encoded.len();
    if write(&mut Bounded(encoded))
        .and_then(|()| Bounded(encoded).write_char('\n'))
        .is_ok()
    {
        return;
    }
    encoded.truncate(start);
    flush_encoded(encoded, sink);
    if write(&mut Bounded(encoded))
        .and_then(|()| Bounded(encoded).write_char('\n'))
        .is_err()
    {
        encoded.clear();
        status.encoding_dropped = status.encoding_dropped.saturating_add(1);
    }
}

/// Fractional microseconds preserve nanosecond precision without floating-point timestamp rounding.
pub fn format_chrome_event(
    info: BatchInfo,
    sequence: u64,
    event: &PerfEvent,
    w: &mut impl Write,
) -> fmt::Result {
    w.write_str(r#"{"name":""#)?;
    write_json_escaped(event.name, w)?;
    let duration = event.end_ns.saturating_sub(event.start_ns);
    write!(
        w,
        r#"","cat":"mirui.wall","ph":"X","pid":{},"tid":{},"ts":{}.{:03},"dur":{}.{:03},"args":{{"capture":{},"sequence":{},"depth":{},"clock":"mirui.core","instrumented":true}}}}"#,
        info.process,
        info.track,
        event.start_ns / 1000,
        event.start_ns % 1000,
        duration / 1000,
        duration % 1000,
        info.capture,
        sequence,
        event.depth
    )
}

fn format_status(info: BatchInfo, w: &mut impl Write) -> fmt::Result {
    write!(
        w,
        r#"{{"name":"trace.status","ph":"M","pid":{},"tid":{},"args":{{"schema":1,"capture":{},"logical_track":{},"registered_tracks":{},"dropped":{},"invalid":{},"open_scopes":{},"closed":{},"thread_cpu":"{}","gpu":"unavailable","display":"native-feedback-only","os_thread":"#,
        info.process,
        info.track,
        info.capture,
        info.track,
        info.registered_tracks,
        info.status.dropped,
        info.status.invalid,
        info.status.open_scopes,
        info.status.closed,
        cpu::capability()
    )?;
    if let Some(id) = info.os_thread {
        write!(w, "{id}")?;
    } else {
        w.write_str("null")?;
    }
    w.write_str("}}")
}

fn write_json_escaped(s: &str, w: &mut impl Write) -> fmt::Result {
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

fn process_id() -> u32 {
    #[cfg(feature = "std")]
    {
        std::process::id()
    }
    #[cfg(not(feature = "std"))]
    {
        1
    }
}

#[cfg(all(test, feature = "std"))]
pub(crate) mod tests;
