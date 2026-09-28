use crate::prelude::*;

pub(super) const ROW_H: i32 = 32;

#[derive(Clone)]
pub(super) struct Fruit {
    pub(super) name: &'static str,
    pub(super) color: ColorToken,
}

pub(super) const PALETTE: [Fruit; 6] = [
    Fruit {
        name: "Apple",
        color: ColorToken::Error,
    },
    Fruit {
        name: "Lime",
        color: ColorToken::Success,
    },
    Fruit {
        name: "Plum",
        color: ColorToken::Primary,
    },
    Fruit {
        name: "Mango",
        color: ColorToken::Secondary,
    },
    Fruit {
        name: "Berry",
        color: ColorToken::Tertiary,
    },
    Fruit {
        name: "Pear",
        color: ColorToken::SurfaceVariant,
    },
];
