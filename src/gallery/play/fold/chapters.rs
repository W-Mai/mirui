pub(crate) const CHAPTER_NAMES: [&str; 6] = [
    "AWAKEN HULL",
    "THIN ICE",
    "LOST SEALS",
    "BRIDGE SWITCH",
    "CROSSED REEFS",
    "FINAL HARBOR",
];

pub(crate) const CHAPTER_MECHANICS: [&str; 6] = [
    "直立占一格，平躺占两格",
    "薄冰不能直立承重",
    "经过全部徽记后才可归港",
    "直立压开关，切换桥梁",
    "薄冰、徽记与桥梁交错",
    "全部机制组合 · 规划折返",
];

/// Two lines for the briefing that opens each chapter.
pub(crate) const CHAPTER_TIPS: [[&str; 2]; 6] = [
    [
        "直立占一格，横卧或纵卧占两格。",
        "船体压到的每一格下面都要有地面。",
    ],
    ["薄冰可以躺着经过。", "直立站上薄冰会落水。"],
    ["经过金色徽记就能收下。", "收齐全部徽记，才能直立入港。"],
    [
        "直立压在紫色开关上，桥会升起或收起。",
        "躺着经过开关不会触发。",
    ],
    ["薄冰、徽记和桥会一起出现。", "先想好收徽记的顺序。"],
    ["所有机制都会出现。", "有时要先绕远路，再折返。"],
];
