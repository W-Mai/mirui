mod missions;
mod model;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use missions::MISSIONS;
pub(crate) use model::FactoryModel;
#[cfg(test)]
pub(crate) use model::FactoryModelHandle;
#[cfg(any(feature = "persistence", test))]
#[allow(unused_imports)]
pub(crate) use model::{FactoryProgress, SAVE_LEN};
#[allow(unused_imports)]
pub(crate) use types::{
    Arrival, CELL_COUNT, FactoryCell, FactoryError, FactoryItem, FactoryLayout, FactoryMission,
    FactoryModal, FactoryPage, FactoryStatus, FactoryTelemetry, FactoryTool, GRID_HEIGHT,
    GRID_WIDTH, MAX_ARRIVALS, MAX_TELEMETRY, MAX_UNDO, MISSION_COUNT, MaterialStage, ModuleKind,
};
