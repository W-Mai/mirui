use super::render::{BG, CHANNEL_A, CHANNEL_B, ICE, ScopeCanvas};
use super::state::{ScopeAction, ScopeModel, ScopeState, TriggerEdge};
use crate::core::reactive::Signal;
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Image, ParagraphStyle, Text};

const CONTROL_TOP: i32 = 382;
const CONTROL_HEIGHT: i32 = 58;

fn model(cx: &mut crate::ui::UiScope<'_>) -> ScopeModel {
    cx.world_mut()
        .resource::<ScopeModel>()
        .cloned()
        .expect("Signal Scope model")
}

fn centered() -> ParagraphStyle {
    let mut paragraph = ParagraphStyle::label();
    paragraph.letter_spacing = Fixed::from_ratio(1, 2);
    paragraph
}

#[derive(Clone, Copy)]
enum ScopeReadout {
    Blank,
    Acquire,
    ChannelA,
    ChannelB,
    Time,
    Trigger,
}

impl ScopeReadout {
    fn text(self, state: ScopeState) -> &'static str {
        match self {
            Self::Blank => "",
            Self::Acquire => {
                if state.running {
                    "RUN"
                } else {
                    "HOLD"
                }
            }
            Self::ChannelA => "500 mV",
            Self::ChannelB => {
                if state.channel_b {
                    "500 mV"
                } else {
                    "OFF"
                }
            }
            Self::Time => match state.time_scale {
                0 => "20 ms/div",
                1 => "10 ms/div",
                2 => "5 ms/div",
                _ => "2 ms/div",
            },
            Self::Trigger => {
                if state.trigger_edge == TriggerEdge::Rising {
                    "RISING"
                } else {
                    "FALLING"
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ButtonControlSpec {
    left_px: i32,
    width_px: i32,
    readout: ScopeReadout,
    accent: Color,
}

#[derive(Clone, Copy)]
struct ChannelControlSpec {
    left_px: i32,
    width_px: i32,
    label_width_px: i32,
    value_width_px: i32,
    label: &'static str,
    readout: ScopeReadout,
    accent: Color,
}

#[derive(Clone, Copy)]
struct StepControlSpec {
    left_px: i32,
    width_px: i32,
    value_width_px: i32,
    text_height: Dimension,
    readout: ScopeReadout,
}

const fn x_dimension(px: i32) -> Dimension {
    Dimension::Percent(Fixed::from_ratio(px * 100, 800))
}

const fn y_dimension(px: i32) -> Dimension {
    Dimension::Percent(Fixed::from_ratio(px * 100, 480))
}

const fn child_percent(px: i32, parent_px: i32) -> Dimension {
    Dimension::Percent(Fixed::from_ratio(px * 100, parent_px))
}

#[compose]
fn compose_button_control(
    spec: ButtonControlSpec,
    value: Signal<ScopeState>,
    action: ScopeAction,
    model: ScopeModel,
) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: x_dimension(spec.left_px), top: y_dimension(CONTROL_TOP),
            width: x_dimension(spec.width_px), height: y_dimension(CONTROL_HEIGHT),
            border_radius: 5
        ) on Tap { action.publish(&model); }
        {
            Text (
                text: ${ spec.readout.text(value.get()) },
                position: Position::Absolute,
                left: 0, top: 0,
                width: Dimension::percent(100), height: Dimension::percent(100),
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(48)).to_int().clamp(9, 16) as u16
                },
                text_color: spec.accent,
                paragraph: centered()
            ) [IgnoreHitTest]
        }
    }
}

#[compose]
fn compose_channel_control(
    spec: ChannelControlSpec,
    value: Signal<ScopeState>,
    action: ScopeAction,
    model: ScopeModel,
) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: x_dimension(spec.left_px), top: y_dimension(CONTROL_TOP),
            width: x_dimension(spec.width_px), height: y_dimension(CONTROL_HEIGHT),
            border_radius: 5
        ) on Tap { action.publish(&model); }
        {
            Text (
                spec.label,
                position: Position::Absolute,
                left: 0, top: 0,
                width: child_percent(spec.label_width_px, spec.width_px), height: Dimension::percent(100),
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(54)).to_int().clamp(9, 14) as u16
                },
                text_color: spec.accent,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                text: ${ spec.readout.text(value.get()) },
                position: Position::Absolute,
                left: child_percent(spec.label_width_px, spec.width_px), top: 0,
                width: child_percent(spec.value_width_px, spec.width_px), height: Dimension::percent(100),
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(50)).to_int().clamp(9, 15) as u16
                },
                text_color: ICE,
                paragraph: centered()
            ) [IgnoreHitTest]
        }
    }
}

#[compose]
fn compose_step_control(
    spec: StepControlSpec,
    value: Signal<ScopeState>,
    action: ScopeAction,
    model: ScopeModel,
) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: x_dimension(spec.left_px), top: y_dimension(CONTROL_TOP),
            width: x_dimension(spec.width_px), height: y_dimension(CONTROL_HEIGHT),
            border_radius: 5
        ) on Tap { action.publish(&model); }
        {
            Text (
                text: ${ spec.readout.text(value.get()) },
                position: Position::Absolute,
                left: 0, top: 0,
                width: child_percent(spec.value_width_px, spec.width_px), height: spec.text_height,
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(50)).to_int().clamp(9, 15) as u16
                },
                text_color: ICE,
                paragraph: centered()
            ) [IgnoreHitTest]
        }
    }
}

#[compose]
pub fn build_widgets() {
    let model = model(cx);
    let canvas_revision = model.state.clone();
    let controls_state = model.state.clone();
    let uart_state = controls_state.clone();
    let run_action = model.clone();
    let wave_action = model.clone();
    let gain_a_action = model.clone();
    let channel_b_action = model.clone();
    let time_action = model.clone();
    let trigger_action = model;
    let toggle_run = ScopeAction::ToggleRun;
    let stop_scope = ScopeAction::Stop;
    let gain_a = ScopeAction::GainA;
    let toggle_channel_b = ScopeAction::ToggleChannelB;
    let time_scale = ScopeAction::TimeScale;
    let toggle_edge = ScopeAction::ToggleEdge;
    let acquire_control = ButtonControlSpec {
        left_px: 20,
        width_px: 78,
        readout: ScopeReadout::Acquire,
        accent: CHANNEL_A,
    };
    let channel_a_control = ChannelControlSpec {
        left_px: 193,
        width_px: 167,
        label_width_px: 56,
        value_width_px: 74,
        label: "CH A",
        readout: ScopeReadout::ChannelA,
        accent: CHANNEL_A,
    };
    let channel_b_control = ChannelControlSpec {
        left_px: 370,
        width_px: 162,
        label_width_px: 55,
        value_width_px: 72,
        label: "CH B",
        readout: ScopeReadout::ChannelB,
        accent: CHANNEL_B,
    };
    let time_readout = ScopeReadout::Time;
    let trigger_readout = ScopeReadout::Trigger;
    let full_control_height = Dimension::percent(100);
    let trigger_text_height = Dimension::percent(56);
    let time_control = StepControlSpec {
        left_px: 552,
        width_px: 118,
        value_width_px: 82,
        text_height: full_control_height,
        readout: time_readout,
    };
    let trigger_control = StepControlSpec {
        left_px: 680,
        width_px: 106,
        value_width_px: 71,
        text_height: trigger_text_height,
        readout: trigger_readout,
    };
    ui! {
        View (
            id: "scope_shell",
            grow: 1.0,
            bg_color: BG,
            clip_children: true
        ) {
            Image (
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                src: "signal_scope_housing"
            ) [IgnoreHitTest]
            ScopeCanvas (
                id: "scope_canvas",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                min_height: 150,
                render_key: ${ u64::from(canvas_revision.get().revision) }
            ) [IgnoreHitTest]
            Text (
                "SIGNAL SCOPE",
                position: Position::Absolute,
                left: Dimension::percent(4), top: Dimension::percent(1),
                width: Dimension::percent(20), height: Dimension::percent(6),
                font: FontToken::Heading,
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(44)).to_int().clamp(11, 19) as u16
                },
                text_color: ICE,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                "CH A",
                position: Position::Absolute,
                left: Dimension::percent(1), top: Dimension::percent(27),
                width: Dimension::percent(6), height: Dimension::percent(6),
                font_size: @id(scope_shell).width { (scope_shell.width / Fixed::from_int(48)).to_int().clamp(10, 17) as u16 },
                text_color: CHANNEL_A,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                "CH B",
                position: Position::Absolute,
                left: Dimension::percent(1), top: Dimension::percent(55),
                width: Dimension::percent(6), height: Dimension::percent(6),
                font_size: @id(scope_shell).width { (scope_shell.width / Fixed::from_int(48)).to_int().clamp(10, 17) as u16 },
                text_color: CHANNEL_B,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                text: ${ uart_state.get().uart_label() },
                position: Position::Absolute,
                left: Dimension::percent(71), top: Dimension::percent(68),
                width: Dimension::percent(24), height: Dimension::percent(4),
                font: FontToken::Mono,
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(80)).to_int().clamp(7, 11) as u16
                },
                text_color: CHANNEL_B,
                paragraph: centered()
            ) [IgnoreHitTest]
            compose_button_control (acquire_control, controls_state.clone(), toggle_run, run_action)
            compose_button_control (ButtonControlSpec { left_px: 108, width_px: 62, readout: ScopeReadout::Blank, accent: ICE }, controls_state.clone(), stop_scope, wave_action)
            compose_channel_control (channel_a_control, controls_state.clone(), gain_a, gain_a_action)
            compose_channel_control (channel_b_control, controls_state.clone(), toggle_channel_b, channel_b_action)
            compose_step_control (time_control, controls_state.clone(), time_scale, time_action)
            compose_step_control (trigger_control, controls_state, toggle_edge, trigger_action)
        }
    };
}
