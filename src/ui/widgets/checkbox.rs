use crate::ecs::{Entity, World};
use crate::input::event::gesture::Tap;
use crate::render::command::DrawCommand;
use crate::ui::property::PropertyChange;
use crate::ui::theme::{ColorToken, ThemedColor};

crate::input::event::contract::widget_events! {
    Checkbox;
    Toggled { now: bool },
}

#[crate::component]
pub struct Checkbox {
    pub checked: bool,
    pub checked_color: ThemedColor,
    pub unchecked_color: ThemedColor,
}

impl Default for Checkbox {
    fn default() -> Self {
        Self {
            checked: false,
            checked_color: ThemedColor::Token(ColorToken::Primary),
            unchecked_color: ThemedColor::Token(ColorToken::SurfaceVariant),
        }
    }
}

impl Checkbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_checked_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.checked_color = color.into();
        self
    }

    pub fn with_unchecked_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.unchecked_color = color.into();
        self
    }

    pub fn toggle(&mut self) {
        self.update_checked(!self.checked);
    }

    pub(crate) fn update_checked(&mut self, checked: bool) -> PropertyChange {
        let old = self.checked;
        if old == checked {
            PropertyChange::Unchanged
        } else {
            self.checked = checked;
            PropertyChange::Visual
        }
    }

    pub fn build() -> CheckboxBuilder {
        CheckboxBuilder {
            checkbox: Checkbox::new(),
            style: None,
            properties: (),
            bindings: (),
        }
    }
}

pub struct CheckboxBuilder<P = (), B = ()> {
    checkbox: Checkbox,
    style: Option<crate::ui::Style>,
    properties: P,
    bindings: B,
}

impl<P, B> CheckboxBuilder<P, B> {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn checked(mut self, v: bool) -> Self {
        self.checkbox.checked = v;
        self
    }

    pub fn checked_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.checkbox.checked_color = color.into();
        self
    }

    pub fn unchecked_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.checkbox.unchecked_color = color.into();
        self
    }
}

crate::ui::recipe::widget_builder_bindings!(CheckboxBuilder, Checkbox, checkbox, style);

impl<P: crate::__macro_support::PropertyBindings, B: crate::__macro_support::EntityEventBindings>
    crate::ui::recipe::WidgetRecipe for CheckboxBuilder<P, B>
{
    type Properties = P;
    type Events = B;

    fn insert(self, world: &mut World, entity: Entity) -> (P, B) {
        world.insert(entity, self.checkbox);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        (self.properties, self.bindings)
    }
}

#[crate::view(component = Checkbox, name = "Checkbox", priority = 40, input = checkbox_tap::callback())]
fn checkbox_render(cb: component!(), theme: res!(crate::ui::Theme), paint: paint!()) {
    let state = paint.state();
    let color = if cb.checked {
        cb.checked_color.resolve_in(&theme, state)
    } else {
        cb.unchecked_color.resolve_in(&theme, state)
    };
    paint.draw(&DrawCommand::Fill {
        area: paint.rect(),
        transform: paint.transform(),
        quad: paint.quad(),
        color,
        radius: paint.style().border_radius,
        opa: 255,
    });
    paint.set_background_handled(true);
}

#[crate::event]
fn checkbox_tap(
    node: entity!(),
    _tap: event!(Tap),
    checkboxes: query!(Checkbox),
    commands: commands!(),
) -> bool {
    let Some(checkbox) = checkboxes.get(node) else {
        return false;
    };
    let checked = !checkbox.checked;
    commands.set::<crate::ui::property::prop::CheckboxChecked>(node, checked);
    commands.emit(node, Toggled { now: checked });
    true
}

pub use checkbox_render::view;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::gesture::GestureEvent;
    use crate::input::event::{EventCapacity, runtime};
    use crate::ui::dirty::VisualDirty;

    #[derive(Default)]
    struct ToggleTrace {
        calls: usize,
        checked: bool,
    }

    #[crate::event]
    fn record_toggle(input: event!(Toggled), trace: res!(mut ToggleTrace)) {
        trace.calls += 1;
        trace.checked = input.now;
    }

    #[crate::event]
    fn remove_checkbox(
        node: entity!(),
        values: query!(Checkbox, VisualDirty),
        commands: commands!(),
    ) {
        assert!(values.get(node).is_some());
        commands.despawn(node);
    }

    #[test]
    fn build_spawns_checkbox_with_style_and_typed_event_binding() {
        let mut world = World::new();
        world.insert_resource(ToggleTrace::default());
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .commands(crate::input::event::view_input::fixture_capacity())
                .notifications::<Toggled>(1)
                .flush_limit(1),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent).spawn(
            Checkbox::build()
                .on_event(Checkbox::Toggled, record_toggle::callback())
                .style(crate::ui::Style::default()),
        );
        assert!(world.has::<crate::ui::Style>(entity));
        assert!(world.has::<crate::ui::Widget>(entity));
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: crate::types::Fixed::ZERO,
                    y: crate::types::Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert_eq!(world.resource::<ToggleTrace>().unwrap().calls, 0);
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 1);
        let trace = world.resource::<ToggleTrace>().unwrap();
        assert_eq!(trace.calls, 1);
        assert!(trace.checked);
    }

    #[test]
    fn bare_builder_does_not_require_notification_storage() {
        let mut world = World::new();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent).spawn(Checkbox::build());
        assert!(world.has::<crate::ui::Style>(entity));
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: crate::types::Fixed::ZERO,
                    y: crate::types::Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert!(world.get::<Checkbox>(entity).unwrap().checked);
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
    }

    #[test]
    fn external_checked_value_and_tap_share_one_update_path() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Checkbox::new());
        let update = world
            .get_mut::<Checkbox>(entity)
            .unwrap()
            .update_checked(true);
        assert!(world.get::<Checkbox>(entity).unwrap().checked);
        assert_eq!(update, PropertyChange::Visual);
        assert_eq!(
            world
                .get_mut::<Checkbox>(entity)
                .unwrap()
                .update_checked(true),
            PropertyChange::Unchanged
        );
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: crate::types::Fixed::ZERO,
                    y: crate::types::Fixed::ZERO,
                    target: entity,
                },
            )
            .unwrap()
        );
        assert!(!world.get::<Checkbox>(entity).unwrap().checked);
        assert!(world.has::<VisualDirty>(entity));
    }

    #[test]
    fn tap_callback_can_remove_checkbox_after_invalidation_and_input() {
        let mut world = World::new();
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .notifications::<Toggled>(1)
                .commands(crate::input::event::view_input::fixture_capacity())
                .flush_limit(1),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent)
            .spawn(Checkbox::build().on_event(Checkbox::Toggled, remove_checkbox::callback()));
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: crate::types::Fixed::ZERO,
                    y: crate::types::Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert!(world.is_alive(entity));
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 1);
        assert!(!world.is_alive(entity));
    }

    #[test]
    fn programmatic_checked_value_does_not_publish_user_toggled_notification() {
        let mut world = World::new();
        world.insert_resource(ToggleTrace::default());
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .commands(crate::input::event::view_input::fixture_capacity())
                .notifications::<Toggled>(1)
                .flush_limit(1),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent)
            .spawn(Checkbox::build().on_event(Checkbox::Toggled, record_toggle::callback()));
        crate::ui::property::apply_to_world::<crate::ui::property::prop::CheckboxChecked>(
            &mut world, entity, true,
        );
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
        assert_eq!(world.resource::<ToggleTrace>().unwrap().calls, 0);
    }
}
