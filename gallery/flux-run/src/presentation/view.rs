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
        Button(text: label, grow: 1.0, min_width: 0, height: size,
            font_size: if size <= 20 { 8 } else { 14 })
            on Pressed { model.button(index, true); }
            on Released { model.button(index, false); }
            on PressCancelled { model.cancel_touch(index); }
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
            Fixed::from_ratio(9, 10)
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
    let pad = if compact { 2 } else { 14 };
    let gap = if compact { 2 } else { 10 };
    ui! {
        Column(grow: 1.0, bg_color: INK, clip_children: true, min_height: 0) {
            Row(padding: Padding::all(pad), gap: gap, bg_color: PANEL, align: AlignItems::Center) {
                Text(text: "FLUX RUN", text_color: CYAN, font_size: font, grow: 1.0)
                Text(text: ${ format!("{:02}  {}/{}", model.hud().level, model.hud().cores, model.hud().quota) },
                    text_color: BRIGHT, font_size: font)
                if ${ model.hud().session == Session::Running } {
                    Button(text: "II", height: controls, min_width: 0, font_size: font) on Tap { model.menu(); }
                }
            }
            StackViewport(stage: model.stage(), camera: camera, resolution: resolution,
                grow: 1.0, min_height: 0, bg_color: WATER, clip_children: true,
                direction: FlexDirection::Column, justify: JustifyContent::FlexEnd, padding: Padding::all(pad)) {
                if ${ model.hud().session == Session::Title } {
                    Column(width: Dimension::percent(100),
                        bg_color: PANEL, padding: Padding::all(pad), gap: gap) {
                        Text(text: "MAGNETIC SALVAGE", text_color: CYAN, font_size: font)
                        if !compact {
                            Text(text: "Catch a core. Curve around a patrol. Let go to send it through the gate.",
                                text_color: BRIGHT, font_size: font)
                        }
                        Row(gap: gap) {
                            Button(text: "PLAY", grow: 1.0, height: controls, min_width: 0, font_size: font) on Tap { model.start(Mode::Manual); }
                            Button(text: "AUTO", grow: 1.0, height: controls, min_width: 0, font_size: font) on Tap { model.start(Mode::Auto); }
                        }
                    }
                }
                if ${ model.hud().session == Session::Paused } {
                    Column(width: Dimension::percent(100),
                        bg_color: PANEL, padding: Padding::all(pad), gap: gap) {
                        Text(text: "PAUSED", text_color: CYAN, font_size: font)
                        Row(gap: gap) {
                            Button(text: "GO", grow: 1.0, height: controls, min_width: 0, font_size: font) on Tap { model.menu(); }
                            Button(text: ${ if model.hud().mode == Mode::Auto { "MANUAL" } else { "AUTO" } },
                                grow: 1.0, height: controls, min_width: 0, font_size: font) on Tap { model.set_autoplay(model.hud().mode != Mode::Auto); }
                        }
                        Row(gap: gap) {
                            Button(text: "RETRY", grow: 1.0, height: controls, min_width: 0, font_size: font) on Tap { model.retry(); }
                            Button(text: "END", grow: 1.0, height: controls, min_width: 0, font_size: font) on Tap { model.finish(); }
                        }
                    }
                }
                if ${ matches!(model.hud().session, Session::Departed | Session::Results) } {
                    Column(width: Dimension::percent(100),
                        bg_color: PANEL, padding: Padding::all(pad), gap: gap) {
                        Text(text: ${ if model.hud().session == Session::Departed { "HARBOR CLEARED" } else { "RUN COMPLETE" } },
                            text_color: CYAN, font_size: font)
                        Text(text: ${ format!("{} PTS  /  {} REV", model.hud().score, model.hud().revivals) },
                            text_color: BRIGHT, font_size: font)
                        if ${ model.hud().session == Session::Departed } {
                            Button(text: "NEXT", height: controls, min_width: 0, font_size: font) on Tap { model.next(); }
                        } else {
                            Button(text: "TITLE", height: controls, min_width: 0, font_size: font) on Tap { model.title(); }
                        }
                    }
                }
            }
            if ${ model.hud().session == Session::Running } {
                Column(bg_color: PANEL, padding: Padding::all(pad), gap: gap) {
                    Row(gap: gap, align: AlignItems::Center) {
                        Text(text: ${
                            let hud = model.hud();
                            if hud.life == 0 { "RESPAWNING" }
                            else if hud.mode == Mode::Auto { "AUTO PILOT" }
                            else if hud.cores >= hud.quota { "EXIT OPEN" }
                            else if hud.tether { "RELEASE TO THROW" }
                            else if hud.target { "CORE IN REACH" }
                            else { hud.name }
                        }, text_color: AMBER, font_size: font, grow: 1.0)
                        Text(text: ${ format!("{} HP", model.hud().life) }, text_color: BRIGHT, font_size: font)
                    }
                    Row(gap: gap) {
                        control(model.clone(), 3, "<", controls)
                        control(model.clone(), 0, "^", controls)
                        control(model.clone(), 2, "v", controls)
                        control(model.clone(), 1, ">", controls)
                        control(model.clone(), 4, "MAG", controls)
                    }
                    if !compact {
                        Text(text: ${ format!("{:06} PTS    /    {} S    /    RECORD {:06}", model.hud().score, model.hud().seconds, model.hud().best) },
                            text_color: CYAN, font_size: font)
                    }
                }
            }
        }
    }
}
