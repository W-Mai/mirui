use core::any::TypeId;

use crate::core::model::{BindType, ModelHandle};
use crate::core::reactive::retained::InputLease;
use crate::ecs::world::WorldId;
use crate::ecs::{Entity, World};

use super::{TextDocument, TextEditing};

pub(super) struct EditTarget {
    owner: WorldId,
    entity: Entity,
    attachment: Option<u64>,
    retained: Option<InputLease>,
    pub(super) document: Option<<TextDocument as BindType>::Shared>,
    document_revision: u64,
    selection_revision: u64,
}

impl EditTarget {
    pub(super) fn capture(world: &World, entity: Entity) -> Option<Self> {
        let editor = world.get::<TextEditing>(entity)?;
        if !editor.usable(world) {
            return None;
        }
        let retained = InputLease::capture(world, entity);
        if retained.is_some_and(|lease| !lease.valid(world)) {
            return None;
        }
        Some(Self {
            owner: world.id(),
            entity,
            attachment: world.component_generation(entity, TypeId::of::<TextEditing>()),
            retained,
            document: editor.document.clone(),
            document_revision: editor.read_document(TextDocument::revision),
            selection_revision: editor.selection_revision,
        })
    }

    pub(super) fn after_edit<'world>(
        &self,
        world: &'world World,
        content_changed: bool,
    ) -> Option<&'world TextEditing> {
        if self.owner != world.id()
            || !world.is_alive(self.entity)
            || self.retained.is_some_and(|lease| !lease.valid(world))
            || world.component_generation(self.entity, TypeId::of::<TextEditing>())
                != self.attachment
        {
            return None;
        }
        let editor = world.get::<TextEditing>(self.entity)?;
        if !editor.usable(world)
            || editor.selection_revision != self.selection_revision
            || editor.read_document(TextDocument::revision)
                != self
                    .document_revision
                    .checked_add(u64::from(content_changed))?
        {
            return None;
        }
        let same_document = match (&self.document, &editor.document) {
            (Some(old), Some(current)) => old.access().same_identity(current.access()),
            (None, None) => true,
            _ => false,
        };
        same_document.then_some(editor)
    }
}

#[cfg(test)]
mod tests;
