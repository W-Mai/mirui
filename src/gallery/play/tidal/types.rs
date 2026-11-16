use crate::core::i18n::Phrase;
use crate::tr;

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
    pub(crate) const fn name(self) -> Phrase {
        match self {
            Self::Sea => tr!(en: "Sea", zh: "海域"),
            Self::Grove => tr!(en: "Grove", zh: "森林"),
            Self::Field => tr!(en: "Field", zh: "梯田"),
            Self::Hamlet => tr!(en: "Hamlet", zh: "聚落"),
            Self::Harbor => tr!(en: "Harbor", zh: "港湾"),
            Self::Lens => tr!(en: "Observatory", zh: "观星台"),
            Self::Lagoon => tr!(en: "Lagoon", zh: "泻湖"),
            Self::Beacon => tr!(en: "Beacon", zh: "灯塔"),
            Self::Dike => tr!(en: "Seawall", zh: "石堤"),
        }
    }

    pub(crate) const fn description(self) -> Phrase {
        match self {
            Self::Sea => tr!(en: "Open sea", zh: "相邻海域"),
            Self::Grove => tr!(en: "2, +2 per grove beside", zh: "基础 2；每片邻林 +2"),
            Self::Field => tr!(en: "2, +2 per water side", zh: "基础 2；每面临水 +2"),
            Self::Hamlet => tr!(en: "3, +2 per kind beside", zh: "基础 3；每种邻居 +2"),
            Self::Harbor => tr!(en: "1, +2 per water side", zh: "基础 1；每面临水 +2"),
            Self::Lens => tr!(en: "3, +2 per open sea side", zh: "基础 3；每面空海 +2"),
            Self::Lagoon => tr!(en: "1, +2 per field or harbor", zh: "基础 1；邻田 / 港 +2"),
            Self::Beacon => tr!(en: "2, +3 per harbor beside", zh: "基础 2；每座邻港 +3"),
            Self::Dike => tr!(en: "2, +1 per grove beside", zh: "基础 2；每片邻林 +1"),
        }
    }

    pub(crate) const fn rule(self) -> Phrase {
        match self {
            Self::Sea => tr!(en: "Gives water to its neighbors", zh: "海域为相邻地块提供水面"),
            Self::Grove => tr!(en: "+1 next to a field", zh: "邻接梯田再 +1"),
            Self::Field | Self::Hamlet => {
                tr!(en: "Low ground floods at high tide", zh: "涨潮时，低地停产")
            }
            Self::Harbor => tr!(en: "+3 at high tide", zh: "涨潮时额外 +3"),
            Self::Lens => tr!(en: "+3 on a long night", zh: "长夜时额外 +3"),
            Self::Lagoon => tr!(en: "Counts as water for neighbors", zh: "为邻格永久提供水面"),
            Self::Beacon => tr!(en: "+2 in a storm", zh: "风暴时额外 +2"),
            Self::Dike => tr!(en: "Keeps low ground beside it dry", zh: "保护四邻低地不被淹"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TideLevel {
    #[default]
    Low,
    High,
}

impl TideLevel {
    pub(crate) const fn name(self) -> Phrase {
        match self {
            Self::Low => tr!(en: "Low tide", zh: "退潮"),
            Self::High => tr!(en: "High tide", zh: "涨潮"),
        }
    }
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
    pub(crate) const fn name(self) -> Phrase {
        match self {
            Self::Clear => tr!(en: "Clear", zh: "晴朗"),
            Self::Harvest => tr!(en: "Bounty", zh: "丰收"),
            Self::LongNight => tr!(en: "Long night", zh: "长夜"),
            Self::Storm => tr!(en: "Storm", zh: "风暴"),
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

    pub(crate) const fn name(self) -> Phrase {
        match self {
            Self::Forest => tr!(en: "Forest pact", zh: "林间协议"),
            Self::Water => tr!(en: "Tide lore", zh: "潮汐学说"),
            Self::Town => tr!(en: "Good neighbors", zh: "邻里公约"),
            Self::Star => tr!(en: "Night watch", zh: "长夜观测"),
            Self::Levee => tr!(en: "Dry fields", zh: "低地复兴"),
            Self::Lagoon => tr!(en: "Living water", zh: "水脉复苏"),
            Self::Beacon => tr!(en: "Far lights", zh: "远航信标"),
            Self::Survey => tr!(en: "Highland survey", zh: "高地测绘"),
        }
    }

    pub(crate) const fn description(self) -> Phrase {
        match self {
            Self::Forest => tr!(en: "Groves +2 a harvest", zh: "森林每次结算 +2"),
            Self::Water => tr!(en: "Harbors +2 a harvest", zh: "港湾每次结算 +2"),
            Self::Town => tr!(en: "Hamlets +1 per kind", zh: "聚落每种邻居再 +1"),
            Self::Star => tr!(en: "Observatories +3", zh: "观星台每次结算 +3"),
            Self::Levee => tr!(en: "Low ground stays dry", zh: "梯田 / 聚落免疫涨潮"),
            Self::Lagoon => tr!(en: "Lagoons +3 a harvest", zh: "泻湖每次结算 +3"),
            Self::Beacon => tr!(en: "Beacons +3 a harvest", zh: "灯塔每次结算 +3"),
            Self::Survey => tr!(en: "High ground tiles +1", zh: "高地建筑每次结算 +1"),
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
    pub(crate) name: Phrase,
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
    /// A tap on sea that does not touch land.
    Shore,
    /// Place was pressed before a spot was picked.
    Spot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TideCommand {
    Place { index: u8, choice: u8 },
    Reroll,
    Undo,
    Continue { perk: Option<Perk> },
}
