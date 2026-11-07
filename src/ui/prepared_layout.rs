use alloc::{boxed::Box, vec::Vec};

use crate::ecs::{Entity, World};
use crate::types::Rect;

use super::{
    layout::{LayoutNode, paths::LayoutPaths},
    property::PropertyChange,
};

pub(crate) struct PreparedLayout<'a> {
    pub(crate) tree: &'a mut LayoutNode,
    pub(crate) entities: &'a [Entity],
    pub(crate) paths: &'a LayoutPaths,
    pub(crate) viewport: Rect,
}

pub(crate) trait LayoutParticipant: 'static {
    fn active(world: &World) -> bool
    where
        Self: Sized;
    fn prepare(&mut self, world: &World, entities: &[Entity]) -> bool;
    fn has_geometry(&self) -> bool;
    fn needs_geometry(&self, entity: Entity) -> bool;
    fn invalidate(&mut self);
    fn reconcile(&mut self, world: &mut World, layout: &mut PreparedLayout<'_>) -> PropertyChange;
    #[cfg(test)]
    fn as_any(&self) -> &dyn core::any::Any;
}

pub(crate) struct LayoutHook {
    pub(crate) active: fn(&World) -> bool,
    create: fn() -> Box<dyn LayoutParticipant>,
}

impl LayoutHook {
    pub(crate) const fn new<P: LayoutParticipant + Default>() -> Self {
        Self {
            active: P::active,
            create: || Box::<P>::default(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RegisteredLayout {
    pub(crate) registration: u64,
    pub(crate) hook: &'static LayoutHook,
}

struct Entry {
    registration: u64,
    state: Box<dyn LayoutParticipant>,
}

#[derive(Default)]
pub(crate) struct LayoutParticipants {
    entries: Vec<Entry>,
    revision: u64,
}

impl LayoutParticipants {
    pub(crate) fn prepare(&mut self, world: &World, entities: &[Entity]) {
        let hooks = world
            .resource::<super::view::ViewRegistry>()
            .and_then(super::view::ViewRegistry::prepared_layout_plan);
        let hooks = hooks.as_deref().map_or(&[][..], |hooks| hooks.as_slice());
        let mut changed = false;
        self.entries.retain(|entry| {
            let keep = hooks
                .iter()
                .any(|hook| hook.registration == entry.registration && (hook.hook.active)(world));
            changed |= !keep;
            keep
        });
        for hook in hooks {
            if !(hook.hook.active)(world) {
                continue;
            }
            let index = self
                .entries
                .iter()
                .position(|entry| entry.registration == hook.registration);
            let index = match index {
                Some(index) => index,
                None => {
                    if self.entries.try_reserve_exact(1).is_err() {
                        crate::warn!(target: "mirui::layout", "layout participant storage unavailable");
                        continue;
                    }
                    self.entries.push(Entry {
                        registration: hook.registration,
                        state: (hook.hook.create)(),
                    });
                    changed = true;
                    self.entries.len() - 1
                }
            };
            changed |= self.entries[index].state.prepare(world, entities);
        }
        if self.entries.is_empty() {
            self.entries = Vec::new();
        }
        if changed {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("layout participant revision exhausted");
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) fn has_geometry(&self) -> bool {
        self.entries.iter().any(|entry| entry.state.has_geometry())
    }
    pub(crate) fn needs_geometry(&self, entity: Entity) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.state.needs_geometry(entity))
    }
    pub(crate) fn invalidate(&mut self) {
        for entry in &mut self.entries {
            entry.state.invalidate();
        }
    }
    pub(crate) fn release(&mut self) {
        *self = Self::default();
    }
    pub(crate) fn reconcile(
        &mut self,
        world: &mut World,
        layout: &mut PreparedLayout<'_>,
    ) -> PropertyChange {
        self.entries
            .iter_mut()
            .fold(PropertyChange::Unchanged, |change, entry| {
                change.merge(entry.state.reconcile(world, layout))
            })
    }
    #[cfg(test)]
    pub(crate) fn state<P: LayoutParticipant>(&self) -> Option<&P> {
        self.entries
            .iter()
            .find_map(|entry| entry.state.as_any().downcast_ref())
    }
}
