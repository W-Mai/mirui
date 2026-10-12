use super::lifecycle::prepare;
use crate::ecs::World;
use crate::input::event::focus::FocusState;
use crate::input::event::gesture::GestureEvent;
use crate::input::event::lifecycle::AppNotice;
use crate::input::event::listeners::{InputBranch, ListenerError};
use crate::input::event::{BubbleControl, multi_tap, runtime};

pub fn flush(world: &mut World) -> Result<usize, runtime::RuntimeRunError> {
    prepare(world);
    runtime::flush(world, prepare)
}

pub(crate) fn notify_app<E: AppNotice>(
    world: &mut World,
    event: E,
) -> Result<usize, runtime::RuntimeRunError> {
    prepare(world);
    runtime::notify_app(world, event, prepare)
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
