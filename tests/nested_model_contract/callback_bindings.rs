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
    let workspace = app.add_model(Workspace::new());
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
