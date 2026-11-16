use crate::core::i18n::Phrase;
use crate::tr;

pub(crate) const CHAPTER_NAMES: [Phrase; 6] = [
    tr!(en: "FIRST RESPONSE", zh: "初次联动"),
    tr!(en: "MIRROR ROUTE", zh: "镜像航线"),
    tr!(en: "REVERSE TIDE", zh: "反向潮汐"),
    tr!(en: "KEY PROTOCOL", zh: "钥片协议"),
    tr!(en: "RIGHT ANGLE", zh: "直角转向"),
    tr!(en: "TWIN FINALE", zh: "双子终章"),
];

pub(crate) const CHAPTER_MECHANICS: [Phrase; 6] = [
    tr!(en: "Same direction · a blocked beacon waits", zh: "同向联动 · 撞墙的一位停住"),
    tr!(en: "Left and right mirrored · up and down shared", zh: "水平镜像 · 上下仍然同向"),
    tr!(en: "B moves opposite · split them on walls", zh: "完全反向 · 借障碍分离路线"),
    tr!(en: "A takes the key · both gates open", zh: "A 取钥片 · 打开两站闸门"),
    tr!(en: "B turns 90° right · ↑ sends B right", zh: "B 向右转 90° · ↑ 让 B 向右"),
    tr!(en: "Mirror and key · every rule at once", zh: "镜像加钥片 · 全部规则一起用"),
];

/// Two lines for the briefing that opens each chapter.
pub(crate) const CHAPTER_TIPS: [[Phrase; 2]; 6] = [
    [
        tr!(en: "A and B move the same way.", zh: "A 和 B 朝同一方向走。"),
        tr!(
            en: "A blocked beacon stays put; the other keeps going.",
            zh: "撞墙的一方停在原地，另一方继续走。"
        ),
    ],
    [
        tr!(en: "← sends B right, → sends B left.", zh: "按 ← 时 B 向右，按 → 时 B 向左。"),
        tr!(en: "↑ and ↓ still move both the same way.", zh: "↑ 和 ↓ 两边仍然同向。"),
    ],
    [
        tr!(en: "All four of B's directions are reversed.", zh: "B 的四个方向全部反过来。"),
        tr!(
            en: "Stop one beacon on a wall to change their spacing.",
            zh: "让一方撞墙停住，就能改变两人的相对位置。"
        ),
    ],
    [
        tr!(en: "Only A can pick up the key.", zh: "钥片只有 A 能取。"),
        tr!(
            en: "From the next move on, both gates let you through.",
            zh: "取到后，下一步起两边的闸门都能通过。"
        ),
    ],
    [
        tr!(en: "↑ sends B right, → sends B down.", zh: "按 ↑ 时 B 向右，按 → 时 B 向下。"),
        tr!(en: "Only A can take the key, as before.", zh: "钥片仍然只有 A 能取。"),
    ],
    [
        tr!(
            en: "B's left and right are swapped, and the key comes first.",
            zh: "B 左右相反，还要先取钥片。"
        ),
        tr!(en: "Plan which walls to stop on before you set off.", zh: "先想好撞墙的顺序，再出发。"),
    ],
];
