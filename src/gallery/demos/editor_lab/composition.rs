use super::editors::{CommandEditor, SearchEditor, SharedEditor};
use super::keyboard::panel as keyboard_panel;
use super::model::{EditorLab, TOOLS};
use crate::prelude::*;
use crate::ui::clipboard::ClipboardFinished;
use crate::ui::widgets::{Button, ParagraphStyle, Placeholder, Text, TextEditing};

#[compose]
pub fn build_widgets(model: model!(EditorLab)) {
    ui! {
        Column (grow: 1.0, padding: Padding::all(20), row_gap: 8) {
            Text ("EDITOR LAB", height: 32, font_size: 22)
            Text ("01 / SEARCH", height: 20, font_size: 12, text_color: ColorToken::Primary)
            SearchEditor (
                id: "editor_search",
                document: Some(model.search()),
                width: Dimension::percent(100),
                height: 40,
                font_size: 16,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 8
            ) [
                Placeholder("Filter tools..."),
            ]
            Column (height: 100, row_gap: 2) {
                walk TOOLS.iter().enumerate() with tool {
                    Text (
                        *tool.1,
                        height: 24,
                        font_size: 14,
                        visible: ${ model.matches() & (1 << tool.0) != 0 },
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label()
                    )
                }
                Text ("No matching tools", height: 24, visible: ${ model.matches() == 0 })
            }
            Text ("02 / COMMAND", height: 20, font_size: 12, text_color: ColorToken::Success)
            Row (height: 40, align: AlignItems::Center, column_gap: 8) {
                Text (">", width: 20, height: 28, text_color: ColorToken::Success)
                CommandEditor (
                    id: "editor_command",
                    document: Some(model.command()),
                    grow: 1.0,
                    height: 40,
                    font_size: 16,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 8
                ) [
                    Placeholder("Type a command, press Enter"),
                ] on TextEditing::Submitted {
                    if let Err(error) = model.submit() {
                        crate::warn!("command rejected: {:?}", error);
                    }
                }
            }
            Text (
                text: ${ model.output() },
                height: 28,
                font_size: 12,
                text_color: ${ if model.accent() { ColorToken::Success } else { ColorToken::Primary } },
                paragraph: ParagraphStyle::label()
            )
            Text (
                "03 / SHARED DOCUMENT",
                height: 20,
                font_size: 12,
                text_color: ColorToken::Secondary
            )
            SharedEditor (
                id: "editor_shared_a",
                document: Some(model.shared()),
                width: Dimension::percent(100),
                height: 40,
                font_size: 16,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 8
            ) on ClipboardFinished(|event: event!(ClipboardFinished)| {
                model.clipboard_finished(event.result);
            })
            SharedEditor (
                id: "editor_shared_b",
                document: Some(model.shared()),
                width: Dimension::percent(100),
                height: 40,
                font_size: 16,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 8
            )
            shared_actions (model.clone(), id("editor_shared_a"))
            Text (text: ${ model.clipboard_status() }, height: 20, font_size: 12)
            keyboard_panel (model.keyboard())
        }
    };
}

#[compose]
fn shared_actions(model: model!(EditorLab), editor: Entity) -> Entity {
    ui! {
        Row (height: 34, column_gap: 8) {
            Button (id: "editor_select_all", width: 112, font_size: 12, border_radius: 8) [
                PointerFocus::Preserve,
                Text::label("Select all"),
            ] on Tap { commands!().selection().select_all(editor); }
            Button (id: "editor_copy", width: 80, font_size: 12, border_radius: 8) [
                PointerFocus::Preserve,
                Text::label("Copy"),
            ] on Tap { commands!().clipboard().copy(editor); }
            Button (id: "editor_replace", height: 34, width: 144, font_size: 12, border_radius: 8) [
                PointerFocus::Preserve,
                Text::label("Replace document"),
            ] on Tap {
                if let Err(error) = model.reset_shared() {
                    crate::warn!("document rejected: {:?}", error);
                }
            }
        }
    }
}
