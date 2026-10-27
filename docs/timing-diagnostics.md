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

## Operations, submissions and feedback

The same bounded stream contains `RecordData` variants for spans, operation contexts, submission attempts, waits, App acceptance and native feedback. Use `record.span()` when consuming only ordinary spans. An operation references its App and caller. A submission has its own capture-scoped identity; multiple requests and failed attempts are not collapsed into a frame number. `app.accepted` reports the existing App acceptance boundary and does not mean display completion.

Backends can attach a diagnostic token to an existing pending native request without changing the `Surface` trait:

```rust
let attempt = perf::submission("driver.submit");
let token = attempt.id();
match driver.submit() {
    Ok(request) => {
        pending.attach_trace(request, token);
        attempt.finish(perf::SubmissionOutcome::Issued);
    }
    Err(error) => {
        attempt.finish(perf::SubmissionOutcome::Failed);
        return Err(error);
    }
}

// In a later operation, use the token belonging to this particular request.
let _wait = perf::wait("driver.wait_previous", pending.trace_id());
driver.wait_previous()?;
```

`pending` above is the backend's existing request storage, not a required Mirui interface. Dropping an unfinished attempt records `Abandoned`. A wait with no token records an unknown cause. The wait's wall interval stays in the operation that executes it, while `waits_on` points to the earlier request. Tokens from an ended capture cannot associate events with a new capture.

`perf::feedback` distinguishes GPU completion, presentation feedback, buffer release and transfer completion. `perf::display_presented` records presentation feedback specifically. Pass a `PlatformTimestamp` with a static `ClockDomain`, nonzero tick frequency and source epoch; native timestamps remain separate from the callback's Mirui observation time. The caller must supply the actual request's token and display identity. Clock names do not establish calibration, and sequence gaps do not create extra presented images. No pending map, device wait or completion callback is introduced by recording these events.

Record capacity depends on the compiled record layout and the configured byte budget. `Capture::event_capacity()` exposes it. Detailed captures can require a larger explicit budget or fewer probes; adding evidence never silently expands the configured storage.

## Backend evidence

SDL software and GPU presentation record submission attempts at the native `present` call; SDL GPU records its explicit queue flush separately. WGPU records queue submissions, presentation calls and surface acquisition. Acquisition has no exact request association. Synchronous WGPU readback associates its existing device poll with the actual readback submission. These scopes measure calls, not GPU execution or display completion, and do not add waits or callbacks.

Linux DRM retains a trace token beside its existing pending page flip. Poll retries refer to that token until the matching CRTC event retires it. The event retains the kernel timestamp domain (`drm.monotonic` or `drm.realtime`), its 32-bit native refresh sequence and the separately sampled callback time. Unknown timestamp capability produces no presentation measurement. Display identities identify one surface's CRTC stream within a capture; reopening the surface or changing capture starts a distinct stream. Modesets and direct framebuffer damage are issued calls without presentation feedback. A capture ending with an outstanding flip cannot establish a complete display-rate window.

Web Canvas acknowledgement and embedded framebuffer flushes retain their existing App acceptance and wall spans. They do not provide native presentation feedback. Browser animation callbacks are scheduling events, and a completed display transfer does not establish scanout timing. Third-party backends retain the existing `Surface` contract; optional evidence uses the same recorder functions above.

## Thread CPU time

`FpsSummaryPlugin::with_thread_cpu_time(true)` enables CPU samples at operation boundaries without requiring detailed traces. `PerfReportPlugin::with_thread_cpu_time(true)` also enables them for traced submission and wait scopes. A direct capture uses `capture.set_thread_cpu_time(true)`. Ordinary draw spans do not read the thread CPU clock. CPU collection is disabled by default.

Linux, Android, macOS and iOS use `CLOCK_THREAD_CPUTIME_ID`; Windows adds the current thread's kernel and user times from `GetThreadTimes`. Failed reads, clock rollback and unsupported platforms return no sample. Web and bare-metal targets do not substitute wall time or cycle counts. Available native thread IDs are exported separately from logical track IDs for matching external scheduling evidence. A native ID is meaningful only within the recorded process and capture lifetime.

`TimingSample::instrumented` and each summary's `instrumented_operations` identify operations with detailed tracing enabled. Inline instrumentation remains in their wall and CPU measurements. `thread_cpu_nanos()` on a summary requires complete CPU sample coverage; `partial_thread_cpu_nanos()` and `cpu_samples()` expose partial coverage explicitly. Wall minus CPU is only a difference between measurements, not a measurement of GPU execution or a specific wait cause. Actual scheduling intervals require separate system evidence.

## Observer cost

`probe.aggregate`, `probe.encode` and `probe.sink` report accumulated wall time over disjoint measured intervals of one drain. `probe.encode` excludes sink calls; `probe.drain` is the remaining measured drain wall time after those intervals. They are counters with `intervals: disjoint-total`, not contiguous execution slices or CPU measurements. `probe.report` measures the summary callback separately. These records carry the closed operation's identity when produced by its timing hook.

Observer records are delivered by a later drain. Exporting only observer records does not generate another observer report. Stopping and draining exports the known tail, but the last sink call itself has no later observation; metadata states that this tail is uncovered. Recorder append, clock reads and probe branches inside business operations remain included in instrumented measurements and require matched enabled/disabled runs to quantify.
