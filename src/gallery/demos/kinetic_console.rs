extern crate alloc;

use alloc::vec;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::reactive::Signal;
use crate::ecs::DeltaTimeMs;
use crate::input::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::DimPoint;
use crate::ui::IgnoreHitTest;
use crate::ui::dirty::VisualDirty;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{ParagraphStyle, Slider, Text, TextAlign, TextVerticalAlign, TextWrap};

pub const VIEWPORT: (u16, u16) = (128, 128);

const BACKGROUND: Color = Color::rgb(4, 12, 20);
const PANEL: Color = Color::rgb(8, 25, 38);
const BORDER: Color = Color::rgb(24, 60, 78);
const TEXT: Color = Color::rgb(222, 242, 248);
const MUTED: Color = Color::rgb(100, 142, 158);
const CYAN: Color = Color::rgb(62, 232, 213);
const BLUE: Color = Color::rgb(75, 148, 255);
const VIOLET: Color = Color::rgb(177, 112, 255);
const AMBER: Color = Color::rgb(255, 188, 72);
const PINK: Color = Color::rgb(255, 90, 154);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ConsoleMode {
    #[default]
    Orbit,
    Flow,
    Pulse,
}

impl ConsoleMode {
    const fn accent(self) -> Color {
        match self {
            Self::Orbit => CYAN,
            Self::Flow => VIOLET,
            Self::Pulse => AMBER,
        }
    }

    const fn secondary(self) -> Color {
        match self {
            Self::Orbit => BLUE,
            Self::Flow => CYAN,
            Self::Pulse => PINK,
        }
    }

    const fn speed(self) -> i32 {
        match self {
            Self::Orbit => 34,
            Self::Flow => 48,
            Self::Pulse => 62,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ConsoleState {
    mode: ConsoleMode,
    intensity: Fixed,
    focused: u8,
    paused: bool,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self {
            mode: ConsoleMode::Orbit,
            intensity: Fixed::from_int(68),
            focused: 0,
            paused: false,
        }
    }
}

#[derive(Clone)]
struct ConsoleModel {
    mode: Signal<ConsoleMode>,
    intensity: Signal<Fixed>,
    focused: Signal<u8>,
    paused: Signal<bool>,
}

impl Default for ConsoleModel {
    fn default() -> Self {
        let state = ConsoleState::default();
        Self {
            mode: Signal::new(state.mode),
            intensity: Signal::new(state.intensity),
            focused: Signal::new(state.focused),
            paused: Signal::new(state.paused),
        }
    }
}

impl ConsoleModel {
    fn snapshot(&self) -> ConsoleState {
        ConsoleState {
            mode: self.mode.get_untracked(),
            intensity: self.intensity.get_untracked(),
            focused: self.focused.get_untracked(),
            paused: self.paused.get_untracked(),
        }
    }
}

#[derive(Clone, Copy)]
struct ConsoleMotion {
    phase: Fixed,
    rate: Fixed,
    state: ConsoleState,
    wave_elapsed_ms: u16,
}

impl Default for ConsoleMotion {
    fn default() -> Self {
        Self {
            phase: Fixed::ZERO,
            rate: Fixed::ONE,
            state: ConsoleState::default(),
            wave_elapsed_ms: 0,
        }
    }
}

struct ConsoleNodes {
    orbit: Entity,
    wave: Entity,
}

enum ConsoleAction {
    Select(ConsoleMode),
    SetIntensity(Fixed),
    CycleFocus,
    TogglePaused,
}

impl ConsoleAction {
    fn publish(self, model: &ConsoleModel) {
        match self {
            Self::Select(mode) => {
                if mode != model.mode.get_untracked() {
                    model.mode.set(mode);
                }
            }
            Self::SetIntensity(value) => {
                let current = model.intensity.get_untracked();
                let next = value.clamp(Fixed::ZERO, Fixed::from_int(100));
                if next != current {
                    model.intensity.set(next);
                }
            }
            Self::CycleFocus => model
                .focused
                .update(|focused| *focused = (*focused + 1) % 3),
            Self::TogglePaused => model.paused.update(|paused| *paused = !*paused),
        }
    }
}

#[derive(Default)]
struct KineticOrbit;

#[derive(Default)]
struct KineticWave;

struct InstrumentPainter<'a, 'ctx> {
    renderer: &'a mut dyn Renderer,
    ctx: &'a mut ViewCtx<'ctx>,
    clip: Rect,
}

impl InstrumentPainter<'_, '_> {
    fn fill(&mut self, area: Rect, color: Color, radius: Fixed, opacity: u8) {
        self.ctx.draw(
            self.renderer,
            &DrawCommand::Fill {
                area,
                transform: self.ctx.transform,
                quad: None,
                color,
                radius,
                opa: opacity,
            },
            &self.clip,
        );
    }
}

fn orbit_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if !world.has::<KineticOrbit>(entity) {
        return;
    }
    let Some(model) = world.resource::<ConsoleModel>() else {
        return;
    };
    let state = model.snapshot();
    let phase = world
        .resource::<ConsoleMotion>()
        .map_or(Fixed::ZERO, |motion| motion.phase);
    let accent = state.mode.accent();
    let secondary = state.mode.secondary();

    ctx.bg_handled = true;
    let clip = *ctx.clip;
    let mut painter = InstrumentPainter {
        renderer,
        ctx,
        clip,
    };
    painter.fill(*rect, PANEL, Fixed::ZERO, 255);

    let grid = Color::rgb(13, 45, 62);
    for step in 1..4 {
        let x = rect.x + rect.w * Fixed::from_ratio(step, 4);
        painter.fill(
            Rect::new(
                x,
                rect.y + Fixed::ONE,
                Fixed::ONE,
                rect.h - Fixed::from_int(2),
            ),
            grid,
            Fixed::ZERO,
            130,
        );
    }
    for step in 1..3 {
        let y = rect.y + rect.h * Fixed::from_ratio(step, 3);
        painter.fill(
            Rect::new(
                rect.x + Fixed::ONE,
                y,
                rect.w - Fixed::from_int(2),
                Fixed::ONE,
            ),
            grid,
            Fixed::ZERO,
            130,
        );
    }

    let center = Point {
        x: rect.x + rect.w / Fixed::from_int(2),
        y: rect.y + rect.h * Fixed::from_ratio(9, 20),
    };
    let base_radius = rect.w.min(rect.h) * Fixed::from_ratio(7, 20);
    for ring in 0..3 {
        let radius = base_radius - Fixed::from_int(ring * 5);
        let node_angle = phase * Fixed::from_int(ring + 2) + Fixed::from_int(ring * 73);
        for trail in 1..=4 {
            let angle = node_angle - Fixed::from_int(trail * (8 + ring * 2));
            let point = Point {
                x: center.x + Fixed::cos_deg(angle) * radius,
                y: center.y + Fixed::sin_deg(angle) * radius,
            };
            let size = if ring == state.focused as i32 && trail <= 2 {
                Fixed::from_int(2)
            } else {
                Fixed::ONE
            };
            painter.fill(
                Rect::new(point.x - size / 2, point.y - size / 2, size, size),
                if ring == state.focused as i32 {
                    accent
                } else {
                    secondary
                },
                Fixed::ZERO,
                230u8.saturating_sub((trail * 27) as u8),
            );
        }
        let node = Point {
            x: center.x + Fixed::cos_deg(node_angle) * radius,
            y: center.y + Fixed::sin_deg(node_angle) * radius,
        };
        let size = if ring == state.focused as i32 { 5 } else { 3 };
        painter.fill(
            Rect::new(
                node.x - Fixed::from_ratio(size, 2),
                node.y - Fixed::from_ratio(size, 2),
                Fixed::from_int(size),
                Fixed::from_int(size),
            ),
            if ring == state.focused as i32 {
                TEXT
            } else {
                secondary
            },
            Fixed::from_int(size),
            255,
        );
    }

    for size in [13, 8, 3] {
        let color = if size == 13 {
            Color::rgba(accent.r, accent.g, accent.b, 70)
        } else if size == 8 {
            secondary
        } else {
            TEXT
        };
        painter.fill(
            Rect::new(
                center.x - Fixed::from_ratio(size, 2),
                center.y - Fixed::from_ratio(size, 2),
                Fixed::from_int(size),
                Fixed::from_int(size),
            ),
            color,
            Fixed::from_int(size),
            255,
        );
    }
}

fn wave_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if !world.has::<KineticWave>(entity) {
        return;
    }
    let Some(model) = world.resource::<ConsoleModel>() else {
        return;
    };
    let state = model.snapshot();
    let phase = world
        .resource::<ConsoleMotion>()
        .map_or(Fixed::ZERO, |motion| motion.phase);
    let accent = state.mode.accent();
    ctx.bg_handled = true;
    let clip = *ctx.clip;
    let mut painter = InstrumentPainter {
        renderer,
        ctx,
        clip,
    };
    painter.fill(*rect, PANEL, Fixed::ZERO, 255);

    let plot_inset = Fixed::from_int(5);
    let bar_width = Fixed::from_int(2);
    let plot_y = rect.y + rect.h - plot_inset;
    let plot_span = rect.w - plot_inset * Fixed::from_int(2) - bar_width;
    for bar in 0..13 {
        let wave = Fixed::sin_deg(phase * Fixed::from_int(2) + Fixed::from_int(bar * 37));
        let height = Fixed::from_int(2)
            + wave.abs() * Fixed::from_int(7) * state.intensity / Fixed::from_int(100);
        let x = rect.x + plot_inset + plot_span * Fixed::from_ratio(bar, 12);
        painter.fill(
            Rect::new(x, plot_y - height, bar_width, height),
            if bar % 3 == state.focused as i32 {
                accent
            } else {
                MUTED
            },
            Fixed::ONE,
            220,
        );
    }
}

fn orbit_view() -> View {
    View::new("KineticOrbit", 60, orbit_render).with_filter::<KineticOrbit>()
}

fn wave_view() -> View {
    View::new("KineticWave", 61, wave_render).with_filter::<KineticWave>()
}

#[mirui_macros::system(order = ANIMATION)]
pub fn kinetic_animation_system(world: &mut World) {
    const MAX_STEP_MS: u16 = 50;
    const RATE_RAMP_MS: i32 = 450;

    let Some(state) = world.resource::<ConsoleModel>().map(ConsoleModel::snapshot) else {
        return;
    };
    let dt = world
        .resource::<DeltaTimeMs>()
        .map_or(16, |delta| delta.0)
        .min(MAX_STEP_MS);
    let Some(motion) = world.resource_mut::<ConsoleMotion>() else {
        return;
    };
    let controls_changed = motion.state.mode != state.mode || motion.state.focused != state.focused;
    let intensity_changed = motion.state.intensity != state.intensity;
    motion.state = state;
    let previous_rate = motion.rate;
    let rate_step = Fixed::from_ratio(i32::from(dt), RATE_RAMP_MS);
    motion.rate = if state.paused {
        (motion.rate - rate_step).max(Fixed::ZERO)
    } else {
        (motion.rate + rate_step).min(Fixed::ONE)
    };
    let moving = previous_rate > Fixed::ZERO || motion.rate > Fixed::ZERO;
    if moving {
        let speed = Fixed::from_int(state.mode.speed()) + state.intensity * Fixed::from_ratio(3, 5);
        let average_rate = (previous_rate + motion.rate) / Fixed::from_int(2);
        let delta = speed * average_rate * Fixed::from_int(i32::from(dt)) / Fixed::from_int(1000);
        if delta > Fixed::ZERO {
            motion.phase += delta;
            while motion.phase >= Fixed::from_int(360) {
                motion.phase -= Fixed::from_int(360);
            }
            motion.wave_elapsed_ms = motion.wave_elapsed_ms.saturating_add(dt);
        }
    }
    let orbit_dirty = controls_changed || moving;
    let wave_dirty = controls_changed || intensity_changed || motion.wave_elapsed_ms >= 64;
    if wave_dirty {
        motion.wave_elapsed_ms = 0;
    }
    if let Some(nodes) = world
        .resource::<ConsoleNodes>()
        .map(|nodes| (nodes.orbit, nodes.wave))
    {
        if orbit_dirty {
            world.insert(nodes.0, VisualDirty);
        }
        if wave_dirty {
            world.insert(nodes.1, VisualDirty);
        }
    }
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

fn header_label() -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        vertical_align: TextVerticalAlign::Center,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

fn console_model(cx: &mut crate::ui::UiScope<'_>) -> ConsoleModel {
    cx.world_mut()
        .resource::<ConsoleModel>()
        .cloned()
        .expect("Kinetic Console model")
}

#[compose]
pub fn build_widgets() {
    let model = console_model(cx);
    let status_text = model.paused.clone();
    let status_bg = model.paused.clone();
    let status_action = model.clone();
    let stage_action = model.clone();
    let orbit_bg = model.mode.clone();
    let orbit_fg = model.mode.clone();
    let orbit_action = model.clone();
    let flow_bg = model.mode.clone();
    let flow_fg = model.mode.clone();
    let flow_action = model.clone();
    let pulse_bg = model.mode.clone();
    let pulse_fg = model.mode.clone();
    let pulse_action = model.clone();
    let slider_action = model;

    ui! {
        Column (
            id: "kinetic_console_shell",
            grow: 1.0,
            padding: Padding::all(5),
            row_gap: 4,
            bg_color: BACKGROUND
        ) [
            IgnoreHitTest,
        ] {
            Row (height: 18, align: AlignItems::Center, column_gap: 4) {
                View (width: 4, height: 4, bg_color: CYAN, border_radius: 2) [
                    IgnoreHitTest,
                ]
                Text (
                    "MIRUI",
                    grow: 1.0,
                    height: 18,
                    font_size: 8,
                    text_color: TEXT,
                    paragraph: header_label()
                ) [
                    IgnoreHitTest,
                ]
                View (
                    id: "kinetic_console_status",
                    width: 38,
                    height: 18,
                    padding: Padding::all(2),
                    bg_color: ${ if status_bg.get() { Color::rgb(64, 38, 46) } else { Color::rgb(10, 64, 59) } },
                    border_color: CYAN,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { ConsoleAction::TogglePaused.publish(&status_action); }
                {
                    Text (
                        text: ${ if status_text.get() { "HOLD" } else { "LIVE" } },
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: TEXT,
                        paragraph: centered_label()
                    ) [
                        IgnoreHitTest,
                    ]
                }
            }
            Column (
                id: "kinetic_console_instrument",
                grow: 1.0,
                min_height: 58,
                bg_color: PANEL,
                clip_children: true
            ) [
                IgnoreHitTest,
            ] {
                Row (
                    grow: 1.0,
                    justify: JustifyContent::Center,
                    align: AlignItems::Stretch
                ) [
                    IgnoreHitTest,
                ] {
                    KineticOrbit (
                        id: "kinetic_console_orbit_layer",
                        width: 52,
                        height: Dimension::percent(100)
                    ) on Tap { ConsoleAction::CycleFocus.publish(&stage_action); }
                }
                KineticWave (
                    id: "kinetic_console_wave_layer",
                    width: Dimension::percent(100),
                    height: 14
                ) [
                    IgnoreHitTest,
                ]
            }
            Row (height: 16, column_gap: 3) {
                View (
                    id: "kinetic_console_orbit",
                    grow: 1.0,
                    height: 16,
                    padding: Padding::all(2),
                    bg_color: ${ if orbit_bg.get() == ConsoleMode::Orbit { CYAN } else { PANEL } },
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { ConsoleAction::Select(ConsoleMode::Orbit).publish(&orbit_action); }
                {
                    Text (
                        "ORB",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ if orbit_fg.get() == ConsoleMode::Orbit { BACKGROUND } else { MUTED } },
                        paragraph: centered_label()
                    ) [
                        IgnoreHitTest,
                    ]
                }
                View (
                    id: "kinetic_console_flow",
                    grow: 1.0,
                    height: 16,
                    padding: Padding::all(2),
                    bg_color: ${ if flow_bg.get() == ConsoleMode::Flow { VIOLET } else { PANEL } },
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { ConsoleAction::Select(ConsoleMode::Flow).publish(&flow_action); }
                {
                    Text (
                        "FLOW",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ if flow_fg.get() == ConsoleMode::Flow { TEXT } else { MUTED } },
                        paragraph: centered_label()
                    ) [
                        IgnoreHitTest,
                    ]
                }
                View (
                    id: "kinetic_console_pulse",
                    grow: 1.0,
                    height: 16,
                    padding: Padding::all(2),
                    bg_color: ${ if pulse_bg.get() == ConsoleMode::Pulse { AMBER } else { PANEL } },
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { ConsoleAction::Select(ConsoleMode::Pulse).publish(&pulse_action); }
                {
                    Text (
                        "PLS",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ if pulse_fg.get() == ConsoleMode::Pulse { BACKGROUND } else { MUTED } },
                        paragraph: centered_label()
                    ) [
                        IgnoreHitTest,
                    ]
                }
            }
            Slider (
                id: "kinetic_console_intensity",
                height: 10
            ) on ValueChanged {
                let _ = old;
                ConsoleAction::SetIntensity(*new).publish(&slider_action);
            }
        }
    };

    let slider = cx
        .world_mut()
        .find_by_id("kinetic_console_intensity")
        .expect("Kinetic Console slider");
    if let Some(control) = cx.world_mut().get_mut::<Slider>(slider) {
        control.min = Fixed::ZERO;
        control.max = Fixed::from_int(100);
        control.value = Fixed::from_int(68);
        control.track_color = Color::rgb(18, 48, 64).into();
        control.fill_color = CYAN.into();
        control.thumb_color = TEXT.into();
    }
}

pub fn build_sim_timeline(world: &World) -> Option<SimTimeline> {
    let orbit = world.find_by_id("kinetic_console_orbit")?;
    let flow = world.find_by_id("kinetic_console_flow")?;
    let pulse = world.find_by_id("kinetic_console_pulse")?;
    let orbit_layer = world.find_by_id("kinetic_console_orbit_layer")?;
    let status = world.find_by_id("kinetic_console_status")?;
    let slider = world.find_by_id("kinetic_console_intensity")?;

    Some(
        SimTimeline::new(vec![
            SimAction::wait(900),
            SimAction::tap(DimPoint::CENTER).on(flow),
            SimAction::wait(700),
            SimAction::drag(
                DimPoint::percent(20, 50),
                DimPoint::percent(88, 50),
                900,
                crate::anim::ease::ease_in_out_cubic,
            )
            .on(slider),
            SimAction::wait(700),
            SimAction::tap(DimPoint::CENTER).on(orbit_layer),
            SimAction::wait(700),
            SimAction::tap(DimPoint::CENTER).on(pulse),
            SimAction::wait(900),
            SimAction::tap(DimPoint::CENTER).on(status),
            SimAction::wait(2_800),
            SimAction::tap(DimPoint::CENTER).on(status),
            SimAction::wait(500),
            SimAction::tap(DimPoint::CENTER).on(orbit_layer),
            SimAction::wait(700),
            SimAction::tap(DimPoint::CENTER).on(orbit),
            SimAction::wait(700),
            SimAction::drag(
                DimPoint::percent(88, 50),
                DimPoint::percent(32, 50),
                800,
                crate::anim::ease::ease_in_out_cubic,
            )
            .on(slider),
            SimAction::wait(900),
        ])
        .looping(true),
    )
}

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity, autoplay: bool)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(orbit_view()).with_widget(wave_view());
    app.world.insert_resource(ConsoleModel::default());
    app.world.insert_resource(ConsoleMotion::default());
    app.add_system(kinetic_animation_system::system());
    app.compose(parent, build_widgets);

    let orbit = app
        .world
        .find_by_id("kinetic_console_orbit_layer")
        .expect("Kinetic Console orbit layer");
    let wave = app
        .world
        .find_by_id("kinetic_console_wave_layer")
        .expect("Kinetic Console wave layer");
    app.world.insert_resource(ConsoleNodes { orbit, wave });
    if autoplay && let Some(timeline) = build_sim_timeline(&app.world) {
        app.world.insert_resource(timeline);
        app.add_system(sim_timeline_system::system());
    }
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin);
    install(app, parent, true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::IdMap;
    use crate::ui::view::ViewRegistry;

    fn fixture() -> World {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(ConsoleModel::default());
        world.insert_resource(ConsoleMotion::default());
        let mut views = ViewRegistry::with_builtins();
        views.insert(orbit_view());
        views.insert(wave_view());
        world.insert_resource(views);
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = crate::ui::UiScope::new(&mut world, parent);
        build_widgets(&mut cx);
        drop(cx);
        world
    }

    #[test]
    fn actions_publish_only_changed_control_state() {
        let model = ConsoleModel::default();
        ConsoleAction::Select(ConsoleMode::Flow).publish(&model);
        ConsoleAction::SetIntensity(Fixed::from_int(120)).publish(&model);
        ConsoleAction::CycleFocus.publish(&model);
        ConsoleAction::TogglePaused.publish(&model);
        assert_eq!(
            model.snapshot(),
            ConsoleState {
                mode: ConsoleMode::Flow,
                intensity: Fixed::from_int(100),
                focused: 1,
                paused: true,
            }
        );
    }

    #[test]
    fn animation_uses_delta_and_honors_pause() {
        let mut world = World::new();
        world.insert_resource(ConsoleModel::default());
        world.insert_resource(ConsoleMotion::default());
        world.insert_resource(DeltaTimeMs(100));
        let orbit = world.spawn_empty();
        let wave = world.spawn_empty();
        world.insert_resource(ConsoleNodes { orbit, wave });

        kinetic_animation_system(&mut world);
        let phase = world.resource::<ConsoleMotion>().unwrap().phase;
        assert!(phase > Fixed::ZERO);
        assert!(world.get::<VisualDirty>(orbit).is_some());
        assert!(world.get::<VisualDirty>(wave).is_none());
        world.remove::<VisualDirty>(orbit);

        kinetic_animation_system(&mut world);
        assert!(world.get::<VisualDirty>(orbit).is_some());
        assert!(world.get::<VisualDirty>(wave).is_some());
        world.remove::<VisualDirty>(orbit);
        world.remove::<VisualDirty>(wave);

        let model = world.resource::<ConsoleModel>().unwrap().clone();
        ConsoleAction::TogglePaused.publish(&model);
        kinetic_animation_system(&mut world);
        let braking = *world.resource::<ConsoleMotion>().unwrap();
        assert!(braking.phase > phase);
        assert!(braking.rate > Fixed::ZERO && braking.rate < Fixed::ONE);
        assert!(world.get::<VisualDirty>(orbit).is_some());
        world.remove::<VisualDirty>(orbit);

        for _ in 0..9 {
            kinetic_animation_system(&mut world);
            world.remove::<VisualDirty>(orbit);
            world.remove::<VisualDirty>(wave);
        }
        let stopped = *world.resource::<ConsoleMotion>().unwrap();
        assert_eq!(stopped.rate, Fixed::ZERO);

        world.insert_resource(DeltaTimeMs(5_000));
        kinetic_animation_system(&mut world);
        assert_eq!(
            world.resource::<ConsoleMotion>().unwrap().phase,
            stopped.phase
        );
        assert!(world.get::<VisualDirty>(wave).is_none());
        assert!(world.get::<VisualDirty>(orbit).is_none());

        ConsoleAction::TogglePaused.publish(&model);
        kinetic_animation_system(&mut world);
        let resuming = *world.resource::<ConsoleMotion>().unwrap();
        assert!(resuming.rate > Fixed::ZERO && resuming.rate < Fixed::ONE);
        assert!(resuming.phase > stopped.phase);
        assert!(world.get::<VisualDirty>(orbit).is_some());
    }

    #[test]
    fn intensity_changes_do_not_starve_orbit_animation() {
        let mut world = World::new();
        world.insert_resource(ConsoleModel::default());
        world.insert_resource(ConsoleMotion::default());
        world.insert_resource(DeltaTimeMs(16));
        let orbit = world.spawn_empty();
        let wave = world.spawn_empty();
        world.insert_resource(ConsoleNodes { orbit, wave });
        let model = world.resource::<ConsoleModel>().unwrap().clone();

        ConsoleAction::SetIntensity(Fixed::from_int(82)).publish(&model);
        kinetic_animation_system(&mut world);

        assert!(world.get::<VisualDirty>(orbit).is_some());
        assert!(world.get::<VisualDirty>(wave).is_some());
    }

    #[test]
    fn composition_exposes_every_timeline_target_by_id() {
        let world = fixture();
        for id in [
            "kinetic_console_shell",
            "kinetic_console_status",
            "kinetic_console_instrument",
            "kinetic_console_orbit_layer",
            "kinetic_console_wave_layer",
            "kinetic_console_orbit",
            "kinetic_console_flow",
            "kinetic_console_pulse",
            "kinetic_console_intensity",
        ] {
            assert!(world.find_by_id(id).is_some(), "missing {id}");
        }
        assert!(build_sim_timeline(&world).is_some_and(|timeline| timeline.total_ms >= 10_000));
    }
}
