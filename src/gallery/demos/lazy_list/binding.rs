use crate::prelude::*;
use crate::ui::widgets::lazy_list::{CreateCell, ListItem};
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

pub(super) const ROW_H: i32 = 38;
pub(super) const ITEM_COUNT: u32 = 1 << 16;

#[event]
pub(super) fn create_row(event: event!(CreateCell)) -> Entity {
    ui! {
        row (event.item.clone())
    }
}

#[compose]
fn row(item: model!(ListItem)) -> Entity {
    ui! {
        Row (
            bg_color: ${ if item.index() % 2 == 0 { ColorToken::Surface } else { ColorToken::SurfaceVariant } },
            width: Dimension::percent(100),
            align: AlignItems::Center,
            padding: Padding {
                top: Dimension::px(0),
                right: Dimension::px(12),
                bottom: Dimension::px(0),
                left: Dimension::px(12),
            }
        ) {
            Text (
                text: ${ alloc::format!("Row {}", item.index()) },
                grow: 1.0,
                height: 24,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            View (
                width: 5,
                height: 5,
                bg_color: ColorToken::Primary,
                border_radius: 3
            )
        }
    }
}
