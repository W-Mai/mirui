use super::*;
use mirui::input::event::gesture::{GestureEvent, Tap};

#[event]
fn increment(counter: model!(Counter)) {
    counter.add(1);
}

#[compose]
fn buttons(left: model!(Counter), right: model!(Counter)) {
    ui! {
        Row () {
            View (id: "nested-block", width: 8, height: 8) on Tap { left.add(1); }
            View (id: "nested-inline", width: 8, height: 8) on Tap(|_input: event!(Tap)| {
                right.add(1);
            })
            View (id: "nested-named", width: 8, height: 8) on Tap(increment::callback(left.clone()))
        }
    };
}

#[test]
fn composed_child_bindings_keep_identity_in_all_three_event_forms_after_reset() {
    let mut app = App::headless(32, 16);
    app.with_default_widgets();
    let workspace = app.add_model(Workspace::new()).finish();
    let root = app.spawn_root().id();
    app.compose(
        root,
        buttons(workspace.pair().left(), workspace.pair().right()),
    );
    workspace.reset_pair();
    for id in ["nested-block", "nested-inline", "nested-named"] {
        let target = app.world.find_by_id(id).unwrap();
        let event = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        };
        mirui::__macro_support::dispatch_entity_gesture(&mut app.world, &event, 0).unwrap();
        mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
    }
    assert_eq!(
        (
            workspace.pair().left().value(),
            workspace.pair().right().value()
        ),
        (3, 11)
    );
    assert_eq!(workspace.total(), 114);
}

#[test]
fn retired_collection_member_is_skipped_by_all_three_event_forms() {
    use mirui::core::model::ModelList;

    let mut app = App::headless(32, 16);
    app.with_default_widgets();
    let list = app
        .add_model(ModelList::from([Counter::new(1), Counter::new(10)]))
        .finish();
    let (id, retired) = list.entries().next().unwrap();
    let survivor_id = list.entries().nth(1).unwrap().0;
    let survivor = list.get(1).unwrap();
    let root = app.spawn_root().id();
    app.compose(root, buttons(retired.clone(), survivor.clone()));
    list.remove(id).unwrap();
    assert!(list.try_push(Counter::new(40)).is_ok());
    for id in ["nested-block", "nested-inline", "nested-named"] {
        let target = app.world.find_by_id(id).unwrap();
        let event = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        };
        mirui::__macro_support::dispatch_entity_gesture(&mut app.world, &event, 0).unwrap();
        mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
    }
    assert_eq!(survivor.value(), 11);
    assert_eq!(list.get(1).unwrap().value(), 40);
    list.remove(survivor_id).unwrap();
    assert!(list.try_push(Counter::new(50)).is_ok());
    let target = app.world.find_by_id("nested-inline").unwrap();
    let event = GestureEvent::Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target,
    };
    mirui::__macro_support::dispatch_entity_gesture(&mut app.world, &event, 0).unwrap();
    mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
    assert_eq!(list.get(1).unwrap().value(), 50);
}
