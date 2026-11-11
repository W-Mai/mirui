#[cfg(any(feature = "perf", test))]
use super::{RecordData, imp};

/// Resource categories encoded without per-record string pointers.
#[derive(Clone, Copy, Debug)]
pub enum ResourceKind {
    OutputTarget,
    SdlTargetWorkspace,
}

impl ResourceKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::OutputTarget => "output.target",
            Self::SdlTargetWorkspace => "sdl.target.workspace",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionOutcome {
    NotOffered,
    Budget,
    WorkingSetProtected,
    CpuConsumerPreferred,
    ArithmeticOverflow,
    WorkspaceUnavailable,
    WorkspaceAllocation,
    Reused,
    Unchanged,
    Allocated,
    AllocationFailed,
    BackendError,
    Busy,
    HistoryFrozen,
    ZeroSize,
    InvalidPlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionStorage {
    Unselected,
    Native,
    Cpu,
}

/// An instantaneous resource decision, not a duration or completion event.
///
/// Resource identities are local to the App in the associated operation context. An operation of zero has no established App association. Unknown requested bytes are exported as null; available bytes describe only the admission budget at the decision point, including reclaimable storage when applicable.
#[derive(Clone, Copy, Debug)]
pub struct ResourceDecision {
    pub kind: ResourceKind,
    /// Identity within the operation's App; ignored when resource_known is false.
    pub resource: u64,
    pub requested_bytes: u64,
    pub available_bytes: u64,
    pub scale: crate::types::Fixed,
    pub width: u16,
    pub height: u16,
    pub outcome: DecisionOutcome,
    pub storage: DecisionStorage,
    pub slot: u8,
    pub retained: bool,
    pub requested_known: bool,
    pub resource_known: bool,
}

// Preserve the recorder's existing per-entry budget on host and embedded targets.
const _: () = assert!(core::mem::size_of::<ResourceDecision>() <= 40);
const _: () = assert!(core::mem::size_of::<super::Record>() <= 72);

/// Append a decision to the current capture without allocating or changing resource admission.
#[cfg(any(feature = "perf", test))]
pub(crate) fn decision(data: ResourceDecision) {
    let Some(scope) = imp::next_scope() else {
        return;
    };
    let Some(at_ns) = crate::core::time::try_clock_now_ns() else {
        super::invalid_scope(scope.capture);
        return;
    };
    let operation = imp::operation()
        .filter(|op| op.capture == scope.capture)
        .map_or(0, |op| op.value);
    imp::record(
        scope.capture,
        RecordData::Decision {
            operation,
            at_ns,
            data,
        },
    );
}
