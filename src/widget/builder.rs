use crate::ecs::{Entity, World};
use crate::layout::LayoutStyle;
use crate::types::Color;

use super::{Children, Parent, Style, Widget};

pub struct WidgetBuilder<'a> {
    world: &'a mut World,
    entity: Entity,
}

impl<'a> WidgetBuilder<'a> {
    pub fn new(world: &'a mut World) -> Self {
        let entity = world.spawn();
        world.insert(entity, Widget);
        world.insert(entity, Style::default());
        world.insert(entity, Children(alloc::vec::Vec::new()));
        Self { world, entity }
    }

    pub fn bg_color(self, color: Color) -> Self {
        if let Some(style) = self.world.get_mut::<Style>(self.entity) {
            style.bg_color = Some(color);
        }
        self
    }

    pub fn layout(self, layout: LayoutStyle) -> Self {
        if let Some(style) = self.world.get_mut::<Style>(self.entity) {
            style.layout = layout;
        }
        self
    }

    pub fn child(self, child: Entity) -> Self {
        self.world.insert(child, Parent(self.entity));
        if let Some(children) = self.world.get_mut::<Children>(self.entity) {
            children.0.push(child);
        }
        self
    }

    pub fn id(self) -> Entity {
        self.entity
    }
}
