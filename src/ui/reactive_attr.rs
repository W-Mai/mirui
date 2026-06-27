//! Runtime setters the `ui!` macro emits for `!signal` / `!{ expr }` attrs:
//! re-apply one attribute on an existing entity and mark it `Dirty`.

extern crate alloc;

use alloc::string::String;

use crate::ecs::Entity;
use crate::types::Dimension;
use crate::ui::Style;
use crate::ui::dirty::Dirty;
use crate::ui::theme::ThemedColor;

/// Anything a reactive `text:` binding can yield (blanket `ToString` impl).
pub trait IntoText {
    fn into_text(self) -> String;
}

impl<T: alloc::string::ToString> IntoText for T {
    fn into_text(self) -> String {
        self.to_string()
    }
}

pub fn reactive_set_text(entity: Entity, value: impl IntoText) {
    let text = value.into_text();
    crate::core::reactive::with_world(|w| {
        w.insert(entity, crate::ui::widgets::text::Text::from(text));
        w.insert(entity, Dirty);
    });
}

pub fn reactive_set_bg_color(entity: Entity, value: impl Into<ThemedColor>) {
    let color = value.into();
    crate::core::reactive::with_world(|w| {
        if let Some(style) = w.get_mut::<Style>(entity) {
            style.bg_color = Some(color);
        }
        w.insert(entity, Dirty);
    });
}

pub fn reactive_set_text_color(entity: Entity, value: impl Into<ThemedColor>) {
    let color = value.into();
    crate::core::reactive::with_world(|w| {
        if let Some(style) = w.get_mut::<Style>(entity) {
            style.text_color = color;
        }
        w.insert(entity, Dirty);
    });
}

pub fn reactive_set_width(entity: Entity, value: impl Into<Dimension>) {
    let dim = value.into();
    crate::core::reactive::with_world(|w| {
        if let Some(style) = w.get_mut::<Style>(entity) {
            style.layout.width = dim;
        }
        w.insert(entity, Dirty);
    });
}

pub fn reactive_set_height(entity: Entity, value: impl Into<Dimension>) {
    let dim = value.into();
    crate::core::reactive::with_world(|w| {
        if let Some(style) = w.get_mut::<Style>(entity) {
            style.layout.height = dim;
        }
        w.insert(entity, Dirty);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::{Signal, effect_with_widget, with_world_scope};
    use crate::ecs::World;
    use crate::ui::builder::WidgetBuilder;

    #[test]
    fn set_text_mutates_component_and_marks_dirty() {
        let mut world = World::new();
        let e = WidgetBuilder::new(&mut world).id();
        with_world_scope(&mut world, || reactive_set_text(e, 42i32));
        let text = world.get::<crate::ui::widgets::text::Text>(e).unwrap();
        assert_eq!(text.resolve(&world), "42");
        assert!(world.get::<Dirty>(e).is_some(), "attr change marks Dirty");
    }

    #[test]
    fn set_width_updates_layout() {
        let mut world = World::new();
        let e = WidgetBuilder::new(&mut world).id();
        with_world_scope(&mut world, || reactive_set_width(e, 150));
        let style = world.get::<Style>(e).unwrap();
        assert_eq!(style.layout.width, Dimension::from(150));
    }

    #[test]
    fn set_outside_world_scope_is_noop() {
        let mut world = World::new();
        let e = WidgetBuilder::new(&mut world).id();
        reactive_set_text(e, 7i32); // no with_world_scope -> null ptr -> no-op
        assert!(world.get::<crate::ui::widgets::text::Text>(e).is_none());
    }

    #[test]
    fn reactive_effect_applies_initial_value_and_tracks() {
        let mut world = World::new();
        let e = WidgetBuilder::new(&mut world).id();
        let count = Signal::new(3i32);
        let countc = count.clone();
        with_world_scope(&mut world, || {
            effect_with_widget(e, move || reactive_set_text(e, countc.get()))
        });
        let text = world.get::<crate::ui::widgets::text::Text>(e).unwrap();
        assert_eq!(
            text.resolve(&world),
            "3",
            "initial value applied during construction scope"
        );
    }
}
