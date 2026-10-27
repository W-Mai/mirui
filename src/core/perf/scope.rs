use super::{
    FeedbackKind, PlatformTimestamp, RecordData, ScopeKind, SubmissionId, SubmissionOutcome, imp,
};
use crate::ecs::{FrameOperation, TimingOutcome};

/// Diagnostic scope. It neither submits work nor changes native request lifetime.
pub struct Scope {
    id: Option<SubmissionId>,
    name: &'static str,
    start: Option<u64>,
    cpu_start: Option<u64>,
    kind: ScopeKind,
    previous: Option<SubmissionId>,
    is_operation: bool,
    _thread: core::marker::PhantomData<*mut ()>,
}

impl Scope {
    fn new(name: &'static str, kind: ScopeKind, app: u64, is_operation: bool) -> Self {
        let id = imp::next_scope();
        let start = id.and_then(|_| crate::core::time::try_clock_now_ns());
        let previous = id.and_then(|_| imp::operation());
        let cpu_start = (id.is_some() && !is_operation && imp::thread_cpu_time())
            .then(super::cpu::now_ns)
            .flatten();
        if let Some(id) = id {
            let parent = previous
                .filter(|p| p.capture == id.capture)
                .map_or(0, |p| p.value);
            imp::record(
                id.capture,
                RecordData::Context {
                    id: id.value,
                    parent,
                    app,
                },
            );
            if is_operation {
                imp::replace_operation(Some(id));
            }
        }
        Self {
            id,
            name,
            start,
            cpu_start,
            kind,
            previous,
            is_operation,
            _thread: core::marker::PhantomData,
        }
    }

    pub fn id(&self) -> Option<SubmissionId> {
        self.id
    }

    pub fn finish(mut self, outcome: SubmissionOutcome) {
        if matches!(self.kind, ScopeKind::Submission(_)) {
            self.kind = ScopeKind::Submission(outcome);
        }
    }

    pub(crate) fn close_operation(
        mut self,
        outcome: TimingOutcome,
        cpu: Option<u64>,
    ) -> Option<SubmissionId> {
        if let ScopeKind::Operation(kind, _) = self.kind {
            self.kind = ScopeKind::Operation(kind, outcome);
        }
        if let Some(id) = self.id
            && let Some(nanos) = cpu
        {
            imp::record(
                id.capture,
                RecordData::ThreadCpu {
                    scope: id.value,
                    nanos,
                },
            );
        }
        self.id
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            if let Some(start) = self.cpu_start
                && let Some(nanos) = super::cpu::now_ns().and_then(|end| end.checked_sub(start))
            {
                imp::record(
                    id.capture,
                    RecordData::ThreadCpu {
                        scope: id.value,
                        nanos,
                    },
                );
            }
            if self.is_operation && imp::operation() == Some(id) {
                imp::replace_operation(self.previous);
            }
            if let Some((start_ns, end_ns)) = self.start.zip(crate::core::time::try_clock_now_ns())
                && end_ns >= start_ns
            {
                imp::record(
                    id.capture,
                    RecordData::Scope {
                        id: id.value,
                        name: self.name,
                        start_ns,
                        end_ns,
                        kind: self.kind,
                    },
                );
            } else {
                // An invalid scope is represented by a missing endpoint, never a measured zero.
                imp::record(id.capture, RecordData::IncompleteScope { id: id.value });
            }
        }
    }
}

pub(crate) fn operation(app: u64, kind: FrameOperation) -> Scope {
    Scope::new(
        "app.operation",
        ScopeKind::Operation(kind, TimingOutcome::Failed),
        app,
        true,
    )
}

pub fn submission(name: &'static str) -> Scope {
    Scope::new(
        name,
        ScopeKind::Submission(SubmissionOutcome::Abandoned),
        0,
        false,
    )
}

/// The wait stays in the calling operation. Only a token from the existing native request establishes its cause.
pub fn wait(name: &'static str, submission: Option<SubmissionId>) -> Scope {
    let scope = Scope::new(name, ScopeKind::Wait, 0, false);
    if let Some(id) = scope.id {
        let target = submission
            .filter(|target| target.capture == id.capture)
            .map_or(0, |target| target.value);
        imp::record(
            id.capture,
            RecordData::WaitsOn {
                wait: id.value,
                submission: target,
            },
        );
    }
    scope
}

/// Record optional native evidence. `timestamp` is not converted into the Mirui clock domain. Late feedback from an ended capture is rejected.
pub fn feedback(
    submission: Option<SubmissionId>,
    kind: FeedbackKind,
    display: u64,
    sequence: u64,
    timestamp: PlatformTimestamp,
) {
    let Some(current) = imp::next_scope() else {
        return;
    };
    if submission.is_some_and(|id| id.capture != current.capture) {
        super::invalid_scope(current.capture);
        return;
    }
    let Some(observed_ns) = crate::core::time::try_clock_now_ns() else {
        super::invalid_scope(current.capture);
        return;
    };
    imp::record(
        current.capture,
        RecordData::Feedback {
            submission: submission.map_or(0, |id| id.value),
            display,
            native_sequence: sequence,
            observed_ns,
            timestamp,
            kind,
        },
    );
}

pub fn display_presented(
    submission: Option<SubmissionId>,
    display: u64,
    sequence: u64,
    timestamp: PlatformTimestamp,
) {
    feedback(
        submission,
        FeedbackKind::Presented,
        display,
        sequence,
        timestamp,
    );
}

pub(crate) fn accepted() {
    if !imp::is_enabled() {
        return;
    }
    if let Some(id) = imp::operation()
        && let Some(at_ns) = crate::core::time::try_clock_now_ns()
    {
        imp::record(
            id.capture,
            RecordData::Accepted {
                operation: id.value,
                at_ns,
            },
        );
    }
}
