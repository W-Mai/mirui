//! Prepared layout bounds and their screen-space placement.

use core::cell::Ref;

use crate::ecs::{Entity, World};
use crate::input::event::scroll::ScrollOffset;
use crate::types::{Fixed, Rect, Transform, Transform3D};

use super::layout::LayoutNode;
use super::outputs::{Output, Outputs, private::OutputKind};

/// Widget geometry from a successful layout and visual transform preparation.
///
/// Declare demand with `#[depends_on(LayoutGeometry)]` on an entity-valued component field, then read `Outputs<LayoutGeometry>`. Reads do not compute layout or request production.
pub struct LayoutGeometry;

/// A snapshot in logical pixels. Local rectangles start at the source widget's untransformed border origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutGeometryRead {
    border_rect: Rect,
    content_rect: Rect,
    transform: Transform3D,
    clip: Rect,
}

impl LayoutGeometryRead {
    /// The source widget's border rectangle in local coordinates.
    pub const fn border_rect(&self) -> Rect {
        self.border_rect
    }

    /// The local content rectangle after resolved layout padding, including safe-area padding.
    pub const fn content_rect(&self) -> Rect {
        self.content_rect
    }

    /// Local-to-screen transform, including ancestor scrolling and affine/projective transforms.
    pub const fn transform(&self) -> Transform3D {
        self.transform
    }

    /// Screen-space intersection of the viewport and ancestor clips. An empty rectangle is fully clipped; the source widget's own child clip is not included.
    pub const fn clip(&self) -> Rect {
        self.clip
    }
}

impl Output for LayoutGeometry {
    type Storage = ();
    type Record = LayoutGeometryRead;
    type Read<'a> = LayoutGeometryRead;

    fn read<'a>(_: Ref<'a, ()>, record: Self::Record) -> Option<Self::Read<'a>> {
        Some(record)
    }
}

impl OutputKind for LayoutGeometry {
    fn install(world: &mut World) {
        if world.resource::<Outputs<Self>>().is_none() {
            world.insert_resource(Outputs::<Self>::new(()));
        }
    }

    fn reconcile(world: &World) {
        let Some(outputs) = world.resource::<Outputs<Self>>() else {
            return;
        };
        outputs.reset_where(|entity| {
            !world.is_alive(entity)
                || !world.has::<super::Widget>(entity)
                || super::branch::is_hidden_in_tree(world, entity)
        });
        outputs.retain_entities(|_| false);
    }
}

#[derive(Clone, Copy)]
struct Placement {
    scroll_x: Fixed,
    scroll_y: Fixed,
    affine: Transform,
    projective: Transform3D,
    clip: Rect,
}

pub(crate) fn publish(world: &World, tree: &LayoutNode, entities: &[Entity], viewport: Rect) {
    let Some(outputs) = world.resource::<Outputs<LayoutGeometry>>() else {
        return;
    };
    if !outputs.has_requests() {
        return;
    }
    outputs.reset_where(|entity| {
        !entities.contains(&entity) || super::branch::is_hidden_in_tree(world, entity)
    });
    publish_node(
        world,
        outputs,
        tree,
        Placement {
            scroll_x: Fixed::ZERO,
            scroll_y: Fixed::ZERO,
            affine: Transform::IDENTITY,
            projective: Transform3D::IDENTITY,
            clip: viewport,
        },
    );
}

fn publish_node(
    world: &World,
    outputs: &Outputs<LayoutGeometry>,
    node: &LayoutNode,
    parent: Placement,
) {
    let Some(entity) = node.entity else {
        return;
    };
    if super::branch::is_effectively_hidden(world, entity) {
        outputs.reset(entity);
        return;
    }
    let rect = Rect {
        x: node.rect.x - parent.scroll_x,
        y: node.rect.y - parent.scroll_y,
        ..node.rect
    };
    let (affine, projective) =
        super::geometry::render_transforms(parent.affine, parent.projective, world, entity, rect);
    let parent_clip = super::widgets::follow_bounds::clip(world, entity)
        .map_or(parent.clip, |clip| {
            parent.clip.intersect(&clip).unwrap_or(Rect::ZERO)
        });
    if outputs.is_requested(entity) {
        let content = node.content_rect();
        let record = LayoutGeometryRead {
            border_rect: Rect::new(0, 0, rect.w, rect.h),
            content_rect: Rect {
                x: content.x - node.rect.x,
                y: content.y - node.rect.y,
                ..content
            },
            transform: if projective.is_identity() {
                Transform3D::from(affine)
            } else {
                projective
            }
            .compose(&Transform3D::translate(rect.x, rect.y)),
            clip: parent_clip,
        };
        if let Err(error) = outputs.publish_changed(entity, record) {
            outputs.invalidate(entity);
            crate::warn!(target: "mirui::outputs", "layout geometry rejected: {:?}", error);
        }
    }
    let scroll = world.get::<ScrollOffset>(entity);
    let clip = if scroll.is_some()
        || world
            .get::<super::Style>(entity)
            .is_some_and(|style| style.clip_children)
    {
        super::geometry::transformed_clip_bounds(rect, affine, projective)
            .and_then(|bounds| parent_clip.intersect(&bounds))
            .unwrap_or(Rect::ZERO)
    } else {
        parent_clip
    };
    let child = Placement {
        scroll_x: parent.scroll_x + scroll.map_or(Fixed::ZERO, |scroll| scroll.x),
        scroll_y: parent.scroll_y + scroll.map_or(Fixed::ZERO, |scroll| scroll.y),
        affine,
        projective,
        clip,
    };
    for node in &node.children {
        publish_node(world, outputs, node, child);
    }
}

#[cfg(test)]
mod tests;
