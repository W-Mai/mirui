extern crate alloc;

use alloc::format;

use crate::input::event::BubbleControl;
#[cfg(feature = "std")]
use crate::prelude::plugin::InputFeedbackPlugin;
use crate::prelude::*;
use crate::ui::UserState;
use crate::ui::dirty::Dirty;
use crate::ui::widgets::{
    Checkbox, ParagraphStyle, Placeholder, Switch, Text, TextAlign, TextInput, TextVerticalAlign,
    TextWrap,
};
pub const VIEWPORT: (u16, u16) = (1024, 720);

const BACKGROUND: Color = Color::rgb(9, 16, 28);
const PANEL: Color = Color::rgb(17, 30, 49);
const PANEL_ALT: Color = Color::rgb(21, 38, 60);
const BORDER: Color = Color::rgb(47, 73, 101);
const TEXT: Color = Color::rgb(232, 240, 248);
const MUTED: Color = Color::rgb(139, 163, 188);
const CYAN: Color = Color::rgb(86, 226, 205);
const BLUE: Color = Color::rgb(102, 161, 255);
const VIOLET: Color = Color::rgb(179, 132, 255);
const GOLD: Color = Color::rgb(255, 198, 92);
const ERROR: Color = Color::rgb(244, 96, 112);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InteractionLabState {
    single: u16,
    double: u16,
    triple: u16,
    long: u16,
    switch_on: bool,
    switch_changes: u16,
    checkbox_on: bool,
    checkbox_changes: u16,
    drag_x: Fixed,
    drag_y: Fixed,
    errored: bool,
    disabled: bool,
    allow_bubble: bool,
    child_taps: u16,
    parent_taps: u16,
}

impl Default for InteractionLabState {
    fn default() -> Self {
        Self {
            single: 0,
            double: 0,
            triple: 0,
            long: 0,
            switch_on: false,
            switch_changes: 0,
            checkbox_on: false,
            checkbox_changes: 0,
            drag_x: Fixed::ZERO,
            drag_y: Fixed::ZERO,
            errored: true,
            disabled: true,
            allow_bubble: false,
            child_taps: 0,
            parent_taps: 0,
        }
    }
}

#[derive(Clone)]
struct InteractionModel {
    state: Signal<InteractionLabState>,
}

impl Default for InteractionModel {
    fn default() -> Self {
        Self {
            state: Signal::new(InteractionLabState::default()),
        }
    }
}

impl InteractionModel {
    fn signal(&self) -> Signal<InteractionLabState> {
        self.state.clone()
    }
}

enum InteractionAction {
    Single,
    Double,
    Triple,
    Long,
    Switch(bool),
    Checkbox(bool),
    Drag(Fixed, Fixed),
    ResetDrag,
    ToggleError,
    ToggleDisabled,
    ToggleBubble,
    ChildTap,
    ParentTap,
}

impl InteractionAction {
    fn publish(self, signal: &Signal<InteractionLabState>) {
        let current = signal.get_untracked();
        let mut next = current;
        match self {
            Self::Single => next.single = next.single.saturating_add(1),
            Self::Double => next.double = next.double.saturating_add(1),
            Self::Triple => next.triple = next.triple.saturating_add(1),
            Self::Long => next.long = next.long.saturating_add(1),
            Self::Switch(on) => {
                next.switch_on = on;
                next.switch_changes = next.switch_changes.saturating_add(1);
            }
            Self::Checkbox(on) => {
                next.checkbox_on = on;
                next.checkbox_changes = next.checkbox_changes.saturating_add(1);
            }
            Self::Drag(x, y) => {
                next.drag_x = x;
                next.drag_y = y;
            }
            Self::ResetDrag => {
                next.drag_x = Fixed::ZERO;
                next.drag_y = Fixed::ZERO;
            }
            Self::ToggleError => next.errored = !next.errored,
            Self::ToggleDisabled => next.disabled = !next.disabled,
            Self::ToggleBubble => next.allow_bubble = !next.allow_bubble,
            Self::ChildTap => next.child_taps = next.child_taps.saturating_add(1),
            Self::ParentTap => next.parent_taps = next.parent_taps.saturating_add(1),
        }
        if next != current {
            signal.set(next);
        }
    }
}

fn model_signal(cx: &mut crate::ui::UiScope<'_>) -> Signal<InteractionLabState> {
    if cx.world_mut().resource::<InteractionModel>().is_none() {
        cx.world_mut().insert_resource(InteractionModel::default());
    }
    cx.world_mut()
        .resource::<InteractionModel>()
        .map(InteractionModel::signal)
        .expect("Interaction Lab model")
}

fn centered_label() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        align: TextAlign::Center,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

fn sync_user_state(
    world: &mut World,
    id: &'static str,
    enabled: bool,
    state: fn() -> UserState,
    matches: fn(&UserState) -> bool,
) {
    let Some(entity) = world.find_by_id(id) else {
        return;
    };
    let already = world.get::<UserState>(entity).is_some_and(matches);
    if already == enabled {
        return;
    }
    if enabled {
        world.insert(entity, state());
    } else {
        world.remove::<UserState>(entity);
    }
    world.insert(entity, Dirty);
}

#[mirui_macros::system]
fn sync_interaction_user_states(world: &mut World) {
    let Some(state) = world
        .resource::<InteractionModel>()
        .map(|model| model.state.get_untracked())
    else {
        return;
    };
    sync_user_state(
        world,
        "interaction_error_target",
        state.errored,
        || UserState::Errored,
        |value| matches!(value, UserState::Errored),
    );
    sync_user_state(
        world,
        "interaction_disabled_target",
        state.disabled,
        || UserState::Disabled,
        |value| matches!(value, UserState::Disabled),
    );
    if let Some(entity) = world.find_by_id("interaction_drag_target") {
        crate::ui::set_position(
            world,
            entity,
            Fixed::from_int(24) + state.drag_x,
            Fixed::from_int(27) + state.drag_y,
        );
    }
}

#[compose]
fn compose_header() -> Entity {
    ui! {
        Row (
            id: "interaction_lab_header",
            height: 62,
            align: AlignItems::Center,
            column_gap: 14
        ) {
            View (width: 8, height: 42, bg_color: CYAN, border_radius: 4)
            Column (grow: 1.0, row_gap: 3) {
                Text ("INTERACTION LAB", font_size: 24, text_color: TEXT)
                Text (
                    "gesture intent publishes actions · signals own visible state",
                    font_size: 13,
                    text_color: MUTED
                )
            }
            Text (
                "LIVE TIMELINE",
                width: 144,
                height: 30,
                bg_color: PANEL_ALT,
                border_color: BORDER,
                border_width: 1,
                border_radius: 15,
                font_size: 11,
                text_color: CYAN,
                paragraph: centered_label()
            )
        }
    }
}

#[compose]
fn compose_gesture_card() -> Entity {
    let state = model_signal(cx);
    let counter_state = state.clone();
    let single_action = state.clone();
    let double_action = state.clone();
    let triple_action = state.clone();
    let long_action = state;
    let counter_text = Computed::new(move || {
        let value = counter_state.get();
        format!(
            "single {}  ·  double {}  ·  triple {}  ·  long {}",
            value.single, value.double, value.triple, value.long
        )
    });

    ui! {
        Column (
            id: "interaction_gestures",
            grow: 1.0,
            min_width: 430,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 11,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text ("GESTURES · TAP COUNTS / LONG PRESS", font_size: 12, text_color: BLUE)
            Text (
                text: $counter_text,
                id: "interaction_gesture_status",
                height: 28,
                font_size: 11,
                text_color: TEXT
            )
            Row (grow: 1.0, min_height: 86, align: AlignItems::Center, column_gap: 8) {
                Text (
                    id: "interaction_single",
                    "1× TAP",
                    grow: 1.0,
                    min_width: 70,
                    height: 68,
                    bg_color: Color::rgb(31, 70, 91),
                    border_color: CYAN,
                    border_width: 1,
                    border_radius: 12,
                    font_size: 10,
                    text_color: CYAN,
                    paragraph: centered_label()
                ) on Tap { InteractionAction::Single.publish(&single_action); }
                Text (
                    id: "interaction_double",
                    "2× TAP",
                    grow: 1.0,
                    min_width: 70,
                    height: 68,
                    bg_color: Color::rgb(31, 55, 91),
                    border_color: BLUE,
                    border_width: 1,
                    border_radius: 12,
                    font_size: 10,
                    text_color: BLUE,
                    paragraph: centered_label()
                ) on Tap(2) { InteractionAction::Double.publish(&double_action); }
                Text (
                    id: "interaction_triple",
                    "3× TAP",
                    grow: 1.0,
                    min_width: 70,
                    height: 68,
                    bg_color: Color::rgb(49, 41, 83),
                    border_color: VIOLET,
                    border_width: 1,
                    border_radius: 12,
                    font_size: 10,
                    text_color: VIOLET,
                    paragraph: centered_label()
                ) on Tap(3) { InteractionAction::Triple.publish(&triple_action); }
                Text (
                    id: "interaction_long",
                    "HOLD",
                    grow: 1.0,
                    min_width: 70,
                    height: 68,
                    bg_color: Color::rgb(73, 57, 31),
                    border_color: GOLD,
                    border_width: 1,
                    border_radius: 12,
                    font_size: 10,
                    text_color: GOLD,
                    paragraph: centered_label()
                ) on LongPress { InteractionAction::Long.publish(&long_action); }
            }
            Text (
                "Each callback emits one domain action; counters render from a Computed value.",
                width: Dimension::percent(100),
                height: 30,
                font_size: 10,
                text_color: MUTED
            )
        }
    }
}

#[compose]
fn compose_state_card() -> Entity {
    let state = model_signal(cx);
    let error_bg = state.clone();
    let error_action = state.clone();
    let disabled_bg = state.clone();
    let disabled_text = state.clone();
    let disabled_action = state;

    ui! {
        Column (
            id: "interaction_states",
            grow: 1.0,
            min_width: 300,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 10,
            bg_color: PANEL_ALT,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text ("STATE · HOVER / PRESS / ERROR / DISABLED", font_size: 11, text_color: GOLD)
            Row (height: 66, column_gap: 7) {
                Text (
                    id: "interaction_hover_target",
                    "HOVER",
                    grow: 1.0,
                    bg_color: CYAN,
                    border_radius: 10,
                    font_size: 8,
                    text_color: BACKGROUND,
                    paragraph: centered_label()
                )
                Text (
                    id: "interaction_press_target",
                    "PRESS",
                    grow: 1.0,
                    bg_color: BLUE,
                    border_radius: 10,
                    font_size: 8,
                    text_color: BACKGROUND,
                    paragraph: centered_label()
                )
                Text (
                    id: "interaction_error_target",
                    "ERROR",
                    grow: 1.0,
                    bg_color: ${ if error_bg.get().errored { ERROR } else { PANEL } },
                    border_color: ERROR,
                    border_width: 1,
                    border_radius: 10,
                    font_size: 8,
                    text_color: TEXT,
                    paragraph: centered_label()
                ) on Tap { InteractionAction::ToggleError.publish(&error_action); }
                Text (
                    id: "interaction_disabled_target",
                    "DISABLED",
                    grow: 1.0,
                    bg_color: ${ if disabled_bg.get().disabled { MUTED } else { PANEL } },
                    border_color: MUTED,
                    border_width: 1,
                    border_radius: 10,
                    font_size: 8,
                    text_color: TEXT,
                    paragraph: centered_label()
                )
            }
            TextInput (
                id: "interaction_focus_target",
                height: 34,
                bg_color: BACKGROUND,
                border_color: BLUE,
                border_width: 1,
                border_radius: 9
            ) [
                Placeholder("tap to focus, then type"),
            ]
            Text (
                id: "interaction_toggle_disabled",
                text: ${ if disabled_text.get().disabled { "ENABLE TARGET" } else { "DISABLE TARGET" } },
                height: 34,
                bg_color: Color::rgb(34, 53, 73),
                border_color: BORDER,
                border_width: 1,
                border_radius: 9,
                font_size: 10,
                text_color: TEXT,
                paragraph: centered_label()
            ) on Tap { InteractionAction::ToggleDisabled.publish(&disabled_action); }
        }
    }
}

#[compose]
fn compose_motion_card() -> Entity {
    let state = model_signal(cx);
    let drag_action = state.clone();
    let drag_reset = state.clone();
    let parent_action = state.clone();
    let child_action = state.clone();
    let policy_text = state.clone();
    let policy_action = state.clone();
    let bubble_state = state;
    let bubble_text = Computed::new(move || {
        let value = bubble_state.get();
        format!(
            "child {}  ·  parent {}  ·  {}",
            value.child_taps,
            value.parent_taps,
            if value.allow_bubble {
                "allow"
            } else {
                "blocked"
            }
        )
    });

    ui! {
        Column (
            id: "interaction_motion",
            grow: 1.0,
            min_width: 430,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 9,
            bg_color: PANEL_ALT,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text ("MOTION · DRAG / DYNAMIC BUBBLING", font_size: 12, text_color: VIOLET)
            View (
                id: "interaction_drag_stage",
                height: 94,
                bg_color: BACKGROUND,
                border_radius: 10,
                clip_children: true
            ) {
                Text (
                    id: "interaction_drag_target",
                    "DRAG",
                    position: Position::Absolute,
                    left: 24,
                    top: 27,
                    width: 92,
                    height: 40,
                    bg_color: VIOLET,
                    border_radius: 12,
                    font_size: 10,
                    text_color: TEXT,
                    paragraph: centered_label()
                ) on DragMove { InteractionAction::Drag(*dx, *dy).publish(&drag_action); } on DragEnd { InteractionAction::ResetDrag.publish(&drag_reset); }
            }
            Row (height: 58, column_gap: 8) {
                Row (
                    id: "interaction_bubble_parent",
                    grow: 1.0,
                    padding: Padding::all(7),
                    bg_color: Color::rgb(27, 52, 75),
                    border_color: BLUE,
                    border_width: 1,
                    border_radius: 10
                ) on Tap { InteractionAction::ParentTap.publish(&parent_action); }
                {
                    Text (
                        id: "interaction_bubble_child",
                        "CHILD TAP",
                        grow: 1.0,
                        height: 40,
                        bg_color: BLUE,
                        border_radius: 8,
                        font_size: 9,
                        text_color: BACKGROUND,
                        paragraph: centered_label()
                    ) on Tap {
                        let allow = child_action.get_untracked().allow_bubble;
                        InteractionAction::ChildTap.publish(&child_action);
                        if allow { BubbleControl::Allow } else { BubbleControl::Prevent }
                    }
                }
                Text (
                    id: "interaction_bubble_policy",
                    text: ${ if policy_text.get().allow_bubble { "ALLOW" } else { "BLOCK" } },
                    width: 88,
                    height: 58,
                    bg_color: Color::rgb(67, 52, 29),
                    border_color: GOLD,
                    border_width: 1,
                    border_radius: 10,
                    font_size: 9,
                    text_color: GOLD,
                    paragraph: centered_label()
                ) on Tap { InteractionAction::ToggleBubble.publish(&policy_action); }
            }
            Text (
                text: $bubble_text,
                id: "interaction_bubble_status",
                height: 24,
                font_size: 10,
                text_color: MUTED
            )
        }
    }
}

#[compose]
fn compose_controls_card() -> Entity {
    let state = model_signal(cx);
    let switch_action = state.clone();
    let checkbox_action = state.clone();
    let status_state = state;
    let status_text = Computed::new(move || {
        let value = status_state.get();
        format!(
            "switch {} ({} changes)  ·  check {} ({} changes)",
            if value.switch_on { "ON" } else { "OFF" },
            value.switch_changes,
            if value.checkbox_on { "ON" } else { "OFF" },
            value.checkbox_changes
        )
    });

    ui! {
        Column (
            id: "interaction_controls",
            grow: 1.0,
            min_width: 300,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 12,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text ("CONTROLS · BUSINESS SIGNALS", font_size: 12, text_color: CYAN)
            Row (height: 54, align: AlignItems::Center, column_gap: 14) {
                Switch (
                    id: "interaction_switch",
                    width: 64,
                    height: 34
                ) on Toggled { InteractionAction::Switch(*now).publish(&switch_action); }
                Text (
                    "Switch publishes its new value",
                    grow: 1.0,
                    font_size: 11,
                    text_color: TEXT
                )
            }
            Row (height: 54, align: AlignItems::Center, column_gap: 14) {
                Checkbox (
                    id: "interaction_checkbox",
                    width: 34,
                    height: 34
                ) on Toggled { InteractionAction::Checkbox(*now).publish(&checkbox_action); }
                Text (
                    "Checkbox shares the same state",
                    grow: 1.0,
                    font_size: 11,
                    text_color: TEXT
                )
            }
            Text (
                text: $status_text,
                id: "interaction_control_status",
                height: 28,
                font_size: 10,
                text_color: MUTED
            )
            Text (
                id: "interaction_feedback_status",
                "CURSOR + ROTARY FEEDBACK · LIVE INPUT",
                height: 32,
                bg_color: Color::rgb(21, 67, 68),
                border_radius: 8,
                font_size: 9,
                text_color: CYAN,
                paragraph: centered_label()
            )
        }
    }
}

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            id: "interaction_lab_shell",
            grow: 1.0,
            padding: Padding::all(18),
            row_gap: 12,
            bg_color: BACKGROUND
        ) {
            compose_header ()
            Row (
                id: "interaction_lab_grid",
                grow: 1.0,
                wrap: FlexWrap::Wrap,
                align: AlignItems::FlexStart,
                row_gap: 12,
                column_gap: 12
            ) {
                compose_gesture_card ()
                compose_state_card ()
                compose_motion_card ()
                compose_controls_card ()
            }
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    if app.world.resource::<InteractionModel>().is_none() {
        app.world.insert_resource(InteractionModel::default());
    }
    app.add_plugin(InputFeedbackPlugin::new())
        .add_system(sync_interaction_user_states::system());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::focus::{FocusState, Focusable, focus_on_tap};
    use crate::input::event::gesture::GestureEvent;
    use crate::input::event::{bubble_dispatch_at, entity_or_ancestor_disabled};
    use crate::input::feedback::{InputFeedback, InputFeedbackInput};
    use crate::ui::view::ViewRegistry;
    use crate::ui::{IdMap, UiScope};

    fn fixture_tree() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(ViewRegistry::with_builtins());
        world.insert_resource(InteractionModel::default());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        sync_interaction_user_states(&mut world);
        (world, parent)
    }

    fn fixture() -> World {
        fixture_tree().0
    }

    fn state(world: &World) -> InteractionLabState {
        world
            .resource::<InteractionModel>()
            .unwrap()
            .state
            .get_untracked()
    }

    fn tap(world: &mut World, id: &'static str, now_ms: u32) {
        let target = world.find_by_id(id).expect("interaction id");
        bubble_dispatch_at(
            world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target,
            },
            now_ms,
        );
        flush_signal_dirty(world);
    }

    #[test]
    fn preserves_the_complete_interaction_matrix() {
        let world = fixture();
        for id in [
            "interaction_single",
            "interaction_double",
            "interaction_triple",
            "interaction_long",
            "interaction_drag_target",
            "interaction_hover_target",
            "interaction_press_target",
            "interaction_error_target",
            "interaction_disabled_target",
            "interaction_focus_target",
            "interaction_bubble_child",
            "interaction_bubble_policy",
            "interaction_switch",
            "interaction_checkbox",
            "interaction_feedback_status",
        ] {
            assert!(world.find_by_id(id).is_some(), "missing {id}");
        }
        assert!(world.has::<Switch>(world.find_by_id("interaction_switch").unwrap()));
        assert!(world.has::<Checkbox>(world.find_by_id("interaction_checkbox").unwrap()));
        assert!(world.has::<TextInput>(world.find_by_id("interaction_focus_target").unwrap()));
    }

    #[test]
    fn gesture_callbacks_publish_typed_actions() {
        let mut world = fixture();
        tap(&mut world, "interaction_single", 100);
        tap(&mut world, "interaction_double", 1_000);
        tap(&mut world, "interaction_double", 1_100);
        tap(&mut world, "interaction_triple", 2_000);
        tap(&mut world, "interaction_triple", 2_100);
        tap(&mut world, "interaction_triple", 2_200);
        let target = world.find_by_id("interaction_long").unwrap();
        bubble_dispatch_at(
            &mut world,
            &GestureEvent::LongPress {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target,
            },
            3_000,
        );

        let state = state(&world);
        assert_eq!(
            (state.single, state.double, state.triple, state.long),
            (1, 1, 1, 1)
        );
    }

    #[test]
    fn drag_and_bubble_policy_update_only_the_model() {
        let mut world = fixture();
        let drag = world.find_by_id("interaction_drag_target").unwrap();
        bubble_dispatch_at(
            &mut world,
            &GestureEvent::DragMove {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                dx: Fixed::from_int(34),
                dy: Fixed::from_int(18),
                target: drag,
            },
            100,
        );
        assert_eq!(
            (state(&world).drag_x, state(&world).drag_y),
            (Fixed::from_int(34), Fixed::from_int(18))
        );

        tap(&mut world, "interaction_bubble_child", 1_000);
        assert_eq!(
            (state(&world).child_taps, state(&world).parent_taps),
            (1, 0)
        );
        tap(&mut world, "interaction_bubble_policy", 1_500);
        tap(&mut world, "interaction_bubble_child", 2_000);
        assert_eq!(
            (state(&world).child_taps, state(&world).parent_taps),
            (2, 1)
        );
    }

    #[test]
    fn signal_state_projects_to_user_state_and_focus() {
        let mut world = fixture();
        let error = world.find_by_id("interaction_error_target").unwrap();
        let disabled = world.find_by_id("interaction_disabled_target").unwrap();
        assert!(matches!(
            world.get::<UserState>(error),
            Some(UserState::Errored)
        ));
        assert!(entity_or_ancestor_disabled(&world, disabled));

        tap(&mut world, "interaction_error_target", 100);
        tap(&mut world, "interaction_toggle_disabled", 500);
        sync_interaction_user_states(&mut world);
        assert!(world.get::<UserState>(error).is_none());
        assert!(!entity_or_ancestor_disabled(&world, disabled));

        let focus = world.find_by_id("interaction_focus_target").unwrap();
        assert!(world.has::<Focusable>(focus));
        world.insert_resource(FocusState::default());
        focus_on_tap(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: focus,
            },
        );
        assert_eq!(world.resource::<FocusState>().unwrap().focused, Some(focus));
    }

    #[test]
    fn switch_and_checkbox_publish_business_values() {
        let mut world = fixture();
        tap(&mut world, "interaction_switch", 100);
        tap(&mut world, "interaction_checkbox", 500);
        let state = state(&world);
        assert!(state.switch_on);
        assert!(state.checkbox_on);
        assert_eq!((state.switch_changes, state.checkbox_changes), (1, 1));
    }

    #[test]
    fn setup_installs_input_feedback() {
        use crate::input::event::sim::SimTimeline;

        let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);

        assert!(app.world.resource::<InputFeedback>().is_some());
        assert!(app.world.resource::<InputFeedbackInput>().is_some());
        assert!(app.world.resource::<SimTimeline>().is_none());
    }

    #[test]
    fn default_viewport_keeps_the_two_by_two_grid_inside_the_shell() {
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        let (mut world, parent) = fixture_tree();
        update_layout(
            &mut world,
            parent,
            &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
        );
        let rect = |world: &World, id| {
            world
                .get::<ComputedRect>(world.find_by_id(id).unwrap())
                .unwrap()
                .0
        };
        let shell = rect(&world, "interaction_lab_shell");
        let gestures = rect(&world, "interaction_gestures");
        let states = rect(&world, "interaction_states");
        let motion = rect(&world, "interaction_motion");
        let controls = rect(&world, "interaction_controls");

        assert_eq!(gestures.y, states.y);
        assert_eq!(motion.y, controls.y);
        assert!(motion.y > gestures.y);
        for card in [gestures, states, motion, controls] {
            assert!(card.x >= shell.x && card.x + card.w <= shell.x + shell.w);
            assert!(card.y >= shell.y && card.y + card.h <= shell.y + shell.h);
        }
    }
}
