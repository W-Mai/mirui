pub mod focus;
pub mod gesture;
pub mod hit_test;
pub mod input;
pub mod multi_tap;
pub mod scroll;
pub mod sim;
pub mod widget_input;

use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::widget::{Parent, UserState};

use focus::key_dispatch;
use gesture::{GestureEvent, GestureSystem};
use hit_test::hit_test;
use input::InputEvent;
use scroll::{ScrollDragState, scroll_system};

#[derive(Clone, Copy, Default)]
pub struct PointerCursor {
    pub x: Fixed,
    pub y: Fixed,
    pub down: bool,
    /// Bumps on every PointerDown / PointerUp; PointerMove leaves it.
    pub event_seq: u32,
}

/// Single source of truth for the per-event side of the input
/// pipeline. Both `App::run`'s real input loop and
/// `sim_timeline_system` (which fakes pointer events) call this so
/// that simulated inputs traverse the exact same scroll / hit-test /
/// gesture-recognizer / key-dispatch path as real ones.
///
/// Does *not* drain `GestureSystem.events` — the caller decides when
/// to dispatch (real input loop drains after the whole `poll_event`
/// burst; sim drains every system tick).
pub fn dispatch_input(
    world: &mut World,
    root: Entity,
    event: &InputEvent,
    now_ms: u32,
    lw: u16,
    lh: u16,
) {
    match event {
        InputEvent::PointerDown { x, y, .. } => {
            let mut next = world
                .resource::<PointerCursor>()
                .copied()
                .unwrap_or_default();
            next.x = *x;
            next.y = *y;
            next.down = true;
            next.event_seq = next.event_seq.wrapping_add(1);
            world.insert_resource(next);
        }
        InputEvent::PointerMove { x, y, .. } => {
            let mut next = world
                .resource::<PointerCursor>()
                .copied()
                .unwrap_or_default();
            next.x = *x;
            next.y = *y;
            world.insert_resource(next);
        }
        InputEvent::PointerUp { x, y, .. } => {
            let mut next = world
                .resource::<PointerCursor>()
                .copied()
                .unwrap_or_default();
            next.x = *x;
            next.y = *y;
            next.down = false;
            next.event_seq = next.event_seq.wrapping_add(1);
            world.insert_resource(next);
        }
        _ => {}
    }

    if let InputEvent::PointerDown { x, y, .. } = event {
        if let Some(target) = hit_test(world, root, *x, *y, lw, lh) {
            if entity_or_ancestor_disabled(world, target) {
                key_dispatch(world, event);
                return;
            }
        }
    }

    scroll_system(world, root, event, lw, lh);

    let hit = match event {
        InputEvent::PointerDown { x, y, .. } => hit_test(world, root, *x, *y, lw, lh),
        _ => None,
    };
    let scroll_claimed = world
        .resource::<ScrollDragState>()
        .is_some_and(|s| s.active && s.resolved);
    if let Some(gs) = world.resource_mut::<GestureSystem>() {
        gs.recognizer.scroll_claimed = scroll_claimed;
        gs.recognizer.update(event, now_ms, hit, &mut gs.events);
    }

    key_dispatch(world, event);
}

pub fn entity_or_ancestor_disabled(world: &World, entity: Entity) -> bool {
    let mut cur = Some(entity);
    while let Some(e) = cur {
        if matches!(world.get::<UserState>(e), Some(UserState::Disabled)) {
            return true;
        }
        cur = world.get::<Parent>(e).map(|p| p.0);
    }
    false
}

/// Gesture handler callback. `Fn` is a zero-alloc pointer for internal and
/// hand-written handlers; `Closure` carries an `Rc` so `ui!` handlers can
/// capture state (e.g. a `Signal`). Returns `true` to consume the event.
type GestureFn = fn(&mut World, Entity, &GestureEvent) -> bool;
type GestureClosure = alloc::rc::Rc<dyn Fn(&mut World, Entity, &GestureEvent) -> bool>;

pub enum GestureCallback {
    Fn(GestureFn),
    Closure(GestureClosure),
}

impl GestureCallback {
    fn call(&self, world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
        match self {
            GestureCallback::Fn(f) => f(world, entity, event),
            GestureCallback::Closure(rc) => rc(world, entity, event),
        }
    }
}

pub struct GestureHandler {
    pub on_gesture: GestureCallback,
}

impl GestureHandler {
    pub fn from_fn(f: GestureFn) -> Self {
        GestureHandler {
            on_gesture: GestureCallback::Fn(f),
        }
    }

    /// Look up `entity`'s gesture handler and run it. Clones the callback out
    /// of the borrow before invoking so the closure may touch the World.
    pub fn trigger(world: &mut World, entity: Entity, event: &GestureEvent) -> Option<bool> {
        let cb = world
            .get::<GestureHandler>(entity)
            .map(|h| match &h.on_gesture {
                GestureCallback::Fn(f) => GestureCallback::Fn(*f),
                GestureCallback::Closure(rc) => GestureCallback::Closure(alloc::rc::Rc::clone(rc)),
            })?;
        Some(cb.call(world, entity, event))
    }
}

/// Business-event callback, generic over the widget's event type. `Closure`
/// carries an `Rc` so a `ui!` handler can capture state such as a `Signal`.
pub type BusinessFn<E> = fn(&mut World, Entity, &E) -> bool;
pub type BusinessClosure<E> = alloc::rc::Rc<dyn Fn(&mut World, Entity, &E) -> bool>;

pub enum BusinessCallback<E> {
    Fn(BusinessFn<E>),
    Closure(BusinessClosure<E>),
}

impl<E> BusinessCallback<E> {
    pub fn call(&self, world: &mut World, entity: Entity, event: &E) -> bool {
        match self {
            BusinessCallback::Fn(f) => f(world, entity, event),
            BusinessCallback::Closure(rc) => rc(world, entity, event),
        }
    }

    /// Clone the callback out of a component borrow so it can run while the
    /// World is mutably borrowed (same pattern as `GestureHandler::trigger`).
    pub fn clone_out(&self) -> Self {
        match self {
            BusinessCallback::Fn(f) => BusinessCallback::Fn(*f),
            BusinessCallback::Closure(rc) => BusinessCallback::Closure(alloc::rc::Rc::clone(rc)),
        }
    }
}

impl<E> From<BusinessFn<E>> for BusinessCallback<E> {
    fn from(f: BusinessFn<E>) -> Self {
        BusinessCallback::Fn(f)
    }
}

/// Aggregated context handed to user `on EventKind` bodies and callback-form fns.
pub struct HandlerCtx<'a, E> {
    pub world: &'a mut World,
    pub entity: Entity,
    pub event: &'a E,
}

/// Spelled-out bubble control for `on EventKind` body return values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BubbleControl {
    Prevent,
    Allow,
}

/// Bridges user `on EventKind` body return types into the dispatch fn's bubble-control bit.
///
/// - `()` → `Prevent` (default; mirrors web's "stopPropagation by default" idiom)
/// - `bool` → `true = Prevent, false = Allow`
/// - `BubbleControl` → pass through
pub trait HandlerReturn {
    fn into_consumed(self) -> bool;
}

impl HandlerReturn for () {
    fn into_consumed(self) -> bool {
        true
    }
}

impl HandlerReturn for bool {
    fn into_consumed(self) -> bool {
        self
    }
}

impl HandlerReturn for BubbleControl {
    fn into_consumed(self) -> bool {
        matches!(self, BubbleControl::Prevent)
    }
}

/// Walk from `target` up via `Parent` links, invoking the first
/// `GestureHandler` found. Stops when a handler returns `true`
/// (consumed) or the root is reached.
pub fn bubble_dispatch(world: &mut World, event: &GestureEvent) {
    bubble_dispatch_at(world, event, 0);
}

/// `now_ms`-aware variant; pass `0` when no clock is available.
pub fn bubble_dispatch_at(world: &mut World, event: &GestureEvent, now_ms: u32) {
    multi_tap::observe_gesture(world, event, now_ms);
    let mut current = event.target();
    loop {
        let internals = collect_internal_handlers(world, current);
        for f in internals {
            if f(world, current, event) {
                return;
            }
        }
        if GestureHandler::trigger(world, current, event) == Some(true) {
            return;
        }
        match world.get::<Parent>(current) {
            Some(p) => current = p.0,
            None => return,
        }
    }
}

fn collect_internal_handlers(
    world: &World,
    entity: Entity,
) -> alloc::vec::Vec<crate::widget::view::ViewInternalGesture> {
    let Some(registry) = world.resource::<crate::widget::view::ViewRegistry>() else {
        return alloc::vec::Vec::new();
    };
    registry
        .iter()
        .filter_map(|v| {
            let f = v.internal_gesture()?;
            match v.component_filter() {
                Some(type_id) if world.has_type(entity, type_id) => Some(f),
                Some(_) => None,
                None => Some(f),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestor_disabled_propagates() {
        let mut world = World::new();
        let parent = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(child, Parent(parent));
        world.insert(parent, UserState::Disabled);
        assert!(entity_or_ancestor_disabled(&world, child));
    }

    #[test]
    fn entity_self_disabled() {
        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, UserState::Disabled);
        assert!(entity_or_ancestor_disabled(&world, e));
    }

    #[test]
    fn unrelated_entity_not_disabled() {
        let mut world = World::new();
        let a = world.spawn_empty();
        let b = world.spawn_empty();
        world.insert(a, UserState::Disabled);
        assert!(!entity_or_ancestor_disabled(&world, b));
    }

    #[test]
    fn errored_does_not_propagate_via_disabled_walk() {
        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, UserState::Errored);
        assert!(!entity_or_ancestor_disabled(&world, e));
    }

    mod dual_channel {
        use super::*;
        use crate::types::Fixed;
        use crate::widget::view::{View, ViewRegistry};
        use core::sync::atomic::{AtomicU8, Ordering};
        use std::sync::Mutex;

        struct ChannelMarker;

        static INTERNAL_FIRES: AtomicU8 = AtomicU8::new(0);
        static USER_FIRES: AtomicU8 = AtomicU8::new(0);
        static SERIAL: Mutex<()> = Mutex::new(());

        fn reset() {
            INTERNAL_FIRES.store(0, Ordering::SeqCst);
            USER_FIRES.store(0, Ordering::SeqCst);
        }

        fn internal_consume(_w: &mut World, _e: Entity, _ev: &GestureEvent) -> bool {
            INTERNAL_FIRES.fetch_add(1, Ordering::SeqCst);
            true
        }

        fn internal_passthrough(_w: &mut World, _e: Entity, _ev: &GestureEvent) -> bool {
            INTERNAL_FIRES.fetch_add(1, Ordering::SeqCst);
            false
        }

        fn user_handler(_w: &mut World, _e: Entity, _ev: &GestureEvent) -> bool {
            USER_FIRES.fetch_add(1, Ordering::SeqCst);
            true
        }

        fn registry_with(internal: fn(&mut World, Entity, &GestureEvent) -> bool) -> ViewRegistry {
            fn dummy_render(
                _r: &mut dyn crate::draw::renderer::Renderer,
                _w: &World,
                _e: Entity,
                _rect: &crate::types::Rect,
                _ctx: &mut crate::widget::view::ViewCtx,
            ) {
            }
            let mut reg = ViewRegistry::default();
            reg.insert(
                View::new("ChannelMarker", 60, dummy_render)
                    .with_filter::<ChannelMarker>()
                    .with_internal_gesture(internal),
            );
            reg
        }

        fn tap_event(target: Entity) -> GestureEvent {
            GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target,
            }
        }

        #[test]
        fn user_only_runs_when_no_internal() {
            let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            reset();
            let mut world = World::new();
            world.insert_resource(ViewRegistry::default());
            let e = world.spawn_empty();
            world.insert(e, GestureHandler::from_fn(user_handler));
            bubble_dispatch_at(&mut world, &tap_event(e), 0);
            assert_eq!(INTERNAL_FIRES.load(Ordering::SeqCst), 0);
            assert_eq!(USER_FIRES.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn internal_only_runs_when_no_user() {
            let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            reset();
            let mut world = World::new();
            world.insert_resource(registry_with(internal_consume));
            let e = world.spawn_empty();
            world.insert(e, ChannelMarker);
            bubble_dispatch_at(&mut world, &tap_event(e), 0);
            assert_eq!(INTERNAL_FIRES.load(Ordering::SeqCst), 1);
            assert_eq!(USER_FIRES.load(Ordering::SeqCst), 0);
        }

        #[test]
        fn internal_consumes_blocks_user() {
            let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            reset();
            let mut world = World::new();
            world.insert_resource(registry_with(internal_consume));
            let e = world.spawn_empty();
            world.insert(e, ChannelMarker);
            world.insert(e, GestureHandler::from_fn(user_handler));
            bubble_dispatch_at(&mut world, &tap_event(e), 0);
            assert_eq!(INTERNAL_FIRES.load(Ordering::SeqCst), 1);
            assert_eq!(
                USER_FIRES.load(Ordering::SeqCst),
                0,
                "internal returned true, user must not fire",
            );
        }

        #[test]
        fn internal_passthrough_lets_user_run() {
            let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            reset();
            let mut world = World::new();
            world.insert_resource(registry_with(internal_passthrough));
            let e = world.spawn_empty();
            world.insert(e, ChannelMarker);
            world.insert(e, GestureHandler::from_fn(user_handler));
            bubble_dispatch_at(&mut world, &tap_event(e), 0);
            assert_eq!(INTERNAL_FIRES.load(Ordering::SeqCst), 1);
            assert_eq!(
                USER_FIRES.load(Ordering::SeqCst),
                1,
                "internal returned false, user must fire on the same entity",
            );
        }
    }
}
