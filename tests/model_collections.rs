use mirui::core::model::ModelSet;
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
    let tasks = app.add_model(Tasks { items: data.into() });
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
    let tasks = app.add_model(Tasks { items: data.into() });
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
}

#[event]
fn select_key(selected: model!(ModelSet<u64>)) {
    assert_eq!(selected.try_insert(7), Ok(true));
}

#[test]
fn set_fields_and_direct_event_roles_publish_parent_derived_values() {
    let mut app = App::headless(32, 32);
    let selection = app.add_model(Selection {
        keys: ModelSet::bounded(4).into(),
    });
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
