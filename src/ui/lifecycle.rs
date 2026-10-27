use crate::ecs::{Entity, World};
use crate::input::event::{view_attach, view_input};
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

#[derive(Clone, Copy)]
struct AttachedInstance {
    entity: Entity,
    generation: u64,
    ready: bool,
}

#[crate::component]
struct AttachTarget;

#[derive(Default)]
struct ActiveAttachments(Rc<RefCell<Vec<Entity>>>);

struct AttachmentGuard {
    active: Rc<RefCell<Vec<Entity>>>,
    entity: Entity,
}

impl AttachmentGuard {
    fn enter(world: &mut World, entity: Entity) -> Option<Self> {
        if world.resource::<ActiveAttachments>().is_none() {
            world.insert_resource(ActiveAttachments::default());
        }
        let active = world.resource::<ActiveAttachments>().unwrap().0.clone();
        if active.borrow().contains(&entity) {
            return None;
        }
        active.borrow_mut().push(entity);
        Some(Self { active, entity })
    }
}

impl Drop for AttachmentGuard {
    fn drop(&mut self) {
        let mut active = self.active.borrow_mut();
        if let Some(index) = active.iter().position(|entity| *entity == self.entity) {
            active.remove(index);
        }
    }
}

#[derive(Default)]
struct AttachedInstances {
    values: RefCell<Vec<AttachedInstance>>,
    tracking_ready: Cell<bool>,
}

#[derive(Clone)]
pub(crate) struct InputAttach {
    registration: u64,
    component_filter: Option<core::any::TypeId>,
    attach: Option<view_attach::Attachment>,
    inputs: Option<view_input::SharedInputs>,
    reconcile: Option<fn(&mut World, Entity) -> bool>,
    retire: Option<fn(&mut World, Entity)>,
    instances: Rc<AttachedInstances>,
}

impl InputAttach {
    pub(crate) fn inputs(&self) -> Option<(&Option<core::any::TypeId>, &view_input::SharedInputs)> {
        self.inputs
            .as_ref()
            .map(|inputs| (&self.component_filter, inputs))
    }

    pub(crate) fn for_view(view: &crate::ui::view::View) -> Option<Self> {
        let attach = view.auto_attach().cloned();
        (attach.is_some()
            || view.inputs().is_some()
            || view.reconcile().is_some()
            || view.retire().is_some())
        .then(|| Self {
            registration: view.registration_id(),
            component_filter: view.component_filter(),
            attach,
            inputs: view.inputs().cloned(),
            reconcile: view.reconcile(),
            retire: view.retire(),
            instances: Rc::new(AttachedInstances::default()),
        })
    }

    fn generation(&self, world: &World, entity: Entity) -> Option<u64> {
        if !world.is_alive(entity) {
            return None;
        }
        self.component_filter
            .map_or(Some(0), |kind| world.component_generation(entity, kind))
    }

    fn retire_replaced(&self, world: &mut World, entity: Entity) {
        let generation = self.generation(world, entity);
        let live_slot = world.is_alive(entity);
        let removed = {
            let mut instances = self.instances.values.borrow_mut();
            instances
                .iter()
                .position(|instance| {
                    (instance.entity == entity && Some(instance.generation) != generation)
                        || (live_slot
                            && instance.entity.id == entity.id
                            && instance.entity != entity)
                })
                .map(|index| instances.swap_remove(index).entity)
        };
        if let Some(removed) = removed
            && let Some(retire) = self.retire
        {
            retire(world, removed);
        }
    }

    fn begin(&self, entity: Entity, generation: u64) -> bool {
        let mut instances = self.instances.values.borrow_mut();
        if let Some(instance) = instances
            .iter_mut()
            .find(|instance| instance.entity == entity)
        {
            if instance.generation == generation {
                return false;
            }
            *instance = AttachedInstance {
                entity,
                generation,
                ready: false,
            };
        } else {
            let instance = AttachedInstance {
                entity,
                generation,
                ready: false,
            };
            instances.push(instance);
        }
        true
    }

    fn finish(&self, entity: Entity, generation: u64, ready: bool) {
        if let Some(instance) = self
            .instances
            .values
            .borrow_mut()
            .iter_mut()
            .find(|instance| instance.entity == entity && instance.generation == generation)
        {
            instance.ready = ready;
        }
    }

    fn ready(&self, world: &World, entity: Entity) -> bool {
        self.generation(world, entity).is_some_and(|generation| {
            self.instances.values.borrow().iter().any(|instance| {
                instance.entity == entity && instance.generation == generation && instance.ready
            })
        })
    }
}

fn input_attach_plan(world: &World) -> Option<alloc::rc::Rc<alloc::vec::Vec<InputAttach>>> {
    world
        .resource::<crate::ui::view::ViewRegistry>()?
        .input_attach_plan()
}

fn prepare_plan(world: &mut World, plan: &[InputAttach]) {
    if plan
        .last()
        .is_none_or(|hook| hook.instances.tracking_ready.get())
    {
        return;
    }
    for hook in plan {
        if hook.instances.tracking_ready.replace(true) {
            continue;
        }
        if let Some(kind) = hook.component_filter {
            world.watch_component_lifetime(kind);
        }
        world.queue_existing_attachments(
            hook.component_filter
                .unwrap_or(core::any::TypeId::of::<AttachTarget>()),
        );
    }
}

fn apply_input_attach_plan(world: &mut World, entity: Entity, plan: &[InputAttach]) {
    prepare_plan(world, plan);
    let mut remaining = 1024_usize;
    loop {
        let mut progress = false;
        for hook in plan {
            hook.retire_replaced(world, entity);
            let Some(generation) = hook.generation(world, entity) else {
                continue;
            };
            if !hook.begin(entity, generation) {
                continue;
            }
            if remaining == 0 {
                crate::warn!("View initialization stopped after repeated component replacement");
                return;
            }
            remaining -= 1;
            progress = true;
            let mut ready = hook
                .attach
                .as_ref()
                .is_none_or(|attach| attach.invoke(world, entity));
            if ready && hook.generation(world, entity) == Some(generation) {
                if let Some(inputs) = &hook.inputs {
                    view_input::prepare(inputs, world);
                    if let view_input::InputState::Ready(prepared) = &*inputs.borrow() {
                        if prepared.hooks.iter().any(|hook| hook.gesture) {
                            world.insert(entity, crate::ui::HitTarget);
                            world.insert(entity, crate::ui::InteractionFeedback);
                        }
                    } else {
                        ready = false;
                    }
                }
                hook.finish(entity, generation, ready);
            }
        }
        if !progress {
            return;
        }
    }
}

pub(crate) fn attach_handlers_for(world: &mut World, entity: Entity) {
    if !world.is_alive(entity) {
        return;
    }
    let Some(_guard) = AttachmentGuard::enter(world, entity) else {
        return;
    };
    if !world.has::<AttachTarget>(entity) {
        world.insert(entity, AttachTarget);
    }
    if let Some(plan) = input_attach_plan(world) {
        apply_input_attach_plan(world, entity, &plan);
    }
}

pub(crate) fn prepare(world: &mut World) {
    for _ in 0..1024 {
        crate::core::reactive::flush_signal_dirty(world);
        let plan = input_attach_plan(world);
        if let Some(plan) = &plan {
            prepare_plan(world, plan);
        }
        let pending = world.pending_attachment_changes();
        let plan = if pending == 0 {
            plan
        } else {
            drop(plan);
            for _ in 0..pending {
                let Some(entity) = world.next_attachment_change() else {
                    break;
                };
                if world.has::<AttachTarget>(entity) {
                    attach_handlers_for(world, entity);
                } else if let Some(plan) = input_attach_plan(world) {
                    for hook in plan.iter() {
                        hook.retire_replaced(world, entity);
                    }
                }
            }
            input_attach_plan(world)
        };
        let mut changed = false;
        if let Some(plan) = plan {
            for hook in plan.iter() {
                let Some(reconcile) = hook.reconcile else {
                    continue;
                };
                let mut index = 0;
                loop {
                    let instance = hook.instances.values.borrow().get(index).copied();
                    let Some(instance) = instance else {
                        break;
                    };
                    if hook.generation(world, instance.entity) != Some(instance.generation) {
                        hook.retire_replaced(world, instance.entity);
                        continue;
                    }
                    index += 1;
                    if !instance.ready {
                        continue;
                    }
                    let Some(_guard) = AttachmentGuard::enter(world, instance.entity) else {
                        continue;
                    };
                    changed |= reconcile(world, instance.entity);
                }
            }
        }
        if !changed && pending == 0 && world.pending_attachment_changes() == 0 {
            return;
        }
    }
    crate::warn!("View initialization stopped after repeated structural changes");
    while world.next_attachment_change().is_some() {}
}

pub(crate) struct ViewInputGuard {
    registration: u64,
    generation: u64,
    instances: Rc<AttachedInstances>,
}

impl ViewInputGuard {
    pub(crate) fn valid(&self, world: &World, entity: Entity) -> bool {
        input_attach_plan(world).is_some_and(|plan| {
            plan.iter()
                .find(|hook| hook.registration == self.registration)
                .is_some_and(|hook| {
                    Rc::ptr_eq(&hook.instances, &self.instances)
                        && hook.generation(world, entity) == Some(self.generation)
                        && hook.ready(world, entity)
                })
        })
    }
}

pub(crate) fn view_input_guard(
    world: &World,
    entity: Entity,
    registration: u64,
) -> Option<ViewInputGuard> {
    let plan = input_attach_plan(world)?;
    let hook = plan.iter().find(|hook| hook.registration == registration)?;
    hook.ready(world, entity).then(|| ViewInputGuard {
        registration,
        generation: hook.generation(world, entity).unwrap(),
        instances: hook.instances.clone(),
    })
}

#[cfg(test)]
#[path = "lifecycle/tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::contract::Event;
    use crate::input::event::gesture::GestureEvent;
    use crate::input::event::gesture::Tap;
    use crate::types::{Fixed, Rect};
    use crate::ui::ComputedRect;
    use crate::ui::view::ViewRegistry;
    use crate::ui::widgets::button::Button;
    use crate::ui::widgets::checkbox::Checkbox;
    use crate::ui::widgets::tabbar::TabBar;
    use crate::ui::widgets::text_input::TextInput;
    use crate::ui::{HitTarget, InteractionFeedback};

    #[derive(Default)]
    struct UserCalls(usize);

    #[crate::component]
    struct LifecycleOwner;

    #[derive(Default)]
    struct RetirementLog(Vec<Entity>);

    fn record_retirement(world: &mut World, entity: Entity) {
        world
            .resource_mut::<RetirementLog>()
            .unwrap()
            .0
            .push(entity);
    }

    #[test]
    fn registered_retirement_precedes_replacement_and_does_not_touch_reused_entities() {
        let mut world = World::default();
        world.insert_resource(RetirementLog::default());
        let mut registry = ViewRegistry::default();
        registry.insert(
            crate::ui::View::systems_only("LifecycleOwner", &[])
                .with_filter::<LifecycleOwner>()
                .with_retire(record_retirement),
        );
        world.insert_resource(registry);
        let entity = world.spawn_empty();
        world.insert(entity, LifecycleOwner);
        attach_handlers_for(&mut world, entity);
        prepare(&mut world);
        world.insert(entity, LifecycleOwner);
        prepare(&mut world);
        assert_eq!(world.resource::<RetirementLog>().unwrap().0, [entity]);
        world.remove::<LifecycleOwner>(entity);
        prepare(&mut world);
        prepare(&mut world);
        assert_eq!(
            world.resource::<RetirementLog>().unwrap().0,
            [entity, entity]
        );
        world.insert(entity, LifecycleOwner);
        prepare(&mut world);
        world.despawn(entity);
        let replacement = world.spawn_empty();
        assert_eq!(replacement.id, entity.id);
        assert_ne!(replacement, entity);
        world.insert(replacement, LifecycleOwner);
        attach_handlers_for(&mut world, replacement);
        prepare(&mut world);
        assert_eq!(
            world.resource::<RetirementLog>().unwrap().0,
            [entity, entity, entity]
        );
        let plan = input_attach_plan(&world).unwrap();
        assert!(plan[0].ready(&world, replacement));
    }

    #[crate::event]
    fn user_handler(calls: res!(mut UserCalls)) -> bool {
        calls.0 += 1;
        false
    }

    #[test]
    fn tabbar_tap_picks_correct_tab() {
        let mut world = World::default();
        let e = world.spawn_empty();
        world.insert(e, ComputedRect(Rect::new(0, 0, 300, 40)));
        world.insert(e, TabBar::new(3));
        for (x, expected) in [(150, 1u8), (270, 2), (0, 0), (299, 2)] {
            crate::input::event::view_input::invoke_gesture(
                crate::ui::widgets::tabbar::view(),
                &mut world,
                e,
                &GestureEvent::Tap {
                    x: Fixed::from_int(x),
                    y: Fixed::from_int(20),
                    target: e,
                },
            )
            .unwrap();
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
        world.insert_resource(UserCalls::default());
        crate::input::event::binding::install_entity_event(
            &mut world,
            e,
            Tap::EVENT,
            user_handler::callback::<Tap, crate::input::event::callback::NodeContext>(),
        );

        attach_handlers_for(&mut world, e);

        assert!(world.has::<HitTarget>(e));
        assert!(world.has::<InteractionFeedback>(e));

        assert!(crate::input::event::runtime::has_entity_listener::<Tap>(&world, e).unwrap());
        crate::ui::delivery::dispatch_gesture(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: e,
            },
            0,
        )
        .unwrap();
        assert_eq!(world.resource::<UserCalls>().unwrap().0, 1);
    }

    #[test]
    fn delegated_gesture_target_does_not_gain_control_feedback() {
        let mut world = World::default();
        world.insert_resource(ViewRegistry::default());
        let entity = world.spawn_empty();
        world.insert_resource(UserCalls::default());
        crate::input::event::binding::install_entity_event(
            &mut world,
            entity,
            Tap::EVENT,
            user_handler::callback::<Tap, crate::input::event::callback::NodeContext>(),
        );

        attach_handlers_for(&mut world, entity);

        assert!(world.has::<HitTarget>(entity));
        assert!(!world.has::<InteractionFeedback>(entity));
    }

    #[test]
    fn text_input_attach_installs_focus_and_typed_inputs() {
        use crate::input::event::focus::Focusable;
        use crate::ui::widgets::TextEditing;

        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.insert(crate::ui::widgets::text_input::view());
        reg.insert(crate::ui::widgets::text_editing::view());
        world.insert_resource(reg);

        let e = world.spawn_empty();
        world.insert(e, TextInput::new());

        attach_handlers_for(&mut world, e);

        assert!(world.get::<Focusable>(e).is_some());
        assert!(world.get::<TextEditing>(e).is_some());
        assert!(world.has::<HitTarget>(e));
        assert!(world.has::<InteractionFeedback>(e));
        assert!(
            crate::ui::widgets::text_editing::view().inputs().is_some(),
            "TextEditing owns the typed input callbacks"
        );
    }

    #[test]
    fn registry_carries_progress_bar_internal_gesture() {
        let view = crate::ui::widgets::progress_bar::view();
        assert!(
            view.inputs().is_some(),
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
            view.inputs().is_some(),
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

        let consumed = crate::input::event::view_input::invoke_gesture(
            crate::ui::widgets::checkbox::view(),
            &mut world,
            e,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: e,
            },
        );
        assert_eq!(consumed, Ok(true));
        assert!(world.get::<Checkbox>(e).unwrap().checked);
    }

    #[test]
    fn tabbar_ignores_non_tap() {
        let mut world = World::default();
        let e = world.spawn_empty();
        world.insert(e, ComputedRect(Rect::new(0, 0, 300, 40)));
        world.insert(e, TabBar::new(3));
        let consumed = crate::input::event::view_input::invoke_gesture(
            crate::ui::widgets::tabbar::view(),
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
        assert_eq!(consumed, Ok(false));
        assert_eq!(world.get::<TabBar>(e).unwrap().selected, 0);
    }

    #[test]
    fn registered_attach_plan_is_allocation_free_and_frozen_during_invocation() {
        use crate::test_support::tracked_allocations;
        use crate::ui::view::{View, ViewRegistry};

        #[derive(Default)]
        struct Calls {
            first: u32,
            second: u32,
            third: u32,
            append: bool,
            replace: bool,
        }
        fn first(scope: &mut crate::ui::UiScope<'_>) {
            let world = scope.world_mut();
            let calls = world.resource_mut::<Calls>().unwrap();
            calls.first += 1;
            if calls.replace {
                world.insert_resource(ViewRegistry::default());
            } else if calls.append {
                calls.append = false;
                world
                    .resource_mut::<ViewRegistry>()
                    .unwrap()
                    .insert(View::systems_only("third", &[]).with_attach(third));
            }
        }
        fn second(scope: &mut crate::ui::UiScope<'_>) {
            let world = scope.world_mut();
            world.resource_mut::<Calls>().unwrap().second += 1;
        }
        fn third(scope: &mut crate::ui::UiScope<'_>) {
            let world = scope.world_mut();
            world.resource_mut::<Calls>().unwrap().third += 1;
        }
        let mut world = World::default();
        let node = world.spawn_empty();
        let mut registry = ViewRegistry::default();
        registry.insert(View::systems_only("first", &[]).with_attach(first));
        registry.insert(View::systems_only("second", &[]).with_attach(second));
        world.insert_resource(registry);
        world.insert_resource(Calls {
            append: true,
            ..Calls::default()
        });
        drop(crate::ui::UiScope::new(&mut world, node));
        attach_handlers_for(&mut world, node);
        assert_eq!(world.resource::<Calls>().unwrap().first, 1);
        assert_eq!(world.resource::<Calls>().unwrap().second, 1);
        assert_eq!(world.resource::<Calls>().unwrap().third, 0);
        attach_handlers_for(&mut world, node);
        assert_eq!(world.resource::<Calls>().unwrap().third, 1);
        assert_eq!(
            tracked_allocations(|| attach_handlers_for(&mut world, node)),
            0
        );
        assert_eq!(world.resource::<Calls>().unwrap().first, 1);
        assert_eq!(world.resource::<Calls>().unwrap().second, 1);
        world.resource_mut::<Calls>().unwrap().replace = true;
        let second = world.spawn_empty();
        attach_handlers_for(&mut world, second);
        assert_eq!(world.resource::<Calls>().unwrap().first, 2);
        assert_eq!(world.resource::<Calls>().unwrap().second, 2);
        assert_eq!(world.resource::<Calls>().unwrap().third, 2);
        attach_handlers_for(&mut world, node);
        assert_eq!(world.resource::<Calls>().unwrap().first, 2);
    }
}
