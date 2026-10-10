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
    let items = app.add_model(ModelList::from([
        Row { value: 1 },
        Row { value: 2 },
        Row { value: 3 },
    ]));
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
    let items = app.add_model(ModelList::from([Row { value: 1 }, Row { value: 2 }]));
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
