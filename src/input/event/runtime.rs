//! App-owned event installation, structural preparation, and dispatch.

use alloc::rc::Rc;
use core::any::TypeId;
use core::cell::Cell;

use crate::ecs::world::WorldId;
use crate::ecs::{CommandCapacity, CommandError, Entity, World};
use crate::ui::view::ViewRegistry;

use super::callback::{AppContext, EventCallback};
use super::capacity::{EventCapacity, EventCapacityError, EventLimits};
use super::contract::{AppEventSource, AppNotification, AppRoute, Event, EventKey, EventRoute};
use super::emission::{
    enqueue_app_notice, flush_notifications_with, prepare_runtime_notifications,
};
use super::focus::KeyHandler;
use super::input::{CharInput, InputEvent, Key, Rotary, Wheel};
use super::lifecycle::AppNotice;
use super::listeners::{
    EventListeners, ListenerCapacity, ListenerDecision, ListenerError, ListenerSelector,
    ListenerTarget,
};
use super::notification::NotificationFlushError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Configuration {
    Open(Option<EventLimits>),
    Frozen(Option<EventLimits>),
}

impl Configuration {
    fn limits(self) -> EventLimits {
        let limits = match self {
            Self::Open(limits) | Self::Frozen(limits) => limits,
        };
        limits.unwrap_or(EventLimits {
            notifications: None,
            commands: None,
            flush_limit: None,
        })
    }

    fn freeze(self) -> Self {
        match self {
            Self::Open(limits) | Self::Frozen(limits) => Self::Frozen(limits),
        }
    }
}

struct EventRuntime {
    owner: WorldId,
    listeners: EventListeners,
    configuration: Cell<Configuration>,
}

impl EventRuntime {
    fn check_owner(&self, world: &World) -> Result<(), ListenerError> {
        if self.owner != world.id() {
            return Err(ListenerError::ForeignOwner);
        }
        Ok(())
    }

    fn prepare(&self, world: &World, additional: usize) -> Result<(), ListenerError> {
        self.check_owner(world)?;
        let listeners = self
            .listeners
            .listener_count(world)?
            .checked_add(additional)
            .ok_or(ListenerError::SizeOverflow)?;
        let live = world.live_entity_count();
        let gestures = match world.resource::<ViewRegistry>() {
            Some(registry) => registry.iter().try_fold(0_usize, |count, view| {
                if view.internal_gesture().is_none() {
                    return Ok(count);
                }
                let matches = view
                    .component_filter()
                    .map_or(live, |kind| world.component_count(kind));
                count
                    .checked_add(matches)
                    .ok_or(ListenerError::SizeOverflow)
            })?,
            None => 0,
        };
        let keyboard = world.component_count(TypeId::of::<KeyHandler>());
        let snapshot_steps = listeners
            .checked_add(gestures.max(keyboard))
            .ok_or(ListenerError::SizeOverflow)?;
        self.listeners.reserve(ListenerCapacity {
            listeners,
            snapshot_steps,
        })
    }

    fn flush(&self, world: &mut World, limit: usize) -> Result<usize, RuntimeRunError> {
        self.check_owner(world)?;
        match flush_notifications_with(world, &self.listeners, limit, |world| {
            self.prepare(world, 0)
        }) {
            Some(result) => result.map_err(RuntimeRunError::Notification),
            None => {
                self.prepare(world, 0)?;
                Ok(0)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeInstallError {
    Capacity(EventCapacityError),
    Listener(ListenerError),
    MissingCommandCapacity,
}

impl From<ListenerError> for RuntimeInstallError {
    fn from(error: ListenerError) -> Self {
        Self::Listener(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeRunError {
    Listener(ListenerError),
    Command(CommandError),
    Notification(NotificationFlushError<ListenerError>),
    SizeOverflow,
}

impl From<ListenerError> for RuntimeRunError {
    fn from(error: ListenerError) -> Self {
        Self::Listener(error)
    }
}

impl From<CommandError> for RuntimeRunError {
    fn from(error: CommandError) -> Self {
        Self::Command(error)
    }
}

pub(crate) fn initialize(world: &mut World) {
    if world.resource::<Rc<EventRuntime>>().is_some() {
        return;
    }
    let listeners = EventListeners::new(world);
    world.insert_resource(Rc::new(EventRuntime {
        owner: world.id(),
        listeners,
        configuration: Cell::new(Configuration::Open(None)),
    }));
}

fn runtime(world: &World) -> Option<Rc<EventRuntime>> {
    world.resource::<Rc<EventRuntime>>().cloned()
}

pub(crate) fn configure(
    world: &mut World,
    capacity: EventCapacity,
) -> Result<(), EventCapacityError> {
    initialize(world);
    let runtime = runtime(world).expect("App event runtime is initialized");
    if runtime.owner != world.id() {
        return Err(EventCapacityError::ForeignOwner);
    }
    let limits = capacity.resolve()?;
    let configuration = runtime.configuration.get();
    if let Configuration::Frozen(_) = configuration {
        return if configuration.limits() == limits {
            Ok(())
        } else {
            Err(EventCapacityError::FrozenConfiguration)
        };
    }
    let app_slot = runtime
        .listeners
        .has_app_notifications(world)
        .map_err(|error| match error {
            ListenerError::ForeignOwner => EventCapacityError::ForeignOwner,
            _ => EventCapacityError::Busy,
        })?;
    prepare_runtime_notifications(world, limits.notifications, app_slot)
        .map_err(EventCapacityError::Storage)?;
    runtime.configuration.set(Configuration::Open(Some(limits)));
    Ok(())
}

pub(crate) fn install_app<E>(
    world: &mut World,
    key: EventKey<E>,
    callback: EventCallback<E, AppContext>,
) -> Result<(), RuntimeInstallError>
where
    E: Event<Options = ()>,
    E::Route: AppRoute,
    E::Source: AppEventSource,
    <E::Route as EventRoute>::Decision: ListenerDecision,
{
    initialize(world);
    let runtime = runtime(world).expect("App event runtime is initialized");
    runtime.check_owner(world)?;
    let configuration = runtime.configuration.get();
    let limits = configuration.limits();
    let commands = if callback.uses_commands() {
        limits
            .commands
            .ok_or(RuntimeInstallError::MissingCommandCapacity)?
    } else {
        CommandCapacity::new()
    };
    runtime.prepare(world, 1)?;
    let app_slot = runtime.listeners.has_app_notifications(world)?
        || TypeId::of::<E::Route>() == TypeId::of::<AppNotification>();
    prepare_runtime_notifications(world, limits.notifications, app_slot)
        .map_err(EventCapacityError::Storage)
        .map_err(RuntimeInstallError::Capacity)?;
    runtime.listeners.install_app(
        world,
        ListenerSelector::new(key, |(), _| true),
        callback,
        commands,
    )?;
    runtime.configuration.set(configuration.freeze());
    Ok(())
}

pub(crate) fn prepare(world: &World) -> Result<(), ListenerError> {
    match runtime(world) {
        Some(runtime) => runtime.prepare(world, 0),
        None => Ok(()),
    }
}

pub(crate) fn dispatch_input(
    world: &mut World,
    root: Option<Entity>,
    event: &InputEvent,
    width: u16,
    height: u16,
) -> Result<bool, ListenerError> {
    let Some(runtime) = runtime(world) else {
        return Ok(false);
    };
    runtime.check_owner(world)?;
    match event {
        InputEvent::Key { code, pressed } => {
            runtime
                .listeners
                .snapshot_keyboard(
                    world,
                    &Key {
                        code: *code,
                        pressed: *pressed,
                    },
                )?
                .run(world)?;
        }
        InputEvent::CharInput { ch } => {
            runtime
                .listeners
                .snapshot_keyboard(world, &CharInput { ch: *ch })?
                .run(world)?;
        }
        InputEvent::Wheel { dx, dy, x, y } => {
            let input = Wheel {
                dx: *dx,
                dy: *dy,
                x: *x,
                y: *y,
            };
            if let Some(root) = root {
                runtime
                    .listeners
                    .dispatch_scroll(world, root, &input, width, height)?;
            } else {
                runtime
                    .listeners
                    .snapshot(world, &input, ListenerTarget::App)?
                    .run(world)?;
            }
        }
        InputEvent::Rotary { id, delta } => {
            let input = Rotary {
                id: *id,
                delta: *delta,
            };
            if let Some(root) = root {
                runtime
                    .listeners
                    .dispatch_scroll(world, root, &input, width, height)?;
            } else {
                runtime
                    .listeners
                    .snapshot(world, &input, ListenerTarget::App)?
                    .run(world)?;
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

pub(crate) fn flush(world: &mut World) -> Result<usize, RuntimeRunError> {
    match runtime(world) {
        Some(runtime) => {
            let limit = runtime
                .configuration
                .get()
                .limits()
                .flush_limit
                .unwrap_or(0);
            runtime.flush(world, limit)
        }
        None => Ok(0),
    }
}

pub(crate) fn notify_app<E: AppNotice>(
    world: &mut World,
    event: E,
) -> Result<usize, RuntimeRunError> {
    let Some(runtime) = runtime(world) else {
        return Ok(0);
    };
    runtime.check_owner(world)?;
    let limit = runtime
        .configuration
        .get()
        .limits()
        .flush_limit
        .unwrap_or(0);
    if !runtime.listeners.has_app_listener::<E>(world)? {
        return runtime.flush(world, limit);
    }
    let limit = limit.checked_add(1).ok_or(RuntimeRunError::SizeOverflow)?;
    enqueue_app_notice(world, event)?;
    runtime.flush(world, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::callback::NodeContext;
    use crate::input::event::contract::{EntityNotification, WidgetSource};
    use crate::input::event::focus::FocusState;
    use crate::input::event::lifecycle::{Resumed, Started};
    use crate::test_support::tracked_allocations;
    use crate::types::Fixed;

    #[derive(Default)]
    struct Trace {
        values: [u32; 8],
        len: usize,
    }

    impl Trace {
        fn push(&mut self, value: u32) {
            self.values[self.len] = value;
            self.len += 1;
        }
    }

    #[crate::component]
    #[derive(Debug, PartialEq, Eq)]
    struct Marker(u32);

    #[crate::event]
    fn key(input: event!(Key), trace: res!(mut Trace)) -> bool {
        trace.push(input.code);
        true
    }

    #[crate::event]
    fn deferred(input: event!(Key), target: Entity, commands: commands!()) -> bool {
        commands.insert(target, Marker(input.code));
        true
    }

    #[crate::event]
    fn started(trace: res!(mut Trace)) {
        trace.push(99);
    }

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(Trace::default());
        initialize(&mut world);
        world
    }

    #[test]
    fn first_successful_binding_freezes_configuration_but_failed_binding_does_not() {
        let mut world = world();
        let target = world.spawn(Marker(0));
        assert_eq!(
            install_app(&mut world, Key::EVENT, deferred::callback(target)),
            Err(RuntimeInstallError::MissingCommandCapacity)
        );
        let capacity = EventCapacity::new().commands(CommandCapacity::new().insert::<Marker>(1));
        configure(&mut world, capacity).unwrap();
        install_app(&mut world, Key::EVENT, deferred::callback(target)).unwrap();
        assert_eq!(
            tracked_allocations(|| {
                configure(&mut world, capacity).unwrap();
                assert_eq!(
                    configure(
                        &mut world,
                        EventCapacity::new().commands(CommandCapacity::new().insert::<Marker>(2))
                    ),
                    Err(EventCapacityError::FrozenConfiguration)
                );
            }),
            0
        );
        assert_eq!(
            dispatch_input(
                &mut world,
                None,
                &InputEvent::Key {
                    code: 7,
                    pressed: true
                },
                32,
                32,
            ),
            Ok(true)
        );
        assert_eq!(world.get::<Marker>(target), Some(&Marker(7)));
    }

    #[test]
    fn plain_app_input_needs_neither_root_nor_business_capacity() {
        let mut world = world();
        install_app(&mut world, Key::EVENT, key::callback()).unwrap();
        prepare(&world).unwrap();
        assert_eq!(
            tracked_allocations(|| {
                configure(&mut world, EventCapacity::new()).unwrap();
                for code in [1, 2] {
                    assert_eq!(
                        dispatch_input(
                            &mut world,
                            None,
                            &InputEvent::Key {
                                code,
                                pressed: true
                            },
                            0,
                            0,
                        ),
                        Ok(true)
                    );
                }
                assert_eq!(
                    dispatch_input(
                        &mut world,
                        None,
                        &InputEvent::PointerDown {
                            id: 0,
                            x: Fixed::ZERO,
                            y: Fixed::ZERO,
                        },
                        0,
                        0,
                    ),
                    Ok(false)
                );
            }),
            0
        );
        let trace = world.resource::<Trace>().unwrap();
        assert_eq!(&trace.values[..trace.len], &[1, 2]);
    }

    #[test]
    fn structural_unit_slot_is_prepared_during_installation_and_reused_without_growth() {
        let mut world = world();
        install_app(&mut world, Started::EVENT, started::callback()).unwrap();
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..2 {
                    assert_eq!(notify_app(&mut world, Started), Ok(1));
                }
            }),
            0
        );
        let trace = world.resource::<Trace>().unwrap();
        assert_eq!(&trace.values[..trace.len], &[99, 99]);
    }

    #[test]
    fn unmatched_app_notices_do_not_allocate_or_occupy_the_structural_slot() {
        let mut world = world();
        install_app(&mut world, Key::EVENT, key::callback()).unwrap();
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(notify_app(&mut world, Started), Ok(0));
            }),
            0
        );
        install_app(&mut world, Started::EVENT, started::callback()).unwrap();
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(notify_app(&mut world, Resumed), Ok(0));
                assert_eq!(notify_app(&mut world, Started), Ok(1));
            }),
            0
        );
        let trace = world.resource::<Trace>().unwrap();
        assert_eq!(&trace.values[..trace.len], &[99]);
    }

    fn editing(world: &mut World, node: Entity, _: &InputEvent) -> bool {
        world.resource_mut::<Trace>().unwrap().push(40 + node.id);
        false
    }

    #[test]
    fn structural_flush_prepares_without_notification_service_and_preserves_route_order() {
        let mut world = world();
        install_app(&mut world, Key::EVENT, key::callback()).unwrap();
        let parent = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(parent, KeyHandler { on_key: editing });
        world.insert(child, KeyHandler { on_key: editing });
        world.insert(child, crate::ui::Parent(parent));
        world.insert_resource(FocusState {
            focused: Some(child),
        });
        let input = InputEvent::Key {
            code: 7,
            pressed: true,
        };
        assert_eq!(
            dispatch_input(&mut world, None, &input, 0, 0),
            Err(ListenerError::SnapshotCapacity { available: 1 })
        );
        assert_eq!(world.resource::<Trace>().unwrap().len, 0);
        assert_eq!(flush(&mut world), Ok(0));
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(dispatch_input(&mut world, None, &input, 0, 0), Ok(true));
            }),
            0
        );
        let trace = world.resource::<Trace>().unwrap();
        assert_eq!(
            &trace.values[..trace.len],
            &[40 + child.id, 40 + parent.id, 7]
        );
    }

    #[test]
    fn mandatory_suspend_cancellation_precedes_busy_route_preparation() {
        let mut world = world();
        install_app(&mut world, Started::EVENT, started::callback()).unwrap();
        world.insert_resource(super::super::PointerCursor {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            down: true,
            event_seq: 5,
        });
        let runtime = runtime(&world).unwrap();
        let snapshot = runtime
            .listeners
            .snapshot(&world, &Started, ListenerTarget::App)
            .unwrap();
        assert_eq!(prepare(&world), Err(ListenerError::Busy));
        super::super::dispatch_input(&mut world, None, &InputEvent::AppSuspend, 0, 0, 0);
        let cursor = world.resource::<super::super::PointerCursor>().unwrap();
        assert!(!cursor.down);
        assert_eq!(cursor.event_seq, 6);
        drop(snapshot);
        assert_eq!(prepare(&world), Ok(()));
    }

    #[crate::component]
    struct Pad;

    struct Hit(u32);

    impl Event for Hit {
        type Route = EntityNotification;
        type Source = WidgetSource<Pad>;
        type Options = ();
    }

    #[crate::event]
    fn hit(input: event!(Hit), trace: res!(mut Trace)) {
        trace.push(input.0);
    }

    #[test]
    fn app_notice_joins_pending_business_notifications_in_the_same_fifo() {
        let mut world = world();
        let target = world.spawn(Pad);
        configure(
            &mut world,
            EventCapacity::new().notifications::<Hit>(1).flush_limit(1),
        )
        .unwrap();
        install_app(&mut world, Started::EVENT, started::callback()).unwrap();
        let runtime = runtime(&world).unwrap();
        runtime.prepare(&world, 1).unwrap();
        runtime
            .listeners
            .install_entity(
                &mut world,
                target,
                ListenerSelector::new(Hit::EVENT, |(), _| true),
                hit::callback::<NodeContext>(),
                CommandCapacity::new(),
            )
            .unwrap();
        let mut commands =
            crate::ecs::commands::CommandQueue::bounded(CommandCapacity::new().emit::<Hit>(1))
                .unwrap();
        let mut batch = commands.begin();
        batch.commands().emit(target, Hit(7));
        batch.finish(&mut world).unwrap();
        assert_eq!(notify_app(&mut world, Started), Ok(2));
        let trace = world.resource::<Trace>().unwrap();
        assert_eq!(&trace.values[..trace.len], &[7, 99]);
    }

    #[test]
    fn invalid_configuration_and_foreign_owner_do_not_change_live_configuration() {
        let mut world = world();
        let capacity = EventCapacity::new().commands(CommandCapacity::new().insert::<Marker>(1));
        configure(&mut world, capacity).unwrap();
        let runtime = runtime(&world).unwrap();
        let previous = runtime.configuration.get();
        assert_eq!(
            configure(&mut world, EventCapacity::new().notifications::<Hit>(1)),
            Err(EventCapacityError::MissingFlushLimit)
        );
        assert_eq!(runtime.configuration.get(), previous);
        let mut other = World::new();
        other.insert_resource(runtime.clone());
        assert_eq!(
            configure(&mut other, capacity),
            Err(EventCapacityError::ForeignOwner)
        );
        assert_eq!(prepare(&other), Err(ListenerError::ForeignOwner));
        assert_eq!(runtime.configuration.get(), previous);
    }
}
