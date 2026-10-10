use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use crate::ecs::{Entity, World};
use crate::input::event::contract::{Event, EventKey, WidgetConstruction, WidgetSource};
use crate::input::event::runtime::ConstructionProvider;
use crate::input::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::types::{Dimension, Fixed};
use crate::ui::dirty::Dirty;
use crate::ui::{ComputedRect, Hidden, Style};

mod geometry;
mod item;
mod items;
use geometry::ItemGeometry;
pub use item::ListItem;
use item::{CellItem, ItemHandle};
pub use items::ListItems;

/// A virtual list whose cells are created on demand and retained across scrolling.
///
/// `item_extent` is the cell size along `ScrollConfig.direction` (vertical by default). Supply one `on CreateCell` callback and bind cell properties to `event.item.index()`.
#[crate::component]
#[derive(Default)]
pub struct LazyList {
    pub items: ListItems,
    pub item_extent: Fixed,
    /// First data index in the visible interval.
    pub visible_start: u32,
}

impl LazyList {
    #[allow(non_upper_case_globals)]
    pub const CreateCell: EventKey<CreateCell> = CreateCell::EVENT;

    pub fn new(items: impl Into<ListItems>, item_extent: impl Into<Fixed>) -> Self {
        Self {
            items: items.into(),
            item_extent: item_extent.into(),
            visible_start: 0,
        }
    }
}

/// Construct one retained cell and return its new root entity.
///
/// Called only when the physical pool grows. Clone `item` into reactive properties and input callbacks; scrolling updates its observed index without repeating construction.
pub struct CreateCell {
    pub item: ItemHandle,
}

impl Event for CreateCell {
    type Route = WidgetConstruction;
    type Source = WidgetSource<LazyList>;
    type Options = ();
}

#[derive(Clone, Copy)]
struct Slot {
    entity: Entity,
    index: Option<u32>,
}
struct ListBinding {
    items: ListItems,
    provider: Option<ConstructionProvider<CreateCell>>,
    _subscription: Option<crate::core::model::ModelSubscription>,
}
#[derive(Default)]
struct CellPool {
    slots: Vec<Slot>,
    binding: Option<ListBinding>,
}
impl crate::ecs::Component for CellPool {}
#[derive(Clone, Copy)]
struct PlannedSlot {
    slot: Slot,
    next: Option<u32>,
    alive: bool,
}
#[derive(Default)]
struct ListScratch {
    lists: Vec<Entity>,
    slots: Vec<PlannedSlot>,
    targets: Vec<usize>,
}
#[derive(Clone, Default)]
struct ListRuntime(Rc<RefCell<ListScratch>>);

fn horizontal(world: &World, entity: Entity) -> bool {
    world
        .get::<ScrollConfig>(entity)
        .is_some_and(|config| config.direction == ScrollAxis::Horizontal)
}

pub(crate) fn content_extent(world: &World, entity: Entity) -> Option<(bool, Fixed)> {
    let list = world.get::<LazyList>(entity)?;
    let count = list.items.count(world).unwrap_or(0);
    let extent = ItemGeometry::new(count, list.item_extent)
        .map_or(Fixed::ZERO, |geometry| geometry.position(count));
    Some((horizontal(world, entity), extent))
}

fn attach_list(world: &mut World, entity: Entity) {
    if !world.has::<ScrollOffset>(entity) {
        world.insert(entity, ScrollOffset::default());
    }
    if !world.has::<ScrollConfig>(entity) {
        world.insert(entity, ScrollConfig::default());
    }
    if !world.has::<CellPool>(entity) {
        world.insert(entity, CellPool::default());
    }
    if let Some(config) = ListConfig::read(world, entity) {
        prepare_binding(world, entity, &config);
    }
}

fn cell_is_valid(world: &World, cell: Entity) -> bool {
    world.get::<CellItem>(cell).is_some_and(|item| {
        world.get::<LazyList>(item.list).is_some_and(|list| {
            list.items.count(world).is_ok_and(|count| {
                item.handle.index() < count
                    && list.items.id(item.handle.index()) == item.handle.id()
                    && ItemGeometry::new(count, list.item_extent).is_some()
                    && world
                        .get::<CellPool>(item.list)
                        .and_then(|pool| pool.binding.as_ref())
                        .is_some_and(|binding| binding.matches(world, item.list, &list.items))
            })
        })
    })
}

fn same_provider(
    left: Option<&ConstructionProvider<CreateCell>>,
    right: Option<&ConstructionProvider<CreateCell>>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.same_instance(right),
        (None, None) => true,
        _ => false,
    }
}

impl ListBinding {
    fn matches(&self, world: &World, entity: Entity, items: &ListItems) -> bool {
        self.items.same_source(items)
            && same_provider(
                self.provider.as_ref(),
                world.get::<ConstructionProvider<CreateCell>>(entity),
            )
    }
}

struct ListConfig {
    items: ListItems,
    count: u32,
    extent: Fixed,
    horizontal: bool,
    provider: Option<ConstructionProvider<CreateCell>>,
}

impl ListConfig {
    fn read(world: &World, entity: Entity) -> Option<Self> {
        let list = world.get::<LazyList>(entity)?;
        let count = match list.items.count(world) {
            Ok(count) => count,
            Err(error) => {
                crate::warn!("list source rejected: {}", error);
                return None;
            }
        };
        Some(Self {
            items: list.items.clone(),
            count,
            extent: list.item_extent,
            horizontal: horizontal(world, entity),
            provider: world
                .get::<ConstructionProvider<CreateCell>>(entity)
                .cloned(),
        })
    }

    fn matches(&self, world: &World, entity: Entity) -> bool {
        world.get::<LazyList>(entity).is_some_and(|list| {
            list.items == self.items
                && list.items.count(world).ok() == Some(self.count)
                && list.item_extent == self.extent
        }) && horizontal(world, entity) == self.horizontal
            && same_provider(
                self.provider.as_ref(),
                world.get::<ConstructionProvider<CreateCell>>(entity),
            )
    }
}

fn dispose_cells(world: &mut World, entity: Entity) {
    while let Some(slot) = world
        .get_mut::<CellPool>(entity)
        .and_then(|pool| pool.slots.pop())
    {
        crate::input::event::retire_subtree_interaction(world, slot.entity);
        crate::ui::despawn_subtree(world, slot.entity);
    }
    if let Some(pool) = world.get_mut::<CellPool>(entity) {
        pool.binding = None;
    }
}

fn prepare_binding(world: &mut World, entity: Entity, config: &ListConfig) -> bool {
    if world
        .get::<CellPool>(entity)
        .and_then(|pool| pool.binding.as_ref())
        .is_some_and(|binding| binding.matches(world, entity, &config.items))
    {
        return false;
    }
    let changed = world
        .get::<CellPool>(entity)
        .is_some_and(|pool| !pool.slots.is_empty());
    dispose_cells(world, entity);
    if !config.matches(world, entity) {
        return true;
    }
    let binding = ListBinding {
        _subscription: config.items.subscribe(world, entity),
        items: config.items.clone(),
        provider: config.provider.clone(),
    };
    if let Some(pool) = world.get_mut::<CellPool>(entity) {
        pool.binding = Some(binding);
    }
    changed
}

fn update_list(
    world: &mut World,
    entity: Entity,
    slots: &mut Vec<PlannedSlot>,
    targets: &mut Vec<usize>,
) -> bool {
    let Some(config) = ListConfig::read(world, entity) else {
        let changed = world
            .get::<CellPool>(entity)
            .is_some_and(|pool| !pool.slots.is_empty());
        dispose_cells(world, entity);
        return changed;
    };
    let mut changed = prepare_binding(world, entity, &config);
    if !config.matches(world, entity) {
        return true;
    }
    let (count, extent) = (config.count, config.extent);
    let geometry = ItemGeometry::new(count, extent)
        .unwrap_or_else(|| ItemGeometry::new(0, Fixed::ONE).expect("empty geometry"));
    let is_horizontal = config.horizontal;
    let viewport = world
        .get::<ComputedRect>(entity)
        .map_or(Fixed::ZERO, |rect| {
            if rect.0.w <= Fixed::ZERO || rect.0.h <= Fixed::ZERO {
                Fixed::ZERO
            } else if is_horizontal {
                rect.0.w
            } else {
                rect.0.h
            }
        });
    let offset = world
        .get::<ScrollOffset>(entity)
        .map_or(
            Fixed::ZERO,
            |offset| if is_horizontal { offset.x } else { offset.y },
        );
    let visible = geometry.visible(offset, viewport);
    let first = visible.start;
    let capacity = geometry.capacity(viewport);
    let retained = geometry.retained(visible, capacity);
    let Some(pool) = world.get::<CellPool>(entity) else {
        return false;
    };
    slots.clear();
    slots.extend(pool.slots.iter().map(|&slot| PlannedSlot {
        slot,
        next: None,
        alive: world.is_alive(slot.entity),
    }));
    targets.clear();
    targets.resize((retained.end - retained.start) as usize, usize::MAX);
    for (i, slot) in slots.iter_mut().enumerate() {
        if slot.alive
            && let Some(index) = slot.slot.index.filter(|index| retained.contains(index))
        {
            let target = &mut targets[(index - retained.start) as usize];
            if *target == usize::MAX {
                *target = i;
                slot.next = Some(index);
            }
        }
    }
    let mut free = 0;
    for (offset, target) in targets.iter_mut().enumerate() {
        if *target != usize::MAX {
            continue;
        }
        let index = retained.start + offset as u32;
        while free < slots.len() && (!slots[free].alive || slots[free].next.is_some()) {
            free += 1;
        }
        if free < slots.len() {
            slots[free].next = Some(index);
            *target = free;
            free += 1;
            continue;
        }
        let Some(provider) = &config.provider else {
            break;
        };
        let id = config.items.id(index);
        let item = CellItem::new(world, entity, index, id);
        let request = CreateCell {
            item: item.handle.clone(),
        };
        let cell = provider.invoke(world, &request, entity);
        if !config.matches(world, entity) || config.items.id(index) != id {
            if let Some(cell) = cell {
                crate::ui::despawn_subtree(world, cell);
            }
            return true;
        }
        let Some(cell) = cell else {
            break;
        };
        world.insert(cell, item);
        world.reserve_component_storage::<Hidden>(
            cell.id,
            world.component_count(core::any::TypeId::of::<CellItem>()),
        );
        let gate = crate::core::reactive::retained::RetainedEffects::new(world, cell_is_valid);
        world.insert(cell, gate);
        let slot = Slot {
            entity: cell,
            index: Some(index),
        };
        let plan = PlannedSlot {
            slot,
            next: Some(index),
            alive: true,
        };
        if let Some(dead) = slots.iter().position(|slot| !slot.alive) {
            world
                .get_mut::<CellPool>(entity)
                .expect("attached pool")
                .slots[dead] = slot;
            slots[dead] = plan;
            *target = dead;
        } else {
            world
                .get_mut::<CellPool>(entity)
                .expect("attached pool")
                .slots
                .push(slot);
            *target = slots.len();
            slots.push(plan);
        }
        free = slots.len();
        changed = true;
    }
    for (i, planned) in slots.iter().enumerate() {
        let cell = planned.slot.entity;
        if !planned.alive {
            continue;
        }
        let id = planned.next.and_then(|index| config.items.id(index));
        let rebound = planned.slot.index != planned.next
            || (planned.next.is_some()
                && world
                    .get::<CellItem>(cell)
                    .is_some_and(|item| item.handle.id() != id));
        if rebound {
            crate::input::event::retire_subtree_interaction(world, cell);
            if !world.is_alive(cell)
                || !config.matches(world, entity)
                || planned.next.and_then(|index| config.items.id(index)) != id
            {
                return true;
            }
            world
                .get_mut::<CellPool>(entity)
                .expect("attached pool")
                .slots[i]
                .index = planned.next;
            changed = true;
        }
        let Some(index) = planned.next else {
            if let Some(gate) =
                world.get_mut::<crate::core::reactive::retained::RetainedEffects>(cell)
            {
                gate.active = false;
            }
            if !world.has::<Hidden>(cell) {
                world.insert(cell, Hidden);
                changed = true;
            }
            continue;
        };
        if world.has::<Hidden>(cell) {
            world.remove::<Hidden>(cell);
            changed = true;
        }
        if let Some(gate) = world.get_mut::<crate::core::reactive::retained::RetainedEffects>(cell)
        {
            let was_active = gate.active;
            gate.active = true;
            if !was_active {
                crate::core::reactive::retained::wake(world, cell);
            }
        }
        if rebound && let Some(item) = world.get::<CellItem>(cell).map(|item| item.handle.clone()) {
            item.rebind(index, id);
        }
        let position = geometry.position(index);
        if let Some(style) = world.get_mut::<Style>(cell) {
            use crate::ui::layout::Position;
            let (x, y, width, height) = if is_horizontal {
                (
                    position,
                    Fixed::ZERO,
                    Dimension::Px(extent),
                    Dimension::percent(100),
                )
            } else {
                (
                    Fixed::ZERO,
                    position,
                    Dimension::percent(100),
                    Dimension::Px(extent),
                )
            };
            let layout = &mut style.layout;
            if layout.position != Position::Absolute
                || layout.left != Dimension::Px(x)
                || layout.top != Dimension::Px(y)
                || layout.width != width
                || layout.height != height
            {
                layout.position = Position::Absolute;
                layout.left = Dimension::Px(x);
                layout.top = Dimension::Px(y);
                layout.width = width;
                layout.height = height;
                world.insert(cell, Dirty);
                changed = true;
            }
        }
    }
    if let Some(list) = world.get_mut::<LazyList>(entity) {
        list.visible_start = first;
    }
    changed
}

pub(crate) fn reconcile(world: &mut World) -> bool {
    let runtime = match world.resource::<ListRuntime>() {
        Some(runtime) => runtime.clone(),
        None => {
            if world.query::<LazyList>().iter().next().is_none() {
                return false;
            }
            let runtime = ListRuntime::default();
            world.insert_resource(runtime.clone());
            runtime
        }
    };
    let Ok(mut scratch) = runtime.0.try_borrow_mut() else {
        return false;
    };
    let ListScratch {
        lists,
        slots,
        targets,
    } = &mut *scratch;
    world.query::<LazyList>().collect_into(lists);
    let mut changed = false;
    for &entity in lists.iter() {
        if !crate::core::reactive::retained::enabled(world, entity)
            || crate::ui::branch::is_hidden_in_tree(world, entity)
        {
            continue;
        }
        if update_list(world, entity, slots, targets) {
            if world.is_alive(entity) {
                world.insert(entity, Dirty);
            }
            changed = true;
        }
    }
    changed
}

pub(crate) fn lazy_list_system(world: &mut World) -> Result<(), crate::ecs::CommandError> {
    reconcile(world);
    Ok(())
}

crate::ecs::system::framework_system!(pub lazy_list_system, LazyList, expect = [LazyList]);
#[crate::view(component = LazyList, name = "LazyList", attach = attach_list, systems = [lazy_list_system::system()])]
fn lazy_list_view() {}
pub use lazy_list_view::view;

#[cfg(test)]
mod tests;
