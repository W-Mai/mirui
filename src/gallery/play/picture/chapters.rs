use crate::core::i18n::Phrase;
use crate::tr;

pub(crate) const CHAPTER_NAMES: [Phrase; 6] = [
    tr!(en: "FIRST MARKS", zh: "初次落笔"),
    tr!(en: "SIGNAL PUZZLES", zh: "信号谜题"),
    tr!(en: "DISTANT LETTER", zh: "远方来信"),
    tr!(en: "MECHANICAL MAP", zh: "机关图纸"),
    tr!(en: "STAR VOYAGE", zh: "星海远航"),
    tr!(en: "HOME PANORAMA", zh: "归途全景"),
];

pub(crate) const CHAPTER_MECHANICS: [Phrase; 6] = [
    tr!(en: "5×5 · read the runs", zh: "5×5 · 读懂连续段"),
    tr!(en: "6×6 · gaps between runs", zh: "6×6 · 学会分隔与留白"),
    tr!(en: "7×7 · cross rows with columns", zh: "7×7 · 行列交叉推理"),
    tr!(en: "8×8 · start from the given cells", zh: "8×8 · 借给定的格子定位"),
    tr!(en: "9×9 · several runs per line", zh: "9×9 · 多段线索组合"),
    tr!(en: "10×10 · the full drawing", zh: "10×10 · 完整图纸复原"),
];
