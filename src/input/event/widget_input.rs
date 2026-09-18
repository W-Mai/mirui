use crate::ecs::{Entity, World};

// Backwards-compat re-export; canonical path is components::text_input.
pub use crate::ui::widgets::text_input::{CursorBlinkPhase, cursor_blink_system};

// Snapshot ViewAttach fn pointers so the borrow on ViewRegistry
// drops before each fn gets &mut World. The mut-borrow conflict
// is the reason for the two-step copy instead of streaming.
#[derive(Clone, Copy)]
struct InputAttach {
    component_filter: Option<core::any::TypeId>,
    attach: Option<crate::ui::view::ViewAttach>,
    has_internal_gesture: bool,
}

fn input_attach_plan(world: &World) -> alloc::vec::Vec<InputAttach> {
    let mut pending = alloc::vec::Vec::new();
    if let Some(reg) = world.resource::<crate::ui::view::ViewRegistry>() {
        for view in reg.iter() {
            let attach = view.auto_attach();
            let has_internal_gesture = view.internal_gesture().is_some();
            if attach.is_some() || has_internal_gesture {
                pending.push(InputAttach {
                    component_filter: view.component_filter(),
                    attach,
                    has_internal_gesture,
                });
            }
        }
    }
    pending
}

fn apply_input_attach_plan(world: &mut World, entity: Entity, plan: &[InputAttach]) {
    let mut has_internal_gesture = false;
    for hook in plan {
        if hook
            .component_filter
            .is_some_and(|filter| !world.has_type(entity, filter))
        {
            continue;
        }
        if let Some(attach) = hook.attach {
            attach(world, entity);
        }
        has_internal_gesture |= hook.has_internal_gesture;
    }

    if has_internal_gesture
        || world
            .get::<crate::input::event::GestureHandler>(entity)
            .is_some()
    {
        world.insert(entity, crate::ui::HitTarget);
    }
    if has_internal_gesture {
        world.insert(entity, crate::ui::InteractionFeedback);
    }
}

pub fn attach_handlers_for(world: &mut World, entity: Entity) {
    let plan = input_attach_plan(world);
    apply_input_attach_plan(world, entity, &plan);
}

/// Walk the widget tree from `root` and auto-install gesture/key
/// handlers on built-in widgets that don't already have one. Call once
/// after building the tree.
pub fn attach_widget_input_handlers(world: &mut World, root: Entity) {
    let plan = input_attach_plan(world);
    let mut stack = alloc::vec::Vec::with_capacity(16);
    stack.push(root);
    while let Some(entity) = stack.pop() {
        if let Some(children) = world.get::<crate::ui::Children>(entity) {
            for &child in &children.0 {
                stack.push(child);
            }
        }
        apply_input_attach_plan(world, entity, &plan);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::types::{Fixed, Rect};
    use crate::ui::ComputedRect;
    use crate::ui::view::ViewRegistry;
    use crate::ui::widgets::button::Button;
    use crate::ui::widgets::checkbox::{Checkbox, checkbox_handler};
    use crate::ui::widgets::tabbar::{TabBar, tabbar_handler};
    use crate::ui::widgets::text_input::TextInput;
    use crate::ui::{HitTarget, InteractionFeedback};

    #[test]
    fn tabbar_tap_picks_correct_tab() {
        let mut world = World::default();
        let e = world.spawn_empty();
        world.insert(e, ComputedRect(Rect::new(0, 0, 300, 40)));
        world.insert(e, TabBar::new(3));
        for (x, expected) in [(150, 1u8), (270, 2), (0, 0), (299, 2)] {
            tabbar_handler(
                &mut world,
                e,
                &GestureEvent::Tap {
                    x: Fixed::from_int(x),
                    y: Fixed::from_int(20),
                    target: e,
                },
            );
            let tb = world.get::<TabBar>(e).unwrap();
            assert_eq!(tb.selected, expected, "x={x} → expected {expected}");
        }
    }

    #[test]
    fn button_attach_marks_hit_target() {
        let mut world = World::default();
        let mut registry = ViewRegistry::default();
        registry.insert(crate::ui::widgets::button::view());
        world.insert_resource(registry);
        let entity = world.spawn_empty();
        world.insert(entity, Button::new());

        attach_handlers_for(&mut world, entity);

        assert!(world.has::<HitTarget>(entity));
        assert!(world.has::<InteractionFeedback>(entity));
    }

    #[test]
    fn button_attach_plan_ignores_layout_entities() {
        let mut world = World::default();
        let mut registry = ViewRegistry::default();
        registry.insert(crate::ui::widgets::button::view());
        world.insert_resource(registry);
        let entity = world.spawn_empty();

        attach_handlers_for(&mut world, entity);

        assert!(!world.has::<HitTarget>(entity));
        assert!(!world.has::<InteractionFeedback>(entity));
    }

    #[test]
    fn user_gesture_handler_survives_button_attach() {
        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.insert(crate::ui::widgets::button::view());
        world.insert_resource(reg);

        let e = world.spawn_empty();
        world.insert(e, Button::new());
        fn user_handler(_: &mut World, _: Entity, _: &GestureEvent) -> bool {
            false
        }
        world.insert(e, GestureHandler::from_fn(user_handler));

        attach_handlers_for(&mut world, e);

        assert!(world.has::<HitTarget>(e));
        assert!(world.has::<InteractionFeedback>(e));

        let h = world.get::<GestureHandler>(e).expect("user handler stays");
        let installed = match h.on_gesture {
            crate::input::event::GestureCallback::Fn(f) => f as *const (),
            crate::input::event::GestureCallback::Closure(_) => {
                panic!("expected the user fn handler")
            }
        };
        assert!(
            core::ptr::eq(installed, user_handler as *const ()),
            "button attachment must preserve the user-supplied GestureHandler"
        );
    }

    #[test]
    fn delegated_gesture_target_does_not_gain_control_feedback() {
        let mut world = World::default();
        world.insert_resource(ViewRegistry::default());
        let entity = world.spawn_empty();
        world.insert(entity, GestureHandler::from_fn(|_, _, _| false));

        attach_handlers_for(&mut world, entity);

        assert!(world.has::<HitTarget>(entity));
        assert!(!world.has::<InteractionFeedback>(entity));
    }

    #[test]
    fn text_input_attach_installs_focus_and_key_handler() {
        use crate::input::event::focus::{Focusable, KeyHandler};

        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.insert(crate::ui::widgets::text_input::view());
        world.insert_resource(reg);

        let e = world.spawn_empty();
        world.insert(e, TextInput::new());

        attach_handlers_for(&mut world, e);

        assert!(world.get::<Focusable>(e).is_some());
        assert!(world.get::<KeyHandler>(e).is_some());
        assert!(world.has::<HitTarget>(e));
        assert!(world.has::<InteractionFeedback>(e));
        assert!(
            crate::ui::widgets::text_input::view()
                .internal_gesture()
                .is_some(),
            "TextInput view must expose internal gesture for focus + tap-to-focus"
        );
    }

    #[test]
    fn registry_carries_progress_bar_internal_gesture() {
        let view = crate::ui::widgets::progress_bar::view();
        assert!(
            view.internal_gesture().is_some(),
            "ProgressBar view must expose an internal gesture handler"
        );
    }

    #[test]
    fn built_in_gesture_widgets_become_hit_targets() {
        let mut world = World::default();
        world.insert_resource(ViewRegistry::with_builtins());
        let slider = world.spawn_empty();
        world.insert(
            slider,
            crate::ui::widgets::Slider::new(Fixed::ZERO, Fixed::from_int(100)),
        );
        let switch = world.spawn_empty();
        world.insert(switch, crate::ui::widgets::Switch::new());

        attach_handlers_for(&mut world, slider);
        attach_handlers_for(&mut world, switch);

        assert!(world.has::<HitTarget>(slider));
        assert!(world.has::<HitTarget>(switch));
        assert!(world.has::<InteractionFeedback>(slider));
        assert!(world.has::<InteractionFeedback>(switch));
    }

    #[test]
    fn registry_carries_checkbox_internal_gesture() {
        let view = crate::ui::widgets::checkbox::view();
        assert!(
            view.internal_gesture().is_some(),
            "Checkbox view must expose an internal gesture handler"
        );
    }

    #[test]
    fn checkbox_tap_toggles_checked() {
        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.insert(crate::ui::widgets::checkbox::view());
        world.insert_resource(reg);

        let e = world.spawn_empty();
        world.insert(e, Checkbox::new());
        attach_handlers_for(&mut world, e);

        let consumed = checkbox_handler(
            &mut world,
            e,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: e,
            },
        );
        assert!(consumed);
        assert!(world.get::<Checkbox>(e).unwrap().checked);
    }

    #[test]
    fn tabbar_ignores_non_tap() {
        let mut world = World::default();
        let e = world.spawn_empty();
        world.insert(e, ComputedRect(Rect::new(0, 0, 300, 40)));
        world.insert(e, TabBar::new(3));
        let consumed = tabbar_handler(
            &mut world,
            e,
            &GestureEvent::DragMove {
                x: Fixed::from_int(50),
                y: Fixed::from_int(20),
                dx: Fixed::ZERO,
                dy: Fixed::ZERO,
                target: e,
            },
        );
        assert!(!consumed);
        assert_eq!(world.get::<TabBar>(e).unwrap().selected, 0);
    }
}
