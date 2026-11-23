use super::CAMERA_YAW;
use crate::{
    app::{Control, FluxRun, Hud},
    gameplay::{Mode, Session},
};
use alloc::format;
use core::num::NonZeroU16;
use mirui::{
    prelude::*,
    ui::theme::ColorToken::{OnSurface, Surface, SurfaceVariant},
    ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign, TextOverflow},
};
use mirui_sprite_stack::{Camera, Resolution, StackViewport};

#[derive(Clone, Copy)]
enum Action {
    Play,
    Autopilot,
    Pause,
    Resume,
    TogglePilot,
    Retry,
    End,
    Title,
}

impl Action {
    fn label(self, mode: Mode, compact: bool) -> &'static str {
        match self {
            Self::Play => "PLAY",
            Self::Autopilot => "AUTO",
            Self::Pause => "II",
            Self::Resume => "GO",
            Self::TogglePilot if mode == Mode::Manual => "AUTO",
            Self::TogglePilot if compact => "HAND",
            Self::TogglePilot => "MANUAL",
            Self::Retry if compact => "NEW",
            Self::Retry => "RETRY",
            Self::End => "END",
            Self::Title => "TITLE",
        }
    }
}

#[mirui::compose]
fn action(model: model!(FluxRun), input: Action, height: u16, compact: bool) -> Entity {
    let primary = matches!(input, Action::Play | Action::Resume);
    ui! {
        Button (
            text: ${
                let mode = if matches!(input, Action::TogglePilot) {
                    model.hud().mode
                } else {
                    Mode::Manual
                };
                input.label(mode, compact)
            },
            grow: 1.0,
            min_width: 0,
            height: height,
            size: ButtonSize::Custom,
            padding: Padding::all(if compact { 1 } else { 4 }),
            font_size: if compact { 8 } else { 16 },
            border_radius: if compact { 1 } else { 4 },
            normal_color: if primary { OnSurface } else { SurfaceVariant },
            text_color: if primary { Surface } else { OnSurface },
            pressed_color: OnSurface,
            pressed_text_color: Surface
        ) on Tap {
            match input {
                Action::Play => model.start(Mode::Manual),
                Action::Autopilot => model.start(Mode::Auto),
                Action::Pause | Action::Resume => model.menu(),
                Action::TogglePilot => model.set_autoplay(model.hud().mode != Mode::Auto),
                Action::Retry => model.retry(),
                Action::End => model.finish(),
                Action::Title => model.title(),
            }
        }
    }
}

#[mirui::compose]
fn control(model: model!(FluxRun), input: Control, label: &'static str, height: u16) -> Entity {
    let compact = height <= 20;
    let magnet = matches!(input, Control::Magnet);
    ui! {
        Button (
            text: label,
            grow: 1.0,
            height: height,
            min_width: if compact && magnet { 30 } else { 0 },
            size: ButtonSize::Custom,
            padding: Padding::all(2),
            font_size: if compact { 8 } else { 16 },
            border_radius: if compact { 1 } else { 4 },
            normal_color: if magnet { OnSurface } else { SurfaceVariant },
            text_color: if magnet { Surface } else { OnSurface },
            pressed_color: OnSurface,
            pressed_text_color: Surface
        ) on Pressed { model.button(input, true); } on Released { model.button(input, false); } on PressCancelled { model.cancel_touch(input); }
    }
}

fn status(hud: Hud, compact: bool) -> &'static str {
    match hud.session {
        Session::Title if compact => "SALVAGE",
        Session::Title => "MAGNETIC SALVAGE",
        Session::Paused => "PAUSED",
        Session::Departed if compact => "TRANSIT",
        Session::Departed => "GATE TRANSIT",
        Session::Results if compact => "COMPLETE",
        Session::Results => "RUN COMPLETE",
        Session::Running if hud.life == 0 => "RESPAWN",
        Session::Running if hud.mode == Mode::Auto => "AUTO PILOT",
        Session::Running if hud.cores >= hud.quota => "EXIT OPEN",
        Session::Running if hud.tether && compact => "THROW",
        Session::Running if hud.tether => "RELEASE TO THROW",
        Session::Running if hud.target && compact => "IN REACH",
        Session::Running if hud.target => "CORE IN REACH",
        Session::Running if compact => hud
            .name
            .split_once(' ')
            .map_or(hud.name, |(first, _)| first),
        Session::Running => hud.name,
    }
}

#[mirui::compose]
pub(crate) fn build(model: model!(FluxRun), compact: bool) -> Entity {
    use Action::{Autopilot, End, Pause, Play, Resume, Retry, Title, TogglePilot};
    use Control::{Down, Left, Magnet, Right, Up};
    if !res!(optional MonoClock).with(|clock| clock.is_some()) {
        res!().insert(MonoClock::from_time_source());
    }
    let camera = Camera {
        center: [
            Fixed::from_int(36),
            Fixed::from_int(if compact { 3 } else { 1 }),
            Fixed::from_int(36),
        ],
        yaw: CAMERA_YAW,
        pitch: Fixed::from_int(if compact { 48 } else { 41 }),
        scale: if compact {
            Fixed::from_ratio(3, 4)
        } else {
            Fixed::from_int(4)
        },
        range: Fixed::from_int(80),
        focal_distance: if compact {
            None
        } else {
            Some(Fixed::from_int(230))
        },
    };
    let resolution = if compact {
        Resolution::Native
    } else {
        Resolution::Integer {
            limit: [1120, 540].map(|side| NonZeroU16::new(side).unwrap()),
            minimum: [8, 8],
            scale: NonZeroU16::MIN,
        }
    };
    let font = if compact { 8 } else { 16 };
    let controls = if compact { 20 } else { 38 };
    let header = if compact { 24 } else { 60 };
    let footer = if compact { 40 } else { 116 };
    let status_height = if compact { 12 } else { 20 };
    let pad = if compact { 2 } else { 14 };
    let gap = if compact { 2 } else { 7 };
    let label = ParagraphStyle {
        align: TextAlign::Start,
        overflow: TextOverflow::Ellipsis,
        ..ParagraphStyle::label()
    };
    ui! {
        Column (
            grow: 1.0,
            bg_color: Surface,
            clip_children: true,
            min_height: 0,
            align: AlignItems::Center
        ) {
            Row (
                width: Dimension::percent(100),
                height: header,
                shrink: 0.0,
                padding: Padding::all(pad),
                gap: gap,
                bg_color: SurfaceVariant,
                align: AlignItems::Center
            ) {
                Text (
                    text: if compact { "FLUX" } else { "FLUX RUN" },
                    font_size: font,
                    height: status_height,
                    min_width: 0,
                    grow: 1.0,
                    text_color: OnSurface,
                    paragraph: label.clone()
                )
                Text (
                    text: ${
                        let hud = model.hud();
                        format!("{:02} {}/{}", hud.level, hud.cores, hud.quota)
                    },
                    font_size: font,
                    height: status_height,
                    shrink: 0.0,
                    text_color: OnSurface,
                    paragraph: label.clone()
                )
                View (width: if compact { 26 } else { 48 }, height: controls, shrink: 0.0) {
                    if ${ model.hud().session == Session::Running } {
                        action (model.clone(), Pause, controls, compact)
                    }
                }
            }
            StackViewport (
                stage: model.stage(),
                camera: ${
                    let mut camera = camera;
                    if !compact {
                        camera.scale = model.camera_scale();
                    }
                    if let Some(passage) = model.passage() {
                        let follow = passage.focus * Fixed::from_ratio(3, 4);
                        camera.center[0] += (passage.center[0] - camera.center[0]) * follow;
                        camera.center[2] += (passage.center[1] - camera.center[2]) * follow;
                        camera.scale = camera.scale * (Fixed::ONE + follow);
                    }
                    camera
                },
                resolution: resolution,
                grow: if compact { 0.0 } else { 1.0 },
                width: if compact { Dimension::px(88) } else { Dimension::percent(100) },
                height: if compact { Dimension::px(64) } else { Dimension::Auto },
                min_height: 0,
                bg_color: Surface,
                clip_children: true
            ) {
                if ${ model.passage().is_some() } {
                    View (
                        position: Position::Absolute,
                        left: 0,
                        top: 0,
                        width: Dimension::percent(100),
                        height: Dimension::percent(100),
                        bg_color: ${ model.tone().passage_cover(model.passage().map_or(0, |passage| passage.cover)) }
                    )
                }
            } on SizeChanged {
                if !compact {
                    let pixels = resolution
                        .fit([
                            event.new.width.ceil().to_int().clamp(1, i32::from(u16::MAX)) as u16,
                            event.new.height.ceil().to_int().clamp(1, i32::from(u16::MAX)) as u16,
                        ])
                        .pixels();
                    model.fit_camera(pixels);
                }
            }
            Column (
                width: Dimension::percent(100),
                height: footer,
                shrink: 0.0,
                bg_color: Surface,
                padding: Padding::all(pad),
                gap: gap
            ) {
                Row (height: status_height, gap: gap, align: AlignItems::Center) {
                    Text (
                        text: ${ status(model.hud(), compact) },
                        font_size: font,
                        height: status_height,
                        text_color: OnSurface,
                        min_width: 0,
                        grow: 1.0,
                        paragraph: label.clone()
                    )
                    Text (
                        text: ${
                            let hud = model.hud();
                            match hud.session {
                                Session::Running => format!("{} HP", hud.life),
                                Session::Departed | Session::Results if compact => format!("{} PTS", hud.score),
                                Session::Departed | Session::Results => format!("{} REV", hud.revivals),
                                _ => alloc::string::String::new(),
                            }
                        },
                        font_size: font,
                        height: status_height,
                        text_color: OnSurface,
                        paragraph: label.clone()
                    )
                }
                Row (height: controls, gap: gap) {
                    if ${ model.hud().session == Session::Title } {
                        action (model.clone(), Play, controls, compact)
                        action (model.clone(), Autopilot, controls, compact)
                    }
                    if ${ model.hud().session == Session::Running } {
                        control (model.clone(), Left, "<", controls)
                        control (model.clone(), Up, "^", controls)
                        control (model.clone(), Down, "v", controls)
                        control (model.clone(), Right, ">", controls)
                        control (model.clone(), Magnet, "MAG", controls)
                    }
                    if ${ model.hud().session == Session::Paused } {
                        action (model.clone(), Resume, controls, compact)
                        action (model.clone(), TogglePilot, controls, compact)
                        action (model.clone(), Retry, controls, compact)
                        action (model.clone(), End, controls, compact)
                    }
                    if ${ model.hud().session == Session::Results } {
                        action (model.clone(), Title, controls, compact)
                    }
                }
                if !compact {
                    Text (
                        text: ${
                            let hud = model.hud();
                            if hud.session == Session::Title {
                                alloc::string::String::from("HOLD SPACE TO TOW. RELEASE TO THROW.")
                            } else {
                                format!("{:06} PTS / {} S / BEST {:06}", hud.score, hud.seconds, hud.best)
                            }
                        },
                        width: Dimension::percent(100),
                        height: 16,
                        text_color: OnSurface,
                        font_size: 12,
                        paragraph: label
                    )
                }
            }
        }
    }
}
