use mirui::core::model::{ModelList, ModelSet};
use mirui::input::event::contract::Event;
use mirui::input::event::lifecycle::Started;
use mirui::{app::App, core::model::ModelMap, event, model};

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;
use tracking_allocator::tracked_allocations;

#[model]
struct Task {
    #[observe]
    done: bool,
}

#[model]
impl Task {
    fn toggle(&mut self) {
        self.done = !self.done;
    }
}

#[model]
struct Tasks {
    items: model!(owned ModelMap<u64, Task>),
}

#[model]
impl Tasks {
    #[observe]
    fn remaining(&self) -> usize {
        self.items.values().filter(|item| !item.done()).count()
    }
    fn toggle_all(&mut self) {
        for (_, task) in self.items.iter_mut() {
            task.toggle();
        }
    }
}

#[event]
fn toggle(task: model!(Task)) {
    task.toggle();
}

#[test]
fn keyed_child_models_support_author_macros_and_parent_transactions() {
    let data = ModelMap::from_entries([(7, Task { done: false }), (2, Task { done: false })])
        .ok()
        .unwrap();
    let mut app = App::headless(32, 32);
    let tasks = app.add_model(Tasks { items: data.into() }).finish();
    app.on_event(
        Started::EVENT,
        toggle::callback(tasks.items().get(&7).unwrap()),
    );
    assert_eq!(tasks.remaining(), 2);
    app.tick();
    assert!(tasks.items().get(&7).unwrap().done());
    assert_eq!(tasks.remaining(), 1);
    assert_eq!(tracked_allocations(|| tasks.toggle_all()), 0);
    assert!(!tasks.items().get(&7).unwrap().done());
    assert!(tasks.items().get(&2).unwrap().done());
    assert_eq!(tasks.remaining(), 1);
}

#[test]
fn removed_map_member_is_not_invoked_after_reinsertion_of_the_same_key() {
    let data = ModelMap::from_entries([(7, Task { done: false })])
        .ok()
        .unwrap();
    let mut app = App::headless(32, 32);
    let tasks = app.add_model(Tasks { items: data.into() }).finish();
    app.on_event(
        Started::EVENT,
        toggle::callback(tasks.items().get(&7).unwrap()),
    );
    tasks.items().remove(&7).unwrap();
    assert!(tasks.items().try_insert(7, Task { done: false }).is_ok());
    app.tick();
    assert!(!tasks.items().get(&7).unwrap().done());
    assert_eq!(tasks.remaining(), 1);
}

#[test]
fn loading_a_list_retires_declared_callbacks_and_accepts_owned_input() {
    let mut app = App::headless(32, 32);
    let items = app
        .add_model(ModelList::from([Task { done: false }]))
        .finish();
    app.on_event(Started::EVENT, toggle::callback(items.get(0).unwrap()));
    items
        .load([Task { done: false }, Task { done: true }])
        .unwrap();
    app.tick();
    assert!(!items.get(0).unwrap().done());
    assert!(items.get(1).unwrap().done());
}

#[test]
fn loading_keyed_data_retires_callbacks_even_when_business_keys_match() {
    let mut app = App::headless(32, 32);
    let map = app
        .add_model(
            ModelMap::from_entries([(7, Task { done: false })])
                .ok()
                .unwrap(),
        )
        .finish();
    app.on_event(Started::EVENT, toggle::callback(map.get(&7).unwrap()));
    map.load([(8, Task { done: true }), (7, Task { done: false })])
        .unwrap();
    app.tick();
    assert!(!map.get(&7).unwrap().done());
    assert!(map.get(&8).unwrap().done());
}

#[test]
fn reconciling_keyed_data_preserves_declared_callbacks_and_local_fields() {
    let mut app = App::headless(32, 32);
    let map = app
        .add_model(
            ModelMap::from_entries([(7, Task { done: true })])
                .ok()
                .unwrap(),
        )
        .finish();
    app.on_event(Started::EVENT, toggle::callback(map.get(&7).unwrap()));
    let old = map.entries().next().unwrap().0;
    map.reconcile(
        [(8, Task { done: true }), (7, Task { done: false })],
        |_, _| {},
    )
    .unwrap();
    assert_eq!(map.entries().next().unwrap().0, old);
    assert!(map.get(&7).unwrap().done());
    app.tick();
    assert!(!map.get(&7).unwrap().done());
    assert!(map.get(&8).unwrap().done());
}

#[test]
fn reconciled_members_keep_declared_callbacks_after_reordering() {
    let mut app = App::headless(32, 32);
    let items = app
        .add_model(ModelList::from([Task { done: false }, Task { done: true }]))
        .finish();
    let (id, captured) = items.entries().next().unwrap();
    app.on_event(Started::EVENT, toggle::callback(captured));
    items
        .reconcile(
            [Task { done: true }, Task { done: false }],
            |task| task.done,
            |current, incoming| current.done = !incoming.done,
        )
        .unwrap();
    assert_eq!(items.entries().nth(1).unwrap().0, id);
    assert!(items.get(1).unwrap().done());
    app.tick();
    assert!(!items.get(0).unwrap().done());
    assert!(!items.get(1).unwrap().done());
}

#[model]
struct Selection {
    keys: model!(owned ModelSet<u64>),
}

#[model]
impl Selection {
    #[observe]
    fn selected(&self) -> usize {
        self.keys.len()
    }

    fn refresh(&mut self) {
        self.keys.reconcile([1, 2, 2, 3]).unwrap();
    }
}

#[test]
fn set_imports_are_available_inside_owned_parent_methods_and_registered_roles() {
    let mut app = App::headless(32, 32);
    let selection = app
        .add_model(Selection {
            keys: ModelSet::bounded(4).into(),
        })
        .finish();
    selection.keys().load([9, 9]).unwrap();
    selection.refresh();
    assert_eq!(selection.selected(), 3);
    assert!(selection.keys().contains(&2));
    assert!(!selection.keys().contains(&9));
}

#[event]
fn select_key(selected: model!(ModelSet<u64>)) {
    assert_eq!(selected.try_insert(7), Ok(true));
}

#[test]
fn set_fields_and_direct_event_roles_publish_parent_derived_values() {
    let mut app = App::headless(32, 32);
    let selection = app
        .add_model(Selection {
            keys: ModelSet::bounded(4).into(),
        })
        .finish();
    app.on_event(Started::EVENT, select_key::callback(selection.keys()));
    assert_eq!(selection.selected(), 0);
    app.tick();
    assert!(selection.keys().contains(&7));
    assert_eq!(selection.selected(), 1);
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(selection.keys().try_insert(7), Ok(false));
            assert!(selection.keys().remove(&7));
            assert_eq!(selection.selected(), 0);
        }),
        0
    );
}
