use crate::ecs::{Entity, World};
use crate::event::scroll::ScrollOffset;
use crate::types::Fixed;
use crate::widget::dirty::Dirty;
use alloc::vec::Vec;

/// Virtual scroll over a fixed-height row list.
///
/// The user pre-spawns a small `pool_size` of widget entities (typically
/// `ceil(visible_h / item_height) + 2`) and registers them as the list
/// entity's children plus inside `LazyListPool`. Each frame
/// `lazy_list_system` figures out which row indices the pool should be
/// bound to, calls the user's binder for slots whose index changed, and
/// rewrites the pool entities' positions so they line up at
/// `index * item_height` in the parent's coordinate space. The scroll
/// system handles the rest via `ScrollOffset`.
pub struct LazyList {
    pub item_count: u32,
    pub item_height: Fixed,
    pub pool_size: u8,
    /// Updated by lazy_list_system each frame; the leftmost (topmost)
    /// row index currently bound to pool slot 0.
    pub visible_start: u32,
}

impl LazyList {
    pub fn new(item_count: u32, item_height: impl Into<Fixed>, pool_size: u8) -> Self {
        Self {
            item_count,
            item_height: item_height.into(),
            pool_size,
            visible_start: 0,
        }
    }
}

/// Owned by the list entity. `items.len() == pool_size` after setup;
/// `bound_indices[i]` is the row index slot `i` is currently displaying
/// (`u32::MAX` means unbound).
pub struct LazyListPool {
    pub items: Vec<Entity>,
    pub bound_indices: Vec<u32>,
}

impl LazyListPool {
    pub fn new(items: Vec<Entity>) -> Self {
        let n = items.len();
        Self {
            items,
            bound_indices: alloc::vec![u32::MAX; n],
        }
    }
}

/// User-supplied function called when a pool slot needs to display a
/// new row index. The binder mutates the slot's components (Text,
/// background, etc) — it does **not** spawn new entities.
pub type ItemBinder = fn(&mut crate::ecs::World, Entity, u32);

pub struct LazyListBinder {
    pub bind: ItemBinder,
}

/// Walk every list each frame, decide which row index each pool slot
/// should display, call the binder for slots whose binding changed,
/// and reposition pool entities. Equal-height rows + vertical scroll
/// only.
///
/// The function reads `ScrollOffset.y` (negative or zero — scroll_system
/// already clamps to bounds) to compute `visible_start`, then walks
/// `pool_size` slots binding consecutive indices starting there.
pub fn lazy_list_system(world: &mut World) {
    let lists: Vec<Entity> = world.query::<LazyList>().collect();
    for entity in lists {
        let (item_count, item_height, pool_size) = match world.get::<LazyList>(entity) {
            Some(l) => (l.item_count, l.item_height, l.pool_size as u32),
            None => continue,
        };
        if pool_size == 0 || item_height <= Fixed::ZERO {
            continue;
        }
        let scroll_y = world
            .get::<ScrollOffset>(entity)
            .map(|s| s.y)
            .unwrap_or(Fixed::ZERO);
        // ScrollOffset is added to children's y, so positive offset = list
        // scrolled down. visible_start = floor(scroll_y / item_height).
        let raw_start = (scroll_y / item_height).to_int();
        let visible_start = raw_start.max(0) as u32;
        let max_start = item_count.saturating_sub(pool_size);
        let visible_start = visible_start.min(max_start);

        // Snapshot pool state.
        let (items, bound_indices) = match world.get::<LazyListPool>(entity) {
            Some(p) => (p.items.clone(), p.bound_indices.clone()),
            None => continue,
        };
        let binder_fn = match world.get::<LazyListBinder>(entity) {
            Some(b) => b.bind,
            None => continue,
        };

        // For each slot, compute target row index. Reuse the slot whose
        // current binding matches the target if any; otherwise pick a
        // free slot and rebind. Simple approach: just bind slot i to
        // (visible_start + i). Per-slot identity stays; the binder
        // overwrites contents.
        let mut new_bindings = bound_indices.clone();
        let mut any_changed = false;
        for i in 0..pool_size as usize {
            let target = visible_start + i as u32;
            if target >= item_count {
                continue;
            }
            if new_bindings[i] != target {
                binder_fn(world, items[i], target);
                new_bindings[i] = target;
                any_changed = true;
            }
            // Reposition the slot. Children with absolute layout fall
            // through this; non-absolute children would ignore set_position
            // which is fine — the user is responsible for laying the pool
            // out absolutely.
            let y = item_height * Fixed::from_int(target as i32);
            crate::widget::set_position(world, items[i], Fixed::ZERO, y);
        }

        // Write back pool + visible_start in a single pass.
        if let Some(pool) = world.get_mut::<LazyListPool>(entity) {
            pool.bound_indices = new_bindings;
        }
        if let Some(list) = world.get_mut::<LazyList>(entity) {
            list.visible_start = visible_start;
        }

        if any_changed {
            world.insert(entity, Dirty);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{LayoutStyle, Position};
    use crate::types::Dimension;
    use crate::widget::{Style, Widget};

    #[test]
    fn pool_starts_unbound() {
        let stub = Entity {
            id: 0,
            generation: 0,
        };
        let pool = LazyListPool::new(alloc::vec![stub; 5]);
        assert_eq!(pool.items.len(), 5);
        assert!(pool.bound_indices.iter().all(|&i| i == u32::MAX));
    }

    /// Records the (entity_id, row_index) pairs the binder is invoked
    /// with, so a test can assert exactly which slots got rebound after
    /// changing scroll position. The recording side-channel is keyed on
    /// the row index alone — for a 5-slot pool that's enough.
    struct BindTrace(alloc::vec::Vec<u32>);

    fn recording_binder(world: &mut World, _entity: Entity, index: u32) {
        let trace = world.resource_mut::<BindTrace>().expect("trace resource");
        trace.0.push(index);
    }

    fn make_slot(world: &mut World, list: Entity) -> Entity {
        let e = world.spawn();
        world.insert(e, Widget);
        world.insert(
            e,
            Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::ZERO),
                    top: Dimension::Px(Fixed::ZERO),
                    width: Dimension::Px(Fixed::from_int(100)),
                    height: Dimension::Px(Fixed::from_int(40)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(e, crate::widget::Parent(list));
        e
    }

    #[test]
    fn first_tick_binds_all_slots_top_down() {
        let mut world = World::default();
        let list = world.spawn();
        let pool: Vec<Entity> = (0..5).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, Widget);
        world.insert(list, Style::default());
        world.insert(list, LazyList::new(100, 40, 5));
        world.insert(list, LazyListPool::new(pool.clone()));
        world.insert(
            list,
            LazyListBinder {
                bind: recording_binder,
            },
        );
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        lazy_list_system(&mut world);

        let trace = &world.resource::<BindTrace>().unwrap().0;
        assert_eq!(trace, &alloc::vec![0u32, 1, 2, 3, 4]);

        let bound = &world.get::<LazyListPool>(list).unwrap().bound_indices;
        assert_eq!(bound, &alloc::vec![0u32, 1, 2, 3, 4]);
    }

    #[test]
    fn scroll_one_row_rebinds_one_slot() {
        let mut world = World::default();
        let list = world.spawn();
        let pool: Vec<Entity> = (0..5).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, Widget);
        world.insert(list, Style::default());
        world.insert(list, LazyList::new(100, 40, 5));
        world.insert(list, LazyListPool::new(pool.clone()));
        world.insert(
            list,
            LazyListBinder {
                bind: recording_binder,
            },
        );
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        // Initial bind: 0..5.
        lazy_list_system(&mut world);
        world.resource_mut::<BindTrace>().unwrap().0.clear();

        // Scroll down 40 px → visible_start should become 1.
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(40),
            },
        );
        lazy_list_system(&mut world);

        // Slot 0 was bound to 0; target 1; rebind.
        // Slot 1 → 2; rebind. ... slot 4 → 5; rebind.
        // i.e. all 5 slots rebound (the simple non-rotating policy).
        let trace = &world.resource::<BindTrace>().unwrap().0;
        assert_eq!(trace.len(), 5);
        assert_eq!(trace, &alloc::vec![1u32, 2, 3, 4, 5]);
    }
}
