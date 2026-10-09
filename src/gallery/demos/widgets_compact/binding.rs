use super::style::bitmap_label;
use crate::prelude::*;
use crate::ui::widgets::lazy_list::CreateCell;
use crate::ui::widgets::{Text, TextAlign};

pub(super) const ROW_HEIGHT: i32 = 12;
pub(super) const VIRTUAL_ITEM_COUNT: u32 = 600_000;
const ROW_LABELS: [&str; 8] = [
    "ALPHA", "BETA", "GAMMA", "DELTA", "EPS", "ZETA", "ETA", "THETA",
];

#[event]
pub(super) fn create_row(event: event!(CreateCell)) -> Entity {
    let item = event.item.clone();
    ui! {
        Row (
            width: Dimension::percent(100),
            padding: Padding {
                top: Dimension::px(0),
                right: Dimension::px(7),
                bottom: Dimension::px(0),
                left: Dimension::px(7),
            },
            align: AlignItems::Center,
            bg_color: ColorToken::SurfaceVariant
        ) {
            Text (
                text: ${ ROW_LABELS[item.index() as usize % ROW_LABELS.len()] },
                grow: 1.0,
                font: FontToken::Default,
                font_size: 6,
                text_color: ColorToken::OnSurface,
                paragraph: bitmap_label(TextAlign::Start)
            )
            View (
                width: 4,
                height: 4,
                bg_color: ColorToken::Primary,
                border_radius: 2
            )
        }
    }
}
