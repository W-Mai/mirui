use super::*;
use crate::ui::{Parent, branch::CachedBranchVisibility};

fn install(
    world: &mut World,
    entity: Entity,
    role: BindingRole,
    source: &Signal<u32>,
) -> Rc<Cell<u32>> {
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let source = source.clone();
    with_world_scope(world, || {
        effect_with_role(entity, role, move || {
            source.get();
            counted.set(counted.get() + 1);
        })
    });
    calls
}

#[test]
fn cached_visibility_only_defers_display_and_wakes_latest_state_once() {
    let mut world = World::new();
    let root = world.spawn_empty();
    let child = world.spawn_empty();
    world.insert(child, Parent(root));
    world.insert(root, CachedBranchVisibility { selected: false });
    let source = Signal::new(0);
    let display = install(&mut world, child, BindingRole::Display, &source);
    let maintenance = install(&mut world, child, BindingRole::Maintenance, &source);
    let ordinary = install(&mut world, child, BindingRole::Ordinary, &source);
    flush_signal_dirty(&mut world);
    for value in 1..4 {
        source.set(value);
        assert_eq!(
            crate::test_support::tracked_allocations(|| flush_signal_dirty(&mut world)),
            0
        );
    }
    assert_eq!(display.get(), 1);
    assert_eq!(maintenance.get(), 4);
    assert_eq!(ordinary.get(), 4);
    world
        .get_mut::<CachedBranchVisibility>(root)
        .unwrap()
        .selected = true;
    retained::wake_display(&world, root);
    retained::wake_display(&world, root);
    flush_signal_dirty(&mut world);
    assert_eq!(display.get(), 2);
    assert_eq!(ordinary.get(), 4);
    world.insert(root, crate::ui::Hidden);
    source.set(4);
    flush_signal_dirty(&mut world);
    assert_eq!(display.get(), 3, "authored Hidden is not an execution gate");
    let gate = retained::RetainedEffects::new(&mut world, |_, _| true);
    world.insert(root, gate);
    world
        .get_mut::<retained::RetainedEffects>(root)
        .unwrap()
        .active = false;
    source.set(5);
    flush_signal_dirty(&mut world);
    assert_eq!(
        (display.get(), maintenance.get(), ordinary.get()),
        (3, 5, 5)
    );
}

#[test]
fn maintenance_precedes_earlier_display_and_reparenting_reorders_remaining_work() {
    let mut world = World::new();
    let parent = world.spawn_empty();
    let child = world.spawn_empty();
    let leaf = world.spawn_empty();
    world.insert(child, Parent(parent));
    world.insert(leaf, Parent(child));
    world.insert(parent, CachedBranchVisibility { selected: true });
    let source = Signal::new(0);
    let display = install(&mut world, parent, BindingRole::Display, &source);
    let log = Rc::new(RefCell::new(Vec::with_capacity(8)));
    // Installed before the maintenance effect that will become its ancestor.
    for (entity, label) in [(child, 1), (leaf, 2), (parent, 0)] {
        let source = source.clone();
        let log = log.clone();
        with_world_scope(&mut world, || {
            effect_with_role(entity, BindingRole::Maintenance, move || {
                if source.get() == 0 {
                    return;
                }
                log.borrow_mut().push(label);
                if label == 0 {
                    with_world(|world| {
                        world.remove::<Parent>(leaf);
                        world.insert(child, Parent(leaf));
                        world
                            .get_mut::<CachedBranchVisibility>(parent)
                            .unwrap()
                            .selected = false;
                    });
                }
            })
        });
    }
    source.set(1);
    flush_signal_dirty(&mut world);
    assert_eq!(&*log.borrow(), &[0, 2, 1]);
    assert_eq!(display.get(), 1);
}

#[test]
fn foreign_worlds_and_ordinary_queue_order_remain_independent() {
    let mut first = World::new();
    let mut second = World::new();
    let a = first.spawn_empty();
    let b = second.spawn_empty();
    assert_eq!(a, b);
    first.insert(a, CachedBranchVisibility { selected: false });
    let source = Signal::new(0);
    let hidden = install(&mut first, a, BindingRole::Display, &source);
    let visible = install(&mut second, b, BindingRole::Display, &source);
    source.set(1);
    flush_signal_dirty(&mut first);
    assert_eq!((hidden.get(), visible.get()), (1, 1));
    flush_signal_dirty(&mut second);
    assert_eq!(visible.get(), 2);
    let log = Rc::new(RefCell::new(Vec::with_capacity(4)));
    let s1 = Signal::new(false);
    let s2 = Signal::new(false);
    for (signal, number) in [(s1.clone(), 1), (s2.clone(), 2)] {
        let log = log.clone();
        with_world_scope(&mut first, || {
            effect_with_widget(a, move || {
                if signal.get() {
                    log.borrow_mut().push(number);
                }
            })
        });
    }
    s2.set(true);
    s1.set(true);
    flush_signal_dirty(&mut first);
    assert_eq!(&*log.borrow(), &[2, 1]);
}
