use super::*;
use crate::app::App;
use crate::input::event::contract::Event;
use crate::input::event::gesture::{GestureEvent, Tap};
use crate::prelude::*;
use crate::test_support::tracked_allocations;
use crate::ui::selection::SelectAll;
use crate::ui::widgets::{TextEditing, TextInput, TextSelection};

#[crate::component]
#[derive(Default)]
struct Selectable(u32);

#[crate::event]
fn choose(
    node: entity!(),
    _request: event!(SelectAll),
    values: query!(mut Selectable),
) -> Result<(), RequestError> {
    values.get(node).ok_or(RequestError::Unavailable)?.0 += 1;
    Ok(())
}

#[crate::view(component = Selectable, input = choose::callback())]
fn selectable() {}

#[crate::view(component = Selectable, input = choose::callback())]
fn duplicate() {}

#[crate::event]
fn recurse(
    node: entity!(),
    _request: event!(SelectAll),
    commands: commands!(),
) -> Result<(), RequestError> {
    commands.selection().select_all(node);
    Ok(())
}

#[test]
fn view_requests_share_prepared_callbacks_without_input_feedback_or_hot_allocation() {
    let mut app = App::headless(32, 32);
    app.with_widget(selectable::view());
    let root = app.spawn_root().id();
    let node = app.compose(root, |cx| ui! { View () [Selectable::default()] });
    assert!(!app.world.has::<crate::ui::HitTarget>(node));
    assert!(!app.world.has::<crate::ui::InteractionFeedback>(node));
    assert_eq!(invoke(&mut app.world, node, &SelectAll), Ok(Ok(())));
    assert_eq!(
        tracked_allocations(|| {
            for _ in 0..8 {
                assert_eq!(invoke(&mut app.world, node, &SelectAll), Ok(Ok(())));
            }
        }),
        0
    );
    assert_eq!(app.world.get::<Selectable>(node).unwrap().0, 9);
    app.world.remove::<Selectable>(node);
    assert_eq!(
        invoke(&mut app.world, node, &SelectAll),
        Err(RequestError::Unsupported)
    );
}

#[test]
fn conflicting_defaults_do_not_run_and_explicit_handlers_override_them() {
    let mut app = App::headless(32, 32);
    app.with_widget(selectable::view())
        .with_widget(duplicate::view());
    let root = app.spawn_root().id();
    let node = app.compose(root, |cx| ui! { View () [Selectable::default()] });
    assert_eq!(
        invoke(&mut app.world, node, &SelectAll),
        Err(RequestError::Ambiguous)
    );
    assert_eq!(app.world.get::<Selectable>(node).unwrap().0, 0);
    install(
        &mut app.world,
        node,
        choose::callback::<crate::input::event::callback::NodeContext>(),
    )
    .unwrap();
    assert_eq!(invoke(&mut app.world, node, &SelectAll), Ok(Ok(())));
    assert_eq!(app.world.get::<Selectable>(node).unwrap().0, 1);
    assert!(matches!(
        install(
            &mut app.world,
            node,
            choose::callback::<crate::input::event::callback::NodeContext>()
        ),
        Err(RuntimeInstallError::DuplicateRequest { .. })
    ));
    assert_eq!(invoke(&mut app.world, node, &SelectAll), Ok(Ok(())));
    assert_eq!(app.world.get::<Selectable>(node).unwrap().0, 2);
}

#[test]
fn request_callbacks_use_all_author_forms_and_keep_results() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let node = app.compose(root, |cx| {
        ui! {
            View () on SelectAll { let result = Err(RequestError::Rejected); result }
        }
    });
    assert_eq!(
        invoke(&mut app.world, node, &SelectAll),
        Ok(Err(RequestError::Rejected))
    );
    let node = app.compose(root, |cx| {
        ui! {
            View () on SelectAll(|_event: event!(SelectAll)| Ok(()))
        }
    });
    assert_eq!(invoke(&mut app.world, node, &SelectAll), Ok(Ok(())));
    let node = app.compose(root, |cx| {
        ui! {
            View () [Selectable::default()] on SelectAll(choose::callback())
        }
    });
    assert_eq!(invoke(&mut app.world, node, &SelectAll), Ok(Ok(())));
    assert_eq!(app.world.get::<Selectable>(node).unwrap().0, 1);
    let node = app.compose(root, |cx| {
        cx.spawn(TextInput::build().on_event(SelectAll::EVENT, choose::callback()))
    });
    assert_eq!(
        invoke(&mut app.world, node, &SelectAll),
        Ok(Err(RequestError::Unavailable))
    );
}

#[test]
fn commands_inside_a_request_reject_recursive_delivery_without_retrying() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    let node = app.compose(root, |cx| ui! { View () on SelectAll(recurse::callback()) });
    assert_eq!(invoke(&mut app.world, node, &SelectAll), Ok(Ok(())));
    assert_eq!(invoke(&mut app.world, node, &SelectAll), Ok(Ok(())));
}

#[crate::event]
fn select_target(_tap: event!(Tap), node: entity!(), commands: commands!()) {
    commands.selection().select_all(node);
}

#[test]
fn selection_command_can_invoke_requests_from_an_active_gesture_snapshot() {
    let mut app = App::headless(32, 32);
    app.with_widget(selectable::view());
    let root = app.spawn_root().id();
    let node = app.compose(root, |cx| {
        ui! {
            View () [Selectable::default()] on Tap(select_target::callback())
        }
    });
    let event = GestureEvent::Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target: node,
    };
    crate::ui::delivery::dispatch_gesture(&mut app.world, &event, 0).unwrap();
    assert_eq!(app.world.get::<Selectable>(node).unwrap().0, 1);
}

#[test]
fn text_selection_targets_one_instance_and_keeps_shared_content_unchanged() {
    let mut app = App::headless(32, 32);
    app.with_default_widgets();
    let document = app.add_model(crate::ui::widgets::TextDocument::new());
    document.set_text("A中🙂").unwrap();
    let first_editor = TextEditing::new(document.clone());
    let second_editor = TextEditing::new(document.clone());
    let root = app.spawn_root().id();
    let first = app.compose(root, |cx| ui! { TextInput () [first_editor] });
    let second = app.compose(root, |cx| ui! { TextInput () [second_editor] });
    assert_eq!(invoke(&mut app.world, first, &SelectAll), Ok(Ok(())));
    assert_eq!(
        app.world.get::<TextEditing>(first).unwrap().selection(),
        TextSelection {
            anchor: 0,
            cursor: 8
        }
    );
    assert_eq!(
        app.world.get::<TextEditing>(second).unwrap().selection(),
        TextSelection::default()
    );
    document.read(|text| assert_eq!(text, "A中🙂"));
    assert_eq!(
        tracked_allocations(|| {
            app.world
                .get_mut::<TextEditing>(first)
                .unwrap()
                .set_selection(TextSelection::default())
                .unwrap();
            assert_eq!(invoke(&mut app.world, first, &SelectAll), Ok(Ok(())));
        }),
        0
    );
}

#[test]
fn unavailable_targets_are_rejected_without_bubbling_or_calling_other_providers() {
    let mut app = App::headless(32, 32);
    app.with_widget(selectable::view());
    let root = app.spawn_root().id();
    let node = app.compose(root, |cx| ui! { View () [Selectable::default()] });
    app.world.insert(root, crate::ui::Hidden);
    assert_eq!(
        invoke(&mut app.world, node, &SelectAll),
        Err(RequestError::Unavailable)
    );
    app.world.remove::<crate::ui::Hidden>(root);
    app.world.insert(root, crate::ui::UserState::Disabled);
    assert_eq!(
        invoke(&mut app.world, node, &SelectAll),
        Err(RequestError::Unavailable)
    );
    app.world.remove::<crate::ui::UserState>(root);
    let child = app.compose(node, |cx| ui! { View () });
    assert_eq!(
        invoke(&mut app.world, child, &SelectAll),
        Err(RequestError::Unsupported)
    );
    assert_eq!(app.world.get::<Selectable>(node).unwrap().0, 0);
    app.world.despawn(node);
    assert_eq!(
        invoke(&mut app.world, node, &SelectAll),
        Err(RequestError::Unavailable)
    );
}

#[test]
fn request_view_rejects_duplicate_types_before_installing_any_hooks() {
    let mut world = World::new();
    assert!(matches!(
        crate::input::event::view_input::prepare_inputs(
            &mut world,
            (choose::callback(), choose::callback())
        ),
        Err(RuntimeInstallError::DuplicateRequest { .. })
    ));
    crate::input::event::runtime::configure(&mut world, crate::input::event::EventCapacity::new())
        .unwrap();
}
