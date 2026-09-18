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
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{
    ParagraphStyle, ShapingPolicy, Slider, Text, TextDirection, TextOverflow, TextWrap,
};
use crate::ui::{IgnoreHitTest, LayoutAxis, LayoutDependency, Theme};

pub const VIEWPORT: (u16, u16) = (960, 540);

const UI: FontToken = FontToken::Custom("typography_ui");
const CJK: FontToken = FontToken::Custom("typography_cjk");
const ARABIC: FontToken = FontToken::Custom("typography_arabic");
const THAI: FontToken = FontToken::Custom("typography_thai");
const FALLBACKS: [FontToken; 3] = [CJK, ARABIC, THAI];

const BACKGROUND: ColorToken = ColorToken::Surface;
const PANEL: ColorToken = ColorToken::SurfaceVariant;
const PANEL_ALT: ColorToken = ColorToken::Surface;
const BORDER: ColorToken = ColorToken::Outline;
const TEXT: ColorToken = ColorToken::OnSurface;
const MUTED: ColorToken = ColorToken::OnSurfaceVariant;
const CYAN: ColorToken = ColorToken::Primary;
const VIOLET: ColorToken = ColorToken::Tertiary;
const GOLD: ColorToken = ColorToken::Success;
const LANE_COLORS: [ColorToken; 3] = [CYAN, VIOLET, GOLD];
const BASE_STAGE_WIDTH: Fixed = Fixed::from_int(916);
const BASE_STAGE_HEIGHT: Fixed = Fixed::from_int(360);

#[derive(Clone)]
struct CurveModel {
    phase: Signal<Fixed>,
    amplitude: Signal<Fixed>,
    speed: Signal<Fixed>,
    reversed: Signal<bool>,
    paused: Signal<bool>,
    stage_size: Signal<CurveStageSize>,
}

impl Default for CurveModel {
    fn default() -> Self {
        Self {
            phase: Signal::new(Fixed::ZERO),
            amplitude: Signal::new(Fixed::from_int(68)),
            speed: Signal::new(Fixed::from_int(64)),
            reversed: Signal::new(false),
            paused: Signal::new(false),
            stage_size: Signal::new(CurveStageSize::BASE),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CurveStageSize {
    width: Fixed,
    height: Fixed,
}

impl CurveStageSize {
    const BASE: Self = Self {
        width: BASE_STAGE_WIDTH,
        height: BASE_STAGE_HEIGHT,
    };

    fn from_dimensions(width: Fixed, height: Fixed) -> Self {
        Self {
            width: width.max(Fixed::from_int(120)),
            height: height.max(Fixed::from_int(220)),
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
    primary: Entity,
    multiscript: Entity,
    caption: Entity,
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
        overflow: TextOverflow::Ellipsis,
        direction,
        shaping: ShapingPolicy::Required,
        max_lines: Some(1),
        ..ParagraphStyle::default()
    }
}

fn bounded_text(lines: u16) -> ParagraphStyle {
    ParagraphStyle {
        wrap: if lines == 1 {
            TextWrap::NoWrap
        } else {
            TextWrap::Word
        },
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(lines),
        ..ParagraphStyle::default()
    }
}

fn path_window_end(width: Fixed) -> Fixed {
    (width * Fixed::from_ratio(89, 100)).max(Fixed::from_int(100))
}

fn lane_commands(
    lane: usize,
    phase: Fixed,
    amplitude: Fixed,
    stage_width: Fixed,
    stage_height: Fixed,
) -> [PathCmd; 3] {
    let lane_scale = match lane {
        0 => Fixed::ONE,
        1 => Fixed::from_ratio(3, 4),
        _ => Fixed::from_ratio(1, 2),
    };
    let lane_phase = phase + Fixed::from_int(lane as i32 * 71);
    let geometry_scale = (stage_width / BASE_STAGE_WIDTH)
        .min(stage_height / BASE_STAGE_HEIGHT)
        .max(Fixed::from_ratio(1, 4));
    let wave = amplitude * lane_scale * geometry_scale;
    let x = |value| stage_width * Fixed::from_ratio(value, 916);
    let y = |value| stage_height * Fixed::from_ratio(value, 360);
    let base_y = y(104 + lane as i32 * 106);
    let start = Point {
        x: x(34),
        y: base_y + Fixed::sin_deg(lane_phase - Fixed::from_int(38)) * wave / 3,
    };
    let middle = Point {
        x: x(456),
        y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(124)) * wave / 2,
    };
    let end = Point {
        x: x(878),
        y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(286)) * wave / 3,
    };
    [
        PathCmd::MoveTo(start),
        PathCmd::CubicTo {
            ctrl1: Point {
                x: x(168),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(12)) * wave,
            },
            ctrl2: Point {
                x: x(316),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(82)) * wave,
            },
            end: middle,
        },
        PathCmd::CubicTo {
            ctrl1: Point {
                x: x(596),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(168)) * wave,
            },
            ctrl2: Point {
                x: x(744),
                y: base_y + Fixed::sin_deg(lane_phase + Fixed::from_int(238)) * wave,
            },
            end,
        },
    ]
}

fn make_lane(lane: usize) -> Path {
    let [start, first, second] = lane_commands(
        lane,
        Fixed::ZERO,
        Fixed::from_int(68),
        BASE_STAGE_WIDTH,
        BASE_STAGE_HEIGHT,
    );
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

fn update_lane_for_stage(
    path: &mut Path,
    lane: usize,
    phase: Fixed,
    amplitude: Fixed,
    stage_width: Fixed,
    stage_height: Fixed,
) {
    for (index, command) in lane_commands(lane, phase, amplitude, stage_width, stage_height)
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
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let panel = theme.resolve(PANEL);
    let border = theme.resolve(BORDER);
    let lane_colors = LANE_COLORS.map(|token| theme.resolve(token));
    ctx.bg_handled = true;
    fill(renderer, ctx, *rect, panel, Fixed::from_int(18), 255);
    for index in 0..12 {
        let x = rect.x + rect.w * Fixed::from_ratio(index, 11);
        fill(
            renderer,
            ctx,
            Rect::new(x, rect.y, Fixed::ONE, rect.h),
            border,
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
            border,
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
            lane_colors[index as usize % lane_colors.len()],
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
        let paint = Paint::Color(lane_colors[index].into());
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

fn bind_stage_layout(
    cx: &mut crate::ui::UiScope<'_>,
    nodes: CurveNodes,
    paths: CurvePaths,
    model: CurveModel,
) {
    let dependencies = [
        LayoutDependency::entity(nodes.stage, LayoutAxis::Width),
        LayoutDependency::entity(nodes.stage, LayoutAxis::Height),
    ];
    cx.bind_layout(nodes.stage, &dependencies, move |world, _, values| {
        let size = CurveStageSize::from_dimensions(values.get(0), values.get(1));
        model.stage_size.set(size);

        let end = path_window_end(size.width);
        for (entity, path) in [
            (nodes.primary, paths.ids[0]),
            (nodes.multiscript, paths.ids[1]),
            (nodes.caption, paths.ids[2]),
        ] {
            world
                .widget_mut(entity)
                .expect("responsive curve path target")
                .text_path(crate::text::TextPath::new(path).with_range(Fixed::ZERO..end));
        }
        world.invalidate_visual(nodes.stage);
    });
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
        let stage_size = model.stage_size.clone();
        cx.bind_path(path, move |geometry| {
            let current_phase = phase.get();
            let stage_size = stage_size.get();
            let envelope = Fixed::from_ratio(17, 20)
                + Fixed::sin_deg(current_phase / 3 + Fixed::from_int(lane as i32 * 37))
                    * Fixed::from_ratio(3, 20);
            update_lane_for_stage(
                geometry,
                lane,
                current_phase,
                amplitude.get() * envelope,
                stage_size.width,
                stage_size.height,
            );
        })
        .expect("mutable curve path");
    }
}

#[compose]
fn compose_header() -> Entity {
    ui! {
        Column (
            id: "curve_text_header",
            width: Dimension::percent(100),
            height: 104,
            min_height: 104,
            row_gap: 6
        ) {
            Row (
                width: Dimension::percent(100),
                height: 62,
                align: AlignItems::Center,
                column_gap: 12
            ) {
                View (width: 8, height: 38, bg_color: CYAN, border_radius: 4)
                Column (grow: 1.0, min_width: 150, row_gap: 2) {
                    Text (
                        "KINETIC TYPE",
                        width: Dimension::percent(100),
                        height: 30,
                        font: UI,
                        font_size: 25,
                        text_color: TEXT,
                        paragraph: bounded_text(1)
                    )
                    Text (
                        "One shaped run · one retained path · continuous pose",
                        width: Dimension::percent(100),
                        height: 30,
                        font: UI,
                        font_size: 12,
                        text_color: MUTED,
                        paragraph: bounded_text(2)
                    )
                }
            }
            Text (
                ROUTE_LABEL,
                width: 248,
                min_width: 170,
                max_width: 248,
                height: 30,
                bg_color: PANEL_ALT,
                border_color: BORDER,
                border_width: 1,
                border_radius: 15,
                font: UI,
                font_size: 10,
                text_color: CYAN,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose]
fn compose_stage(paths: CurvePaths) -> Entity {
    ui! {
        View (
            id: "curve_text_stage_shell",
            width: Dimension::percent(100),
            height: @id(curve_text_shell).height {
                if curve_text_shell.height < Fixed::from_int(600) {
                    250
                } else if curve_text_shell.height < Fixed::from_int(720) {
                    300
                } else {
                    360
                }
            },
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
                font_size: @id(curve_text_stage).width {
                    if curve_text_stage.width < Fixed::from_int(520) { 18_u16 } else { 30_u16 }
                },
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
                font_size: @id(curve_text_stage).width {
                    if curve_text_stage.width < Fixed::from_int(520) { 17_u16 } else { 25_u16 }
                },
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
                font_size: @id(curve_text_stage).width {
                    if curve_text_stage.width < Fixed::from_int(520) { 10_u16 } else { 14_u16 }
                },
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
            width: Dimension::percent(100),
            height: @id(curve_text_stage).width {
                if curve_text_stage.width < Fixed::from_int(500) {
                    146
                } else if curve_text_stage.width < Fixed::from_int(720) {
                    104
                } else {
                    62
                }
            },
            min_height: @id(curve_text_stage).width {
                if curve_text_stage.width < Fixed::from_int(500) {
                    146
                } else if curve_text_stage.width < Fixed::from_int(720) {
                    104
                } else {
                    62
                }
            },
            padding: Padding {
                top: Dimension::px(10),
                right: Dimension::px(12),
                bottom: Dimension::px(10),
                left: Dimension::px(12),
            },
            wrap: FlexWrap::Wrap,
            align: AlignItems::Center,
            row_gap: 8,
            column_gap: 10,
            clip_children: true,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 14
        ) {
            Row (grow: 1.0, min_width: 250, height: 34, align: AlignItems::Center, column_gap: 8) {
                Text ("AMPLITUDE", width: 74, font: UI, font_size: 10, text_color: MUTED)
                Slider (
                    id: "curve_text_amplitude",
                    grow: 1.0,
                    min_width: 110,
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
                    text_color: TEXT,
                    paragraph: bounded_text(1)
                )
            }
            Row (grow: 1.0, min_width: 210, height: 34, align: AlignItems::Center, column_gap: 8) {
                Text ("SPEED", width: 46, font: UI, font_size: 10, text_color: MUTED)
                Slider (
                    id: "curve_text_speed",
                    grow: 1.0,
                    min_width: 100,
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
                    text_color: TEXT,
                    paragraph: bounded_text(1)
                )
            }
            Row (grow: 1.0, min_width: 190, height: 34, column_gap: 8) {
                Text (
                    id: "curve_text_direction",
                    text: ${ if direction_label.get() { "REVERSE" } else { "FORWARD" } },
                    grow: 1.0,
                    min_width: 92,
                    height: 34,
                    bg_color: PANEL_ALT,
                    border_color: VIOLET,
                    border_width: 1,
                    border_radius: 10,
                    font: UI,
                    font_size: 10,
                    text_color: VIOLET,
                    paragraph: ParagraphStyle::label()
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
                    paragraph: ParagraphStyle::label()
                ) on Tap { CurveAction::TogglePaused.publish(ctx.world); }
            }
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
    let viewport = ui! {
        View (
            id: "curve_text_shell",
            grow: 1.0,
            clip_children: true,
            bg_color: BACKGROUND
        ) {
            Column (
                id: "curve_text_document",
                width: Dimension::percent(100),
                height: Dimension::Content,
                min_height: Dimension::percent(100),
                padding: Padding::all(22),
                row_gap: 12
            ) {
                compose_header ()
                compose_stage (paths)
                compose_controls ()
            }
        }
    };
    let nodes = CurveNodes {
        stage: cx
            .world_mut()
            .find_by_id("curve_text_stage")
            .expect("Curve Text stage"),
        primary: cx
            .world_mut()
            .find_by_id("curve_text_primary")
            .expect("Curve Text primary text"),
        multiscript: cx
            .world_mut()
            .find_by_id("curve_text_multiscript")
            .expect("Curve Text multiscript text"),
        caption: cx
            .world_mut()
            .find_by_id("curve_text_caption")
            .expect("Curve Text caption"),
    };
    bind_stage_layout(cx, nodes, paths, model);
    cx.world_mut().insert_resource(nodes);
    let content = cx
        .world_mut()
        .find_by_id("curve_text_document")
        .expect("Curve Text document");
    super::lab_scroll::LabScroll::attach(
        cx.world_mut(),
        viewport,
        content,
        Fixed::from_int(720),
        Fixed::from_int(22),
    );
    //~focus-end
}

pub fn install<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    crate::gallery::showcase_theme::install(&mut app.world);
    app.with_widget(curve_stage_view());
    crate::gallery::demos::typography_lab::register_fonts(&mut app.world);
    app.world.insert_resource(CurveModel::default());
    app.world.insert_resource(CurveMotion::default());
    let paths = register_paths(&mut app.world);
    app.world.insert_resource(paths);
    app.add_system(curve_text_animation_system::system())
        .add_system(super::lab_scroll::sync_lab_scroll_extents::system());
    app.compose(parent, |cx| build_widgets(cx, paths));
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
    use crate::types::Viewport;

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
    fn stage_size_is_published_as_one_clamped_value() {
        assert_eq!(
            CurveStageSize::from_dimensions(Fixed::from_int(80), Fixed::from_int(160)),
            CurveStageSize {
                width: Fixed::from_int(120),
                height: Fixed::from_int(220),
            }
        );
        assert_eq!(
            CurveStageSize::from_dimensions(Fixed::from_int(640), Fixed::from_int(360)),
            CurveStageSize {
                width: Fixed::from_int(640),
                height: Fixed::from_int(360),
            }
        );
    }

    #[test]
    fn layout_binding_updates_responsive_geometry_without_animation() {
        let mut app = fixture();
        let root = app.root.expect("root");
        crate::ui::render_system::update_layout(
            &mut app.world,
            root,
            &Viewport::new(400, 540, Fixed::ONE),
        );

        let nodes = *app.world.resource::<CurveNodes>().unwrap();
        let controls = app
            .world
            .find_by_id("curve_text_controls")
            .expect("Curve Text controls");
        let stage_rect = app
            .world
            .get::<crate::ui::ComputedRect>(nodes.stage)
            .unwrap()
            .0;
        let stage_size = app
            .world
            .resource::<CurveModel>()
            .unwrap()
            .stage_size
            .get_untracked();
        assert_eq!(
            stage_size,
            CurveStageSize::from_dimensions(stage_rect.w, stage_rect.h)
        );
        assert_eq!(
            app.world.get::<Style>(controls).unwrap().layout.height,
            Dimension::px(146)
        );
        assert_eq!(
            app.world.get::<Style>(nodes.primary).unwrap().font_size,
            Some(18)
        );
        assert_eq!(
            app.world
                .get::<crate::text::TextPath>(nodes.primary)
                .unwrap()
                .end(),
            Some(path_window_end(stage_size.width))
        );

        crate::ui::render_system::update_layout(
            &mut app.world,
            root,
            &Viewport::new(VIEWPORT.0, VIEWPORT.1, Fixed::ONE),
        );
        assert_eq!(
            app.world.get::<Style>(controls).unwrap().layout.height,
            Dimension::px(62)
        );
        assert_eq!(
            app.world.get::<Style>(nodes.primary).unwrap().font_size,
            Some(30)
        );
    }

    #[test]
    fn controls_and_stage_share_the_available_height_at_medium_sizes() {
        for (width, height, expected_controls, expected_stage) in
            [(672, 666, 104, 300), (960, 540, 62, 250)]
        {
            let mut app = fixture();
            let root = app.root.expect("root");
            crate::ui::render_system::update_layout(
                &mut app.world,
                root,
                &Viewport::new(width, height, Fixed::ONE),
            );

            let rect = |world: &World, id| {
                world
                    .get::<crate::ui::ComputedRect>(world.find_by_id(id).unwrap())
                    .unwrap()
                    .0
            };
            let shell = rect(&app.world, "curve_text_shell");
            let stage = rect(&app.world, "curve_text_stage_shell");
            let controls = rect(&app.world, "curve_text_controls");

            assert_eq!(stage.h, Fixed::from_int(expected_stage), "{width}x{height}");
            assert_eq!(
                controls.h,
                Fixed::from_int(expected_controls),
                "{width}x{height}"
            );
            assert!(
                controls.y + controls.h <= shell.y + shell.h,
                "{width}x{height}: {controls:?} {shell:?}"
            );
        }
    }

    #[test]
    fn responsive_text_window_stays_inside_every_scaled_lane() {
        for width in [Fixed::from_int(120), Fixed::from_int(378), BASE_STAGE_WIDTH] {
            for lane in 0..3 {
                let mut path = make_lane(lane);
                update_lane_for_stage(
                    &mut path,
                    lane,
                    Fixed::ZERO,
                    Fixed::from_int(68),
                    width,
                    BASE_STAGE_HEIGHT,
                );
                let mut segments = [crate::text::baseline::MeasuredSegment {
                    end: Point::ZERO,
                    end_distance: Fixed::ZERO,
                }; 128];
                let baseline = crate::text::baseline::PathMeasure::new(
                    &path,
                    0,
                    crate::text::baseline::DEFAULT_TOLERANCE,
                )
                .unwrap()
                .measure_into(&mut segments)
                .unwrap();
                assert!(path_window_end(width) <= baseline.length());
            }
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
            update_lane_for_stage(
                &mut path,
                0,
                phase,
                Fixed::from_int(68) * envelope,
                BASE_STAGE_WIDTH,
                BASE_STAGE_HEIGHT,
            );
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
        let background = Theme::default().resolve(BACKGROUND);
        let non_background = texture
            .buf
            .as_slice()
            .chunks_exact(4)
            .filter(|pixel| pixel[..3] != [background.r, background.g, background.b])
            .count();
        assert!(non_background > 20_000);
    }

    #[test]
    fn shell_and_custom_stage_resolve_the_active_theme() {
        let mut app = fixture();
        app.world.insert_resource(Theme::light());
        app.render().unwrap();

        let surface = Theme::light().resolve(BACKGROUND);
        assert_eq!(
            &app.backend.framebuffer().buf.as_slice()[..3],
            &[surface.r, surface.g, surface.b]
        );
    }
}
