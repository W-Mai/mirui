use super::lifecycle::prepare;
use crate::ecs::World;
use crate::input::event::focus::FocusState;
use crate::input::event::gesture::GestureEvent;
use crate::input::event::lifecycle::AppNotice;
use crate::input::event::listeners::{InputBranch, ListenerError};
use crate::input::event::{BubbleControl, multi_tap, runtime};

pub(super) struct KeyboardDelivery {
    origin: Option<crate::ecs::Entity>,
    key: crate::input::event::input::Key,
    shortcut: Option<super::selection::Shortcut>,
}

impl KeyboardDelivery {
    pub(super) fn capture(
        world: &World,
        origin: Option<crate::ecs::Entity>,
        key: crate::input::event::input::Key,
    ) -> Self {
        Self {
            origin,
            shortcut: origin
                .and_then(|target| super::selection::Shortcut::capture(world, target, &key)),
            key,
        }
    }

    pub(super) fn deliver(self, world: &mut World) -> Result<(), ListenerError> {
        if runtime::keyboard(world, self.origin, &self.key)? == BubbleControl::Allow
            && let Some(shortcut) = self.shortcut
        {
            shortcut.apply(world);
        }
        Ok(())
    }
}

pub fn flush(world: &mut World) -> Result<usize, runtime::RuntimeRunError> {
    prepare(world);
    let result = runtime::flush(world, prepare);
    super::text_input::flush(world);
    result
}

pub(crate) fn dispatch_input(
    world: &mut World,
    root: Option<crate::ecs::Entity>,
    event: &crate::input::event::input::InputEvent,
    width: u16,
    height: u16,
) -> Result<bool, ListenerError> {
    use crate::input::event::input::{InputEvent, Key};
    match event {
        InputEvent::ClipboardInput(input) => {
            if let Some(target) = world
                .resource::<FocusState>()
                .and_then(|focus| focus.focused)
            {
                input.deliver(world, target);
            }
            return Ok(true);
        }
        InputEvent::Clipboard(response) => {
            super::clipboard::complete(world, response);
            return Ok(true);
        }
        InputEvent::TextInput(event) => {
            super::text_input::deliver_native(world, event);
            return Ok(true);
        }
        InputEvent::Key {
            physical,
            code,
            pressed,
            modifiers,
            repeat,
        } => {
            let key = Key {
                physical: *physical,
                code: *code,
                pressed: *pressed,
                modifiers: *modifiers,
                repeat: *repeat,
            };
            let origin = world
                .resource::<FocusState>()
                .and_then(|focus| focus.focused);
            let delivery = KeyboardDelivery::capture(world, origin, key);
            if super::text_input::intercept_key(world, &delivery.key) {
                return Ok(true);
            }
            delivery.deliver(world)?;
            return Ok(true);
        }
        InputEvent::TextCommitted(_) if super::text_input::software_selected(world) => {
            return Ok(true);
        }
        _ => {}
    }
    runtime::dispatch_input(world, root, event, width, height)
}

pub(crate) fn notify_app<E: AppNotice>(
    world: &mut World,
    event: E,
) -> Result<usize, runtime::RuntimeRunError> {
    prepare(world);
    let result = runtime::notify_app(world, event, prepare);
    super::text_input::flush(world);
    result
}

pub fn dispatch_gesture(
    world: &mut World,
    event: &GestureEvent,
    now_ms: u32,
) -> Result<BubbleControl, ListenerError> {
    prepare(world);
    let branch = InputBranch::scan(world, event.target())?;
    if branch.is_hidden() || !crate::core::reactive::retained::enabled(world, event.target()) {
        return Ok(BubbleControl::Allow);
    }
    let _owner_scope = crate::core::reactive::OwnerGuard::enter(world);
    let origin = crate::core::reactive::retained::InputLease::capture(world, event.target());
    if matches!(event, GestureEvent::Tap { .. })
        && !branch.preserves_pointer_focus()
        && let Some(focus) = world.resource_mut::<FocusState>()
    {
        focus.focused = branch.focus_target();
    }
    multi_tap::observe_gesture(world, event, now_ms);
    runtime::initialize(world);
    if matches!(event, GestureEvent::Tap { .. })
        && let Err(error) = flush(world)
    {
        crate::warn!(target: "mirui::event::focus", "focus delivery skipped: {:?}", error);
    }
    if origin.is_some_and(|lease| !lease.valid(world)) {
        return Ok(BubbleControl::Allow);
    }
    prepare(world);
    runtime::dispatch_gesture(world, event)
}
