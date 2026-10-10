use super::*;
use crate::core::model::{BindType, ModelHandle, ModelList, ModelMap};
use crate::input::event::{gesture::GestureSystem, input::InputEvent};

#[crate::model]
#[derive(Debug)]
struct RowData {
    #[observe]
    title: &'static str,
}

#[crate::model]
impl RowData {
    fn rename(&mut self, title: &'static str) {
        self.title = title;
    }
}

#[crate::compose]
fn cell(item: model!(ListItem), rows: model!(ModelList<RowData>)) -> Entity {
    ui! {
        Text (
            text_capacity: 16,
            text: ${ item.id().and_then(|id| rows.get_by_id(id)).map(|row| row.title()).unwrap_or("") }
        )
    }
}

#[crate::event]
fn create(rows: model!(ModelList<RowData>), event: event!(CreateCell)) -> Entity {
    ui! {
        cell (event.item.clone(), rows.clone())
    }
}

#[crate::compose]
fn collection(rows: model!(ModelList<RowData>)) -> Entity {
    ui! {
        LazyList (items: rows.clone(), item_extent: 40) on CreateCell(create::callback(rows.clone()))
    }
}

fn data() -> ModelList<RowData> {
    let mut rows = ModelList::with_capacity(8);
    for title in ["first", "second", "third", "fourth"] {
        rows.try_push(RowData { title }).unwrap();
    }
    rows
}

type Rows = <ModelList<RowData> as BindType>::Shared;

fn setup_collection() -> (TestApp, Entity, Rows, Entity) {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let rows = app.add_model(data());
    let list = app.compose(root, collection(rows.clone()));
    viewport(&mut app.world, list, 200, 80, 0);
    let cell = app.world.get::<Children>(list).unwrap().0[0];
    (app, list, rows, cell)
}

fn press(world: &mut World, cell: Entity) {
    let mut gestures = GestureSystem::default();
    gestures.recognizer.update(
        &InputEvent::PointerDown {
            id: 0,
            x: 10.into(),
            y: 10.into(),
        },
        1,
        Some(cell),
        &mut gestures.events,
    );
    world.insert_resource(gestures);
}

#[test]
fn membership_changes_rebind_index_and_identity_without_reconstructing_cells() {
    let (mut app, list, rows, first) = setup_collection();
    let cells = app.world.get::<Children>(list).unwrap().0.clone();
    let item = app.world.get::<CellItem>(first).unwrap().handle.clone();
    let original = item.id().unwrap();
    press(&mut app.world, first);
    rows.rotate_left(1).unwrap();
    assert!(!cell_is_valid(&app.world, first));
    viewport(&mut app.world, list, 200, 80, 0);
    assert_ne!(item.id(), Some(original));
    assert_eq!(item.index(), 0);
    assert_eq!(
        app.world.get::<Text>(first).unwrap().resolve(&app.world),
        "second"
    );
    assert_eq!(app.world.get::<Children>(list).unwrap().0, cells);
    assert!(
        app.world
            .resource::<GestureSystem>()
            .unwrap()
            .recognizer
            .captured_target()
            .is_none()
    );
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..8 {
                rows.rotate_left(1).unwrap();
                viewport(&mut app.world, list, 200, 80, 0);
            }
        }),
        0
    );
}

#[test]
fn pointer_release_rejects_reordered_data_before_the_next_layout() {
    let (mut app, list, rows, first) = setup_collection();
    press(&mut app.world, first);
    rows.rotate_left(1).unwrap();
    crate::input::event::dispatch_input(
        &mut app.world,
        Some(list),
        &InputEvent::PointerUp {
            id: 0,
            x: 10.into(),
            y: 10.into(),
        },
        2,
        200,
        200,
    );
    let gestures = app.world.resource::<GestureSystem>().unwrap();
    assert!(gestures.recognizer.captured_target().is_none());
    assert!(gestures.events.buffer.is_empty());
}

#[test]
fn field_changes_and_unrelated_appends_keep_the_current_press() {
    let (mut app, list, rows, first) = setup_collection();
    let id = app.world.get::<CellItem>(first).unwrap().handle.id();
    press(&mut app.world, first);
    rows.get(0).unwrap().rename("updated");
    rows.try_push(RowData { title: "fifth" }).unwrap();
    viewport(&mut app.world, list, 200, 80, 0);
    assert_eq!(app.world.get::<CellItem>(first).unwrap().handle.id(), id);
    assert_eq!(
        app.world
            .resource::<GestureSystem>()
            .unwrap()
            .recognizer
            .captured_target(),
        Some(first)
    );
    assert_eq!(
        app.world.get::<Text>(first).unwrap().resolve(&app.world),
        "updated"
    );
}

#[test]
fn source_and_provider_replacement_retire_old_cells_and_their_models() {
    let (mut app, list, _, first) = setup_collection();
    let old = app.world.get::<CellItem>(first).unwrap().handle.clone();
    let rows = app.add_model(data());
    app.world.get_mut::<LazyList>(list).unwrap().items = rows.clone().into();
    app.compose(list, |tree| {
        tree.on_event(list, LazyList::CreateCell, create::callback(rows.clone()))
    });
    viewport(&mut app.world, list, 200, 80, 0);
    assert!(!app.world.is_alive(first));
    assert!(old.access().upgrade().is_none());
    let current = app.world.get::<Children>(list).unwrap().0[0];
    app.compose(list, |tree| {
        tree.on_event(list, LazyList::CreateCell, create::callback(rows.clone()))
    });
    viewport(&mut app.world, list, 200, 80, 0);
    assert!(!app.world.is_alive(current));
    assert_eq!(app.world.get::<Children>(list).unwrap().0.len(), 4);
}

#[test]
fn map_rows_follow_sorted_member_ids_and_reject_foreign_or_retired_sources() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let mut data = ModelMap::with_capacity(4);
    let (b, _) = data.try_insert("b", RowData { title: "B" }).unwrap();
    let (a, _) = data.try_insert("a", RowData { title: "A" }).unwrap();
    let rows = app.add_model(data);
    let list = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(rows.clone(), 40));
        tree.on_event(list, LazyList::CreateCell, row::callback());
        list
    });
    viewport(&mut app.world, list, 200, 80, 0);
    let first = app.world.get::<Children>(list).unwrap().0[0];
    let item = app.world.get::<CellItem>(first).unwrap().handle.clone();
    assert_eq!(item.id(), Some(a));
    rows.remove("a").unwrap();
    viewport(&mut app.world, list, 200, 80, 0);
    assert_eq!(item.id(), Some(b));
    let other = App::headless(8, 8);
    let source: ListItems = rows.clone().into();
    assert!(matches!(
        source.count(&other.world),
        Err(items::SourceError::Owner(_))
    ));
    let retired: ListItems = {
        let mut other = App::headless(8, 8);
        other
            .add_model(ModelList::from([RowData { title: "retired" }]))
            .into()
    };
    assert_eq!(retired.len(), 0);
    assert!(retired.count(&app.world).is_err());
}

#[test]
fn replacing_a_removed_member_never_reuses_its_cell_identity() {
    let (mut app, list, rows, first) = setup_collection();
    let old = app
        .world
        .get::<CellItem>(first)
        .unwrap()
        .handle
        .id()
        .unwrap();
    press(&mut app.world, first);
    rows.remove(old).unwrap();
    let new = rows
        .try_insert(
            0,
            RowData {
                title: "replacement",
            },
        )
        .unwrap();
    assert_ne!(old, new);
    viewport(&mut app.world, list, 200, 80, 0);
    assert_eq!(
        app.world.get::<CellItem>(first).unwrap().handle.id(),
        Some(new)
    );
    assert!(
        app.world
            .resource::<GestureSystem>()
            .unwrap()
            .recognizer
            .captured_target()
            .is_none()
    );
}

#[test]
fn structural_subscription_marks_an_empty_list_dirty_without_row_effects() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let rows = app.add_model(ModelList::<RowData>::with_capacity(4));
    let list = app.compose(root, collection(rows.clone()));
    viewport(&mut app.world, list, 200, 80, 0);
    app.world.remove::<Dirty>(list);
    rows.try_push(RowData { title: "first" }).unwrap();
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert!(app.world.has::<Dirty>(list));
    viewport(&mut app.world, list, 200, 80, 0);
    assert_eq!(app.world.get::<Children>(list).unwrap().0.len(), 1);
}

#[test]
fn inactive_collection_cells_settle_without_repeated_retirement() {
    let (mut app, list, rows, first) = setup_collection();
    rows.clear().unwrap();
    viewport(&mut app.world, list, 200, 80, 0);
    assert!(app.world.has::<Hidden>(first));
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..8 {
                assert!(!reconcile(&mut app.world));
            }
        }),
        0
    );
    rows.try_push(RowData { title: "restored" }).unwrap();
    viewport(&mut app.world, list, 200, 80, 0);
    assert!(!app.world.has::<Hidden>(first));
    assert_eq!(
        app.world.get::<Text>(first).unwrap().resolve(&app.world),
        "restored"
    );
}

#[test]
fn retiring_the_source_disposes_cells_without_reading_expired_models() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let (registration, rows) = crate::core::model::register(&mut app.world, data());
    let list = app.compose(root, collection(rows));
    viewport(&mut app.world, list, 200, 80, 0);
    let first = app.world.get::<Children>(list).unwrap().0[0];
    press(&mut app.world, first);
    drop(registration);
    viewport(&mut app.world, list, 200, 80, 0);
    assert!(app.world.get::<Children>(list).unwrap().0.is_empty());
    assert!(!app.world.is_alive(first));
    assert!(
        app.world
            .resource::<GestureSystem>()
            .unwrap()
            .recognizer
            .captured_target()
            .is_none()
    );
}

#[crate::event]
fn clear_during_creation(rows: model!(ModelList<RowData>), _event: event!(CreateCell)) -> Entity {
    rows.clear().unwrap();
    ui! {
        View ()
    }
}

#[test]
fn creation_rechecks_source_after_user_mutation_without_leaking_a_partial_cell() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let rows = app.add_model(data());
    let list = app.compose(root, |tree| {
        let list = tree.spawn(LazyList::new(rows.clone(), 40));
        tree.on_event(
            list,
            LazyList::CreateCell,
            clear_during_creation::callback(rows.clone()),
        );
        list
    });
    let before = app.world.live_entity_count();
    viewport(&mut app.world, list, 200, 80, 0);
    assert!(rows.is_empty());
    assert!(app.world.get::<Children>(list).unwrap().0.is_empty());
    assert_eq!(app.world.live_entity_count(), before);
}

#[crate::compose]
fn interactive_cell(item: model!(ListItem), rows: model!(ModelList<RowData>)) -> Entity {
    ui! {
        Text (
            text_capacity: 16,
            text: ${ item.id().and_then(|id| rows.get_by_id(id)).map(|row| row.title()).unwrap_or("") }
        ) on Tap {
            if let Some(row) = item.id().and_then(|id| rows.get_by_id(id)) {
                row.rename("clicked");
            }
        }
    }
}

#[crate::compose]
fn interactive_collection(rows: model!(ModelList<RowData>)) -> Entity {
    ui! {
        LazyList (items: rows.clone(), item_extent: 40) on CreateCell(create_interactive::callback(rows.clone()))
    }
}

#[crate::event]
fn create_interactive(rows: model!(ModelList<RowData>), event: event!(CreateCell)) -> Entity {
    ui! {
        interactive_cell (event.item.clone(), rows.clone())
    }
}

#[test]
fn collection_callbacks_resolve_current_identity_after_reordering_and_slot_reuse() {
    let mut app = App::headless(200, 200);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let rows = app.add_model(data());
    let list = app.compose(root, interactive_collection(rows.clone()));
    viewport(&mut app.world, list, 200, 80, 0);
    let first = app.world.get::<Children>(list).unwrap().0[0];
    let original = rows.get(0).unwrap();
    let tap = crate::input::event::gesture::GestureEvent::Tap {
        target: first,
        x: 10.into(),
        y: 10.into(),
    };
    rows.rotate_left(1).unwrap();
    crate::input::event::runtime::dispatch_gesture(&mut app.world, &tap, 100).unwrap();
    assert_eq!(rows.get(0).unwrap().title(), "second");
    viewport(&mut app.world, list, 200, 80, 0);
    crate::input::event::runtime::dispatch_gesture(&mut app.world, &tap, 200).unwrap();
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(rows.get(0).unwrap().title(), "clicked");
    assert_eq!(original.title(), "first");
    let removed = app
        .world
        .get::<CellItem>(first)
        .unwrap()
        .handle
        .id()
        .unwrap();
    rows.remove(removed).unwrap();
    let replacement = rows.try_insert(0, RowData { title: "new" }).unwrap();
    assert_ne!(removed, replacement);
    viewport(&mut app.world, list, 200, 80, 0);
    crate::input::event::runtime::dispatch_gesture(&mut app.world, &tap, 300).unwrap();
    crate::core::reactive::flush_signal_dirty(&mut app.world);
    assert_eq!(rows.get_by_id(replacement).unwrap().title(), "clicked");
    assert!(rows.get_by_id(removed).is_none());
    assert_eq!(app.world.get::<Children>(list).unwrap().0[0], first);
    assert_eq!(
        app.world.get::<Text>(first).unwrap().resolve(&app.world),
        "clicked"
    );
}
