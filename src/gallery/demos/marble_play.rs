extern crate alloc;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
#[cfg(feature = "audio")]
use crate::audio::{
    AudioHandle, AudioOutputState, AudioState, AudioStateSignal, AudioTone, Waveform,
};
use crate::ecs::DeltaTimeMs;
use crate::gallery::fit_logical_canvas;
#[cfg(feature = "audio")]
use crate::gallery::play::audio::MARBLE_AUDIO_BANK;
#[cfg(test)]
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
use crate::ui::view::ViewCtx;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Slider, Switch, Text, TextAlign};
use alloc::format;

pub const VIEWPORT: (u16, u16) = (480, 320);

#[crate::component(bind(model))]
struct MarbleBoard {
    model: MarbleModel,
}

fn marble_status(page: Page, paused: bool, add_mode: bool) -> &'static str {
    match page {
        Page::Play if paused => "HOLD · PHYSICS PAUSED",
        Page::Play => "LIVE · DRAG EMPTY SPACE TO TILT",
        Page::Edit if add_mode => "EDIT · TAP EMPTY SPACE TO ADD",
        Page::Edit => "EDIT · DRAG A PAD TO MOVE",
        Page::Scenes => "SCENES · CHOOSE A LITTLE WORLD",
        Page::Settings => "SETTINGS · SESSION ONLY",
    }
}

fn marble_readout(
    page: Page,
    pad_letter: u8,
    gravity: Fixed,
    hits: u32,
    radius: Fixed,
    bounce: Fixed,
) -> alloc::string::String {
    match page {
        Page::Play => format!(
            "PAD {} · {:.2} g · {} HITS",
            pad_letter as char,
            gravity.to_f32(),
            hits
        ),
        Page::Edit => format!(
            "PAD {} · RADIUS {} · BOUNCE {:.2}",
            pad_letter as char,
            radius.to_int(),
            bounce.to_f32()
        ),
        Page::Scenes => "PRESETS RESET LAYOUT AND MARBLES".into(),
        Page::Settings => "GRAVITY · TRAILS · FEEDBACK".into(),
    }
}

fn marble_count(count: usize) -> &'static str {
    const LABELS: [&str; 9] = [
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
    LABELS[count]
}

fn pad_count(count: usize) -> &'static str {
    const LABELS: [&str; 7] = [
        "0 PADS", "1 PAD", "2 PADS", "3 PADS", "4 PADS", "5 PADS", "6 PADS",
    ];
    LABELS[count]
}

fn nav_color(page: Page, active: Page) -> Color {
    if page == active {
        Color::rgb(217, 248, 138)
    } else {
        Color::rgb(34, 47, 37)
    }
}

fn nav_text_color(page: Page, active: Page) -> Color {
    if page == active {
        Color::rgb(48, 69, 41)
    } else {
        TEXT
    }
}

#[cfg(feature = "audio")]
fn audio_state() -> Option<AudioState> {
    crate::core::reactive::with_world(|world| {
        world
            .resource::<AudioStateSignal>()
            .map(AudioStateSignal::get)
    })
    .flatten()
}

fn audio_label() -> &'static str {
    #[cfg(feature = "audio")]
    if let Some(state) = audio_state() {
        return if state.muted {
            "SOUND"
        } else if state.output == AudioOutputState::Ready {
            "ON"
        } else {
            "WAIT"
        };
    }
    "SOUND"
}

fn audio_visible() -> bool {
    #[cfg(feature = "audio")]
    {
        audio_state().is_some()
    }
    #[cfg(not(feature = "audio"))]
    {
        false
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
fn submit_pad_sound(audio: &AudioHandle, pitch: u8, timbre: PadTimbre, gain: u8, delay_ms: u16) {
    let tone = |pitch, waveform, duration_ms, scale| {
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

#[crate::view(
    component = MarbleBoard,
    read(model),
    watch(model.visual_revision()),
    name = "MarbleBoard",
    priority = 60
)]
fn board_render(renderer: &mut dyn Renderer, model: &MarbleModel, rect: &Rect, ctx: &mut ViewCtx) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 199);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    match model.page {
        Page::Play | Page::Edit => paint_play_board(&mut painter, model, model.theme()),
        Page::Scenes => paint_scene_cards(&mut painter, model.scene),
        Page::Settings => paint_settings(&mut painter, model, model.theme()),
    }
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
    let Some(model) = world
        .get::<MarbleBoard>(entity)
        .map(|board| board.model.clone())
    else {
        return false;
    };
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
    match action {
        0 => {
            model.begin_board_drag(point);
        }
        1 => {
            model.move_board_drag(point);
        }
        2 => {
            model.end_board_drag(false);
        }
        3 => {
            model.end_board_drag(true);
        }
        _ => {
            let local_y = point.y - Fixed64::from_int(52);
            if model.page() == Page::Scenes {
                if (Fixed64::from_int(18)..=Fixed64::from_int(181)).contains(&local_y) {
                    let slot = ((point.x.to_int() - 18) / 151).clamp(0, 2) as usize;
                    model.reset_scene(slot);
                }
            } else if model.page() != Page::Settings {
                model.begin_board_drag(point);
                model.end_board_drag(false);
            }
        }
    };
    true
}

#[mirui_macros::system(order = ANIMATION, bind(model))]
fn marble_tick_system(model: &MarbleModel, delta: Option<DeltaTimeMs>) {
    model.advance_ms(delta.map_or(16, |delta| delta.0));
}

fn symmetric_padding(vertical: i32, horizontal: i32) -> Padding {
    Padding {
        top: Dimension::px(vertical),
        right: Dimension::px(horizontal),
        bottom: Dimension::px(vertical),
        left: Dimension::px(horizontal),
    }
}

#[compose(bind(model))]
fn build_widgets(model: MarbleModel, #[cfg(feature = "audio")] audio: Option<AudioHandle>) {
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
                    text: ${ if model.paused() { "PLAY" } else { "HOLD" } },
                    id: "marble_pause",
                    size: ButtonSize::Compact,
                    width: 52,
                    height: 22,
                    font_size: 9,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.toggle_pause(); }
                Button (
                    id: "marble_audio",
                    size: ButtonSize::Compact,
                    width: 50,
                    height: 22,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    text: ${ audio_label() },
                    visible: ${ audio_visible() },
                    border_radius: 6
                ) on Tap {
                    #[cfg(not(feature = "audio"))]
                    let _ = &model;
                    #[cfg(feature = "audio")]
                    if let Some(audio) = &audio {
                        let enable = audio.state().is_some_and(|state| state.muted);
                        if audio.set_muted(!enable) && enable {
                            submit_pad_sound(audio, model.selected_pitch(), model.selected_timbre(), 190, 0);
                        }
                    }
                }
                Button (
                    text: ${
                        if model.recording() { "DONE" } else if model.looping() { "STOP" } else { "REC" }
                    },
                    id: "marble_record",
                    size: ButtonSize::Compact,
                    width: 44,
                    height: 22,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(238, 172, 139),
                    text_color: TEXT,
                    visible: ${ audio_visible() },
                    border_radius: 6
                ) on Tap { model.toggle_recording(); }
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
                ) on Tap { model.drop_ball(); }
            }
            Row (
                height: 23,
                padding: symmetric_padding(3, 14),
                align: AlignItems::Center,
                column_gap: 10
            ) {
                Text (
                    text: ${ marble_status(model.page(), model.paused(), model.add_mode()) },
                    id: "marble_status",
                    grow: 1.0,
                    height: 17,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    text: ${ marble_count(model.ball_count()) },
                    id: "marble_marble_count",
                    width: 74,
                    height: 17,
                    font_size: 8,
                    text_color: Color::rgb(217, 248, 138),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    text: ${ pad_count(model.pad_count()) },
                    id: "marble_pad_count",
                    width: 52,
                    height: 17,
                    font_size: 8,
                    text_color: Color::rgb(217, 248, 138),
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
            }
            View (
                id: "marble_play_board",
                width: 480,
                height: 199,
                clip_children: true
            ) [
                MarbleBoard { model: model.clone() },
                TouchAction::None,
            ] on Tap { board_gesture(ctx.world, ctx.entity, ctx.event); } on DragStart { board_gesture(ctx.world, ctx.entity, ctx.event); } on DragMove { board_gesture(ctx.world, ctx.entity, ctx.event); } on DragEnd { board_gesture(ctx.world, ctx.entity, ctx.event); } on DragCancel { board_gesture(ctx.world, ctx.entity, ctx.event); }
            {
                Text (
                    "DAYDREAM",
                    id: "marble_scene_0_name",
                    visible: ${ model.page() == Page::Scenes },
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
                    visible: ${ model.page() == Page::Scenes },
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
                    visible: ${ model.page() == Page::Scenes },
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
                    visible: ${ model.page() == Page::Scenes },
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
                    visible: ${ model.page() == Page::Scenes },
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
                    visible: ${ model.page() == Page::Scenes },
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
                    visible: ${ model.page() == Page::Settings },
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
                    text: ${ format!("TEMPO · {} BPM", model.bpm()) },
                    id: "marble_setting_bpm",
                    visible: ${ model.page() == Page::Settings },
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
                    visible: ${ model.page() == Page::Settings },
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
                    visible: ${ model.page() == Page::Settings },
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
                    visible: ${ model.page() == Page::Settings },
                    position: Position::Absolute,
                    left: 42,
                    top: 29,
                    width: 380,
                    height: 28,
                    min: Fixed::ZERO,
                    max: Fixed::from_ratio(16, 10),
                    value: ${ model.gravity().to_fixed() },
                    track_color: Color::rgb(69, 87, 70),
                    fill_color: Color::rgb(217, 248, 138),
                    thumb_color: Color::rgb(225, 233, 214)
                ) on ValueChanged { model.set_gravity(Fixed64::from_fixed(*new)); }
                Slider (
                    id: "marble_setting_bpm_control",
                    visible: ${ model.page() == Page::Settings },
                    position: Position::Absolute,
                    left: 42,
                    top: 79,
                    width: 380,
                    height: 28,
                    min: Fixed::from_int(55),
                    max: Fixed::from_int(160),
                    value: ${ Fixed::from_int(i32::from(model.bpm())) },
                    track_color: Color::rgb(69, 87, 70),
                    fill_color: Color::rgb(198, 176, 239),
                    thumb_color: Color::rgb(225, 233, 214)
                ) on ValueChanged { model.set_bpm(Fixed64::from_fixed(*new)); }
                Switch (
                    id: "marble_setting_trails_control",
                    visible: ${ model.page() == Page::Settings },
                    position: Position::Absolute,
                    left: 370,
                    top: 121,
                    width: 54,
                    height: 24,
                    on: ${ model.trails() },
                    on_color: Color::rgb(217, 248, 138),
                    off_color: Color::rgb(69, 87, 70),
                    thumb_color: Color::rgb(23, 34, 28)
                ) on Toggled {
                    if model.trails() != *now {
                        model.toggle_trails();
                    }
                }
                Switch (
                    id: "marble_setting_feedback_control",
                    visible: ${ model.page() == Page::Settings },
                    position: Position::Absolute,
                    left: 370,
                    top: 156,
                    width: 54,
                    height: 24,
                    on: ${ model.feedback() },
                    on_color: Color::rgb(217, 248, 138),
                    off_color: Color::rgb(69, 87, 70),
                    thumb_color: Color::rgb(23, 34, 28)
                ) on Toggled {
                    if model.feedback() != *now {
                        model.toggle_feedback();
                    }
                }
            }
            Row (
                height: 35,
                padding: symmetric_padding(5, 14),
                align: AlignItems::Center,
                column_gap: 6
            ) {
                Text (
                    text: ${ marble_readout(model.page(), model.selected_letter(), model.gravity().to_fixed(), model.hits(), model.selected_radius().to_fixed(), model.selected_bounce().to_fixed()) },
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
                    visible: ${ model.page() == Page::Edit },
                    size: ButtonSize::Compact,
                    width: 52,
                    height: 23,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.set_inspector(true); }
                Button (
                    "ADD",
                    id: "marble_add",
                    visible: ${ model.page() == Page::Edit },
                    size: ButtonSize::Compact,
                    width: 46,
                    height: 23,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.toggle_add_mode(); }
                Button (
                    "REMOVE",
                    id: "marble_remove",
                    visible: ${ model.page() == Page::Edit },
                    size: ButtonSize::Compact,
                    width: 62,
                    height: 23,
                    font_size: 8,
                    normal_color: Color::rgb(34, 47, 37),
                    pressed_color: Color::rgb(238, 172, 139),
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { model.remove_selected(); }
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
                    normal_color: ${ nav_color(model.page(), Page::Play) },
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: ${ nav_text_color(model.page(), Page::Play) },
                    border_radius: 6
                ) on Tap { model.set_page(Page::Play); }
                Button (
                    "EDIT",
                    id: "marble_nav_edit",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: ${ nav_color(model.page(), Page::Edit) },
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: ${ nav_text_color(model.page(), Page::Edit) },
                    border_radius: 6
                ) on Tap { model.set_page(Page::Edit); }
                Button (
                    "SCENES",
                    id: "marble_nav_scenes",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: ${ nav_color(model.page(), Page::Scenes) },
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: ${ nav_text_color(model.page(), Page::Scenes) },
                    border_radius: 6
                ) on Tap { model.set_page(Page::Scenes); }
                Button (
                    "SETTINGS",
                    id: "marble_nav_settings",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 26,
                    font_size: 8,
                    normal_color: ${ nav_color(model.page(), Page::Settings) },
                    pressed_color: Color::rgb(217, 248, 138),
                    text_color: ${ nav_text_color(model.page(), Page::Settings) },
                    border_radius: 6
                ) on Tap { model.set_page(Page::Settings); }
            }
            View (
                id: "marble_inspector",
                visible: ${ model.inspector() },
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
                        ) on Tap { model.set_inspector(false); }
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
                        ) on Tap { model.set_color(0); }
                        Button (
                            "",
                            size: ButtonSize::Custom,
                            grow: 1.0,
                            height: 20,
                            normal_color: Color::rgb(198, 176, 239),
                            pressed_color: Color::rgb(198, 176, 239),
                            border_radius: 5
                        ) on Tap { model.set_color(1); }
                        Button (
                            "",
                            size: ButtonSize::Custom,
                            grow: 1.0,
                            height: 20,
                            normal_color: Color::rgb(238, 217, 132),
                            pressed_color: Color::rgb(238, 217, 132),
                            border_radius: 5
                        ) on Tap { model.set_color(2); }
                        Button (
                            "",
                            size: ButtonSize::Custom,
                            grow: 1.0,
                            height: 20,
                            normal_color: Color::rgb(238, 172, 139),
                            pressed_color: Color::rgb(238, 172, 139),
                            border_radius: 5
                        ) on Tap { model.set_color(3); }
                        Button (
                            "",
                            size: ButtonSize::Custom,
                            grow: 1.0,
                            height: 20,
                            normal_color: Color::rgb(160, 210, 232),
                            pressed_color: Color::rgb(160, 210, 232),
                            border_radius: 5
                        ) on Tap { model.set_color(4); }
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
                        ) on Tap { model.adjust_pitch(-1); }
                        Text (
                            text: ${ pitch_label(model.selected_pitch()) },
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
                        ) on Tap { model.adjust_pitch(1); }
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
                            text: ${ model.selected_timbre().label() },
                            id: "marble_timbre",
                            size: ButtonSize::Compact,
                            grow: 1.0,
                            height: 22,
                            font_size: 8,
                            normal_color: Color::rgb(45, 61, 48),
                            pressed_color: Color::rgb(198, 176, 239),
                            text_color: TEXT,
                            border_radius: 5
                        ) on Tap { model.cycle_timbre(); }
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
                        value: ${ model.selected_bounce().to_fixed() },
                        track_color: Color::rgb(69, 87, 70),
                        fill_color: Color::rgb(217, 248, 138),
                        thumb_color: Color::rgb(225, 233, 214)
                    ) on ValueChanged { model.set_bounce(Fixed64::from_fixed(*new)); }
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
                        value: ${ model.selected_radius().to_fixed() },
                        track_color: Color::rgb(69, 87, 70),
                        fill_color: Color::rgb(217, 248, 138),
                        thumb_color: Color::rgb(225, 233, 214)
                    ) on ValueChanged { model.set_radius(Fixed64::from_fixed(*new)); }
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
    app.with_widget(board_render::view());
    let model = app.add_model(MarbleModel::new());
    #[cfg(feature = "audio")]
    {
        let audio = app.audio();
        app.on_effect(&model, move |sound: MarbleSound| {
            if let Some(audio) = &audio {
                let MarbleSound::Pad {
                    pitch,
                    timbre,
                    gain,
                    delay_ms,
                    ..
                } = sound;
                submit_pad_sound(audio, pitch, timbre, gain, delay_ms);
            }
        })
        .expect("Marble sound consumer is registered once");
    }
    app.add_system(marble_tick_system::system(model.clone()));
    #[cfg(feature = "audio")]
    {
        let audio = app.audio();
        app.compose(parent, |cx| build_widgets(cx, model.clone(), audio.clone()));
    }
    #[cfg(not(feature = "audio"))]
    app.compose(parent, |cx| build_widgets(cx, model.clone()));
    #[cfg(feature = "audio")]
    if let Some(audio) = app.audio() {
        let _ = audio.set_master_gain(107);
        let _ = audio.set_muted(true);
    }
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 320);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gallery::play::marble::MarbleModelHandle;
    use crate::ui::view::ViewRegistry;
    use crate::ui::{Children, Hidden, IdMap, UiScope};

    fn fixture() -> World {
        let mut world = World::new();
        let mut registry = ViewRegistry::with_builtins();
        registry.insert(board_render::view());
        world.insert_resource(registry);
        world.insert_resource(IdMap::new());
        let (cell, model) = crate::core::model::register(world.id(), MarbleModel::new());
        let registration = world.spawn_empty();
        world.insert(registration, cell);
        #[cfg(feature = "audio")]
        {
            let core = crate::audio::SharedAudioCore::new(crate::audio::AudioBus::<32>::new());
            world.insert_resource(crate::audio::SharedAudioCore::handle(&core));
            world.insert_resource(core.state_signal());
            world.insert_resource(core);
        }
        let root = WidgetBuilder::new(&mut world).id();
        #[cfg(feature = "audio")]
        let audio = world.resource::<AudioHandle>().cloned();
        let mut cx = UiScope::new(&mut world, root);
        #[cfg(feature = "audio")]
        build_widgets(&mut cx, model.clone(), audio);
        #[cfg(not(feature = "audio"))]
        build_widgets(&mut cx, model.clone());
        world.insert_resource(model);
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
    fn marble_boards_share_only_their_bound_model_instance() {
        use crate::ui::dirty::VisualDirty;

        let mut app = App::headless(480, 320);
        app.with_default_widgets();
        app.with_widget(board_render::view());
        let shared = app.add_model(MarbleModel::new());
        let separate = app.add_model(MarbleModel::new());
        let board_layout = LayoutStyle {
            width: Dimension::px(150),
            height: Dimension::px(199),
            ..LayoutStyle::default()
        };
        let boards: [_; 3] =
            core::array::from_fn(|_| WidgetBuilder::new(&mut app.world).layout(board_layout).id());
        let root = WidgetBuilder::new(&mut app.world)
            .child(boards[0])
            .child(boards[1])
            .child(boards[2])
            .id();
        app.set_root(root);
        for (index, board) in boards.into_iter().enumerate() {
            app.world.insert(
                board,
                MarbleBoard {
                    model: if index == 2 {
                        separate.clone()
                    } else {
                        shared.clone()
                    },
                },
            );
        }
        ViewRegistry::reconcile_observations(&mut app.world);
        for board in boards {
            app.world.remove::<VisualDirty>(board);
        }

        shared.set_page(Page::Edit);
        crate::core::reactive::flush_signal_dirty(&mut app.world);
        assert!(app.world.has::<VisualDirty>(boards[0]));
        assert!(app.world.has::<VisualDirty>(boards[1]));
        assert!(!app.world.has::<VisualDirty>(boards[2]));
        for board in boards {
            app.world.remove::<VisualDirty>(board);
        }

        separate.set_page(Page::Scenes);
        crate::core::reactive::flush_signal_dirty(&mut app.world);
        assert!(!app.world.has::<VisualDirty>(boards[0]));
        assert!(!app.world.has::<VisualDirty>(boards[1]));
        assert!(app.world.has::<VisualDirty>(boards[2]));
    }

    #[cfg(feature = "audio")]
    #[test]
    fn installed_model_routes_pad_sound_to_the_shared_audio_core() {
        use crate::audio::AudioCommand;

        let mut app = App::headless(480, 320);
        app.with_default_widgets();
        let core = crate::audio::SharedAudioCore::new(crate::audio::AudioBus::<32>::new());
        app.world
            .insert_resource(crate::audio::SharedAudioCore::handle(&core));
        app.world.insert_resource(core.state_signal());
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        assert_eq!(core.pop(), Some(AudioCommand::SetMasterGain(107)));
        assert_eq!(core.pop(), Some(AudioCommand::SetMuted(true)));

        let board = app.world.find_by_id("marble_play_board").unwrap();
        let model = app.world.get::<MarbleBoard>(board).unwrap().model.clone();
        model.adjust_pitch(1);
        assert!(matches!(core.pop(), Some(AudioCommand::Tone(_))));
        assert!(matches!(core.pop(), Some(AudioCommand::Tone(_))));
        assert_eq!(core.pop(), None);
    }

    #[test]
    fn bound_controls_follow_page_selection_and_scene_reset() {
        let mut world = fixture();
        let properties = world.find_by_id("marble_properties").unwrap();
        let scene_label = world.find_by_id("marble_scene_0_name").unwrap();
        let gravity = world.find_by_id("marble_setting_gravity_control").unwrap();
        let bpm = world.find_by_id("marble_setting_bpm_control").unwrap();
        let pitch = world.find_by_id("marble_pitch").unwrap();
        assert!(world.has::<Hidden>(properties));
        assert!(world.has::<Hidden>(scene_label));

        world
            .resource::<MarbleModelHandle>()
            .unwrap()
            .set_page(Page::Edit);
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert!(!world.has::<Hidden>(properties));

        world
            .resource::<MarbleModelHandle>()
            .unwrap()
            .adjust_pitch(1);
        crate::core::reactive::flush_signal_dirty(&mut world);
        let selected_pitch = world
            .resource::<MarbleModelHandle>()
            .unwrap()
            .selected_pitch();
        assert_eq!(
            world.get::<Text>(pitch).unwrap().resolve(&world),
            pitch_label(selected_pitch)
        );

        world
            .resource::<MarbleModelHandle>()
            .unwrap()
            .set_page(Page::Scenes);
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert!(world.has::<Hidden>(properties));
        assert!(!world.has::<Hidden>(scene_label));

        world
            .resource::<MarbleModelHandle>()
            .unwrap()
            .reset_scene(2);
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert!(world.has::<Hidden>(scene_label));
        assert_eq!(
            world.get::<Slider>(gravity).unwrap().value,
            THEMES[2].gravity.to_fixed()
        );
        assert_eq!(
            world.get::<Slider>(bpm).unwrap().value,
            Fixed::from_int(128)
        );
    }

    #[test]
    fn bound_readout_tracks_visual_only_hit_changes() {
        let mut world = fixture();
        let readout = world.find_by_id("marble_readout").unwrap();
        crate::core::model::ModelHandle::update(
            world.resource::<MarbleModelHandle>().unwrap(),
            |model| {
                model.hits += 1;
                ChangeSet::VISUAL
            },
        );
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert!(
            world
                .get::<Text>(readout)
                .unwrap()
                .resolve(&world)
                .contains("1 HITS")
        );
    }

    #[test]
    fn bound_switches_update_without_widget_events() {
        let mut world = fixture();
        let trails = world.find_by_id("marble_setting_trails_control").unwrap();
        assert!(world.get::<Switch>(trails).unwrap().on);
        world
            .resource::<MarbleModelHandle>()
            .unwrap()
            .toggle_trails();
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert!(!world.get::<Switch>(trails).unwrap().on);
        assert!(!world.resource::<MarbleModelHandle>().unwrap().trails());
    }

    #[test]
    fn nav_tap_updates_model_and_bound_page() {
        let mut world = fixture();
        let edit = world.find_by_id("marble_nav_edit").unwrap();
        let properties = world.find_by_id("marble_properties").unwrap();
        crate::input::event::bubble_dispatch(
            &mut world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: edit,
            },
        );
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert_eq!(
            world.resource::<MarbleModelHandle>().unwrap().page(),
            Page::Edit
        );
        assert!(!world.has::<Hidden>(properties));
    }

    #[cfg(feature = "audio")]
    #[test]
    fn audio_display_tracks_shared_controls() {
        let mut world = fixture();
        let audio = world.find_by_id("marble_audio").unwrap();
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert!(world.get::<Hidden>(audio).is_none());
        assert_eq!(world.get::<Text>(audio).unwrap().resolve(&world), "WAIT");

        world
            .resource::<alloc::rc::Rc<crate::audio::SharedAudioCore<32>>>()
            .unwrap()
            .set_output_state(AudioOutputState::Ready);
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert_eq!(world.get::<Text>(audio).unwrap().resolve(&world), "ON");

        world.resource::<AudioHandle>().unwrap().set_muted(true);
        crate::core::reactive::flush_signal_dirty(&mut world);
        assert_eq!(world.get::<Text>(audio).unwrap().resolve(&world), "SOUND");
    }

    #[cfg(feature = "audio")]
    #[test]
    fn external_mute_updates_shared_state_without_playing_a_pad() {
        use crate::audio::AudioCommand;

        let mut world = fixture();
        let core = world
            .resource::<alloc::rc::Rc<crate::audio::SharedAudioCore<32>>>()
            .unwrap()
            .clone();
        world.resource::<AudioHandle>().unwrap().set_muted(true);
        assert!(
            world
                .resource::<AudioHandle>()
                .unwrap()
                .state()
                .unwrap()
                .muted
        );
        assert_eq!(core.pop(), Some(AudioCommand::SetMuted(true)));
        world.resource::<AudioHandle>().unwrap().set_muted(false);
        assert!(
            !world
                .resource::<AudioHandle>()
                .unwrap()
                .state()
                .unwrap()
                .muted
        );
        assert_eq!(core.pop(), Some(AudioCommand::SetMuted(false)));
        assert_eq!(core.pop(), None);

        let audio_button = world.find_by_id("marble_audio").unwrap();
        let tap = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: audio_button,
        };
        crate::input::event::bubble_dispatch(&mut world, &tap);
        assert_eq!(core.pop(), Some(AudioCommand::SetMuted(true)));
        crate::input::event::bubble_dispatch(&mut world, &tap);
        assert_eq!(core.pop(), Some(AudioCommand::SetMuted(false)));
        assert!(matches!(core.pop(), Some(AudioCommand::Tone(_))));
    }

    #[test]
    fn drag_cancel_reaches_transactional_model_path() {
        let mut world = fixture();
        let board = world.find_by_id("marble_play_board").unwrap();
        world.insert(board, ComputedRect(Rect::new(0, 52, 480, 199)));
        let original = crate::core::model::ModelHandle::read(
            world.resource::<MarbleModelHandle>().unwrap(),
            |model| model.selected_pad().pos,
        );
        world
            .resource::<MarbleModelHandle>()
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
            crate::core::model::ModelHandle::read(
                world.resource::<MarbleModelHandle>().unwrap(),
                |model| model.selected_pad().pos,
            ),
            original
        );
    }

    #[test]
    fn tap_selects_a_pad_without_crossing_drag_threshold() {
        let mut world = fixture();
        let board = world.find_by_id("marble_play_board").unwrap();
        world.insert(board, ComputedRect(Rect::new(0, 52, 480, 199)));
        let target = crate::core::model::ModelHandle::read(
            world.resource::<MarbleModelHandle>().unwrap(),
            |model| model.pads[1].expect("second pad").pos,
        );
        assert!(board_gesture(
            &mut world,
            board,
            &GestureEvent::Tap {
                x: target.x.to_fixed(),
                y: target.y.to_fixed(),
                target: board,
            }
        ));
        let model = world.resource::<MarbleModelHandle>().unwrap();
        assert_eq!(model.selected(), 1);
        assert!(crate::core::model::ModelHandle::read(model, |model| model
            .selected_pad()
            .pulse
            .is_positive()));
    }

    #[test]
    fn scene_card_tap_loads_selected_preset() {
        let mut world = fixture();
        let board = world.find_by_id("marble_play_board").unwrap();
        world.insert(board, ComputedRect(Rect::new(0, 52, 480, 199)));
        world
            .resource::<MarbleModelHandle>()
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
        let model = world.resource::<MarbleModelHandle>().unwrap();
        assert_eq!(model.scene(), 2);
        assert_eq!(model.page(), Page::Play);
    }
}
