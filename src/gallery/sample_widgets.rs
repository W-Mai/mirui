use crate::prelude::*;
use crate::ui::widgets::lazy_list::{CreateCell, ListItem};
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

pub(super) const NUMBERED_ROW_HEIGHT: i32 = 38;

#[event]
pub(super) fn create_numbered_row(event: event!(CreateCell), inset: Dimension) -> Entity {
    ui! {
        numbered_list_row (event.item.clone(), inset)
    }
}

#[compose]
fn numbered_list_row(item: model!(ListItem), inset: Dimension) -> Entity {
    ui! {
        Row (
            bg_color: ${ if item.index() % 2 == 0 { ColorToken::Surface } else { ColorToken::SurfaceVariant } },
            width: Dimension::percent(100),
            align: AlignItems::Center,
            padding: Padding {
                top: Dimension::px(0),
                right: inset,
                bottom: Dimension::px(0),
                left: inset,
            }
        ) {
            Text (
                text: ${ format_args!("Row {}", item.index()) },
                capacity: 14,
                grow: 1.0,
                height: 24,
                font_size: 14,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            View (width: 6, height: 6, bg_color: ColorToken::Primary, border_radius: 3)
        }
    }
}
