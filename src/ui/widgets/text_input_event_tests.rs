use super::*;
use crate::ecs::CommandError;
use crate::input::event::{EventCapacity, runtime, view_input};

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
            .on_event(TextInput::Changed, changed::callback())
            .on_event(TextInput::SelectionChanged, selected::callback())
            .on_event(TextInput::Submitted, submitted::callback()),
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
    view_input::invoke_payload(view(), world, node, &CharInput { ch }, None)
}

#[test]
fn input_edits_report_content_before_directed_selection() {
    let (mut world, node) = fixture(EventCapacity::default());
    let input = world.get_mut::<TextInput>(node).unwrap();
    input.replace_selection("A中🙂Z").unwrap();
    input
        .set_selection(TextSelection {
            anchor: 8,
            cursor: 1,
        })
        .unwrap();
    assert!(character(&mut world, node, '界').unwrap());
    assert_eq!(runtime::flush(&mut world).unwrap(), 2);
    assert_eq!(world.get::<TextInput>(node).unwrap().as_str(), "A界Z");
    assert_eq!(
        world.resource::<Trace>().unwrap().as_slice(),
        &[Notice::Content(5), Notice::Selection(4, 4)]
    );
}

#[test]
fn selection_only_edits_and_identical_replacement_do_not_report_content_changes() {
    let (mut world, node) = fixture(EventCapacity::default());
    let input = world.get_mut::<TextInput>(node).unwrap();
    input.replace_selection("abc").unwrap();
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
    assert_eq!(runtime::flush(&mut world).unwrap(), 3);
    assert_eq!(world.get::<TextInput>(node).unwrap().as_str(), "abc");
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
        .get_mut::<TextInput>(node)
        .unwrap()
        .set_capacity(0)
        .unwrap();
    view_input::fixture_inputs(view(), &mut world);
    world.remove::<Dirty>(node);
    assert!(character(&mut world, node, '界').unwrap());
    assert!(press(&mut world, node, KEY_LEFT).unwrap());
    assert!(press(&mut world, node, KEY_DELETE).unwrap());
    assert_eq!(runtime::flush(&mut world).unwrap(), 0);
    assert!(!world.has::<Dirty>(node));
}

#[test]
fn return_reports_each_press_without_editing_or_changing_focus() {
    let (mut world, node) = fixture(EventCapacity::default());
    world.insert_resource(FocusState {
        focused: Some(node),
    });
    let input = world.get_mut::<TextInput>(node).unwrap();
    input.replace_selection("A中").unwrap();
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
    assert_eq!(runtime::flush(&mut world).unwrap(), 2);
    let input = world.get::<TextInput>(node).unwrap();
    assert_eq!(input.as_str(), "A中");
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
    assert_eq!(world.get::<TextInput>(node).unwrap().as_str(), "中");
    assert_eq!(world.get::<TextInput>(node).unwrap().cursor(), 3);
    assert_eq!(runtime::flush(&mut world).unwrap(), 1);
    assert_eq!(runtime::flush(&mut world).unwrap(), 0);
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
    assert_eq!(runtime::flush(&mut world).unwrap(), 0);
}

#[test]
fn prepared_edit_and_submission_callbacks_reuse_default_storage() {
    let (mut world, node) = fixture(EventCapacity::default());
    let character = textinput_character::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut world, view_input::fixture_capacity())
        .unwrap();
    let keyboard = textinput_key::callback::<crate::__macro_support::NodeContext>()
        .prepare(&mut world, view_input::fixture_capacity())
        .unwrap();
    for warm in [true, false] {
        let allocations = crate::test_support::tracked_allocations(|| {
            character
                .invoke(&mut world, &CharInput { ch: '中' }, node)
                .unwrap();
            runtime::flush(&mut world).unwrap();
            keyboard
                .invoke(&mut world, &key(KEY_BACKSPACE, true), node)
                .unwrap();
            runtime::flush(&mut world).unwrap();
            keyboard
                .invoke(&mut world, &key(KEY_RETURN, true), node)
                .unwrap();
            runtime::flush(&mut world).unwrap();
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
