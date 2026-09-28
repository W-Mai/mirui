use super::style::{BLUE, CYAN, GOLD, VIOLET};
use crate::prelude::ColorToken;

#[derive(Clone, Copy)]
pub(super) struct LayoutChip {
    pub(super) label: &'static str,
    pub(super) color: ColorToken,
}

pub(super) const CHIPS: [LayoutChip; 5] = [
    LayoutChip {
        label: "CONTENT",
        color: CYAN,
    },
    LayoutChip {
        label: "GROW",
        color: BLUE,
    },
    LayoutChip {
        label: "SHRINK",
        color: VIOLET,
    },
    LayoutChip {
        label: "MIN / MAX",
        color: GOLD,
    },
    LayoutChip {
        label: "WRAP",
        color: CYAN,
    },
];
