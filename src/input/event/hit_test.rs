use alloc::vec::Vec;

use crate::ecs::{Entity, World};
use crate::input::event::scroll::{ScrollConfig, ScrollOffset};
use crate::types::{Fixed, Point, Rect, Transform, Transform3D};
use crate::ui::layout::LayoutNode;
use crate::ui::{HitTarget, IgnoreHitTest};

#[derive(Clone, Copy, PartialEq, Eq)]
enum HitShape {
    Rect(Rect),
    TransformedRect { rect: Rect, inverse: Transform },
    Quad([Point; 4]),
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
    prepared_root: Option<Entity>,
    logical_w: u16,
    logical_h: u16,
    revision: u64,
    layout_revision: u64,
    visual_revision: u64,
    preparation_blocked: bool,
    entries: Vec<HitGeometry>,
}

fn count_potential_targets(world: &World, entity: Entity) -> usize {
    if world.get::<crate::ui::Widget>(entity).is_none()
        || world.get::<crate::ui::Style>(entity).is_none()
    {
        return 0;
    }
    let mut count = usize::from(
        world.get::<IgnoreHitTest>(entity).is_none()
            && (world.get::<HitTarget>(entity).is_some()
                || world.get::<ScrollConfig>(entity).is_some()
                || world.get::<ScrollOffset>(entity).is_some()),
    );
    if let Some(children) = world.get::<crate::ui::Children>(entity) {
        for &child in &children.0 {
            count = count.saturating_add(count_potential_targets(world, child));
        }
    }
    count
}

impl HitTestGeometry {
    fn matches(&self, root: Entity, logical_w: u16, logical_h: u16) -> bool {
        self.root == Some(root) && self.logical_w == logical_w && self.logical_h == logical_h
    }
}

struct GeometryWriter<'a> {
    entries: &'a mut Vec<HitGeometry>,
    used: usize,
    changed: bool,
}

impl GeometryWriter<'_> {
    fn record(&mut self, entry: HitGeometry) {
        if let Some(previous) = self.entries.get_mut(self.used) {
            if *previous != entry {
                *previous = entry;
                self.changed = true;
            }
        } else {
            self.entries.push(entry);
            self.changed = true;
        }
        self.used += 1;
    }

    fn finish(&mut self) {
        if self.entries.len() != self.used {
            self.entries.truncate(self.used);
            self.changed = true;
        }
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

pub(crate) fn geometry_revision(world: &World) -> u64 {
    world
        .resource::<HitTestGeometry>()
        .map_or(0, |geometry| geometry.revision)
}

pub(super) fn geometry_viewport(world: &World) -> Option<(Entity, u16, u16)> {
    let geometry = world.resource::<HitTestGeometry>()?;
    if geometry.preparation_blocked {
        return None;
    }
    Some((geometry.root?, geometry.logical_w, geometry.logical_h))
}

pub(crate) fn prepare_geometry_tracking(world: &mut World) {
    world.watch_component_revision::<crate::ui::dirty::Dirty>();
    world.watch_component_revision::<crate::ui::dirty::VisualDirty>();
}

pub(crate) fn set_preparation_blocked(world: &mut World, blocked: bool) {
    if let Some(geometry) = world.resource_mut::<HitTestGeometry>() {
        geometry.preparation_blocked = blocked;
    } else if blocked {
        world.insert_resource(HitTestGeometry {
            preparation_blocked: true,
            ..HitTestGeometry::default()
        });
    }
}

pub(crate) fn preparation_blocked(world: &World) -> bool {
    world
        .resource::<HitTestGeometry>()
        .is_some_and(|geometry| geometry.preparation_blocked)
}

pub(crate) fn geometry_changes(world: &World) -> (bool, bool) {
    world
        .resource::<HitTestGeometry>()
        .map_or((true, true), |geometry| {
            (
                geometry.layout_revision != world.component_revision::<crate::ui::dirty::Dirty>(),
                geometry.visual_revision
                    != world.component_revision::<crate::ui::dirty::VisualDirty>(),
            )
        })
}

pub(crate) fn invalidate_hit_test_geometry(world: &mut World) {
    if let Some(geometry) = world.resource_mut::<HitTestGeometry>() {
        geometry.root = None;
        geometry.revision = geometry.revision.wrapping_add(1);
    }
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
    writer: &mut GeometryWriter<'_>,
) {
    let Some(&entity) = entities.get(*index) else {
        return;
    };
    if crate::ui::branch::is_effectively_hidden(world, entity) {
        *index += crate::ui::render_system::count_nodes(node);
        return;
    }
    *index += 1;

    let scroll_clip = crate::ui::widgets::follow_bounds::clip(world, entity)
        .map_or(scroll_clip, |clip| intersect_clip(scroll_clip, clip));

    let rect = shifted(node.rect, scroll);
    let (effective_2d, effective_3d) =
        crate::ui::geometry::render_transforms(parent_2d, parent_3d, world, entity, rect);

    let is_candidate = world.get::<HitTarget>(entity).is_some()
        || world.get::<ScrollConfig>(entity).is_some()
        || world.get::<ScrollOffset>(entity).is_some();
    if is_candidate && world.get::<IgnoreHitTest>(entity).is_none() {
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
            writer.record(HitGeometry {
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
        || world
            .get::<crate::ui::Style>(entity)
            .is_some_and(|style| style.clip_children)
    {
        let bounds = crate::ui::geometry::transformed_clip_bounds(rect, effective_2d, effective_3d)
            .unwrap_or(Rect::ZERO);
        intersect_clip(scroll_clip, bounds)
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
            writer,
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
    prepare_geometry_tracking(world);
    let mut geometry = world
        .take_resource_box::<HitTestGeometry>()
        .unwrap_or_default();
    if geometry.prepared_root != Some(root) {
        let target_count = count_potential_targets(world, root);
        geometry
            .entries
            .reserve(target_count.saturating_sub(geometry.entries.len()));
        geometry.prepared_root = Some(root);
    }
    let geometry_changed = !geometry.matches(root, logical_w, logical_h);
    let mut writer = GeometryWriter {
        entries: &mut geometry.entries,
        used: 0,
        changed: geometry_changed,
    };
    let has_targets = world.storage::<HitTarget>().is_some_and(|s| !s.is_empty())
        || world
            .storage::<ScrollConfig>()
            .is_some_and(|s| !s.is_empty())
        || world
            .storage::<ScrollOffset>()
            .is_some_and(|s| !s.is_empty());
    if has_targets {
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
            &mut writer,
        );
    }
    writer.finish();
    let geometry_changed = writer.changed;
    geometry.root = Some(root);
    geometry.logical_w = logical_w;
    geometry.logical_h = logical_h;
    geometry.layout_revision = world.component_revision::<crate::ui::dirty::Dirty>();
    geometry.visual_revision = world.component_revision::<crate::ui::dirty::VisualDirty>();
    if geometry_changed {
        geometry.revision = geometry.revision.wrapping_add(1);
    }
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
    if crate::ui::branch::is_hidden_in_tree(world, root) {
        return None;
    }
    let geometry = world.resource::<HitTestGeometry>().filter(|geometry| {
        !geometry.preparation_blocked && geometry.matches(root, screen_w, screen_h)
    })?;
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
        if contains && !crate::ui::branch::is_hidden_in_tree(world, entry.entity) {
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
    use crate::ui::widgets::transform::WidgetTransform;
    use crate::ui::widgets::transform_3d::{TransformOrigin, WidgetTransform3D};
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
    fn empty_geometry_tracks_target_installation_and_retirement_without_allocating() {
        let mut world = World::new();
        let root = world.spawn_empty();
        world.insert(root, Widget);
        world.insert(root, fixed_style(32, 24));
        let mut node = LayoutNode::new(LayoutStyle::default());
        node.entity = Some(root);
        node.rect = Rect::new(0, 0, 32, 24);
        let refresh = |world: &mut World| {
            update_hit_test_geometry(world, root, 128, 128, &node, &[root]);
        };
        refresh(&mut world);
        assert!(geometry_matches(&world, root, 128, 128));
        let empty_revision = geometry_revision(&world);
        refresh(&mut world);
        assert_eq!(geometry_revision(&world), empty_revision);
        for kind in 0..3 {
            match kind {
                0 => world.insert(root, HitTarget),
                1 => world.insert(root, ScrollConfig::default()),
                _ => world.insert(root, ScrollOffset::default()),
            };
            refresh(&mut world);
            assert_eq!(
                hit_test(&world, root, 4.into(), 4.into(), 128, 128),
                Some(root)
            );
            let populated_revision = geometry_revision(&world);
            world.remove::<HitTarget>(root);
            world.remove::<ScrollConfig>(root);
            world.remove::<ScrollOffset>(root);
            assert_eq!(
                crate::test_support::tracked_allocations(|| refresh(&mut world)),
                0
            );
            assert!(
                world
                    .resource::<HitTestGeometry>()
                    .unwrap()
                    .entries
                    .is_empty()
            );
            assert_ne!(geometry_revision(&world), populated_revision);
            assert_eq!(hit_test(&world, root, 4.into(), 4.into(), 128, 128), None);
            assert_eq!(geometry_changes(&world), (false, false));
        }
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

        crate::ui::render_system::try_update_layout(
            &mut world,
            root,
            &Viewport::new(128, 128, Fixed::ONE),
        )
        .unwrap();
        assert_eq!(
            hit_test(&world, root, 4.into(), 4.into(), 128, 128),
            Some(target)
        );
        let revision = geometry_revision(&world);
        crate::ui::render_system::try_update_layout(
            &mut world,
            root,
            &Viewport::new(128, 128, Fixed::ONE),
        )
        .unwrap();
        assert_eq!(geometry_revision(&world), revision);
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
    fn hidden_root_rejects_stale_children_only_hit_targets() {
        let mut world = World::new();
        let root = world.spawn_empty();
        world.insert(root, Widget);
        world.insert(root, fixed_style(128, 128));
        let target = world.spawn_empty();
        world.insert(target, Widget);
        world.insert(target, fixed_style(32, 24));
        world.insert(target, HitTarget);
        world.insert(root, Children(vec![target]));
        assert!(world.get::<Parent>(target).is_none());

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::ui::render_system::try_update_layout(&mut world, root, &viewport).unwrap();
        assert_eq!(
            hit_test(&world, root, 4.into(), 4.into(), 128, 128),
            Some(target)
        );

        world.insert(root, crate::ui::Hidden);
        assert_eq!(hit_test(&world, root, 4.into(), 4.into(), 128, 128), None);
        crate::ui::render_system::try_update_layout(&mut world, root, &viewport).unwrap();
        assert_eq!(hit_test(&world, root, 4.into(), 4.into(), 128, 128), None);
    }

    #[test]
    fn hidden_targets_are_reserved_before_their_first_visible_layout() {
        let mut world = World::new();
        let root = world.spawn_empty();
        world.insert(root, Widget);
        world.insert(root, fixed_style(128, 128));
        world.insert(root, Children(Vec::new()));

        let visible = world.spawn_empty();
        world.insert(visible, Widget);
        world.insert(visible, fixed_style(20, 20));
        world.insert(visible, HitTarget);
        append(&mut world, root, visible);

        let hidden = world.spawn_empty();
        world.insert(hidden, Widget);
        world.insert(hidden, fixed_style(64, 32));
        world.insert(hidden, Children(Vec::new()));
        world.insert(hidden, crate::ui::Hidden);
        append(&mut world, root, hidden);
        for _ in 0..2 {
            let target = world.spawn_empty();
            world.insert(target, Widget);
            world.insert(target, fixed_style(16, 16));
            world.insert(target, HitTarget);
            append(&mut world, hidden, target);
        }

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        crate::ui::render_system::try_update_layout(&mut world, root, &viewport).unwrap();
        let geometry = world.resource::<HitTestGeometry>().unwrap();
        let reserved = geometry.entries.capacity();
        assert_eq!(geometry.entries.len(), 1);
        assert!(reserved >= 3);

        world.remove::<crate::ui::Hidden>(hidden);
        crate::ui::render_system::try_update_layout(&mut world, root, &viewport).unwrap();
        let geometry = world.resource::<HitTestGeometry>().unwrap();
        assert_eq!(geometry.entries.len(), 3);
        assert_eq!(geometry.entries.capacity(), reserved);
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

        crate::ui::render_system::try_update_layout(
            &mut world,
            root,
            &Viewport::new(128, 128, Fixed::ONE),
        )
        .unwrap();
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

        crate::ui::render_system::try_update_layout(
            &mut world,
            root,
            &Viewport::new(128, 128, Fixed::ONE),
        )
        .unwrap();
        assert_eq!(
            hit_test(&world, root, 35.into(), 10.into(), 128, 128),
            Some(target)
        );
        assert_eq!(hit_test(&world, root, 45.into(), 10.into(), 128, 128), None);
    }

    #[test]
    fn hit_geometry_matches_paint_for_scrolled_and_mixed_transforms() {
        let rotation = Transform::rotate_deg(Fixed::from_int(30));
        let projective =
            Transform3D::rotate_y_perspective(Fixed::from_int(25), Fixed::from_int(300));
        for (name, parent_affine, parent_projective, local_affine, local_projective) in [
            (
                "scrolled rotation",
                Transform::IDENTITY,
                Transform3D::IDENTITY,
                rotation,
                Transform3D::IDENTITY,
            ),
            (
                "affine ancestor",
                rotation,
                Transform3D::IDENTITY,
                Transform::IDENTITY,
                projective,
            ),
            (
                "projective ancestor",
                Transform::IDENTITY,
                projective,
                rotation,
                Transform3D::IDENTITY,
            ),
            (
                "combined local transforms",
                rotation,
                projective,
                rotation,
                projective,
            ),
        ] {
            let mut world = World::new();
            let root = world.spawn_empty();
            world.insert(root, Widget);
            world.insert(root, fixed_style(128, 128));
            world.insert(root, Children(Vec::new()));
            let parent = world.spawn_empty();
            world.insert(parent, Widget);
            world.insert(parent, fixed_style(110, 100));
            world.insert(parent, Children(Vec::new()));
            world.insert(parent, WidgetTransform(parent_affine));
            world.insert(parent, WidgetTransform3D(parent_projective));
            world.insert(parent, TransformOrigin::TOP_LEFT);
            world.insert(
                parent,
                ScrollOffset {
                    x: 7.into(),
                    y: 11.into(),
                },
            );
            append(&mut world, root, parent);
            let target = world.spawn_empty();
            world.insert(target, Widget);
            let mut style = fixed_style(24, 20);
            style.layout.position = crate::ui::layout::Position::Absolute;
            style.layout.left = Dimension::px(32);
            style.layout.top = Dimension::px(40);
            world.insert(target, style);
            world.insert(target, HitTarget);
            world.insert(target, WidgetTransform(local_affine));
            world.insert(target, WidgetTransform3D(local_projective));
            world.insert(target, TransformOrigin::TOP_LEFT);
            append(&mut world, parent, target);
            crate::ui::render_system::try_update_layout(
                &mut world,
                root,
                &Viewport::new(128, 128, Fixed::ONE),
            )
            .unwrap();

            let (rect, affine, projective) =
                crate::ui::geometry::widget_render_geometry(&world, target).unwrap();
            let expected = if projective.is_identity() {
                HitShape::TransformedRect {
                    rect,
                    inverse: affine.inverse().unwrap(),
                }
            } else {
                HitShape::Quad(projective.apply_rect(rect).unwrap())
            };
            let geometry = world.resource::<HitTestGeometry>().unwrap();
            let actual = geometry
                .entries
                .iter()
                .find(|entry| entry.entity == target)
                .unwrap();
            assert!(actual.shape == expected, "{name}: input and paint disagree");
            let local_center = Point {
                x: rect.x + rect.w / 2,
                y: rect.y + rect.h / 2,
            };
            let center = if projective.is_identity() {
                affine.apply_point(local_center)
            } else {
                projective.apply_point(local_center).unwrap()
            };
            assert_eq!(
                hit_test(&world, root, center.x, center.y, 128, 128),
                Some(target),
                "{name}"
            );
        }
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
        crate::ui::render_system::try_update_layout(&mut world, root, &viewport).unwrap();
        let revision = geometry_revision(&world);

        world.insert(
            target,
            WidgetTransform(Transform::translate(Fixed::from_int(40), Fixed::ZERO)),
        );
        world.insert(target, crate::ui::dirty::VisualDirty::New);
        let mut dirty = crate::ui::dirty::DirtyRegions::default();
        crate::ui::render_system::try_collect_dirty_regions_into(
            &mut world, root, &viewport, &mut dirty,
        )
        .unwrap();
        assert_ne!(geometry_revision(&world), revision);

        assert_eq!(hit_test(&world, root, 4.into(), 4.into(), 128, 128), None);
        assert_eq!(
            hit_test(&world, root, 44.into(), 4.into(), 128, 128),
            Some(target)
        );
    }
}
