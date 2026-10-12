use crate::core::model::{BindType, ModelHandle};
use crate::ui::clipboard::{
    ClipboardData, ClipboardError, CopySelection, InspectSelection, SelectionState,
};

use super::{TextDocument, TextEditing};

struct Context {
    document: Option<<TextDocument as BindType>::Shared>,
    content: u64,
    selection: u64,
}
impl PartialEq for Context {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
            && self.selection == other.selection
            && match (&self.document, &other.document) {
                (Some(left), Some(right)) => left.access().same_identity(right.access()),
                (None, None) => true,
                _ => false,
            }
    }
}
impl Eq for Context {}

#[crate::event]
pub(super) fn inspect(
    node: entity!(),
    _request: event!(InspectSelection),
    editors: query!(TextEditing),
) -> Result<SelectionState, ClipboardError> {
    let editor = editors.get(node).ok_or(ClipboardError::Unavailable)?;
    if !editor.usable_by(crate::core::reactive::current_world_id()) {
        return Err(ClipboardError::Unavailable);
    }
    Ok(SelectionState::new(
        !editor.selection().range().is_empty(),
        Context {
            document: editor.document.clone(),
            content: editor.read_document(TextDocument::revision),
            selection: editor.selection_revision,
        },
    ))
}

#[crate::event]
pub(super) fn copy(
    node: entity!(),
    _request: event!(CopySelection),
    editors: query!(TextEditing),
) -> Result<ClipboardData, ClipboardError> {
    let editor = editors.get(node).ok_or(ClipboardError::Unavailable)?;
    if !editor.usable_by(crate::core::reactive::current_world_id()) {
        return Err(ClipboardError::Unavailable);
    }
    let range = editor.selection().range();
    editor.read(|text| ClipboardData::text(&text[range]))
}
