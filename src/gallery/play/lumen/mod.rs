mod model;
mod trace;
mod types;

pub(crate) use model::LumenModel;
#[allow(unused_imports)]
pub(crate) use trace::{Trace, TraceStop, trace};
#[allow(unused_imports)]
pub(crate) use types::{
    GRID_COLUMNS, GRID_ROWS, GridPoint, LEVEL_COUNT, LEVELS, Level, MAX_MIRRORS, MAX_TRACE_POINTS,
    MirrorOrientation, MirrorSpec,
};

const HISTORY_CAPACITY: usize = 32;

#[cfg(test)]
mod tests;
