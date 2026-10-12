use core::any::TypeId;

use crate::core::model::{BindType, ModelHandle};
use crate::core::reactive::retained::InputLease;
use crate::ecs::world::WorldId;
use crate::ecs::{Entity, World};

use super::{TextDocument, TextEditing};

pub(super) struct EditorIdentity {
    owner: WorldId,
    entity: Entity,
    attachment: Option<u64>,
    retained: Option<InputLease>,
    pub(super) document: Option<<TextDocument as BindType>::Shared>,
}

impl EditorIdentity {
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
        })
    }

    pub(super) fn resolve<'world>(&self, world: &'world World) -> Option<&'world TextEditing> {
        if self.owner != world.id()
            || !world.is_alive(self.entity)
            || self.retained.is_some_and(|lease| !lease.valid(world))
            || world.component_generation(self.entity, TypeId::of::<TextEditing>())
                != self.attachment
        {
            return None;
        }
        let editor = world.get::<TextEditing>(self.entity)?;
        if !editor.usable(world) {
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct EditVersion {
    document: u64,
    selection: u64,
}

impl EditVersion {
    fn capture(editor: &TextEditing) -> Self {
        Self {
            document: editor.read_document(TextDocument::revision),
            selection: editor.selection_revision,
        }
    }

    fn after_edit(self, content_changed: bool) -> Option<Self> {
        Some(Self {
            document: self.document.checked_add(u64::from(content_changed))?,
            ..self
        })
    }
}

pub(super) struct EditTarget {
    pub(super) identity: EditorIdentity,
    version: EditVersion,
}

impl EditTarget {
    pub(super) fn capture(world: &World, entity: Entity) -> Option<Self> {
        let identity = EditorIdentity::capture(world, entity)?;
        let version = EditVersion::capture(world.get::<TextEditing>(entity)?);
        Some(Self { identity, version })
    }

    pub(super) fn after_edit<'world>(
        &self,
        world: &'world World,
        content_changed: bool,
    ) -> Option<&'world TextEditing> {
        let editor = self.identity.resolve(world)?;
        (EditVersion::capture(editor) == self.version.after_edit(content_changed)?)
            .then_some(editor)
    }
}

#[cfg(test)]
mod tests;
