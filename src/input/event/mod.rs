pub(crate) mod binding;
pub(crate) mod callback;
pub(crate) mod capacity;
pub mod contract;
pub(crate) mod emission;
pub mod focus;
pub mod gesture;
pub mod hit_test;
pub mod input;
pub mod layout;
pub mod lifecycle;
pub(crate) mod listeners;
pub mod multi_tap;
pub(crate) mod notification;
pub(crate) mod runtime;
pub mod scroll;
pub mod sim;
pub mod widget_input;

pub use capacity::EventCapacity;

use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::ui::{Parent, UserState};

use gesture::GestureSystem;
use hit_test::hit_test;
use input::InputEvent;
use scroll::{ScrollDragState, scroll_system_with_target};

const INACTIVE_POINTER_COORDINATE: Fixed = Fixed::from_int(-1);

#[derive(Clone, Copy)]
pub struct PointerCursor {
    pub x: Fixed,
    pub y: Fixed,
    pub down: bool,
    /// Bumps on every pointer press, release, or cancellation.
    pub event_seq: u32,
}

impl Default for PointerCursor {
    fn default() -> Self {
        Self {
            x: INACTIVE_POINTER_COORDINATE,
            y: INACTIVE_POINTER_COORDINATE,
            down: false,
            event_seq: 0,
        }
    }
}

fn update_pointer_cursor(world: &mut World, update: impl FnOnce(&mut PointerCursor)) {
    if let Some(cursor) = world.resource_mut::<PointerCursor>() {
        update(cursor);
    } else {
        let mut cursor = PointerCursor::default();
        update(&mut cursor);
        world.insert_resource(cursor);
    }
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
    root: Option<Entity>,
    event: &InputEvent,
    now_ms: u32,
    lw: u16,
    lh: u16,
) {
    let _owner_scope = crate::core::reactive::OwnerGuard::enter(world);
    if matches!(event, InputEvent::AppSuspend) {
        cancel_active_input(world, now_ms);
        return;
    }
    runtime::initialize(world);
    if let Some(invalid) = world
        .resource::<GestureSystem>()
        .and_then(|system| system.recognizer.captured_target())
        .and_then(|target| crate::core::reactive::retained::invalid_root(world, target))
    {
        retire_subtree_interaction(world, invalid);
    }
    if let Err(error) = runtime::prepare(world) {
        crate::warn!("input route preparation failed: {:?}", error);
        return;
    }
    match runtime::dispatch_input(world, root, event, lw, lh) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            crate::warn!("input event dispatch failed: {:?}", error);
            return;
        }
    }
    match event {
        InputEvent::PointerDown { x, y, .. } => {
            update_pointer_cursor(world, |cursor| {
                cursor.x = *x;
                cursor.y = *y;
                cursor.down = true;
                cursor.event_seq = cursor.event_seq.wrapping_add(1);
            });
        }
        InputEvent::PointerMove { x, y, .. } => {
            update_pointer_cursor(world, |cursor| {
                cursor.x = *x;
                cursor.y = *y;
            });
        }
        InputEvent::PointerUp { x, y, .. } | InputEvent::PointerCancel { x, y, .. } => {
            update_pointer_cursor(world, |cursor| {
                cursor.x = *x;
                cursor.y = *y;
                cursor.down = false;
                cursor.event_seq = cursor.event_seq.wrapping_add(1);
            });
        }
        _ => {}
    }

    let Some(root) = root else {
        return;
    };

    let pointer_down_target = match event {
        InputEvent::PointerDown { x, y, .. } => hit_test(world, root, *x, *y, lw, lh),
        _ => None,
    };
    if pointer_down_target.is_some_and(|target| {
        entity_or_ancestor_disabled(world, target)
            || !crate::core::reactive::retained::enabled(world, target)
    }) {
        return;
    }

    scroll_system_with_target(world, root, event, lw, lh, pointer_down_target, now_ms);

    let scroll_claimed = world
        .resource::<ScrollDragState>()
        .is_some_and(|s| s.active && s.resolved);
    if let Some(gs) = world.resource_mut::<GestureSystem>() {
        gs.recognizer.scroll_claimed = scroll_claimed;
        gs.recognizer
            .update(event, now_ms, pointer_down_target, &mut gs.events);
    }
}

pub(crate) fn cancel_active_input(world: &mut World, now_ms: u32) {
    if let Some(cursor) = world.resource_mut::<PointerCursor>() {
        cursor.down = false;
        cursor.event_seq = cursor.event_seq.wrapping_add(1);
    }
    scroll::system::cancel_scroll_drag(world);
    if let Some(gestures) = world.resource_mut::<GestureSystem>() {
        gestures
            .recognizer
            .update(&InputEvent::AppSuspend, now_ms, None, &mut gestures.events);
    }
    crate::ui::state::clear_active_interaction_states(world);
}

pub(crate) fn retire_subtree_interaction(world: &mut World, root: Entity) {
    use crate::ui::branch::is_in_subtree;
    crate::core::reactive::retained::retire(world, root);
    let captured = world
        .resource::<GestureSystem>()
        .and_then(|system| system.recognizer.captured_target());
    let cancellation = if captured.is_some_and(|target| is_in_subtree(world, root, target)) {
        world
            .resource_mut::<GestureSystem>()
            .and_then(|system| system.recognizer.release_capture())
    } else {
        None
    };
    gesture::system::discard_pending_in_subtree(world, root);
    let mut index = 0;
    while let Some(target) = world
        .resource::<GestureSystem>()
        .and_then(|system| system.events.buffer.get(index))
        .map(|event| event.target())
    {
        if is_in_subtree(world, root, target) {
            world
                .resource_mut::<GestureSystem>()
                .unwrap()
                .events
                .buffer
                .remove(index);
        } else {
            index += 1;
        }
    }
    if let Some(event) = cancellation
        && let Err(error) = runtime::dispatch_gesture(world, &event, 0)
    {
        crate::warn!(target: "mirui::event", "retained cell cancellation skipped: {:?}", error);
    }
    if world
        .resource::<focus::FocusState>()
        .and_then(|focus| focus.focused)
        .is_some_and(|entity| is_in_subtree(world, root, entity))
    {
        world.resource_mut::<focus::FocusState>().unwrap().focused = None;
    }
    if world
        .resource::<multi_tap::MultiTapTracker>()
        .and_then(|tracker| tracker.last)
        .is_some_and(|last| is_in_subtree(world, root, last.entity))
    {
        world
            .resource_mut::<multi_tap::MultiTapTracker>()
            .unwrap()
            .last = None;
    }
    if world
        .resource::<ScrollDragState>()
        .is_some_and(|drag| drag.active && is_in_subtree(world, root, drag.target))
    {
        scroll::system::cancel_scroll_drag(world);
    }
    crate::ui::state::clear_subtree_interaction_states(world, root);
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

/// Spelled-out bubble control for `on EventKind` body return values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BubbleControl {
    Prevent,
    Allow,
}

#[cfg(test)]
mod tests {
    use super::contract::Event;
    use super::gesture::GestureEvent;
    use super::*;

    struct TapCount(u8);

    #[crate::event]
    fn count_tap(count: res!(mut TapCount)) -> bool {
        count.0 += 1;
        true
    }

    struct ExpectedOwner(crate::ecs::world::WorldId);

    #[crate::event]
    fn check_owner(expected: res!(ExpectedOwner)) -> bool {
        assert_eq!(crate::core::reactive::current_world_id(), Some(expected.0));
        assert!(crate::core::reactive::with_world(|_| ()).is_none());
        true
    }

    #[crate::event]
    fn panic_handler() -> bool {
        panic!("gesture unwind");
    }

    #[test]
    fn internal_handlers_keep_registration_order_and_snapshot_filter_matches() {
        struct Marker;
        impl crate::ecs::Component for Marker {}
        #[derive(Default)]
        struct Calls {
            first: u8,
            second: u8,
            user: u8,
        }
        fn first(
            world: &mut World,
            entity: Entity,
            _: &GestureEvent,
        ) -> Result<bool, crate::ecs::CommandError> {
            world.remove::<Marker>(entity);
            world.resource_mut::<Calls>().unwrap().first += 1;
            Ok(false)
        }
        fn second(
            world: &mut World,
            _: Entity,
            _: &GestureEvent,
        ) -> Result<bool, crate::ecs::CommandError> {
            world.resource_mut::<Calls>().unwrap().second += 1;
            Ok(true)
        }

        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Marker);
        world.insert_resource(Calls::default());
        // SAFETY: this fixture fetches only Calls and captures no Model.
        let user = unsafe {
            callback::event_callback::<
                gesture::Tap,
                callback::NodeContext,
                (crate::__macro_support::ResMut<Calls>,),
                bool,
            >(
                "user",
                const { &[crate::__macro_support::SystemAccess::write_resource::<Calls>()] },
                |(calls,), _, _, _scope| {
                    calls.user += 1;
                    true
                },
            )
        };
        binding::install_entity_event(&mut world, entity, gesture::Tap::EVENT, user);
        let mut registry = crate::ui::view::ViewRegistry::default();
        registry.insert(
            crate::ui::view::View::systems_only("first", &[])
                .with_filter::<Marker>()
                .with_internal_gesture(first),
        );
        registry.insert(
            crate::ui::view::View::systems_only("second", &[])
                .with_filter::<Marker>()
                .with_internal_gesture(second),
        );
        world.insert_resource(registry);

        runtime::dispatch_gesture(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: entity,
            },
            0,
        )
        .unwrap();
        let calls = world.resource::<Calls>().unwrap();
        assert_eq!((calls.first, calls.second, calls.user), (1, 1, 0));
    }

    #[test]
    fn gesture_saved_before_a_branch_switch_cannot_reach_a_hidden_target() {
        let mut world = World::new();
        world.insert_resource(TapCount(0));
        let previous_focus = world.spawn_empty();
        world.insert_resource(focus::FocusState {
            focused: Some(previous_focus),
        });
        let branch = world.spawn_empty();
        let target = world.spawn_empty();
        world.insert(target, Parent(branch));
        world.insert(target, focus::Focusable);
        binding::install_entity_event(
            &mut world,
            target,
            gesture::Tap::EVENT,
            count_tap::callback::<gesture::Tap, callback::NodeContext>(),
        );
        world.insert(
            branch,
            crate::ui::branch::CachedBranchVisibility { selected: false },
        );
        let tap = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        };

        runtime::dispatch_gesture(&mut world, &tap, 0).unwrap();
        assert_eq!(world.resource::<TapCount>().unwrap().0, 0);
        assert!(world.resource::<multi_tap::MultiTapTracker>().is_none());
        assert_eq!(
            world.resource::<focus::FocusState>().unwrap().focused,
            Some(previous_focus)
        );

        world
            .get_mut::<crate::ui::branch::CachedBranchVisibility>(branch)
            .unwrap()
            .selected = true;
        runtime::dispatch_gesture(&mut world, &tap, 0).unwrap();
        assert_eq!(world.resource::<TapCount>().unwrap().0, 1);
        let recorded = world
            .resource::<multi_tap::MultiTapTracker>()
            .unwrap()
            .last
            .unwrap();
        assert_eq!(
            (recorded.entity, recorded.time_ms, recorded.count),
            (target, 0, 1)
        );
        assert_eq!(
            world.resource::<focus::FocusState>().unwrap().focused,
            Some(target)
        );

        world.insert(branch, crate::ui::Hidden);
        world.resource_mut::<focus::FocusState>().unwrap().focused = Some(previous_focus);
        runtime::dispatch_gesture(&mut world, &tap, 0).unwrap();
        assert_eq!(world.resource::<TapCount>().unwrap().0, 1);
        let preserved = world
            .resource::<multi_tap::MultiTapTracker>()
            .unwrap()
            .last
            .unwrap();
        assert_eq!(
            (preserved.entity, preserved.time_ms, preserved.count),
            (recorded.entity, recorded.time_ms, recorded.count)
        );
        assert_eq!(
            world.resource::<focus::FocusState>().unwrap().focused,
            Some(previous_focus)
        );
    }

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
    fn gesture_callback_runs_with_its_world_scope() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert_resource(ExpectedOwner(world.id()));
        binding::install_entity_event(
            &mut world,
            entity,
            gesture::Tap::EVENT,
            check_owner::callback::<gesture::Tap, callback::NodeContext>(),
        );
        runtime::dispatch_gesture(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: entity,
            },
            0,
        )
        .unwrap();
        assert_eq!(crate::core::reactive::current_world_id(), None);
    }

    #[test]
    fn gesture_owner_scope_restores_after_callback_unwinds() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        binding::install_entity_event(
            &mut world,
            entity,
            gesture::Tap::EVENT,
            panic_handler::callback::<gesture::Tap, callback::NodeContext>(),
        );
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime::dispatch_gesture(
                &mut world,
                &GestureEvent::Tap {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                    target: entity,
                },
                0,
            )
            .unwrap();
        }));
        assert!(failed.is_err());
        assert_eq!(crate::core::reactive::current_world_id(), None);
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

    #[test]
    fn pointer_cancel_releases_cursor_and_advances_sequence() {
        let mut world = World::new();
        let root = world.spawn_empty();
        world.insert_resource(PointerCursor {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            down: true,
            event_seq: 7,
        });

        dispatch_input(
            &mut world,
            Some(root),
            &InputEvent::PointerCancel {
                id: 0,
                x: Fixed::from_int(12),
                y: Fixed::from_int(18),
            },
            10,
            64,
            64,
        );

        let cursor = world.resource::<PointerCursor>().unwrap();
        assert_eq!(cursor.x, Fixed::from_int(12));
        assert_eq!(cursor.y, Fixed::from_int(18));
        assert!(!cursor.down);
        assert_eq!(cursor.event_seq, 8);
    }

    #[test]
    fn pointer_cursor_updates_existing_resource_across_events() {
        let mut world = World::new();
        let root = world.spawn_empty();
        world.insert_resource(PointerCursor::default());
        let address = world.resource::<PointerCursor>().unwrap() as *const PointerCursor;

        for event in [
            InputEvent::PointerDown {
                id: 0,
                x: Fixed::from_int(4),
                y: Fixed::from_int(5),
            },
            InputEvent::PointerMove {
                id: 0,
                x: Fixed::from_int(6),
                y: Fixed::from_int(7),
            },
            InputEvent::PointerUp {
                id: 0,
                x: Fixed::from_int(8),
                y: Fixed::from_int(9),
            },
        ] {
            dispatch_input(&mut world, Some(root), &event, 0, 64, 64);
            assert_eq!(
                world.resource::<PointerCursor>().unwrap() as *const _,
                address
            );
        }

        let cursor = world.resource::<PointerCursor>().unwrap();
        assert_eq!(
            (cursor.x, cursor.y),
            (Fixed::from_int(8), Fixed::from_int(9))
        );
        assert!(!cursor.down);
        assert_eq!(cursor.event_seq, 2);
    }

    #[test]
    fn suspend_cancellation_clears_active_input_without_a_root() {
        use crate::input::event::scroll::{ScrollMotion, ScrollOffset};
        use crate::ui::{InteractionState, dirty::Dirty};

        let mut world = World::new();
        let node = world.spawn_empty();
        world.insert(node, InteractionState::Pressed);
        world.insert(node, UserState::Errored);
        world.insert(node, Dirty);
        world.insert(
            node,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(-8),
            },
        );
        world.insert_resource(PointerCursor {
            down: true,
            event_seq: 7,
            ..PointerCursor::default()
        });
        world.insert_resource(ScrollDragState {
            active: true,
            resolved: true,
            target: node,
            ..ScrollDragState::default()
        });
        world.insert_resource(ScrollMotion::default());
        world.insert_resource(GestureSystem::default());

        dispatch_input(&mut world, None, &InputEvent::AppSuspend, 10, 64, 64);

        let cursor = world.resource::<PointerCursor>().unwrap();
        assert!(!cursor.down);
        assert_eq!(cursor.event_seq, 8);
        let scroll = world.resource::<ScrollDragState>().unwrap();
        assert!(!scroll.active);
        assert!(!scroll.resolved);
        assert!(
            world
                .resource::<ScrollMotion>()
                .unwrap()
                .target_entity
                .is_none()
        );
        assert_eq!(world.get::<ScrollOffset>(node).unwrap().y, Fixed::ZERO);
        assert!(!world.has::<InteractionState>(node));
        assert!(world.has::<Dirty>(node));
        assert!(matches!(
            world.get::<UserState>(node),
            Some(UserState::Errored)
        ));
    }

    mod dual_channel {
        use super::*;
        use crate::types::Fixed;
        use crate::ui::view::{View, ViewRegistry};
        use core::sync::atomic::{AtomicU8, Ordering};
        use std::sync::Mutex;

        struct ChannelMarker;
        impl crate::ecs::Component for ChannelMarker {}

        static INTERNAL_FIRES: AtomicU8 = AtomicU8::new(0);
        static USER_FIRES: AtomicU8 = AtomicU8::new(0);
        static SERIAL: Mutex<()> = Mutex::new(());

        fn reset() {
            INTERNAL_FIRES.store(0, Ordering::SeqCst);
            USER_FIRES.store(0, Ordering::SeqCst);
        }

        fn internal_consume(
            _w: &mut World,
            _e: Entity,
            _ev: &GestureEvent,
        ) -> Result<bool, crate::ecs::CommandError> {
            INTERNAL_FIRES.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }

        fn internal_passthrough(
            _w: &mut World,
            _e: Entity,
            _ev: &GestureEvent,
        ) -> Result<bool, crate::ecs::CommandError> {
            INTERNAL_FIRES.fetch_add(1, Ordering::SeqCst);
            Ok(false)
        }

        #[crate::event]
        fn user_handler() -> bool {
            USER_FIRES.fetch_add(1, Ordering::SeqCst);
            true
        }

        fn registry_with(internal: crate::ui::view::ViewInternalGesture) -> ViewRegistry {
            fn dummy_render(_w: &World, _e: Entity, _paint: &mut crate::ui::view::PaintCtx<'_>) {}
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
            binding::install_entity_event(
                &mut world,
                e,
                gesture::Tap::EVENT,
                user_handler::callback::<gesture::Tap, callback::NodeContext>(),
            );
            runtime::dispatch_gesture(&mut world, &tap_event(e), 0).unwrap();
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
            runtime::dispatch_gesture(&mut world, &tap_event(e), 0).unwrap();
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
            binding::install_entity_event(
                &mut world,
                e,
                gesture::Tap::EVENT,
                user_handler::callback::<gesture::Tap, callback::NodeContext>(),
            );
            runtime::dispatch_gesture(&mut world, &tap_event(e), 0).unwrap();
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
            binding::install_entity_event(
                &mut world,
                e,
                gesture::Tap::EVENT,
                user_handler::callback::<gesture::Tap, callback::NodeContext>(),
            );
            runtime::dispatch_gesture(&mut world, &tap_event(e), 0).unwrap();
            assert_eq!(INTERNAL_FIRES.load(Ordering::SeqCst), 1);
            assert_eq!(
                USER_FIRES.load(Ordering::SeqCst),
                1,
                "internal returned false, user must fire on the same entity",
            );
        }
    }
}
