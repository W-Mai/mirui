use crate::ecs::{Entity, World};

extern crate alloc;

pub trait IntoText {
    fn into_text(self) -> alloc::string::String;
}

impl<T: alloc::string::ToString> IntoText for T {
    fn into_text(self) -> alloc::string::String {
        self.to_string()
    }
}

pub trait Property: 'static {
    type Value;

    /// Applies the value and reports whether the property changed layout or visual state.
    fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool;
}

pub struct WidgetMut<'w> {
    world: &'w mut World,
    entity: Entity,
}

impl<'w> WidgetMut<'w> {
    pub(crate) const fn new(world: &'w mut World, entity: Entity) -> Self {
        Self { world, entity }
    }

    pub const fn id(&self) -> Entity {
        self.entity
    }

    pub fn set<P: Property>(&mut self, value: P::Value) -> &mut Self {
        let _ = P::apply(self.world, self.entity, value);
        self
    }

    pub fn text_path(&mut self, value: impl Into<crate::text::TextPath>) -> &mut Self {
        self.set::<prop::TextPath>(value.into())
    }
}

impl World {
    pub fn widget_mut(&mut self, entity: Entity) -> Option<WidgetMut<'_>> {
        (self.is_alive(entity) && self.has::<crate::ui::Widget>(entity))
            .then(|| WidgetMut::new(self, entity))
    }
}

#[doc(hidden)]
pub fn apply<P: Property>(entity: Entity, value: P::Value) {
    crate::core::reactive::with_world(|world| {
        if world.is_alive(entity) && world.has::<crate::ui::Widget>(entity) {
            let _ = P::apply(world, entity, value);
            world.invalidate(entity);
        }
    });
}

pub mod prop {
    use super::Property;
    use crate::ecs::{Entity, World};

    pub struct TextPath;
    pub struct TextContent;
    pub struct BackgroundColor;
    pub struct TextColor;
    pub struct ButtonNormalColor;
    pub struct FontSize;
    pub struct Paragraph;
    pub struct Width;
    pub struct MinWidth;
    pub struct MaxWidth;
    pub struct Height;
    pub struct MinHeight;
    pub struct MaxHeight;
    pub struct Padding;
    pub struct RowGap;
    pub struct ColumnGap;
    pub struct Left;
    pub struct Top;

    impl Property for TextContent {
        type Value = alloc::string::String;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool {
            let is_button = world.has::<crate::ui::widgets::Button>(entity);
            if let Some(text) = world.get_mut::<crate::ui::widgets::Text>(entity) {
                text.set_content(value);
            } else if is_button {
                world.insert(entity, crate::ui::widgets::Text::label(value));
            } else {
                world.insert(entity, crate::ui::widgets::Text::from(value));
            }
            world.invalidate(entity);
            true
        }
    }

    macro_rules! style_property {
        ($name:ident, $value:ty, $field:ident) => {
            impl Property for $name {
                type Value = $value;

                fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool {
                    if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                        if style.$field == value {
                            return false;
                        }
                        style.$field = value;
                    } else {
                        return false;
                    }
                    world.invalidate(entity);
                    true
                }
            }
        };
    }

    impl Property for BackgroundColor {
        type Value = crate::ui::theme::ThemedColor;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                if style.bg_color == Some(value) {
                    return false;
                }
                style.bg_color = Some(value);
            } else {
                return false;
            }
            world.invalidate(entity);
            true
        }
    }

    style_property!(TextColor, crate::ui::theme::ThemedColor, text_color);

    impl Property for ButtonNormalColor {
        type Value = crate::ui::theme::ThemedColor;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool {
            if let Some(button) = world.get_mut::<crate::ui::widgets::Button>(entity) {
                if button.normal_color == value {
                    return false;
                }
                button.normal_color = value;
            } else {
                return false;
            }
            world.invalidate(entity);
            true
        }
    }

    impl Property for FontSize {
        type Value = u16;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                if style.font_size == Some(value) {
                    return false;
                }
                style.set_font_size(value);
            } else {
                return false;
            }
            world.invalidate(entity);
            true
        }
    }

    impl Property for Paragraph {
        type Value = crate::ui::widgets::ParagraphStyle;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool {
            let Some(text) = world.get_mut::<crate::ui::widgets::Text>(entity) else {
                return false;
            };
            text.set_paragraph(value);
            world.invalidate(entity);
            true
        }
    }

    impl Property for Width {
        type Value = crate::types::Dimension;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                if style.layout.width == value {
                    return false;
                }
                style.layout.width = value;
            } else {
                return false;
            }
            world.invalidate(entity);
            true
        }
    }

    macro_rules! layout_property {
        ($name:ident, $field:ident) => {
            impl Property for $name {
                type Value = crate::types::Dimension;

                fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool {
                    if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                        if style.layout.$field == value {
                            return false;
                        }
                        style.layout.$field = value;
                    } else {
                        return false;
                    }
                    world.invalidate(entity);
                    true
                }
            }
        };
    }

    layout_property!(MinWidth, min_width);
    layout_property!(MaxWidth, max_width);

    impl Property for Height {
        type Value = crate::types::Dimension;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                if style.layout.height == value {
                    return false;
                }
                style.layout.height = value;
            } else {
                return false;
            }
            world.invalidate(entity);
            true
        }
    }

    layout_property!(MinHeight, min_height);
    layout_property!(MaxHeight, max_height);
    layout_property!(RowGap, row_gap);
    layout_property!(ColumnGap, column_gap);
    layout_property!(Left, left);
    layout_property!(Top, top);

    impl Property for Padding {
        type Value = crate::ui::layout::Padding;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> bool {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                if style.layout.padding == value {
                    return false;
                }
                style.layout.padding = value;
            } else {
                return false;
            }
            world.invalidate(entity);
            true
        }
    }

    impl Property for TextPath {
        type Value = crate::text::TextPath;

        fn apply(world: &mut World, entity: Entity, path: Self::Value) -> bool {
            if let Some(current) = world.get::<crate::text::TextPath>(entity).copied() {
                if current == path {
                    return false;
                }
                if current.path() == path.path()
                    && current.subpath() == path.subpath()
                    && current.direction() == path.direction()
                    && current.seam() == path.seam()
                    && let (Some(current_end), Some(next_end)) = (current.end(), path.end())
                    && current_end - current.start() - current.offset()
                        == next_end - path.start() - path.offset()
                {
                    world.insert(entity, path);
                    world.invalidate_visual(entity);
                    return true;
                }
            }

            let subscription = world
                .resource_mut::<crate::render::path::PathStore>()
                .and_then(|store| {
                    store
                        .subscribe(path.path(), entity, path.end().is_some())
                        .ok()
                        .flatten()
                })
                .map(|inner| crate::text::path::TextPathSubscription { _inner: inner });

            world.insert(entity, path);
            if let Some(subscription) = subscription {
                world.insert(entity, subscription);
            } else {
                world.remove::<crate::text::path::TextPathSubscription>(entity);
            }
            world.invalidate(entity);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::{Signal, flush_signal_dirty};
    use crate::types::Dimension;
    use crate::ui::dirty::Dirty;
    use crate::ui::widgets::{Button, ParagraphStyle, Text};
    use crate::ui::{Children, Widget};

    #[test]
    fn typed_property_updates_share_widget_mut() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Widget);
        world.insert(entity, crate::ui::Style::default());

        world
            .widget_mut(entity)
            .unwrap()
            .set::<prop::Width>(Dimension::from(240))
            .set::<prop::Height>(Dimension::from(120))
            .set::<prop::MinHeight>(Dimension::from(80));

        let layout = &world.get::<crate::ui::Style>(entity).unwrap().layout;
        assert_eq!(layout.width, Dimension::from(240));
        assert_eq!(layout.height, Dimension::from(120));
        assert_eq!(layout.min_height, Dimension::from(80));
    }

    #[test]
    fn dead_entities_do_not_produce_widget_handles() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.despawn(entity);
        assert!(world.widget_mut(entity).is_none());
    }

    #[test]
    fn non_widget_entities_do_not_produce_widget_handles() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        assert!(world.widget_mut(entity).is_none());
    }

    #[test]
    fn unchanged_layout_property_does_not_invalidate_widget() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Widget);
        world.insert(entity, crate::ui::Style::default());

        <prop::Width as Property>::apply(&mut world, entity, Dimension::Auto);
        assert!(!world.has::<Dirty>(entity));

        <prop::Width as Property>::apply(&mut world, entity, Dimension::px(120));
        assert!(world.has::<Dirty>(entity));
    }

    #[test]
    fn button_text_shorthand_and_reactive_text_use_label_paragraphs() {
        let mut world = World::new();
        world.insert_resource(crate::ui::IdMap::new());
        let root = crate::ui::builder::WidgetBuilder::new(&mut world).id();
        let label = Signal::new(alloc::string::String::from("Idle"));
        let bound_label = label.clone();

        crate::ui! {
            :(
                parent: root
                world: &mut world
            :)

            Column () {
                Button("Save")
                Button(text: ${ bound_label.get() })
                Text("Body")
            }
        };

        let column = world.get::<Children>(root).unwrap().0[0];
        let children = &world.get::<Children>(column).unwrap().0;
        let static_button = children[0];
        let reactive_button = children[1];
        let body_text = children[2];
        for (entity, expected) in [(static_button, "Save"), (reactive_button, "Idle")] {
            assert!(world.has::<Button>(entity));
            let text = world.get::<Text>(entity).expect("Button owns Text");
            assert_eq!(text.resolve(&world).as_ref(), expected);
            assert_eq!(text.paragraph(), &ParagraphStyle::label());
        }
        assert_eq!(
            world.get::<Text>(body_text).unwrap().paragraph(),
            &ParagraphStyle::default()
        );

        label.set(alloc::string::String::from("Saved"));
        flush_signal_dirty(&mut world);
        let text = world
            .get::<Text>(reactive_button)
            .expect("reactive Button keeps Text");
        assert_eq!(text.resolve(&world).as_ref(), "Saved");
        assert_eq!(text.paragraph(), &ParagraphStyle::label());
    }
}
