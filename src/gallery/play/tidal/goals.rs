use super::types::{Goal, Tile};

pub(super) const GOALS: [Goal; 6] = [
    Goal {
        name: "连片森林",
        tile: Tile::Grove,
        count: 5,
    },
    Goal {
        name: "潮汐农场",
        tile: Tile::Field,
        count: 4,
    },
    Goal {
        name: "港口之约",
        tile: Tile::Harbor,
        count: 4,
    },
    Goal {
        name: "星图编织",
        tile: Tile::Lens,
        count: 3,
    },
    Goal {
        name: "邻里计划",
        tile: Tile::Hamlet,
        count: 4,
    },
    Goal {
        name: "万象群岛",
        tile: Tile::Sea,
        count: 8,
    },
];
