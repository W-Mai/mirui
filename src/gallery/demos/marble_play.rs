extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "audio")]
use crate::audio::{AudioBus, AudioTone, Waveform};
use crate::ecs::DeltaTimeMs;
use crate::gallery::fit_logical_canvas;
#[cfg(feature = "audio")]
use crate::gallery::play::audio::MARBLE_AUDIO_BANK;
use crate::gallery::play::change::ChangeSet;
#[cfg(feature = "audio")]
use crate::gallery::play::marble::MarbleSound;
#[cfg(feature = "audio")]
use crate::gallery::play::marble::PadTimbre;
use crate::gallery::play::marble::{MarbleModel, PAD_PITCHES, Page, THEMES, Theme};
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::types::Fixed64;
use crate::ui::ComputedRect;
use crate::ui::Hidden;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Slider, Switch, Text, TextAlign};
use alloc::format;

#[derive(crate::Component, Default)]
struct MarbleBoard;

#[derive(Clone, Copy)]
struct MarbleNodes {
    board: Entity,
    pause: Entity,
    audio: Entity,
    record: Entity,
    status: Entity,
    counts: [Entity; 2],
    readout: Entity,
    properties: Entity,
    add: Entity,
    remove: Entity,
    inspector: Entity,
    bounce: Entity,
    radius: Entity,
    pitch: Entity,
    timbre: Entity,
    nav: [Entity; 4],
    scene_labels: [Entity; 6],
    setting_labels: [Entity; 4],
    setting_controls: [Entity; 4],
}

impl MarbleNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut MarbleModel) -> ChangeSet) {
        let (changes, sounds) = world
            .resource_mut::<MarbleModel>()
            .map(|model| {
                let changes = update(model);
                (changes, model.take_sounds())
            })
            .unwrap_or((ChangeSet::NONE, [None; 8]));
        #[cfg(feature = "audio")]
        if let Some(audio) = world.resource_mut::<AudioBus>() {
            for sound in sounds.into_iter().flatten() {
                let MarbleSound::Pad {
                    pitch,
                    timbre,
                    gain,
                    delay_ms,
                    ..
                } = sound;
                submit_pad_sound(audio, pitch, timbre, gain, delay_ms);
            }
        }
        #[cfg(not(feature = "audio"))]
        let _ = sounds;
        if changes.contains(ChangeSet::VISUAL)
            && let Some(board) = world.resource::<Self>().map(|nodes| nodes.board)
        {
            world.invalidate_visual(board);
        }
        if changes.contains(ChangeSet::MODEL) {
            Self::sync(world);
        }
    }

    fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<MarbleModel>() else {
            return;
        };
        let page = model.page;
        let paused = model.paused;
        #[cfg(feature = "audio")]
        let audio_state = world
            .resource::<AudioBus>()
            .map(|audio| (true, audio.is_muted()))
            .unwrap_or((false, false));
        #[cfg(not(feature = "audio"))]
        let audio_state = (false, false);
        const MARBLE_COUNTS: [&str; 9] = [
            "0 MARBLES",
            "1 MARBLE",
            "2 MARBLES",
            "3 MARBLES",
            "4 MARBLES",
            "5 MARBLES",
            "6 MARBLES",
            "7 MARBLES",
            "8 MARBLES",
        ];
        const PAD_COUNTS: [&str; 7] = [
            "0 PADS", "1 PAD", "2 PADS", "3 PADS", "4 PADS", "5 PADS", "6 PADS",
        ];
        let counts = [
            MARBLE_COUNTS[model.ball_count()],
            PAD_COUNTS[model.pad_count()],
        ];
        let status = match page {
            Page::Play if paused => "HOLD · PHYSICS PAUSED",
            Page::Play => "LIVE · DRAG EMPTY SPACE TO TILT",
            Page::Edit if model.add_mode => "EDIT · TAP EMPTY SPACE TO ADD",
            Page::Edit => "EDIT · DRAG A PAD TO MOVE",
            Page::Scenes => "SCENES · CHOOSE A LITTLE WORLD",
            Page::Settings => "SETTINGS · SESSION ONLY",
        };
        let readout = match page {
            Page::Play => format!(
                "PAD {} · {:.2} g · {} HITS",
                model.selected_pad().letter as char,
                model.gravity.to_f32(),
                model.hits
            ),
            Page::Edit => format!(
                "PAD {} · RADIUS {} · BOUNCE {:.2}",
                model.selected_pad().letter as char,
                model.selected_pad().radius.to_int(),
                model.selected_pad().bounce.to_f32()
            ),
            Page::Scenes => "PRESETS RESET LAYOUT AND MARBLES".into(),
            Page::Settings => "GRAVITY · TRAILS · FEEDBACK".into(),
        };
        let inspector_open = model.inspector;
        let gravity = model.gravity.to_fixed();
        let toggles = [model.trails, model.feedback];
        let bounce = model.selected_pad().bounce.to_fixed();
        let radius = model.selected_pad().radius.to_fixed();
        let pitch = pitch_label(model.selected_pad().pitch);
        let timbre = model.selected_pad().timbre.label();
        let bpm = model.bpm;
        let text_updates = [
            (
                nodes.pause,
                if paused { "PLAY".into() } else { "HOLD".into() },
            ),
            (nodes.status, status.into()),
            (nodes.readout, readout),
            (nodes.pitch, pitch.into()),
            (nodes.timbre, timbre.into()),
            (nodes.setting_labels[1], format!("TEMPO · {bpm} BPM")),
            (
                nodes.record,
                if model.recording {
                    "DONE".into()
                } else if model.looping {
                    "STOP".into()
                } else {
                    "REC".into()
                },
            ),
        ];
        let _ = model;
        for (entity, text) in text_updates {
            if let Some(label) = world.get_mut::<Text>(entity) {
                label.set_content(text);
            }
            world.invalidate(entity);
        }
        for entity in [nodes.audio, nodes.record] {
            if audio_state.0 {
                world.remove::<Hidden>(entity);
            } else if !world.has::<Hidden>(entity) {
                world.insert(entity, Hidden);
            }
            world.invalidate(entity);
        }
        if let Some(label) = world.get_mut::<Text>(nodes.audio) {
            label.set_content(if audio_state.1 { "SOUND" } else { "ON" });
        }
        world.invalidate(nodes.audio);
        for (entity, text) in nodes.counts.into_iter().zip(counts) {
            if let Some(label) = world.get_mut::<Text>(entity) {
                label.set_content(text);
            }
            world.invalidate(entity);
        }
        for (index, entity) in nodes.nav.into_iter().enumerate() {
            let active = index == page as usize;
            if let Some(button) = world.get_mut::<Button>(entity) {
                button.normal_color = if active {
                    Color::rgb(217, 248, 138).into()
                } else {
                    Color::rgb(34, 47, 37).into()
                };
            }
            let label = world
                .get::<crate::ui::Children>(entity)
                .and_then(|children| children.0.first())
                .copied();
            if let Some(label) = label {
                if let Some(style) = world.get_mut::<Style>(label) {
                    style.text_color = if active {
                        Color::rgb(48, 69, 41).into()
                    } else {
                        TEXT.into()
                    };
                }
                world.invalidate_visual(label);
            }
            world.invalidate_visual(entity);
        }
        for entity in [nodes.properties, nodes.add, nodes.remove] {
            if page == Page::Edit {
                world.remove::<Hidden>(entity);
            } else if !world.has::<Hidden>(entity) {
                world.insert(entity, Hidden);
            }
            world.invalidate(entity);
        }
        if inspector_open {
            world.remove::<Hidden>(nodes.inspector);
        } else if !world.has::<Hidden>(nodes.inspector) {
            world.insert(nodes.inspector, Hidden);
        }
        for (entity, value) in [(nodes.bounce, bounce), (nodes.radius, radius)] {
            if let Some(slider) = world.get_mut::<Slider>(entity) {
                slider.value = value;
            }
            world.invalidate_visual(entity);
        }
        world.invalidate(nodes.inspector);
        for entity in nodes.scene_labels {
            if page == Page::Scenes {
                world.remove::<Hidden>(entity);
            } else if !world.has::<Hidden>(entity) {
                world.insert(entity, Hidden);
            }
            world.invalidate(entity);
        }
        for entity in nodes.setting_labels {
            if page == Page::Settings {
                world.remove::<Hidden>(entity);
            } else if !world.has::<Hidden>(entity) {
                world.insert(entity, Hidden);
            }
            world.invalidate(entity);
        }
        for entity in nodes.setting_controls {
            if page == Page::Settings {
                world.remove::<Hidden>(entity);
            } else if !world.has::<Hidden>(entity) {
                world.insert(entity, Hidden);
            }
            world.invalidate(entity);
        }
        if let Some(slider) = world.get_mut::<Slider>(nodes.setting_controls[0]) {
            slider.value = gravity;
        }
        if let Some(slider) = world.get_mut::<Slider>(nodes.setting_controls[1]) {
            slider.value = Fixed::from_int(i32::from(bpm));
        }
        for (entity, on) in nodes.setting_controls[2..].iter().copied().zip(toggles) {
            if let Some(switch) = world.get_mut::<Switch>(entity) {
                switch.on = on;
            }
            world.invalidate_visual(entity);
        }
    }
}

fn pitch_label(pitch: u8) -> &'static str {
    const LABELS: [&str; 10] = ["C4", "D4", "E4", "G4", "A4", "C5", "D5", "E5", "G5", "A5"];
    PAD_PITCHES
        .iter()
        .position(|candidate| *candidate == pitch)
        .map(|index| LABELS[index])
        .unwrap_or("NOTE")
}

#[cfg(feature = "audio")]
fn scaled_gain(gain: u8, scale: u8) -> u8 {
    (u16::from(gain) * u16::from(scale) / 255) as u8
}

#[cfg(feature = "audio")]
fn submit_pad_sound(audio: &mut AudioBus, pitch: u8, timbre: PadTimbre, gain: u8, delay_ms: u16) {
    let mut tone = |pitch, waveform, duration_ms, scale| {
        let _ = audio.tone(
            AudioTone::new(pitch, waveform, duration_ms, scaled_gain(gain, scale))
                .with_delay_ms(delay_ms),
        );
    };
    match timbre {
        PadTimbre::Mallet => {
            tone(pitch, Waveform::Sine, 720, 225);
            tone(pitch.saturating_add(17).min(127), Waveform::Sine, 190, 47);
        }
        PadTimbre::Synth => {
            tone(pitch, Waveform::Sine, 950, 190);
            tone(pitch, Waveform::Triangle, 700, 54);
        }
        PadTimbre::Bass => {
            let bass = pitch.saturating_sub(12);
            tone(bass, Waveform::Sine, 360, 230);
            tone(bass, Waveform::Triangle, 280, 43);
        }
        PadTimbre::Drum => {
            tone(48, Waveform::Sine, 180, 220);
            tone(41, Waveform::Triangle, 120, 72);
        }
    }
}

#[cfg(feature = "audio")]
pub fn audio_bank() -> &'static crate::audio::AudioBank {
    &MARBLE_AUDIO_BANK
}

fn toggle_audio(world: &mut World) {
    #[cfg(feature = "audio")]
    {
        let preview = world.resource::<MarbleModel>().map(|model| {
            let pad = model.selected_pad();
            (pad.pitch, pad.timbre)
        });
        if let Some(audio) = world.resource_mut::<AudioBus>() {
            let enable = audio.is_muted();
            let _ = audio.set_muted(!enable);
            if enable && let Some((pitch, timbre)) = preview {
                submit_pad_sound(audio, pitch, timbre, 190, 0);
            }
        }
    }
    MarbleNodes::sync(world);
}

const MUTED: Color = Color::rgb(137, 156, 123);
const TEXT: Color = Color::rgb(225, 233, 214);

fn color(value: u32) -> Color {
    Color::rgb(
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    )
}

fn mix(a: u32, b: u32, amount: Fixed) -> Color {
    color(a).blend_with(color(b), amount)
}

fn point(value: crate::gallery::play::marble::Vec2, y_offset: i32) -> Point {
    Point {
        x: value.x.to_fixed(),
        y: value.y.to_fixed() - Fixed::from_int(y_offset),
    }
}

fn paint_play_board(painter: &mut PlayPainter<'_, '_>, model: &MarbleModel, theme: Theme) {
    let board = Rect::new(10, 0, 460, 199);
    painter.fill(board, color(theme.bg), Fixed::from_int(10));
    painter.border(
        board,
        mix(theme.bg, 0x90a679, Fixed::from_ratio(15, 100)),
        Fixed::ONE,
        Fixed::from_int(10),
    );
    for slot in 0..model.ball_count() {
        painter.circle(
            Point::new(18 + slot as i32 * 7, 10),
            Fixed::from_int(2),
            color(theme.accent).scale_alpha(150),
        );
    }
    for x in (23..463).step_by(16) {
        for y in (62..247).step_by(16) {
            painter.circle(
                Point::new(x, y - 52),
                Fixed::from_ratio(1, 2),
                mix(
                    theme.bg,
                    0x93a278,
                    if model.page == Page::Edit {
                        Fixed::from_ratio(25, 100)
                    } else {
                        Fixed::from_ratio(13, 100)
                    },
                ),
            );
        }
    }
    paint_gravity_direction(painter, model, theme);
    paint_transport(painter, model, theme);
    for (ax, ay, bx, by) in [
        (36, 145, 78, 165),
        (215, 80, 264, 70),
        (269, 213, 308, 234),
        (404, 144, 437, 122),
    ] {
        painter.line(
            Point::new(ax, ay - 50),
            Point::new(bx, by - 50),
            mix(theme.bg, 0x05110a, Fixed::from_ratio(65, 100)),
            Fixed::from_int(6),
        );
        painter.line(
            Point::new(ax, ay - 52),
            Point::new(bx, by - 52),
            Color::rgb(110, 131, 94),
            Fixed::from_int(5),
        );
        painter.line(
            Point::new(ax, ay - 52),
            Point::new(bx, by - 52),
            Color::rgb(189, 204, 164),
            Fixed::from_int(2),
        );
    }
    painter.fill(
        Rect::new(30, 194, 420, 2),
        mix(
            theme.bg,
            theme.accent,
            Fixed::from_ratio(22, 100) + model.flash.to_fixed() * Fixed::from_ratio(4, 10),
        ),
        Fixed::ONE,
    );
    for ring in model.rings.iter().flatten() {
        painter.arc(
            point(ring.pos, 52),
            ring.radius.to_fixed(),
            Fixed::ZERO,
            Fixed::from_int(360),
            color(ring.color).scale_alpha(110),
            Fixed::ONE,
        );
    }
    for (slot, pad) in model.pads.iter().enumerate() {
        let Some(pad) = pad else { continue };
        let pulse = if model.feedback {
            Fixed64::ONE + pad.pulse * Fixed64::from_ratio(75, 1_000)
        } else {
            Fixed64::ONE
        };
        let radius = (pad.radius * pulse).to_fixed();
        let center = point(pad.pos, 52);
        painter.circle(
            Point {
                y: center.y + Fixed::from_int(2),
                ..center
            },
            radius,
            mix(theme.bg, 0x05110a, Fixed::from_ratio(65, 100)),
        );
        painter.circle(
            center,
            radius,
            mix(theme.bg, pad.color, Fixed::from_ratio(65, 100)),
        );
        painter.circle(
            center,
            radius - Fixed::ONE,
            mix(theme.bg, pad.color, Fixed::from_ratio(12, 100)),
        );
        painter.arc(
            center,
            radius - Fixed::from_int(4),
            Fixed::ZERO,
            Fixed::from_int(360),
            color(pad.color).scale_alpha(80),
            Fixed::ONE,
        );
        painter.arc(
            center,
            radius - Fixed::ONE,
            Fixed::from_int(204),
            Fixed::from_int(281),
            color(pad.color),
            Fixed::ONE,
        );
        if slot == model.selected {
            let id_width = 4 + model.selected_pad().id.min(12) as i32;
            painter.fill(
                Rect::new(
                    center.x - Fixed::from_int(id_width / 2),
                    center.y - Fixed::ONE,
                    id_width,
                    2,
                ),
                color(pad.color),
                Fixed::ONE,
            );
            painter.arc(
                center,
                radius + Fixed::from_int(4),
                Fixed::from_int(340),
                Fixed::from_int(380),
                color(pad.color).scale_alpha(180),
                Fixed::ONE,
            );
            painter.arc(
                center,
                radius + Fixed::from_int(4),
                Fixed::from_int(160),
                Fixed::from_int(200),
                color(pad.color).scale_alpha(180),
                Fixed::ONE,
            );
        }
    }
    for ball in model.balls.iter().flatten() {
        if model.trails {
            for index in 1..ball.trail_len {
                painter.line(
                    point(ball.trail[index - 1], 52),
                    point(ball.trail[index], 52),
                    color(ball.color).scale_alpha((24 + index * 27) as u8),
                    Fixed::from_ratio(5 + index as i32, 4),
                );
            }
        }
        let center = point(ball.pos, 52);
        painter.circle(center, Fixed::from_int(4), color(ball.color));
        painter.circle(
            Point {
                x: center.x - Fixed::ONE,
                y: center.y - Fixed::ONE,
            },
            Fixed::ONE,
            Color::rgb(251, 255, 239),
        );
    }
    for particle in model.particles.iter().flatten() {
        painter.circle(
            point(particle.pos, 52),
            Fixed::ONE,
            color(particle.color).scale_alpha(170),
        );
    }
}

fn paint_transport(painter: &mut PlayPainter<'_, '_>, model: &MarbleModel, theme: Theme) {
    let active = model.transport_beat();
    for beat in 0..16 {
        let emphasized = beat % 4 == 0;
        let color = if beat == active {
            if model.recording {
                Color::rgb(238, 172, 139)
            } else {
                color(theme.accent)
            }
        } else if model.looping {
            color(theme.accent).scale_alpha(if emphasized { 90 } else { 55 })
        } else {
            mix(
                theme.bg,
                theme.accent,
                Fixed::from_ratio(if emphasized { 30 } else { 16 }, 100),
            )
        };
        painter.circle(
            Point::new(170 + beat as i32 * 7, 10),
            if beat == active {
                Fixed::from_ratio(3, 2)
            } else {
                Fixed::from_ratio(3, 4)
            },
            color,
        );
    }
}

fn paint_gravity_direction(painter: &mut PlayPainter<'_, '_>, model: &MarbleModel, theme: Theme) {
    let center = Point::new(435, 18);
    painter.circle(
        center,
        Fixed::from_int(15),
        mix(theme.bg, theme.panel, Fixed::from_ratio(55, 100)),
    );
    painter.arc(
        center,
        Fixed::from_int(15),
        Fixed::ZERO,
        Fixed::from_int(360),
        mix(theme.bg, theme.accent, Fixed::from_ratio(35, 100)),
        Fixed::ONE,
    );
    let target = crate::gallery::play::marble::Vec2 {
        x: model.target.x * Fixed64::from_int(190),
        y: model.gravity * Fixed64::from_int(150) + model.target.y * Fixed64::from_int(200),
    };
    let actual = crate::gallery::play::marble::Vec2 {
        x: model.tilt.x * Fixed64::from_int(190),
        y: model.gravity * Fixed64::from_int(150) + model.tilt.y * Fixed64::from_int(200),
    };
    paint_gravity_arrow(
        painter,
        center,
        target,
        Fixed::from_int(12),
        color(theme.accent).scale_alpha(110),
        Fixed::ONE,
        false,
    );
    paint_gravity_arrow(
        painter,
        center,
        actual,
        Fixed::from_int(9),
        color(theme.accent),
        Fixed::from_ratio(3, 2),
        true,
    );
    painter.circle(center, Fixed::from_int(2), color(theme.accent));
}

fn paint_gravity_arrow(
    painter: &mut PlayPainter<'_, '_>,
    center: Point,
    vector: crate::gallery::play::marble::Vec2,
    length: Fixed,
    color: Color,
    width: Fixed,
    arrowhead: bool,
) {
    let magnitude = (vector.x * vector.x + vector.y * vector.y).sqrt();
    if magnitude <= Fixed64::from_ratio(1, 1_000) {
        return;
    }
    let dx = (vector.x / magnitude).to_fixed();
    let dy = (vector.y / magnitude).to_fixed();
    let end = Point {
        x: center.x + dx * length,
        y: center.y + dy * length,
    };
    painter.line(center, end, color, width);
    if arrowhead {
        let back = Fixed::from_int(3);
        let side = Fixed::from_int(2);
        let base = Point {
            x: end.x - dx * back,
            y: end.y - dy * back,
        };
        painter.line(
            end,
            Point {
                x: base.x - dy * side,
                y: base.y + dx * side,
            },
            color,
            width,
        );
        painter.line(
            end,
            Point {
                x: base.x + dy * side,
                y: base.y - dx * side,
            },
            color,
            width,
        );
    } else {
        painter.circle(end, Fixed::from_ratio(3, 2), color);
    }
}

fn paint_scene_cards(painter: &mut PlayPainter<'_, '_>, selected: usize) {
    painter.fill(
        Rect::new(10, 0, 460, 199),
        color(THEMES[selected].bg),
        Fixed::from_int(10),
    );
    for (index, theme) in THEMES.into_iter().enumerate() {
        let x = 18 + index as i32 * 151;
        let area = Rect::new(x, 18, 140, 163);
        painter.fill(area, color(theme.panel), Fixed::from_int(8));
        painter.border(
            area,
            if index == selected {
                color(theme.accent)
            } else {
                mix(theme.bg, 0x90a679, Fixed::from_ratio(25, 100))
            },
            Fixed::ONE,
            Fixed::from_int(8),
        );
        painter.fill(
            Rect::new(x + 10, 29, 120, 72),
            color(theme.bg),
            Fixed::from_int(6),
        );
        for dot in 0..3 {
            painter.circle(
                Point::new(x + 37 + dot * 29, 57 + (dot % 2) * 17),
                Fixed::from_int(7),
                color(crate::gallery::play::marble::PALETTE[(index + dot as usize) % 5]),
            );
        }
        painter.fill(
            Rect::new(x + 14, 108, 18 + theme.name.len() as i32 * 4, 2),
            color(theme.accent),
            Fixed::ONE,
        );
        painter.fill(
            Rect::new(x + 14, 160, 12 + theme.subtitle.len() as i32 * 3, 2),
            MUTED,
            Fixed::ONE,
        );
    }
}

fn paint_settings(painter: &mut PlayPainter<'_, '_>, _model: &MarbleModel, theme: Theme) {
    painter.fill(
        Rect::new(10, 0, 460, 199),
        color(theme.bg),
        Fixed::from_int(10),
    );
    painter.fill(
        Rect::new(24, 18, 432, 163),
        color(theme.panel),
        Fixed::from_int(9),
    );
    for y in [63, 113, 151] {
        painter.line(
            Point::new(42, y),
            Point::new(422, y),
            mix(theme.panel, 0xa1b887, Fixed::from_ratio(18, 100)),
            Fixed::ONE,
        );
    }
}

fn board_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<MarbleModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 199);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    match model.page {
        Page::Play | Page::Edit => paint_play_board(&mut painter, model, model.theme()),
        Page::Scenes => paint_scene_cards(&mut painter, model.scene),
        Page::Settings => paint_settings(&mut painter, model, model.theme()),
    }
}

fn board_view() -> View {
    View::new("MarbleBoard", 60, board_render).with_filter::<MarbleBoard>()
}

fn event_point(
    world: &World,
    entity: Entity,
    x: Fixed,
    y: Fixed,
) -> Option<crate::gallery::play::marble::Vec2> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = (x - rect.x) * Fixed::from_int(480) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(199) / rect.h + Fixed::from_int(52);
    Some(crate::gallery::play::marble::Vec2 {
        x: Fixed64::from_fixed(local_x),
        y: Fixed64::from_fixed(local_y),
    })
}

fn board_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let (x, y, action) = match event {
        GestureEvent::Tap { x, y, .. } => (*x, *y, 4),
        GestureEvent::DragStart { x, y, .. } => (*x, *y, 0),
        GestureEvent::DragMove { x, y, .. } => (*x, *y, 1),
        GestureEvent::DragEnd { x, y, .. } => (*x, *y, 2),
        GestureEvent::DragCancel { x, y, .. } => (*x, *y, 3),
        _ => return false,
    };
    let Some(point) = event_point(world, entity, x, y) else {
        return false;
    };
    MarbleNodes::update(world, |model| match action {
        0 => model.begin_board_drag(point),
        1 => model.move_board_drag(point),
        2 => model.end_board_drag(false),
        3 => model.end_board_drag(true),
        _ => {
            let local_y = point.y - Fixed64::from_int(52);
            if model.page == Page::Scenes {
                if (Fixed64::from_int(18)..=Fixed64::from_int(181)).contains(&local_y) {
                    let slot = ((point.x.to_int() - 18) / 151).clamp(0, 2) as usize;
                    return model.reset_scene(slot);
                }
                return ChangeSet::NONE;
            }
            if model.page == Page::Settings {
                return ChangeSet::NONE;
            }
            let started = model.begin_board_drag(point);
            started | model.end_board_drag(false)
        }
    });
    true
}

#[mirui_macros::system(order = ANIMATION)]
fn marble_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    MarbleNodes::update(world, |model| model.advance_ms(elapsed));
}

fn symmetric_padding(vertical: i32, horizontal: i32) -> Padding {
    Padding {
        top: Dimension::px(vertical),
        right: Dimension::px(horizontal),
        bottom: Dimension::px(vertical),
        left: Dimension::px(horizontal),
    }
}

#[compose]
fn build_widgets() {
    ui! {
        Column (
            width: 480,
            height: 320,
            bg_color: Color::rgb(23, 34, 28)
        ) {
            Row (
                height: 29,
                padding: symmetric_padding(4, 10),
                align: AlignItems::Center,
                column_gap: 6,
                bg_color: Color::rgb(31, 45, 35)
            ) {
                Text (
                    "MARBLE PLAY",
                    grow: 1.0,
                    height: 21,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "HOLD",
                    id: "marble_pause",
                    size: ButtonSize::Compact,
                    width: 52,
                    height: 22,
                    font_size: 9,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { MarbleNodes::update(ctx.world, MarbleModel::toggle_pause); }
                Button (
                    "SOUND",
                    id: "marble_audio",
                    size: ButtonSize::Compact,
                    width: 50,
                    height: 22,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { toggle_audio(ctx.world); }
                Button (
                    "REC",
                    id: "marble_record",
                    size: ButtonSize::Compact,
                    width: 44,
                    height: 22,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(238, 172, 139),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { MarbleNodes::update(ctx.world, MarbleModel::toggle_recording); }
                Button (
                    "+",
                    size: ButtonSize::Compact,
                    width: 54,
                    height: 22,
                    font_size: 12,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { MarbleNodes::update(ctx.world, MarbleModel::drop_ball); }
            }
            Row (
                height: 23,
                padding: symmetric_padding(3, 14),
                align: AlignItems::Center,
                column_gap: 10
            ) {
                Text (
                    "LIVE · DRAG EMPTY SPACE TO TILT",
                    id: "marble_status",
                    grow: 1.0,
                    height: 17,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "5 MARBLES",
                    id: "marble_marble_count",
                    width: 74,
                    height: 17,
                    font_size: 8,
                    text_color: Color::rgb(217, 248, 138),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "5 PADS",
                    id: "marble_pad_count",
                    width: 52,
                    height: 17,
                    font_size: 8,
                    text_color: Color::rgb(217, 248, 138),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
            }
            MarbleBoard (
                id: "marble_play_board",
                width: 480,
                height: 199,
                clip_children: true
            ) [
                TouchAction::None,
            ] on Tap { board_gesture(ctx.world, ctx.entity, ctx.event); } on DragStart { board_gesture(ctx.world, ctx.entity, ctx.event); } on DragMove { board_gesture(ctx.world, ctx.entity, ctx.event); } on DragEnd { board_gesture(ctx.world, ctx.entity, ctx.event); } on DragCancel { board_gesture(ctx.world, ctx.entity, ctx.event); }
            {
                Text (
                    "DAYDREAM",
                    id: "marble_scene_0_name",
                    position: Position::Absolute,
                    left: 32,
                    top: 118,
                    width: 112,
                    height: 18,
                    font_size: 8,
                    text_color: Color::rgb(225, 233, 214),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "SOFT GREEN",
                    id: "marble_scene_0_sub",
                    position: Position::Absolute,
                    left: 32,
                    top: 139,
                    width: 112,
                    height: 16,
                    font_size: 7,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "AFTER HOURS",
                    id: "marble_scene_1_name",
                    position: Position::Absolute,
                    left: 183,
                    top: 118,
                    width: 112,
                    height: 18,
                    font_size: 8,
                    text_color: Color::rgb(225, 233, 214),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "BLUE GREY",
                    id: "marble_scene_1_sub",
                    position: Position::Absolute,
                    left: 183,
                    top: 139,
                    width: 112,
                    height: 16,
                    font_size: 7,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "ZERO GRAVITY",
                    id: "marble_scene_2_name",
                    position: Position::Absolute,
                    left: 334,
                    top: 118,
                    width: 112,
                    height: 18,
                    font_size: 8,
                    text_color: Color::rgb(225, 233, 214),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "COOL BLUE",
                    id: "marble_scene_2_sub",
                    position: Position::Absolute,
                    left: 334,
                    top: 139,
                    width: 112,
                    height: 16,
                    font_size: 7,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "GRAVITY",
                    id: "marble_setting_gravity",
                    position: Position::Absolute,
                    left: 42,
                    top: 15,
                    width: 100,
                    height: 16,
                    font_size: 8,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "TEMPO · 96 BPM",
                    id: "marble_setting_bpm",
                    position: Position::Absolute,
                    left: 42,
                    top: 65,
                    width: 180,
                    height: 16,
                    font_size: 8,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "TRAILS · 6 POINTS PER MARBLE",
                    id: "marble_setting_trails",
                    position: Position::Absolute,
                    left: 42,
                    top: 122,
                    width: 260,
                    height: 18,
                    font_size: 8,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "FEEDBACK · RINGS / PARTICLES",
                    id: "marble_setting_feedback",
                    position: Position::Absolute,
                    left: 42,
                    top: 157,
                    width: 260,
                    height: 18,
                    font_size: 8,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Slider (
                    id: "marble_setting_gravity_control",
                    position: Position::Absolute,
                    left: 42,
                    top: 29,
                    width: 380,
                    height: 28,
                    min: Fixed::ZERO,
                    max: Fixed::from_ratio(16, 10),
                    value: Fixed::from_ratio(8, 10),
                    track_color: Color::rgb(69, 87, 70),
                    fill_color: Color::rgb(217, 248, 138),
                    thumb_color: Color::rgb(225, 233, 214)
                ) on ValueChanged { MarbleNodes::update(ctx.world, |model| model.set_gravity(Fixed64::from_fixed(*new))); }
                Slider (
                    id: "marble_setting_bpm_control",
                    position: Position::Absolute,
                    left: 42,
                    top: 79,
                    width: 380,
                    height: 28,
                    min: Fixed::from_int(55),
                    max: Fixed::from_int(160),
                    value: Fixed::from_int(96),
                    track_color: Color::rgb(69, 87, 70),
                    fill_color: Color::rgb(198, 176, 239),
                    thumb_color: Color::rgb(225, 233, 214)
                ) on ValueChanged { MarbleNodes::update(ctx.world, |model| model.set_bpm(Fixed64::from_fixed(*new))); }
                Switch (
                    id: "marble_setting_trails_control",
                    position: Position::Absolute,
                    left: 370,
                    top: 121,
                    width: 54,
                    height: 24,
                    on: true,
                    on_color: Color::rgb(217, 248, 138),
                    off_color: Color::rgb(69, 87, 70),
                    thumb_color: Color::rgb(23, 34, 28)
                ) on Toggled {
                    MarbleNodes::update(
                        ctx.world,
                        |model| {
                            if model.trails != *now { model.toggle_trails() } else { ChangeSet::NONE }
                        },
                    );
                }
                Switch (
                    id: "marble_setting_feedback_control",
                    position: Position::Absolute,
                    left: 370,
                    top: 156,
                    width: 54,
                    height: 24,
                    on: true,
                    on_color: Color::rgb(217, 248, 138),
                    off_color: Color::rgb(69, 87, 70),
                    thumb_color: Color::rgb(23, 34, 28)
                ) on Toggled {
                    MarbleNodes::update(
                        ctx.world,
                        |model| {
                            if model.feedback != *now {
                                model.toggle_feedback()
                            } else {
                                ChangeSet::NONE
                            }
                        },
                    );
                }
            }
            Row (
                height: 35,
                padding: symmetric_padding(5, 14),
                align: AlignItems::Center,
                column_gap: 6
            ) {
                Text (
                    "PAD A · BOUNDED FIXED-POINT PHYSICS",
                    id: "marble_readout",
                    grow: 1.0,
                    height: 22,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "PROPS",
                    id: "marble_properties",
                    size: ButtonSize::Compact,
                    width: 52,
                    height: 23,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_inspector(true)); }
                Button (
                    "ADD",
                    id: "marble_add",
                    size: ButtonSize::Compact,
                    width: 46,
                    height: 23,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { MarbleNodes::update(ctx.world, MarbleModel::toggle_add_mode); }
                Button (
                    "REMOVE",
                    id: "marble_remove",
                    size: ButtonSize::Compact,
                    width: 62,
                    height: 23,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(238, 172, 139),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { MarbleNodes::update(ctx.world, MarbleModel::remove_selected); }
            }
            Row (
                height: 34,
                padding: symmetric_padding(4, 5),
                column_gap: 6,
                bg_color: Color::rgb(31, 45, 35)
            ) {
                Button (
                    "PLAY",
                    id: "marble_nav_play",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: Color::rgb(217, 248, 138),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: Color::rgb(48, 69, 41),
                    border_radius: 6
                ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_page(Page::Play)); }
                Button (
                    "EDIT",
                    id: "marble_nav_edit",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_page(Page::Edit)); }
                Button (
                    "SCENES",
                    id: "marble_nav_scenes",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_page(Page::Scenes)); }
                Button (
                    "SETTINGS",
                    id: "marble_nav_settings",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_page(Page::Settings)); }
            }
            View (
                id: "marble_inspector",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 286,
                bg_color: Color::rgba(6, 12, 8, 150)
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Column (
                    position: Position::Absolute,
                    left: 213,
                    top: 35,
                    width: 253,
                    height: 247,
                    padding: Padding::all(10),
                    row_gap: 4,
                    bg_color: Color::rgb(34, 47, 37),
                    border_color: Color::rgb(217, 248, 138),
                    border_width: 1,
                    border_radius: 9
                ) {
                    Row (height: 22, align: AlignItems::Center) {
                        Text (
                            "PAD PROPERTIES",
                            grow: 1.0,
                            height: 20,
                            font_size: 9,
                            text_color: TEXT,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Button (
                            "X",
                            size: ButtonSize::Compact,
                            width: 24,
                            height: 22,
                            font_size: 10,
                            normal_color: Color::rgb(45, 61, 48),
                            pressed_color: Color::rgb(217, 248, 138),
                            text_color: TEXT,
                            border_radius: 6
                        ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_inspector(false)); }
                    }
                    Text (
                        "COLOR",
                        height: 12,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Row (height: 20, column_gap: 8) {
                        Button (
                            "",
                            size: ButtonSize::Custom,
                            grow: 1.0,
                            height: 20,
                            normal_color: Color::rgb(180, 234, 189),
                            pressed_color: Color::rgb(180, 234, 189),
                            border_radius: 5
                        ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_color(0)); }
                        Button (
                            "",
                            size: ButtonSize::Custom,
                            grow: 1.0,
                            height: 20,
                            normal_color: Color::rgb(198, 176, 239),
                            pressed_color: Color::rgb(198, 176, 239),
                            border_radius: 5
                        ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_color(1)); }
                        Button (
                            "",
                            size: ButtonSize::Custom,
                            grow: 1.0,
                            height: 20,
                            normal_color: Color::rgb(238, 217, 132),
                            pressed_color: Color::rgb(238, 217, 132),
                            border_radius: 5
                        ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_color(2)); }
                        Button (
                            "",
                            size: ButtonSize::Custom,
                            grow: 1.0,
                            height: 20,
                            normal_color: Color::rgb(238, 172, 139),
                            pressed_color: Color::rgb(238, 172, 139),
                            border_radius: 5
                        ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_color(3)); }
                        Button (
                            "",
                            size: ButtonSize::Custom,
                            grow: 1.0,
                            height: 20,
                            normal_color: Color::rgb(160, 210, 232),
                            pressed_color: Color::rgb(160, 210, 232),
                            border_radius: 5
                        ) on Tap { MarbleNodes::update(ctx.world, |model| model.set_color(4)); }
                    }
                    Row (height: 24, align: AlignItems::Center, column_gap: 6) {
                        Text (
                            "PITCH",
                            width: 50,
                            height: 18,
                            font_size: 8,
                            text_color: MUTED,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Button (
                            "-",
                            size: ButtonSize::Compact,
                            width: 34,
                            height: 22,
                            font_size: 10,
                            normal_color: Color::rgb(45, 61, 48),
                            pressed_color: Color::rgb(217, 248, 138),
                            text_color: TEXT,
                            border_radius: 5
                        ) on Tap { MarbleNodes::update(ctx.world, |model| model.adjust_pitch(-1)); }
                        Text (
                            "C5",
                            id: "marble_pitch",
                            grow: 1.0,
                            height: 18,
                            font_size: 9,
                            text_color: TEXT,
                            paragraph: ParagraphStyle::label()
                        )
                        Button (
                            "+",
                            size: ButtonSize::Compact,
                            width: 34,
                            height: 22,
                            font_size: 10,
                            normal_color: Color::rgb(45, 61, 48),
                            pressed_color: Color::rgb(217, 248, 138),
                            text_color: TEXT,
                            border_radius: 5
                        ) on Tap { MarbleNodes::update(ctx.world, |model| model.adjust_pitch(1)); }
                    }
                    Row (height: 24, align: AlignItems::Center, column_gap: 6) {
                        Text (
                            "TIMBRE",
                            width: 50,
                            height: 18,
                            font_size: 8,
                            text_color: MUTED,
                            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                        )
                        Button (
                            "MALLET",
                            id: "marble_timbre",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 22,
                            font_size: 8,
                            normal_color: Color::rgb(45, 61, 48),
                            pressed_color: Color::rgb(198, 176, 239),
                            text_color: TEXT,
                            border_radius: 5
                        ) on Tap { MarbleNodes::update(ctx.world, MarbleModel::cycle_timbre); }
                    }
                    Text (
                        "BOUNCE",
                        height: 12,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Slider (
                        id: "marble_bounce",
                        height: 20,
                        min: Fixed::from_ratio(7, 10),
                        max: Fixed::from_ratio(14, 10),
                        value: Fixed::from_ratio(11, 10),
                        track_color: Color::rgb(69, 87, 70),
                        fill_color: Color::rgb(217, 248, 138),
                        thumb_color: Color::rgb(225, 233, 214)
                    ) on ValueChanged { MarbleNodes::update(ctx.world, |model| model.set_bounce(Fixed64::from_fixed(*new))); }
                    Text (
                        "RADIUS",
                        height: 12,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Slider (
                        id: "marble_radius",
                        height: 20,
                        min: Fixed::from_int(13),
                        max: Fixed::from_int(23),
                        value: Fixed::from_int(18),
                        track_color: Color::rgb(69, 87, 70),
                        fill_color: Color::rgb(217, 248, 138),
                        thumb_color: Color::rgb(225, 233, 214)
                    ) on ValueChanged { MarbleNodes::update(ctx.world, |model| model.set_radius(Fixed64::from_fixed(*new))); }
                }
            }
        }
    };
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.with_widget(board_view());
    app.world.insert_resource(MarbleModel::new());
    app.add_system(marble_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| {
        app.world
            .find_by_id(id)
            .unwrap_or_else(|| panic!("missing {id}"))
    };
    let nodes = MarbleNodes {
        board: find("marble_play_board"),
        pause: find("marble_pause"),
        audio: find("marble_audio"),
        record: find("marble_record"),
        status: find("marble_status"),
        counts: [find("marble_marble_count"), find("marble_pad_count")],
        readout: find("marble_readout"),
        properties: find("marble_properties"),
        add: find("marble_add"),
        remove: find("marble_remove"),
        inspector: find("marble_inspector"),
        bounce: find("marble_bounce"),
        radius: find("marble_radius"),
        pitch: find("marble_pitch"),
        timbre: find("marble_timbre"),
        nav: [
            find("marble_nav_play"),
            find("marble_nav_edit"),
            find("marble_nav_scenes"),
            find("marble_nav_settings"),
        ],
        scene_labels: [
            find("marble_scene_0_name"),
            find("marble_scene_0_sub"),
            find("marble_scene_1_name"),
            find("marble_scene_1_sub"),
            find("marble_scene_2_name"),
            find("marble_scene_2_sub"),
        ],
        setting_labels: [
            find("marble_setting_gravity"),
            find("marble_setting_bpm"),
            find("marble_setting_trails"),
            find("marble_setting_feedback"),
        ],
        setting_controls: [
            find("marble_setting_gravity_control"),
            find("marble_setting_bpm_control"),
            find("marble_setting_trails_control"),
            find("marble_setting_feedback_control"),
        ],
    };
    app.world.insert_resource(nodes);
    #[cfg(feature = "audio")]
    if let Some(audio) = app.world.resource_mut::<AudioBus>() {
        let _ = audio.set_master_gain(107);
        let _ = audio.set_muted(true);
    }
    MarbleNodes::sync(&mut app.world);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::view::ViewRegistry;
    use crate::ui::{Children, IdMap, UiScope};

    fn fixture() -> World {
        let mut world = World::new();
        let mut registry = ViewRegistry::with_builtins();
        registry.insert(board_view());
        world.insert_resource(registry);
        world.insert_resource(IdMap::new());
        world.insert_resource(MarbleModel::new());
        let root = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, root);
        build_widgets(&mut cx);
        let entity = world.find_by_id("marble_play_board").unwrap();
        let find = |id| world.find_by_id(id).unwrap();
        world.insert_resource(MarbleNodes {
            board: entity,
            pause: find("marble_pause"),
            audio: find("marble_audio"),
            record: find("marble_record"),
            status: find("marble_status"),
            counts: [find("marble_marble_count"), find("marble_pad_count")],
            readout: find("marble_readout"),
            properties: find("marble_properties"),
            add: find("marble_add"),
            remove: find("marble_remove"),
            inspector: find("marble_inspector"),
            bounce: find("marble_bounce"),
            radius: find("marble_radius"),
            pitch: find("marble_pitch"),
            timbre: find("marble_timbre"),
            nav: [
                find("marble_nav_play"),
                find("marble_nav_edit"),
                find("marble_nav_scenes"),
                find("marble_nav_settings"),
            ],
            scene_labels: [
                find("marble_scene_0_name"),
                find("marble_scene_0_sub"),
                find("marble_scene_1_name"),
                find("marble_scene_1_sub"),
                find("marble_scene_2_name"),
                find("marble_scene_2_sub"),
            ],
            setting_labels: [
                find("marble_setting_gravity"),
                find("marble_setting_bpm"),
                find("marble_setting_trails"),
                find("marble_setting_feedback"),
            ],
            setting_controls: [
                find("marble_setting_gravity_control"),
                find("marble_setting_bpm_control"),
                find("marble_setting_trails_control"),
                find("marble_setting_feedback_control"),
            ],
        });
        MarbleNodes::sync(&mut world);
        assert!(world.get::<Children>(root).is_some());
        world
    }

    #[test]
    fn composition_uses_real_text_and_button_widgets() {
        let world = fixture();
        assert!(world.query::<Text>().collect().len() >= 4);
        assert!(world.query::<Button>().collect().len() >= 6);
        assert_eq!(world.query::<MarbleBoard>().collect().len(), 1);
    }

    #[test]
    fn drag_cancel_reaches_transactional_model_path() {
        let mut world = fixture();
        let board = world.find_by_id("marble_play_board").unwrap();
        world.insert(board, ComputedRect(Rect::new(0, 52, 480, 199)));
        let original = world.resource::<MarbleModel>().unwrap().selected_pad().pos;
        let _ = world
            .resource_mut::<MarbleModel>()
            .unwrap()
            .set_page(Page::Edit);
        assert!(board_gesture(
            &mut world,
            board,
            &GestureEvent::DragStart {
                x: original.x.to_fixed(),
                y: original.y.to_fixed(),
                target: board,
            }
        ));
        assert!(board_gesture(
            &mut world,
            board,
            &GestureEvent::DragMove {
                x: original.x.to_fixed() + Fixed::from_int(30),
                y: original.y.to_fixed(),
                dx: Fixed::from_int(30),
                dy: Fixed::ZERO,
                target: board,
            }
        ));
        assert!(board_gesture(
            &mut world,
            board,
            &GestureEvent::DragCancel {
                x: original.x.to_fixed() + Fixed::from_int(30),
                y: original.y.to_fixed(),
                target: board,
            }
        ));
        assert_eq!(
            world.resource::<MarbleModel>().unwrap().selected_pad().pos,
            original
        );
    }

    #[test]
    fn tap_selects_a_pad_without_crossing_drag_threshold() {
        let mut world = fixture();
        let board = world.find_by_id("marble_play_board").unwrap();
        world.insert(board, ComputedRect(Rect::new(0, 52, 480, 199)));
        let target = world.resource::<MarbleModel>().unwrap().pads[1]
            .expect("second pad")
            .pos;
        assert!(board_gesture(
            &mut world,
            board,
            &GestureEvent::Tap {
                x: target.x.to_fixed(),
                y: target.y.to_fixed(),
                target: board,
            }
        ));
        let model = world.resource::<MarbleModel>().unwrap();
        assert_eq!(model.selected, 1);
        assert!(model.selected_pad().pulse.is_positive());
    }

    #[test]
    fn scene_card_tap_loads_selected_preset() {
        let mut world = fixture();
        let board = world.find_by_id("marble_play_board").unwrap();
        world.insert(board, ComputedRect(Rect::new(0, 52, 480, 199)));
        let _ = world
            .resource_mut::<MarbleModel>()
            .unwrap()
            .set_page(Page::Scenes);
        assert!(board_gesture(
            &mut world,
            board,
            &GestureEvent::Tap {
                x: Fixed::from_int(340),
                y: Fixed::from_int(120),
                target: board,
            }
        ));
        let model = world.resource::<MarbleModel>().unwrap();
        assert_eq!(model.scene, 2);
        assert_eq!(model.page, Page::Play);
    }
}
