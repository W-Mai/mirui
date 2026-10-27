# Timing diagnostics

Install a clock and the existing report plugin:

```rust
use mirui::app::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};

app.add_plugin(StdInstantClockPlugin::default());
app.add_plugin(FpsSummaryPlugin::new(120).with_sink(|report| {
    for operation in report.operations.iter().filter(|op| op.count != 0) {
        println!("{:?}: wall {:?} ns, rate {:?}/s", operation.operation,
            operation.wall_nanos(), report.operation_rate_hz(operation.operation));
    }
}));
```

`new(120)` reports every 120 closed App operations. `Tick` covers input, systems, drawing and platform maintenance. Hosts that deliver update and redraw separately produce separate `Update` and `Draw` samples. Direct `render`, `render_dirty` and `snapshot` calls produce `Draw` samples. Nested drawing inside a tick belongs to that tick and does not produce another operation sample.

Operation wall time includes business `post_render` hooks and platform maintenance. It excludes the subsequent timing reports and host pacing. Instrumentation inside an operation remains included. Wall time includes blocking and time when the thread is not scheduled; it must not be described as CPU time. A long poll can wait for an earlier submission while still belonging to the operation that called it. Without submission and scheduling evidence, the cause of that wait is unknown.

`FrameTimings` records input, systems, layout, render, flush and previous-coverage seeding. Its accessors return `Option<u64>`: `None` means absent or invalid, while `Some(0)` is a measured zero. `phase_wall_nanos()` sums these measured phases, not all operation work. The difference from operation wall time is uncovered wall time, not automatically wasted computation. A failed attempt publishes its own attempted phases rather than repeating the preceding successful sample.

`FrameStats::summary(operation, phases)` selects an exact operation and phase set from one 256-sample ring. Only valid completed operations enter the ring. The result contains its own sample count, average, minimum, maximum, nearest-rank P99 and jitter. Different operation types or phase sets cannot be combined by this query. Missing CPU timing is unavailable; partial CPU sample counts must not be subtracted from a complete wall-time average.

Operation cadence counts callbacks in a half-open timestamp interval. Equal timestamps remain in the next window, and a missing or backward clock invalidates that interval. Callback cadence includes idle operations and does not measure GPU completion or display refresh. Presentation feedback requires its own timestamp and completion semantics; no display rate is inferred from phase sums, successful submissions or callback arrival time.

Custom sinks receive the just-closed operation through the existing reporting plugins. Low-level plugins can implement `on_timing`; installing a timing reporter enables its delivery. The read-only `TimingSample` refers to a closed snapshot, and `FrameTimings` in the World has already been updated. Business `post_render` retains its original ordering and sees the previous closed operation. Mutating application state inside a timing reporter is outside the measured operation.

## Bounded traces

`PerfReportPlugin` owns the trace stream and drains registered producers after each closed operation. `with_trace_budget(bytes)` sets the per-producer diagnostic payload budget before installation: 128 KiB on `std`, 8 KiB on `no_std`. It includes recording storage, export scratch, encoding, aggregation and registration overhead. Allocator headers, active stack guards and user-owned sinks are outside this budget. Each extra producer conservatively reserves the same workspace allowance; the setting is not a process-wide memory limit. Capacity exhaustion drops records and increments counters instead of growing the buffer.

```rust
use mirui::core::perf::{self, Capture, DEFAULT_TRACE_BUDGET};

let mut capture = Capture::new(DEFAULT_TRACE_BUDGET)?;
{
    let _span = perf::enter("assets.decode");
    decode_assets();
}
capture.drain_events(|batch, events| {
    // Borrowed until this callback returns. Empty batches carry track status.
    consume(batch, events);
});
capture.stop();
capture.drain_events(|batch, events| consume(batch, events));
```

Only one `Capture` can consume the process stream. A second capture returns `CaptureError::InUse`; a second report plugin logs a warning and leaves the existing consumer intact. The capture's thread is registered at construction. Other producer threads call `perf::register_thread()` before tracing. Registration may allocate; recording and draining reuse prepared storage. Unregistered producers count as lost records. A stopped capture rejects new scopes, and an old guard cannot write into a later capture. Stop and drain before dropping a direct capture to export its tail.

`Capture::drain` aggregates inclusive wall totals and optionally writes batches of complete newline-delimited Chrome JSON records. One operation can emit several batches. Sinks preserve their order and run outside recorder locks. Fractional microseconds preserve nanosecond timestamps. `Capture::drain_events` is the raw alternative; it does not also aggregate or encode the records it consumes. Copying a batch into application storage is outside the recorder budget.

Track IDs are capture-local logical identities, not operating-system thread IDs. Sequence numbers preserve close order, including gaps caused by loss. `trace.status` and `trace.capture` expose dropped records, invalid clocks or nesting, open scopes, and aggregation or encoding loss. Missing CPU, GPU or display measurements remain unavailable. An open scope can cross a drain boundary; a single batch is not necessarily a complete tree. Truncated trees cannot yield exact uncovered wall time. Inclusive totals overlap and must not be added together.
