use crate::core::model::ModelItemId;
use crate::core::reactive::retained::{self, InputLease};
use crate::ecs::commands::DeferredCommands;
use crate::ecs::{CommandCapacity, Entity, World};
use crate::types::{Fixed, Rect, Transform3D};
use crate::ui::prepared_layout::{LayoutParticipant, PreparedLayout};
use crate::ui::property::{PropertyChange, invalidate_for_change};
use crate::ui::{Parent, Widget};

use super::{ScrollAxis, ScrollConfig, ScrollOffset};

mod animation;

/// A widget to reveal through its scrollable ancestors.
#[derive(Clone, Copy, Debug)]
pub enum ScrollTarget {
    Node(Entity),
    Index { list: Entity, index: u32 },
    Item { list: Entity, id: ModelItemId },
}

impl From<(Entity, u32)> for ScrollTarget {
    fn from((list, index): (Entity, u32)) -> Self {
        Self::Index { list, index }
    }
}
impl From<(Entity, ModelItemId)> for ScrollTarget {
    fn from((list, id): (Entity, ModelItemId)) -> Self {
        Self::Item { list, id }
    }
}

impl From<Entity> for ScrollTarget {
    fn from(entity: Entity) -> Self {
        Self::Node(entity)
    }
}

/// A content position or descendant to align inside a scroll container.
#[derive(Clone, Copy, Debug)]
pub enum ScrollPosition {
    Offset(ScrollOffset),
    Node(Entity),
    Index(u32),
    Item(ModelItemId),
}

impl From<u32> for ScrollPosition {
    fn from(index: u32) -> Self {
        Self::Index(index)
    }
}
impl From<ModelItemId> for ScrollPosition {
    fn from(id: ModelItemId) -> Self {
        Self::Item(id)
    }
}

impl From<Entity> for ScrollPosition {
    fn from(entity: Entity) -> Self {
        Self::Node(entity)
    }
}
impl From<ScrollOffset> for ScrollPosition {
    fn from(offset: ScrollOffset) -> Self {
        Self::Offset(offset)
    }
}

impl DeferredCommands<'_> {
    /// Reveal a widget with the smallest movement of its scrollable ancestors.
    pub fn scroll_into(&mut self, target: impl Into<ScrollTarget>) {
        let target = target.into();
        let entity = match target {
            ScrollTarget::Node(entity) => entity,
            ScrollTarget::Index { list, .. } | ScrollTarget::Item { list, .. } => list,
        };
        self.apply(entity, target, |world, _, target| {
            let (scope, position, mut parent) = match target {
                ScrollTarget::Node(entity) => (
                    entity,
                    ScrollPosition::Node(entity),
                    world.get::<Parent>(entity).map(|parent| parent.0),
                ),
                ScrollTarget::Index { list, index } => {
                    (list, ScrollPosition::Index(index), Some(list))
                }
                ScrollTarget::Item { list, id } => (list, ScrollPosition::Item(id), Some(list)),
            };
            for _ in 0..world.live_entity_count() {
                let Some(container) = parent else { break };
                if world.has::<ScrollOffset>(container) {
                    accept_scoped(world, container, scope, position, true);
                }
                parent = world.get::<Parent>(container).map(|parent| parent.0);
            }
            Ok(())
        });
    }

    /// Position one container after layout, clamping to its content bounds.
    pub fn scroll_to(&mut self, container: Entity, position: impl Into<ScrollPosition>) {
        self.apply(container, position.into(), |world, container, position| {
            accept(world, container, position, false);
            Ok(())
        });
    }
}

impl CommandCapacity {
    /// Reserve reveal requests in one authored callback.
    pub const fn scroll_into(self, count: usize) -> Self {
        self.entity_value::<ScrollTarget>(count)
    }
    /// Reserve explicit scroll positions in one authored callback.
    pub const fn scroll_to(self, count: usize) -> Self {
        self.entity_value::<ScrollPosition>(count)
    }
}

struct Request {
    target: RequestTarget,
    reveal: bool,
    container_parent: Option<Entity>,
}

enum RequestTarget {
    Offset {
        offset: ScrollOffset,
        lease: Option<InputLease>,
    },
    Node(TargetLease),
    Item(TargetLease, crate::ui::widgets::lazy_list::ScrollItem),
}

struct TargetLease {
    entity: Entity,
    parent: Option<Entity>,
    retained: Option<InputLease>,
}

impl TargetLease {
    fn capture(world: &World, container: Entity, entity: Entity) -> Option<Self> {
        (available(world, entity) && crate::ui::branch::is_in_subtree(world, container, entity))
            .then(|| Self {
                entity,
                parent: world.get::<Parent>(entity).map(|parent| parent.0),
                retained: InputLease::capture(world, entity),
            })
    }

    fn valid(&self, world: &World, container: Entity) -> bool {
        available(world, self.entity)
            && self.parent == world.get::<Parent>(self.entity).map(|parent| parent.0)
            && self.retained.is_none_or(|lease| lease.valid(world))
            && crate::ui::branch::is_in_subtree(world, container, self.entity)
    }
}

#[derive(Default)]
struct Pending(Option<Request>);
impl crate::ecs::Component for Pending {}

fn available(world: &World, entity: Entity) -> bool {
    world.is_alive(entity)
        && world.has::<Widget>(entity)
        && !crate::ui::branch::is_hidden_in_tree(world, entity)
        && retained::enabled(world, entity)
}

fn accept(world: &mut World, container: Entity, position: ScrollPosition, reveal: bool) {
    accept_scoped(world, container, container, position, reveal);
}

fn accept_scoped(
    world: &mut World,
    container: Entity,
    scope: Entity,
    position: ScrollPosition,
    reveal: bool,
) {
    if !world.has::<ScrollOffset>(container) || !available(world, container) {
        return;
    }
    let target = match position {
        ScrollPosition::Node(node) => {
            if node == container {
                return;
            }
            let Some(lease) = TargetLease::capture(world, container, node) else {
                return;
            };
            RequestTarget::Node(lease)
        }
        ScrollPosition::Offset(offset) => RequestTarget::Offset {
            offset,
            lease: InputLease::capture(world, container),
        },
        ScrollPosition::Index(index) => {
            let Some(lease) = TargetLease::capture(world, container, scope) else {
                return;
            };
            let Some(item) =
                crate::ui::widgets::lazy_list::ScrollItem::from_index(world, scope, index)
            else {
                return;
            };
            RequestTarget::Item(lease, item)
        }
        ScrollPosition::Item(id) => {
            let Some(lease) = TargetLease::capture(world, container, scope) else {
                return;
            };
            let Some(item) = crate::ui::widgets::lazy_list::ScrollItem::from_id(world, scope, id)
            else {
                return;
            };
            RequestTarget::Item(lease, item)
        }
    };
    let request = Request {
        target,
        reveal,
        container_parent: world.get::<Parent>(container).map(|parent| parent.0),
    };
    if let Some(pending) = world.get_mut::<Pending>(container) {
        pending.0 = Some(request);
    } else {
        world.insert(container, Pending(Some(request)));
    }
    world.invalidate_visual(container);
}

pub(super) fn cancel(world: &mut World, container: Entity) {
    animation::cancel(world, container);
    if world
        .get::<Pending>(container)
        .is_some_and(|pending| pending.0.is_some())
        && let Some(pending) = world.get_mut::<Pending>(container)
    {
        pending.0 = None;
    }
}

pub(super) fn cancel_chain(world: &mut World, mut cursor: Option<Entity>) {
    for _ in 0..world.live_entity_count() {
        let Some(entity) = cursor else { break };
        cancel(world, entity);
        cursor = world.get::<Parent>(entity).map(|parent| parent.0);
    }
}

fn local_transform(world: &World, entity: Entity, container: Entity) -> Option<Transform3D> {
    let (rect, affine, projective) =
        crate::ui::geometry::widget_geometry_with_scroll(world, entity, &|node| {
            if node != container
                && let Some(offset) = animation::destination(world, node)
            {
                return offset;
            }
            world.get::<ScrollOffset>(node).copied().unwrap_or_default()
        })?;
    Some(
        if projective.is_identity() {
            Transform3D::from(affine)
        } else {
            projective
        }
        .compose(&Transform3D::translate(rect.x, rect.y)),
    )
}

fn node_rect(world: &World, container: Entity, node: Entity) -> Option<Rect> {
    let rect = world.get::<crate::ui::ComputedRect>(node)?.0;
    relative_rect(world, container, node, Rect::new(0, 0, rect.w, rect.h))
}

fn relative_rect(world: &World, container: Entity, node: Entity, rect: Rect) -> Option<Rect> {
    let transform = local_transform(world, container, container)?
        .inverse()?
        .compose(&local_transform(world, node, container)?);
    Some(Rect::bounding_quad(&transform.apply_rect(rect)?))
}

#[derive(Clone, Copy)]
enum Edge {
    Keep,
    Leading,
    Trailing,
}

struct Goal {
    offset: ScrollOffset,
    edges: [Edge; 2],
    bounds: super::ScrollBounds,
    direction: ScrollAxis,
}

fn edge(start: Fixed, length: Fixed, viewport: Fixed, reveal: bool) -> Edge {
    if !reveal || length > viewport || start < Fixed::ZERO {
        Edge::Leading
    } else if start + length > viewport {
        Edge::Trailing
    } else {
        Edge::Keep
    }
}

fn displacement(edge: Edge, start: Fixed, length: Fixed, viewport: Fixed) -> Fixed {
    match edge {
        Edge::Keep => Fixed::ZERO,
        Edge::Leading => start,
        Edge::Trailing => start + length - viewport,
    }
}

fn goal(
    world: &World,
    container: Entity,
    request: &Request,
    edges: Option<[Edge; 2]>,
) -> Option<Goal> {
    if !available(world, container)
        || request.container_parent != world.get::<Parent>(container).map(|parent| parent.0)
    {
        return None;
    }
    let bounds = super::bounds::scroll_bounds(world, container)?;
    let current = world.get::<ScrollOffset>(container).copied()?;
    let rect = match &request.target {
        RequestTarget::Offset { lease, .. } => {
            if lease.is_some_and(|lease| !lease.valid(world)) {
                return None;
            }
            None
        }
        RequestTarget::Node(lease) => {
            if !lease.valid(world, container) {
                return None;
            }
            Some(node_rect(world, container, lease.entity)?)
        }
        RequestTarget::Item(lease, item) => {
            if !lease.valid(world, container) {
                return None;
            }
            let scope = lease.entity;
            let mut rect = item.rect(world, scope)?;
            if scope != container
                && let Some(destination) = animation::destination(world, scope)
            {
                let actual = world.get::<ScrollOffset>(scope)?;
                rect.x += actual.x - destination.x;
                rect.y += actual.y - destination.y;
            }
            Some(relative_rect(world, container, scope, rect)?)
        }
    };
    let edges = edges.unwrap_or_else(|| {
        rect.map_or([Edge::Keep; 2], |rect| {
            [
                edge(rect.x, rect.w, bounds.viewport_width, request.reveal),
                edge(rect.y, rect.h, bounds.viewport_height, request.reveal),
            ]
        })
    });
    let desired = match (&request.target, rect) {
        (RequestTarget::Offset { offset, .. }, _) => *offset,
        (_, Some(rect)) => ScrollOffset {
            x: current.x + displacement(edges[0], rect.x, rect.w, bounds.viewport_width),
            y: current.y + displacement(edges[1], rect.y, rect.h, bounds.viewport_height),
        },
        _ => return None,
    };
    let direction = world
        .get::<ScrollConfig>(container)
        .map_or(ScrollAxis::Vertical, |config| config.direction);
    let next = ScrollOffset {
        x: if direction == ScrollAxis::Vertical {
            current.x
        } else {
            desired.x.clamp(Fixed::ZERO, bounds.max_x)
        },
        y: if direction == ScrollAxis::Horizontal {
            current.y
        } else {
            desired.y.clamp(Fixed::ZERO, bounds.max_y)
        },
    };
    Some(Goal {
        offset: next,
        edges,
        bounds,
        direction,
    })
}

fn execute(world: &mut World, container: Entity, request: Request) -> PropertyChange {
    let Some(goal) = goal(world, container, &request, None) else {
        return PropertyChange::Unchanged;
    };
    let next = goal.offset;
    if world.get::<ScrollOffset>(container) == Some(&next) {
        return PropertyChange::Unchanged;
    }
    if animation::redirect(world, container, request, next, goal.edges) {
        return PropertyChange::Transform;
    }
    if super::system::set_position(world, container, next) {
        invalidate_for_change(world, container, PropertyChange::Transform);
        PropertyChange::Transform
    } else {
        PropertyChange::Unchanged
    }
}

#[derive(Default)]
struct ScrollPreparation;

impl LayoutParticipant for ScrollPreparation {
    fn active(world: &World) -> bool {
        world.has_any_by_id(core::any::TypeId::of::<Pending>())
    }
    fn needed(world: &World) -> bool {
        world
            .storage::<Pending>()
            .is_some_and(|storage| storage.iter().any(|(_, pending)| pending.0.is_some()))
            || animation::any_active(world)
    }
    fn prepare(&mut self, _: &World, _: &[Entity]) -> bool {
        false
    }
    fn has_geometry(&self) -> bool {
        false
    }
    fn needs_geometry(&self, _: Entity) -> bool {
        false
    }
    fn invalidate(&mut self) {}
    fn reconcile(&mut self, world: &mut World, _: &mut PreparedLayout<'_>) -> PropertyChange {
        let mut change = PropertyChange::Unchanged;
        // Resolve inner viewports before their ancestors without retaining another work queue.
        loop {
            let next = world.storage::<Pending>().and_then(|storage| {
                storage
                    .entities()
                    .iter()
                    .copied()
                    .filter(|&entity| {
                        world
                            .get::<Pending>(entity)
                            .is_some_and(|pending| pending.0.is_some())
                    })
                    .max_by_key(|&entity| {
                        let mut cursor = entity;
                        for depth in 0..world.live_entity_count() {
                            let Some(parent) = world.get::<Parent>(cursor) else {
                                return depth;
                            };
                            cursor = parent.0;
                        }
                        world.live_entity_count()
                    })
            });
            let Some(container) = next else { break };
            let request = world
                .get_mut::<Pending>(container)
                .unwrap()
                .0
                .take()
                .unwrap();
            change = change.merge(execute(world, container, request));
        }
        change.merge(animation::reconcile(world))
    }
    #[cfg(test)]
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[crate::view(component = ScrollOffset, name = "Scroll", systems = [animation::programmatic_scroll_system::system()])]
fn scroll_view() {}

/// Register programmatic scrolling when installing widget capabilities individually.
pub fn view() -> crate::ui::View {
    scroll_view::view()
        .with_prepared_layout::<ScrollPreparation>()
        .with_reconcile(animation::prepare)
        .with_retire(|world, entity| {
            world.remove::<Pending>(entity);
            world.remove::<animation::ProgrammaticMotion>(entity);
        })
}

#[cfg(test)]
mod tests;

pub(super) fn active(world: &World, entity: Entity) -> bool {
    animation::active(world, entity)
}
