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
use geometry::ItemGeometry;
pub use item::ListItem;
use item::{CellItem, ItemHandle};

/// A virtual list whose cells are created on demand and retained across scrolling.
///
/// `item_extent` is the cell size along `ScrollConfig.direction` (vertical by default). Supply one `on CreateCell` callback and bind cell properties to `event.item.index()`.
#[crate::component]
#[derive(Default)]
pub struct LazyList {
    pub item_count: u32,
    pub item_extent: Fixed,
    /// First data index in the visible interval.
    pub visible_start: u32,
}

impl LazyList {
    #[allow(non_upper_case_globals)]
    pub const CreateCell: EventKey<CreateCell> = CreateCell::EVENT;

    pub fn new(item_count: u32, item_extent: impl Into<Fixed>) -> Self {
        Self {
            item_count,
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
#[derive(Default)]
struct CellPool(Vec<Slot>);
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
    let extent = ItemGeometry::new(list.item_count, list.item_extent)
        .map_or(Fixed::ZERO, |geometry| geometry.position(list.item_count));
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
}

fn cell_is_valid(world: &World, cell: Entity) -> bool {
    world.get::<CellItem>(cell).is_some_and(|item| {
        world.get::<LazyList>(item.list).is_some_and(|list| {
            item.handle.index() < list.item_count
                && ItemGeometry::new(list.item_count, list.item_extent).is_some()
        })
    })
}

fn configuration_matches(
    world: &World,
    entity: Entity,
    count: u32,
    extent: Fixed,
    is_horizontal: bool,
) -> bool {
    world
        .get::<LazyList>(entity)
        .is_some_and(|list| list.item_count == count && list.item_extent == extent)
        && horizontal(world, entity) == is_horizontal
}

fn update_list(
    world: &mut World,
    entity: Entity,
    slots: &mut Vec<PlannedSlot>,
    targets: &mut Vec<usize>,
) -> bool {
    let Some(list) = world.get::<LazyList>(entity) else {
        return false;
    };
    let (count, extent) = (list.item_count, list.item_extent);
    let geometry = ItemGeometry::new(count, extent)
        .unwrap_or_else(|| ItemGeometry::new(0, Fixed::ONE).expect("empty geometry"));
    let is_horizontal = horizontal(world, entity);
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
    slots.extend(pool.0.iter().map(|&slot| PlannedSlot {
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
    let mut changed = false;
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
        let Some(provider) = world
            .get::<ConstructionProvider<CreateCell>>(entity)
            .cloned()
        else {
            break;
        };
        let item = CellItem::new(world, entity, index);
        let request = CreateCell {
            item: item.handle.clone(),
        };
        let Some(cell) = provider.invoke(world, &request, entity) else {
            break;
        };
        if !world.is_alive(entity) || !world.is_alive(cell) {
            break;
        }
        if !configuration_matches(world, entity, count, extent, is_horizontal) {
            crate::ui::despawn_subtree(world, cell);
            return true;
        }
        world.insert(cell, item);
        world.reserve_component_storage::<Hidden>(
            cell.id,
            world.component_count(core::any::TypeId::of::<CellItem>()),
        );
        world.insert(
            cell,
            crate::core::reactive::retained::RetainedEffects {
                active: true,
                valid: cell_is_valid,
            },
        );
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
            world.get_mut::<CellPool>(entity).expect("attached pool").0[dead] = slot;
            slots[dead] = plan;
            *target = dead;
        } else {
            world
                .get_mut::<CellPool>(entity)
                .expect("attached pool")
                .0
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
        let rebound = planned.slot.index != planned.next;
        if rebound {
            crate::input::event::retire_subtree_interaction(world, cell);
            if !world.is_alive(cell)
                || !configuration_matches(world, entity, count, extent, is_horizontal)
            {
                return true;
            }
            world.get_mut::<CellPool>(entity).expect("attached pool").0[i].index = planned.next;
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
            item.set_index(index);
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
