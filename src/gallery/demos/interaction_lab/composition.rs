use alloc::format;

#[cfg(any(feature = "std", test))]
use super::runtime::InteractionNodes;
use super::state::{InteractionAction, model_signal};
use super::style::{
    BACKGROUND, BLUE, BORDER, CYAN, ERROR, GOLD, MUTED, PANEL, PANEL_ALT, TEXT, VIOLET,
    bounded_text, card_width, ellipsis_label, gesture_cell_width, status_text,
};
use crate::input::event::BubbleControl;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, Checkbox, ParagraphStyle, Placeholder, Switch, Text, TextInput};

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
    #[cfg(any(feature = "std", test))]
    {
        let nodes = InteractionNodes {
            error: cx
                .world_mut()
                .find_by_id("interaction_error_target")
                .expect("Interaction Lab error target"),
            disabled: cx
                .world_mut()
                .find_by_id("interaction_disabled_target")
                .expect("Interaction Lab disabled target"),
            drag: cx
                .world_mut()
                .find_by_id("interaction_drag_target")
                .expect("Interaction Lab drag target"),
        };
        cx.world_mut().insert_resource(nodes);
    }
    //~focus-end
}
