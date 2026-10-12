use super::*;
use crate::core::model::BindType;
use crate::prelude::*;
use crate::ui::widgets::{TextDocument, TextInput, TextSelection};

#[crate::model]
#[derive(Default)]
struct Provider {
    session: Option<TextInputSession>,
    calls: u32,
}
#[crate::model]
impl Provider {
    fn update(&mut self, state: Option<TextInputState>) {
        self.session = state.map(|state| state.session);
        self.calls += 1;
    }
    fn session(&self) -> Option<TextInputSession> {
        self.session
    }
}
#[crate::event]
fn changed(event: event!(TextInputChanged), provider: model!(Provider)) {
    provider.update(event.state);
}

#[crate::event]
fn spelling(provider: model!(Provider), commands: commands!()) {
    if let Some(session) = provider.session() {
        commands.text_input().preedit(session, "ni", Some(2..2));
    }
}

#[crate::compose]
fn panel(provider: model!(Provider)) -> (Entity, Entity) {
    let editor = ui! {
        TextInput (width: 160, height: 36, capacity: 32, preedit_capacity: 8)
    };
    let key = ui! {
        View (width: 24, height: 24) [
            PointerFocus::Preserve,
        ] on Tap(spelling::callback(provider))
    };
    (editor, key)
}

type TestApp = App<crate::surface::framebuf::FramebufSurface<fn(&[u8], PhysicalRect)>>;
type ProviderModel = <Provider as BindType>::Shared;
fn fixture() -> (TestApp, Entity, Entity, ProviderModel) {
    let mut app = App::headless(200, 100);
    app.with_default_widgets().with_default_systems();
    let provider = app.add_model(Provider::default());
    app.with_input_method(InputMethod::software(
        (changed::callback(provider.clone()),),
    ));
    let root = app.spawn_root().id();
    let (editor, key) = app.compose(root, panel(provider.clone()));
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
    super::super::delivery::flush(&mut app.world).unwrap();
    (app, editor, key, provider)
}
fn run(app: &mut TestApp, request: Request) {
    enqueue(&mut app.world, request);
    flush(&mut app.world);
}
fn preview(session: TextInputSession, text: &'static str) -> Request {
    Request::Preedit(
        session,
        CompositionUpdated {
            text: text.into(),
            selection: Some(text.len()..text.len()),
        },
    )
}

fn native(app: &mut TestApp, event: crate::surface::InputEvent) {
    crate::input::event::deliver_input(&mut app.world, None, &event, 0, 200, 100);
}

#[test]
fn native_composition_borrows_payload_and_keeps_empty_preview_until_commit() {
    let (mut app, editor, _, _) = fixture();
    app.with_input_method(InputMethod::native());
    let session = native_state(&mut app.world, 0).unwrap().session;
    let text = alloc::string::String::from("A中🙂");
    let pointer = text.as_ptr();
    let event = session.commit(text);
    let crate::surface::InputEvent::TextInput(TextInputEvent(Request::Commit(_, text))) = &event
    else {
        panic!("native commit")
    };
    assert_eq!(text.text().as_ptr(), pointer);
    native(&mut app, event);
    let allocations = crate::test_support::tracked_allocations(|| {
        native(&mut app, session.preedit("ni", Some(2..2)));
        native(&mut app, session.preedit("", Some(0..0)));
        assert!(
            service(&app.world)
                .unwrap()
                .state
                .borrow()
                .active
                .as_ref()
                .unwrap()
                .composition
                .is_some()
        );
        native(&mut app, session.commit("你"));
        assert_eq!(native_state(&mut app.world, 0).unwrap().session, session);
    });
    assert_eq!(allocations, 0);
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, "A中🙂你"));
    let state = service(&app.world).unwrap();
    let state = state.state.borrow();
    assert!(state.active.as_ref().unwrap().composition.is_none());
    assert_eq!(state.pending.capacity(), 0);
}

#[test]
fn native_sessions_reject_retired_targets_foreign_apps_and_software_tokens() {
    let (mut app, editor, _, provider) = fixture();
    let software = provider.session().unwrap();
    app.with_input_method(InputMethod::native());
    let first = native_state(&mut app.world, 0).unwrap().session;
    native(&mut app, software.commit("old"));
    app.world.resource_mut::<FocusState>().unwrap().focused = None;
    assert!(native_state(&mut app.world, 0).is_none());
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
    let next = native_state(&mut app.world, 0).unwrap().session;
    assert_ne!(first, next);
    native(&mut app, first.commit("old"));
    let (mut other, _, _, _) = fixture();
    other.with_input_method(InputMethod::native());
    let foreign = native_state(&mut other.world, 0).unwrap().session;
    native(&mut app, foreign.commit("foreign"));
    native(&mut app, next.preedit("ni", Some(2..2)));
    native(&mut app, next.cancel());
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, ""));
    native(&mut app, next.commit("OK"));
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, "OK"));
}

#[test]
fn native_service_is_lazy_and_native_mode_does_not_notify_the_software_provider() {
    let mut empty = App::headless(200, 100);
    assert!(native_state(&mut empty.world, 0).is_none());
    assert!(service(&empty.world).is_none());
    let (mut app, _, _, provider) = fixture();
    run(&mut app, Request::Mode(TextInputMode::Native));
    assert_eq!(provider.session(), None);
    let state = native_state(&mut app.world, 0).unwrap();
    native(&mut app, state.session.commit("OK"));
    assert_eq!(provider.session(), None);
    run(&mut app, Request::Mode(TextInputMode::Software));
    let software = provider.session().unwrap();
    assert!(native_state(&mut app.world, 0).is_none());
    native(&mut app, software.commit("wrong producer"));
    assert_eq!(provider.session(), Some(software));
}

#[test]
fn composition_is_display_only_and_confirmed_text_replaces_the_original_selection() {
    let (mut app, editor, _, provider) = fixture();
    let document = app.add_model(TextDocument::new());
    document.set_text("A🙂Z").unwrap();
    let mut editing = TextEditing::new(document.clone());
    editing
        .set_selection(TextSelection {
            anchor: 1,
            cursor: 5,
        })
        .unwrap();
    app.world.insert(editor, editing);
    super::super::delivery::flush(&mut app.world).unwrap();
    let session = provider.session().unwrap();
    let allocations = crate::test_support::tracked_allocations(|| {
        run(&mut app, preview(session, "ni"));
        document.read(|text| assert_eq!(text, "A🙂Z"));
        app.world
            .get::<TextEditing>(editor)
            .unwrap()
            .read_display(|text| assert_eq!(text, "AniZ"));
        run(&mut app, Request::Commit(session, TextCommitted::new("你")));
        document.read(|text| assert_eq!(text, "A你Z"));
        assert_eq!(app.world.get::<TextEditing>(editor).unwrap().cursor(), 4);
    });
    assert_eq!(allocations, 0);
    assert_eq!(provider.session(), Some(session));
}

#[test]
fn event_commands_keep_focus_and_deliver_after_the_outer_snapshot_is_released() {
    let (mut app, editor, key, _) = fixture();
    super::super::delivery::dispatch_gesture(
        &mut app.world,
        &crate::input::event::gesture::GestureEvent::Tap {
            target: key,
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        },
        1,
    )
    .unwrap();
    super::super::delivery::flush(&mut app.world).unwrap();
    assert_eq!(
        app.world.resource::<FocusState>().unwrap().focused,
        Some(editor)
    );
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, "ni"));
}

#[test]
fn external_edits_focus_and_component_replacement_retire_old_sessions() {
    let (mut app, editor, _, provider) = fixture();
    let document = app.add_model(TextDocument::new());
    app.world.insert(editor, TextEditing::new(document.clone()));
    super::super::delivery::flush(&mut app.world).unwrap();
    let old = provider.session().unwrap();
    run(&mut app, preview(old, "ni"));
    document.set_text("external").unwrap();
    run(&mut app, Request::Commit(old, TextCommitted::new("wrong")));
    document.read(|text| assert_eq!(text, "external"));
    assert_ne!(provider.session(), Some(old));
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, "external"));
    let old = provider.session().unwrap();
    app.world.resource_mut::<FocusState>().unwrap().focused = None;
    flush(&mut app.world);
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
    run(&mut app, Request::Commit(old, TextCommitted::new("wrong")));
    document.read(|text| assert_eq!(text, "external"));
    let old = provider.session().unwrap();
    app.world.insert(editor, TextEditing::default());
    run(&mut app, Request::Commit(old, TextCommitted::new("wrong")));
    assert!(app.world.get::<TextEditing>(editor).unwrap().is_empty());
}

#[test]
fn oversized_preview_clears_display_but_later_valid_updates_recover() {
    let (mut app, editor, _, provider) = fixture();
    let session = provider.session().unwrap();
    run(&mut app, preview(session, "ni"));
    run(&mut app, preview(session, "too long to fit"));
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, ""));
    assert_eq!(provider.session(), Some(session));
    run(&mut app, preview(session, "n"));
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, "n"));
    run(&mut app, Request::Cancel(session));
    assert!(app.world.get::<TextEditing>(editor).unwrap().is_empty());
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, ""));
}

#[test]
fn rejected_commit_still_ends_composition_and_modes_do_not_reuse_tokens() {
    #[crate::compose]
    fn reject() -> Entity {
        ui! {
            TextInput () on TextCommitted(|_| true)
        }
    }
    let (mut app, _, _, provider) = fixture();
    let root = app.spawn_root().id();
    let editor = app.compose(root, reject());
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
    flush(&mut app.world);
    let session = provider.session().unwrap();
    run(&mut app, preview(session, "ni"));
    run(&mut app, Request::Commit(session, TextCommitted::new("你")));
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, ""));
    assert!(
        service(&app.world)
            .unwrap()
            .state
            .borrow()
            .active
            .as_ref()
            .unwrap()
            .composition
            .is_none()
    );
    run(&mut app, Request::Mode(TextInputMode::Native));
    assert!(provider.session().is_none());
    run(&mut app, Request::Mode(TextInputMode::Software));
    assert_ne!(provider.session(), Some(session));
    run(
        &mut app,
        Request::Commit(session, TextCommitted::new("wrong")),
    );
    assert!(app.world.get::<TextEditing>(editor).unwrap().is_empty());
}

#[test]
fn host_unavailability_cancels_without_dropping_the_editors_focus() {
    let (mut app, editor, _, provider) = fixture();
    let session = provider.session().unwrap();
    run(&mut app, preview(session, "ni"));
    app.suspend();
    assert!(provider.session().is_none());
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, ""));
    app.resume();
    assert_ne!(provider.session(), Some(session));
    assert!(provider.session().is_some());
}

#[derive(Default)]
struct Trace {
    updated: usize,
    changed: usize,
    keys: usize,
}
#[crate::event]
fn updated(
    entity: entity!(),
    event: event!(CompositionUpdated),
    editors: query!(TextEditing),
    trace: res!(mut Trace),
) {
    let editor = editors.get(entity).unwrap();
    assert!(editor.is_empty());
    editor.read_display(|display| assert_eq!(display, event.text()));
    trace.updated += 1;
}
#[crate::event]
fn document_changed(
    _event: event!(crate::ui::widgets::text_editing::Changed),
    trace: res!(mut Trace),
) {
    trace.changed += 1;
}

#[test]
fn composition_facts_use_all_callback_forms() {
    #[crate::compose]
    fn observed(started: Rc<Cell<usize>>, ended: Rc<Cell<usize>>) -> Entity {
        ui! {
            TextInput () on CompositionStarted { started.set(started.get() + 1); } on CompositionUpdated(updated::callback()) on CompositionEnded(move |event: &CompositionEnded| {
                assert_eq!(event.reason, CompositionEndReason::Committed);
                ended.set(ended.get() + 1);
            }) on TextEditing::Changed(document_changed::callback())
        }
    }
    let (mut app, _, _, provider) = fixture();
    app.add_resource(Trace::default());
    let started = Rc::new(Cell::new(0));
    let ended = Rc::new(Cell::new(0));
    let root = app.spawn_root().id();
    let editor = app.compose(root, observed(started.clone(), ended.clone()));
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
    flush(&mut app.world);
    let session = provider.session().unwrap();
    run(&mut app, preview(session, "ni"));
    run(&mut app, Request::Commit(session, TextCommitted::new("你")));
    assert_eq!(started.get(), 1);
    assert_eq!(ended.get(), 1);
    let trace = app.world.resource::<Trace>().unwrap();
    assert_eq!(trace.updated, 1);
    assert_eq!(trace.changed, 1);
}

#[crate::event]
fn move_selection(
    entity: entity!(),
    _event: event!(TextCommitted),
    editors: query!(mut TextEditing),
) -> bool {
    editors
        .get(entity)
        .unwrap()
        .set_selection(TextSelection {
            anchor: 0,
            cursor: 0,
        })
        .unwrap();
    false
}
#[test]
fn a_commit_listener_cannot_redirect_a_frozen_composition_to_another_selection() {
    #[crate::compose]
    fn observed() -> Entity {
        ui! {
            TextInput () [
                TextEditing::default().with_text("AB").unwrap(),
            ] on TextCommitted(move_selection::callback())
        }
    }
    let (mut app, _, _, provider) = fixture();
    let root = app.spawn_root().id();
    let editor = app.compose(root, observed());
    app.world.resource_mut::<FocusState>().unwrap().focused = Some(editor);
    flush(&mut app.world);
    let session = provider.session().unwrap();
    run(&mut app, preview(session, "ni"));
    run(&mut app, Request::Commit(session, TextCommitted::new("你")));
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read_display(|text| assert_eq!(text, "AB"));
}

#[test]
fn requests_are_ordered_bounded_and_not_replayed_after_a_capacity_failure() {
    let (mut app, editor, _, provider) = fixture();
    let session = provider.session().unwrap();
    let capacity = service(&app.world).unwrap().state.borrow().capacity;
    for _ in 0..capacity + 2 {
        enqueue(
            &mut app.world,
            Request::Commit(session, TextCommitted::new("a")),
        );
    }
    flush(&mut app.world);
    assert_eq!(
        app.world.get::<TextEditing>(editor).unwrap().len(),
        capacity
    );
    flush(&mut app.world);
    assert_eq!(
        app.world.get::<TextEditing>(editor).unwrap().len(),
        capacity
    );
    enqueue(&mut app.world, Request::Mode(TextInputMode::Native));
    enqueue(
        &mut app.world,
        Request::Commit(session, TextCommitted::new("wrong")),
    );
    enqueue(&mut app.world, Request::Mode(TextInputMode::Software));
    flush(&mut app.world);
    assert_eq!(
        app.world.get::<TextEditing>(editor).unwrap().len(),
        capacity
    );
    assert_ne!(provider.session(), Some(session));
}

#[test]
fn a_failed_provider_replacement_keeps_the_current_session_and_callbacks() {
    let (mut app, _, _, provider) = fixture();
    let session = provider.session();
    app.with_input_method(InputMethod::software((
        changed::callback(provider.clone()),
        changed::callback(provider.clone()),
    )));
    assert_eq!(provider.session(), session);
    assert_eq!(
        service(&app.world)
            .unwrap()
            .state
            .borrow()
            .active
            .as_ref()
            .unwrap()
            .state
            .session,
        session.unwrap()
    );
}

#[crate::event]
fn provider_key(event: event!(Key), trace: res!(mut Trace)) -> bool {
    trace.keys += 1;
    event.code == crate::input::event::input::KEY_BACKSPACE
}

#[test]
fn software_keys_intercept_platform_input_but_do_not_reenter_for_provider_output() {
    use crate::input::event::input::{InputEvent, KEY_BACKSPACE, KEY_LEFT};
    let (mut app, editor, _, provider) = fixture();
    app.add_resource(Trace::default());
    app.with_input_method(InputMethod::software((
        changed::callback(provider.clone()),
        provider_key::callback(),
    )));
    let session = provider.session().unwrap();
    run(&mut app, Request::Commit(session, TextCommitted::new("ab")));
    let key = Key {
        code: KEY_BACKSPACE,
        pressed: true,
        modifiers: None,
        repeat: None,
    };
    for code in [KEY_BACKSPACE, KEY_LEFT] {
        crate::input::event::deliver_input(
            &mut app.world,
            app.root,
            &InputEvent::Key {
                code,
                pressed: true,
                modifiers: None,
                repeat: None,
            },
            1,
            200,
            100,
        );
    }
    assert_eq!(app.world.resource::<Trace>().unwrap().keys, 2);
    assert_eq!(app.world.get::<TextEditing>(editor).unwrap().cursor(), 1);
    run(&mut app, Request::Key(session, key));
    app.world
        .get::<TextEditing>(editor)
        .unwrap()
        .read(|text| assert_eq!(text, "b"));
    assert_eq!(app.world.resource::<Trace>().unwrap().keys, 2);
    crate::input::event::deliver_input(
        &mut app.world,
        app.root,
        &TextCommitted::new("ignored").into(),
        2,
        200,
        100,
    );
    assert_eq!(app.world.get::<TextEditing>(editor).unwrap().len(), 1);
}

#[test]
fn preedit_partial_and_full_frames_match_and_caret_tracks_display_layout() {
    use crate::surface::FramebufferAccess;
    let (mut app, editor, _, provider) = fixture();
    app.tick();
    app.render().unwrap();
    let session = provider.session().unwrap();
    for text in ["n", "ni", "n", ""] {
        run(&mut app, preview(session, text));
        app.tick();
        app.render_dirty().unwrap();
        let dirty = app.backend.framebuffer().buf.as_slice().to_vec();
        app.render().unwrap();
        assert_eq!(dirty.as_slice(), app.backend.framebuffer().buf.as_slice());
        assert!(
            super::super::widgets::text_editing::geometry::caret_rect(&app.world, editor).is_some()
        );
    }
}
