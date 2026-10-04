use crate::ecs::{Entity, World};
use crate::input::event::BusinessCallback;
use crate::input::event::gesture::GestureEvent;
use crate::render::command::DrawCommand;
use crate::ui::property::{PropertyChange, ValueUpdate, invalidate_for_change};
use crate::ui::theme::{ColorToken, ThemedColor};

crate::input::event::contract::widget_events! {
    Checkbox;
    #[derive(Clone, Debug)]
    pub enum CheckboxEvent {
        Toggled { now: bool },
    }
}

pub struct CheckboxHandler {
    pub on_event: BusinessCallback<CheckboxEvent>,
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

    pub(crate) fn update_checked(&mut self, checked: bool) -> ValueUpdate<bool> {
        let old = self.checked;
        let change = if old == checked {
            PropertyChange::Unchanged
        } else {
            self.checked = checked;
            PropertyChange::Visual
        };
        ValueUpdate {
            old,
            new: checked,
            change,
        }
    }

    pub fn build() -> CheckboxBuilder {
        CheckboxBuilder {
            checkbox: Checkbox::new(),
            style: None,
            handler: None,
        }
    }
}

pub struct CheckboxBuilder {
    checkbox: Checkbox,
    style: Option<crate::ui::Style>,
    handler: Option<CheckboxHandler>,
}

impl CheckboxBuilder {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn on_change(mut self, on_event: fn(&mut World, Entity, &CheckboxEvent) -> bool) -> Self {
        self.handler = Some(CheckboxHandler {
            on_event: BusinessCallback::Fn(on_event),
        });
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

    pub fn spawn(self, world: &mut World) -> Entity {
        world.spawn(self)
    }
}

impl crate::ecs::IntoBundle for CheckboxBuilder {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.checkbox);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        if let Some(handler) = self.handler {
            world.insert(entity, handler);
        }
    }
}

#[crate::view(component = Checkbox, name = "Checkbox", priority = 40, gesture = checkbox_handler)]
fn checkbox_render(cb: component!(), theme: res!(crate::ui::Theme), paint: paint!()) {
    let state = paint.state();
    let color = if cb.checked {
        cb.checked_color.resolve_in(theme, state)
    } else {
        cb.unchecked_color.resolve_in(theme, state)
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

pub(crate) fn checkbox_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if let GestureEvent::Tap { .. } = event {
        let update = if let Some(cb) = world.get_mut::<Checkbox>(entity) {
            cb.update_checked(!cb.checked)
        } else {
            return false;
        };
        if update.old != update.new {
            invalidate_for_change(world, entity, update.change);
            emit_checkbox_event(world, entity, &CheckboxEvent::Toggled { now: update.new });
        }
        return true;
    }
    false
}

fn emit_checkbox_event(world: &mut World, entity: Entity, event: &CheckboxEvent) {
    let cb = world
        .get::<CheckboxHandler>(entity)
        .map(|h| h.on_event.clone_out());
    if let Some(cb) = cb {
        cb.call(world, entity, event);
    }
}

pub use checkbox_render::view;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dirty::VisualDirty;

    fn h(_: &mut World, _: Entity, _: &CheckboxEvent) -> bool {
        true
    }

    #[test]
    fn build_spawns_checkbox_with_style_and_handler() {
        let mut world = World::new();
        let e = Checkbox::build()
            .style(crate::ui::Style::default())
            .on_change(h)
            .spawn(&mut world);
        assert!(world.has::<Checkbox>(e));
        assert!(world.has::<crate::ui::Style>(e));
        assert!(world.has::<CheckboxHandler>(e));
        assert!(world.has::<crate::ui::Widget>(e));
    }

    #[test]
    fn build_without_handler_omits_it() {
        let mut world = World::new();
        let e = Checkbox::build().spawn(&mut world);
        assert!(world.has::<Checkbox>(e));
        assert!(!world.has::<CheckboxHandler>(e));
        assert!(!world.has::<crate::ui::Style>(e));
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
        assert!(!update.old);
        assert!(update.new);
        assert_eq!(update.change, PropertyChange::Visual);
        assert_eq!(
            world
                .get_mut::<Checkbox>(entity)
                .unwrap()
                .update_checked(true)
                .change,
            PropertyChange::Unchanged
        );
        assert!(checkbox_handler(
            &mut world,
            entity,
            &GestureEvent::Tap {
                x: crate::types::Fixed::ZERO,
                y: crate::types::Fixed::ZERO,
                target: entity,
            },
        ));
        assert!(!world.get::<Checkbox>(entity).unwrap().checked);
        assert!(world.has::<VisualDirty>(entity));
    }

    #[test]
    fn tap_callback_can_remove_checkbox_after_invalidation() {
        fn remove_checkbox(world: &mut World, entity: Entity, _: &CheckboxEvent) -> bool {
            assert!(world.has::<VisualDirty>(entity));
            world.despawn(entity);
            true
        }

        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Checkbox::new());
        world.insert(
            entity,
            CheckboxHandler {
                on_event: BusinessCallback::Fn(remove_checkbox),
            },
        );
        assert!(checkbox_handler(
            &mut world,
            entity,
            &GestureEvent::Tap {
                x: crate::types::Fixed::ZERO,
                y: crate::types::Fixed::ZERO,
                target: entity,
            },
        ));
        assert!(!world.is_alive(entity));
    }
}
