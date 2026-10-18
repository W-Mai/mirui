pub(crate) mod binding;
pub(crate) mod callback;
pub(crate) mod capacity;
pub mod contract;
pub(crate) mod emission;
pub mod focus;
pub mod gesture;
pub mod hit_test;
pub mod hover;
pub mod input;
pub mod layout;
pub mod lifecycle;
pub(crate) mod listeners;
pub mod multi_tap;
pub(crate) mod notification;
pub(crate) mod physical_key;
pub mod press;
pub(crate) mod request;
pub(crate) mod runtime;
pub mod scroll;
pub mod sim;
pub(crate) mod view_attach;
pub(crate) mod view_input;

pub use capacity::EventCapacity;

use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::ui::{Parent, UserState};

use gesture::GestureSystem;
use hit_test::hit_test;
use input::{InputEvent, PointerKind};
use scroll::{ScrollDragState, scroll_system_with_target};

#[derive(Clone, Copy)]
pub struct PointerCursor {
    pub kind: PointerKind,
    pub id: u8,
    /// Whether the cursor has a current position inside the surface.
    pub present: bool,
    pub(crate) hover_capable: bool,
    pub x: Fixed,
    pub y: Fixed,
    pub down: bool,
    /// Interaction sequence used by cursor feedback.
    pub event_seq: u32,
}

impl Default for PointerCursor {
    fn default() -> Self {
        Self {
            kind: PointerKind::Unknown,
            id: 0,
            present: false,
            hover_capable: false,
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            down: false,
            event_seq: 0,
        }
    }
}

impl PointerCursor {
    fn update(&mut self, event: &InputEvent, width: u16, height: u16) {
        let Some((kind, id)) = event.pointer_identity() else {
            return;
        };
        let same = (self.kind, self.id) == (kind, id);
        if self.down && !same {
            return;
        }
        let inside = |x: Fixed, y: Fixed| {
            x >= Fixed::ZERO
                && y >= Fixed::ZERO
                && x < Fixed::from_int(width as i32)
                && y < Fixed::from_int(height as i32)
        };
        match event {
            InputEvent::PointerEnter { .. } => {
                self.kind = kind;
                self.id = id;
                self.present = false;
                self.hover_capable = matches!(kind, PointerKind::Mouse | PointerKind::Pen);
            }
            InputEvent::PointerLeave { .. } if same => self.present = false,
            InputEvent::PointerDown { x, y, .. } | InputEvent::PointerMove { x, y, .. } => {
                if !same {
                    self.hover_capable = false;
                }
                self.kind = kind;
                self.id = id;
                self.x = *x;
                self.y = *y;
                self.present = inside(*x, *y);
                self.hover_capable |= kind == PointerKind::Mouse
                    || (kind == PointerKind::Pen
                        && !self.down
                        && matches!(event, InputEvent::PointerMove { .. }));
                if matches!(event, InputEvent::PointerDown { .. }) && !self.down {
                    self.down = true;
                    self.event_seq = self.event_seq.wrapping_add(1);
                }
            }
            InputEvent::PointerUp { x, y, .. } if same && self.down => {
                self.x = *x;
                self.y = *y;
                self.present &= inside(*x, *y) && self.hover_capable;
                self.down = false;
                self.event_seq = self.event_seq.wrapping_add(1);
            }
            InputEvent::PointerCancel { x, y, .. } if same => {
                self.x = *x;
                self.y = *y;
                self.present = false;
                self.hover_capable = false;
                if self.down {
                    self.event_seq = self.event_seq.wrapping_add(1);
                }
                self.down = false;
            }
            _ => {}
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

pub(crate) fn deliver_input(
    world: &mut World,
    root: Option<Entity>,
    event: &InputEvent,
    now_ms: u32,
    width: u16,
    height: u16,
) {
    crate::ui::lifecycle::prepare(world);
    if matches!(
        event,
        InputEvent::PointerDown { .. }
            | InputEvent::PointerMove { .. }
            | InputEvent::PointerUp { .. }
            | InputEvent::Wheel { .. }
    ) && let Some(root) = root
    {
        if hit_test::preparation_blocked(world) {
            cancel_active_input(world, now_ms, press::PressCancelReason::Unavailable);
            dispatch_pending_gestures(world, now_ms);
            return;
        }
        let viewport = world
            .resource::<crate::app::RenderViewport>()
            .map(|viewport| viewport.0)
            .unwrap_or_else(|| crate::types::Viewport::new(width, height, Fixed::ONE));
        if let Err(error) = crate::ui::render_system::prepare_input_geometry(world, root, &viewport)
        {
            crate::warn!("input geometry unavailable: {:?}", error);
            cancel_active_input(world, now_ms, press::PressCancelReason::Unavailable);
            dispatch_pending_gestures(world, now_ms);
            return;
        }
    }
    dispatch_prepared_input(world, root, event, now_ms, width, height);
    if let Err(error) = crate::ui::delivery::flush(world) {
        crate::warn!("input notification flush failed: {:?}", error);
    }
    dispatch_pending_gestures(world, now_ms);
    if let Err(error) = crate::ui::delivery::flush(world) {
        crate::warn!("input notification flush failed: {:?}", error);
    }
    crate::core::reactive::flush_signal_dirty(world);
}

pub(crate) fn dispatch_pending_gestures(world: &mut World, now_ms: u32) {
    if let Err(error) = gesture::system::drain_gestures(world, |world, gesture| {
        crate::ui::delivery::dispatch_gesture(world, gesture, now_ms)?;
        crate::ui::delivery::flush(world)?;
        crate::core::reactive::flush_signal_dirty(world);
        Ok::<(), runtime::RuntimeRunError>(())
    }) {
        crate::warn!("gesture dispatch failed: {:?}", error);
    }
}

/// Feed one raw input through key dispatch, scrolling, and gesture recognition.
/// Recognized gestures remain queued until delivery; App and simulation use a
/// shared delivery boundary before processing their next raw input.
pub fn dispatch_input(
    world: &mut World,
    root: Option<Entity>,
    event: &InputEvent,
    now_ms: u32,
    lw: u16,
    lh: u16,
) {
    crate::ui::lifecycle::prepare(world);
    dispatch_prepared_input(world, root, event, now_ms, lw, lh);
}

fn dispatch_prepared_input(
    world: &mut World,
    root: Option<Entity>,
    event: &InputEvent,
    now_ms: u32,
    lw: u16,
    lh: u16,
) {
    let _owner_scope = crate::core::reactive::OwnerGuard::enter(world);
    if matches!(event, InputEvent::AppSuspend) {
        cancel_active_input(world, now_ms, press::PressCancelReason::Suspended);
        return;
    }
    runtime::initialize(world);
    crate::ui::text_input::flush(world);
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
    match crate::ui::delivery::dispatch_input(world, root, event, lw, lh) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            crate::warn!("input event dispatch failed: {:?}", error);
            return;
        }
    }
    if event.pointer_identity().is_some() {
        update_pointer_cursor(world, |cursor| cursor.update(event, lw, lh));
        runtime::observe_pointer(world, event, lw, lh);
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

    let can_start = world
        .resource::<GestureSystem>()
        .is_some_and(|system| system.recognizer.can_start_press());
    scroll_system_with_target(world, root, event, lw, lh, pointer_down_target, now_ms);

    let scroll_claimed = world
        .resource::<ScrollDragState>()
        .is_some_and(|s| s.active && s.resolved);
    if let Some(gs) = world.resource_mut::<GestureSystem>() {
        gs.recognizer.scroll_claimed = scroll_claimed;
        gs.recognizer
            .update(event, now_ms, pointer_down_target, &mut gs.events);
    }
    let after = world
        .resource::<GestureSystem>()
        .and_then(|system| system.recognizer.single_press());
    runtime::observe_press_input(world, event, can_start, after, scroll_claimed);
}

pub(crate) fn cancel_active_input(
    world: &mut World,
    now_ms: u32,
    reason: press::PressCancelReason,
) {
    runtime::cancel_hover(world);
    runtime::cancel_press(world, reason);
    if let Some(cursor) = world.resource_mut::<PointerCursor>() {
        cursor.down = false;
        cursor.present = false;
        cursor.hover_capable = false;
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
    runtime::observe_press(world);
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
        && let Err(error) = runtime::dispatch_gesture(world, &event)
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

pub(crate) fn entity_or_ancestor_disabled(world: &World, entity: Entity) -> bool {
    let mut cur = Some(entity);
    while let Some(e) = cur {
        if matches!(world.get::<UserState>(e), Some(UserState::Disabled))
            || world.has::<listeners::InputBlocked>(e)
        {
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
    fn count_key(count: res!(mut TapCount)) {
        count.0 += 1;
    }

    #[test]
    fn ordered_delivery_reuses_prepared_callback_and_reactive_storage() {
        use crate::input::event::input::Key;
        let mut world = World::new();
        world.insert_resource(TapCount(0));
        runtime::install_app(&mut world, Key::EVENT, count_key::callback()).unwrap();
        let input = InputEvent::Key {
            physical: None,
            code: 42,
            pressed: true,
            modifiers: Some(Default::default()),
            repeat: Some(false),
        };
        deliver_input(&mut world, None, &input, 0, 8, 8);
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for time in 1..5 {
                    deliver_input(&mut world, None, &input, time, 8, 8);
                }
            }),
            0
        );
        assert_eq!(world.resource::<TapCount>().unwrap().0, 5);
    }

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

    struct RouteMarker;
    impl crate::ecs::Component for RouteMarker {}
    #[derive(Default)]
    struct RouteCalls {
        first: u8,
        second: u8,
        user: u8,
    }
    #[crate::event]
    fn route_first(
        node: entity!(),
        _tap: event!(gesture::Tap),
        calls: res!(mut RouteCalls),
        commands: commands!(),
    ) -> bool {
        commands.remove::<RouteMarker>(node);
        calls.first += 1;
        false
    }
    #[crate::event]
    fn route_second(_tap: event!(gesture::Tap), calls: res!(mut RouteCalls)) -> bool {
        calls.second += 1;
        true
    }

    #[test]
    fn internal_handlers_keep_registration_order_and_skip_removed_components() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, RouteMarker);
        world.insert_resource(RouteCalls::default());
        // SAFETY: this fixture fetches only RouteCalls and captures no Model.
        let user = unsafe {
            callback::event_callback::<
                gesture::Tap,
                callback::NodeContext,
                (crate::__macro_support::ResMut<RouteCalls>,),
                bool,
            >(
                "user",
                const {
                    &[crate::__macro_support::SystemAccess::write_resource::<
                        RouteCalls,
                    >()]
                },
                |(mut calls,), _, _, _scope| {
                    calls.user += 1;
                    true
                },
            )
        };
        binding::install_entity_event(&mut world, entity, gesture::Tap::EVENT, user);
        let mut registry = crate::ui::view::ViewRegistry::default();
        registry.insert(
            crate::ui::view::View::systems_only("first", &[])
                .with_filter::<RouteMarker>()
                .with_inputs(&mut world, route_first::callback()),
        );
        registry.insert(
            crate::ui::view::View::systems_only("second", &[])
                .with_filter::<RouteMarker>()
                .with_inputs(&mut world, route_second::callback()),
        );
        world.insert_resource(registry);
        crate::ui::lifecycle::attach_handlers_for(&mut world, entity);
        crate::ui::delivery::dispatch_gesture(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: entity,
            },
            0,
        )
        .unwrap();
        let calls = world.resource::<RouteCalls>().unwrap();
        assert_eq!((calls.first, calls.second, calls.user), (1, 0, 1));
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

        crate::ui::delivery::dispatch_gesture(&mut world, &tap, 0).unwrap();
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
        crate::ui::delivery::dispatch_gesture(&mut world, &tap, 0).unwrap();
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
        crate::ui::delivery::dispatch_gesture(&mut world, &tap, 0).unwrap();
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
        crate::ui::delivery::dispatch_gesture(
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
            crate::ui::delivery::dispatch_gesture(
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
            kind: crate::surface::PointerKind::Mouse,
            id: 0,
            present: true,
            hover_capable: true,
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            down: true,
            event_seq: 7,
        });

        dispatch_input(
            &mut world,
            Some(root),
            &InputEvent::PointerCancel {
                kind: crate::input::event::input::PointerKind::Mouse,
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
    fn pointer_boundaries_preserve_capture_and_require_a_current_position() {
        let mut cursor = PointerCursor::default();
        let kind = PointerKind::Mouse;
        let down = InputEvent::PointerDown {
            kind,
            id: 0,
            x: 12.into(),
            y: 18.into(),
        };
        cursor.update(&down, 64, 64);
        assert!(cursor.present && cursor.down);
        let sequence = cursor.event_seq;
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                cursor.update(&InputEvent::PointerLeave { kind, id: 0 }, 64, 64);
                assert!(!cursor.present && cursor.down);
                assert_eq!((cursor.x, cursor.y), (12.into(), 18.into()));
                cursor.update(&InputEvent::PointerEnter { kind, id: 0 }, 64, 64);
                assert!(!cursor.present && cursor.down);
                cursor.update(
                    &InputEvent::PointerMove {
                        kind,
                        id: 0,
                        x: 20.into(),
                        y: 30.into(),
                    },
                    64,
                    64,
                );
                assert!(cursor.present && cursor.down);
                cursor.update(
                    &InputEvent::PointerMove {
                        kind,
                        id: 0,
                        x: 65.into(),
                        y: 30.into(),
                    },
                    64,
                    64,
                );
                assert!(!cursor.present && cursor.down);
            }),
            0
        );
        assert_eq!(cursor.event_seq, sequence);
        cursor.update(
            &InputEvent::PointerUp {
                kind,
                id: 0,
                x: 20.into(),
                y: 30.into(),
            },
            64,
            64,
        );
        assert!(!cursor.present && !cursor.down);
        assert_eq!(cursor.event_seq, sequence + 1);
    }

    #[test]
    fn display_capture_ignores_other_pointers_and_touch_does_not_hover() {
        let mut cursor = PointerCursor::default();
        cursor.update(
            &InputEvent::PointerDown {
                kind: PointerKind::Touch,
                id: 2,
                x: 10.into(),
                y: 12.into(),
            },
            64,
            64,
        );
        for event in [
            InputEvent::PointerLeave {
                kind: PointerKind::Mouse,
                id: 0,
            },
            InputEvent::PointerDown {
                kind: PointerKind::Touch,
                id: 3,
                x: 40.into(),
                y: 40.into(),
            },
            InputEvent::PointerUp {
                kind: PointerKind::Touch,
                id: 3,
                x: 40.into(),
                y: 40.into(),
            },
            InputEvent::PointerCancel {
                kind: PointerKind::Pen,
                id: 2,
                x: 40.into(),
                y: 40.into(),
            },
        ] {
            cursor.update(&event, 64, 64);
        }
        assert!(cursor.present && cursor.down && !cursor.hover_capable);
        assert_eq!(
            (cursor.kind, cursor.id, cursor.x, cursor.y, cursor.event_seq),
            (PointerKind::Touch, 2, 10.into(), 12.into(), 1)
        );
        cursor.update(
            &InputEvent::PointerUp {
                kind: PointerKind::Touch,
                id: 2,
                x: 10.into(),
                y: 12.into(),
            },
            64,
            64,
        );
        assert!(!cursor.present && !cursor.down);
        cursor.update(
            &InputEvent::PointerDown {
                kind: PointerKind::Pen,
                id: 1,
                x: 10.into(),
                y: 12.into(),
            },
            64,
            64,
        );
        assert!(!cursor.hover_capable);
        cursor.update(
            &InputEvent::PointerUp {
                kind: PointerKind::Pen,
                id: 1,
                x: 10.into(),
                y: 12.into(),
            },
            64,
            64,
        );
        assert!(!cursor.present);
        cursor.update(
            &InputEvent::PointerMove {
                kind: PointerKind::Pen,
                id: 1,
                x: 10.into(),
                y: 12.into(),
            },
            64,
            64,
        );
        assert!(cursor.present && cursor.hover_capable);
    }

    #[test]
    fn pointer_cursor_updates_existing_resource_across_events() {
        let mut world = World::new();
        let root = world.spawn_empty();
        world.insert_resource(PointerCursor::default());
        let address = world.resource::<PointerCursor>().unwrap() as *const PointerCursor;

        for event in [
            InputEvent::PointerDown {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(4),
                y: Fixed::from_int(5),
            },
            InputEvent::PointerMove {
                kind: crate::input::event::input::PointerKind::Mouse,
                id: 0,
                x: Fixed::from_int(6),
                y: Fixed::from_int(7),
            },
            InputEvent::PointerUp {
                kind: crate::input::event::input::PointerKind::Mouse,
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

        #[crate::event]
        fn internal_consume(_tap: event!(gesture::Tap)) -> bool {
            INTERNAL_FIRES.fetch_add(1, Ordering::SeqCst);
            true
        }

        #[crate::event]
        fn internal_passthrough(_tap: event!(gesture::Tap)) -> bool {
            INTERNAL_FIRES.fetch_add(1, Ordering::SeqCst);
            false
        }

        #[crate::event]
        fn user_handler() -> bool {
            USER_FIRES.fetch_add(1, Ordering::SeqCst);
            true
        }

        fn registry_with(
            world: &mut World,
            internal: callback::EventCallback<gesture::Tap, callback::NodeContext>,
        ) -> ViewRegistry {
            fn dummy_render(_w: &World, _e: Entity, _paint: &mut crate::ui::view::PaintCtx<'_>) {}
            let mut reg = ViewRegistry::default();
            reg.insert(
                View::new("ChannelMarker", 60, dummy_render)
                    .with_filter::<ChannelMarker>()
                    .with_inputs(world, internal),
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
            crate::ui::delivery::dispatch_gesture(&mut world, &tap_event(e), 0).unwrap();
            assert_eq!(INTERNAL_FIRES.load(Ordering::SeqCst), 0);
            assert_eq!(USER_FIRES.load(Ordering::SeqCst), 1);
        }

        #[test]
        fn internal_only_runs_when_no_user() {
            let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            reset();
            let mut world = World::new();
            let registry = registry_with(&mut world, internal_consume::callback());
            world.insert_resource(registry);
            let e = world.spawn_empty();
            world.insert(e, ChannelMarker);
            crate::ui::lifecycle::attach_handlers_for(&mut world, e);
            crate::ui::delivery::dispatch_gesture(&mut world, &tap_event(e), 0).unwrap();
            assert_eq!(INTERNAL_FIRES.load(Ordering::SeqCst), 1);
            assert_eq!(USER_FIRES.load(Ordering::SeqCst), 0);
        }

        #[test]
        fn internal_consumes_blocks_user() {
            let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            reset();
            let mut world = World::new();
            let registry = registry_with(&mut world, internal_consume::callback());
            world.insert_resource(registry);
            let e = world.spawn_empty();
            world.insert(e, ChannelMarker);
            crate::ui::lifecycle::attach_handlers_for(&mut world, e);
            binding::install_entity_event(
                &mut world,
                e,
                gesture::Tap::EVENT,
                user_handler::callback::<gesture::Tap, callback::NodeContext>(),
            );
            crate::ui::delivery::dispatch_gesture(&mut world, &tap_event(e), 0).unwrap();
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
            let registry = registry_with(&mut world, internal_passthrough::callback());
            world.insert_resource(registry);
            let e = world.spawn_empty();
            world.insert(e, ChannelMarker);
            crate::ui::lifecycle::attach_handlers_for(&mut world, e);
            binding::install_entity_event(
                &mut world,
                e,
                gesture::Tap::EVENT,
                user_handler::callback::<gesture::Tap, callback::NodeContext>(),
            );
            crate::ui::delivery::dispatch_gesture(&mut world, &tap_event(e), 0).unwrap();
            assert_eq!(INTERNAL_FIRES.load(Ordering::SeqCst), 1);
            assert_eq!(
                USER_FIRES.load(Ordering::SeqCst),
                1,
                "internal returned false, user must fire on the same entity",
            );
        }
    }
}
