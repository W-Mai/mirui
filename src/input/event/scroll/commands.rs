use crate::core::reactive::retained::{self, InputLease};
use crate::ecs::commands::DeferredCommands;
use crate::ecs::{CommandCapacity, Entity, World};
use crate::types::{Fixed, Rect, Transform3D};
use crate::ui::prepared_layout::{LayoutParticipant, PreparedLayout};
use crate::ui::property::{PropertyChange, invalidate_for_change};
use crate::ui::{Parent, Widget};

use super::{ScrollAxis, ScrollConfig, ScrollOffset};

/// A widget to reveal through its scrollable ancestors.
#[derive(Clone, Copy, Debug)]
pub enum ScrollTarget {
    Node(Entity),
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
        let ScrollTarget::Node(entity) = target;
        self.apply(entity, target, |world, _, target| {
            let ScrollTarget::Node(entity) = target;
            let mut parent = world.get::<Parent>(entity).map(|parent| parent.0);
            for _ in 0..world.live_entity_count() {
                let Some(container) = parent else { break };
                if world.has::<ScrollOffset>(container) {
                    accept(world, container, ScrollPosition::Node(entity), true);
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

#[derive(Clone, Copy)]
struct Request {
    position: ScrollPosition,
    reveal: bool,
    parent: Option<Entity>,
    container_parent: Option<Entity>,
    lease: Option<InputLease>,
    container_lease: Option<InputLease>,
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
    if !world.has::<ScrollOffset>(container) || !available(world, container) {
        return;
    }
    let node = match position {
        ScrollPosition::Node(node) => {
            if node == container
                || !available(world, node)
                || !crate::ui::branch::is_in_subtree(world, container, node)
            {
                return;
            }
            Some(node)
        }
        ScrollPosition::Offset(_) => None,
    };
    let request = Request {
        position,
        reveal,
        parent: node.and_then(|node| world.get::<Parent>(node).map(|parent| parent.0)),
        container_parent: world.get::<Parent>(container).map(|parent| parent.0),
        lease: node.and_then(|node| InputLease::capture(world, node)),
        container_lease: InputLease::capture(world, container),
    };
    if let Some(pending) = world.get_mut::<Pending>(container) {
        pending.0 = Some(request);
    } else {
        world.insert(container, Pending(Some(request)));
    }
    world.invalidate_visual(container);
}

pub(super) fn cancel(world: &mut World, container: Entity) {
    if world
        .get::<Pending>(container)
        .is_some_and(|pending| pending.0.is_some())
        && let Some(pending) = world.get_mut::<Pending>(container)
    {
        pending.0 = None;
    }
}

fn local_transform(world: &World, entity: Entity) -> Option<Transform3D> {
    let (rect, affine, projective) = crate::ui::geometry::widget_render_geometry(world, entity)?;
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
    let transform = local_transform(world, container)?
        .inverse()?
        .compose(&local_transform(world, node)?);
    Some(Rect::bounding_quad(
        &transform.apply_rect(Rect::new(0, 0, rect.w, rect.h))?,
    ))
}

fn displacement(start: Fixed, length: Fixed, viewport: Fixed, reveal: bool) -> Fixed {
    if !reveal || length > viewport || start < Fixed::ZERO {
        start
    } else {
        (start + length - viewport).max(Fixed::ZERO)
    }
}

fn execute(world: &mut World, container: Entity, request: Request) -> PropertyChange {
    if !available(world, container)
        || request.container_parent != world.get::<Parent>(container).map(|parent| parent.0)
        || request
            .container_lease
            .is_some_and(|lease| !lease.valid(world))
    {
        return PropertyChange::Unchanged;
    }
    let Some(bounds) = super::bounds::scroll_bounds(world, container) else {
        return PropertyChange::Unchanged;
    };
    let Some(current) = world.get::<ScrollOffset>(container).copied() else {
        return PropertyChange::Unchanged;
    };
    let desired = match request.position {
        ScrollPosition::Offset(offset) => offset,
        ScrollPosition::Node(node) => {
            if !available(world, node)
                || request.parent != world.get::<Parent>(node).map(|parent| parent.0)
                || request.lease.is_some_and(|lease| !lease.valid(world))
                || !crate::ui::branch::is_in_subtree(world, container, node)
            {
                return PropertyChange::Unchanged;
            }
            let Some(rect) = node_rect(world, container, node) else {
                return PropertyChange::Unchanged;
            };
            ScrollOffset {
                x: current.x + displacement(rect.x, rect.w, bounds.viewport_width, request.reveal),
                y: current.y + displacement(rect.y, rect.h, bounds.viewport_height, request.reveal),
            }
        }
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
        change
    }
    #[cfg(test)]
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

#[crate::view(component = ScrollOffset, name = "Scroll")]
fn scroll_view() {}

/// Register programmatic scrolling when installing widget capabilities individually.
pub fn view() -> crate::ui::View {
    scroll_view::view()
        .with_prepared_layout::<ScrollPreparation>()
        .with_retire(|world, entity| {
            world.remove::<Pending>(entity);
        })
}

#[cfg(test)]
mod tests;
