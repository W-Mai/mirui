pub(crate) const CELL_COUNT: usize = 54;
pub(crate) const GRID_WIDTH: usize = 9;
pub(crate) const GRID_HEIGHT: usize = 6;
pub(crate) const MAX_UNDO: usize = 32;
pub(crate) const MAX_TELEMETRY: usize = 80;
pub(crate) const MISSION_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModuleKind {
    Source,
    Dock,
    Belt,
    Furnace,
    Assembler,
    Inspector,
}

impl ModuleKind {
    pub(crate) const BUILDABLE: [Self; 4] =
        [Self::Belt, Self::Furnace, Self::Assembler, Self::Inspector];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Source => "IN",
            Self::Dock => "OUT",
            Self::Belt => "传送带",
            Self::Furnace => "熔炼炉",
            Self::Assembler => "装配机",
            Self::Inspector => "质检台",
        }
    }

    pub(crate) const fn cost(self) -> u8 {
        match self {
            Self::Belt => 1,
            Self::Furnace => 4,
            Self::Assembler => 5,
            Self::Inspector => 3,
            Self::Source | Self::Dock => 0,
        }
    }

    pub(crate) const fn power(self) -> u8 {
        match self {
            Self::Furnace => 3,
            Self::Assembler => 4,
            Self::Inspector => 2,
            Self::Source | Self::Dock | Self::Belt => 0,
        }
    }

    pub(super) const fn accepts(self, stage: MaterialStage) -> bool {
        match self {
            Self::Source => false,
            Self::Furnace => matches!(stage, MaterialStage::Ore),
            Self::Assembler => matches!(stage, MaterialStage::Plate),
            Self::Inspector => matches!(stage, MaterialStage::Gear),
            Self::Dock | Self::Belt => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FactoryCell {
    pub(crate) kind: ModuleKind,
    pub(crate) direction: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaterialStage {
    Ore,
    Plate,
    Gear,
    Certified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FactoryItem {
    pub(crate) id: u16,
    pub(crate) stage: MaterialStage,
    pub(crate) age: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FactoryTelemetry {
    pub(crate) tick: u16,
    pub(crate) delivered: u16,
    pub(crate) blocked: u8,
    pub(crate) wip: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactoryPage {
    Line,
    Orders,
    Telemetry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactoryTool {
    Select,
    Build(ModuleKind),
    Erase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactoryModal {
    None,
    Tools,
    Confirm { mission: u8, reference: bool },
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactoryStatus {
    Ready,
    Running,
    Paused,
    Won,
    Timeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactoryError {
    InvalidCell,
    ImmutableCell,
    InvalidModule,
    Duplicate,
    BudgetExceeded,
    PowerExceeded,
    NothingSelected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FactoryMission {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) goal: u16,
    pub(crate) budget: u8,
    pub(crate) power: u8,
    pub(crate) target: MaterialStage,
}
