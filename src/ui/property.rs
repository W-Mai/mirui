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

    /// Applies the value and classifies the invalidation it requires.
    fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropertyChange {
    Unchanged,
    Visual,
    Layout,
}

impl PropertyChange {
    pub const fn changed(self) -> bool {
        !matches!(self, Self::Unchanged)
    }
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
        apply_to_world::<P>(self.world, self.entity, value);
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
            apply_to_world::<P>(world, entity, value);
        }
    });
}

#[doc(hidden)]
pub fn apply_to_world<P: Property>(
    world: &mut World,
    entity: Entity,
    value: P::Value,
) -> PropertyChange {
    let change = P::apply(world, entity, value);
    match change {
        PropertyChange::Unchanged => {}
        PropertyChange::Visual => world.invalidate_visual(entity),
        PropertyChange::Layout => {
            if world.has::<crate::ui::Hidden>(entity) {
                if let Some(parent) = world
                    .get::<crate::ui::Parent>(entity)
                    .map(|parent| parent.0)
                {
                    world.invalidate(parent);
                }
            } else {
                world.invalidate(entity);
            }
        }
    }
    change
}

pub mod prop {
    use super::{Property, PropertyChange};
    use crate::ecs::{Entity, World};

    pub struct TextPath;
    pub struct TextContent;
    pub struct Visible;
    pub struct BackgroundColor;
    pub struct TextColor;
    pub struct ButtonNormalColor;
    pub struct RenderKey;
    pub struct FontSize;
    pub struct Paragraph;
    pub struct Direction;
    pub struct Width;
    pub struct MinWidth;
    pub struct MaxWidth;
    pub struct Height;
    pub struct MinHeight;
    pub struct MaxHeight;
    pub struct Padding;
    pub struct ProgressValue;
    pub struct RowGap;
    pub struct ColumnGap;
    pub struct Left;
    pub struct Top;

    impl Property for TextContent {
        type Value = alloc::string::String;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            let is_button = world.has::<crate::ui::widgets::Button>(entity);
            if let Some(text) = world.get_mut::<crate::ui::widgets::Text>(entity) {
                text.set_content(value);
            } else if is_button {
                world.insert(entity, crate::ui::widgets::Text::label(value));
            } else {
                world.insert(entity, crate::ui::widgets::Text::from(value));
            }
            PropertyChange::Layout
        }
    }

    impl Property for Visible {
        type Value = bool;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            let currently_visible = world.get::<crate::ui::Hidden>(entity).is_none();
            if currently_visible == value {
                return PropertyChange::Unchanged;
            }
            world.remove_resource::<crate::ui::render_system::LayoutSnapshot>();
            if value {
                world.remove::<crate::ui::Hidden>(entity);
                world.mark_subtree_dirty(entity);
            } else {
                if let Some(rect) = world
                    .get::<crate::ui::ComputedRect>(entity)
                    .map(|rect| rect.0)
                {
                    world.invalidate_rect(rect);
                }
                world.insert(entity, crate::ui::Hidden);
                world.clear_subtree_dirty(entity);
            }
            PropertyChange::Layout
        }
    }

    macro_rules! style_property {
        ($name:ident, $value:ty, $field:ident) => {
            impl Property for $name {
                type Value = $value;

                fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
                    if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                        if style.$field == value {
                            return PropertyChange::Unchanged;
                        }
                        style.$field = value;
                    } else {
                        return PropertyChange::Unchanged;
                    }
                    PropertyChange::Visual
                }
            }
        };
    }

    impl Property for BackgroundColor {
        type Value = crate::ui::theme::ThemedColor;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                if style.bg_color == Some(value) {
                    return PropertyChange::Unchanged;
                }
                style.bg_color = Some(value);
            } else {
                return PropertyChange::Unchanged;
            }
            PropertyChange::Visual
        }
    }

    style_property!(TextColor, crate::ui::theme::ThemedColor, text_color);

    impl Property for ButtonNormalColor {
        type Value = crate::ui::theme::ThemedColor;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            if let Some(button) = world.get_mut::<crate::ui::widgets::Button>(entity) {
                if button.normal_color == value {
                    return PropertyChange::Unchanged;
                }
                button.normal_color = value;
            } else {
                return PropertyChange::Unchanged;
            }
            PropertyChange::Visual
        }
    }

    impl Property for ProgressValue {
        type Value = f32;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            let Some(progress) = world.get_mut::<crate::ui::widgets::ProgressBar>(entity) else {
                return PropertyChange::Unchanged;
            };
            let value = value.clamp(0.0, 1.0);
            if (progress.value - value).abs() <= f32::EPSILON {
                return PropertyChange::Unchanged;
            }
            progress.value = value;
            PropertyChange::Visual
        }
    }

    impl Property for RenderKey {
        type Value = u64;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            if world
                .get::<crate::ui::RenderKey>(entity)
                .is_some_and(|current| current.0 == value)
            {
                return PropertyChange::Unchanged;
            }
            world.insert(entity, crate::ui::RenderKey(value));
            PropertyChange::Visual
        }
    }

    impl Property for FontSize {
        type Value = u16;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                if style.font_size == Some(value) {
                    return PropertyChange::Unchanged;
                }
                style.set_font_size(value);
            } else {
                return PropertyChange::Unchanged;
            }
            PropertyChange::Layout
        }
    }

    impl Property for Paragraph {
        type Value = crate::ui::widgets::ParagraphStyle;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            let Some(text) = world.get_mut::<crate::ui::widgets::Text>(entity) else {
                return PropertyChange::Unchanged;
            };
            if text.paragraph() == &value {
                return PropertyChange::Unchanged;
            }
            text.set_paragraph(value);
            PropertyChange::Layout
        }
    }

    impl Property for Direction {
        type Value = crate::ui::layout::FlexDirection;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            let Some(style) = world.get_mut::<crate::ui::Style>(entity) else {
                return PropertyChange::Unchanged;
            };
            if style.layout.direction == value {
                return PropertyChange::Unchanged;
            }
            style.layout.direction = value;
            PropertyChange::Layout
        }
    }

    impl Property for Width {
        type Value = crate::types::Dimension;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                if style.layout.width == value {
                    return PropertyChange::Unchanged;
                }
                style.layout.width = value;
            } else {
                return PropertyChange::Unchanged;
            }
            PropertyChange::Layout
        }
    }

    macro_rules! layout_property {
        ($name:ident, $field:ident) => {
            impl Property for $name {
                type Value = crate::types::Dimension;

                fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
                    if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                        if style.layout.$field == value {
                            return PropertyChange::Unchanged;
                        }
                        style.layout.$field = value;
                    } else {
                        return PropertyChange::Unchanged;
                    }
                    PropertyChange::Layout
                }
            }
        };
    }

    layout_property!(MinWidth, min_width);
    layout_property!(MaxWidth, max_width);

    impl Property for Height {
        type Value = crate::types::Dimension;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                if style.layout.height == value {
                    return PropertyChange::Unchanged;
                }
                style.layout.height = value;
            } else {
                return PropertyChange::Unchanged;
            }
            PropertyChange::Layout
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

        fn apply(world: &mut World, entity: Entity, value: Self::Value) -> PropertyChange {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                if style.layout.padding == value {
                    return PropertyChange::Unchanged;
                }
                style.layout.padding = value;
            } else {
                return PropertyChange::Unchanged;
            }
            PropertyChange::Layout
        }
    }

    impl Property for TextPath {
        type Value = crate::text::TextPath;

        fn apply(world: &mut World, entity: Entity, path: Self::Value) -> PropertyChange {
            if let Some(current) = world.get::<crate::text::TextPath>(entity).copied() {
                if current == path {
                    return PropertyChange::Unchanged;
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
                    return PropertyChange::Visual;
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
            PropertyChange::Layout
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::{Signal, flush_signal_dirty};
    use crate::types::Dimension;
    use crate::ui::dirty::{Dirty, VisualDirty};
    use crate::ui::widgets::{Button, ParagraphStyle, ProgressBar, Text};
    use crate::ui::{Children, Parent, Widget};

    #[test]
    fn render_key_invalidates_visuals_only_when_it_changes() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Widget);

        assert_eq!(
            apply_to_world::<prop::RenderKey>(&mut world, entity, 7),
            PropertyChange::Visual
        );
        assert!(world.has::<crate::ui::dirty::VisualDirty>(entity));
        assert!(!world.has::<Dirty>(entity));

        world.remove::<crate::ui::dirty::VisualDirty>(entity);
        assert_eq!(
            apply_to_world::<prop::RenderKey>(&mut world, entity, 7),
            PropertyChange::Unchanged
        );
        assert!(!world.has::<crate::ui::dirty::VisualDirty>(entity));
    }

    #[test]
    fn reactive_property_preserves_visual_only_invalidation() {
        use crate::render::path::{Path, PathStore};
        use crate::text::TextPath;

        let mut world = World::new();
        world.insert_resource(PathStore::new(1).unwrap());
        let entity = world.spawn_empty();
        world.insert(entity, Widget);
        let path = world
            .resource_mut::<PathStore>()
            .unwrap()
            .insert(Path::new())
            .unwrap();
        world.widget_mut(entity).unwrap().text_path(
            TextPath::new(path)
                .with_range(crate::types::Fixed::ZERO..crate::types::Fixed::from_int(80)),
        );
        world.remove::<Dirty>(entity);

        crate::core::reactive::with_world_scope(&mut world, || {
            super::apply::<prop::TextPath>(
                entity,
                TextPath::new(path).with_range(
                    crate::types::Fixed::from_int(12)..crate::types::Fixed::from_int(92),
                ),
            );
        });

        assert!(!world.has::<Dirty>(entity));
        assert!(world.has::<crate::ui::dirty::VisualDirty>(entity));
    }

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
    fn reactive_projection_properties_update_semantic_widgets() {
        let mut world = World::new();
        let parent = world.spawn_empty();
        world.insert(parent, Widget);
        let visible = world.spawn_empty();
        world.insert(visible, Widget);
        world.insert(visible, Parent(parent));
        let child = world.spawn_empty();
        world.insert(child, Widget);
        world.insert(child, Dirty);
        world.insert(child, VisualDirty);
        world.insert(visible, Children(alloc::vec![child]));
        assert_eq!(
            apply_to_world::<prop::Visible>(&mut world, visible, false),
            PropertyChange::Layout
        );
        assert!(world.has::<crate::ui::Hidden>(visible));
        assert!(!world.has::<Dirty>(visible));
        assert!(!world.has::<Dirty>(child));
        assert!(!world.has::<VisualDirty>(child));
        assert!(world.has::<Dirty>(parent));
        assert_eq!(
            apply_to_world::<prop::Visible>(&mut world, visible, true),
            PropertyChange::Layout
        );
        assert!(!world.has::<crate::ui::Hidden>(visible));
        assert!(world.has::<Dirty>(visible));
        assert!(world.has::<Dirty>(child));

        let progress = world.spawn_empty();
        world.insert(progress, Widget);
        world.insert(progress, ProgressBar::new());
        assert_eq!(
            apply_to_world::<prop::ProgressValue>(&mut world, progress, 0.5),
            PropertyChange::Visual
        );
        assert_eq!(world.get::<ProgressBar>(progress).unwrap().value, 0.5);
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

        apply_to_world::<prop::Width>(&mut world, entity, Dimension::Auto);
        assert!(!world.has::<Dirty>(entity));

        apply_to_world::<prop::Width>(&mut world, entity, Dimension::px(120));
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
