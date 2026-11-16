mod model;
mod tasks;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use model::CircuitModel;
#[cfg(test)]
pub(crate) use model::CircuitModelHandle;
#[cfg(any(feature = "persistence", test))]
#[allow(unused_imports)]
pub(crate) use model::{CircuitProgress, SAVE_LEN};
pub(crate) use tasks::{SET_SIZE, TASK_COUNT, TASKS};
#[allow(unused_imports)]
pub(crate) use types::{
    BOARD, CircuitError, CircuitGate, CircuitModal, CircuitPage, Controls, Evaluation,
    GATE_HALF_HEIGHT, GATE_HALF_WIDTH, GateKind, GateSet, INPUT_PIN_X, MAX_DRIVEN_INPUTS,
    MAX_GATES, MAX_TRACE, MAX_UNDO, OUTPUT_PIN, PIN_SPREAD, SignalSource, TraceSample, TruthRow,
    VerifyResult, input_y,
};
