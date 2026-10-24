//! Prepared layout bounds and their screen-space placement.

use core::cell::Ref;

use crate::ecs::{Entity, World};
use crate::input::event::scroll::ScrollOffset;
use crate::types::{Fixed, Rect, Transform, Transform3D};

use super::layout::LayoutNode;
use super::layout::paths::{LayoutPaths, SelectedPaths};
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

impl Placement {
    fn root(viewport: Rect) -> Self {
        Self {
            scroll_x: Fixed::ZERO,
            scroll_y: Fixed::ZERO,
            affine: Transform::IDENTITY,
            projective: Transform3D::IDENTITY,
            clip: viewport,
        }
    }

    fn descend(self, world: &World, node: &LayoutNode, entity: Entity) -> (Rect, Rect, Self) {
        let rect = Rect {
            x: node.rect.x - self.scroll_x,
            y: node.rect.y - self.scroll_y,
            ..node.rect
        };
        let (affine, projective) =
            super::geometry::render_transforms(self.affine, self.projective, world, entity, rect);
        let parent_clip = super::widgets::follow_bounds::clip(world, entity)
            .map_or(self.clip, |clip| {
                self.clip.intersect(&clip).unwrap_or(Rect::ZERO)
            });
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
        (
            rect,
            parent_clip,
            Self {
                scroll_x: self.scroll_x + scroll.map_or(Fixed::ZERO, |scroll| scroll.x),
                scroll_y: self.scroll_y + scroll.map_or(Fixed::ZERO, |scroll| scroll.y),
                affine,
                projective,
                clip,
            },
        )
    }
}

pub(crate) fn publish(
    world: &World,
    tree: &LayoutNode,
    entities: &[Entity],
    paths: &LayoutPaths,
    viewport: Rect,
) {
    let Some(outputs) = world.resource::<Outputs<LayoutGeometry>>() else {
        return;
    };
    if !outputs.has_requests() {
        return;
    }
    if let Some(selected) = paths.selected() {
        outputs.reset_where(|entity| !selected.contains(entity));
        publish_selected(world, outputs, tree, Placement::root(viewport), selected, 0);
    } else {
        outputs.reset_where(|entity| {
            !entities.contains(&entity) || super::branch::is_hidden_in_tree(world, entity)
        });
        publish_node(world, outputs, tree, Placement::root(viewport));
    }
}

pub(crate) fn publish_subtree(
    world: &World,
    tree: &LayoutNode,
    target: Entity,
    paths: &LayoutPaths,
    viewport: Rect,
) {
    if let Some(outputs) = world.resource::<Outputs<LayoutGeometry>>()
        && let Some(path) = paths.path(target)
        && let Some(selected) = paths.selected().and_then(|selected| selected.subtree(path))
    {
        let mut node = tree;
        let mut parent = Placement::root(viewport);
        for &index in path {
            let Some(entity) = node.entity else {
                return;
            };
            if super::branch::is_effectively_hidden(world, entity) {
                for entity in selected.entities() {
                    outputs.reset(entity);
                }
                return;
            }
            parent = parent.descend(world, node, entity).2;
            let Some(child) = node.children.get(index) else {
                return;
            };
            node = child;
        }
        if node.entity == Some(target) {
            publish_selected(world, outputs, node, parent, selected, path.len());
            return;
        }
    }
    struct Ancestor<'a> {
        node: &'a LayoutNode,
        parent: Option<&'a Ancestor<'a>>,
    }

    fn placement(world: &World, path: Option<&Ancestor<'_>>, viewport: Rect) -> Option<Placement> {
        let Some(path) = path else {
            return Some(Placement::root(viewport));
        };
        let entity = path.node.entity?;
        if super::branch::is_effectively_hidden(world, entity) {
            return None;
        }
        let parent = placement(world, path.parent, viewport)?;
        Some(parent.descend(world, path.node, entity).2)
    }

    fn find(
        world: &World,
        outputs: &Outputs<LayoutGeometry>,
        node: &LayoutNode,
        target: Entity,
        parent: Option<&Ancestor<'_>>,
        viewport: Rect,
    ) -> bool {
        if node.entity == Some(target) {
            if let Some(parent) = placement(world, parent, viewport) {
                publish_node(world, outputs, node, parent);
            } else {
                reset_subtree(outputs, node);
            }
            return true;
        }
        let ancestor = Ancestor { node, parent };
        node.children
            .iter()
            .any(|node| find(world, outputs, node, target, Some(&ancestor), viewport))
    }

    if let Some(outputs) = world.resource::<Outputs<LayoutGeometry>>()
        && outputs.has_requests()
    {
        find(world, outputs, tree, target, None, viewport);
    }
}

fn reset_subtree(outputs: &Outputs<LayoutGeometry>, node: &LayoutNode) {
    if let Some(entity) = node.entity {
        outputs.reset(entity);
    }
    for child in &node.children {
        reset_subtree(outputs, child);
    }
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
        reset_subtree(outputs, node);
        return;
    }
    let (rect, parent_clip, child) = parent.descend(world, node, entity);
    publish_record(outputs, node, entity, rect, parent_clip, child);
    for node in &node.children {
        publish_node(world, outputs, node, child);
    }
}

fn publish_selected(
    world: &World,
    outputs: &Outputs<LayoutGeometry>,
    node: &LayoutNode,
    parent: Placement,
    selected: SelectedPaths<'_>,
    depth: usize,
) {
    let Some(entity) = node.entity else {
        return;
    };
    if super::branch::is_effectively_hidden(world, entity) {
        for entity in selected.entities() {
            outputs.reset(entity);
        }
        return;
    }
    let (rect, parent_clip, child) = parent.descend(world, node, entity);
    publish_record(outputs, node, entity, rect, parent_clip, child);
    for (index, selected) in selected.children(depth) {
        if let Some(node) = node.children.get(index) {
            publish_selected(world, outputs, node, child, selected, depth + 1);
        }
    }
}

fn publish_record(
    outputs: &Outputs<LayoutGeometry>,
    node: &LayoutNode,
    entity: Entity,
    rect: Rect,
    parent_clip: Rect,
    child: Placement,
) {
    if outputs.is_requested(entity) {
        let content = node.content_rect();
        let record = LayoutGeometryRead {
            border_rect: Rect::new(0, 0, rect.w, rect.h),
            content_rect: Rect {
                x: content.x - node.rect.x,
                y: content.y - node.rect.y,
                ..content
            },
            transform: if child.projective.is_identity() {
                Transform3D::from(child.affine)
            } else {
                child.projective
            }
            .compose(&Transform3D::translate(rect.x, rect.y)),
            clip: parent_clip,
        };
        if let Err(error) = outputs.publish_changed(entity, record) {
            outputs.invalidate(entity);
            crate::warn!(target: "mirui::outputs", "layout geometry rejected: {:?}", error);
        }
    }
}

#[cfg(test)]
mod tests;
