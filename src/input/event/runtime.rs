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
            Some(registry) => {
                registry
                    .input_bindings()
                    .try_fold(0_usize, |count, (filter, inputs)| {
                        let inputs = inputs.try_borrow().map_err(|_| ListenerError::Busy)?;
                        let super::view_input::InputState::Ready(inputs) = &*inputs else {
                            return Ok(count);
                        };
                        let matches = filter.map_or(live, |kind| world.component_count(kind));
                        inputs
                            .max_dispatch
                            .checked_mul(matches)
                            .and_then(|additional| count.checked_add(additional))
                            .ok_or(ListenerError::SizeOverflow)
                    })?
            }
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
    DuplicateRequest {
        event: &'static str,
    },
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
    crate::ui::text_input::observe(world);
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
    C: CallbackContext<E>,
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

pub(crate) fn delivery_limits(world: &World) -> EventLimits {
    runtime(world)
        .expect("initialized event runtime")
        .configuration
        .get()
        .limits()
}

pub(crate) fn input_notice<E>(
    world: &mut World,
    origin: Entity,
    event: &E,
) -> Result<(), ListenerError>
where
    E: Event<Route = super::contract::InputNotification>,
{
    let runtime = runtime(world).expect("initialized event runtime");
    runtime.prepare(world, 0)?;
    runtime
        .listeners
        .snapshot_input_notice(world, event, origin)?
        .run(world)?;
    Ok(())
}

pub(crate) fn keyboard_from<E>(
    world: &mut World,
    origin: Entity,
    event: &E,
) -> Result<super::BubbleControl, ListenerError>
where
    E: super::input::InputPayload + Event<Source = super::contract::InputSource>,
{
    keyboard(world, Some(origin), event)
}

pub(crate) fn keyboard<E>(
    world: &mut World,
    origin: Option<Entity>,
    event: &E,
) -> Result<super::BubbleControl, ListenerError>
where
    E: super::input::InputPayload + Event<Source = super::contract::InputSource>,
{
    let runtime = runtime(world).expect("initialized event runtime");
    runtime.prepare(world, 0)?;
    runtime
        .listeners
        .snapshot_keyboard_from(world, event, origin)?
        .run(world)
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
            if !self.runtime.sizes.has_pending(index) {
                continue;
            }
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
    if invoke && !changed {
        runtime.sizes.finish();
        return Ok(None);
    }
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
    if branch.is_hidden() || !crate::ui::retained::enabled(world, event.target()) {
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
            physical,
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
                        physical: *physical,
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
#[path = "../../../tests/unit/input/event/runtime/tests.rs"]
mod tests;
