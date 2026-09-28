mod model;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use model::{CircuitModel, CircuitModelHandle};
#[allow(unused_imports)]
pub(crate) use types::{
    CircuitError, CircuitGate, CircuitModal, CircuitPage, Evaluation, GateKind, MAX_DRIVEN_INPUTS,
    MAX_GATES, MAX_TRACE, MAX_UNDO, SignalSource, TASK_COUNT, TraceSample, TruthRow, VerifyResult,
};
