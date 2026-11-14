mod levels;
mod model;
mod trace;
mod types;

pub(crate) use levels::LEVELS;
#[cfg(test)]
pub(crate) use model::LumenModelHandle;
pub(crate) use model::{BeamStatus, LumenModel, LumenNote};
#[allow(unused_imports)]
pub(crate) use trace::{Trace, TraceStop, trace};
#[allow(unused_imports)]
pub(crate) use types::{
    FixedMirror, GRID_COLUMNS, GRID_ROWS, GridPoint, LEVEL_COUNT, Level, LevelSet, MAX_MIRRORS,
    MAX_TRACE_POINTS, MirrorOrientation, MirrorSpec, SET_COUNT, SET_SIZE, SETS,
};

const HISTORY_CAPACITY: usize = 32;

#[cfg(test)]
mod tests;
