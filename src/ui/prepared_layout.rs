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
    fn needed(world: &World) -> bool
    where
        Self: Sized,
    {
        Self::active(world)
    }
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
    pub(crate) needed: fn(&World) -> bool,
    create: fn() -> Box<dyn LayoutParticipant>,
}

impl LayoutHook {
    pub(crate) const fn new<P: LayoutParticipant + Default>() -> Self {
        Self {
            active: P::active,
            needed: P::needed,
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
        let mut changed = self.retain_registered(world, hooks);
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
        self.finish_preparation(changed);
    }

    pub(crate) fn retire_inactive(&mut self, world: &World) {
        if self.entries.is_empty() {
            return;
        }
        let hooks = world
            .resource::<super::view::ViewRegistry>()
            .and_then(super::view::ViewRegistry::prepared_layout_plan);
        let hooks = hooks.as_deref().map_or(&[][..], |hooks| hooks.as_slice());
        let changed = self.retain_registered(world, hooks);
        self.finish_preparation(changed);
    }

    fn retain_registered(&mut self, world: &World, hooks: &[RegisteredLayout]) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| {
            hooks
                .iter()
                .any(|hook| hook.registration == entry.registration && (hook.hook.active)(world))
        });
        self.entries.len() != before
    }

    fn finish_preparation(&mut self, changed: bool) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::ui::View;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Work {
        active: bool,
        needed: bool,
        prepares: Rc<Cell<usize>>,
        drops: Rc<Cell<usize>>,
    }

    #[derive(Default)]
    struct Participant(Option<Rc<Cell<usize>>>);

    impl Drop for Participant {
        fn drop(&mut self) {
            if let Some(drops) = &self.0 {
                drops.set(drops.get() + 1);
            }
        }
    }

    impl LayoutParticipant for Participant {
        fn active(world: &World) -> bool {
            world.resource::<Work>().is_some_and(|work| work.active)
        }
        fn needed(world: &World) -> bool {
            world
                .resource::<Work>()
                .is_some_and(|work| work.active && work.needed)
        }
        fn prepare(&mut self, world: &World, _: &[Entity]) -> bool {
            let work = world.resource::<Work>().unwrap();
            work.prepares.set(work.prepares.get() + 1);
            self.0.get_or_insert_with(|| Rc::clone(&work.drops));
            false
        }
        fn has_geometry(&self) -> bool {
            false
        }
        fn needs_geometry(&self, _: Entity) -> bool {
            false
        }
        fn invalidate(&mut self) {}
        fn reconcile(&mut self, _: &mut World, _: &mut PreparedLayout<'_>) -> PropertyChange {
            PropertyChange::Unchanged
        }
        fn as_any(&self) -> &dyn core::any::Any {
            self
        }
    }

    #[test]
    fn idle_maintenance_does_not_create_or_prepare_participants_and_retires_exits() {
        let mut app = App::headless(16, 16);
        app.with_widget(
            View::systems_only("IdleParticipant", &[]).with_prepared_layout::<Participant>(),
        );
        let prepares = Rc::new(Cell::new(0));
        let drops = Rc::new(Cell::new(0));
        app.add_resource(Work {
            active: true,
            needed: false,
            prepares: Rc::clone(&prepares),
            drops: Rc::clone(&drops),
        });
        let mut participants = LayoutParticipants::default();
        participants.retire_inactive(&app.world);
        assert!(participants.entries.is_empty());
        assert_eq!(prepares.get(), 0);

        app.world.resource_mut::<Work>().unwrap().needed = true;
        participants.prepare(&app.world, &[]);
        assert_eq!(prepares.get(), 1);
        let revision = participants.revision();
        app.world.resource_mut::<Work>().unwrap().needed = false;
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..8 {
                    participants.retire_inactive(&app.world);
                }
            }),
            0
        );
        assert_eq!(prepares.get(), 1);
        assert_eq!(drops.get(), 0);
        assert_eq!(participants.revision(), revision);

        app.world.resource_mut::<Work>().unwrap().needed = true;
        participants.prepare(&app.world, &[]);
        assert_eq!(prepares.get(), 2);
        assert_eq!(drops.get(), 0);
        app.world.resource_mut::<Work>().unwrap().active = false;
        participants.retire_inactive(&app.world);
        assert_eq!(drops.get(), 1);
        assert!(participants.entries.is_empty());
        assert_eq!(participants.entries.capacity(), 0);
        assert!(participants.revision() > revision);

        app.world.resource_mut::<Work>().unwrap().active = true;
        participants.prepare(&app.world, &[]);
        app.world.remove_resource::<crate::ui::view::ViewRegistry>();
        participants.retire_inactive(&app.world);
        assert_eq!(drops.get(), 2);
        assert!(participants.entries.is_empty());
    }
}
