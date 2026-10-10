use super::model::{TodoItemModel, TodoModel};
use crate::prelude::*;
use crate::ui::icons::ICON_CHECK;
use crate::ui::widgets::{Button, Icon, ParagraphStyle, Text};

#[compose]
fn todo_row(label: &'static str, item: model!(TodoItemModel)) -> Entity {
    ui! {
        Button (
            width: Dimension::percent(100),
            max_width: 280,
            height: 40,
            border_radius: 10,
            normal_color: ColorToken::SurfaceVariant,
            pressed_color: ColorToken::Primary,
            text_color: ColorToken::OnSurface
        ) {
            Text (
                label,
                grow: 1.0,
                paragraph: ParagraphStyle::label()
            )
            Icon (
                path: ICON_CHECK,
                color: ThemedColor::Token(ColorToken::Success),
                position: Position::Absolute,
                left: 12,
                top: 10,
                width: 20,
                height: 20,
                visible: ${ item.completed() }
            )
        } on Tap { item.toggle(); }
    }
}

#[compose]
pub fn build_widgets(todo: model!(TodoModel)) {
    let rows = [
        ("Buy milk", todo.milk()),
        ("Write docs", todo.docs()),
        ("Ship release", todo.release()),
    ];
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 8
        ) {
            Text (
                text: ${ alloc::format!("{} REMAINING", todo.remaining()) },
                width: Dimension::percent(100),
                max_width: 280,
                height: 38,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            walk rows with row {
                todo_row (row.0, row.1)
            }
        }
    };
}
