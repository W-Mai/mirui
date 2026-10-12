//! Selection commands for text and custom selectable components.

use crate::ecs::commands::DeferredCommands;
use crate::ecs::{CommandCapacity, Entity};
use crate::input::event::contract::{EntityRequest, EntitySource, Event};

pub use crate::input::event::contract::RequestError;

/// Select all content owned by the receiving component.
///
/// Register a custom handler through `on SelectAll` or a View's `input` callbacks. The handler returns `Result<(), RequestError>`; the request does not bubble or enter the notification queue.
pub struct SelectAll;

impl Event for SelectAll {
    type Route = EntityRequest<Result<(), RequestError>>;
    type Source = EntitySource;
    type Options = ();
}

#[doc(hidden)]
pub trait SelectionSelectors {
    #[allow(non_upper_case_globals)]
    const SelectAll: crate::input::event::contract::EventKey<SelectAll> = SelectAll::EVENT;
}
impl<T: crate::ecs::Component> SelectionSelectors for T {}

/// Deferred operations on an explicitly targeted component's selection.
pub struct SelectionCommands<'call, 'queue> {
    commands: &'call mut DeferredCommands<'queue>,
}

impl<'queue> DeferredCommands<'queue> {
    pub fn selection(&mut self) -> SelectionCommands<'_, 'queue> {
        SelectionCommands { commands: self }
    }
}

impl CommandCapacity {
    /// Reserve selection operations, independently of clipboard storage.
    pub const fn selection(self, count: usize) -> Self {
        self.entity_value::<()>(count)
    }
}

impl SelectionCommands<'_, '_> {
    /// Select the target's complete content without changing keyboard focus.
    pub fn select_all(&mut self, target: Entity) {
        self.commands.apply(target, (), |world, target, ()| {
            let result = crate::input::event::request::invoke(world, target, &SelectAll)
                .and_then(core::convert::identity);
            if let Err(error) = result {
                crate::warn!(target: "mirui::selection", "select-all skipped: {:?}", error);
            }
            Ok(())
        });
    }
}
