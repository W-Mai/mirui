use alloc::rc::Rc;
use core::cell::RefCell;

use crate::ecs::{Entity, World};
use crate::ui::UiScope;

use super::callback::{EventCallback, NodeContext, PreparedEvent};
use super::contract::{EntitySource, Event, Initialization};
use super::runtime::{RuntimeInstallError, freeze_shared_callbacks, prepare_shared_callback};

#[doc(hidden)]
pub struct Initialize;

impl Event for Initialize {
    type Route = Initialization;
    type Source = EntitySource;
    type Options = ();
}

#[doc(hidden)]
pub type PreparedAttach = PreparedEvent<Initialize, NodeContext>;

#[doc(hidden)]
pub enum AttachDefinition {
    Callback(fn(&mut World) -> Result<PreparedAttach, RuntimeInstallError>),
    Composition(fn(&mut UiScope<'_>)),
}

enum AttachState {
    Pending(fn(&mut World) -> Result<PreparedAttach, RuntimeInstallError>),
    Ready(PreparedAttach),
    Failed,
}

#[derive(Clone)]
pub(crate) struct ViewAttach(Rc<RefCell<AttachState>>);

#[derive(Clone)]
pub(crate) enum Attachment {
    Callback(ViewAttach),
    Composition(fn(&mut UiScope<'_>)),
}

impl From<AttachDefinition> for Attachment {
    fn from(definition: AttachDefinition) -> Self {
        match definition {
            AttachDefinition::Callback(factory) => Self::Callback(ViewAttach(Rc::new(
                RefCell::new(AttachState::Pending(factory)),
            ))),
            AttachDefinition::Composition(compose) => Self::Composition(compose),
        }
    }
}

impl Attachment {
    pub(crate) fn invoke(&self, world: &mut World, entity: Entity) -> bool {
        if !world.is_alive(entity) {
            return false;
        }
        match self {
            Self::Composition(compose) => {
                compose(&mut UiScope::new(world, entity));
                world.is_alive(entity)
            }
            Self::Callback(callback) => callback.invoke(world, entity),
        }
    }
}

impl ViewAttach {
    fn invoke(&self, world: &mut World, entity: Entity) -> bool {
        let factory = match &*self.0.borrow() {
            AttachState::Pending(factory) => Some(*factory),
            _ => None,
        };
        if let Some(factory) = factory {
            *self.0.borrow_mut() = AttachState::Failed;
            match factory(world) {
                Ok(prepared) => *self.0.borrow_mut() = AttachState::Ready(prepared),
                Err(error) => {
                    crate::warn!(target: "mirui::event::install", "View attachment skipped: {:?}", error);
                }
            }
        }
        let prepared = match &*self.0.borrow() {
            AttachState::Ready(prepared) => prepared.clone(),
            _ => return false,
        };
        match prepared.invoke(world, &Initialize, entity) {
            Ok(Some(())) => true,
            Ok(None) => false,
            Err(error) => {
                crate::warn!(target: "mirui::event::attach", "View attachment skipped: {:?}", error);
                false
            }
        }
    }
}

#[doc(hidden)]
pub fn prepare_attach(
    world: &mut World,
    callback: EventCallback<Initialize, NodeContext>,
) -> Result<PreparedAttach, RuntimeInstallError> {
    let prepared = prepare_shared_callback(world, callback)?;
    freeze_shared_callbacks(world);
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::CommandCapacity;
    use crate::input::event::{EventCapacity, runtime};
    use crate::test_support::tracked_allocations;

    #[crate::component]
    struct Seed(u8);

    #[crate::component]
    #[derive(Debug, PartialEq)]
    struct Ready(u8);

    #[crate::event]
    fn initialize(node: entity!(), seeds: query!(Seed), commands: commands!()) {
        commands.insert(node, Ready(seeds.get(node).unwrap().0));
    }

    #[crate::event]
    fn observe(node: entity!(), rows: query!(Ready), trace: res!(mut [u8; 2])) {
        trace[0] += 1;
        trace[1] = rows.get(node).unwrap().0;
    }

    #[crate::view(component = Seed, attach = initialize::callback())]
    fn producer() {}

    #[crate::view(component = Seed, attach = observe::callback())]
    fn consumer() {}

    #[test]
    fn matching_entities_share_prepared_hooks_and_commit_before_the_next_hook() {
        let mut world = World::new();
        runtime::configure(
            &mut world,
            EventCapacity::new().commands(CommandCapacity::new().insert::<Ready>(1)),
        )
        .unwrap();
        world.insert_resource([0_u8; 2]);
        let mut registry = crate::ui::view::ViewRegistry::default();
        registry.insert(producer::view());
        registry.insert(consumer::view());
        world.insert_resource(registry);
        let first = world.spawn(Seed(3));
        let second = world.spawn(Seed(7));
        world.insert(first, Ready(0));
        world.insert(second, Ready(0));
        super::super::widget_input::attach_handlers_for(&mut world, first);
        assert_eq!(world.resource::<[u8; 2]>(), Some(&[1, 3]));
        super::super::widget_input::attach_handlers_for(&mut world, second);
        assert_eq!(
            tracked_allocations(|| {
                super::super::widget_input::attach_handlers_for(&mut world, second);
                super::super::widget_input::attach_handlers_for(&mut world, first);
            }),
            0
        );
        assert_eq!(world.get::<Ready>(second), Some(&Ready(7)));
        assert_eq!(world.resource::<[u8; 2]>(), Some(&[2, 7]));
        assert!(!world.has::<crate::ui::HitTarget>(first));
        assert_eq!(runtime::flush(&mut world), Ok(0));
    }

    #[test]
    fn initialization_failure_does_not_freeze_configuration_or_retry_later() {
        let mut world = World::new();
        runtime::configure(&mut world, EventCapacity::new()).unwrap();
        let view = producer::view();
        let first = world.spawn(Seed(1));
        view.auto_attach().unwrap().invoke(&mut world, first);
        assert!(world.get::<Ready>(first).is_none());
        runtime::configure(&mut world, EventCapacity::default()).unwrap();
        view.auto_attach().unwrap().invoke(&mut world, first);
        assert!(world.get::<Ready>(first).is_none());
    }
}
