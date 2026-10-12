use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use crate::ecs::World;

use super::callback::{EventCallback, NodeContext};
use super::contract::{Event, EventRoute};
use super::listeners::{ErasedListener, InstalledListener, ListenerDecision, ListenerSelector};
use super::runtime::{RuntimeInstallError, prepare_shared_callback};

pub(crate) struct InputHook {
    pub(crate) callback: Rc<dyn ErasedListener>,
    pub(crate) gesture: bool,
}

#[doc(hidden)]
pub struct PreparedInputs {
    pub(crate) hooks: Vec<InputHook>,
    pub(crate) requests: Vec<super::request::RequestHook>,
    pub(crate) max_dispatch: usize,
}

#[doc(hidden)]
pub type InputFactory = fn(&mut World) -> Result<PreparedInputs, RuntimeInstallError>;

pub(crate) enum InputState {
    Pending(InputFactory),
    Ready(PreparedInputs),
    Failed,
}

pub(crate) type SharedInputs = Rc<RefCell<InputState>>;

pub(crate) fn prepare(inputs: &SharedInputs, world: &mut World) {
    let factory = match *inputs.borrow() {
        InputState::Pending(factory) => factory,
        _ => return,
    };
    *inputs.borrow_mut() = InputState::Failed;
    match factory(world) {
        Ok(prepared) => *inputs.borrow_mut() = InputState::Ready(prepared),
        Err(error) => {
            crate::warn!(target: "mirui::event::install", "View input skipped: {:?}", error);
        }
    }
}

mod sealed {
    pub trait Input {}
}

#[doc(hidden)]
pub trait ViewInput: Event + sealed::Input {
    const GESTURE: bool;
    fn selector() -> ListenerSelector<Self>;
}

macro_rules! gesture_inputs {
    ($($event:ty),+ $(,)?) => {$(
        impl sealed::Input for $event {}
        impl ViewInput for $event {
            const GESTURE: bool = true;
            fn selector() -> ListenerSelector<Self> {
                ListenerSelector::gesture(Self::EVENT)
            }
        }
    )+};
}

gesture_inputs!(
    super::gesture::Tap,
    super::gesture::LongPress,
    super::gesture::DragStart,
    super::gesture::DragMove,
    super::gesture::DragEnd,
    super::gesture::DragCancel,
    super::gesture::Pinch,
    super::gesture::Rotate,
);

macro_rules! direct_inputs {
    ($($event:ty),+ $(,)?) => {$(
        impl sealed::Input for $event {}
        impl ViewInput for $event {
            const GESTURE: bool = false;
            fn selector() -> ListenerSelector<Self> {
                ListenerSelector::new(Self::EVENT, |(), _| true)
            }
        }
    )+};
}

direct_inputs!(super::input::Key, super::input::TextCommitted);
direct_inputs!(crate::ui::selection::SelectAll);
direct_inputs!(
    crate::ui::clipboard::InspectSelection,
    crate::ui::clipboard::CopySelection
);
direct_inputs!(
    crate::ui::text_input::CompositionStarted,
    crate::ui::text_input::CompositionUpdated,
    crate::ui::text_input::CompositionEnded
);

#[doc(hidden)]
pub trait ViewInputs {
    const COUNT: usize;
    const REQUESTS: usize;
    fn prepare(
        self,
        world: &mut World,
        inputs: &mut PreparedInputs,
    ) -> Result<(), RuntimeInstallError>;
}

impl<E> ViewInputs for EventCallback<E, NodeContext>
where
    E: ViewInput,
    NodeContext: super::callback::CallbackContext<E, Target = crate::ecs::Entity>,
    E::Route: PrepareInput<E>,
{
    const COUNT: usize = 1;
    const REQUESTS: usize = E::Route::REQUEST as usize;

    fn prepare(
        self,
        world: &mut World,
        inputs: &mut PreparedInputs,
    ) -> Result<(), RuntimeInstallError> {
        E::Route::prepare(self, world, inputs)
    }
}

#[doc(hidden)]
pub trait PrepareInput<E: ViewInput>: EventRoute
where
    NodeContext: super::callback::CallbackContext<E, Target = crate::ecs::Entity>,
{
    const REQUEST: bool;
    fn prepare(
        callback: EventCallback<E, NodeContext>,
        world: &mut World,
        inputs: &mut PreparedInputs,
    ) -> Result<(), RuntimeInstallError>;
}

fn prepare_listener<E>(
    callback: EventCallback<E, NodeContext>,
    world: &mut World,
    inputs: &mut PreparedInputs,
) -> Result<(), RuntimeInstallError>
where
    E: ViewInput,
    NodeContext: super::callback::CallbackContext<E, Target = crate::ecs::Entity>,
    <E::Route as EventRoute>::Decision: ListenerDecision,
{
    let callback = prepare_shared_callback(world, callback)?;
    let matching = inputs
        .hooks
        .iter()
        .filter(|hook| hook.callback.kind() == core::any::TypeId::of::<E>())
        .count()
        + 1;
    inputs.max_dispatch = inputs.max_dispatch.max(matching);
    inputs.hooks.push(InputHook {
        callback: Rc::new(InstalledListener {
            callback,
            selector: E::selector(),
        }),
        gesture: E::GESTURE,
    });
    Ok(())
}

macro_rules! listener_routes {
    ($($route:ty),+ $(,)?) => {$(
        impl<E> PrepareInput<E> for $route
        where
            E: ViewInput<Route = Self>,
            NodeContext: super::callback::CallbackContext<E, Target = crate::ecs::Entity>,
        {
            const REQUEST: bool = false;
            fn prepare(callback: EventCallback<E, NodeContext>, world: &mut World, inputs: &mut PreparedInputs) -> Result<(), RuntimeInstallError> {
                prepare_listener(callback, world, inputs)
            }
        }
    )+};
}

listener_routes!(
    super::contract::EntityBubble,
    super::contract::InputFallback,
    super::contract::InputNotification
);

impl<E, D: 'static> PrepareInput<E> for super::contract::EntityRequest<D>
where
    E: ViewInput<Route = Self>,
    E::Source: super::contract::EntityEventSource,
    NodeContext: super::callback::CallbackContext<E, Target = crate::ecs::Entity>,
{
    const REQUEST: bool = true;
    fn prepare(
        callback: EventCallback<E, NodeContext>,
        world: &mut World,
        inputs: &mut PreparedInputs,
    ) -> Result<(), RuntimeInstallError> {
        if inputs
            .requests
            .iter()
            .any(|hook| hook.kind == core::any::TypeId::of::<E>())
        {
            return Err(RuntimeInstallError::DuplicateRequest {
                event: core::any::type_name::<E>(),
            });
        }
        inputs
            .requests
            .push(super::request::RequestHook::prepare(world, callback)?);
        Ok(())
    }
}

macro_rules! input_tuple {
    ($($item:ident),+) => {
        impl<$($item: ViewInputs),+> ViewInputs for ($($item,)+) {
            const COUNT: usize = 0 $(+ $item::COUNT)+;
            const REQUESTS: usize = 0 $(+ $item::REQUESTS)+;
            fn prepare(self, world: &mut World, inputs: &mut PreparedInputs) -> Result<(), RuntimeInstallError> {
                #[allow(non_snake_case)]
                let ($($item,)+) = self;
                $($item.prepare(world, inputs)?;)+
                Ok(())
            }
        }
    };
}

input_tuple!(A);
input_tuple!(A, B);
input_tuple!(A, B, C);
input_tuple!(A, B, C, D);
input_tuple!(A, B, C, D, E);
input_tuple!(A, B, C, D, E, F);
input_tuple!(A, B, C, D, E, F, G);
input_tuple!(A, B, C, D, E, F, G, H);
input_tuple!(A, B, C, D, E, F, G, H, I);
input_tuple!(A, B, C, D, E, F, G, H, I, J);

#[doc(hidden)]
pub fn prepare_inputs<I: ViewInputs>(
    world: &mut World,
    inputs: I,
) -> Result<PreparedInputs, RuntimeInstallError> {
    let mut prepared = PreparedInputs {
        hooks: Vec::with_capacity(I::COUNT - I::REQUESTS),
        requests: Vec::with_capacity(I::REQUESTS),
        max_dispatch: 0,
    };
    inputs.prepare(world, &mut prepared)?;
    super::runtime::freeze_shared_callbacks(world);
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{EventCapacity, gesture::Tap, runtime};

    #[crate::event]
    fn passthrough(_tap: event!(Tap)) -> bool {
        false
    }

    #[crate::event]
    fn remove(node: entity!(), _tap: event!(Tap), commands: commands!()) {
        commands.despawn(node);
    }

    #[test]
    fn failed_input_tuple_does_not_freeze_capacity_or_install_partial_callbacks() {
        let mut world = World::new();
        runtime::configure(&mut world, EventCapacity::new()).unwrap();
        assert!(matches!(
            prepare_inputs(&mut world, (passthrough::callback(), remove::callback())),
            Err(RuntimeInstallError::MissingCommandCapacity)
        ));
        runtime::configure(&mut world, EventCapacity::default()).unwrap();
        let prepared =
            prepare_inputs(&mut world, (passthrough::callback(), remove::callback())).unwrap();
        assert_eq!(prepared.hooks.len(), 2);
        assert_eq!(prepared.max_dispatch, 2);
        assert_eq!(
            runtime::configure(&mut world, EventCapacity::new()),
            Err(super::super::capacity::EventCapacityError::FrozenConfiguration)
        );
    }
}

#[cfg(test)]
pub(crate) fn fixture_capacity() -> crate::ecs::CommandCapacity {
    super::EventCapacity::default()
        .resolve()
        .unwrap()
        .commands
        .unwrap()
}

#[cfg(test)]
pub(crate) fn fixture_inputs(view: crate::ui::view::View, world: &mut World) -> SharedInputs {
    use crate::ui::view::ViewRegistry;
    let name = view.name();
    if world.resource::<ViewRegistry>().is_none() {
        world.insert_resource(ViewRegistry::default());
    }
    let existing = world
        .resource::<ViewRegistry>()
        .unwrap()
        .iter()
        .find(|registered| registered.name() == name)
        .and_then(|registered| registered.inputs().cloned());
    let inputs = match existing {
        Some(inputs) => inputs,
        None => {
            let inputs = view.inputs().expect("fixture View inputs").clone();
            world.resource_mut::<ViewRegistry>().unwrap().insert(view);
            inputs
        }
    };
    ViewRegistry::reconcile_observations(world);
    prepare(&inputs, world);
    inputs
}

#[cfg(test)]
pub(crate) fn invoke_payload<E: Event>(
    view: crate::ui::view::View,
    world: &mut World,
    node: crate::ecs::Entity,
    payload: &E,
    tap_count: Option<u8>,
) -> Result<bool, crate::ecs::CommandError> {
    let inputs = fixture_inputs(view, world);
    let state = inputs.borrow();
    let InputState::Ready(inputs) = &*state else {
        panic!("fixture preparation failed");
    };
    let event = super::listeners::BorrowedEvent::new(payload);
    let result = (|| {
        for hook in &inputs.hooks {
            if hook.callback.matches(&event, tap_count)
                && hook.callback.invoke(
                    world,
                    &event,
                    super::listeners::ListenerTarget::Entity(node),
                )? == super::BubbleControl::Prevent
            {
                return Ok(true);
            }
        }
        Ok(false)
    })();
    crate::ui::view::ViewRegistry::reconcile_observations(world);
    result
}

#[cfg(test)]
pub(crate) fn invoke_gesture(
    view: crate::ui::view::View,
    world: &mut World,
    node: crate::ecs::Entity,
    event: &super::gesture::GestureEvent,
) -> Result<bool, crate::ecs::CommandError> {
    use super::gesture::GestureEvent;
    macro_rules! dispatch {
        ($($kind:ident { $($field:ident),+ }),+ $(,)?) => {
            match event {
                $(GestureEvent::$kind { $($field),+ } => {
                    let payload = super::gesture::$kind { $($field: *$field),+ };
                    return invoke_payload(view, world, node, &payload, Some(1));
                }),+
            }
        };
    }
    dispatch!(
        Tap { x, y, target },
        LongPress { x, y, target },
        DragStart { x, y, target },
        DragMove {
            x,
            y,
            dx,
            dy,
            target
        },
        DragEnd {
            x,
            y,
            vx,
            vy,
            target
        },
        DragCancel { x, y, target },
        Pinch {
            x,
            y,
            scale_delta,
            target
        },
        Rotate {
            x,
            y,
            angle,
            target
        },
    );
}
