//! App-owned event installation, structural preparation, and dispatch.

use alloc::rc::Rc;
use core::any::{TypeId, type_name};
use core::cell::Cell;
use core::mem::{align_of, size_of};

use crate::ecs::world::WorldId;
use crate::ecs::{CommandCapacity, CommandError, Entity, World};
use crate::ui::view::ViewRegistry;

use super::binding::EntityInstallRoute;
use super::callback::{AppContext, CallbackContext, EventCallback};
use super::capacity::{EventCapacity, EventCapacityError, EventLimits};
use super::contract::{
    AppEventSource, AppNotification, AppRoute, EntityBubble, EntityEventSource, EntityNotification,
    Event, EventKey, EventRoute,
};
use super::emission::{
    enqueue_app_notice, flush_notifications_with, has_business_storage,
    prepare_runtime_notifications,
};
use super::focus::{FocusChanged, FocusObserver};
use super::hover::{HoverChanged, HoverObservers};
use super::input::{InputEvent, Key, Rotary, Wheel};
use super::layout::{SizeChanged, SizeObservers};
use super::lifecycle::AppNotice;
use super::listeners::{
    EventListeners, InputBranch, ListenerCapacity, ListenerDecision, ListenerError,
    ListenerSelector, ListenerTarget,
};
use super::notification::{NotificationError, NotificationFlushError};
use super::press::{PressCancelReason, PressCancelled, PressObserver, Pressed, Released};
use super::scroll::{ScrollChanged, ScrollEnded, ScrollStarted, notification::ScrollObservers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Configuration {
    Open(EventLimits),
    Frozen(EventLimits),
}

impl Configuration {
    fn limits(self) -> EventLimits {
        match self {
            Self::Open(limits) | Self::Frozen(limits) => limits,
        }
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
    sizes: SizeObservers,
    focus: FocusObserver,
    hover: HoverObservers,
    press: PressObserver,
    scroll: ScrollObservers,
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
        let typed_inputs = match world.resource::<ViewRegistry>() {
            Some(registry) => registry.iter().try_fold(0_usize, |count, view| {
                let Some(inputs) = view.inputs() else {
                    return Ok(count);
                };
                let inputs = inputs.try_borrow().map_err(|_| ListenerError::Busy)?;
                let super::view_input::InputState::Ready(inputs) = &*inputs else {
                    return Ok(count);
                };
                let matches = view
                    .component_filter()
                    .map_or(live, |kind| world.component_count(kind));
                inputs
                    .max_dispatch
                    .checked_mul(matches)
                    .and_then(|additional| count.checked_add(additional))
                    .ok_or(ListenerError::SizeOverflow)
            })?,
            None => 0,
        };
        let snapshot_steps = listeners
            .checked_add(typed_inputs)
            .ok_or(ListenerError::SizeOverflow)?;
        self.listeners.reserve(ListenerCapacity {
            listeners,
            snapshot_steps,
        })
    }

    fn flush(
        &self,
        world: &mut World,
        limit: usize,
        before_dispatch: fn(&mut World),
    ) -> Result<usize, RuntimeRunError> {
        self.check_owner(world)?;
        self.focus.observe(world);
        self.hover.observe(world, &self.listeners)?;
        self.press.observe(world);
        self.scroll.observe(world, &self.listeners)?;
        match flush_notifications_with(world, &self.listeners, limit, |world| {
            before_dispatch(world);
            self.prepare(world, 0)
        }) {
            Some(result) => result.map_err(RuntimeRunError::Notification),
            None => {
                self.prepare(world, 0)?;
                Ok(0)
            }
        }
    }

    fn prepare_notifications<E: Event>(
        &self,
        world: &mut World,
        uses_commands: bool,
        app_slot: bool,
    ) -> Result<(), RuntimeInstallError> {
        let limits = self.configuration.get().limits();
        let is_business = TypeId::of::<E::Route>() == TypeId::of::<EntityNotification>();
        if is_business && let Some(capacity) = limits.notifications {
            let error = if capacity.records == 0 {
                Some(NotificationError::RecordCapacity { available: 0 })
            } else if size_of::<E>() > 0 && align_of::<E>() > capacity.payload_alignment {
                Some(NotificationError::PayloadAlignment {
                    required: align_of::<E>(),
                    available: capacity.payload_alignment,
                })
            } else if size_of::<E>() > capacity.payload_bytes {
                Some(NotificationError::PayloadCapacity {
                    required: size_of::<E>(),
                    available: capacity.payload_bytes,
                })
            } else {
                None
            };
            if let Some(error) = error {
                return Err(RuntimeInstallError::NotificationPayload {
                    event: type_name::<E>(),
                    error,
                });
            }
        }
        let business = if is_business || uses_commands || has_business_storage(world) {
            limits.notifications
        } else {
            None
        };
        prepare_runtime_notifications(world, business, app_slot)
            .map_err(EventCapacityError::Storage)
            .map_err(RuntimeInstallError::Capacity)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeInstallError {
    Capacity(EventCapacityError),
    Listener(ListenerError),
    MissingCommandCapacity,
    NotificationPayload {
        event: &'static str,
        error: NotificationError,
    },
}

impl From<ListenerError> for RuntimeInstallError {
    fn from(error: ListenerError) -> Self {
        Self::Listener(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum RuntimeRunError {
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
        sizes: SizeObservers::default(),
        focus: FocusObserver::default(),
        hover: HoverObservers::default(),
        press: PressObserver::default(),
        scroll: ScrollObservers::default(),
        configuration: Cell::new(Configuration::Open(
            EventCapacity::default()
                .resolve()
                .expect("valid UI event preset"),
        )),
    }));
    world.set_callback_observer(observe_interaction);
}

fn observe_interaction(world: &mut World) {
    if let Some(runtime) = runtime(world) {
        runtime.focus.observe(world);
        if let Err(error) = runtime.hover.observe(world, &runtime.listeners) {
            crate::warn!("hover observation skipped: {:?}", error);
        }
        runtime.press.observe(world);
        if let Err(error) = runtime.scroll.observe(world, &runtime.listeners) {
            crate::warn!("scroll observation skipped: {:?}", error);
        }
    }
}

pub(crate) fn cancel_scroll(world: &World, entity: Entity) {
    if let Some(runtime) = runtime(world) {
        runtime.scroll.cancel(entity);
    }
}

pub(crate) fn observe_press(world: &mut World) {
    if let Some(runtime) = runtime(world) {
        runtime.press.observe(world);
    }
}

pub(crate) fn cancel_press(world: &mut World, reason: PressCancelReason) {
    if let Some(runtime) = runtime(world) {
        runtime.press.cancel(world, reason);
    }
}

pub(crate) fn observe_press_input(
    world: &mut World,
    event: &InputEvent,
    can_start: bool,
    after: Option<super::gesture::recognizer::SinglePress>,
    scroll_claimed: bool,
) {
    if let Some(runtime) = runtime(world) {
        if let Err(error) = runtime.press.input(
            world,
            &runtime.listeners,
            event,
            can_start,
            after,
            scroll_claimed,
        ) {
            crate::warn!("press observation skipped: {:?}", error);
        }
    }
}

pub(crate) fn observe_pointer(world: &mut World, event: &InputEvent, width: u16, height: u16) {
    if let Some(runtime) = runtime(world) {
        runtime.hover.input(event, width, height);
        if let Err(error) = runtime.hover.observe(world, &runtime.listeners) {
            crate::warn!("hover observation skipped: {:?}", error);
        }
    }
}

pub(crate) fn cancel_hover(world: &mut World) {
    if let Some(runtime) = runtime(world) {
        runtime.hover.cancel();
        if let Err(error) = runtime.hover.observe(world, &runtime.listeners) {
            crate::warn!("hover observation skipped: {:?}", error);
        }
    }
}

fn runtime(world: &World) -> Option<Rc<EventRuntime>> {
    world.resource::<Rc<EventRuntime>>().cloned()
}

type ProviderRun<E> =
    dyn Fn(&mut World, &E, Entity) -> Result<Option<Option<Entity>>, CommandError>;
pub(crate) struct ConstructionProvider<E: Event>(Rc<ProviderRun<E>>);
impl<E: Event> crate::ecs::Component for ConstructionProvider<E> {}
impl<E: Event> Clone for ConstructionProvider<E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<E: Event> ConstructionProvider<E> {
    pub(crate) fn same_instance(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub(crate) fn invoke(&self, world: &mut World, event: &E, target: Entity) -> Option<Entity> {
        match (self.0)(world, event, target) {
            Ok(result) => result.flatten().filter(|&entity| {
                let valid = world.is_alive(target)
                    && world.is_alive(entity)
                    && world
                        .get::<crate::ui::Parent>(entity)
                        .is_some_and(|parent| parent.0 == target);
                if !valid {
                    crate::ui::despawn_subtree(world, entity);
                }
                valid
            }),
            Err(error) => {
                crate::warn!(target: "mirui::event::construction", "creation skipped: {:?}", error);
                None
            }
        }
    }
}

pub(crate) fn install_provider<E, C>(
    world: &mut World,
    entity: Entity,
    callback: EventCallback<E, C>,
) -> Result<(), RuntimeInstallError>
where
    E: Event<Route = super::contract::WidgetConstruction>,
    E::Source: EntityEventSource,
    C: CallbackContext<E, Target = Entity>,
{
    if !world.is_alive(entity) || !E::Source::present(world, entity) {
        return Err(ListenerError::MissingSource { entity }.into());
    }
    initialize(world);
    let runtime = runtime(world).expect("initialized event runtime");
    runtime.check_owner(world)?;
    let configuration = runtime.configuration.get();
    let capacity = if callback.uses_commands() {
        configuration
            .limits()
            .commands
            .ok_or(RuntimeInstallError::MissingCommandCapacity)?
    } else {
        CommandCapacity::new()
    };
    let callback = callback
        .prepare(world, capacity)
        .map_err(ListenerError::from)?;
    world.insert(
        entity,
        ConstructionProvider(Rc::new(move |world, event, target| {
            callback.invoke(world, event, target)
        })),
    );
    runtime.configuration.set(configuration.freeze());
    Ok(())
}

pub(crate) fn prepare_shared_callback<E, C>(
    world: &mut World,
    callback: EventCallback<E, C>,
) -> Result<super::callback::PreparedEvent<E, C>, RuntimeInstallError>
where
    E: Event,
    C: CallbackContext<E, Target = Entity>,
{
    initialize(world);
    let runtime = runtime(world).expect("initialized event runtime");
    runtime.check_owner(world)?;
    let configuration = runtime.configuration.get();
    let capacity = if callback.uses_commands() {
        configuration
            .limits()
            .commands
            .ok_or(RuntimeInstallError::MissingCommandCapacity)?
    } else {
        CommandCapacity::new()
    };
    let app_slot = runtime.listeners.has_app_notifications(world)?;
    runtime.prepare_notifications::<E>(world, callback.uses_commands(), app_slot)?;
    let callback = callback
        .prepare(world, capacity)
        .map_err(ListenerError::from)?;
    Ok(callback)
}

pub(crate) fn freeze_shared_callbacks(world: &World) {
    if let Some(runtime) = runtime(world) {
        runtime
            .configuration
            .set(runtime.configuration.get().freeze());
    }
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
    runtime.configuration.set(Configuration::Open(limits));
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
    runtime.prepare_notifications::<E>(world, callback.uses_commands(), app_slot)?;
    if TypeId::of::<E::Route>() == TypeId::of::<AppNotification>() {
        super::emission::prepare_app_payload::<E>(world)
            .map_err(EventCapacityError::Storage)
            .map_err(RuntimeInstallError::Capacity)?;
    }
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

pub(crate) fn install_entity<E, C>(
    world: &mut World,
    entity: Entity,
    key: EventKey<E>,
    callback: EventCallback<E, C>,
) -> Result<(), RuntimeInstallError>
where
    E: Event,
    C: CallbackContext<E, Target = Entity>,
    E::Route: EntityInstallRoute<E>,
    E::Source: EntityEventSource,
{
    E::Route::install(world, entity, key, callback)
}

pub(crate) fn install_listener<E, C>(
    world: &mut World,
    entity: Entity,
    selector: ListenerSelector<E>,
    callback: EventCallback<E, C>,
) -> Result<(), RuntimeInstallError>
where
    E: Event,
    C: CallbackContext<E, Target = Entity>,
    E::Route: super::contract::EntityRoute,
    E::Source: EntityEventSource,
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
    let app_slot = runtime.listeners.has_app_notifications(world)?;
    runtime.prepare_notifications::<E>(world, callback.uses_commands(), app_slot)?;
    let observes_size = TypeId::of::<E>() == TypeId::of::<SizeChanged>();
    if observes_size {
        runtime.sizes.reserve(&runtime.listeners, world)?;
    }
    let observes_hover = TypeId::of::<E>() == TypeId::of::<HoverChanged>();
    if observes_hover {
        runtime.hover.reserve(world, &runtime.listeners, entity)?;
    }
    let observes_press = [
        TypeId::of::<Pressed>(),
        TypeId::of::<Released>(),
        TypeId::of::<PressCancelled>(),
    ]
    .contains(&TypeId::of::<E>());
    if observes_press {
        runtime.press.reserve(world, &runtime.listeners, entity)?;
    }
    let observes_scroll = [
        TypeId::of::<ScrollStarted>(),
        TypeId::of::<ScrollChanged>(),
        TypeId::of::<ScrollEnded>(),
    ]
    .contains(&TypeId::of::<E>());
    if observes_scroll {
        runtime.scroll.reserve(world, &runtime.listeners, entity)?;
    }
    let listener = runtime
        .listeners
        .install_entity(world, entity, selector, callback, commands)?;
    if TypeId::of::<E>() == TypeId::of::<FocusChanged>() {
        runtime.focus.enable(world);
    }
    if observes_hover {
        runtime.hover.insert(world, entity);
    }
    if observes_press {
        runtime.press.insert(entity);
    }
    if observes_scroll {
        runtime.scroll.insert(world, entity);
    }
    if observes_size {
        runtime.sizes.insert(listener, entity);
        world.insert(entity, crate::ui::dirty::Dirty);
    }
    if TypeId::of::<E::Route>() == TypeId::of::<EntityBubble>() {
        world.insert(entity, crate::ui::HitTarget);
    }
    runtime.configuration.set(configuration.freeze());
    Ok(())
}

pub(crate) fn size_observer_count(world: &World) -> usize {
    runtime(world).map_or(0, |runtime| runtime.sizes.len())
}

pub(crate) struct SizeChangeBatch {
    runtime: Rc<EventRuntime>,
    length: usize,
    pub(crate) changed: bool,
}

impl SizeChangeBatch {
    pub(crate) fn dispatch(self, world: &mut World, before_dispatch: fn(&mut World)) {
        for index in 0..self.length {
            before_dispatch(world);
            let Some((listener, entity, event)) = self.runtime.sizes.take(index) else {
                continue;
            };
            if !world.is_alive(entity) {
                continue;
            }
            let result = self
                .runtime
                .listeners
                .snapshot_listener(world, &event, listener)
                .and_then(|snapshot| snapshot.run(world));
            if let Err(error) = result {
                crate::warn!(target: "mirui::layout", "size callback skipped: {:?}", error);
            }
        }
        if self.changed {
            let limit = self
                .runtime
                .configuration
                .get()
                .limits()
                .flush_limit
                .unwrap_or(0);
            before_dispatch(world);
            if let Err(error) = self.runtime.flush(world, limit, before_dispatch) {
                crate::warn!(target: "mirui::layout", "size callback notifications skipped: {:?}", error);
            }
        }
    }
}

impl Drop for SizeChangeBatch {
    fn drop(&mut self) {
        self.runtime.sizes.finish();
    }
}

pub(crate) fn capture_sizes(
    world: &World,
    laid_out: &[Entity],
    invoke: bool,
) -> Result<Option<SizeChangeBatch>, ListenerError> {
    let Some(runtime) = runtime(world).filter(|runtime| runtime.sizes.len() != 0) else {
        return Ok(None);
    };
    runtime.check_owner(world)?;
    let (length, changed) = runtime
        .sizes
        .capture(&runtime.listeners, world, laid_out, invoke)?;
    Ok(Some(SizeChangeBatch {
        runtime,
        length,
        changed,
    }))
}

pub(crate) fn has_entity_listener<E: Event>(
    world: &World,
    entity: Entity,
) -> Result<bool, ListenerError> {
    match runtime(world) {
        Some(runtime) => runtime.listeners.has_entity_listener::<E>(world, entity),
        None => Ok(false),
    }
}

/// Dispatch a recognizer gesture through the shared prepared event runtime.
pub(crate) fn dispatch_gesture(
    world: &mut World,
    event: &super::gesture::GestureEvent,
) -> Result<super::BubbleControl, ListenerError> {
    use super::gesture::{
        DragCancel, DragEnd, DragMove, DragStart, GesturePayload, LongPress, Pinch, Rotate, Tap,
    };
    let branch = InputBranch::scan(world, event.target())?;
    if branch.is_hidden() || !crate::core::reactive::retained::enabled(world, event.target()) {
        return Ok(super::BubbleControl::Allow);
    }
    let _owner_scope = crate::core::reactive::OwnerGuard::enter(world);
    initialize(world);
    let runtime = runtime(world).expect("App event runtime is initialized");
    runtime.prepare(world, 0)?;
    macro_rules! project {
        ($($kind:ident),+ $(,)?) => {
            match event {
                $(super::gesture::GestureEvent::$kind { .. } => {
                    let payload = $kind::project(event).expect("gesture variant matches payload");
                    runtime.listeners.snapshot_gesture_in_branch(world, &payload, &branch)?.run(world)
                }),+
            }
        };
    }
    project!(
        Tap, LongPress, DragStart, DragMove, DragEnd, DragCancel, Pinch, Rotate
    )
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
        InputEvent::Key {
            code,
            pressed,
            modifiers,
            repeat,
        } => {
            runtime
                .listeners
                .snapshot_keyboard(
                    world,
                    &Key {
                        code: *code,
                        pressed: *pressed,
                        modifiers: *modifiers,
                        repeat: *repeat,
                    },
                )?
                .run(world)?;
        }
        InputEvent::TextCommitted(text) => {
            runtime
                .listeners
                .snapshot_keyboard(world, text)?
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

pub(crate) fn flush(
    world: &mut World,
    before_dispatch: fn(&mut World),
) -> Result<usize, RuntimeRunError> {
    match runtime(world) {
        Some(runtime) => {
            let limit = runtime
                .configuration
                .get()
                .limits()
                .flush_limit
                .unwrap_or(0);
            runtime.flush(world, limit, before_dispatch)
        }
        None => Ok(0),
    }
}

pub(crate) fn notify_app<E: AppNotice>(
    world: &mut World,
    event: E,
    before_dispatch: fn(&mut World),
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
        return runtime.flush(world, limit, before_dispatch);
    }
    let limit = limit.checked_add(1).ok_or(RuntimeRunError::SizeOverflow)?;
    enqueue_app_notice(world, event)?;
    runtime.flush(world, limit, before_dispatch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::callback::NodeContext;
    use crate::input::event::contract::{EntityNotification, WidgetSource};
    use crate::input::event::focus::FocusState;
    use crate::input::event::focus::Focusable;
    use crate::input::event::gesture::{DragStart, GestureEvent, Tap};
    use crate::input::event::lifecycle::{Resumed, Started};
    use crate::input::event::multi_tap::{MultiTapEntry, MultiTapTracker};
    use crate::test_support::tracked_allocations;
    use crate::types::Fixed;
    use crate::ui::delivery::{dispatch_gesture, flush, notify_app};
    use crate::ui::{Hidden, Parent, UserState};

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

    #[crate::event]
    fn tap_trace(input: event!(Tap), trace: res!(mut Trace)) -> bool {
        trace.push(input.target.id);
        false
    }

    fn tap(target: Entity) -> GestureEvent {
        Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        }
        .into()
    }

    #[test]
    fn gesture_cycles_fail_before_focus_tracker_or_callbacks_and_leave_runtime_reusable() {
        let mut world = world();
        let parent = world.spawn_empty();
        let target = world.spawn_empty();
        world.insert(target, Focusable);
        world.insert(target, Parent(parent));
        world.insert_resource(FocusState {
            focused: Some(parent),
        });
        world.insert_resource(MultiTapTracker {
            last: Some(MultiTapEntry {
                entity: parent,
                time_ms: 10,
                count: 2,
            }),
        });
        install_entity(
            &mut world,
            target,
            Tap::EVENT,
            tap_trace::callback::<NodeContext>(),
        )
        .unwrap();
        world.insert(parent, Parent(target));
        let events = [
            tap(target),
            DragStart {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target,
            }
            .into(),
        ];
        for flag in 0..4 {
            match flag {
                1 => {
                    world.insert(parent, Hidden);
                }
                2 => {
                    world.insert(parent, UserState::Disabled);
                }
                3 => {
                    world.insert(
                        parent,
                        crate::ui::branch::CachedBranchVisibility { selected: false },
                    );
                }
                _ => {}
            }
            assert_eq!(
                tracked_allocations(|| {
                    for event in &events {
                        assert!(matches!(
                            dispatch_gesture(&mut world, event, 100),
                            Err(ListenerError::CyclicRoute { .. })
                        ));
                        assert_eq!(
                            world.resource::<FocusState>().unwrap().focused,
                            Some(parent)
                        );
                        let previous = world.resource::<MultiTapTracker>().unwrap().last.unwrap();
                        assert_eq!(
                            (previous.entity, previous.time_ms, previous.count),
                            (parent, 10, 2)
                        );
                        assert_eq!(world.resource::<Trace>().unwrap().len, 0);
                    }
                }),
                0
            );
            world.remove::<Hidden>(parent);
            world.remove::<UserState>(parent);
            world.remove::<crate::ui::branch::CachedBranchVisibility>(parent);
        }
        world.remove::<Parent>(parent);
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(
                    dispatch_gesture(&mut world, &tap(target), 200),
                    Ok(super::super::BubbleControl::Allow)
                );
            }),
            0
        );
        assert_eq!(
            world.resource::<FocusState>().unwrap().focused,
            Some(target)
        );
        let trace = world.resource::<Trace>().unwrap();
        assert_eq!(&trace.values[..trace.len], &[target.id]);
    }

    #[test]
    fn gesture_focus_and_tracker_preserve_disabled_and_nearest_ancestor_semantics() {
        let mut world = world();
        let parent = world.spawn_empty();
        world.insert(parent, Focusable);
        let target = world.spawn_empty();
        world.insert(target, Parent(parent));
        world.insert_resource(FocusState {
            focused: Some(parent),
        });
        world.insert_resource(MultiTapTracker::default());
        install_entity(
            &mut world,
            target,
            Tap::EVENT,
            tap_trace::callback::<NodeContext>(),
        )
        .unwrap();
        world.insert(parent, UserState::Disabled);
        assert_eq!(
            dispatch_gesture(&mut world, &tap(target), 100),
            Ok(super::super::BubbleControl::Allow)
        );
        assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
        assert_eq!(world.resource::<Trace>().unwrap().len, 0);
        let entry = world.resource::<MultiTapTracker>().unwrap().last.unwrap();
        assert_eq!((entry.entity, entry.time_ms, entry.count), (target, 100, 1));
        world.remove::<UserState>(parent);
        assert_eq!(
            dispatch_gesture(&mut world, &tap(target), 200),
            Ok(super::super::BubbleControl::Allow)
        );
        assert_eq!(
            world.resource::<FocusState>().unwrap().focused,
            Some(parent)
        );
        world.insert(target, Focusable);
        assert_eq!(
            dispatch_gesture(&mut world, &tap(target), 300),
            Ok(super::super::BubbleControl::Allow)
        );
        assert_eq!(
            world.resource::<FocusState>().unwrap().focused,
            Some(target)
        );
        world.insert(target, UserState::Disabled);
        assert_eq!(
            dispatch_gesture(&mut world, &tap(target), 400),
            Ok(super::super::BubbleControl::Allow)
        );
        assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
        assert_eq!(world.resource::<Trace>().unwrap().len, 2);
        let entry = world.resource::<MultiTapTracker>().unwrap().last.unwrap();
        assert_eq!((entry.entity, entry.time_ms, entry.count), (target, 400, 4));
        world.remove::<UserState>(target);
        world.remove::<Focusable>(target);
        world.remove::<Focusable>(parent);
        world.resource_mut::<FocusState>().unwrap().focused = Some(parent);
        let drag: GestureEvent = DragStart {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        }
        .into();
        dispatch_gesture(&mut world, &drag, 500).unwrap();
        assert_eq!(
            world.resource::<FocusState>().unwrap().focused,
            Some(parent)
        );
        assert_eq!(
            world
                .resource::<MultiTapTracker>()
                .unwrap()
                .last
                .unwrap()
                .time_ms,
            400
        );
        dispatch_gesture(&mut world, &tap(target), 600).unwrap();
        assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
    }

    #[test]
    fn gesture_stale_generations_stop_at_the_live_prefix_without_following_reused_nodes() {
        let mut world = world();
        let parent = world.spawn_empty();
        world.insert(parent, Focusable);
        let target = world.spawn_empty();
        world.insert(target, Parent(parent));
        world.insert_resource(FocusState {
            focused: Some(parent),
        });
        world.insert_resource(MultiTapTracker::default());
        install_entity(
            &mut world,
            target,
            Tap::EVENT,
            tap_trace::callback::<NodeContext>(),
        )
        .unwrap();
        world.despawn(parent);
        let replacement = world.spawn_empty();
        world.insert(replacement, Focusable);
        assert_eq!(replacement.id, parent.id);
        assert_ne!(replacement.generation, parent.generation);
        world.insert(replacement, UserState::Disabled);
        assert_eq!(
            tracked_allocations(|| {
                for now in [100, 200] {
                    assert_eq!(
                        dispatch_gesture(&mut world, &tap(target), now),
                        Ok(super::super::BubbleControl::Allow)
                    );
                }
            }),
            0
        );
        assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
        let trace = world.resource::<Trace>().unwrap();
        assert_eq!(&trace.values[..trace.len], &[target.id, target.id]);
        world.despawn(target);
        let replacement = world.spawn_empty();
        world.insert(replacement, Focusable);
        assert_eq!(replacement.id, target.id);
        world.resource_mut::<FocusState>().unwrap().focused = Some(replacement);
        dispatch_gesture(&mut world, &tap(target), 300).unwrap();
        assert_eq!(world.resource::<FocusState>().unwrap().focused, None);
        assert_eq!(world.resource::<Trace>().unwrap().len, 2);
        let entry = world.resource::<MultiTapTracker>().unwrap().last.unwrap();
        assert_eq!((entry.entity, entry.time_ms, entry.count), (target, 300, 3));
    }

    #[test]
    fn first_successful_binding_freezes_configuration_but_failed_binding_does_not() {
        let mut world = world();
        configure(&mut world, EventCapacity::new()).unwrap();
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
                    pressed: true,
                    modifiers: Some(Default::default()),
                    repeat: Some(false),
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
                configure(&mut world, EventCapacity::default()).unwrap();
                let runtime = runtime(&world).unwrap();
                assert!(
                    super::super::emission::flush_notifications(&mut world, &runtime.listeners, 0,)
                        .is_none()
                );
                for code in [1, 2] {
                    assert_eq!(
                        dispatch_input(
                            &mut world,
                            None,
                            &InputEvent::Key {
                                code,
                                pressed: true,
                                modifiers: Some(Default::default()),
                                repeat: Some(false),
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
                            kind: crate::input::event::input::PointerKind::Mouse,
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

    #[crate::event]
    fn editing(node: entity!(), _key: event!(Key), trace: res!(mut Trace)) -> bool {
        trace.push(40 + node.id);
        false
    }

    #[test]
    fn structural_flush_prepares_without_notification_service_and_preserves_route_order() {
        let mut world = world();
        install_app(&mut world, Key::EVENT, key::callback()).unwrap();
        let parent = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(parent, Marker(0));
        world.insert(child, Marker(0));
        let mut registry = ViewRegistry::default();
        registry.insert(
            crate::ui::view::View::systems_only("editor", &[])
                .with_filter::<Marker>()
                .with_inputs(&mut world, editing::callback()),
        );
        world.insert_resource(registry);
        world.insert(child, crate::ui::Parent(parent));
        crate::ui::lifecycle::attach_handlers_for(&mut world, parent);
        crate::ui::lifecycle::attach_handlers_for(&mut world, child);
        world.insert_resource(FocusState {
            focused: Some(child),
        });
        let input = InputEvent::Key {
            code: 7,
            pressed: true,
            modifiers: Some(Default::default()),
            repeat: Some(false),
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
            kind: crate::surface::PointerKind::Mouse,
            id: 0,
            present: true,
            hover_capable: true,
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
    fn default_storage_is_lazy_and_later_binding_preserves_business_capacity() {
        let mut world = world();
        let target = world.spawn(Pad);
        install_app(&mut world, Key::EVENT, key::callback()).unwrap();
        assert!(!has_business_storage(&world));
        install_app(&mut world, Started::EVENT, started::callback()).unwrap();
        assert!(!has_business_storage(&world));
        install_entity(
            &mut world,
            target,
            Hit::EVENT,
            hit::callback::<NodeContext>(),
        )
        .unwrap();
        assert!(has_business_storage(&world));
        install_entity(
            &mut world,
            target,
            Tap::EVENT,
            tap_trace::callback::<NodeContext>(),
        )
        .unwrap();
        install_app(&mut world, Resumed::EVENT, started::callback()).unwrap();
        assert!(has_business_storage(&world));
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..2 {
                    world.resource_mut::<Trace>().unwrap().len = 0;
                    for n in 0..8 {
                        crate::input::event::emission::submit_if_listened(
                            &mut world,
                            target,
                            Hit(n),
                        )
                        .unwrap();
                    }
                    assert!(matches!(
                        crate::input::event::emission::submit_if_listened(
                            &mut world,
                            target,
                            Hit(9)
                        ),
                        Err(CommandError::Commit(
                            crate::ecs::CommandCommitError::RecordCapacity { available: 8 }
                        ))
                    ));
                    assert_eq!(flush(&mut world), Ok(8));
                    let trace = world.resource::<Trace>().unwrap();
                    assert_eq!(&trace.values[..trace.len], &[0, 1, 2, 3, 4, 5, 6, 7]);
                }
            }),
            0
        );
    }

    #[test]
    fn default_commands_install_without_configuration_and_reuse_bounded_storage() {
        let mut world = world();
        let target = world.spawn(Marker(0));
        install_app(&mut world, Key::EVENT, deferred::callback(target)).unwrap();
        assert!(has_business_storage(&world));
        assert_eq!(
            tracked_allocations(|| {
                for code in [3, 7] {
                    assert_eq!(
                        dispatch_input(
                            &mut world,
                            None,
                            &InputEvent::Key {
                                code,
                                pressed: true,
                                modifiers: Some(Default::default()),
                                repeat: Some(false),
                            },
                            0,
                            0
                        ),
                        Ok(true)
                    );
                    assert_eq!(world.get::<Marker>(target), Some(&Marker(code)));
                    assert_eq!(flush(&mut world), Ok(0));
                }
            }),
            0
        );
        let capacity = runtime(&world)
            .unwrap()
            .configuration
            .get()
            .limits()
            .commands
            .unwrap();
        let mut queue = crate::ecs::commands::CommandQueue::bounded(capacity).unwrap();
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..2 {
                    let mut batch = queue.begin();
                    for n in 0..5 {
                        batch.commands().insert(target, Marker(n));
                    }
                    assert_eq!(
                        batch.finish(&mut world),
                        Err(CommandError::RecordCapacity { available: 4 })
                    );
                    assert_eq!(world.get::<Marker>(target), Some(&Marker(7)));
                }
            }),
            0
        );
    }

    #[test]
    fn explicit_small_notification_capacity_is_not_replaced_by_preset() {
        let mut world = world();
        let target = world.spawn(Pad);
        configure(
            &mut world,
            EventCapacity::new().notifications::<Hit>(1).flush_limit(1),
        )
        .unwrap();
        install_entity(
            &mut world,
            target,
            Hit::EVENT,
            hit::callback::<NodeContext>(),
        )
        .unwrap();
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..2 {
                    crate::input::event::emission::submit_if_listened(&mut world, target, Hit(1))
                        .unwrap();
                    assert!(matches!(
                        crate::input::event::emission::submit_if_listened(
                            &mut world,
                            target,
                            Hit(2)
                        ),
                        Err(CommandError::Commit(
                            crate::ecs::CommandCommitError::RecordCapacity { available: 1 }
                        ))
                    ));
                    assert_eq!(flush(&mut world), Ok(1));
                }
            }),
            0
        );
    }

    #[crate::event]
    fn feedback(input: event!(Hit), node: entity!(), commands: commands!()) {
        commands.emit(node, Hit(input.0 + 1));
    }

    #[test]
    fn default_feedback_limit_discards_pending_writes_and_storage_remains_reusable() {
        let mut world = world();
        let target = world.spawn(Pad);
        install_entity(
            &mut world,
            target,
            Hit::EVENT,
            feedback::callback::<NodeContext>(),
        )
        .unwrap();
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..2 {
                    crate::input::event::emission::submit_if_listened(&mut world, target, Hit(0))
                        .unwrap();
                    assert_eq!(
                        flush(&mut world),
                        Err(RuntimeRunError::Notification(
                            NotificationFlushError::Storage(NotificationError::FlushLimit {
                                limit: 16
                            })
                        ))
                    );
                    assert_eq!(flush(&mut world), Ok(0));
                }
            }),
            0
        );
    }

    struct Oversized([u8; 1024]);

    #[repr(align(64))]
    struct Aligned(u8);

    macro_rules! custom_events {
        ($($event:ty),+ $(,)?) => {
            $(impl Event for $event {
                type Route = EntityNotification;
                type Source = WidgetSource<Pad>;
                type Options = ();
            })+
        };
    }
    custom_events!(Oversized, Aligned);

    #[test]
    fn oversized_payloads_fail_before_freezing_and_explicit_layout_allows_installation() {
        let mut world = world();
        let target = world.spawn(Pad);
        let capacity = EventCapacity::default()
            .resolve()
            .unwrap()
            .notifications
            .unwrap();
        assert_eq!(
            install_entity(
                &mut world,
                target,
                Oversized::EVENT,
                Oversized::EVENT.__bind_callback::<NodeContext, _>("large", |value: &Oversized| {
                    assert_eq!(value.0[0], 5);
                })
            ),
            Err(RuntimeInstallError::NotificationPayload {
                event: type_name::<Oversized>(),
                error: NotificationError::PayloadCapacity {
                    required: core::mem::size_of::<Oversized>(),
                    available: capacity.payload_bytes,
                },
            })
        );
        assert_eq!(
            install_entity(
                &mut world,
                target,
                Aligned::EVENT,
                Aligned::EVENT.__bind_callback::<NodeContext, _>("aligned", |value: &Aligned| {
                    assert_eq!(value.0, 7);
                })
            ),
            Err(RuntimeInstallError::NotificationPayload {
                event: type_name::<Aligned>(),
                error: NotificationError::PayloadAlignment {
                    required: 64,
                    available: capacity.payload_alignment,
                },
            })
        );
        assert!(!has_business_storage(&world));
        configure(
            &mut world,
            EventCapacity::new()
                .notifications::<Oversized>(1)
                .notifications::<Aligned>(1)
                .flush_limit(2),
        )
        .unwrap();
        install_entity(
            &mut world,
            target,
            Oversized::EVENT,
            Oversized::EVENT.__bind_callback::<NodeContext, _>("large", |value: &Oversized| {
                assert_eq!(value.0[0], 5);
            }),
        )
        .unwrap();
        install_entity(
            &mut world,
            target,
            Aligned::EVENT,
            Aligned::EVENT.__bind_callback::<NodeContext, _>("aligned", |value: &Aligned| {
                assert_eq!(value.0, 7);
            }),
        )
        .unwrap();
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..2 {
                    crate::input::event::emission::submit_if_listened(
                        &mut world,
                        target,
                        Oversized([5; 1024]),
                    )
                    .unwrap();
                    crate::input::event::emission::submit_if_listened(
                        &mut world,
                        target,
                        Aligned(7),
                    )
                    .unwrap();
                    assert_eq!(flush(&mut world), Ok(2));
                }
            }),
            0
        );
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
