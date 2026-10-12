use super::super::view;
use super::*;
use crate::ecs::CommandError;
use crate::input::event::{EventCapacity, runtime, view_input};
use crate::ui::dirty::Dirty;
use crate::ui::widgets::TextInput;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Notice {
    #[default]
    Empty,
    Content(u32),
    Selection(u32, u32),
    Submit,
}

#[derive(Default)]
struct Trace {
    notices: [Notice; 16],
    len: usize,
}

impl Trace {
    fn push(&mut self, notice: Notice) {
        self.notices[self.len] = notice;
        self.len += 1;
    }

    fn as_slice(&self) -> &[Notice] {
        &self.notices[..self.len]
    }
}

#[crate::event]
fn changed(event: event!(Changed), trace: res!(mut Trace)) {
    trace.push(Notice::Content(event.len));
}

#[crate::event]
fn selected(event: event!(SelectionChanged), trace: res!(mut Trace)) {
    trace.push(Notice::Selection(event.anchor, event.cursor));
}

#[crate::event]
fn submitted(_event: event!(Submitted), trace: res!(mut Trace)) {
    trace.push(Notice::Submit);
}

fn fixture(capacity: EventCapacity) -> (World, Entity) {
    let mut world = World::new();
    world.insert_resource(Trace::default());
    runtime::configure(&mut world, capacity).unwrap();
    let root = world.spawn_empty();
    let node = crate::ui::UiScope::new(&mut world, root).spawn(
        TextInput::build()
            .on_event(TextEditing::Changed, changed::callback())
            .on_event(TextEditing::SelectionChanged, selected::callback())
            .on_event(TextEditing::Submitted, submitted::callback()),
    );
    (world, node)
}

fn key(code: u32, pressed: bool) -> Key {
    Key {
        code,
        pressed,
        modifiers: Some(Default::default()),
        repeat: Some(false),
    }
}

fn press(world: &mut World, node: Entity, code: u32) -> Result<bool, CommandError> {
    view_input::invoke_payload(view(), world, node, &key(code, true), None)
}

fn character(world: &mut World, node: Entity, ch: char) -> Result<bool, CommandError> {
    view_input::invoke_payload(
        view(),
        world,
        node,
        &TextCommitted::new(ch.to_string()),
        None,
    )
}

#[test]
fn tapping_a_prepared_editor_collapses_selection_without_editing_content() {
    use crate::input::event::gesture::Tap;
    use crate::text::TextGeometry;
    use crate::types::{Fixed, Rect};
    use crate::ui::outputs::Outputs;
    use crate::ui::widgets::text_editing::geometry::{TextEditingLayoutHandle, prepare_layout};

    let (mut world, node) = fixture(EventCapacity::default());
    world.insert_resource(crate::render::font::default_font_manager());
    world.insert_resource(Outputs::<TextGeometry>::new_text(
        crate::text::TextLayoutLimits::EMBEDDED,
    ));
    world.insert(node, crate::ui::Style::default());
    let rect = Rect::new(0, 0, 100, 24);
    world.insert(node, crate::ui::ComputedRect(rect));
    // This fixture's parent has no layout; the editor geometry is independent.
    world.remove::<crate::ui::Parent>(node);
    world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .local
        .as_mut()
        .unwrap()
        .set_text("A中Z")
        .unwrap();
    let handle = prepare_layout(&world, node, rect).unwrap().unwrap();
    world.insert(node, TextEditingLayoutHandle(handle));
    let callback = super::tap::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut world, view_input::fixture_capacity())
        .unwrap();
    for warm in [true, false] {
        world
            .get_mut::<TextEditing>(node)
            .unwrap()
            .set_selection(TextSelection {
                anchor: 0,
                cursor: 5,
            })
            .unwrap();
        let x = {
            let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
            let layout = storage.layouts.get(handle).unwrap();
            crate::types::fixed::from_textflow(
                layout
                    .carets()
                    .iter()
                    .find(|caret| caret.text_offset == 4)
                    .unwrap()
                    .position
                    .x,
            ) + Fixed::from_int(2)
        };
        *world.resource_mut::<Trace>().unwrap() = Trace::default();
        let allocations = crate::test_support::tracked_allocations(|| {
            callback
                .invoke(
                    &mut world,
                    &Tap {
                        x,
                        y: 5.into(),
                        target: node,
                    },
                    node,
                )
                .unwrap();
            assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 1);
            callback
                .invoke(
                    &mut world,
                    &Tap {
                        x,
                        y: 5.into(),
                        target: node,
                    },
                    node,
                )
                .unwrap();
            assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
        });
        if !warm {
            assert_eq!(allocations, 0);
        }
        assert_eq!(
            world.resource::<Trace>().unwrap().as_slice(),
            &[Notice::Selection(4, 4)]
        );
        let editor = world.get::<TextEditing>(node).unwrap();
        assert_eq!(
            editor.selection(),
            TextSelection {
                anchor: 4,
                cursor: 4
            }
        );
        editor.read(|text| assert_eq!(text, "A中Z"));
    }
}

#[test]
fn input_edits_report_content_before_directed_selection() {
    let (mut world, node) = fixture(EventCapacity::default());
    let input = world.get_mut::<TextEditing>(node).unwrap();
    input.local.as_mut().unwrap().set_text("A中🙂Z").unwrap();
    input
        .set_selection(TextSelection {
            anchor: 8,
            cursor: 1,
        })
        .unwrap();
    assert!(character(&mut world, node, '界').unwrap());
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 2);
    assert_eq!(
        world
            .get::<TextEditing>(node)
            .unwrap()
            .read(|text| text.to_owned()),
        "A界Z"
    );
    assert_eq!(
        world.resource::<Trace>().unwrap().as_slice(),
        &[Notice::Content(5), Notice::Selection(4, 4)]
    );
}

#[test]
fn complete_fragments_replace_selection_once_and_reject_atomically() {
    let (mut world, node) = fixture(EventCapacity::default());
    let input = world.get_mut::<TextEditing>(node).unwrap();
    input.local.as_mut().unwrap().set_text("A中🙂Z").unwrap();
    input
        .set_selection(TextSelection {
            anchor: 8,
            cursor: 1,
        })
        .unwrap();
    let callback = committed::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut world, view_input::fixture_capacity())
        .unwrap();
    callback
        .invoke(&mut world, &TextCommitted::new("文e\u{301}"), node)
        .unwrap();
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 2);
    assert_eq!(
        world
            .get::<TextEditing>(node)
            .unwrap()
            .read(|text| text.to_owned()),
        "A文e\u{301}Z"
    );
    assert_eq!(
        world.resource::<Trace>().unwrap().as_slice(),
        &[Notice::Content(8), Notice::Selection(7, 7)]
    );
    *world.resource_mut::<Trace>().unwrap() = Trace::default();
    world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .set_selection(TextSelection {
            anchor: 1,
            cursor: 7,
        })
        .unwrap();
    world.remove::<Dirty>(node);
    for text in [
        "x\ny",
        "this entire replacement exceeds the declared byte capacity",
    ] {
        callback
            .invoke(&mut world, &TextCommitted::new(text), node)
            .unwrap();
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
        let input = world.get::<TextEditing>(node).unwrap();
        assert_eq!(input.read(|text| text.to_owned()), "A文e\u{301}Z");
        assert_eq!(
            input.selection(),
            TextSelection {
                anchor: 1,
                cursor: 7
            }
        );
        assert!(!world.has::<Dirty>(node));
    }
}

#[test]
fn large_owned_fragment_uses_the_widget_capacity_without_transport_slots() {
    let (mut world, node) = fixture(EventCapacity::default());
    world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .local
        .as_mut()
        .unwrap()
        .set_capacity(512)
        .unwrap();
    let callback = committed::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut world, view_input::fixture_capacity())
        .unwrap();
    callback
        .invoke(&mut world, &TextCommitted::new("warm"), node)
        .unwrap();
    crate::ui::delivery::flush(&mut world).unwrap();
    *world.resource_mut::<Trace>().unwrap() = Trace::default();
    world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .set_selection(TextSelection {
            anchor: 0,
            cursor: 4,
        })
        .unwrap();
    let event = TextCommitted::new("界".repeat(100));
    let allocations = crate::test_support::tracked_allocations(|| {
        callback.invoke(&mut world, &event, node).unwrap();
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 2);
    });
    assert_eq!(allocations, 0);
    assert_eq!(
        world
            .get::<TextEditing>(node)
            .unwrap()
            .read(|text| text.to_owned()),
        event.text()
    );
    assert_eq!(
        world.resource::<Trace>().unwrap().as_slice(),
        &[Notice::Content(300), Notice::Selection(300, 300)]
    );
}

#[test]
fn empty_commit_without_selection_does_not_dirty() {
    let (mut world, node) = fixture(EventCapacity::default());
    let callback = committed::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut world, view_input::fixture_capacity())
        .unwrap();
    world.remove::<Dirty>(node);
    callback
        .invoke(&mut world, &TextCommitted::new(""), node)
        .unwrap();
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
    assert!(!world.has::<Dirty>(node));
}

#[test]
fn shift_navigation_keeps_the_anchor_across_utf8_and_direction_changes() {
    let (mut world, node) = fixture(EventCapacity::default());
    let input = world.get_mut::<TextEditing>(node).unwrap();
    input.local.as_mut().unwrap().set_text("A中🙂Z").unwrap();
    input
        .set_selection(TextSelection {
            anchor: 4,
            cursor: 4,
        })
        .unwrap();
    let callback = super::key::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut world, view_input::fixture_capacity())
        .unwrap();
    for (code, cursor) in [
        (KEY_LEFT, 1),
        (KEY_LEFT, 0),
        (KEY_LEFT, 0),
        (KEY_RIGHT, 1),
        (KEY_RIGHT, 4),
        (KEY_RIGHT, 8),
        (KEY_END, 9),
        (KEY_END, 9),
        (KEY_HOME, 0),
    ] {
        let previous = world.get::<TextEditing>(node).unwrap().cursor();
        world.remove::<Dirty>(node);
        let mut event = key(code, true);
        event.modifiers.as_mut().unwrap().shift = true;
        callback.invoke(&mut world, &event, node).unwrap();
        assert_eq!(
            crate::ui::delivery::flush(&mut world).unwrap(),
            usize::from(previous != cursor)
        );
        let input = world.get::<TextEditing>(node).unwrap();
        assert_eq!(input.selection(), TextSelection { anchor: 4, cursor });
        assert_eq!(input.read(|text| text.to_owned()), "A中🙂Z");
        if previous == cursor {
            assert!(!world.has::<Dirty>(node));
        }
    }
    callback
        .invoke(&mut world, &key(KEY_RIGHT, true), node)
        .unwrap();
    crate::ui::delivery::flush(&mut world).unwrap();
    assert_eq!(
        world.get::<TextEditing>(node).unwrap().selection(),
        TextSelection {
            anchor: 4,
            cursor: 4
        }
    );
    assert_eq!(
        world.resource::<Trace>().unwrap().as_slice(),
        &[
            Notice::Selection(4, 1),
            Notice::Selection(4, 0),
            Notice::Selection(4, 1),
            Notice::Selection(4, 4),
            Notice::Selection(4, 8),
            Notice::Selection(4, 9),
            Notice::Selection(4, 0),
            Notice::Selection(4, 4),
        ]
    );
}

#[test]
fn prepared_shift_navigation_reuses_commands_and_notifications() {
    let (mut world, node) = fixture(EventCapacity::default());
    world.insert(node, TextEditing::default().with_text("中🙂").unwrap());
    let callback = super::key::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut world, view_input::fixture_capacity())
        .unwrap();
    for warm in [true, false] {
        let allocations = crate::test_support::tracked_allocations(|| {
            for (code, shift) in [
                (KEY_HOME, true),
                (KEY_END, true),
                (KEY_HOME, false),
                (KEY_END, false),
            ] {
                let mut event = key(code, true);
                event.modifiers.as_mut().unwrap().shift = shift;
                callback.invoke(&mut world, &event, node).unwrap();
                crate::ui::delivery::flush(&mut world).unwrap();
            }
        });
        if !warm {
            assert_eq!(allocations, 0);
        }
        assert_eq!(
            world.get::<TextEditing>(node).unwrap().selection(),
            TextSelection {
                anchor: 7,
                cursor: 7
            }
        );
        *world.resource_mut::<Trace>().unwrap() = Trace::default();
    }
}

#[test]
fn selection_only_edits_and_identical_replacement_do_not_report_content_changes() {
    let (mut world, node) = fixture(EventCapacity::default());
    let input = world.get_mut::<TextEditing>(node).unwrap();
    input.local.as_mut().unwrap().set_text("abc").unwrap();
    input
        .set_selection(TextSelection {
            anchor: 2,
            cursor: 1,
        })
        .unwrap();
    assert!(character(&mut world, node, 'b').unwrap());
    assert!(press(&mut world, node, KEY_HOME).unwrap());
    assert!(press(&mut world, node, KEY_HOME).unwrap());
    assert!(press(&mut world, node, KEY_END).unwrap());
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 3);
    assert_eq!(
        world
            .get::<TextEditing>(node)
            .unwrap()
            .read(|text| text.to_owned()),
        "abc"
    );
    assert_eq!(
        world.resource::<Trace>().unwrap().as_slice(),
        &[
            Notice::Selection(2, 2),
            Notice::Selection(0, 0),
            Notice::Selection(3, 3),
        ]
    );
}

#[test]
fn rejected_edits_and_boundary_movement_do_not_notify_or_dirty() {
    let (mut world, node) = fixture(EventCapacity::default());
    world
        .get_mut::<TextEditing>(node)
        .unwrap()
        .local
        .as_mut()
        .unwrap()
        .set_capacity(0)
        .unwrap();
    view_input::fixture_inputs(view(), &mut world);
    world.remove::<Dirty>(node);
    assert!(character(&mut world, node, '界').unwrap());
    assert!(press(&mut world, node, KEY_LEFT).unwrap());
    assert!(press(&mut world, node, KEY_DELETE).unwrap());
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
    assert!(!world.has::<Dirty>(node));
}

#[test]
fn return_reports_each_press_without_editing_or_changing_focus() {
    let (mut world, node) = fixture(EventCapacity::default());
    world.insert_resource(FocusState {
        focused: Some(node),
    });
    let input = world.get_mut::<TextEditing>(node).unwrap();
    input.local.as_mut().unwrap().set_text("A中").unwrap();
    input
        .set_selection(TextSelection {
            anchor: 4,
            cursor: 1,
        })
        .unwrap();
    view_input::fixture_inputs(view(), &mut world);
    world.remove::<Dirty>(node);
    assert!(press(&mut world, node, KEY_RETURN).unwrap());
    assert!(
        !view_input::invoke_payload(view(), &mut world, node, &key(KEY_RETURN, false), None)
            .unwrap()
    );
    assert!(press(&mut world, node, KEY_RETURN).unwrap());
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 2);
    let input = world.get::<TextEditing>(node).unwrap();
    assert_eq!(input.read(|text| text.to_owned()), "A中");
    assert_eq!(
        input.selection(),
        TextSelection {
            anchor: 4,
            cursor: 1
        }
    );
    assert_eq!(world.resource::<FocusState>().unwrap().focused, Some(node));
    assert!(!world.has::<Dirty>(node));
    assert_eq!(
        world.resource::<Trace>().unwrap().as_slice(),
        &[Notice::Submit, Notice::Submit]
    );
}

#[test]
fn notification_overflow_keeps_the_edit_without_retrying_missing_selection() {
    let (mut world, node) = fixture(
        EventCapacity::new()
            .notifications::<SelectionChanged>(1)
            .commands(view_input::fixture_capacity())
            .flush_limit(1),
    );
    assert!(character(&mut world, node, '中').is_err());
    assert_eq!(
        world
            .get::<TextEditing>(node)
            .unwrap()
            .read(|text| text.to_owned()),
        "中"
    );
    assert_eq!(world.get::<TextEditing>(node).unwrap().cursor(), 3);
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 1);
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
    assert_eq!(
        world.resource::<Trace>().unwrap().as_slice(),
        &[Notice::Content(3)]
    );
}

#[test]
fn unobserved_submission_needs_no_notification_storage() {
    let mut world = World::new();
    runtime::configure(
        &mut world,
        EventCapacity::new().commands(view_input::fixture_capacity()),
    )
    .unwrap();
    let root = world.spawn_empty();
    let node = crate::ui::UiScope::new(&mut world, root).spawn(TextInput::build());
    assert!(press(&mut world, node, KEY_RETURN).unwrap());
    assert!(character(&mut world, node, 'a').unwrap());
    assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
}

#[test]
fn prepared_edit_and_submission_callbacks_reuse_default_storage() {
    let (mut world, node) = fixture(EventCapacity::default());
    let character = committed::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut world, view_input::fixture_capacity())
        .unwrap();
    let keyboard = super::key::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut world, view_input::fixture_capacity())
        .unwrap();
    for warm in [true, false] {
        let allocations = crate::test_support::tracked_allocations(|| {
            character
                .invoke(&mut world, &TextCommitted::new("中"), node)
                .unwrap();
            crate::ui::delivery::flush(&mut world).unwrap();
            keyboard
                .invoke(&mut world, &key(KEY_BACKSPACE, true), node)
                .unwrap();
            crate::ui::delivery::flush(&mut world).unwrap();
            keyboard
                .invoke(&mut world, &key(KEY_RETURN, true), node)
                .unwrap();
            crate::ui::delivery::flush(&mut world).unwrap();
        });
        if !warm {
            assert_eq!(allocations, 0);
        }
        assert_eq!(
            world.resource::<Trace>().unwrap().as_slice(),
            &[
                Notice::Content(3),
                Notice::Selection(3, 3),
                Notice::Content(0),
                Notice::Selection(0, 0),
                Notice::Submit,
            ]
        );
        *world.resource_mut::<Trace>().unwrap() = Trace::default();
    }
}
