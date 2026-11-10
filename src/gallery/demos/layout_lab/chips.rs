use super::style::{ACCENT, RAISED};
use crate::prelude::ColorToken;

#[derive(Clone, Copy)]
pub(super) struct LayoutChip {
    pub(super) label: &'static str,
    /// Fill of the chip's step in the walk row.
    pub(super) color: ColorToken,
}

pub(super) const CHIPS: [LayoutChip; 5] = [
    LayoutChip {
        label: "Content",
        color: ACCENT,
    },
    LayoutChip {
        label: "Grow",
        color: RAISED,
    },
    LayoutChip {
        label: "Shrink",
        color: RAISED,
    },
    LayoutChip {
        label: "Min · max",
        color: RAISED,
    },
    LayoutChip {
        label: "Wrap",
        color: RAISED,
    },
];
