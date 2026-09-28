use super::model::{TodoItem, TodoModel};
use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose(bind(todo))]
fn todo_row(
    label: &'static str,
    completed_label: &'static str,
    item: TodoItem,
    todo: TodoModel,
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
        ) on Tap { todo.toggle(item); }
        {
            Text (
                text: ${
                    let done = match item {
                        TodoItem::Milk => todo.milk(),
                        TodoItem::Docs => todo.docs(),
                        TodoItem::Release => todo.release(),
                    };
                    alloc::string::String::from(if done { completed_label } else { label })
                },
                grow: 1.0,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose(bind(todo))]
pub fn build_widgets(todo: TodoModel) {
    let milk = TodoItem::Milk;
    let docs = TodoItem::Docs;
    let release = TodoItem::Release;
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
            todo_row ("Buy milk", "✓  Buy milk", milk, todo)
            todo_row ("Write docs", "✓  Write docs", docs, todo)
            todo_row ("Ship release", "✓  Ship release", release, todo)
        }
    };
}
