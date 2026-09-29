mod generation;
mod model;
mod types;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub(crate) use generation::generate_room;
pub(crate) use model::EchoModel;
#[cfg(test)]
pub(crate) use model::EchoModelHandle;
#[allow(unused_imports)]
pub(crate) use types::{
    BEAT_LIMIT, BOARD_HEIGHT, BOARD_WIDTH, CELL_COUNT, ClockGate, Direction, EchoCommand,
    EchoMessage, EchoModal, EchoResult, EchoRoom, MAX_CLOCKS, MAX_GATES, MAX_GEMS, MAX_GHOSTS,
};
