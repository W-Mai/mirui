use super::model::{TodoItemModel, TodoModel};
use crate::core::model::ModelHandle;
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::icons::ICON_CHECK;
use crate::ui::widgets::{Button, Icon, ParagraphStyle, Text};

const ACTION: StylePreset = style!(grow: 1.0, border_radius: 17);

fn list_width() -> StylePreset {
    style!(width: Dimension::percent(100), max_width: 280)
}

#[compose]
fn todo_row(item: model!(TodoItemModel)) -> Entity {
    let title = item.read(|item| item.title.clone());
    ui! {
        Button (
            style: list_width(),
            height: 40,
            border_radius: 14,
            normal_color: ColorToken::SurfaceVariant,
            pressed_color: ColorToken::Primary,
            text_color: ColorToken::OnSurface
        ) {
            Text (
                text: title,
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
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            padding: Padding::all(16),
            row_gap: 8
        ) {
            Text (
                text: ${ format_args!("{} remaining", todo.remaining()) },
                capacity: usize::MAX.ilog10() as usize + 1 + " remaining".len(),
                style: list_width(),
                height: 30,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Row (
                style: list_width(),
                height: 42,
                padding: Padding::all(4),
                column_gap: 4,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 21,
                font_size: 13
            ) {
                Button (
                    "Add",
                    style: ACTION,
                    text_color: ColorToken::OnPrimary,
                    normal_color: ColorToken::Primary
                ) on Tap {
                    if let Err(error) = todo.add() {
                        crate::warn!("cannot add task: {:?}", error);
                    }
                }
                Button (
                    "Rotate",
                    style: ACTION,
                    normal_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { todo.rotate(); }
                Button (
                    "Clear",
                    style: ACTION,
                    normal_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) on Tap { todo.clear_completed(); }
            }
            Column (
                grow: 1.0,
                style: list_width(),
                row_gap: 8,
                clip_children: true
            ) [
                ScrollOffset::default(),
                ScrollConfig::default(),
            ] {
                walk ${ todo.items().entries() } with row by row.0 {
                    todo_row (row.1)
                }
            }
        }
    };
}
