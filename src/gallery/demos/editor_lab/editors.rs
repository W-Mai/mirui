use crate::prelude::*;
use crate::ui::widgets::{TextDocument, TextEditing, TextInput};

#[crate::component]
#[derive(Default)]
pub(super) struct SearchEditor {
    pub(super) document: model!(optional TextDocument),
}

#[crate::component]
#[derive(Default)]
pub(super) struct CommandEditor {
    pub(super) document: model!(optional TextDocument),
}

#[crate::component]
#[derive(Default)]
pub(super) struct SharedEditor {
    pub(super) document: model!(optional TextDocument),
}

#[crate::event]
fn search_attach(
    node: entity!(),
    rows: query!(SearchEditor),
    editing: query!(TextEditing),
    commands: commands!(),
) {
    let Some(editor) = rows.get(node) else {
        return;
    };
    if editing.get(node).is_none() {
        commands.insert(
            node,
            editor
                .document
                .clone()
                .map(TextEditing::new)
                .unwrap_or_default(),
        );
    }
    commands.insert(
        node,
        TextInput::new().with_cursor_color(ColorToken::Primary),
    );
}

#[crate::event]
fn command_attach(
    node: entity!(),
    rows: query!(CommandEditor),
    editing: query!(TextEditing),
    commands: commands!(),
) {
    let Some(editor) = rows.get(node) else {
        return;
    };
    if editing.get(node).is_none() {
        commands.insert(
            node,
            editor
                .document
                .clone()
                .map(TextEditing::new)
                .unwrap_or_default(),
        );
    }
    commands.insert(
        node,
        TextInput::new().with_cursor_color(ColorToken::Success),
    );
}

#[crate::event]
fn shared_attach(
    node: entity!(),
    rows: query!(SharedEditor),
    editing: query!(TextEditing),
    commands: commands!(),
) {
    let Some(editor) = rows.get(node) else {
        return;
    };
    if editing.get(node).is_none() {
        commands.insert(
            node,
            editor
                .document
                .clone()
                .map(TextEditing::new)
                .unwrap_or_default(),
        );
    }
    commands.insert(
        node,
        TextInput::new().with_cursor_color(ColorToken::Secondary),
    );
}

#[crate::view(component = SearchEditor, attach = search_attach::callback())]
pub(super) fn search_view() {}

#[crate::view(component = CommandEditor, attach = command_attach::callback())]
pub(super) fn command_view() {}

#[crate::view(component = SharedEditor, attach = shared_attach::callback())]
pub(super) fn shared_view() {}
