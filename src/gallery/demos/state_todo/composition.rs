use super::model::{TodoItemModel, TodoModel};
use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose]
fn todo_row(
    label: &'static str,
    completed_label: &'static str,
    item: model!(TodoItemModel),
) -> Entity {
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
                text: ${ alloc::string::String::from(if item.completed() { completed_label } else { label }) },
                grow: 1.0,
                paragraph: ParagraphStyle::label()
            )
        } on Tap { item.toggle(); }
    }
}

#[compose]
pub fn build_widgets(todo: model!(TodoModel)) {
    let milk = todo.milk();
    let docs = todo.docs();
    let release = todo.release();
    let _ = ui! {
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
            todo_row ("Buy milk", "✓  Buy milk", milk)
            todo_row ("Write docs", "✓  Write docs", docs)
            todo_row ("Ship release", "✓  Ship release", release)
        }
    };
}
