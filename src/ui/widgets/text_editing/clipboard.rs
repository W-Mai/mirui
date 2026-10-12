use crate::core::model::{BindType, ModelHandle};
use crate::ui::clipboard::{
    ClipboardData, ClipboardError, CopySelection, CutSelection, InspectSelection, PasteSelection,
    SelectionState,
};

use super::{TextDocument, TextEditing};

struct Context {
    document: Option<<TextDocument as BindType>::Shared>,
    content: u64,
    selection: u64,
    composition: u64,
}
impl PartialEq for Context {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
            && self.selection == other.selection
            && self.composition == other.composition
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
    if !editor.usable_by(crate::core::reactive::current_world_id())
        || editor.preedit.revision == u64::MAX
    {
        return Err(ClipboardError::Unavailable);
    }
    Ok(SelectionState::new(
        !editor.selection().range().is_empty(),
        Context {
            document: editor.document.clone(),
            content: editor.read_document(TextDocument::revision),
            selection: editor.selection_revision,
            composition: editor.preedit.revision,
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

#[crate::event]
pub(super) fn cut(
    node: entity!(),
    _request: event!(CutSelection),
    editors: query!(TextEditing),
    commands: commands!(),
) -> Result<(), ClipboardError> {
    let editor = editors.get(node).ok_or(ClipboardError::Unavailable)?;
    if !editor.usable_by(crate::core::reactive::current_world_id()) {
        return Err(ClipboardError::Unavailable);
    }
    super::input::validate_replacement(editor, "").map_err(|_| ClipboardError::Unavailable)?;
    commands.apply_event::<CutSelection>(node, |world, node, _| {
        super::input::replace_from_clipboard(world, node, "")
    });
    Ok(())
}

#[crate::event]
pub(super) fn paste(
    node: entity!(),
    event: event!(PasteSelection),
    editors: query!(TextEditing),
    commands: commands!(),
) -> Result<(), ClipboardError> {
    let editor = editors.get(node).ok_or(ClipboardError::Unavailable)?;
    if !editor.usable_by(crate::core::reactive::current_world_id()) {
        return Err(ClipboardError::Unavailable);
    }
    let text = paste_text(event)?;
    super::input::validate_replacement(editor, text).map_err(|error| match error {
        super::TextEditError::Capacity { .. } | super::TextEditError::ReserveFailed { .. } => {
            ClipboardError::Capacity
        }
        super::TextEditError::RevisionExhausted => ClipboardError::Unavailable,
        _ => ClipboardError::InvalidData,
    })?;
    commands.apply_event::<PasteSelection>(node, |world, node, event| {
        super::input::replace_from_clipboard(
            world,
            node,
            paste_text(event).expect("validated text clipboard"),
        )
    });
    Ok(())
}

fn paste_text(event: &PasteSelection) -> Result<&str, ClipboardError> {
    let [item] = event.data().items() else {
        return Err(ClipboardError::Unsupported);
    };
    let format = item
        .formats()
        .iter()
        .find(|format| format.mime().eq_ignore_ascii_case("text/plain"))
        .ok_or(ClipboardError::Unsupported)?;
    core::str::from_utf8(format.bytes()).map_err(|_| ClipboardError::InvalidData)
}
