#![doc = include_str!("../../../docs/text-editing.md")]

use crate::core::model::{BindType, ModelHandle};

use super::text_document::{TextDocument, TextEditError};

pub(crate) mod edit_target;
pub(crate) mod geometry;
mod input;
mod preedit;

/// Caret visibility shared by editors using the App clock.
#[derive(Default, Clone, Copy)]
pub struct CursorBlinkPhase(pub bool);

crate::input::event::contract::widget_events! {
    TextEditing;
    /// The editor changed the document's content.
    Changed { len: u32 },
    /// The editor changed its directed UTF-8 selection.
    SelectionChanged { anchor: u32, cursor: u32 },
    /// The editor accepted Return.
    Submitted,
}

/// Directed selection expressed as UTF-8 byte offsets.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextSelection {
    pub anchor: usize,
    pub cursor: usize,
}

impl TextSelection {
    pub fn range(self) -> core::ops::Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    fn normalized(self, text: &str) -> Self {
        fn boundary(text: &str, offset: usize) -> usize {
            let mut offset = offset.min(text.len());
            while !text.is_char_boundary(offset) {
                offset -= 1;
            }
            offset
        }
        Self {
            anchor: boundary(text, self.anchor),
            cursor: boundary(text, self.cursor),
        }
    }
}

/// Text document binding and selection belonging to one editor instance.
///
/// The default owns an inline document. `new(document)` binds a registered `TextDocument`; selection remains local to this component. Use the registered document's methods to change shared content.
#[crate::component]
#[derive(Clone)]
pub struct TextEditing {
    local: Option<TextDocument>,
    document: model!(optional TextDocument),
    selection: TextSelection,
    selection_revision: u64,
    drag_revision: Option<edit_target::EditVersion>,
    focused: bool,
    preedit: preedit::Preedit,
}

impl Default for TextEditing {
    fn default() -> Self {
        Self {
            local: Some(TextDocument::new()),
            document: None,
            selection: TextSelection::default(),
            selection_revision: 0,
            drag_revision: None,
            focused: false,
            preedit: preedit::Preedit::default(),
        }
    }
}

impl TextEditing {
    pub fn new(document: <TextDocument as BindType>::Shared) -> Self {
        Self {
            local: None,
            document: Some(document),
            selection: TextSelection::default(),
            selection_revision: 0,
            drag_revision: None,
            focused: false,
            preedit: preedit::Preedit::default(),
        }
    }

    fn read_document<R>(&self, read: impl FnOnce(&TextDocument) -> R) -> R {
        match &self.document {
            Some(document) => ModelHandle::read(document, read),
            None => read(self.local.as_ref().expect("local editor owns its document")),
        }
    }

    /// Read the bound document without copying it or returning a borrowed reference.
    pub fn read<R>(&self, read: impl FnOnce(&str) -> R) -> R {
        self.read_document(|document| document.read(read))
    }

    pub fn len(&self) -> usize {
        self.read_document(TextDocument::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.read_document(TextDocument::capacity)
    }

    /// Prepare the document's byte limit before attaching the editor.
    pub fn with_capacity(mut self, capacity: usize) -> Result<Self, TextEditError> {
        match &self.document {
            Some(document) => document.set_capacity(capacity)?,
            None => self.local.as_mut().unwrap().set_capacity(capacity)?,
        }
        Ok(self)
    }

    /// Initialize content and place this editor's caret at its end.
    pub fn with_text(mut self, text: &str) -> Result<Self, TextEditError> {
        match &self.document {
            Some(document) => document.set_text(text)?,
            None => self.local.as_mut().unwrap().set_text(text)?,
        }
        self.set_selection(TextSelection {
            anchor: text.len(),
            cursor: text.len(),
        })?;
        Ok(self)
    }

    /// Resolve this editor's endpoints against the current document. External edits clamp offsets to the end and to preceding UTF-8 boundaries.
    pub fn selection(&self) -> TextSelection {
        self.read(|text| self.selection.normalized(text))
    }

    pub fn cursor(&self) -> usize {
        self.selection().cursor
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    pub(crate) fn usable(&self, world: &crate::ecs::World) -> bool {
        self.usable_by(Some(world.id()))
    }

    fn usable_by(&self, owner: Option<crate::ecs::world::WorldId>) -> bool {
        self.document.as_ref().is_none_or(|document| {
            owner.is_some()
                && crate::core::model::try_registered_owner(document) == owner
                && document.callback_ready()
        })
    }

    /// Validate both endpoints before changing either. Selection changes do not modify the shared document or another editor's selection.
    pub fn set_selection(&mut self, selection: TextSelection) -> Result<bool, TextEditError> {
        let previous = self.read(|text| {
            for offset in [selection.anchor, selection.cursor] {
                if !text.is_char_boundary(offset) {
                    return Err(TextEditError::InvalidBoundary { offset });
                }
            }
            Ok(self.selection.normalized(text))
        })?;
        let changed = previous != selection;
        let revision = if changed {
            self.selection_revision
                .checked_add(1)
                .ok_or(TextEditError::RevisionExhausted)?
        } else {
            self.selection_revision
        };
        self.selection = selection;
        self.selection_revision = revision;
        Ok(changed)
    }
}

#[crate::view(
    component = TextEditing,
    name = "TextEditing",
    invalidation = Layout,
    attach = input::attach::callback(),
    input = (input::key::callback(), input::committed::callback(), input::tap::callback(), input::preedit::callback(), input::composition_ended::callback(), input::drag_start::callback(), input::drag_move::callback(), input::drag_end::callback(), input::drag_cancel::callback(), input::select_all::callback()),
    systems = [input::normalize_selections::system(), input::cursor_blink::system()]
)]
fn text_editing_state(_document: model!(optional component.document, watch(revision, capacity))) {}

pub use text_editing_state::view;

#[cfg(test)]
mod tests;
