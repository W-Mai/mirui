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

    fn apply(world: &mut World, entity: Entity, value: Self::Value);
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
        P::apply(self.world, self.entity, value);
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
        if let Some(mut widget) = world.widget_mut(entity) {
            widget.set::<P>(value);
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
    pub struct Height;

    impl Property for TextContent {
        type Value = alloc::string::String;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) {
            if let Some(text) = world.get_mut::<crate::ui::widgets::Text>(entity) {
                text.set_content(value);
            } else {
                world.insert(entity, crate::ui::widgets::Text::from(value));
            }
            world.invalidate(entity);
        }
    }

    macro_rules! style_property {
        ($name:ident, $value:ty, $field:ident) => {
            impl Property for $name {
                type Value = $value;

                fn apply(world: &mut World, entity: Entity, value: Self::Value) {
                    if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                        style.$field = value;
                    }
                    world.invalidate(entity);
                }
            }
        };
    }

    impl Property for BackgroundColor {
        type Value = crate::ui::theme::ThemedColor;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                style.bg_color = Some(value);
            }
            world.invalidate(entity);
        }
    }

    style_property!(TextColor, crate::ui::theme::ThemedColor, text_color);

    impl Property for ButtonNormalColor {
        type Value = crate::ui::theme::ThemedColor;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) {
            if let Some(button) = world.get_mut::<crate::ui::widgets::Button>(entity) {
                button.normal_color = value;
            }
            world.invalidate(entity);
        }
    }

    impl Property for FontSize {
        type Value = u16;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                style.set_font_size(value);
            }
            world.invalidate(entity);
        }
    }

    impl Property for Paragraph {
        type Value = crate::ui::widgets::ParagraphStyle;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) {
            if let Some(text) = world.get_mut::<crate::ui::widgets::Text>(entity) {
                text.set_paragraph(value);
            }
            world.invalidate(entity);
        }
    }

    impl Property for Width {
        type Value = crate::types::Dimension;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                style.layout.width = value;
            }
            world.invalidate(entity);
        }
    }

    impl Property for Height {
        type Value = crate::types::Dimension;

        fn apply(world: &mut World, entity: Entity, value: Self::Value) {
            if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
                style.layout.height = value;
            }
            world.invalidate(entity);
        }
    }

    impl Property for TextPath {
        type Value = crate::text::TextPath;

        fn apply(world: &mut World, entity: Entity, path: Self::Value) {
            if let Some(current) = world.get::<crate::text::TextPath>(entity).copied() {
                if current == path {
                    return;
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
                    return;
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Dimension;
    use crate::ui::Widget;

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
            .set::<prop::Height>(Dimension::from(120));

        let layout = &world.get::<crate::ui::Style>(entity).unwrap().layout;
        assert_eq!(layout.width, Dimension::from(240));
        assert_eq!(layout.height, Dimension::from(120));
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
}
