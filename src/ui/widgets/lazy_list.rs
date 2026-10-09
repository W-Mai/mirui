use crate::ecs::{Entity, World};
use crate::input::event::contract::{Event, EventKey, WidgetSource, WidgetUpdate};
use crate::input::event::scroll::ScrollOffset;
use crate::types::Fixed;
use crate::ui::ComputedRect;
use crate::ui::dirty::Dirty;
use alloc::vec::Vec;

mod geometry;
use geometry::ItemGeometry;

/// Virtual scroll over a fixed-height row list.
///
/// Pre-spawn `pool_size` widget entities (typically `ceil(visible_h / item_height) + 2`) as the list entity's children. The first system run snapshots those children into `LazyListPool` unless an explicit pool is already installed. Each frame `lazy_list_system` binds slots whose row index changed and positions them at `index * item_height`. The scroll system applies `ScrollOffset`.
#[crate::component]
pub struct LazyList {
    pub item_count: u32,
    pub item_height: Fixed,
    pub pool_size: u8,
    /// First data index in the current visible window.
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
    #[allow(non_upper_case_globals)]
    pub const BindItem: EventKey<BindItem> = BindItem::EVENT;

    pub fn new(item_count: u32, item_height: impl Into<Fixed>, pool_size: u8) -> Self {
        Self {
            item_count,
            item_height: item_height.into(),
            pool_size,
            visible_start: 0,
        }
    }
}

/// Owned by the list entity. `items.len() >= pool_size` is required; `bound_indices[i]` is the row index slot `i` is currently displaying (`u32::MAX` means unbound).
///
/// An explicit pool selects its slots independently of `Children`. Automatically created pools retain the initial child order and do not follow subsequent changes to `Children`.
pub struct LazyListPool {
    pub items: Vec<Entity>,
    pub bound_indices: Vec<u32>,
}

impl crate::ecs::Component for LazyListPool {}

impl LazyListPool {
    pub fn new(items: Vec<Entity>) -> Self {
        let n = items.len();
        Self {
            items,
            bound_indices: alloc::vec![u32::MAX; n],
        }
    }
}

/// Update a retained row's content before layout when its data index changes.
///
/// The event source is the list entity; `item` identifies its reused row slot. Callbacks run synchronously after the full pool's positions and bindings are updated. Unchanged indices are not replayed, including for listeners installed after binding. This event does not bubble or consume business notification storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindItem {
    pub item: Entity,
    pub index: u32,
}

impl Event for BindItem {
    type Route = WidgetUpdate;
    type Source = WidgetSource<LazyList>;
    type Options = ();
}

struct ListContext {
    geometry: ItemGeometry,
    pool_size: usize,
    visible_start: u32,
    retained: core::ops::Range<u32>,
    items: Vec<Entity>,
    bound_indices: Vec<u32>,
}

fn ensure_missing_pool(world: &mut World, entity: Entity) {
    if world.has::<LazyListPool>(entity) {
        return;
    }
    let Some(list) = world.get::<LazyList>(entity) else {
        return;
    };
    let pool_size = list.pool_size as usize;
    if pool_size == 0 || list.item_height <= Fixed::ZERO {
        return;
    }
    let Some(children) = world.get::<crate::ui::Children>(entity) else {
        return;
    };
    if children.0.len() < pool_size {
        return;
    }
    let items = children.0.clone();
    world.insert(entity, LazyListPool::new(items));
}

fn collect_list_context(world: &World, entity: Entity) -> Option<ListContext> {
    let list = world.get::<LazyList>(entity)?;
    let (item_count, item_height, pool_size) =
        (list.item_count, list.item_height, list.pool_size as usize);
    if pool_size == 0 || item_height <= Fixed::ZERO {
        return None;
    }
    let scroll_y = world
        .get::<ScrollOffset>(entity)
        .map(|s| s.y)
        .unwrap_or(Fixed::ZERO);
    let Some(geometry) = ItemGeometry::new(item_count, item_height) else {
        crate::warn!(target: "mirui::lazy_list", "content extent exceeds layout coordinates");
        return None;
    };
    let viewport = match world.get::<ComputedRect>(entity) {
        Some(rect) if rect.0.w > Fixed::ZERO => rect.0.h,
        Some(_) => Fixed::ZERO,
        None => geometry.position((pool_size as u32).min(item_count)),
    };
    let visible = geometry.visible(scroll_y, viewport);
    let visible_start = visible.start;
    let retained = geometry.retained(visible, pool_size as u32);

    let pool = world.get::<LazyListPool>(entity)?;
    if pool.items.len() < pool_size || pool.bound_indices.len() < pool_size {
        return None;
    }
    Some(ListContext {
        geometry,
        pool_size,
        visible_start,
        retained,
        items: pool.items.clone(),
        bound_indices: pool.bound_indices.clone(),
    })
}

fn apply_bindings(world: &mut World, entity: Entity, ctx: &ListContext) -> bool {
    let mut changed = false;
    for slot_idx in 0..ctx.pool_size {
        let relative_slot = (slot_idx + ctx.pool_size
            - ctx.retained.start as usize % ctx.pool_size)
            % ctx.pool_size;
        let active_slot = relative_slot < (ctx.retained.end - ctx.retained.start) as usize;
        if active_slot {
            continue;
        }
        if ctx.bound_indices[slot_idx] != u32::MAX {
            world.get_mut::<LazyListPool>(entity).unwrap().bound_indices[slot_idx] = u32::MAX;
            changed = true;
        }
        let item = ctx.items[slot_idx];
        if world.is_alive(item) && !world.has::<crate::ui::Hidden>(item) {
            world.insert(item, crate::ui::Hidden);
            changed = true;
        }
    }

    // Ring mapping retains unchanged row content when the window advances.
    for target in ctx.retained.clone() {
        let slot_idx = target as usize % ctx.pool_size;
        let item = ctx.items[slot_idx];
        if !world.is_alive(item) {
            continue;
        }
        let rebound = ctx.bound_indices[slot_idx] != target;
        if rebound {
            world.get_mut::<LazyListPool>(entity).unwrap().bound_indices[slot_idx] = target;
            changed = true;
        }
        if world.has::<crate::ui::Hidden>(item) {
            world.remove::<crate::ui::Hidden>(item);
            changed = true;
        }
        let y = ctx.geometry.position(target);
        if rebound {
            crate::ui::set_position(world, item, Fixed::ZERO, y);
        } else {
            crate::ui::set_position_quiet(world, item, Fixed::ZERO, y);
        }
    }
    if world.get::<LazyList>(entity).unwrap().visible_start != ctx.visible_start {
        world.get_mut::<LazyList>(entity).unwrap().visible_start = ctx.visible_start;
    }
    changed
}

fn binding_is_current(world: &World, entity: Entity, slot: usize, event: BindItem) -> bool {
    if !world.is_alive(entity) || !world.is_alive(event.item) {
        return false;
    }
    let Some(list) = world.get::<LazyList>(entity) else {
        return false;
    };
    let Some(pool) = world.get::<LazyListPool>(entity) else {
        return false;
    };
    list.pool_size > 0
        && slot < list.pool_size as usize
        && event.index < list.item_count
        && event.index as usize % list.pool_size as usize == slot
        && pool.items.get(slot) == Some(&event.item)
        && pool.bound_indices.get(slot) == Some(&event.index)
}

/// Update retained row bindings and synchronously deliver changed items before layout.
pub fn lazy_list_system(world: &mut World) -> Result<(), crate::ecs::CommandError> {
    let lists: Vec<Entity> = world.query::<LazyList>().collect();
    let mut published = false;
    for entity in lists {
        ensure_missing_pool(world, entity);
        let Some(ctx) = collect_list_context(world, entity) else {
            continue;
        };
        if apply_bindings(world, entity, &ctx) {
            world.insert(entity, Dirty);
        }
        for index in ctx.retained.clone() {
            let slot = index as usize % ctx.pool_size;
            if ctx.bound_indices[slot] == index {
                continue;
            }
            let event = BindItem {
                item: ctx.items[slot],
                index,
            };
            if !binding_is_current(world, entity, slot, event) {
                continue;
            }
            published = true;
            if let Err(error) =
                crate::input::event::runtime::dispatch_widget_update(world, entity, &event)
            {
                crate::warn!(target: "mirui::lazy_list", "item callback skipped: {:?}", error);
            }
        }
    }
    if published && let Err(error) = crate::input::event::runtime::flush(world) {
        crate::warn!(target: "mirui::lazy_list", "item callback notifications skipped: {:?}", error);
    }
    Ok(())
}

crate::ecs::system::framework_system!(pub lazy_list_system, LazyList, expect = [LazyList]);

#[crate::view(component = LazyList, name = "LazyList", systems = [lazy_list_system::system()])]
fn lazy_list_view() {}

pub use lazy_list_view::view;

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

    struct BindTrace(alloc::vec::Vec<u32>);

    #[crate::event]
    fn record_binding(event: event!(BindItem), trace: res!(mut BindTrace)) {
        trace.0.push(event.index);
    }

    fn install_recording(world: &mut World, list: Entity) {
        crate::input::event::binding::install_entity_event(
            world,
            list,
            LazyList::BindItem,
            record_binding::callback::<crate::input::event::callback::NodeContext>(),
        );
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
        install_recording(&mut world, list);
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        lazy_list_system(&mut world).unwrap();

        let trace = &world.resource::<BindTrace>().unwrap().0;
        assert_eq!(trace, &alloc::vec![0u32, 1, 2, 3, 4]);

        let bound = &world.get::<LazyListPool>(list).unwrap().bound_indices;
        assert_eq!(bound, &alloc::vec![0u32, 1, 2, 3, 4]);
    }

    #[test]
    fn missing_pool_snapshots_children_and_binds_on_first_run() {
        let mut world = World::default();
        let list = world.spawn_empty();
        let children: Vec<Entity> = (0..5).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, LazyList::new(100, 40, 3));
        world.insert(list, crate::ui::Children(children.clone()));
        install_recording(&mut world, list);
        world.insert_resource(BindTrace(Vec::new()));

        lazy_list_system(&mut world).unwrap();

        let pool = world.get::<LazyListPool>(list).unwrap();
        assert_eq!(pool.items, children);
        assert_eq!(pool.bound_indices, [0, 1, 2, u32::MAX, u32::MAX]);
        assert_eq!(world.resource::<BindTrace>().unwrap().0, [0, 1, 2]);
    }

    #[test]
    fn explicit_pool_preserves_custom_slots_and_existing_bindings() {
        let mut world = World::default();
        let list = world.spawn_empty();
        let children: Vec<Entity> = (0..3).map(|_| make_slot(&mut world, list)).collect();
        let custom_slots: Vec<Entity> = (0..3).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, LazyList::new(100, 40, 3));
        world.insert(list, crate::ui::Children(children));
        let mut pool = LazyListPool::new(custom_slots.clone());
        pool.bound_indices.copy_from_slice(&[0, 1, 2]);
        let items_ptr = pool.items.as_ptr();
        let bindings_ptr = pool.bound_indices.as_ptr();
        world.insert(list, pool);
        install_recording(&mut world, list);
        world.insert_resource(BindTrace(Vec::new()));

        lazy_list_system(&mut world).unwrap();

        let pool = world.get::<LazyListPool>(list).unwrap();
        assert_eq!(pool.items, custom_slots);
        assert_eq!(pool.bound_indices, [0, 1, 2]);
        assert_eq!(pool.items.as_ptr(), items_ptr);
        assert_eq!(pool.bound_indices.as_ptr(), bindings_ptr);
        assert!(world.resource::<BindTrace>().unwrap().0.is_empty());
    }

    #[test]
    fn initialized_pool_is_retained_when_children_change() {
        let mut world = World::default();
        let list = world.spawn_empty();
        let children: Vec<Entity> = (0..3).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, LazyList::new(100, 40, 3));
        world.insert(list, crate::ui::Children(children.clone()));
        install_recording(&mut world, list);
        world.insert_resource(BindTrace(Vec::new()));
        lazy_list_system(&mut world).unwrap();
        let pool = world.get::<LazyListPool>(list).unwrap();
        let items_ptr = pool.items.as_ptr();
        let bindings_ptr = pool.bound_indices.as_ptr();
        world.resource_mut::<BindTrace>().unwrap().0.clear();
        let replacement = make_slot(&mut world, list);
        world.insert(list, crate::ui::Children(alloc::vec![replacement]));

        for _ in 0..2 {
            lazy_list_system(&mut world).unwrap();
            let pool = world.get::<LazyListPool>(list).unwrap();
            assert_eq!(pool.items, children);
            assert_eq!(pool.bound_indices, [0, 1, 2]);
            assert_eq!(pool.items.as_ptr(), items_ptr);
            assert_eq!(pool.bound_indices.as_ptr(), bindings_ptr);
        }
        assert!(world.resource::<BindTrace>().unwrap().0.is_empty());
    }

    #[test]
    fn missing_pool_waits_for_enough_children() {
        let mut world = World::default();
        let list = world.spawn_empty();
        world.insert(list, LazyList::new(100, 40, 3));
        install_recording(&mut world, list);
        world.insert_resource(BindTrace(Vec::new()));

        lazy_list_system(&mut world).unwrap();
        assert!(!world.has::<LazyListPool>(list));
        let mut children: Vec<Entity> = (0..2).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, crate::ui::Children(children.clone()));
        lazy_list_system(&mut world).unwrap();
        assert!(!world.has::<LazyListPool>(list));
        assert!(world.resource::<BindTrace>().unwrap().0.is_empty());
        children.push(make_slot(&mut world, list));
        world.insert(list, crate::ui::Children(children.clone()));

        lazy_list_system(&mut world).unwrap();

        let pool = world.get::<LazyListPool>(list).unwrap();
        assert_eq!(pool.items, children);
        assert_eq!(pool.bound_indices, [0, 1, 2]);
        assert_eq!(world.resource::<BindTrace>().unwrap().0, [0, 1, 2]);
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
        install_recording(&mut world, list);
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        lazy_list_system(&mut world).unwrap();
        world.resource_mut::<BindTrace>().unwrap().0.clear();

        // Scroll down 40 px → visible_start should become 1.
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(40),
            },
        );
        lazy_list_system(&mut world).unwrap();

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
        install_recording(&mut world, list);
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        lazy_list_system(&mut world).unwrap();

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
        install_recording(&mut world, list);
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        lazy_list_system(&mut world).unwrap();
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
        lazy_list_system(&mut world).unwrap();

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
        install_recording(&mut world, list);
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        lazy_list_system(&mut world).unwrap();
        assert!(world.get::<crate::ui::Hidden>(pool[3]).is_some());
        assert!(world.get::<crate::ui::Hidden>(pool[4]).is_some());

        if let Some(l) = world.get_mut::<LazyList>(list) {
            l.item_count = 5;
        }
        lazy_list_system(&mut world).unwrap();

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
    fn partial_rows_keep_identity_and_tail_does_not_backfill_the_pool() {
        let mut world = World::default();
        let list = world.spawn_empty();
        let pool: Vec<Entity> = (0..8).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, LazyList::new(100, 40, 8));
        world.insert(list, LazyListPool::new(pool.clone()));
        set_list_height(&mut world, list, 80);
        lazy_list_system(&mut world).unwrap();

        for offset in [20, 40, 20, 3980, 4000, -80, -20, 0] {
            world.insert(
                list,
                ScrollOffset {
                    x: Fixed::ZERO,
                    y: Fixed::from_int(offset),
                },
            );
            lazy_list_system(&mut world).unwrap();
            let geometry = ItemGeometry::new(100, Fixed::from_int(40)).unwrap();
            let visible = geometry.visible(Fixed::from_int(offset), Fixed::from_int(80));
            assert_eq!(
                world.get::<LazyList>(list).unwrap().visible_start,
                visible.start
            );
            let retained = geometry.retained(visible.clone(), 8);
            let bindings = &world.get::<LazyListPool>(list).unwrap().bound_indices;
            for (slot, &item) in pool.iter().enumerate() {
                assert_eq!(
                    world.has::<crate::ui::Hidden>(item),
                    bindings[slot] == u32::MAX
                );
                if bindings[slot] != u32::MAX {
                    assert!(retained.contains(&bindings[slot]));
                }
            }
            for index in visible {
                let slot = index as usize % pool.len();
                assert_eq!(bindings[slot], index);
                assert_eq!(
                    world.get::<Style>(pool[slot]).unwrap().layout.top,
                    Dimension::Px(Fixed::from_int(index as i32 * 40))
                );
            }
            assert_eq!(
                world.get::<ScrollOffset>(list).unwrap().y,
                Fixed::from_int(offset)
            );
        }
        set_list_height(&mut world, list, 0);
        lazy_list_system(&mut world).unwrap();
        assert_eq!(visible_count(&world, &pool), 0);
    }

    #[test]
    fn empty_cross_axis_hides_rows_without_rebinding_data() {
        let mut world = World::default();
        let list = world.spawn_empty();
        let pool: Vec<Entity> = (0..4).map(|_| make_slot(&mut world, list)).collect();
        world.insert(list, LazyList::new(100, 40, 4));
        world.insert(list, LazyListPool::new(pool.clone()));
        set_list_height(&mut world, list, 80);
        world.get_mut::<ComputedRect>(list).unwrap().0.w = Fixed::ZERO;
        lazy_list_system(&mut world).unwrap();
        assert_eq!(visible_count(&world, &pool), 0);
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
        install_recording(&mut world, list);
        world.insert_resource(BindTrace(alloc::vec::Vec::new()));

        // Short canvas: 64 px / 32 px = 2 rows + 2 overscan = 4 visible.
        set_list_height(&mut world, list, 64);
        lazy_list_system(&mut world).unwrap();
        let narrow = visible_count(&world, &pool);
        assert_eq!(narrow, 4, "short canvas binds ceil(h/item)+2 rows");

        // Grow taller than pool*item_height: window saturates at the pool.
        set_list_height(&mut world, list, 600);
        lazy_list_system(&mut world).unwrap();
        let wide = visible_count(&world, &pool);
        assert!(
            wide > narrow,
            "more rows must become visible as the list grows: {narrow} -> {wide}",
        );
        assert_eq!(wide, 12, "window is capped at the allocated pool");
    }
}
