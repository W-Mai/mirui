use crate::core::i18n::Phrase;
use crate::tr;

pub(crate) const MISSION_COUNT: usize = 8;
pub(crate) const MAX_GOALS: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OrbitGoal {
    /// Sample while the distance from the planet is inside the band.
    Band { min: i16, max: i16 },
    /// Sample within `range` of the beacon, no faster than `max_speed`.
    Beacon {
        x: i16,
        y: i16,
        range: i16,
        max_speed: i16,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OrbitMission {
    pub(crate) name: Phrase,
    pub(crate) description: Phrase,
    /// A maneuver that completes the mission; the tests fly it.
    pub(crate) hint: Phrase,
    pub(crate) fuel: u8,
    /// Seconds of contact at the start; after that only planned nodes fire.
    /// Zero keeps contact for the whole flight.
    pub(crate) contact_seconds: u8,
    pub(super) goals: [Option<OrbitGoal>; MAX_GOALS],
}

impl OrbitMission {
    pub(crate) fn goal(&self, index: usize) -> Option<OrbitGoal> {
        self.goals.get(index).copied().flatten()
    }

    pub(crate) fn goal_count(&self) -> usize {
        self.goals.iter().flatten().count()
    }
}

const OUTER: OrbitGoal = OrbitGoal::Band { min: 113, max: 130 };
const INNER: OrbitGoal = OrbitGoal::Band { min: 44, max: 58 };
const FAR: OrbitGoal = OrbitGoal::Beacon {
    x: -148,
    y: 0,
    range: 23,
    max_speed: 24,
};
const WEST: OrbitGoal = OrbitGoal::Beacon {
    x: -123,
    y: 0,
    range: 20,
    max_speed: 30,
};
const EAST: OrbitGoal = OrbitGoal::Beacon {
    x: 123,
    y: 0,
    range: 20,
    max_speed: 30,
};
const DOCK: OrbitGoal = OrbitGoal::Beacon {
    x: -148,
    y: 0,
    range: 16,
    max_speed: 14,
};

pub(crate) const MISSIONS: [OrbitMission; MISSION_COUNT] = [
    OrbitMission {
        name: tr!(en: "Raise orbit", zh: "抬升轨道"),
        description: tr!(
            en: "Burn along your motion and the far side of the orbit rises. Scan inside the orange band.",
            zh: "顺着飞行方向点火，轨道另一侧会升高。飞进橙色带时采样。"
        ),
        hint: tr!(
            en: "Δv 3.8 at 0°, burn, then ▶ to coast.",
            zh: "Δv 3.8，角度 0°，点火后按 ▶ 滑行。"
        ),
        fuel: 48,
        contact_seconds: 0,
        goals: [Some(OUTER), None, None],
    },
    OrbitMission {
        name: tr!(en: "Far beacon", zh: "远点信标"),
        description: tr!(
            en: "More Δv puts the far point farther out. Put it on the beacon, speed limit 24.",
            zh: "Δv 越大，远点越远。把远点送到信标处，限速 24。"
        ),
        hint: tr!(
            en: "Δv 5.2 at 0°, burn. About 11 s to the beacon.",
            zh: "Δv 5.2，角度 0°，点火。约 11 s 到信标。"
        ),
        fuel: 48,
        contact_seconds: 0,
        goals: [Some(FAR), None, None],
    },
    OrbitMission {
        name: tr!(en: "Lower orbit", zh: "降低轨道"),
        description: tr!(
            en: "At 180° you burn against your motion and the far side drops. Scan in the inner band.",
            zh: "角度 180° 是反向点火，轨道另一侧会降低。在内圈带采样。"
        ),
        hint: tr!(en: "Turn to 180°, Δv 3.8, burn.", zh: "角度 180°，Δv 3.8，点火。"),
        fuel: 48,
        contact_seconds: 0,
        goals: [Some(INNER), None, None],
    },
    OrbitMission {
        name: tr!(en: "Out and in", zh: "内外两圈"),
        description: tr!(
            en: "Scan the outer band, burn back at the far point, then scan the inner band.",
            zh: "先在外圈采样，到远点反向点火，再到内圈采样。"
        ),
        hint: tr!(
            en: "Burn Δv 3.8. At the far point, about 10 s, turn to 180° and burn Δv 3.2.",
            zh: "Δv 3.8 点火；约 10 s 到远点，转到 180°，Δv 3.2 点火。"
        ),
        fuel: 48,
        contact_seconds: 0,
        goals: [Some(OUTER), Some(INNER), None],
    },
    OrbitMission {
        name: tr!(en: "Round orbit", zh: "圆形轨道"),
        description: tr!(
            en: "Burn forward again at the far point to round the orbit, then pass both beacons.",
            zh: "到远点再顺向点火，把轨道变圆，先后经过两座信标。"
        ),
        hint: tr!(
            en: "Burn Δv 3.8. At the far point, about 10 s, burn Δv 3.4.",
            zh: "Δv 3.8 点火；约 10 s 到远点，再 Δv 3.4 点火。"
        ),
        fuel: 48,
        contact_seconds: 0,
        goals: [Some(WEST), Some(EAST), None],
    },
    OrbitMission {
        name: tr!(en: "Slow dock", zh: "低速对接"),
        description: tr!(
            en: "Only 28 fuel. Burn back at the beacon to get under speed 14.",
            zh: "燃料只有 28。到信标时反向点火，把速度压到 14 以下。"
        ),
        hint: tr!(
            en: "Burn Δv 5.2. At the beacon, about 11 s, turn to 180° and burn Δv 8.0.",
            zh: "Δv 5.2 点火；约 11 s 到信标，转到 180°，Δv 8.0 点火。"
        ),
        fuel: 28,
        contact_seconds: 0,
        goals: [Some(DOCK), None, None],
    },
    OrbitMission {
        name: tr!(en: "Blackout", zh: "失联之前"),
        description: tr!(
            en: "Contact ends after 4 s; then only planned nodes fire. Only 16 fuel.",
            zh: "4 s 后失联，之后只执行排好的节点。燃料只有 16。"
        ),
        hint: tr!(
            en: "Plan a node first: delay 10 s, Δv 3.4. Then burn Δv 3.8.",
            zh: "先排节点：延时 10 s，Δv 3.4。再 Δv 3.8 点火。"
        ),
        fuel: 16,
        contact_seconds: 4,
        goals: [Some(WEST), Some(EAST), None],
    },
    OrbitMission {
        name: tr!(en: "Three legs", zh: "三段航线"),
        description: tr!(
            en: "Contact ends after 4 s, 30 fuel. Scan the outer band, pass the beacon, then scan the inner band.",
            zh: "4 s 后失联，燃料 30。外圈采样，经过信标，再到内圈采样。"
        ),
        hint: tr!(
            en: "Plan nodes: 10 s Δv 3.4, 26 s 180° Δv 6.6. Then burn Δv 3.8.",
            zh: "排节点：延时 10 s Δv 3.4，延时 26 s 180° Δv 6.6，再 Δv 3.8 点火。"
        ),
        fuel: 30,
        contact_seconds: 4,
        goals: [Some(OUTER), Some(EAST), Some(INNER)],
    },
];
