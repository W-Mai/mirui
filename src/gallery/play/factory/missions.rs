use super::types::{FactoryMission, MISSION_COUNT, MaterialStage};

pub(crate) const MISSIONS: [FactoryMission; MISSION_COUNT] = [
    FactoryMission {
        name: "微型装配",
        description: "把矿石变成零件，交付 8 件。先补齐第 6 列缺口。",
        goal: 8,
        budget: 23,
        power: 9,
        target: MaterialStage::Gear,
    },
    FactoryMission {
        name: "折返产线",
        description: "跨两条走廊生产零件。转弯必须指向下一格。",
        goal: 10,
        budget: 34,
        power: 10,
        target: MaterialStage::Gear,
    },
    FactoryMission {
        name: "质量检验",
        description: "装配后的零件要经过质检，才能计入订单。",
        goal: 8,
        budget: 27,
        power: 10,
        target: MaterialStage::Certified,
    },
];
