mod model;
mod tasks;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use model::CircuitModel;
#[cfg(test)]
pub(crate) use model::CircuitModelHandle;
pub(crate) use tasks::{SET_SIZE, TASK_COUNT, TASKS};
#[allow(unused_imports)]
pub(crate) use types::{
    CircuitError, CircuitGate, CircuitModal, CircuitPage, Controls, Evaluation, GateKind, GateSet,
    MAX_DRIVEN_INPUTS, MAX_GATES, MAX_TRACE, MAX_UNDO, SignalSource, TraceSample, TruthRow,
    VerifyResult,
};
