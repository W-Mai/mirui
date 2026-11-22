use super::assets::{AMBER, BRIGHT, CYAN, INK, PANEL, WATER};
use crate::{
    app::FluxRun,
    gameplay::{Mode, Session},
};
use alloc::format;
use core::num::NonZeroU16;
use mirui::{
    prelude::*,
    ui::widgets::{Button, Text},
};
use mirui_sprite_stack::{Camera, Resolution, StackViewport};

#[mirui::compose]
fn control(model: model!(FluxRun), index: u8, label: &'static str, size: u16) -> Entity {
    ui! {
        Button (
            text: label,
            grow: 1.0,
            min_width: 0,
            height: size,
            font_size: if size <= 20 { 8 } else { 14 }
        ) on Pressed { model.button(index, true); } on Released { model.button(index, false); } on PressCancelled { model.cancel_touch(index); }
    }
}

#[mirui::compose]
pub(crate) fn build(model: model!(FluxRun), compact: bool) -> Entity {
    if !res!(optional MonoClock).with(|clock| clock.is_some()) {
        res!().insert(MonoClock::from_time_source());
    }
    let camera = Camera {
        center: [Fixed::from_int(36), Fixed::from_int(2), Fixed::from_int(36)],
        yaw: Fixed::ZERO,
        pitch: Fixed::from_int(56),
        scale: if compact {
            Fixed::from_ratio(4, 5)
        } else {
            Fixed::from_int(2)
        },
        range: Fixed::from_int(80),
        ..Camera::default()
    };
    let resolution = if compact {
        Resolution::Native
    } else {
        Resolution::Integer {
            limit: [320, 220].map(|side| NonZeroU16::new(side).unwrap()),
            minimum: [8, 8],
            scale: NonZeroU16::new(2).unwrap(),
        }
    };
    let font = if compact { 8 } else { 15 };
    let controls = if compact { 20 } else { 36 };
    let header_height = if compact { 24 } else { 60 };
    let footer_height = if compact { 40 } else { 116 };
    let status_height = if compact { 12 } else { 16 };
    let scene_width = if compact {
        Dimension::px(88)
    } else {
        Dimension::percent(100)
    };
    let scene_height = if compact {
        Dimension::px(64)
    } else {
        Dimension::Auto
    };
    let scene_grow = if compact { 0.0 } else { 1.0 };
    let pad = if compact { 2 } else { 14 };
    let gap = if compact { 2 } else { 10 };
    ui! {
        Column (
            grow: 1.0,
            bg_color: INK,
            clip_children: true,
            min_height: 0,
            align: AlignItems::Center
        ) {
            Row (
                width: Dimension::percent(100),
                height: header_height,
                shrink: 0.0,
                padding: Padding::all(pad),
                gap: gap,
                bg_color: PANEL,
                align: AlignItems::Center
            ) {
                Text (
                    text: if compact { "FLUX" } else { "FLUX RUN" },
                    text_color: CYAN,
                    font_size: font,
                    grow: 1.0
                )
                Text (
                    text: ${ format!("{:02} {}/{}", model.hud().level, model.hud().cores, model.hud().quota) },
                    text_color: BRIGHT,
                    font_size: font
                )
                if ${ model.hud().session == Session::Running } {
                    Button (text: "II", height: controls, min_width: 0, font_size: font) on Tap { model.menu(); }
                }
            }
            StackViewport (
                stage: model.stage(),
                camera: camera,
                resolution: resolution,
                grow: scene_grow,
                width: scene_width,
                height: scene_height,
                min_height: 0,
                bg_color: WATER,
                clip_children: true,
                direction: FlexDirection::Column,
                justify: JustifyContent::FlexEnd,
                padding: Padding::all(pad)
            ) {
                if ${ model.hud().session == Session::Title } {
                    Column (
                        width: Dimension::percent(100),
                        height: if compact { 34 } else { 130 },
                        bg_color: PANEL,
                        padding: Padding::all(pad),
                        gap: gap
                    ) {
                        Text (
                            text: if compact { "SALVAGE" } else { "MAGNETIC SALVAGE" },
                            text_color: CYAN,
                            font_size: font
                        )
                        if !compact {
                            Text (
                                text: "Tow cores. Dodge patrols. Throw through the gate.",
                                width: Dimension::percent(100),
                                height: 36,
                                text_color: BRIGHT,
                                font_size: 12
                            )
                        }
                        Row (height: controls, gap: gap) {
                            Button (
                                text: "PLAY",
                                grow: 1.0,
                                height: controls,
                                min_width: 0,
                                font_size: font
                            ) on Tap { model.start(Mode::Manual); }
                            Button (
                                text: "AUTO",
                                grow: 1.0,
                                height: controls,
                                min_width: 0,
                                font_size: font
                            ) on Tap { model.start(Mode::Auto); }
                        }
                    }
                }
                if ${ model.hud().session == Session::Paused } {
                    Column (
                        width: Dimension::percent(100),
                        height: if compact { 56 } else { 136 },
                        bg_color: PANEL,
                        padding: Padding::all(pad),
                        gap: gap
                    ) {
                        Text (text: "PAUSED", text_color: CYAN, font_size: font)
                        Row (height: controls, gap: gap) {
                            Button (
                                text: "GO",
                                grow: 1.0,
                                height: controls,
                                min_width: 0,
                                font_size: font
                            ) on Tap { model.menu(); }
                            Button (
                                text: ${ if model.hud().mode == Mode::Auto { "MANUAL" } else { "AUTO" } },
                                grow: 1.0,
                                height: controls,
                                min_width: 0,
                                font_size: font
                            ) on Tap { model.set_autoplay(model.hud().mode != Mode::Auto); }
                        }
                        Row (height: controls, gap: gap) {
                            Button (
                                text: "RETRY",
                                grow: 1.0,
                                height: controls,
                                min_width: 0,
                                font_size: font
                            ) on Tap { model.retry(); }
                            Button (
                                text: "END",
                                grow: 1.0,
                                height: controls,
                                min_width: 0,
                                font_size: font
                            ) on Tap { model.finish(); }
                        }
                    }
                }
                if ${ matches!(model.hud().session, Session::Departed | Session::Results) } {
                    Column (
                        width: Dimension::percent(100),
                        height: if compact { 46 } else { 114 },
                        bg_color: PANEL,
                        padding: Padding::all(pad),
                        gap: gap
                    ) {
                        Text (
                            text: ${
                                match (compact, model.hud().session) {
                                    (true, Session::Departed) => "CLEARED",
                                    (true, _) => "COMPLETE",
                                    (false, Session::Departed) => "HARBOR CLEARED",
                                    (false, _) => "RUN COMPLETE",
                                }
                            },
                            text_color: CYAN,
                            font_size: font
                        )
                        Text (
                            text: ${
                                if compact {
                                    format!("{} PTS", model.hud().score)
                                } else {
                                    format!("{} PTS / {} REV", model.hud().score, model.hud().revivals)
                                }
                            },
                            width: Dimension::percent(100),
                            height: status_height,
                            text_color: BRIGHT,
                            font_size: font
                        )
                        if ${ model.hud().session == Session::Departed } {
                            Button (text: "NEXT", height: controls, min_width: 0, font_size: font) on Tap { model.next(); }
                        } else {
                            Button (
                                text: "TITLE",
                                height: controls,
                                min_width: 0,
                                font_size: font
                            ) on Tap { model.title(); }
                        }
                    }
                }
            }
            if ${ model.hud().session == Session::Running } {
                Column (
                    width: Dimension::percent(100),
                    height: footer_height,
                    shrink: 0.0,
                    bg_color: PANEL,
                    padding: Padding::all(pad),
                    gap: gap
                ) {
                    Row (height: status_height, gap: gap, align: AlignItems::Center) {
                        Text (
                            text: ${
                                let hud = model.hud();
                                if hud.life == 0 {
                                    "RESPAWN"
                                } else if hud.mode == Mode::Auto {
                                    "AUTO PILOT"
                                } else if hud.cores >= hud.quota {
                                    "EXIT OPEN"
                                } else if hud.tether {
                                    if compact { "THROW" } else { "RELEASE TO THROW" }
                                } else if hud.target {
                                    if compact { "IN REACH" } else { "CORE IN REACH" }
                                } else if compact {
                                    hud.name.split_once(' ').map_or(hud.name, |(first, _)| first)
                                } else {
                                    hud.name
                                }
                            },
                            text_color: AMBER,
                            font_size: font,
                            grow: 1.0
                        )
                        Text (
                            text: ${ format!("{} HP", model.hud().life) },
                            text_color: BRIGHT,
                            font_size: font
                        )
                    }
                    Row (height: controls, gap: gap) {
                        control (model.clone(), 3, "<", controls)
                        control (model.clone(), 0, "^", controls)
                        control (model.clone(), 2, "v", controls)
                        control (model.clone(), 1, ">", controls)
                        control (model.clone(), 4, "MAG", controls)
                    }
                    if !compact {
                        Text (
                            text: ${
                                format!(
                                    "{:06} PTS / {} S / BEST {:06}", model.hud().score, model.hud().seconds, model
                                    .hud().best
                                )
                            },
                            width: Dimension::percent(100),
                            height: status_height,
                            text_color: CYAN,
                            font_size: 10
                        )
                    }
                }
            }
        }
    }
}
