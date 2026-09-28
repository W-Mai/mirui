pub(crate) const BOARD_SIZE: usize = 36;
pub(crate) const ISLAND_COUNT: usize = 4;
pub(crate) const HISTORY_CAPACITY: usize = 24;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Tile {
    #[default]
    Sea = 0,
    Grove = 1,
    Field = 2,
    Hamlet = 3,
    Harbor = 4,
    Lens = 5,
    Lagoon = 6,
    Beacon = 7,
    Dike = 8,
}

impl Tile {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Sea => "海域",
            Self::Grove => "森林",
            Self::Field => "梯田",
            Self::Hamlet => "聚落",
            Self::Harbor => "港湾",
            Self::Lens => "观星台",
            Self::Lagoon => "泻湖",
            Self::Beacon => "灯塔",
            Self::Dike => "石堤",
        }
    }

    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Sea => "相邻海域",
            Self::Grove => "基础 2；每片邻林 +2",
            Self::Field => "基础 2；每面临水 +2",
            Self::Hamlet => "基础 3；每种邻居 +2",
            Self::Harbor => "基础 1；每面临水 +2",
            Self::Lens => "基础 3；每面空海 +2",
            Self::Lagoon => "基础 1；邻田 / 港 +2",
            Self::Beacon => "基础 2；每座邻港 +3",
            Self::Dike => "基础 2；每片邻林 +1",
        }
    }

    pub(crate) const fn rule(self) -> &'static str {
        match self {
            Self::Sea => "海域为相邻地块提供水面",
            Self::Grove => "邻接梯田再 +1",
            Self::Field | Self::Hamlet => "涨潮时，低地停产",
            Self::Harbor => "涨潮时额外 +3",
            Self::Lens => "长夜时额外 +3",
            Self::Lagoon => "为邻格永久提供水面",
            Self::Beacon => "风暴时额外 +2",
            Self::Dike => "保护四邻低地不被淹",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TideLevel {
    #[default]
    Low,
    High,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Weather {
    #[default]
    Clear,
    Harvest,
    LongNight,
    Storm,
}

impl Weather {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Clear => "晴朗",
            Self::Harvest => "丰收",
            Self::LongNight => "长夜",
            Self::Storm => "风暴",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Forecast {
    pub(crate) tide: TideLevel,
    pub(crate) weather: Weather,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Perk {
    Forest,
    Water,
    Town,
    Star,
    Levee,
    Lagoon,
    Beacon,
    Survey,
}

impl Perk {
    pub(super) const ALL: [Self; 8] = [
        Self::Forest,
        Self::Water,
        Self::Town,
        Self::Star,
        Self::Levee,
        Self::Lagoon,
        Self::Beacon,
        Self::Survey,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Forest => "林间协议",
            Self::Water => "潮汐学说",
            Self::Town => "邻里公约",
            Self::Star => "长夜观测",
            Self::Levee => "低地复兴",
            Self::Lagoon => "水脉复苏",
            Self::Beacon => "远航信标",
            Self::Survey => "高地测绘",
        }
    }

    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Forest => "森林每次结算 +2",
            Self::Water => "港湾每次结算 +2",
            Self::Town => "聚落每种邻居再 +1",
            Self::Star => "观星台每次结算 +3",
            Self::Levee => "梯田 / 聚落免疫涨潮",
            Self::Lagoon => "泻湖每次结算 +3",
            Self::Beacon => "灯塔每次结算 +3",
            Self::Survey => "高地建筑每次结算 +1",
        }
    }

    pub(super) const fn bit(self) -> u8 {
        1 << self as u8
    }

    #[cfg(feature = "persistence")]
    pub(crate) const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Forest),
            1 => Some(Self::Water),
            2 => Some(Self::Town),
            3 => Some(Self::Star),
            4 => Some(Self::Levee),
            5 => Some(Self::Lagoon),
            6 => Some(Self::Beacon),
            7 => Some(Self::Survey),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Goal {
    pub(crate) name: &'static str,
    pub(crate) tile: Tile,
    pub(crate) count: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct IslandResult {
    pub(crate) score: u16,
    pub(crate) target: u16,
    pub(crate) goal: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TideModal {
    #[default]
    None,
    Voyage,
    Result,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TideMessage {
    Ready,
    Placed(Tile),
    Harvest(u8, u16),
    Rerolled,
    Undone,
    Settled(bool),
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TideCommand {
    Place { index: u8, choice: u8 },
    Reroll,
    Undo,
    Continue { perk: Option<Perk> },
}
