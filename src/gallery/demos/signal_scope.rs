//! Interactive dual-channel signal scope.

use crate::app::plugins::StdInstantClockPlugin;
use crate::core::reactive::Signal;
use crate::ecs::DeltaTimeMs;
use crate::gallery::demos::instruments::{InstrumentPainter, register_fonts};
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::render::texture::MirxTextureOptions;
use crate::ui::IgnoreHitTest;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Image, ParagraphStyle, Text};

pub const VIEWPORT: (u16, u16) = (800, 480);

const HOUSING_ART: &[u8] = include_bytes!("assets/product/signal-scope-housing.mirx");

const BG: ColorToken = ColorToken::Surface;
const ICE: Color = Color::rgb(222, 231, 237);
const GRID: Color = Color::rgb(46, 70, 76);
const CHANNEL_A: Color = Color::rgb(91, 255, 220);
const CHANNEL_B: Color = Color::rgb(255, 196, 68);
const TRIGGER: Color = Color::rgb(166, 124, 255);

const fn housing_texture_options() -> MirxTextureOptions {
    MirxTextureOptions::new()
        .with_limits(mirx::reader::PayloadLimits::EMBEDDED.with_max_decoded_bytes(800 * 480 * 2))
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum WaveformKind {
    #[default]
    Sine,
    Square,
    Mixed,
}

impl WaveformKind {
    const fn next(self) -> Self {
        match self {
            Self::Sine => Self::Square,
            Self::Square => Self::Mixed,
            Self::Mixed => Self::Sine,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum TriggerEdge {
    #[default]
    Rising,
    Falling,
}

impl TriggerEdge {
    const fn toggled(self) -> Self {
        match self {
            Self::Rising => Self::Falling,
            Self::Falling => Self::Rising,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScopeState {
    running: bool,
    channel_b: bool,
    time_scale: u8,
    gain_a: u8,
    trigger_level: Fixed,
    trigger_edge: TriggerEdge,
    waveform: WaveformKind,
    phase: Fixed,
    revision: u32,
}

impl Default for ScopeState {
    fn default() -> Self {
        Self {
            running: true,
            channel_b: true,
            time_scale: 2,
            gain_a: 2,
            trigger_level: Fixed::from_ratio(3, 10),
            trigger_edge: TriggerEdge::Rising,
            waveform: WaveformKind::Sine,
            phase: Fixed::ZERO,
            revision: 0,
        }
    }
}

impl ScopeState {
    fn sample(self, channel: u8, unit_x: Fixed) -> Fixed {
        let angle = unit_x * Fixed::from_int(360) * Fixed::from_int(i32::from(self.time_scale + 4))
            + self.phase
            + Fixed::from_int(i32::from(channel) * 74);
        let sample = match (channel, self.waveform) {
            (1, _) | (_, WaveformKind::Square) => {
                if Fixed::sin_deg(angle) >= Fixed::ZERO {
                    Fixed::from_ratio(4, 5)
                } else {
                    Fixed::from_ratio(-4, 5)
                }
            }
            (_, WaveformKind::Mixed) => {
                Fixed::sin_deg(angle) * Fixed::from_ratio(3, 5)
                    + Fixed::sin_deg(angle * Fixed::from_int(3)) * Fixed::from_ratio(1, 4)
            }
            _ => Fixed::sin_deg(angle),
        };
        let gain = if channel == 0 { self.gain_a } else { 1 };
        sample * Fixed::from_ratio(i32::from(gain + 1), 3)
    }

    fn tick(&mut self, delta_ms: u16) -> bool {
        if !self.running || delta_ms == 0 {
            return false;
        }
        self.phase += Fixed::from_ratio(i32::from(delta_ms), 3);
        if self.phase >= Fixed::from_int(360) {
            self.phase -= Fixed::from_int(360);
        }
        self.revision = self.revision.wrapping_add(1);
        true
    }
}

#[derive(Clone)]
struct ScopeModel {
    state: Signal<ScopeState>,
}

impl Default for ScopeModel {
    fn default() -> Self {
        Self {
            state: Signal::new(ScopeState::default()),
        }
    }
}

impl ScopeModel {
    fn snapshot(&self) -> ScopeState {
        self.state.get_untracked()
    }

    fn update(&self, update: impl FnOnce(&mut ScopeState)) {
        let mut state = self.snapshot();
        update(&mut state);
        state.revision = state.revision.wrapping_add(1);
        self.state.set(state);
    }
}

#[derive(Clone, Copy)]
enum ScopeAction {
    ToggleRun,
    ToggleChannelB,
    NextWaveform,
    ToggleEdge,
    TimeScale,
    GainA,
}

impl ScopeAction {
    fn publish(self, model: &ScopeModel) {
        model.update(|state| match self {
            Self::ToggleRun => state.running = !state.running,
            Self::ToggleChannelB => state.channel_b = !state.channel_b,
            Self::NextWaveform => state.waveform = state.waveform.next(),
            Self::ToggleEdge => state.trigger_edge = state.trigger_edge.toggled(),
            Self::TimeScale => state.time_scale = (state.time_scale + 1) % 4,
            Self::GainA => state.gain_a = (state.gain_a + 1) % 4,
        });
    }
}

#[derive(Default)]
struct ScopeCanvas;

fn scope_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if !world.has::<ScopeCanvas>(entity) {
        return;
    }
    let Some(model) = world.resource::<ScopeModel>() else {
        return;
    };
    let state = model.snapshot();
    let mut painter = InstrumentPainter::new(renderer, *ctx.clip, ctx.transform);
    let scale = rect.w / Fixed::from_int(800);
    let plot = Rect::new(
        rect.x + rect.w * Fixed::from_ratio(3, 100),
        rect.y + rect.h * Fixed::from_ratio(5, 100),
        rect.w * Fixed::from_ratio(94, 100),
        rect.h * Fixed::from_ratio(68, 100),
    );
    for column in 0..=10 {
        let x = plot.x + plot.w * Fixed::from_ratio(column, 10);
        painter.line(
            Point::new(x, plot.y),
            Point::new(x, plot.y + plot.h),
            GRID,
            scale.max(Fixed::from_ratio(1, 2)),
            if column == 5 { 150 } else { 80 },
        );
    }
    for row in 0..=8 {
        let y = plot.y + plot.h * Fixed::from_ratio(row, 8);
        painter.line(
            Point::new(plot.x, y),
            Point::new(plot.x + plot.w, y),
            GRID,
            scale.max(Fixed::from_ratio(1, 2)),
            if row == 4 { 150 } else { 80 },
        );
    }

    let samples = plot.w.to_int().clamp(96, 320) as usize;
    for channel in 0..=u8::from(state.channel_b) {
        let center = if channel == 0 {
            plot.y + plot.h * Fixed::from_ratio(31, 100)
        } else {
            plot.y + plot.h * Fixed::from_ratio(69, 100)
        };
        let amplitude = plot.h * Fixed::from_ratio(if channel == 0 { 18 } else { 13 }, 100);
        let mut previous = None;
        for index in 0..=samples {
            let unit_x = Fixed::from_ratio(index as i32, samples as i32);
            let point = Point::new(
                plot.x + plot.w * unit_x,
                center - state.sample(channel, unit_x) * amplitude,
            );
            if let Some(start) = previous {
                painter.line(
                    start,
                    point,
                    if channel == 0 { CHANNEL_A } else { CHANNEL_B },
                    Fixed::from_int(2) * scale,
                    245,
                );
            }
            previous = Some(point);
        }
    }

    let trigger_x = plot.x + plot.w / Fixed::from_int(2);
    painter.line(
        Point::new(trigger_x, plot.y),
        Point::new(trigger_x, plot.y + plot.h),
        TRIGGER,
        scale.max(Fixed::ONE),
        180,
    );
    painter.dot(
        Point::new(trigger_x, plot.y),
        Fixed::from_int(3) * scale,
        TRIGGER,
        if state.running { 220 } else { 100 },
    );
    ctx.record(painter.finish());
}

fn scope_view() -> View {
    View::new("ScopeCanvas", 60, scope_render).with_filter::<ScopeCanvas>()
}

fn model(cx: &mut crate::ui::UiScope<'_>) -> ScopeModel {
    cx.world_mut()
        .resource::<ScopeModel>()
        .cloned()
        .expect("Signal Scope model")
}

fn centered() -> ParagraphStyle {
    ParagraphStyle::label()
}

#[derive(Clone, Copy)]
enum ScopeReadout {
    Acquire,
    Wave,
    ChannelA,
    ChannelB,
    Time,
    Trigger,
}

impl ScopeReadout {
    fn text(self, state: ScopeState) -> &'static str {
        match self {
            Self::Acquire => {
                if state.running {
                    "RUN"
                } else {
                    "HOLD"
                }
            }
            Self::Wave => match state.waveform {
                WaveformKind::Sine => "SINE",
                WaveformKind::Square => "SQUARE",
                WaveformKind::Mixed => "MIX",
            },
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
    left: i32,
    width: i32,
    readout: ScopeReadout,
    accent: Color,
}

#[derive(Clone, Copy)]
struct ChannelControlSpec {
    left: i32,
    label: &'static str,
    readout: ScopeReadout,
    accent: Color,
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
            left: Dimension::percent(spec.left), top: Dimension::percent(79),
            width: Dimension::percent(spec.width), height: Dimension::percent(16),
            border_radius: 5
        ) on Tap { action.publish(&model); }
        {
            Text (
                text: ${ spec.readout.text(value.get()) },
                position: Position::Absolute,
                left: 0, top: 0,
                width: Dimension::percent(100), height: Dimension::percent(100),
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(62)).to_int().clamp(7, 13) as u16
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
            left: Dimension::percent(spec.left), top: Dimension::percent(79),
            width: Dimension::percent(21), height: Dimension::percent(16),
            border_radius: 5
        ) on Tap { action.publish(&model); }
        {
            Text (
                spec.label,
                position: Position::Absolute,
                left: 0, top: 0,
                width: Dimension::percent(32), height: Dimension::percent(100),
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(74)).to_int().clamp(7, 11) as u16
                },
                text_color: spec.accent,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                text: ${ spec.readout.text(value.get()) },
                position: Position::Absolute,
                left: Dimension::percent(32), top: 0,
                width: Dimension::percent(45), height: Dimension::percent(100),
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(65)).to_int().clamp(7, 13) as u16
                },
                text_color: ICE,
                paragraph: centered()
            ) [IgnoreHitTest]
        }
    }
}

#[compose]
fn compose_step_control(
    left: i32,
    width: i32,
    readout: ScopeReadout,
    value: Signal<ScopeState>,
    action: ScopeAction,
    model: ScopeModel,
) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: Dimension::percent(left), top: Dimension::percent(79),
            width: Dimension::percent(width), height: Dimension::percent(16),
            border_radius: 5
        ) on Tap { action.publish(&model); }
        {
            Text (
                text: ${ readout.text(value.get()) },
                position: Position::Absolute,
                left: 0, top: 0,
                width: Dimension::percent(69), height: Dimension::percent(100),
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(65)).to_int().clamp(7, 13) as u16
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
    let run_action = model.clone();
    let wave_action = model.clone();
    let gain_a_action = model.clone();
    let channel_b_action = model.clone();
    let time_action = model.clone();
    let trigger_action = model;
    let toggle_run = ScopeAction::ToggleRun;
    let next_waveform = ScopeAction::NextWaveform;
    let gain_a = ScopeAction::GainA;
    let toggle_channel_b = ScopeAction::ToggleChannelB;
    let time_scale = ScopeAction::TimeScale;
    let toggle_edge = ScopeAction::ToggleEdge;
    let acquire_control = ButtonControlSpec {
        left: 2,
        width: 10,
        readout: ScopeReadout::Acquire,
        accent: CHANNEL_A,
    };
    let wave_control = ButtonControlSpec {
        left: 13,
        width: 9,
        readout: ScopeReadout::Wave,
        accent: ICE,
    };
    let channel_a_control = ChannelControlSpec {
        left: 24,
        label: "CH A",
        readout: ScopeReadout::ChannelA,
        accent: CHANNEL_A,
    };
    let channel_b_control = ChannelControlSpec {
        left: 46,
        label: "CH B",
        readout: ScopeReadout::ChannelB,
        accent: CHANNEL_B,
    };
    let time_readout = ScopeReadout::Time;
    let trigger_readout = ScopeReadout::Trigger;
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
                left: Dimension::percent(2), top: 0,
                width: Dimension::percent(22), height: Dimension::percent(7),
                font: FontToken::Heading,
                font_size: @id(scope_shell).width {
                    (scope_shell.width / Fixed::from_int(58)).to_int().clamp(8, 15) as u16
                },
                text_color: ICE,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                "CH A",
                position: Position::Absolute,
                left: Dimension::percent(1), top: Dimension::percent(27),
                width: Dimension::percent(6), height: Dimension::percent(7),
                font_size: @id(scope_shell).width { (scope_shell.width / Fixed::from_int(90)).to_int().clamp(6, 10) as u16 },
                text_color: CHANNEL_A,
                paragraph: centered()
            ) [IgnoreHitTest]
            Text (
                "CH B",
                position: Position::Absolute,
                left: Dimension::percent(1), top: Dimension::percent(55),
                width: Dimension::percent(6), height: Dimension::percent(7),
                font_size: @id(scope_shell).width { (scope_shell.width / Fixed::from_int(90)).to_int().clamp(6, 10) as u16 },
                text_color: CHANNEL_B,
                paragraph: centered()
            ) [IgnoreHitTest]
            compose_button_control (acquire_control, controls_state.clone(), toggle_run, run_action)
            compose_button_control (wave_control, controls_state.clone(), next_waveform, wave_action)
            compose_channel_control (channel_a_control, controls_state.clone(), gain_a, gain_a_action)
            compose_channel_control (channel_b_control, controls_state.clone(), toggle_channel_b, channel_b_action)
            compose_step_control (68, 16, time_readout, controls_state.clone(), time_scale, time_action)
            compose_step_control (85, 13, trigger_readout, controls_state, toggle_edge, trigger_action)
        }
    };
}

#[mirui_macros::system(order = ANIMATION)]
fn scope_animation_system(world: &mut World) {
    let delta = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    let Some(model) = world.resource::<ScopeModel>().cloned() else {
        return;
    };
    let mut state = model.snapshot();
    if state.tick(delta) {
        model.state.set(state);
    }
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    register_fonts(&mut app.world);
    app.world.insert_resource(ScopeModel::default());
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(
            crate::app::plugins::ImageResourcesPlugin::empty().with_mirx_bytes_options(
                "signal_scope_housing",
                HOUSING_ART,
                housing_texture_options(),
            ),
        )
        .with_widget(scope_view())
        .add_system(scope_animation_system::system());
    app.compose(parent, build_widgets);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::texture::{TexBuf, Texture};
    use crate::ui::ComputedRect;

    fn assert_layout(width: u16, height: u16) {
        let mut app = App::headless(width, height);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        app.render().unwrap();

        super::super::assert_text_layouts_fit(&app.world);
        let canvas = app.world.find_by_id("scope_canvas").unwrap();
        let rect = app.world.get::<ComputedRect>(canvas).unwrap().0;
        assert!(rect.w > Fixed::from_int(100), "{width}x{height}: {rect:?}");
        assert!(rect.h >= Fixed::from_int(150), "{width}x{height}: {rect:?}");
    }

    #[test]
    fn held_scope_keeps_phase_stable() {
        let mut state = ScopeState::default();
        state.running = false;
        let phase = state.phase;
        assert!(!state.tick(16));
        assert_eq!(state.phase, phase);
    }

    #[test]
    fn waveform_and_trigger_cycles_are_closed() {
        assert_eq!(WaveformKind::Sine.next().next().next(), WaveformKind::Sine);
        assert_eq!(TriggerEdge::Rising.toggled().toggled(), TriggerEdge::Rising);
    }

    #[test]
    fn scope_actions_update_acquisition_controls() {
        let model = ScopeModel::default();
        ScopeAction::ToggleRun.publish(&model);
        ScopeAction::NextWaveform.publish(&model);
        ScopeAction::TimeScale.publish(&model);
        ScopeAction::ToggleChannelB.publish(&model);

        let state = model.snapshot();
        assert!(!state.running);
        assert_eq!(state.waveform, WaveformKind::Square);
        assert_eq!(state.time_scale, 3);
        assert!(!state.channel_b);
    }

    #[test]
    fn scope_controls_reflow_without_text_overflow() {
        for (width, height) in [(800, 480), (480, 320), (320, 320)] {
            assert_layout(width, height);
        }
    }

    #[test]
    fn housing_art_uses_lossless_lz4_storage() {
        let meta = Texture::probe_mirx(HOUSING_ART).unwrap();
        assert_eq!((meta.width, meta.height), (800, 480));

        let reader = mirx::Reader::open(HOUSING_ART).unwrap();
        let image = reader.primary().unwrap().unwrap().image().unwrap().unwrap();
        let encoded = image.encoded().unwrap();
        assert_eq!(
            encoded.codings().get(0).unwrap().id(),
            mirx::coding::CodingId::LZ4
        );
        assert!(HOUSING_ART.len() < 200 * 1024);

        let texture = Texture::from_mirx_with(HOUSING_ART, housing_texture_options()).unwrap();
        assert!(matches!(&texture.buf, TexBuf::Aligned(_)));
    }
}
