extern crate alloc;

use alloc::format;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::reactive::Signal;
use crate::ecs::DeltaTimeMs;
use crate::prelude::*;
use crate::render::command::{DrawCommand, LineCap, LineJoin, Paint};
use crate::render::font::{FontStack, FontToken};
use crate::render::path::{Path, PathCmd, PathId, PathStore};
use crate::render::renderer::Renderer;
use crate::types::Transform;
use crate::ui::IgnoreHitTest;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{
    ParagraphStyle, ShapingPolicy, Slider, Text, TextAlign, TextDirection, TextVerticalAlign,
    TextWrap,
};

pub const VIEWPORT: (u16, u16) = (960, 540);

const UI: FontToken = FontToken::Custom("typography_ui");
const CJK: FontToken = FontToken::Custom("typography_cjk");
const ARABIC: FontToken = FontToken::Custom("typography_arabic");
const THAI: FontToken = FontToken::Custom("typography_thai");
const FALLBACKS: [FontToken; 3] = [CJK, ARABIC, THAI];

const BACKGROUND: Color = Color::rgb(5, 10, 22);
const PANEL: Color = Color::rgb(10, 20, 39);
const PANEL_ALT: Color = Color::rgb(14, 28, 51);
const BORDER: Color = Color::rgb(39, 64, 94);
const TEXT: Color = Color::rgb(235, 245, 255);
const MUTED: Color = Color::rgb(128, 153, 181);
const CYAN: Color = Color::rgb(64, 237, 218);
const VIOLET: Color = Color::rgb(182, 116, 255);
const GOLD: Color = Color::rgb(255, 197, 88);
const LANE_COLORS: [Color; 3] = [CYAN, VIOLET, GOLD];

#[derive(Clone)]
struct CurveModel {
    phase: Signal<Fixed>,
    amplitude: Signal<Fixed>,
    speed: Signal<Fixed>,
    reversed: Signal<bool>,
    paused: Signal<bool>,
}

impl Default for CurveModel {
    fn default() -> Self {
        Self {
            phase: Signal::new(Fixed::ZERO),
            amplitude: Signal::new(Fixed::from_int(68)),
            speed: Signal::new(Fixed::from_int(64)),
            reversed: Signal::new(false),
            paused: Signal::new(false),
        }
    }
}

#[derive(Clone, Copy)]
struct CurvePaths {
    ids: [PathId; 3],
}

type CurveMotion = super::motion::BrakedPhase;

#[derive(Clone, Copy)]
struct CurveNodes {
    stage: Entity,
}

#[derive(Default, crate::Component)]
struct CurveStage;

enum CurveAction {
    SetAmplitude(Fixed),
    SetSpeed(Fixed),
    ToggleDirection,
    TogglePaused,
}

impl CurveAction {
    fn publish(self, world: &mut World) {
        let Some(model) = world.resource::<CurveModel>().cloned() else {
            return;
        };
        match self {
            Self::SetAmplitude(value) => {
                let next = value
                    .round()
                    .clamp(Fixed::from_int(24), Fixed::from_int(96));
                if next != model.amplitude.get_untracked() {
                    model.amplitude.set(next);
                }
            }
            Self::SetSpeed(value) => {
                let next = value
                    .round()
                    .clamp(Fixed::from_int(20), Fixed::from_int(120));
                if next != model.speed.get_untracked() {
                    model.speed.set(next);
                }
            }
            Self::ToggleDirection => model.reversed.update(|value| *value = !*value),
            Self::TogglePaused => model.paused.update(|value| *value = !*value),
        }
        if let Some(nodes) = world.resource::<CurveNodes>().copied() {
            mark_curve_stage_dirty(world, nodes.stage);
        }
    }
}

fn mixed_stack() -> FontStack {
    FontStack::new(UI).with_fallbacks(&FALLBACKS)
}

fn single_line(direction: TextDirection) -> ParagraphStyle {
    ParagraphStyle {
        wrap: TextWrap::NoWrap,
        direction,
        shaping: ShapingPolicy::Required,
        max_lines: Some(1),
        ..ParagraphStyle::default()
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

fn lane_commands(lane: usize, phase: Fixed, amplitude: Fixed) -> [PathCmd; 3] {
    let lane_scale = match lane {
        0 => Fixed::ONE,
        1 => Fixed::from_ratio(3, 4),
        _ => Fixed::from_ratio(1, 2),
    };
    let lane_phase = phase + Fixed::from_int(lane as i32 * 71);
    let wave = amplitude * lane_scale;
    let (x0, x1, x2, x3, x4, base_y) = (34, 168, 316, 596, 744, 104 + lane as i32 * 106);
    let base_y = Fixed::from_int(base_y);
    let start = Point {
        x: Fixed::from_int(x0),
        y: base_y + Fixed::sin_deg(lane_phase - Fixed::from_int(38)) * wave / 3,
    };
    let middle = Point {
        x: Fixed::from_int(456),
        y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(124)) * wave / 2,
    };
    let end = Point {
        x: Fixed::from_int(878),
        y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(286)) * wave / 3,
    };
    [
        PathCmd::MoveTo(start),
        PathCmd::CubicTo {
            ctrl1: Point {
                x: Fixed::from_int(x1),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(12)) * wave,
            },
            ctrl2: Point {
                x: Fixed::from_int(x2),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(82)) * wave,
            },
            end: middle,
        },
        PathCmd::CubicTo {
            ctrl1: Point {
                x: Fixed::from_int(x3),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(168)) * wave,
            },
            ctrl2: Point {
                x: Fixed::from_int(x4),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(238)) * wave,
            },
            end,
        },
    ]
}

fn make_lane(lane: usize) -> Path {
    let [start, first, second] = lane_commands(lane, Fixed::ZERO, Fixed::from_int(68));
    let mut path = Path::try_with_capacity(3).expect("curve path storage");
    let PathCmd::MoveTo(start) = start else {
        unreachable!();
    };
    let PathCmd::CubicTo { ctrl1, ctrl2, end } = first else {
        unreachable!();
    };
    path.move_to(start).cubic_to(ctrl1, ctrl2, end);
    let PathCmd::CubicTo { ctrl1, ctrl2, end } = second else {
        unreachable!();
    };
    path.cubic_to(ctrl1, ctrl2, end);
    path
}

fn update_lane(path: &mut Path, lane: usize, phase: Fixed, amplitude: Fixed) {
    for (index, command) in lane_commands(lane, phase, amplitude)
        .into_iter()
        .enumerate()
    {
        path.set_command(index, command)
            .expect("curve path topology remains fixed");
    }
}

fn register_paths(world: &mut World) -> CurvePaths {
    let mut paths = world.paths();
    CurvePaths {
        ids: [
            paths.insert(make_lane(0)).expect("first curve path"),
            paths.insert(make_lane(1)).expect("second curve path"),
            paths.insert(make_lane(2)).expect("third curve path"),
        ],
    }
}

fn fill(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx<'_>,
    area: Rect,
    color: Color,
    radius: Fixed,
    opacity: u8,
) {
    ctx.draw(
        renderer,
        &DrawCommand::Fill {
            area,
            transform: ctx.transform,
            quad: None,
            color,
            radius,
            opa: opacity,
        },
        ctx.clip,
    );
}

fn curve_stage_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if !world.has::<CurveStage>(entity) {
        return;
    }
    ctx.bg_handled = true;
    fill(renderer, ctx, *rect, PANEL, Fixed::from_int(18), 255);
    for index in 0..12 {
        let x = rect.x + rect.w * Fixed::from_ratio(index, 11);
        fill(
            renderer,
            ctx,
            Rect::new(x, rect.y, Fixed::ONE, rect.h),
            BORDER,
            Fixed::ZERO,
            if index % 3 == 0 { 48 } else { 22 },
        );
    }
    for index in 1..4 {
        let y = rect.y + rect.h * Fixed::from_ratio(index, 4);
        fill(
            renderer,
            ctx,
            Rect::new(rect.x, y, rect.w, Fixed::ONE),
            BORDER,
            Fixed::ZERO,
            28,
        );
    }

    let phase = world
        .resource::<CurveModel>()
        .map_or(Fixed::ZERO, |model| model.phase.get_untracked());
    for index in 0..5 {
        let angle = phase + Fixed::from_int(index * 72);
        let x = rect.x + rect.w / 2 + Fixed::cos_deg(angle) * Fixed::from_int(300);
        let y = rect.y + rect.h / 2 + Fixed::sin_deg(angle * 2) * Fixed::from_int(126);
        let radius = Fixed::from_int(10 + index % 3 * 4);
        fill(
            renderer,
            ctx,
            Rect::new(x - radius, y - radius, radius * 2, radius * 2),
            LANE_COLORS[index as usize % LANE_COLORS.len()],
            radius,
            32,
        );
    }

    let (Some(paths), Some(store)) = (
        world.resource::<CurvePaths>().copied(),
        world.resource::<PathStore>(),
    ) else {
        return;
    };
    let transform = ctx.transform.compose(&Transform::translate(rect.x, rect.y));
    for (index, id) in paths.ids.into_iter().enumerate() {
        let Ok(path) = store.get(id) else {
            continue;
        };
        let paint = Paint::Color(LANE_COLORS[index].into());
        ctx.draw(
            renderer,
            &DrawCommand::StrokePath {
                path,
                transform,
                paint: &paint,
                width: Fixed::from_int(5),
                opa: 16,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                miter_limit: Fixed::from_int(4),
                dash: &[],
            },
            ctx.clip,
        );
        ctx.draw(
            renderer,
            &DrawCommand::StrokePath {
                path,
                transform,
                paint: &paint,
                width: Fixed::ONE,
                opa: 150,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                miter_limit: Fixed::from_int(4),
                dash: &[],
            },
            ctx.clip,
        );
    }
}

fn curve_stage_view() -> View {
    View::new("CurveStage", 60, curve_stage_render).with_filter::<CurveStage>()
}

fn update_curve_visual(world: &mut World, nodes: CurveNodes) {
    mark_curve_stage_dirty(world, nodes.stage);
}

fn mark_curve_stage_dirty(world: &mut World, stage: Entity) {
    if let Some(rect) = world
        .get::<crate::ui::ComputedRect>(stage)
        .map(|rect| rect.0)
    {
        world.invalidate_rect(rect);
    } else {
        world.invalidate_visual(stage);
    }
}

#[mirui_macros::system(order = ANIMATION)]
fn curve_text_animation_system(world: &mut World) {
    const MAX_STEP_MS: u16 = 50;
    const RATE_RAMP_MS: u16 = 520;

    let Some(model) = world.resource::<CurveModel>().cloned() else {
        return;
    };
    let paused = model.paused.get_untracked();
    let reversed = model.reversed.get_untracked();
    let speed = model.speed.get_untracked();
    let dt = world
        .resource::<DeltaTimeMs>()
        .map_or(16, |delta| delta.0)
        .min(MAX_STEP_MS);
    let Some(motion) = world.resource_mut::<CurveMotion>() else {
        return;
    };
    let direction = if reversed {
        Fixed::from_int(-1)
    } else {
        Fixed::ONE
    };
    let phase = motion.advance(
        dt,
        RATE_RAMP_MS,
        speed,
        direction,
        paused,
        Fixed::from_int(360),
    );
    if phase == model.phase.get_untracked() {
        return;
    }
    model.phase.set(phase);
    if let Some(nodes) = world.resource::<CurveNodes>().copied() {
        update_curve_visual(world, nodes);
    }
}

const ROUTE_LABEL: &str = "POSED GLYPHS · BOUNDED FALLBACK";

fn bind_curve_paths(cx: &mut crate::ui::UiScope<'_>, paths: CurvePaths, model: &CurveModel) {
    for (lane, path) in paths.ids.into_iter().enumerate() {
        let phase = model.phase.clone();
        let amplitude = model.amplitude.clone();
        cx.bind_path(path, move |geometry| {
            let current_phase = phase.get();
            let envelope = Fixed::from_ratio(17, 20)
                + Fixed::sin_deg(current_phase / 3 + Fixed::from_int(lane as i32 * 37))
                    * Fixed::from_ratio(3, 20);
            update_lane(geometry, lane, current_phase, amplitude.get() * envelope);
        })
        .expect("mutable curve path");
    }
}

#[compose]
fn compose_header() -> Entity {
    ui! {
        Row (height: 54, align: AlignItems::Center, column_gap: 12) {
            View (width: 8, height: 38, bg_color: CYAN, border_radius: 4)
            Column (grow: 1.0, row_gap: 2) {
                Text (
                    "KINETIC TYPE",
                    height: 30,
                    font: UI,
                    font_size: 25,
                    text_color: TEXT,
                    paragraph: single_line(TextDirection::LeftToRight)
                )
                Text (
                    "One shaped run · one retained path · continuous pose",
                    font: UI,
                    font_size: 12,
                    text_color: MUTED,
                    paragraph: single_line(TextDirection::LeftToRight)
                )
            }
            Text (
                ROUTE_LABEL,
                width: 248,
                height: 30,
                bg_color: PANEL_ALT,
                border_color: BORDER,
                border_width: 1,
                border_radius: 15,
                font: UI,
                font_size: 10,
                text_color: CYAN,
                paragraph: centered_label()
            )
        }
    }
}

#[compose]
fn compose_stage(paths: CurvePaths) -> Entity {
    ui! {
        View (
            id: "curve_text_stage_shell",
            grow: 1.0,
            min_height: 360,
            clip_children: true,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 18
        ) {
            CurveStage (
                id: "curve_text_stage",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100)
            ) [
                IgnoreHitTest,
            ]
            Text (
                id: "curve_text_primary",
                "MIRUI · BEND SPACE, NOT GLYPHS",
                path: crate::text::TextPath::new(paths.ids[0])
                    .with_range(Fixed::ZERO..Fixed::from_int(820)),
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                font: UI,
                font_size: 30,
                text_color: TEXT,
                paragraph: single_line(TextDirection::LeftToRight)
            ) [
                IgnoreHitTest,
            ]
            Text (
                id: "curve_text_multiscript",
                "中文曲线排版 · مرحبا · ตั้ง",
                path: crate::text::TextPath::new(paths.ids[1])
                    .with_range(Fixed::ZERO..Fixed::from_int(820)),
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                font_stack: mixed_stack(),
                font_size: 25,
                text_color: CYAN,
                paragraph: single_line(TextDirection::Auto)
            ) [
                IgnoreHitTest,
            ]
            Text (
                id: "curve_text_caption",
                "PATH REVISION → MEASURE → PLACE → FOUR BACKENDS",
                path: crate::text::TextPath::new(paths.ids[2])
                    .with_range(Fixed::ZERO..Fixed::from_int(820)),
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                font: UI,
                font_size: 14,
                text_color: GOLD,
                paragraph: single_line(TextDirection::LeftToRight)
            ) [
                IgnoreHitTest,
            ]
        }
    }
}

#[compose]
fn compose_controls() -> Entity {
    let model = cx
        .world_mut()
        .resource::<CurveModel>()
        .cloned()
        .expect("Curve Text model");
    let amplitude_value = model.amplitude;
    let speed_value = model.speed;
    let direction_label = model.reversed;
    let paused_label = model.paused;

    ui! {
        Row (
            id: "curve_text_controls",
            height: 62,
            padding: Padding {
                top: Dimension::px(10),
                right: Dimension::px(12),
                bottom: Dimension::px(10),
                left: Dimension::px(12),
            },
            align: AlignItems::Center,
            column_gap: 10,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Text ("AMPLITUDE", width: 74, font: UI, font_size: 10, text_color: MUTED)
            Slider (
                id: "curve_text_amplitude",
                width: 148,
                height: 14,
                min: Fixed::from_int(24),
                max: Fixed::from_int(96),
                value: Fixed::from_int(68),
                track_color: BORDER,
                fill_color: CYAN,
                thumb_color: TEXT
            ) on ValueChanged {
                let _ = old;
                CurveAction::SetAmplitude(*new).publish(ctx.world);
            }
            Text (
                text: ${ format!("{}", amplitude_value.get().to_int()) },
                width: 28,
                font: UI,
                font_size: 11,
                text_color: TEXT
            )
            Text ("SPEED", width: 46, font: UI, font_size: 10, text_color: MUTED)
            Slider (
                id: "curve_text_speed",
                width: 132,
                height: 14,
                min: Fixed::from_int(20),
                max: Fixed::from_int(120),
                value: Fixed::from_int(64),
                track_color: BORDER,
                fill_color: VIOLET,
                thumb_color: TEXT
            ) on ValueChanged {
                let _ = old;
                CurveAction::SetSpeed(*new).publish(ctx.world);
            }
            Text (
                text: ${ format!("{}", speed_value.get().to_int()) },
                width: 30,
                font: UI,
                font_size: 11,
                text_color: TEXT
            )
            Text (
                id: "curve_text_direction",
                text: ${ if direction_label.get() { "REVERSE" } else { "FORWARD" } },
                width: 96,
                height: 34,
                bg_color: PANEL_ALT,
                border_color: VIOLET,
                border_width: 1,
                border_radius: 10,
                font: UI,
                font_size: 10,
                text_color: VIOLET,
                paragraph: centered_label()
            ) on Tap { CurveAction::ToggleDirection.publish(ctx.world); }
            Text (
                id: "curve_text_pause",
                text: ${ if paused_label.get() { "RESUME" } else { "PAUSE" } },
                grow: 1.0,
                min_width: 82,
                height: 34,
                bg_color: PANEL_ALT,
                border_color: CYAN,
                border_width: 1,
                border_radius: 10,
                font: UI,
                font_size: 10,
                text_color: CYAN,
                paragraph: centered_label()
            ) on Tap { CurveAction::TogglePaused.publish(ctx.world); }
        }
    }
}

#[compose]
fn build_widgets(paths: CurvePaths) {
    let model = cx
        .world_mut()
        .resource::<CurveModel>()
        .cloned()
        .expect("Curve Text model");
    bind_curve_paths(cx, paths, &model);

    //~focus-start
    ui! {
        Column (
            id: "curve_text_shell",
            grow: 1.0,
            padding: Padding::all(22),
            row_gap: 12,
            bg_color: BACKGROUND
        ) {
            compose_header ()
            compose_stage (paths)
            compose_controls ()
        }
    };
    //~focus-end
}

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.with_widget(curve_stage_view());
    crate::gallery::demos::typography_lab::register_fonts(&mut app.world);
    app.world.insert_resource(CurveModel::default());
    app.world.insert_resource(CurveMotion::default());
    let paths = register_paths(&mut app.world);
    app.world.insert_resource(paths);
    app.add_system(curve_text_animation_system::system());
    app.compose(parent, |cx| build_widgets(cx, paths));
    let stage = app
        .world
        .find_by_id("curve_text_stage")
        .expect("Curve Text stage");
    app.world.insert_resource(CurveNodes { stage });
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    if app.world.resource::<MonoClock>().is_none() {
        app.add_plugin(StdInstantClockPlugin);
    }
    install(app, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::surface::FramebufferAccess;

    type TestApp = App<
        crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
        crate::app::SwRendererFactory,
    >;

    fn fixture() -> TestApp {
        let mut app = App::headless(VIEWPORT.0, VIEWPORT.1);
        app.with_default_widgets().with_default_systems();
        let parent = app.spawn_root().id();
        install(&mut app, parent);
        app.set_root(parent);
        app
    }

    #[test]
    fn animated_lanes_keep_a_bounded_flattened_shape() {
        for lane in 0..3 {
            let path = make_lane(lane);
            let requirements = crate::text::baseline::PathMeasure::new(
                &path,
                0,
                crate::text::baseline::DEFAULT_TOLERANCE,
            )
            .unwrap()
            .requirements()
            .unwrap();
            assert!(
                requirements.segments <= 128,
                "lane {lane}: {requirements:?}"
            );
        }
    }

    #[test]
    fn animated_lane_baseline_is_temporally_continuous() {
        let mut path = make_lane(0);
        let mut previous: Option<(Point, Point)> = None;
        let mut segment_count = None;
        let mut maximum_position_step = Fixed::ZERO;
        let mut maximum_tangent_step = Fixed::ZERO;
        let mut previous_quad: Option<[Point; 4]> = None;
        let mut maximum_quad_step = Fixed::ZERO;
        for frame in 0..360 {
            let phase = Fixed::from_ratio(frame * 1_024, 1_000);
            let envelope =
                Fixed::from_ratio(17, 20) + Fixed::sin_deg(phase / 3) * Fixed::from_ratio(3, 20);
            update_lane(&mut path, 0, phase, Fixed::from_int(68) * envelope);
            let measure = crate::text::baseline::PathMeasure::new(
                &path,
                0,
                crate::text::baseline::DEFAULT_TOLERANCE,
            )
            .unwrap();
            let requirements = measure.requirements().unwrap();
            let mut segments = alloc::vec![
                crate::text::baseline::MeasuredSegment {
                    end: Point::ZERO,
                    end_distance: Fixed::ZERO,
                };
                requirements.segments
            ];
            let baseline = measure.measure_into(&mut segments).unwrap();
            let mut cursor = baseline.cursor();
            let sample = cursor.sample_forward(Fixed::from_int(360)).unwrap();
            let pose = Transform {
                m00: sample.unit_tangent.x,
                m01: -sample.unit_tangent.y,
                tx: sample.position.x,
                m10: sample.unit_tangent.y,
                m11: sample.unit_tangent.x,
                ty: sample.position.y,
            };
            let quad = pose.apply_rect(Rect::new(0, -24, 32, 32));
            assert_eq!(
                *segment_count.get_or_insert(requirements.segments),
                requirements.segments
            );
            if let Some((last_position, last_tangent)) = previous {
                let position_step = (sample.position.x - last_position.x)
                    .abs()
                    .max((sample.position.y - last_position.y).abs());
                let tangent_step = (sample.unit_tangent.x - last_tangent.x)
                    .abs()
                    .max((sample.unit_tangent.y - last_tangent.y).abs());
                maximum_position_step = maximum_position_step.max(position_step);
                maximum_tangent_step = maximum_tangent_step.max(tangent_step);
            }
            if let Some(previous_quad) = previous_quad {
                for (current, previous) in quad.into_iter().zip(previous_quad) {
                    maximum_quad_step = maximum_quad_step
                        .max((current.x - previous.x).abs())
                        .max((current.y - previous.y).abs());
                }
            }
            previous = Some((sample.position, sample.unit_tangent));
            previous_quad = Some(quad);
        }
        assert!(
            maximum_position_step <= Fixed::ONE,
            "{maximum_position_step:?}"
        );
        assert!(
            maximum_tangent_step <= Fixed::from_ratio(1, 32),
            "{maximum_tangent_step:?}"
        );
        assert!(
            maximum_quad_step <= Fixed::from_int(2),
            "{maximum_quad_step:?}"
        );
    }

    fn tap(world: &mut World, id: &'static str) {
        let target = world.find_by_id(id).expect("control id");
        GestureHandler::trigger(
            world,
            target,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target,
            },
        );
        flush_signal_dirty(world);
    }

    #[test]
    fn exposes_text_and_control_entities_by_id() {
        let app = fixture();
        for id in [
            "curve_text_shell",
            "curve_text_stage",
            "curve_text_primary",
            "curve_text_multiscript",
            "curve_text_caption",
            "curve_text_amplitude",
            "curve_text_speed",
            "curve_text_direction",
            "curve_text_pause",
        ] {
            assert!(app.world.find_by_id(id).is_some(), "missing {id}");
        }
        for id in [
            "curve_text_primary",
            "curve_text_multiscript",
            "curve_text_caption",
            "curve_text_direction",
            "curve_text_pause",
        ] {
            let entity = app.world.find_by_id(id).unwrap();
            assert!(app.world.get::<Text>(entity).is_some(), "{id} is not Text");
        }
    }

    #[test]
    fn animation_reuses_fixed_path_topology_and_capacity() {
        let mut app = fixture();
        let paths = *app.world.resource::<CurvePaths>().unwrap();
        let before = paths.ids.map(|id| {
            let store = app.world.resource::<PathStore>().unwrap();
            let path = store.get(id).unwrap();
            (
                path.commands().len(),
                path.command_capacity(),
                store.revision(id).unwrap().unwrap(),
            )
        });
        app.world.insert_resource(DeltaTimeMs(16));
        for _ in 0..12 {
            curve_text_animation_system(&mut app.world);
            flush_signal_dirty(&mut app.world);
        }
        for (index, id) in paths.ids.into_iter().enumerate() {
            let store = app.world.resource::<PathStore>().unwrap();
            let path = store.get(id).unwrap();
            assert_eq!(path.commands().len(), before[index].0);
            assert_eq!(path.command_capacity(), before[index].1);
            assert!(store.revision(id).unwrap().unwrap() != before[index].2);
        }
    }

    #[test]
    fn pause_brakes_and_resume_continues_from_relative_phase() {
        let mut app = fixture();
        app.world.insert_resource(DeltaTimeMs(50));
        curve_text_animation_system(&mut app.world);
        let model = app.world.resource::<CurveModel>().unwrap().clone();
        let moving = model.phase.get_untracked();

        tap(&mut app.world, "curve_text_pause");
        for _ in 0..12 {
            curve_text_animation_system(&mut app.world);
        }
        let stopped = model.phase.get_untracked();
        assert!(stopped > moving);
        assert_eq!(
            app.world.resource::<CurveMotion>().unwrap().rate(),
            Fixed::ZERO
        );

        app.world.insert_resource(DeltaTimeMs(5_000));
        curve_text_animation_system(&mut app.world);
        assert_eq!(model.phase.get_untracked(), stopped);

        tap(&mut app.world, "curve_text_pause");
        curve_text_animation_system(&mut app.world);
        assert!(model.phase.get_untracked() > stopped);
        assert!(app.world.resource::<CurveMotion>().unwrap().rate() < Fixed::ONE);
    }

    #[test]
    fn direction_and_sliders_publish_semantic_state() {
        let mut app = fixture();
        let model = app.world.resource::<CurveModel>().unwrap().clone();
        tap(&mut app.world, "curve_text_direction");
        assert!(model.reversed.get_untracked());

        CurveAction::SetAmplitude(Fixed::from_int(120)).publish(&mut app.world);
        CurveAction::SetSpeed(Fixed::ZERO).publish(&mut app.world);
        assert_eq!(model.amplitude.get_untracked(), Fixed::from_int(96));
        assert_eq!(model.speed.get_untracked(), Fixed::from_int(20));
    }

    #[test]
    fn renders_curved_text_and_guides() {
        let mut app = fixture();
        app.render().unwrap();
        let texture = app.backend.framebuffer();
        let non_background = texture
            .buf
            .as_slice()
            .chunks_exact(4)
            .filter(|pixel| pixel[..3] != [BACKGROUND.r, BACKGROUND.g, BACKGROUND.b])
            .count();
        assert!(non_background > 20_000);
    }
}
