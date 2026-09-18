extern crate alloc;

use alloc::format;

use crate::input::event::BubbleControl;
use crate::input::event::scroll::TouchAction;
#[cfg(feature = "std")]
use crate::prelude::plugin::InputFeedbackPlugin;
use crate::prelude::*;
#[cfg(any(feature = "std", test))]
use crate::ui::UserState;
use crate::ui::widgets::{
    Button, Checkbox, ParagraphStyle, Placeholder, Switch, Text, TextInput, TextOverflow,
    TextVerticalAlign, TextWrap,
};
pub const VIEWPORT: (u16, u16) = (1024, 720);

const BACKGROUND: ColorToken = ColorToken::Surface;
const PANEL: ColorToken = ColorToken::SurfaceVariant;
const PANEL_ALT: ColorToken = ColorToken::Surface;
const BORDER: ColorToken = ColorToken::Outline;
const TEXT: ColorToken = ColorToken::OnSurface;
const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
const CYAN: ColorToken = ColorToken::Primary;
const BLUE: ColorToken = ColorToken::Secondary;
const VIOLET: ColorToken = ColorToken::Tertiary;
const GOLD: ColorToken = ColorToken::Success;
const ERROR: ColorToken = ColorToken::Error;

fn bounded_text(lines: u16) -> ParagraphStyle {
    ParagraphStyle {
        wrap: if lines == 1 {
            TextWrap::NoWrap
        } else {
            TextWrap::Word
        },
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(lines),
        ..ParagraphStyle::default()
    }
}

fn ellipsis_label() -> ParagraphStyle {
    ParagraphStyle {
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(1),
        ..ParagraphStyle::label()
    }
}

fn status_text() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        vertical_align: TextVerticalAlign::Center,
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

fn card_width(width: Fixed) -> Dimension {
    if width < Fixed::from_int(800) {
        Dimension::percent(100)
    } else {
        Dimension::percent(48)
    }
}

fn gesture_cell_width(width: Fixed) -> Dimension {
    if width < Fixed::from_int(400) {
        Dimension::percent(48)
    } else {
        Dimension::percent(23)
    }
}

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

#[cfg(any(feature = "std", test))]
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
    world.invalidate(entity);
}

#[cfg(any(feature = "std", test))]
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
        Column (
            id: "interaction_lab_header",
            min_height: @id(interaction_lab_document).width {
                if interaction_lab_document.width < Fixed::from_int(400) { 116 } else { 96 }
            },
            row_gap: 8
        ) {
            Row (
                height: @id(interaction_lab_document).width {
                    if interaction_lab_document.width < Fixed::from_int(400) { 74 } else { 54 }
                },
                align: AlignItems::Center,
                column_gap: 14
            ) {
                View (width: 8, height: 42, bg_color: CYAN, border_radius: 4)
                Column (grow: 1.0, min_width: 0, row_gap: 3) {
                    Text (
                        "INTERACTION LAB",
                        width: Dimension::percent(100),
                        font_size: 24,
                        text_color: TEXT,
                        paragraph: bounded_text(1)
                    )
                    Text (
                        "gesture actions / signal-owned state",
                        width: Dimension::percent(100),
                        min_height: 18,
                        font_size: 13,
                        text_color: MUTED,
                        paragraph: bounded_text(2)
                    )
                }
            }
            Row (height: 30, justify: JustifyContent::FlexEnd) {
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
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    }
}

#[compose]
fn compose_gesture_card() -> Entity {
    let state = model_signal(cx);
    let single_text = state.clone();
    let double_text = state.clone();
    let triple_text = state.clone();
    let long_text = state.clone();
    let single_action = state.clone();
    let double_action = state.clone();
    let triple_action = state.clone();
    let long_action = state;

    ui! {
        Column (
            id: "interaction_gestures",
            grow: 1.0,
            width: @id(interaction_lab_grid).width {
                card_width(interaction_lab_grid.width)
            },
            min_width: 250,
            min_height: @id(interaction_lab_grid).width {
                if interaction_lab_grid.width < Fixed::from_int(400) { 340 } else { 304 }
            },
            padding: Padding::all(14),
            row_gap: 11,
            clip_children: true,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "GESTURES / TAP COUNTS / LONG PRESS",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: BLUE,
                paragraph: bounded_text(2)
            )
            Row (
                id: "interaction_gesture_status",
                width: Dimension::percent(100),
                height: @id(interaction_gestures).width {
                    if interaction_gestures.width < Fixed::from_int(400) { 52 } else { 28 }
                },
                wrap: FlexWrap::Wrap,
                row_gap: 4,
                column_gap: 6
            ) {
                Text (
                    id: "interaction_single_status",
                    text: ${ format!("single {}", single_text.get().single) },
                    width: @id(interaction_gestures).width {
                        gesture_cell_width(interaction_gestures.width)
                    },
                    min_width: 86,
                    height: 24,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: status_text()
                )
                Text (
                    id: "interaction_double_status",
                    text: ${ format!("double {}", double_text.get().double) },
                    width: @id(interaction_gestures).width {
                        gesture_cell_width(interaction_gestures.width)
                    },
                    min_width: 86,
                    height: 24,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: status_text()
                )
                Text (
                    id: "interaction_triple_status",
                    text: ${ format!("triple {}", triple_text.get().triple) },
                    width: @id(interaction_gestures).width {
                        gesture_cell_width(interaction_gestures.width)
                    },
                    min_width: 86,
                    height: 24,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: status_text()
                )
                Text (
                    id: "interaction_long_status",
                    text: ${ format!("long {}", long_text.get().long) },
                    width: @id(interaction_gestures).width {
                        gesture_cell_width(interaction_gestures.width)
                    },
                    min_width: 76,
                    height: 24,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: status_text()
                )
            }
            Row (
                id: "interaction_gesture_targets",
                height: @id(interaction_gestures).width {
                    if interaction_gestures.width < Fixed::from_int(400) { 144 } else { 68 }
                },
                wrap: FlexWrap::Wrap,
                align: AlignItems::Center,
                row_gap: 8,
                column_gap: 8
            ) {
                Button (
                    "1 TAP",
                    id: "interaction_single",
                    width: @id(interaction_gestures).width {
                        gesture_cell_width(interaction_gestures.width)
                    },
                    min_width: 70,
                    height: 68,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: CYAN,
                    border_color: CYAN,
                    border_width: 1,
                    border_radius: 12,
                    font_size: 10,
                    text_color: CYAN
                ) on Tap { InteractionAction::Single.publish(&single_action); }
                Button (
                    "2 TAP",
                    id: "interaction_double",
                    width: @id(interaction_gestures).width {
                        gesture_cell_width(interaction_gestures.width)
                    },
                    min_width: 70,
                    height: 68,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: BLUE,
                    border_color: BLUE,
                    border_width: 1,
                    border_radius: 12,
                    font_size: 10,
                    text_color: BLUE
                ) on Tap(2) { InteractionAction::Double.publish(&double_action); }
                Button (
                    "3 TAP",
                    id: "interaction_triple",
                    width: @id(interaction_gestures).width {
                        gesture_cell_width(interaction_gestures.width)
                    },
                    min_width: 70,
                    height: 68,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: VIOLET,
                    border_color: VIOLET,
                    border_width: 1,
                    border_radius: 12,
                    font_size: 10,
                    text_color: VIOLET
                ) on Tap(3) { InteractionAction::Triple.publish(&triple_action); }
                Button (
                    "HOLD",
                    id: "interaction_long",
                    width: @id(interaction_gestures).width {
                        gesture_cell_width(interaction_gestures.width)
                    },
                    min_width: 70,
                    height: 68,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: GOLD,
                    border_color: GOLD,
                    border_width: 1,
                    border_radius: 12,
                    font_size: 10,
                    text_color: GOLD
                ) on LongPress { InteractionAction::Long.publish(&long_action); }
            }
            Text (
                id: "interaction_gesture_caption",
                "Each callback emits one domain action; counters render from a Computed value.",
                width: Dimension::percent(100),
                min_height: 48,
                font_size: 10,
                text_color: MUTED,
                paragraph: bounded_text(3)
            )
        }
    }
}

#[compose]
fn compose_state_card() -> Entity {
    let state = model_signal(cx);
    let error_action = state.clone();
    let disabled_text = state.clone();
    let disabled_action = state;

    ui! {
        Column (
            id: "interaction_states",
            grow: 1.0,
            width: @id(interaction_lab_grid).width {
                card_width(interaction_lab_grid.width)
            },
            min_width: 250,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 10,
            clip_children: true,
            bg_color: PANEL_ALT,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "STATE / HOVER / PRESS / ERROR / DISABLED",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 11,
                text_color: GOLD,
                paragraph: bounded_text(2)
            )
            Row (height: 80, wrap: FlexWrap::Wrap, row_gap: 7, column_gap: 7) {
                Button (
                    "HOVER",
                    id: "interaction_hover_target",
                    grow: 1.0,
                    min_width: 70,
                    height: 36,
                    normal_color: CYAN,
                    pressed_color: CYAN,
                    border_radius: 10,
                    font_size: 8,
                    text_color: ColorToken::OnPrimary
                )
                Button (
                    "PRESS",
                    id: "interaction_press_target",
                    grow: 1.0,
                    min_width: 70,
                    height: 36,
                    normal_color: BLUE,
                    pressed_color: BLUE,
                    border_radius: 10,
                    font_size: 8,
                    text_color: ColorToken::OnSecondary
                )
                Button (
                    "ERROR",
                    id: "interaction_error_target",
                    grow: 1.0,
                    min_width: 70,
                    height: 36,
                    normal_color: PANEL,
                    pressed_color: ERROR,
                    border_color: ERROR,
                    border_width: 1,
                    border_radius: 10,
                    font_size: 8,
                    text_color: TEXT
                ) on Tap { InteractionAction::ToggleError.publish(&error_action); }
                Button (
                    "DISABLED",
                    id: "interaction_disabled_target",
                    grow: 1.0,
                    min_width: 70,
                    height: 36,
                    normal_color: PANEL,
                    pressed_color: PANEL,
                    border_color: MUTED,
                    border_width: 1,
                    border_radius: 10,
                    font_size: 8,
                    text_color: TEXT
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
            Button (
                id: "interaction_toggle_disabled",
                text: ${ if disabled_text.get().disabled { "ENABLE TARGET" } else { "DISABLE TARGET" } },
                height: 34,
                normal_color: ColorToken::SurfaceVariant,
                pressed_color: BLUE,
                border_color: BORDER,
                border_width: 1,
                border_radius: 9,
                font_size: 10,
                text_color: TEXT
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
            "child {} / parent {} / {}",
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
            width: @id(interaction_lab_grid).width {
                card_width(interaction_lab_grid.width)
            },
            min_width: 250,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 9,
            clip_children: true,
            bg_color: PANEL_ALT,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "MOTION / DRAG / DYNAMIC BUBBLING",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: VIOLET,
                paragraph: bounded_text(2)
            )
            View (
                id: "interaction_drag_stage",
                height: 94,
                bg_color: BACKGROUND,
                border_radius: 10,
                clip_children: true
            ) {
                Button (
                    "DRAG",
                    id: "interaction_drag_target",
                    position: Position::Absolute,
                    left: 24,
                    top: 27,
                    width: 92,
                    height: 40,
                    normal_color: VIOLET,
                    pressed_color: VIOLET,
                    border_radius: 12,
                    font_size: 10,
                    text_color: ColorToken::OnTertiary
                ) [TouchAction::None] on DragMove { InteractionAction::Drag(*dx, *dy).publish(&drag_action); } on DragEnd { InteractionAction::ResetDrag.publish(&drag_reset); }
            }
            Row (height: 58, column_gap: 8) {
                Row (
                    id: "interaction_bubble_parent",
                    grow: 1.0,
                    padding: Padding::all(7),
                    bg_color: ColorToken::SurfaceVariant,
                    border_color: BLUE,
                    border_width: 1,
                    border_radius: 10
                ) on Tap { InteractionAction::ParentTap.publish(&parent_action); }
                {
                    Button (
                        "CHILD TAP",
                        id: "interaction_bubble_child",
                        grow: 1.0,
                        height: 40,
                        normal_color: BLUE,
                        pressed_color: BLUE,
                        border_radius: 8,
                        font_size: 9,
                        text_color: ColorToken::OnSecondary
                    ) on Tap {
                        let allow = child_action.get_untracked().allow_bubble;
                        InteractionAction::ChildTap.publish(&child_action);
                        if allow { BubbleControl::Allow } else { BubbleControl::Prevent }
                    }
                }
                Button (
                    id: "interaction_bubble_policy",
                    text: ${ if policy_text.get().allow_bubble { "ALLOW" } else { "BLOCK" } },
                    width: 88,
                    height: 58,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: GOLD,
                    border_color: GOLD,
                    border_width: 1,
                    border_radius: 10,
                    font_size: 9,
                    text_color: GOLD
                ) on Tap { InteractionAction::ToggleBubble.publish(&policy_action); }
            }
            Text (
                text: $bubble_text,
                id: "interaction_bubble_status",
                width: Dimension::percent(100),
                height: 24,
                font_size: 10,
                text_color: MUTED,
                paragraph: bounded_text(1)
            )
        }
    }
}

#[compose]
fn compose_controls_card() -> Entity {
    let state = model_signal(cx);
    let switch_action = state.clone();
    let checkbox_action = state.clone();
    let switch_text = state.clone();
    let checkbox_text = state;

    ui! {
        Column (
            id: "interaction_controls",
            grow: 1.0,
            width: @id(interaction_lab_grid).width {
                card_width(interaction_lab_grid.width)
            },
            min_width: 250,
            min_height: 278,
            padding: Padding::all(14),
            row_gap: 12,
            clip_children: true,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text (
                "CONTROLS / BUSINESS SIGNALS",
                width: Dimension::percent(100),
                min_height: 28,
                font_size: 12,
                text_color: CYAN,
                paragraph: bounded_text(2)
            )
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
                    text_color: TEXT,
                    paragraph: bounded_text(2)
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
                    text_color: TEXT,
                    paragraph: bounded_text(2)
                )
            }
            Row (
                id: "interaction_control_status",
                width: Dimension::percent(100),
                height: @id(interaction_lab_grid).width {
                    if interaction_lab_grid.width < Fixed::from_int(400) { 52 } else { 28 }
                },
                wrap: FlexWrap::Wrap,
                row_gap: 4,
                column_gap: 8
            ) {
                Text (
                    id: "interaction_switch_status",
                    text: ${
                        let value = switch_text.get();
                        format!(
                            "switch {} / {} changes",
                            if value.switch_on { "ON" } else { "OFF" },
                            value.switch_changes
                        )
                    },
                    grow: 1.0,
                    min_width: 150,
                    height: 24,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: status_text()
                )
                Text (
                    id: "interaction_checkbox_status",
                    text: ${
                        let value = checkbox_text.get();
                        format!(
                            "check {} / {} changes",
                            if value.checkbox_on { "ON" } else { "OFF" },
                            value.checkbox_changes
                        )
                    },
                    grow: 1.0,
                    min_width: 150,
                    height: 24,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: status_text()
                )
            }
            Text (
                id: "interaction_feedback_status",
                "CURSOR + ROTARY FEEDBACK / LIVE INPUT",
                height: 32,
                bg_color: ColorToken::Primary,
                border_radius: 8,
                font_size: 9,
                text_color: ColorToken::OnPrimary,
                paragraph: ellipsis_label()
            )
        }
    }
}

#[compose]
pub fn build_widgets() {
    //~focus-start
    let viewport = ui! {
        View (
            id: "interaction_lab_shell",
            grow: 1.0,
            clip_children: true,
            bg_color: BACKGROUND
        ) {
            Column (
                id: "interaction_lab_document",
                width: Dimension::percent(100),
                height: Dimension::Content,
                min_height: Dimension::percent(100),
                padding: Padding::all(18),
                row_gap: 12
            ) {
                compose_header ()
                Row (
                    id: "interaction_lab_grid",
                    width: Dimension::percent(100),
                    height: Dimension::Content,
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
        }
    };
    let content = cx
        .world_mut()
        .find_by_id("interaction_lab_document")
        .expect("Interaction Lab document");
    super::lab_scroll::LabScroll::attach(
        cx.world_mut(),
        viewport,
        content,
        Fixed::from_int(1280),
        Fixed::from_int(18),
    );
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    if app.world.resource::<InteractionModel>().is_none() {
        app.world.insert_resource(InteractionModel::default());
    }
    app.add_plugin(InputFeedbackPlugin::new())
        .add_system(sync_interaction_user_states::system())
        .add_system(super::lab_scroll::sync_lab_scroll_extents::system());
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
    use crate::ui::{Children, IdMap, UiScope};

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
            let entity = world
                .find_by_id(id)
                .unwrap_or_else(|| panic!("missing {id}"));
            if id != "interaction_focus_target"
                && id != "interaction_switch"
                && id != "interaction_checkbox"
                && id != "interaction_feedback_status"
            {
                assert!(world.has::<Button>(entity), "{id} is not a Button");
            }
        }
        assert!(world.has::<Switch>(world.find_by_id("interaction_switch").unwrap()));
        assert!(world.has::<Checkbox>(world.find_by_id("interaction_checkbox").unwrap()));
        assert!(world.has::<TextInput>(world.find_by_id("interaction_focus_target").unwrap()));
        assert_eq!(
            world
                .get::<TouchAction>(world.find_by_id("interaction_drag_target").unwrap())
                .copied(),
            Some(TouchAction::None)
        );
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
        flush_signal_dirty(&mut world);

        let state = state(&world);
        assert_eq!(
            (state.single, state.double, state.triple, state.long),
            (1, 1, 1, 1)
        );
        for (id, expected) in [
            ("interaction_single_status", "single 1"),
            ("interaction_double_status", "double 1"),
            ("interaction_triple_status", "triple 1"),
            ("interaction_long_status", "long 1"),
        ] {
            let entity = world.find_by_id(id).unwrap();
            assert_eq!(world.get::<Text>(entity).unwrap().resolve(&world), expected);
        }
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
        let policy = world.find_by_id("interaction_bubble_policy").unwrap();
        assert_eq!(world.get::<Text>(policy).unwrap().resolve(&world), "ALLOW");
        assert!(
            world
                .get::<Children>(policy)
                .is_none_or(|children| children.0.iter().all(|child| !world.has::<Text>(*child)))
        );
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
        for (id, expected) in [
            ("interaction_switch_status", "switch ON / 1 changes"),
            ("interaction_checkbox_status", "check ON / 1 changes"),
        ] {
            let entity = world.find_by_id(id).unwrap();
            assert_eq!(world.get::<Text>(entity).unwrap().resolve(&world), expected);
        }
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

    #[test]
    fn dirty_first_frame_reconciles_the_responsive_tree() {
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::dirty::DirtyRegions;
        use crate::ui::render_system::collect_dirty_regions_into;

        let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
        app.with_default_widgets().with_default_systems();
        let parent = app.spawn_root().id();
        setup_app(&mut app, parent);
        app.set_root(parent);
        let viewport = Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE);
        let mut plan = DirtyRegions::default();
        collect_dirty_regions_into(&mut app.world, parent, &viewport, &mut plan);

        let rect = |world: &World, id| {
            world
                .get::<ComputedRect>(world.find_by_id(id).unwrap())
                .unwrap()
                .0
        };
        let gestures = rect(&app.world, "interaction_gestures");
        let states = rect(&app.world, "interaction_states");
        let motion = rect(&app.world, "interaction_motion");
        let controls = rect(&app.world, "interaction_controls");
        assert_eq!(gestures.y, states.y);
        assert_eq!(motion.y, controls.y);
        assert!(motion.y > gestures.y);
        super::super::assert_text_layouts_fit(&app.world);
        assert!(!plan.is_empty());
    }

    #[test]
    fn phone_viewport_stacks_cards_and_wraps_gesture_targets() {
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        let (mut world, parent) = fixture_tree();
        update_layout(&mut world, parent, &Viewport::new(320, 568, Fixed::ONE));
        let rect = |world: &World, id| {
            world
                .get::<ComputedRect>(world.find_by_id(id).unwrap())
                .unwrap()
                .0
        };
        let shell = rect(&world, "interaction_lab_shell");
        let gestures = rect(&world, "interaction_gestures");
        for id in [
            "interaction_gestures",
            "interaction_states",
            "interaction_motion",
            "interaction_controls",
        ] {
            let card = rect(&world, id);
            assert!(card.x >= shell.x);
            assert!(card.x + card.w <= shell.x + shell.w);
        }
        for id in [
            "interaction_single",
            "interaction_double",
            "interaction_triple",
            "interaction_long",
        ] {
            let target = rect(&world, id);
            assert!(target.x >= gestures.x);
            assert!(target.x + target.w <= gestures.x + gestures.w);
        }
    }

    #[test]
    fn medium_portrait_cards_fill_the_single_column() {
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        for (width, height) in [(672, 666), (666, 674), (502, 900), (320, 568)] {
            let mut app = App::headless(width, height);
            app.with_default_widgets().with_default_systems();
            let parent = app.spawn_root().id();
            setup_app(&mut app, parent);
            app.set_root(parent);
            update_layout(
                &mut app.world,
                parent,
                &Viewport::new(width, height, Fixed::ONE),
            );
            let rect = |world: &World, id| {
                world
                    .get::<ComputedRect>(world.find_by_id(id).unwrap())
                    .unwrap()
                    .0
            };
            let grid = rect(&app.world, "interaction_lab_grid");
            for id in [
                "interaction_gestures",
                "interaction_states",
                "interaction_motion",
                "interaction_controls",
            ] {
                let card = rect(&app.world, id);
                assert_eq!(card.x, grid.x, "{width}x{height}: {id}");
                assert_eq!(card.w, grid.w, "{width}x{height}: {id}");
            }
            let gestures = rect(&app.world, "interaction_gestures");
            let status = rect(&app.world, "interaction_gesture_status");
            let targets = rect(&app.world, "interaction_gesture_targets");
            let caption = rect(&app.world, "interaction_gesture_caption");
            assert!(
                status.y + status.h <= targets.y,
                "{width}x{height}: gesture status overlaps targets"
            );
            assert!(
                targets.y + targets.h <= caption.y,
                "{width}x{height}: gesture targets overlap caption"
            );
            assert!(
                caption.y + caption.h <= gestures.y + gestures.h,
                "{width}x{height}: gesture caption escapes card"
            );
            super::super::assert_text_layouts_fit(&app.world);
        }
    }

    #[test]
    fn phone_scroll_extent_keeps_the_last_card_reachable() {
        use crate::input::event::scroll::ScrollConfig;
        use crate::types::Viewport;
        use crate::ui::ComputedRect;
        use crate::ui::render_system::update_layout;

        for (width, height) in [(320, 568), (422, 600), (480, 320)] {
            let (mut world, parent) = fixture_tree();
            update_layout(
                &mut world,
                parent,
                &Viewport::new(width, height, Fixed::ONE),
            );
            super::super::lab_scroll::LabScroll::sync_all(&mut world);

            let shell = world.find_by_id("interaction_lab_shell").unwrap();
            let controls = world.find_by_id("interaction_controls").unwrap();
            let shell_rect = world.get::<ComputedRect>(shell).unwrap().0;
            let controls_rect = world.get::<ComputedRect>(controls).unwrap().0;
            let extent = world.get::<ScrollConfig>(shell).unwrap().content_height;
            let max_offset = extent - shell_rect.h;

            assert!(max_offset > Fixed::ZERO, "{width}x{height}");
            assert!(
                controls_rect.y + controls_rect.h - max_offset <= shell_rect.y + shell_rect.h,
                "{width}x{height}: {controls_rect:?} {shell_rect:?} {extent:?}",
            );
            assert!(
                controls_rect.y + controls_rect.h - max_offset > shell_rect.y,
                "{width}x{height}: {controls_rect:?} {shell_rect:?} {extent:?}",
            );
        }
    }
}
