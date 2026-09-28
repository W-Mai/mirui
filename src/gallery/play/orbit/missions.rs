pub(crate) const MISSION_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitGoal {
    Band {
        min: i16,
        max: i16,
    },
    Beacon {
        x: i16,
        y: i16,
        range: i16,
        max_speed: i16,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OrbitMission {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) hint: &'static str,
    pub(crate) default_dv_tenths: u8,
    pub(super) goals: [Option<OrbitGoal>; 2],
}

pub(crate) const MISSIONS: [OrbitMission; MISSION_COUNT] = [
    OrbitMission {
        name: "轨道抬升",
        description: "进入橙色采样带后手动采样。",
        hint: "沿速度方向 +3.8，滑行到远端。",
        default_dv_tenths: 38,
        goals: [Some(OrbitGoal::Band { min: 113, max: 130 }), None],
    },
    OrbitMission {
        name: "往返测绘",
        description: "先采外轨，再回内轨采样。",
        hint: "外轨采样后继续滑行回内轨。",
        default_dv_tenths: 38,
        goals: [
            Some(OrbitGoal::Band { min: 113, max: 130 }),
            Some(OrbitGoal::Band { min: 75, max: 90 }),
        ],
    },
    OrbitMission {
        name: "远端通信",
        description: "接近远端信标，并降低到接入速度。",
        hint: "沿速度方向 +5.2，到远端信标接入。",
        default_dv_tenths: 52,
        goals: [
            Some(OrbitGoal::Beacon {
                x: -148,
                y: 0,
                range: 23,
                max_speed: 24,
            }),
            None,
        ],
    },
];
