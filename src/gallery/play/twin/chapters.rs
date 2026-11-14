pub(crate) const CHAPTER_NAMES: [&str; 6] = [
    "FIRST RESPONSE",
    "MIRROR ROUTE",
    "REVERSE TIDE",
    "KEY PROTOCOL",
    "RIGHT ANGLE",
    "TWIN FINALE",
];

pub(crate) const CHAPTER_MECHANICS: [&str; 6] = [
    "同向联动 · 撞墙的一位停住",
    "水平镜像 · 上下仍然同向",
    "完全反向 · 借障碍分离路线",
    "A 取钥片 · 打开两站闸门",
    "B 向右转 90° · ↑ 让 B 向右",
    "镜像加钥片 · 全部规则一起用",
];

/// Two lines for the briefing that opens each chapter.
pub(crate) const CHAPTER_TIPS: [[&str; 2]; 6] = [
    [
        "A 和 B 朝同一方向走。",
        "撞墙的一方停在原地，另一方继续走。",
    ],
    ["按 ← 时 B 向右，按 → 时 B 向左。", "↑ 和 ↓ 两边仍然同向。"],
    [
        "B 的四个方向全部反过来。",
        "让一方撞墙停住，就能改变两人的相对位置。",
    ],
    ["钥片只有 A 能取。", "取到后，下一步起两边的闸门都能通过。"],
    ["按 ↑ 时 B 向右，按 → 时 B 向下。", "钥片仍然只有 A 能取。"],
    ["B 左右相反，还要先取钥片。", "先想好撞墙的顺序，再出发。"],
];
