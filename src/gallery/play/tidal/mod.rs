mod goals;
mod model;
mod types;

#[cfg(test)]
mod tests;

#[cfg(any(feature = "persistence", test))]
#[cfg(test)]
pub(crate) use model::TideModelHandle;
pub(crate) use model::{TideModel, TidePreview};
#[allow(unused_imports)]
pub(crate) use types::{
    BOARD_SIZE, Forecast, Goal, HISTORY_CAPACITY, ISLAND_COUNT, IslandResult, Perk, TideCommand,
    TideLevel, TideMessage, TideModal, Tile, Weather,
};
