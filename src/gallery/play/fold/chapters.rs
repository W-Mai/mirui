use crate::core::i18n::Phrase;
use crate::tr;

pub(crate) const CHAPTER_NAMES: [Phrase; 6] = [
    tr!(en: "AWAKEN HULL", zh: "启动船体"),
    tr!(en: "THIN ICE", zh: "薄冰航道"),
    tr!(en: "LOST SEALS", zh: "失落徽记"),
    tr!(en: "BRIDGE SWITCH", zh: "桥梁开关"),
    tr!(en: "CROSSED REEFS", zh: "交错险路"),
    tr!(en: "FINAL HARBOR", zh: "最终港湾"),
];

pub(crate) const CHAPTER_MECHANICS: [Phrase; 6] = [
    tr!(en: "Upright takes one cell, lying takes two", zh: "直立占一格，平躺占两格"),
    tr!(en: "Thin ice cannot hold an upright hull", zh: "薄冰不能直立承重"),
    tr!(en: "Collect every seal before you dock", zh: "经过全部徽记后才可归港"),
    tr!(en: "Stand on a switch to move the bridge", zh: "直立压开关，切换桥梁"),
    tr!(en: "Ice, seals and bridges together", zh: "薄冰、徽记与桥梁交错"),
    tr!(en: "Every rule at once · plan the way back", zh: "全部机制组合 · 规划折返"),
];

/// Two lines for the briefing that opens each chapter.
pub(crate) const CHAPTER_TIPS: [[Phrase; 2]; 6] = [
    [
        tr!(
            en: "Upright, the hull takes one cell; lying, it takes two.",
            zh: "直立占一格，横卧或纵卧占两格。"
        ),
        tr!(en: "Every cell under the hull needs ground.", zh: "船体压到的每一格下面都要有地面。"),
    ],
    [
        tr!(en: "Thin ice can be crossed lying down.", zh: "薄冰可以躺着经过。"),
        tr!(en: "Standing upright on thin ice sinks the hull.", zh: "直立站上薄冰会落水。"),
    ],
    [
        tr!(en: "Roll over a gold seal to collect it.", zh: "经过金色徽记就能收下。"),
        tr!(
            en: "Dock upright once every seal is aboard.",
            zh: "收齐全部徽记，才能直立入港。"
        ),
    ],
    [
        tr!(
            en: "Stand on the violet switch to raise or lower the bridge.",
            zh: "直立压在紫色开关上，桥会升起或收起。"
        ),
        tr!(en: "Rolling over it lying down does nothing.", zh: "躺着经过开关不会触发。"),
    ],
    [
        tr!(en: "Ice, seals and bridges now share a level.", zh: "薄冰、徽记和桥会一起出现。"),
        tr!(en: "Plan the order of the seals first.", zh: "先想好收徽记的顺序。"),
    ],
    [
        tr!(en: "Every rule appears.", zh: "所有机制都会出现。"),
        tr!(en: "Sometimes the long way round comes first.", zh: "有时要先绕远路，再折返。"),
    ],
];
