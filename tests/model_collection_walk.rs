use mirui::core::{model::ModelList, reactive::flush_signal_dirty};
use mirui::ecs::Entity;
use mirui::types::Dimension;
use mirui::ui::{Children, Parent, Style};
use mirui::{app::App, compose, model, ui};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;
use tracking_allocator::tracked_allocations;

#[model]
struct Row {
    #[observe]
    value: u8,
}
#[model]
impl Row {
    fn increment(&mut self) {
        self.value += 1;
    }
}

#[compose]
fn row(item: model!(Row)) -> Entity {
    ui! {
        View (width: ${ Dimension::px(i32::from(item.value())) }, height: 16)
    }
}

#[compose]
fn rows(items: model!(ModelList<Row>)) -> Entity {
    ui! {
        Column () {
            walk ${ items.entries() } with entry by entry.0 {
                row (entry.1)
            }
        }
    }
}

#[compose]
fn indexed_rows(count: mirui::core::reactive::Signal<usize>) -> Entity {
    ui! {
        Column () {
            walk ${ (0..count.get()).filter(|value| value % 2 == 0) } with value {
                View (width: value as i32, height: 16)
            }
        }
    }
}

#[compose]
fn conditional_rows(
    count: mirui::core::reactive::Signal<usize>,
    selected: model!(Row),
    keyed: bool,
    reads: std::rc::Rc<std::cell::Cell<usize>>,
) -> Entity {
    ui! {
        Column () {
            if keyed {
                walk ${ 0..count.get() } with value by value {
                    ${ let reads = reads.clone(); }
                    if ${
                        {
                            reads.set(reads.get() + 1);
                            selected.value() % 2 == 1
                        }
                    } {
                        View (width: value as i32, height: 16)
                    } else {
                        View (width: value as i32, height: 17)
                        View (width: value as i32, height: 18)
                    }
                }
            } else {
                walk ${ 0..count.get() } with value {
                    ${ let reads = reads.clone(); }
                    if ${
                        {
                            reads.set(reads.get() + 1);
                            selected.value() % 2 == 1
                        }
                    } {
                        View (width: value as i32, height: 16)
                    } else {
                        View (width: value as i32, height: 17)
                        View (width: value as i32, height: 18)
                    }
                }
            }
        }
    }
}

#[test]
fn conditional_walks_own_every_branch_root_through_reuse_and_retirement() {
    for keyed in [false, true] {
        let mut app = App::headless(32, 32);
        let root = app.spawn_root().id();
        let count = mirui::core::reactive::Signal::new(2_usize);
        let selected = app.add_model(Row { value: 1 }).finish();
        let reads = std::rc::Rc::new(std::cell::Cell::new(0));
        let list = app.compose(
            root,
            conditional_rows(count.clone(), selected.clone(), keyed, reads.clone()),
        );
        let original = app.world.get::<Children>(list).unwrap().0.clone();
        assert_eq!(original.len(), 6);
        for length in [3, 2, 3, 2] {
            count.set(length);
            selected.increment();
            flush_signal_dirty(&mut app.world);
            let children = &app.world.get::<Children>(list).unwrap().0;
            assert_eq!(children.len(), length * 3);
            assert_eq!(&children[..6], &original);
            assert_eq!(app.world.query::<Style>().iter().count(), length * 3 + 2);
        }
        count.set(0);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.get::<Children>(list).unwrap().0.is_empty());
        assert_eq!(app.world.query::<Style>().iter().count(), 2);
        assert!(original.iter().all(|entity| !app.world.is_alive(*entity)));
        let before = reads.get();
        selected.increment();
        flush_signal_dirty(&mut app.world);
        assert_eq!(
            reads.get(),
            before,
            "Retired members must release condition subscriptions"
        );
    }
}

#[compose]
fn sparse_rows(count: mirui::core::reactive::Signal<usize>, keyed: bool) -> Entity {
    ui! {
        Column () {
            if keyed {
                walk ${ 0..count.get() } with value by value {
                    if value % 2 == 0 {
                        View (width: value as i32, height: 16)
                        View (width: value as i32, height: 17)
                    }
                }
            } else {
                walk ${ 0..count.get() } with value {
                    if value % 2 == 0 {
                        View (width: value as i32, height: 16)
                        View (width: value as i32, height: 17)
                    }
                }
            }
        }
    }
}

#[test]
fn empty_walk_members_retain_their_index_and_retire_all_sibling_roots() {
    for keyed in [false, true] {
        let mut app = App::headless(32, 32);
        let root = app.spawn_root().id();
        let count = mirui::core::reactive::Signal::new(3_usize);
        let list = app.compose(root, sparse_rows(count.clone(), keyed));
        let original = app.world.get::<Children>(list).unwrap().0.clone();
        assert_eq!(original.len(), 4);
        count.set(4);
        flush_signal_dirty(&mut app.world);
        assert_eq!(app.world.get::<Children>(list).unwrap().0, original);
        count.set(0);
        flush_signal_dirty(&mut app.world);
        assert!(app.world.get::<Children>(list).unwrap().0.is_empty());
        assert!(original.iter().all(|entity| !app.world.is_alive(*entity)));
    }
}

#[compose]
fn paired_rows(items: model!(ModelList<Row>)) -> Entity {
    ui! {
        Column () {
            walk ${ items.entries() } with entry by entry.0 {
                row (entry.1.clone())
                row (entry.1)
            }
        }
    }
}

#[test]
fn keyed_compound_members_reorder_without_allocating_and_remove_all_roots() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let items = app
        .add_model(ModelList::from([
            Row { value: 1 },
            Row { value: 2 },
            Row { value: 3 },
        ]))
        .finish();
    let list = app.compose(root, paired_rows(items.clone()));
    let original = app.world.get::<Children>(list).unwrap().0.clone();
    assert_eq!(original.len(), 6);
    assert_eq!(
        tracked_allocations(|| {
            items.rotate_left(1).unwrap();
            flush_signal_dirty(&mut app.world);
        }),
        0
    );
    assert_eq!(
        app.world.get::<Children>(list).unwrap().0,
        [
            original[2],
            original[3],
            original[4],
            original[5],
            original[0],
            original[1]
        ]
    );
    let removed = items.entries().next().unwrap().0;
    items.remove(removed).unwrap();
    flush_signal_dirty(&mut app.world);
    assert!(!app.world.is_alive(original[2]) && !app.world.is_alive(original[3]));
    items.clear().unwrap();
    flush_signal_dirty(&mut app.world);
    assert!(app.world.get::<Children>(list).unwrap().0.is_empty());
    assert_eq!(app.world.query::<Style>().iter().count(), 2);
}

#[test]
fn indexed_rows_stream_unknown_lengths_and_reuse_retained_storage() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let count = mirui::core::reactive::Signal::new(6_usize);
    let list = app.compose(root, indexed_rows(count.clone()));
    flush_signal_dirty(&mut app.world);
    let original = app.world.get::<Children>(list).unwrap().0.clone();
    assert_eq!(original.len(), 3);
    assert_eq!(
        tracked_allocations(|| {
            count.set(5);
            flush_signal_dirty(&mut app.world);
        }),
        0
    );
    count.set(8);
    flush_signal_dirty(&mut app.world);
    let children = &app.world.get::<Children>(list).unwrap().0;
    assert_eq!(children.len(), 4);
    assert_eq!(&children[..3], &original);
    assert_eq!(
        app.world.get::<Style>(children[3]).unwrap().layout.width,
        Dimension::px(6)
    );
    count.set(0);
    flush_signal_dirty(&mut app.world);
    assert!(app.world.get::<Children>(list).unwrap().0.is_empty());
    assert!(original.iter().all(|entity| !app.world.is_alive(*entity)));
    count.set(2);
    flush_signal_dirty(&mut app.world);
    assert_eq!(app.world.get::<Children>(list).unwrap().0.len(), 1);
}

#[test]
fn keyed_model_rows_reorder_without_allocating_and_keep_member_bindings() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let items = app
        .add_model(ModelList::from([
            Row { value: 1 },
            Row { value: 2 },
            Row { value: 3 },
        ]))
        .finish();
    let list = app.compose(root, rows(items.clone()));
    flush_signal_dirty(&mut app.world);
    let original = app.world.get::<Children>(list).unwrap().0.clone();
    let first = items.get(0).unwrap();
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..12 {
                items.rotate_left(1).unwrap();
                flush_signal_dirty(&mut app.world);
            }
        }),
        0
    );
    assert_eq!(app.world.get::<Children>(list).unwrap().0, original);
    items.rotate_left(1).unwrap();
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<Children>(list).unwrap().0,
        [original[1], original[2], original[0]]
    );
    first.increment();
    flush_signal_dirty(&mut app.world);
    assert_eq!(
        app.world.get::<Style>(original[0]).unwrap().layout.width,
        Dimension::px(2)
    );
    for entity in &original {
        assert_eq!(app.world.get::<Parent>(*entity).unwrap().0, list);
    }
}

#[test]
fn keyed_member_removal_and_reuse_replace_only_the_retired_row() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let items = app
        .add_model(ModelList::from([Row { value: 1 }, Row { value: 2 }]))
        .finish();
    let list = app.compose(root, rows(items.clone()));
    let original = app.world.get::<Children>(list).unwrap().0.clone();
    let removed = items.entries().next().unwrap().0;
    items.remove(removed).unwrap();
    flush_signal_dirty(&mut app.world);
    assert!(!app.world.is_alive(original[0]));
    assert_eq!(app.world.get::<Children>(list).unwrap().0, [original[1]]);
    let replacement = items.try_push(Row { value: 9 }).ok().unwrap();
    assert_ne!(removed, replacement);
    flush_signal_dirty(&mut app.world);
    let children = &app.world.get::<Children>(list).unwrap().0;
    assert_eq!(children.len(), 2);
    assert_eq!(children[0], original[1]);
    assert_ne!(children[1], original[0]);
    assert_eq!(
        app.world.get::<Style>(children[1]).unwrap().layout.width,
        Dimension::px(9)
    );
    items.clear().unwrap();
    flush_signal_dirty(&mut app.world);
    assert!(app.world.get::<Children>(list).unwrap().0.is_empty());
}
