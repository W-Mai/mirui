use super::model::{TodoItemModel, TodoModel};
use crate::core::model::ModelHandle;
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::ui::icons::ICON_CHECK;
use crate::ui::widgets::{Button, Icon, ParagraphStyle, Text};

#[compose]
fn todo_row(item: model!(TodoItemModel)) -> Entity {
    let title = item.read(|item| item.title.clone());
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
                text: ${ alloc::format!("{} remaining", todo.remaining()) },
                width: Dimension::percent(100),
                max_width: 280,
                height: 30,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Row (
                width: Dimension::percent(100),
                max_width: 280,
                height: 38,
                padding: Padding::all(3),
                column_gap: 3,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 11
            ) {
                Button (
                    grow: 1.0,
                    font_size: 13,
                    border_radius: 8,
                    text_color: ColorToken::OnPrimary,
                    normal_color: ColorToken::Primary
                ) [
                    Text::label("Add"),
                ] on Tap {
                    if let Err(error) = todo.add() {
                        crate::warn!("cannot add task: {:?}", error);
                    }
                }
                Button (
                    grow: 1.0,
                    font_size: 13,
                    border_radius: 8,
                    normal_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) [
                    Text::label("Rotate"),
                ] on Tap { todo.rotate(); }
                Button (
                    grow: 1.0,
                    font_size: 13,
                    border_radius: 8,
                    normal_color: ColorToken::Outline,
                    text_color: ColorToken::OnSurface
                ) [
                    Text::label("Clear"),
                ] on Tap { todo.clear_completed(); }
            }
            Column (
                grow: 1.0,
                width: Dimension::percent(100),
                max_width: 280,
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
