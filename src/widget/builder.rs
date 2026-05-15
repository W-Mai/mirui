use crate::components::transform::WidgetTransform;
use crate::components::transform_3d::{TransformOrigin, WidgetTransform3D};
use crate::ecs::{Entity, World};
use crate::layout::LayoutStyle;
use crate::types::{Color, Fixed, Transform, Transform3D};

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

    pub fn border(self, color: Color, width: impl Into<crate::types::Fixed>) -> Self {
        if let Some(style) = self.world.get_mut::<Style>(self.entity) {
            style.border_color = Some(color);
            style.border_width = width.into();
        }
        self
    }

    pub fn border_width(self, width: impl Into<crate::types::Fixed>) -> Self {
        if let Some(style) = self.world.get_mut::<Style>(self.entity) {
            style.border_width = width.into();
        }
        self
    }

    pub fn border_radius(self, radius: impl Into<crate::types::Fixed>) -> Self {
        if let Some(style) = self.world.get_mut::<Style>(self.entity) {
            style.border_radius = radius.into();
        }
        self
    }

    pub fn clip_children(self, clip: bool) -> Self {
        if let Some(style) = self.world.get_mut::<Style>(self.entity) {
            style.clip_children = clip;
        }
        self
    }

    pub fn text(self, t: &str) -> Self {
        self.world.insert(
            self.entity,
            super::Text(alloc::vec::Vec::from(t.as_bytes())),
        );
        self
    }

    pub fn image(self, img: crate::components::image::Image) -> Self {
        self.world.insert(self.entity, img);
        self
    }

    pub fn text_color(self, color: Color) -> Self {
        if let Some(style) = self.world.get_mut::<Style>(self.entity) {
            style.text_color = Some(color);
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

    /// Replace the entity's transform wholesale.
    pub fn transform(self, t: Transform) -> Self {
        self.world.insert(self.entity, WidgetTransform(t));
        self
    }

    /// Compose an additional transform on top of the existing one.
    /// Right-to-left order: later chain calls apply first.
    pub fn apply_transform(self, t: Transform) -> Self {
        let current = self
            .world
            .get::<WidgetTransform>(self.entity)
            .map(|wt| wt.0)
            .unwrap_or(Transform::IDENTITY);
        self.world
            .insert(self.entity, WidgetTransform(current.compose(&t)));
        self
    }

    pub fn rotate(self, deg: impl Into<Fixed>) -> Self {
        self.apply_transform(Transform::rotate_deg(deg.into()))
    }

    pub fn translate(self, tx: impl Into<Fixed>, ty: impl Into<Fixed>) -> Self {
        self.apply_transform(Transform::translate(tx.into(), ty.into()))
    }

    pub fn scale_xy(self, sx: impl Into<Fixed>, sy: impl Into<Fixed>) -> Self {
        self.apply_transform(Transform::scale(sx.into(), sy.into()))
    }

    pub fn transform_3d(self, t: Transform3D) -> Self {
        self.world.insert(self.entity, WidgetTransform3D(t));
        self
    }

    pub fn transform_origin(self, x: impl Into<Fixed>, y: impl Into<Fixed>) -> Self {
        self.world.insert(self.entity, TransformOrigin::new(x, y));
        self
    }

    pub fn apply_transform_3d(self, t: Transform3D) -> Self {
        let current = self
            .world
            .get::<WidgetTransform3D>(self.entity)
            .map(|wt| wt.0)
            .unwrap_or(Transform3D::IDENTITY);
        self.world
            .insert(self.entity, WidgetTransform3D(current.compose(&t)));
        self
    }

    pub fn rotate_y(self, deg: impl Into<Fixed>) -> Self {
        self.apply_transform_3d(Transform3D::rotate_y_deg(deg.into()))
    }

    pub fn rotate_x(self, deg: impl Into<Fixed>) -> Self {
        self.apply_transform_3d(Transform3D::rotate_x_deg(deg.into()))
    }

    pub fn rotate_y_perspective(self, deg: impl Into<Fixed>, distance: impl Into<Fixed>) -> Self {
        self.apply_transform_3d(Transform3D::rotate_y_perspective(
            deg.into(),
            distance.into(),
        ))
    }

    pub fn rotate_x_perspective(self, deg: impl Into<Fixed>, distance: impl Into<Fixed>) -> Self {
        self.apply_transform_3d(Transform3D::rotate_x_perspective(
            deg.into(),
            distance.into(),
        ))
    }

    pub fn perspective(self, distance: impl Into<Fixed>) -> Self {
        self.apply_transform_3d(Transform3D::perspective(distance.into()))
    }

    pub fn id(self) -> Entity {
        self.entity
    }
}
