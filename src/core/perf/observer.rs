use super::{RecordData, SubmissionId, imp};

pub(crate) struct ReportContext(Option<SubmissionId>, bool);
pub(crate) fn report_context(id: Option<SubmissionId>) -> ReportContext {
    let enabled = id.is_some() && imp::is_enabled();
    ReportContext(
        if enabled {
            imp::replace_operation(id)
        } else {
            None
        },
        enabled,
    )
}
impl Drop for ReportContext {
    fn drop(&mut self) {
        if self.1 {
            imp::replace_operation(self.0);
        }
    }
}

pub(super) fn measure<R>(total: &mut Option<u64>, action: impl FnOnce() -> R) -> R {
    let start = crate::core::time::try_clock_now_ns();
    let result = action();
    let elapsed = start
        .zip(crate::core::time::try_clock_now_ns())
        .and_then(|(a, b)| b.checked_sub(a));
    *total = total.zip(elapsed).and_then(|(a, b)| a.checked_add(b));
    result
}

/// Observer totals are counters over disjoint intervals, not fabricated contiguous spans.
pub(super) fn record(name: &'static str, wall_ns: Option<u64>) {
    let Some(scope) = imp::next_scope() else {
        return;
    };
    let Some((wall_ns, at_ns)) = wall_ns.zip(crate::core::time::try_clock_now_ns()) else {
        super::invalid_scope(scope.capture);
        return;
    };
    let operation = imp::operation()
        .filter(|op| op.capture == scope.capture)
        .map_or(0, |op| op.value);
    imp::record(
        scope.capture,
        RecordData::Observation {
            name,
            operation,
            at_ns,
            wall_ns,
        },
    );
}

pub(crate) fn report<R>(action: impl FnOnce() -> R) -> R {
    if !imp::is_enabled() {
        return action();
    }
    let mut cost = Some(0);
    let result = measure(&mut cost, action);
    record("probe.report", cost);
    result
}
