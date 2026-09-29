use super::state::InteractionModel;
use super::style::{
    BACKGROUND, BLUE, BORDER, CYAN, ERROR, GOLD, MUTED, PANEL, PANEL_ALT, TEXT, VIOLET,
    bounded_text, card_width, ellipsis_label, gesture_cell_width, status_text,
};
use crate::input::event::BubbleControl;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::UserState;
use crate::ui::widgets::{Button, Checkbox, ParagraphStyle, Placeholder, Switch, Text, TextInput};

#[compose(bind(model))]
fn compose_header(model: InteractionModel) -> Entity {
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
                        "gesture actions / observed model state",
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
                    text: ${ if model.disabled() { "TARGET LOCKED" } else { "LIVE TIMELINE" } },
                    text_capacity: 13,
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

#[compose(bind(model))]
fn compose_gesture_card(model: InteractionModel) -> Entity {
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
                    text: ${ format_args!("single {}", model.single()) },
                    text_capacity: 14,
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
                    text: ${ format_args!("double {}", model.double()) },
                    text_capacity: 14,
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
                    text: ${ format_args!("triple {}", model.triple()) },
                    text_capacity: 14,
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
                    text: ${ format_args!("long {}", model.long()) },
                    text_capacity: 12,
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
                ) on Tap { model.record_single(); }
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
                ) on Tap(2) { model.record_double(); }
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
                ) on Tap(3) { model.record_triple(); }
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
                ) on LongPress { model.record_long_press(); }
            }
            Text (
                id: "interaction_gesture_caption",
                "Each callback updates model state; observed counters refresh automatically.",
                width: Dimension::percent(100),
                min_height: 48,
                font_size: 10,
                text_color: MUTED,
                paragraph: bounded_text(3)
            )
        }
    }
}

#[compose(bind(model))]
fn compose_state_card(model: InteractionModel) -> Entity {
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
                if ${ model.errored() } {
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
                    ) [UserState::Errored] on Tap { model.toggle_error(); }
                } else {
                    Button (
                        "ERROR",
                        id: "interaction_error_clear_target",
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
                    ) on Tap { model.toggle_error(); }
                }
                if ${ model.disabled() } {
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
                    ) [UserState::Disabled]
                } else {
                    Button (
                        "DISABLED",
                        id: "interaction_enabled_target",
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
                text: ${ if model.disabled() { "ENABLE TARGET" } else { "DISABLE TARGET" } },
                height: 34,
                normal_color: ColorToken::SurfaceVariant,
                pressed_color: BLUE,
                border_color: BORDER,
                border_width: 1,
                border_radius: 9,
                font_size: 10,
                text_color: TEXT
            ) on Tap { model.toggle_disabled(); }
        }
    }
}

#[compose(bind(model))]
fn compose_motion_card(model: InteractionModel) -> Entity {
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
                    left: ${ Fixed::from_int(24) + model.drag_x() },
                    top: ${ Fixed::from_int(27) + model.drag_y() },
                    width: 92,
                    height: 40,
                    normal_color: VIOLET,
                    pressed_color: VIOLET,
                    border_radius: 12,
                    font_size: 10,
                    text_color: ColorToken::OnTertiary
                ) [TouchAction::None] on DragMove { model.set_drag(*dx, *dy); } on DragEnd { model.reset_drag(); }
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
                ) on Tap { model.record_parent_tap(); }
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
                        let allow = model.allow_bubble();
                        model.record_child_tap();
                        if allow { BubbleControl::Allow } else { BubbleControl::Prevent }
                    }
                }
                Button (
                    id: "interaction_bubble_policy",
                    text: ${ if model.allow_bubble() { "ALLOW" } else { "BLOCK" } },
                    width: 88,
                    height: 58,
                    normal_color: ColorToken::SurfaceVariant,
                    pressed_color: GOLD,
                    border_color: GOLD,
                    border_width: 1,
                    border_radius: 10,
                    font_size: 9,
                    text_color: GOLD
                ) on Tap { model.toggle_bubble(); }
            }
            Text (
                text: ${ format_args!(
                    "child {} / parent {} / {}",
                    model.child_taps(),
                    model.parent_taps(),
                    if model.allow_bubble() { "allow" } else { "blocked" },
                ) },
                text_capacity: 36,
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

#[compose(bind(model))]
fn compose_controls_card(model: InteractionModel) -> Entity {
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
                "CONTROLS / MODEL STATE",
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
                    height: 34,
                    on: ${ model.switch_on() }
                ) on Toggled { model.set_switch(*now); }
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
                    height: 34,
                    checked: ${ model.checkbox_on() }
                ) on Toggled { model.set_checkbox(*now); }
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
                    text: ${ format_args!(
                        "switch {} / {} changes",
                        if model.switch_on() { "ON" } else { "OFF" },
                        model.switch_changes(),
                    ) },
                    text_capacity: 28,
                    grow: 1.0,
                    min_width: 150,
                    height: 24,
                    font_size: 10,
                    text_color: MUTED,
                    paragraph: status_text()
                )
                Text (
                    id: "interaction_checkbox_status",
                    text: ${ format_args!(
                        "check {} / {} changes",
                        if model.checkbox_on() { "ON" } else { "OFF" },
                        model.checkbox_changes(),
                    ) },
                    text_capacity: 27,
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

#[compose(bind(model))]
pub(super) fn build_widgets(model: InteractionModel) {
    //~focus-start
    ui! {
        Scroll (
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
                compose_header (model)
                Row (
                    id: "interaction_lab_grid",
                    width: Dimension::percent(100),
                    height: Dimension::Content,
                    wrap: FlexWrap::Wrap,
                    align: AlignItems::FlexStart,
                    row_gap: 12,
                    column_gap: 12
                ) {
                    compose_gesture_card (model)
                    compose_state_card (model)
                    compose_motion_card (model)
                    compose_controls_card (model)
                }
            }
        }
    };
    //~focus-end
}
