use super::{PerfEvent, Write, fmt, write_json_escaped};

/// Identity scoped to one capture. Backends retain it only alongside an existing native request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubmissionId {
    pub(crate) capture: u64,
    pub(crate) value: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmissionOutcome {
    Issued,
    Failed,
    Abandoned,
}

/// Native timestamp units and epoch are explicit. A domain name does not calibrate it to the Mirui clock.
#[derive(Clone, Copy, Debug)]
pub struct ClockDomain {
    pub name: &'static str,
    pub ticks_per_second: core::num::NonZeroU64,
}

#[derive(Clone, Copy, Debug)]
pub struct PlatformTimestamp {
    pub domain: &'static ClockDomain,
    pub epoch: u64,
    pub ticks: u64,
}

/// Different completion events must not be treated as interchangeable.
#[derive(Clone, Copy, Debug)]
pub enum FeedbackKind {
    GpuComplete,
    Presented,
    BufferReleased,
    TransferComplete,
}

/// One record in the existing bounded stream. All variants share its byte budget.
#[derive(Clone, Copy, Debug)]
pub struct Record {
    pub sequence: u64,
    pub data: RecordData,
}

#[derive(Clone, Copy, Debug)]
pub enum RecordData {
    Span(PerfEvent),
    Observation {
        name: &'static str,
        operation: u64,
        at_ns: u64,
        wall_ns: u64,
    },
    /// Links a scope to its caller and App. Zero parent means no active operation.
    Context {
        id: u64,
        parent: u64,
        app: u64,
    },
    Scope {
        id: u64,
        name: &'static str,
        start_ns: u64,
        end_ns: u64,
        kind: ScopeKind,
    },
    /// CPU is an independent clock delta, never inferred from wall time.
    ThreadCpu {
        scope: u64,
        nanos: u64,
    },
    IncompleteScope {
        id: u64,
    },
    WaitsOn {
        wait: u64,
        submission: u64,
    },
    /// App acceptance is separate from a backend's issued request.
    Accepted {
        operation: u64,
        at_ns: u64,
    },
    Feedback {
        submission: u64,
        display: u64,
        native_sequence: u64,
        observed_ns: u64,
        timestamp: PlatformTimestamp,
        kind: FeedbackKind,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum ScopeKind {
    Operation(crate::ecs::FrameOperation, crate::ecs::TimingOutcome),
    Submission(SubmissionOutcome),
    Wait,
}

impl Record {
    pub(super) const EMPTY: Self = Self {
        sequence: 0,
        data: RecordData::Span(PerfEvent::EMPTY),
    };

    pub fn span(&self) -> Option<&PerfEvent> {
        match &self.data {
            RecordData::Span(span) => Some(span),
            _ => None,
        }
    }
}

/// Encode one complete record, preserving its capture-local sequence and native clock domain.
pub fn format_record(info: super::BatchInfo, record: &Record, w: &mut impl Write) -> fmt::Result {
    if let RecordData::Span(span) = record.data {
        return super::format_chrome_event(info, record.sequence, &span, w);
    }
    write!(
        w,
        r#"{{"pid":{},"tid":{},"cat":"mirui.evidence","args":{{"capture":{},"sequence":{}"#,
        info.process, info.track, info.capture, record.sequence
    )?;
    match record.data {
        RecordData::Span(_) => unreachable!(),
        RecordData::Observation {
            name,
            operation,
            at_ns,
            wall_ns,
        } => {
            write!(
                w,
                r#", "operation":{operation},"wall_ns":{wall_ns},"intervals":"disjoint-total"}},"name":""#
            )?;
            write_json_escaped(name, w)?;
            write!(
                w,
                r#"","ph":"C","ts":{}.{:03}}}"#,
                at_ns / 1000,
                at_ns % 1000
            )
        }
        RecordData::Context { id, parent, app } => write!(
            w,
            r#", "scope":{id},"parent":{parent},"app":{app}}},"name":"trace.context","ph":"M"}}"#
        ),
        RecordData::ThreadCpu { scope, nanos } => write!(
            w,
            r#", "scope":{scope},"thread_cpu_ns":{nanos}}},"name":"trace.cpu","ph":"M"}}"#
        ),
        RecordData::IncompleteScope { id } => write!(
            w,
            r#", "scope":{id},"incomplete":true}},"name":"trace.incomplete","ph":"M"}}"#
        ),
        RecordData::WaitsOn { wait, submission } => write!(
            w,
            r#", "scope":{wait},"waits_on":{submission},"association":"{}"}},"name":"trace.waits_on","ph":"M"}}"#,
            if submission == 0 { "unknown" } else { "exact" }
        ),
        RecordData::Accepted { operation, at_ns } => write!(
            w,
            r#", "operation":{operation}}},"name":"app.accepted","ph":"i","s":"t","ts":{}.{:03}}}"#,
            at_ns / 1000,
            at_ns % 1000
        ),
        RecordData::Scope {
            id,
            name,
            start_ns,
            end_ns,
            kind,
        } => {
            write!(
                w,
                r#", "scope":{id},"clock":"mirui.core","instrumented":true"#
            )?;
            match kind {
                ScopeKind::Operation(operation, outcome) => write!(
                    w,
                    r#", "kind":"operation","operation":"{operation:?}","outcome":"{outcome:?}"#
                )?,
                ScopeKind::Submission(outcome) => {
                    write!(w, r#", "kind":"submission","outcome":"{outcome:?}"#)?
                }
                ScopeKind::Wait => w.write_str(r#", "kind":"wait"#)?,
            }
            w.write_char('"')?;
            w.write_str(r#"},"name":""#)?;
            write_json_escaped(name, w)?;
            let duration = end_ns.saturating_sub(start_ns);
            write!(
                w,
                r#"","ph":"X","ts":{}.{:03},"dur":{}.{:03}}}"#,
                start_ns / 1000,
                start_ns % 1000,
                duration / 1000,
                duration % 1000
            )
        }
        RecordData::Feedback {
            submission,
            display,
            native_sequence,
            observed_ns,
            timestamp,
            kind,
        } => {
            write!(
                w,
                r#", "submission":{submission},"association":"{}","display":{display},"native_sequence":{native_sequence},"native_ticks":{},"native_epoch":{},"ticks_per_second":{},"native_clock":""#,
                if submission == 0 { "unknown" } else { "exact" },
                timestamp.ticks,
                timestamp.epoch,
                timestamp.domain.ticks_per_second
            )?;
            write_json_escaped(timestamp.domain.name, w)?;
            write!(
                w,
                r#"","feedback":"{kind:?}","observed_clock":"mirui.core"}},"name":"host.feedback","ph":"i","s":"t","ts":{}.{:03}}}"#,
                observed_ns / 1000,
                observed_ns % 1000
            )
        }
    }
}
