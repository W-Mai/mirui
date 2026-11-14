pub(crate) const CELL_COUNT: usize = 54;
pub(crate) const GRID_WIDTH: usize = 9;
pub(crate) const GRID_HEIGHT: usize = 6;
pub(crate) const MAX_UNDO: usize = 32;
pub(crate) const MAX_TELEMETRY: usize = 80;
pub(crate) const MISSION_COUNT: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModuleKind {
    Source,
    Dock,
    Belt,
    Furnace,
    Assembler,
    Inspector,
    Wall,
}

impl ModuleKind {
    pub(crate) const BUILDABLE: [Self; 4] =
        [Self::Belt, Self::Furnace, Self::Assembler, Self::Inspector];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Source => "进料口",
            Self::Dock => "交付口",
            Self::Belt => "传送带",
            Self::Furnace => "熔炼炉",
            Self::Assembler => "装配机",
            Self::Inspector => "质检台",
            Self::Wall => "隔墙",
        }
    }

    pub(crate) const fn cost(self) -> u8 {
        match self {
            Self::Belt => 1,
            Self::Furnace => 4,
            Self::Assembler => 5,
            Self::Inspector => 3,
            Self::Source | Self::Dock | Self::Wall => 0,
        }
    }

    pub(crate) const fn power(self) -> u8 {
        match self {
            Self::Furnace => 3,
            Self::Assembler => 4,
            Self::Inspector => 2,
            Self::Source | Self::Dock | Self::Belt | Self::Wall => 0,
        }
    }

    /// Part of the order's floor plan: the player can neither move nor turn it.
    pub(crate) const fn is_fixed(self) -> bool {
        matches!(self, Self::Source | Self::Dock | Self::Wall)
    }

    /// Hands items on in its direction.
    pub(crate) const fn outputs(self) -> bool {
        !matches!(self, Self::Dock | Self::Wall)
    }

    /// The stage a machine takes in, the stage it hands on and the ticks
    /// the work takes.
    pub(crate) const fn process(self) -> Option<(MaterialStage, MaterialStage, u8)> {
        match self {
            Self::Furnace => Some((MaterialStage::Ore, MaterialStage::Plate, 3)),
            Self::Assembler => Some((MaterialStage::Plate, MaterialStage::Gear, 4)),
            Self::Inspector => Some((MaterialStage::Gear, MaterialStage::Certified, 2)),
            Self::Source | Self::Dock | Self::Belt | Self::Wall => None,
        }
    }

    pub(super) const fn accepts(self, stage: MaterialStage) -> bool {
        match self.process() {
            Some((input, _, _)) => input as u8 == stage as u8,
            None => matches!(self, Self::Dock | Self::Belt),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FactoryCell {
    pub(crate) kind: ModuleKind,
    pub(crate) direction: u8,
}

pub(crate) type FactoryLayout = [Option<FactoryCell>; CELL_COUNT];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaterialStage {
    Ore,
    Plate,
    Gear,
    Certified,
}

impl MaterialStage {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Ore => "矿石",
            Self::Plate => "板材",
            Self::Gear => "零件",
            Self::Certified => "合格品",
        }
    }
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
    pub(crate) target: MaterialStage,
    pub(crate) goal: u16,
    /// Ticks the trial run may take.
    pub(crate) limit: u16,
    pub(crate) budget: u8,
    pub(crate) power: u8,
    /// The cell the order first asks the player to look at.
    pub(crate) focus: u8,
    pub(crate) start: &'static FactoryLayout,
    pub(crate) reference: &'static FactoryLayout,
}
