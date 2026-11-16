use super::types::{Goal, Tile};
use crate::tr;

pub(super) const GOALS: [Goal; 6] = [
    Goal {
        name: tr!(en: "Woodland", zh: "连片森林"),
        tile: Tile::Grove,
        count: 5,
    },
    Goal {
        name: tr!(en: "Tide farms", zh: "潮汐农场"),
        tile: Tile::Field,
        count: 4,
    },
    Goal {
        name: tr!(en: "Port call", zh: "港口之约"),
        tile: Tile::Harbor,
        count: 4,
    },
    Goal {
        name: tr!(en: "Star chart", zh: "星图编织"),
        tile: Tile::Lens,
        count: 3,
    },
    Goal {
        name: tr!(en: "Village plan", zh: "邻里计划"),
        tile: Tile::Hamlet,
        count: 4,
    },
    Goal {
        name: tr!(en: "Every kind", zh: "万象群岛"),
        tile: Tile::Sea,
        count: 8,
    },
];
