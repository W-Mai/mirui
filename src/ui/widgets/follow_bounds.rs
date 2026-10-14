//! Layout bounds that follow another widget without occupying flex space.

use alloc::vec::Vec;

use crate::anim::Transition;
use crate::ecs::{Entity, World};
use crate::input::event::scroll::ScrollOffset;
use crate::types::{Dimension, Fixed, Point, Rect, Transform3D};
use crate::ui::layout::{LayoutNode, LayoutStyle, Position};
use crate::ui::outputs::graph::OutputGraph;
use crate::ui::{ComputedRect, LayoutGeometry, Outputs, Parent, View};

pub(crate) mod clip;
mod motion;

/// Positions this widget around a target's prepared bounds, in its own parent's coordinates.
///
/// Attach with `View(...) [FollowBounds::new(target)]`. The widget does not participate in flex sizing; its children still use normal layout. Missing, hidden, or cyclic targets hide the follower. Removing the component restores the authored layout style.
#[crate::component]
#[derive(Clone, Copy)]
pub struct FollowBounds {
    /// Source widget whose prepared geometry determines the follower's bounds.
    #[depends_on(LayoutGeometry)]
    pub target: Entity,
    /// Follow resolved content bounds instead of the border box.
    pub content: bool,
    /// Expand the mapped bounds on all sides; negative values inset them.
    pub outset: Fixed,
    /// Translation in the follower parent's coordinates.
    pub offset: Point,
    /// Animate subsequent bounds changes. `None` updates immediately.
    pub transition: Option<Transition>,
}

impl FollowBounds {
    /// Follow a target's border bounds without expansion or translation.
    pub const fn new(target: Entity) -> Self {
        Self {
            target,
            content: false,
            outset: Fixed::ZERO,
            offset: Point::ZERO,
            transition: None,
        }
    }

    /// Select content bounds after resolved padding instead of the border box.
    pub const fn with_content(mut self, content: bool) -> Self {
        self.content = content;
        self
    }

    /// Expand in the follower parent's logical coordinates; negative values inset.
    pub fn with_outset(mut self, outset: impl Into<Fixed>) -> Self {
        self.outset = outset.into();
        self
    }

    /// Translate in the follower parent's logical coordinates.
    pub fn with_offset(mut self, x: impl Into<Fixed>, y: impl Into<Fixed>) -> Self {
        self.offset = Point::new(x, y);
        self
    }

    /// Animate bounds changes after the first valid placement. A zero duration hint updates immediately.
    pub fn with_transition(mut self, transition: impl Into<Transition>) -> Self {
        self.transition = Some(transition.into());
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Resolved {
    rect: Rect,
    clip: Rect,
}

#[derive(Default)]
struct FollowState {
    geometry: Option<Resolved>,
    warned: bool,
}
impl crate::ecs::Component for FollowState {}

#[derive(Default)]
struct FollowPlan {
    entities: Vec<Entity>,
    graph: OutputGraph,
}

#[crate::view(component = FollowBounds, invalidation = Visual)]
fn follow_bounds() {}

/// Register layout following. Included by `App::with_default_widgets()`.
pub fn view() -> View {
    follow_bounds::view().with_reconcile(prepare).with_systems(
        const {
            &[crate::ecs::System::new(
                "follow_bounds_motion",
                crate::ecs::SystemSlot::Animation.priority(),
                motion::advance,
            )]
        } as &'static [crate::ecs::System],
    )
}

fn prepare(world: &mut World, entity: Entity) -> bool {
    let mut changed = motion::prepare(world, entity);
    if !world.has::<FollowState>(entity) {
        world.insert(entity, FollowState::default());
        changed = true;
    }
    if world.resource::<FollowPlan>().is_none() {
        world.insert_resource(FollowPlan::default());
    }
    changed
}

pub(crate) fn active(world: &World) -> bool {
    world.resource::<FollowPlan>().is_some()
        && world.query::<FollowBounds>().iter().next().is_some()
}

pub(crate) fn hidden(world: &World, entity: Entity) -> bool {
    world.has::<FollowBounds>(entity)
        && world
            .get::<FollowState>(entity)
            .is_some_and(|state| state.geometry.is_none())
}

pub(crate) fn clip(world: &World, entity: Entity) -> Option<Rect> {
    world.get::<FollowBounds>(entity)?;
    world
        .get::<FollowState>(entity)?
        .geometry
        .map(|geometry| geometry.clip)
}

pub(crate) fn layout_style(world: &World, entity: Entity, layout: &mut LayoutStyle) {
    if !world.has::<FollowBounds>(entity) {
        return;
    }
    let Some(state) = world.get::<FollowState>(entity) else {
        return;
    };
    let rect = state.geometry.map_or(Rect::ZERO, |geometry| geometry.rect);
    layout.position = Position::Absolute;
    layout.left = Dimension::Px(rect.x);
    layout.top = Dimension::Px(rect.y);
    layout.width = Dimension::Px(rect.w);
    layout.height = Dimension::Px(rect.h);
    layout.min_width = Dimension::Auto;
    layout.max_width = Dimension::Auto;
    layout.min_height = Dimension::Auto;
    layout.max_height = Dimension::Auto;
}

impl FollowPlan {
    fn build(&mut self, world: &mut World, entities: &[Entity]) {
        self.entities.clear();
        self.graph.clear();
        self.entities.extend(
            world
                .query::<FollowState>()
                .iter()
                .map(|(entity, _)| entity),
        );
        self.entities.retain(|&entity| {
            if !world.has::<FollowBounds>(entity) {
                world.remove::<FollowState>(entity);
                world.remove::<motion::FollowMotion>(entity);
                return false;
            }
            entities.contains(&entity)
        });
        for _ in &self.entities {
            self.graph.add_node();
        }
        let followers = &self.entities;
        self.graph.resolve_implicit(|producer, consumer| {
            let entity = followers[consumer];
            let source = followers[producer];
            let target = world.get::<FollowBounds>(entity).unwrap().target;
            let parent = world.get::<Parent>(entity).map(|parent| parent.0);
            crate::ui::branch::is_in_subtree(world, source, target)
                || parent
                    .is_some_and(|parent| crate::ui::branch::is_in_subtree(world, source, parent))
        });
    }
}

fn resolve(world: &World, entity: Entity) -> Option<Resolved> {
    let follow = world.get::<FollowBounds>(entity)?;
    let parent = world.get::<Parent>(entity)?.0;
    let source = world
        .resource::<Outputs<LayoutGeometry>>()?
        .get(follow.target)?;
    let (parent_rect, affine, projective) =
        crate::ui::geometry::widget_render_geometry(world, parent)?;
    let scroll = world
        .get::<ScrollOffset>(parent)
        .copied()
        .unwrap_or_default();
    let parent_transform = if projective.is_identity() {
        Transform3D::from(affine)
    } else {
        projective
    }
    .compose(&Transform3D::translate(
        parent_rect.x - scroll.x,
        parent_rect.y - scroll.y,
    ));
    let transform = parent_transform.inverse()?.compose(&source.transform());
    let local = if follow.content {
        source.content_rect()
    } else {
        source.border_rect()
    };
    let mut rect = Rect::bounding_quad(&transform.apply_rect(local)?).inflate(follow.outset);
    rect.w = rect.w.max(Fixed::ZERO);
    rect.h = rect.h.max(Fixed::ZERO);
    rect.x += follow.offset.x;
    rect.y += follow.offset.y;
    Some(Resolved {
        rect,
        clip: source.clip(),
    })
}

fn assign(world: &mut World, entity: Entity, geometry: Option<Resolved>) -> bool {
    let geometry = geometry.map(|mut geometry| {
        geometry.rect = motion::redirect(world, entity, geometry.rect);
        geometry
    });
    let state = world.get_mut::<FollowState>(entity).unwrap();
    if geometry.is_none() && !state.warned {
        crate::warn!(target: "mirui::layout", "FollowBounds has no valid target geometry: {:?}", entity);
    }
    state.warned = geometry.is_none();
    if state.geometry == geometry {
        return false;
    }
    let layout_changed = state.geometry.map(|value| value.rect) != geometry.map(|value| value.rect);
    state.geometry = geometry;
    if layout_changed {
        world.mark_subtree_dirty(entity);
    } else {
        world.invalidate_subtree_visual(entity);
    }
    layout_changed
}

fn node_mut(node: &mut LayoutNode, entity: Entity) -> Option<&mut LayoutNode> {
    if node.entity == Some(entity) {
        return Some(node);
    }
    node.children
        .iter_mut()
        .find_map(|node| node_mut(node, entity))
}

pub(crate) fn reconcile(
    world: &mut World,
    tree: &mut LayoutNode,
    entities: &[Entity],
    viewport: Rect,
) -> bool {
    world
        .with_resource_box::<FollowPlan, _>(|plan, world| {
            plan.build(world, entities);
            let mut changed = false;
            for (index, &entity) in plan.entities.iter().enumerate() {
                if plan.graph.is_blocked(index) {
                    changed |= assign(world, entity, None);
                }
            }
            crate::ui::layout_geometry::publish(world, tree, entities, viewport);
            for &index in plan.graph.order() {
                let entity = plan.entities[index];
                let geometry = resolve(world, entity);
                changed |= assign(world, entity, geometry);
                if let Some(geometry) = world.get::<FollowState>(entity).unwrap().geometry {
                    let parent = world.get::<Parent>(entity).unwrap().0;
                    let origin = world.get::<ComputedRect>(parent).unwrap().0;
                    let node = node_mut(tree, entity).unwrap();
                    layout_style(world, entity, &mut node.style);
                    crate::ui::layout::compute_layout(
                        node,
                        origin.x + geometry.rect.x,
                        origin.y + geometry.rect.y,
                        geometry.rect.w,
                        geometry.rect.h,
                    );
                    let mut position = entities
                        .iter()
                        .position(|&candidate| candidate == entity)
                        .unwrap();
                    crate::ui::render_system::write_computed_rects(
                        node,
                        world,
                        entities,
                        &mut position,
                    );
                }
                crate::ui::layout_geometry::publish(world, tree, entities, viewport);
            }
            changed
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests;
