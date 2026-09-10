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
        if let Some(component) = w.get_mut::<crate::ui::widgets::text::Text>(entity) {
            component.set_content(text);
        } else {
            w.insert(entity, crate::ui::widgets::text::Text::from(text));
        }
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

pub fn reactive_set_font_size(entity: Entity, value: u16) {
    crate::core::reactive::with_world(|w| {
        if let Some(style) = w.get_mut::<Style>(entity) {
            style.set_font_size(value);
        }
        w.insert(entity, Dirty);
    });
}

pub fn reactive_set_paragraph(entity: Entity, value: crate::ui::widgets::text::ParagraphStyle) {
    crate::core::reactive::with_world(|w| {
        if let Some(text) = w.get_mut::<crate::ui::widgets::text::Text>(entity) {
            text.set_paragraph(value);
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
        use crate::ui::widgets::text::{ParagraphStyle, Text, TextAlign, TextVerticalAlign};

        let mut world = World::new();
        let e = WidgetBuilder::new(&mut world).id();
        let paragraph = ParagraphStyle {
            align: TextAlign::Center,
            vertical_align: TextVerticalAlign::Center,
            ..ParagraphStyle::default()
        };
        world.insert(e, Text::from("before").with_paragraph(paragraph.clone()));
        with_world_scope(&mut world, || reactive_set_text(e, 42i32));
        let text = world.get::<Text>(e).unwrap();
        assert_eq!(text.resolve(&world), "42");
        assert_eq!(text.paragraph(), &paragraph);
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
    fn set_font_size_updates_typography() {
        let mut world = World::new();
        let e = WidgetBuilder::new(&mut world).id();
        with_world_scope(&mut world, || reactive_set_font_size(e, 18));
        let style = world.get::<Style>(e).unwrap();
        assert_eq!(style.font_size, Some(18));
        assert!(world.get::<Dirty>(e).is_some());
    }

    #[test]
    fn set_paragraph_updates_text_layout_input() {
        use crate::ui::widgets::text::{ParagraphStyle, Text, TextAlign, TextWrap};

        let mut world = World::new();
        let e = WidgetBuilder::new(&mut world).id();
        world.insert(e, Text::from("responsive text"));
        let paragraph = ParagraphStyle {
            wrap: TextWrap::Grapheme,
            align: TextAlign::Center,
            max_lines: Some(2),
            ..ParagraphStyle::default()
        };
        with_world_scope(&mut world, || reactive_set_paragraph(e, paragraph.clone()));

        assert_eq!(world.get::<Text>(e).unwrap().paragraph(), &paragraph);
        assert!(world.get::<Dirty>(e).is_some());
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
