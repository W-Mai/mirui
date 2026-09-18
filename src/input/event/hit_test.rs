use alloc::vec::Vec;

use crate::ecs::{Entity, World};
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::types::{Fixed, Point, Rect, Transform, Transform3D};
use crate::ui::layout::LayoutNode;
use crate::ui::widgets::transform::WidgetTransform;
use crate::ui::widgets::transform_3d::{TransformOrigin, WidgetTransform3D};
use crate::ui::{HitTarget, IgnoreHitTest};

#[derive(Clone, Copy)]
enum HitShape {
    Rect(Rect),
    TransformedRect { rect: Rect, inverse: Transform },
    Quad([Point; 4]),
}

#[derive(Clone, Copy)]
struct HitGeometry {
    entity: Entity,
    shape: HitShape,
    scroll_clip: Option<Rect>,
}

/// Layout-derived input geometry retained between pointer events.
///
/// Only explicit input targets and scroll viewports occupy this buffer. Its
/// allocation is owned by the world and reused by every layout pass.
#[derive(Default)]
struct HitTestGeometry {
    root: Option<Entity>,
    logical_w: u16,
    logical_h: u16,
    entries: Vec<HitGeometry>,
}

impl HitTestGeometry {
    fn matches(&self, root: Entity, logical_w: u16, logical_h: u16) -> bool {
        self.root == Some(root) && self.logical_w == logical_w && self.logical_h == logical_h
    }
}

pub(crate) fn geometry_matches(
    world: &World,
    root: Entity,
    logical_w: u16,
    logical_h: u16,
) -> bool {
    world
        .resource::<HitTestGeometry>()
        .is_some_and(|geometry| geometry.matches(root, logical_w, logical_h))
}

fn shifted(rect: Rect, scroll: (Fixed, Fixed)) -> Rect {
    Rect {
        x: rect.x - scroll.0,
        y: rect.y - scroll.1,
        w: rect.w,
        h: rect.h,
    }
}

fn inside(rect: Rect, point: Point) -> bool {
    point.x >= rect.x && point.x < rect.x + rect.w && point.y >= rect.y && point.y < rect.y + rect.h
}

fn intersect_clip(parent: Option<Rect>, next: Rect) -> Option<Rect> {
    match parent {
        Some(parent) => Some(parent.intersect(&next).unwrap_or(Rect::ZERO)),
        None => Some(next),
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_geometry(
    world: &World,
    node: &LayoutNode,
    entities: &[Entity],
    index: &mut usize,
    scroll: (Fixed, Fixed),
    scroll_clip: Option<Rect>,
    parent_2d: Transform,
    parent_3d: Transform3D,
    entries: &mut Vec<HitGeometry>,
) {
    let Some(&entity) = entities.get(*index) else {
        return;
    };
    *index += 1;

    let local_2d = world
        .get::<WidgetTransform>(entity)
        .map(|transform| transform.0)
        .unwrap_or(Transform::IDENTITY);
    let effective_2d = if local_2d.is_identity() {
        parent_2d
    } else {
        let cx = node.rect.x + node.rect.w / Fixed::from_int(2);
        let cy = node.rect.y + node.rect.h / Fixed::from_int(2);
        parent_2d
            .compose(&Transform::translate(cx, cy))
            .compose(&local_2d)
            .compose(&Transform::translate(Fixed::ZERO - cx, Fixed::ZERO - cy))
    };

    let local_3d = world
        .get::<WidgetTransform3D>(entity)
        .map(|transform| transform.0)
        .filter(|transform| !transform.is_identity());
    let lifted_2d = if local_3d.is_none() && !parent_3d.is_identity() {
        (!local_2d.is_identity()).then(|| Transform3D::from_affine(local_2d))
    } else {
        None
    };
    let effective_3d = if let Some(transform) = local_3d.or(lifted_2d) {
        let origin = world
            .get::<TransformOrigin>(entity)
            .copied()
            .unwrap_or_default();
        let cx = node.rect.x + node.rect.w * origin.x;
        let cy = node.rect.y + node.rect.h * origin.y;
        parent_3d
            .compose(&Transform3D::translate(cx, cy))
            .compose(&transform)
            .compose(&Transform3D::translate(Fixed::ZERO - cx, Fixed::ZERO - cy))
    } else {
        parent_3d
    };

    let is_candidate = world.get::<HitTarget>(entity).is_some()
        || world.get::<ScrollConfig>(entity).is_some()
        || world.get::<ScrollOffset>(entity).is_some();
    if is_candidate && world.get::<IgnoreHitTest>(entity).is_none() {
        let rect = shifted(node.rect, scroll);
        let shape = if effective_3d.is_identity() {
            if effective_2d.is_identity() {
                Some(HitShape::Rect(rect))
            } else {
                effective_2d
                    .inverse()
                    .map(|inverse| HitShape::TransformedRect { rect, inverse })
            }
        } else {
            effective_3d.apply_rect(rect).map(HitShape::Quad)
        };
        if let Some(shape) = shape {
            entries.push(HitGeometry {
                entity,
                shape,
                scroll_clip,
            });
        }
    }

    let child_scroll = world
        .get::<ScrollOffset>(entity)
        .map_or(scroll, |offset| (scroll.0 + offset.x, scroll.1 + offset.y));
    let child_clip = if world.get::<ScrollConfig>(entity).is_some()
        || world.get::<ScrollOffset>(entity).is_some()
    {
        intersect_clip(scroll_clip, shifted(node.rect, scroll))
    } else {
        scroll_clip
    };

    for child in &node.children {
        collect_geometry(
            world,
            child,
            entities,
            index,
            child_scroll,
            child_clip,
            effective_2d,
            effective_3d,
            entries,
        );
    }
}

/// Refresh retained input geometry from the layout snapshot produced in the
/// same pass. The backing allocation is cleared and reused.
pub(crate) fn update_hit_test_geometry(
    world: &mut World,
    root: Entity,
    logical_w: u16,
    logical_h: u16,
    layout_tree: &LayoutNode,
    entities: &[Entity],
) {
    let mut geometry = world
        .take_resource_box::<HitTestGeometry>()
        .unwrap_or_default();
    geometry.entries.clear();
    let mut index = 0;
    collect_geometry(
        world,
        layout_tree,
        entities,
        &mut index,
        (Fixed::ZERO, Fixed::ZERO),
        None,
        Transform::IDENTITY,
        Transform3D::IDENTITY,
        &mut geometry.entries,
    );
    geometry.root = Some(root);
    geometry.logical_w = logical_w;
    geometry.logical_h = logical_h;
    world.put_resource_box(geometry);
}

/// Find the deepest input target containing the coordinate.
///
/// Ordinary visual widgets do not participate. Scroll viewports remain
/// candidates so a drag can begin over otherwise empty viewport space.
pub fn hit_test(
    world: &World,
    root: Entity,
    x: Fixed,
    y: Fixed,
    screen_w: u16,
    screen_h: u16,
) -> Option<Entity> {
    let geometry = world
        .resource::<HitTestGeometry>()
        .filter(|geometry| geometry.matches(root, screen_w, screen_h))?;
    let point = Point { x, y };
    let mut hit = None;
    for entry in &geometry.entries {
        if world.get::<IgnoreHitTest>(entry.entity).is_some()
            || (world.get::<HitTarget>(entry.entity).is_none()
                && world.get::<ScrollConfig>(entry.entity).is_none()
                && world.get::<ScrollOffset>(entry.entity).is_none())
        {
            continue;
        }
        let contains = match entry.shape {
            HitShape::Rect(rect) => {
                inside(rect, point) && entry.scroll_clip.is_none_or(|clip| inside(clip, point))
            }
            HitShape::TransformedRect { rect, inverse } => {
                let probe = inverse.apply_point(point);
                inside(rect, probe) && entry.scroll_clip.is_none_or(|clip| inside(clip, point))
            }
            HitShape::Quad(quad) => {
                crate::types::transform_3d::point_in_quad(&quad, point)
                    && entry.scroll_clip.is_none_or(|clip| inside(clip, point))
            }
        };
        if contains {
            hit = Some(entry.entity);
        }
    }
    hit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::scroll::ScrollAxis;
    use crate::types::{Dimension, Viewport};
    use crate::ui::layout::LayoutStyle;
    use crate::ui::{Children, Parent, Style, Widget};

    fn fixed_style(width: i32, height: i32) -> Style {
        Style {
            layout: LayoutStyle {
                width: Dimension::px(width),
                height: Dimension::px(height),
                ..LayoutStyle::default()
            },
            ..Style::default()
        }
    }

    fn append(world: &mut World, parent: Entity, child: Entity) {
        world.insert(child, Parent(parent));
        world
            .get_mut::<Children>(parent)
            .expect("parent children")
            .0
            .push(child);
    }

    #[test]
    fn retained_geometry_tracks_layout_without_query_allocations() {
        let mut world = World::new();
        let root = world.spawn_empty();
        world.insert(root, Widget);
        world.insert(root, fixed_style(128, 128));
        world.insert(root, Children(Vec::new()));
        let target = world.spawn_empty();
        world.insert(target, Widget);
        world.insert(target, fixed_style(32, 24));
        world.insert(target, HitTarget);
        world.insert(target, Children(Vec::new()));
        append(&mut world, root, target);

        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(128, 128, Fixed::ONE),
        );
        assert_eq!(
            hit_test(&world, root, 4.into(), 4.into(), 128, 128),
            Some(target)
        );
        let capacity = world
            .resource::<HitTestGeometry>()
            .unwrap()
            .entries
            .capacity();
        for _ in 0..8 {
            assert_eq!(
                hit_test(&world, root, 4.into(), 4.into(), 128, 128),
                Some(target)
            );
        }
        assert_eq!(
            world
                .resource::<HitTestGeometry>()
                .unwrap()
                .entries
                .capacity(),
            capacity
        );
    }

    #[test]
    fn empty_scroll_viewport_can_start_a_drag() {
        let mut world = World::new();
        let root = world.spawn_empty();
        world.insert(root, Widget);
        world.insert(root, fixed_style(128, 128));
        world.insert(root, Children(Vec::new()));
        let scroll = world.spawn_empty();
        world.insert(scroll, Widget);
        world.insert(scroll, fixed_style(96, 72));
        world.insert(scroll, Children(Vec::new()));
        world.insert(
            scroll,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
        );
        world.insert(
            scroll,
            ScrollConfig {
                direction: ScrollAxis::Vertical,
                content_height: Fixed::from_int(256),
                ..ScrollConfig::default()
            },
        );
        append(&mut world, root, scroll);

        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(128, 128, Fixed::ONE),
        );
        assert_eq!(
            hit_test(&world, root, 40.into(), 40.into(), 128, 128),
            Some(scroll)
        );
    }

    #[test]
    fn transformed_target_still_uses_the_viewport_clip_in_screen_space() {
        let mut world = World::new();
        let root = world.spawn_empty();
        world.insert(root, Widget);
        world.insert(root, fixed_style(128, 128));
        world.insert(root, Children(Vec::new()));

        let scroll = world.spawn_empty();
        world.insert(scroll, Widget);
        world.insert(scroll, fixed_style(40, 40));
        world.insert(scroll, Children(Vec::new()));
        world.insert(scroll, ScrollConfig::default());
        append(&mut world, root, scroll);

        let target = world.spawn_empty();
        world.insert(target, Widget);
        world.insert(target, fixed_style(20, 20));
        world.insert(target, HitTarget);
        world.insert(target, Children(Vec::new()));
        world.insert(
            target,
            WidgetTransform(Transform::translate(Fixed::from_int(30), Fixed::ZERO)),
        );
        append(&mut world, scroll, target);

        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(128, 128, Fixed::ONE),
        );
        assert_eq!(
            hit_test(&world, root, 35.into(), 10.into(), 128, 128),
            Some(target)
        );
        assert_eq!(hit_test(&world, root, 45.into(), 10.into(), 128, 128), None);
    }

    #[test]
    fn visual_dirty_refreshes_transformed_geometry() {
        let mut world = World::new();
        let root = world.spawn_empty();
        world.insert(root, Widget);
        world.insert(root, fixed_style(128, 128));
        world.insert(root, Children(Vec::new()));
        let target = world.spawn_empty();
        world.insert(target, Widget);
        world.insert(target, fixed_style(24, 24));
        world.insert(target, HitTarget);
        world.insert(target, Children(Vec::new()));
        append(&mut world, root, target);
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::ui::render_system::update_layout(&mut world, root, &viewport);

        world.insert(
            target,
            WidgetTransform(Transform::translate(Fixed::from_int(40), Fixed::ZERO)),
        );
        world.insert(target, crate::ui::dirty::VisualDirty);
        let mut dirty = crate::ui::dirty::DirtyRegions::default();
        crate::ui::render_system::collect_dirty_regions_into(
            &mut world, root, &viewport, &mut dirty,
        );

        assert_eq!(hit_test(&world, root, 4.into(), 4.into(), 128, 128), None);
        assert_eq!(
            hit_test(&world, root, 44.into(), 4.into(), 128, 128),
            Some(target)
        );
    }
}
