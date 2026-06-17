use crate::ecs::{Entity, World};
use crate::input::event::scroll::ScrollOffset;
use crate::types::Fixed;
use crate::ui::ComputedRect;
use crate::ui::dirty::Dirty;
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
#[derive(crate::Component)]
pub struct LazyList {
    pub item_count: u32,
    pub item_height: Fixed,
    pub pool_size: u8,
    /// Updated by lazy_list_system each frame; the leftmost (topmost)
    /// row index currently bound to pool slot 0.
    pub visible_start: u32,
}

impl Default for LazyList {
    fn default() -> Self {
        Self {
            item_count: 0,
            item_height: Fixed::ZERO,
            pool_size: 0,
            visible_start: 0,
        }
    }
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
struct ListContext {
    item_count: u32,
    item_height: Fixed,
    pool_size: u32,
    visible_start: u32,
    visible_count: u32,
    items: Vec<Entity>,
    bound_indices: Vec<u32>,
    binder: ItemBinder,
}

/// Read the list's components into a snapshot used by `apply_bindings`.
/// Returns None if any required component is missing or the list has
/// degenerate parameters (zero pool, zero item height).
fn collect_list_context(world: &World, entity: Entity) -> Option<ListContext> {
    let (item_count, item_height, pool_size) = world
        .get::<LazyList>(entity)
        .map(|l| (l.item_count, l.item_height, l.pool_size as u32))?;
    if pool_size == 0 || item_height <= Fixed::ZERO {
        return None;
    }
    let scroll_y = world
        .get::<ScrollOffset>(entity)
        .map(|s| s.y)
        .unwrap_or(Fixed::ZERO);
    // ScrollOffset is added to children's y, so positive offset = list
    // scrolled down. visible_start = floor(scroll_y / item_height).
    let raw_start = (scroll_y / item_height).to_int();
    let visible_start = (raw_start.max(0) as u32).min(item_count.saturating_sub(pool_size));

    // +2 overscan covers a partially-revealed row at each edge; before
    // first layout (no ComputedRect) bind the full pool.
    let visible_count = match world.get::<ComputedRect>(entity).map(|r| r.0.h) {
        Some(h) if h > Fixed::ZERO => {
            ((h / item_height).ceil().to_int().max(0) as u32 + 2).min(pool_size)
        }
        _ => pool_size,
    };

    let pool = world.get::<LazyListPool>(entity)?;
    let binder = world.get::<LazyListBinder>(entity).map(|b| b.bind)?;
    Some(ListContext {
        item_count,
        item_height,
        pool_size,
        visible_start,
        visible_count,
        items: pool.items.clone(),
        bound_indices: pool.bound_indices.clone(),
        binder,
    })
}

/// Walk one list's pool and update both bindings and slot positions.
/// Returns true if at least one slot's binding changed (so the caller
/// can mark Dirty).
fn apply_bindings(world: &mut World, entity: Entity, ctx: ListContext) -> bool {
    let mut new_bindings = ctx.bound_indices;
    let mut any_changed = false;
    let pool_size = ctx.pool_size as usize;
    if pool_size == 0 {
        return false;
    }
    // Ring-buffer mapping `slot[target % pool_size] = target` so a
    // one-row scroll only rebinds one slot; the rest keep their
    // content and only their layout position moves.
    let active = (ctx.visible_count as usize).min(pool_size);
    for i in 0..active {
        let target = ctx.visible_start + i as u32;
        if target >= ctx.item_count {
            continue;
        }
        let slot_idx = (target as usize) % pool_size;
        let slot = ctx.items[slot_idx];
        let rebound = new_bindings[slot_idx] != target;
        if rebound {
            (ctx.binder)(world, slot, target);
            new_bindings[slot_idx] = target;
            any_changed = true;
        }
        // Reveal a slot that was previously Hidden (item_count grew
        // back, or this slot was unbound at startup with
        // pool_size > item_count and is now reused).
        if world.get::<crate::ui::Hidden>(slot).is_some() {
            world.remove::<crate::ui::Hidden>(slot);
            any_changed = true;
        }
        // Reposition-only slots ride the container's self-blit; only
        // rebound slots need a Dirty redraw at their new position.
        let y = ctx.item_height * Fixed::from_int(target as i32);
        if rebound {
            crate::ui::set_position(world, slot, Fixed::ZERO, y);
        } else {
            crate::ui::set_position_quiet(world, slot, Fixed::ZERO, y);
        }
    }
    if let Some(pool) = world.get_mut::<LazyListPool>(entity) {
        pool.bound_indices = new_bindings;
    }
    if let Some(list) = world.get_mut::<LazyList>(entity) {
        list.visible_start = ctx.visible_start;
    }
    any_changed
}

#[crate::system(order = LAZY_LIST, expect = LazyList)]
pub fn lazy_list_system(world: &mut World) {
    let lists: Vec<Entity> = world.query::<LazyList>().collect();
    for entity in lists {
        let Some(ctx) = collect_list_context(world, entity) else {
            continue;
        };
        // Always sweep slots whose target index is now beyond
        // `item_count` (pool larger than data, or `item_count` shrank
        // since the last bind). Those slots otherwise keep stale
        // bindings and stale positions, even when the idle short-
        // circuit below decides nothing has changed.
        let stale_cleared = clear_extra_slots(world, entity, &ctx);

        let prev_start = world
            .get::<LazyList>(entity)
            .map(|l| l.visible_start)
            .unwrap_or(u32::MAX);
        let pool_size = ctx.pool_size as usize;
        if prev_start == ctx.visible_start && pool_size > 0 {
            let active = (ctx.visible_count as usize).min(pool_size);
            let mut all_bound = true;
            for i in 0..active {
                let target = ctx.visible_start + i as u32;
                if target >= ctx.item_count {
                    continue;
                }
                let slot_idx = (target as usize) % pool_size;
                if ctx.bound_indices[slot_idx] != target {
                    all_bound = false;
                    break;
                }
            }
            if all_bound {
                if stale_cleared {
                    world.insert(entity, Dirty);
                }
                continue;
            }
        }
        let bindings_changed = apply_bindings(world, entity, ctx);
        if bindings_changed || stale_cleared {
            world.insert(entity, Dirty);
        }
    }
}

/// Returns true if any slot was newly hidden, so the caller marks Dirty.
fn clear_extra_slots(world: &mut World, entity: Entity, ctx: &ListContext) -> bool {
    let pool_size = ctx.pool_size as usize;
    if pool_size == 0 {
        return false;
    }
    let active = (ctx.visible_count as usize).min(pool_size);
    let in_window = |row: u32| {
        row >= ctx.visible_start && row < ctx.visible_start + active as u32 && row < ctx.item_count
    };
    let active_slot = |slot_idx: usize| {
        (0..active).any(|i| {
            let target = ctx.visible_start + i as u32;
            target < ctx.item_count && (target as usize) % pool_size == slot_idx
        })
    };
    let mut any_cleared = false;
    let mut new_bindings = ctx.bound_indices.clone();
    for (i, bound) in new_bindings.iter_mut().enumerate().take(pool_size) {
        let needs_hide = match *bound {
            u32::MAX => !active_slot(i),
            n => !in_window(n),
        };
        if !needs_hide {
            continue;
        }
        let slot = ctx.items[i];
        if world.get::<crate::ui::Hidden>(slot).is_some() {
            continue;
        }
        *bound = u32::MAX;
        any_cleared = true;
        // Hide via Hidden marker: layout / render / hit-test all skip.
        world.insert(slot, crate::ui::Hidden);
    }
    if any_cleared {
        if let Some(pool) = world.get_mut::<LazyListPool>(entity) {
            pool.bound_indices = new_bindings;
        }
    }
    any_cleared
}

pub fn view() -> crate::ui::view::View {
    crate::ui::view::View::systems_only("LazyList", const { &[lazy_list_system::system()] })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Dimension;
    use crate::ui::layout::{LayoutStyle, Position};
    use crate::ui::{Style, Widget};

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
        let e = world.spawn_empty();
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
        world.insert(e, crate::ui::Parent(list));
        e
    }

    #[test]
    fn first_tick_binds_all_slots_top_down() {
        let mut world = World::default();
        let list = world.spawn_empty();
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
        let list = world.spawn_empty();
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

        // After scrolling to visible_start=1 with pool_size=5, only
        // slot 0 (mapped from target=5) needs rebinding; targets 1..4
        // still occupy their original slots.
        let trace = &world.resource::<BindTrace>().unwrap().0;
        assert_eq!(
            trace.len(),
            1,
            "ring buffer rebinds one slot per row scrolled, got {trace:?}"
        );
        assert_eq!(trace, &alloc::vec![5u32]);
    }

    #[test]
    fn pool_larger_than_item_count_hides_extra_slots() {
        // 5-slot pool but only 3 items. Slots 3 and 4 must end up
        // Hidden so they don't paint stale (or empty) bindings.
        let mut world = World::default();
        let list = world.spawn_empty();
        let pool: Vec<Entity> = (0..5).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, Widget);
        world.insert(list, Style::default());
        world.insert(list, LazyList::new(3, 40, 5));
        world.insert(list, LazyListPool::new(pool.clone()));
        world.insert(
            list,
            LazyListBinder {
                bind: recording_binder,
            },
        );
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        lazy_list_system(&mut world);

        for &slot in &pool[..3] {
            assert!(
                world.get::<crate::ui::Hidden>(slot).is_none(),
                "first 3 slots must be visible (bound)"
            );
        }
        for &slot in &pool[3..] {
            assert!(
                world.get::<crate::ui::Hidden>(slot).is_some(),
                "extra slots beyond item_count must be Hidden"
            );
        }
    }

    #[test]
    fn shrinking_item_count_clears_stale_bindings() {
        // Bind all 5 rows, then shrink item_count to 3. Slots that
        // were bound to rows 3 / 4 must be cleared and Hidden.
        let mut world = World::default();
        let list = world.spawn_empty();
        let pool: Vec<Entity> = (0..5).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, Widget);
        world.insert(list, Style::default());
        world.insert(list, LazyList::new(5, 40, 5));
        world.insert(list, LazyListPool::new(pool.clone()));
        world.insert(
            list,
            LazyListBinder {
                bind: recording_binder,
            },
        );
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        lazy_list_system(&mut world);
        let bound_after_bind = world
            .get::<LazyListPool>(list)
            .unwrap()
            .bound_indices
            .clone();
        assert_eq!(bound_after_bind, alloc::vec![0u32, 1, 2, 3, 4]);

        // Shrink: item_count 5 → 3.
        if let Some(l) = world.get_mut::<LazyList>(list) {
            l.item_count = 3;
        }
        lazy_list_system(&mut world);

        let bound_after_shrink = &world.get::<LazyListPool>(list).unwrap().bound_indices;
        assert_eq!(
            bound_after_shrink,
            &alloc::vec![0u32, 1, 2, u32::MAX, u32::MAX],
            "rows 3 / 4 must be cleared from bound_indices",
        );
        assert!(world.get::<crate::ui::Hidden>(pool[3]).is_some());
        assert!(world.get::<crate::ui::Hidden>(pool[4]).is_some());
        assert!(world.get::<crate::ui::Hidden>(pool[0]).is_none());
    }

    #[test]
    fn growing_item_count_unhides_reused_slots() {
        let mut world = World::default();
        let list = world.spawn_empty();
        let pool: Vec<Entity> = (0..5).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, Widget);
        world.insert(list, Style::default());
        world.insert(list, LazyList::new(3, 40, 5));
        world.insert(list, LazyListPool::new(pool.clone()));
        world.insert(
            list,
            LazyListBinder {
                bind: recording_binder,
            },
        );
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        lazy_list_system(&mut world);
        assert!(world.get::<crate::ui::Hidden>(pool[3]).is_some());
        assert!(world.get::<crate::ui::Hidden>(pool[4]).is_some());

        if let Some(l) = world.get_mut::<LazyList>(list) {
            l.item_count = 5;
        }
        lazy_list_system(&mut world);

        for &slot in &pool {
            assert!(
                world.get::<crate::ui::Hidden>(slot).is_none(),
                "all slots should be visible once item_count grows back",
            );
        }
    }

    fn set_list_height(world: &mut World, list: Entity, h: i32) {
        world.insert(
            list,
            crate::ui::ComputedRect(crate::types::Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(100),
                h: Fixed::from_int(h),
            }),
        );
    }

    fn visible_count(world: &World, pool: &[Entity]) -> usize {
        pool.iter()
            .filter(|&&s| world.get::<crate::ui::Hidden>(s).is_none())
            .count()
    }

    #[test]
    fn visible_window_tracks_live_height() {
        let mut world = World::default();
        let list = world.spawn_empty();
        let pool: Vec<Entity> = (0..12).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, Widget);
        world.insert(list, Style::default());
        world.insert(list, LazyList::new(1000, 32, 12));
        world.insert(list, LazyListPool::new(pool.clone()));
        world.insert(
            list,
            LazyListBinder {
                bind: recording_binder,
            },
        );
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        // Short canvas: 64 px / 32 px = 2 rows + 2 overscan = 4 visible.
        set_list_height(&mut world, list, 64);
        lazy_list_system(&mut world);
        let narrow = visible_count(&world, &pool);
        assert_eq!(narrow, 4, "short canvas binds ceil(h/item)+2 rows");

        // Grow taller than pool*item_height: window saturates at the pool.
        set_list_height(&mut world, list, 600);
        lazy_list_system(&mut world);
        let wide = visible_count(&world, &pool);
        assert!(
            wide > narrow,
            "more rows must become visible as the list grows: {narrow} -> {wide}",
        );
        assert_eq!(wide, 12, "window is capped at the allocated pool");
    }
}
