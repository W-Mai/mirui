extern crate alloc;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 320);

use alloc::format;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::orbit::{
    MAX_PREVIEW, MISSIONS, ManeuverNode, OrbitEventKind, OrbitModal, OrbitModel, OrbitPage,
    OrbitPoint, OrbitStatus,
};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::Hidden;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

pub const VIEWPORT: (u16, u16) = (480, 320);

const INK: Color = Color::rgb(24, 34, 45);
const BACKGROUND: Color = Color::rgb(239, 239, 233);
const PANEL: Color = Color::rgb(250, 250, 244);
const SPACE: Color = Color::rgb(17, 28, 41);
const LINE: Color = Color::rgb(188, 199, 201);
const MUTED: Color = Color::rgb(110, 126, 139);
const ORANGE: Color = Color::rgb(244, 139, 54);
const CYAN: Color = Color::rgb(65, 210, 204);
const VIOLET: Color = Color::rgb(147, 112, 222);

#[derive(crate::Component, Default)]
struct OrbitSurface;

#[derive(crate::Component, Default)]
struct OrbitModalSurface;

#[derive(Clone, Copy)]
struct OrbitNodes {
    surface: Entity,
    mission_meta: Entity,
    time_meta: Entity,
    tabs: [Entity; 3],
    pages: [Entity; 3],
    map_values: [Entity; 8],
    plan_values: [Entity; 5],
    record_values: [Entity; 6],
    footer: [Entity; 5],
    modal: Entity,
    modal_title: Entity,
    modal_subtitle: Entity,
    modal_buttons: [Entity; 4],
}

#[derive(Clone, Copy)]
struct OrbitPreview {
    points: [OrbitPoint; MAX_PREVIEW],
    len: u8,
    sampled_at: crate::types::Fixed64,
    body: crate::gallery::play::orbit::OrbitBody,
    dv_tenths: u8,
    angle: i16,
}

impl Default for OrbitPreview {
    fn default() -> Self {
        Self {
            points: [OrbitPoint::default(); MAX_PREVIEW],
            len: 0,
            sampled_at: crate::types::Fixed64::from_int(-1),
            body: Default::default(),
            dv_tenths: 0,
            angle: 0,
        }
    }
}

impl OrbitNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut OrbitModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<OrbitModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        if changes.contains(ChangeSet::MODEL) {
            Self::refresh_preview(world);
            Self::sync(world);
        } else if changes.contains(ChangeSet::VISUAL)
            && let Some(surface) = world.resource::<Self>().map(|nodes| nodes.surface)
        {
            world.invalidate_visual(surface);
        }
    }

    fn result(
        world: &mut World,
        update: impl FnOnce(
            &mut OrbitModel,
        ) -> Result<ChangeSet, crate::gallery::play::orbit::OrbitError>,
    ) {
        Self::update(world, |model| update(model).unwrap_or(ChangeSet::VISUAL));
    }

    fn refresh_preview(world: &mut World) {
        let Some(model) = world.resource::<OrbitModel>() else {
            return;
        };
        let body = model.body();
        let time = model.time();
        let dv_tenths = model.dv_tenths();
        let angle = model.angle_degrees();
        let refresh = world.resource::<OrbitPreview>().is_none_or(|preview| {
            preview.body != body
                && (time - preview.sampled_at).abs() >= crate::types::Fixed64::from_ratio(7, 10)
                || preview.dv_tenths != dv_tenths
                || preview.angle != angle
        });
        if !refresh {
            return;
        }
        let mut points = [OrbitPoint::default(); MAX_PREVIEW];
        let len = model.predict(&mut points);
        let preview = OrbitPreview {
            points,
            len: len as u8,
            sampled_at: time,
            body,
            dv_tenths,
            angle,
        };
        let _ = model;
        world.insert_resource(preview);
    }

    fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<OrbitModel>() else {
            return;
        };
        let mission = model.mission();
        let page = model.page();
        let modal = model.modal();
        let status = model.status();
        let body = model.body();
        let map_values = [
            format!("FUEL  {} / 48", decimal(model.fuel(), 1)),
            format!("HEAT  {}%", decimal(model.heat(), 0)),
            format!("R  {}", decimal(body.radius(), 1)),
            format!("V  {}", decimal(body.speed(), 1)),
            format!("DV  {}.{}", model.dv_tenths() / 10, model.dv_tenths() % 10),
            format!("A  {}", model.angle_degrees()),
            if model.preview() {
                "预演 已开".into()
            } else {
                "预演 已关".into()
            },
            format!("{}×", model.warp()),
        ];
        let plan_values = [
            format!("{} s", model.delay_seconds()),
            node_text(model.queue(0), 1),
            node_text(model.queue(1), 2),
            node_text(model.queue(2), 3),
            format!("{} / 3 个节点", model.queue_len()),
        ];
        let latest = model
            .event_len()
            .checked_sub(1)
            .and_then(|index| model.event(index));
        let record_values = [
            format!("{}", model.telemetry_len()),
            format!("{}", model.trail_len()),
            decimal(body.radius(), 1),
            decimal(body.speed(), 1),
            latest.map_or("等待事件".into(), |event| {
                event_label(event.kind).into()
            }),
            status_label(status).into(),
        ];
        let footer_labels = [
            if status == OrbitStatus::Running {
                "Ⅱ 暂停"
            } else {
                "▶ 滑行"
            },
            "点火",
            if model.eligible() {
                "● 立即采样"
            } else {
                "采样"
            },
            "＋ 节点",
            "重新装载",
        ];
        let terminal = status.terminal();
        let _ = model;

        set_text(
            world,
            nodes.mission_meta,
            format!("任务 0{} · {}", mission_index(world) + 1, mission.name),
        );
        set_text(
            world,
            nodes.time_meta,
            format!("SIM T+{}", decimal_time(world)),
        );
        for (index, entity) in nodes.tabs.into_iter().enumerate() {
            set_button_state(world, entity, page_index(page) == index, true);
        }
        for (index, entity) in nodes.pages.into_iter().enumerate() {
            set_hidden(world, entity, page_index(page) != index);
        }
        for (entity, value) in nodes.map_values.into_iter().zip(map_values) {
            set_text(world, entity, value);
        }
        for (entity, value) in nodes.plan_values.into_iter().zip(plan_values) {
            set_text(world, entity, value);
        }
        for (entity, value) in nodes.record_values.into_iter().zip(record_values) {
            set_text(world, entity, value);
        }
        for (index, entity) in nodes.footer.into_iter().enumerate() {
            set_text(world, entity, footer_labels[index]);
            let enabled = match index {
                3 => !terminal && queue_len(world) < 3,
                _ => index == 4 || !terminal,
            };
            set_button_state(
                world,
                entity,
                index == 0 || (index == 2 && eligible(world)),
                enabled,
            );
        }
        set_hidden(world, nodes.modal, modal == OrbitModal::None);
        let (title, subtitle) = match modal {
            OrbitModal::None => ("", ""),
            OrbitModal::Missions => ("选择轨道任务", "切换会清空当前轨迹、计划与遥测记录。"),
            OrbitModal::Confirm(_) => ("重新装载任务？", "当前模拟状态将被重置。"),
            OrbitModal::Help => (
                "轨道任务台 / 操作手册",
                "设定机动并点火；进入目标窗口后手动采样。",
            ),
        };
        set_text(world, nodes.modal_title, title);
        set_text(world, nodes.modal_subtitle, subtitle);
        for (index, entity) in nodes.modal_buttons.into_iter().enumerate() {
            let (label, visible, active) = match modal {
                OrbitModal::Missions => (
                    MISSIONS.get(index).map_or("", |mission| mission.name),
                    index < 3,
                    index as u8 == mission_index(world),
                ),
                OrbitModal::Confirm(_) => match index {
                    0 => ("取消", true, false),
                    1 => ("确认装载", true, true),
                    _ => ("", false, false),
                },
                OrbitModal::Help => (
                    if index == 0 { "明白了" } else { "" },
                    index == 0,
                    index == 0,
                ),
                OrbitModal::None => ("", false, false),
            };
            set_hidden(world, entity, !visible);
            if visible {
                set_text(world, entity, label);
                set_button_state(world, entity, active, true);
            }
        }
        world.invalidate_visual(nodes.surface);
    }
}

fn decimal(value: crate::types::Fixed64, digits: u8) -> alloc::string::String {
    if digits == 0 {
        return format!("{}", value.to_int());
    }
    let scaled = (value * 10).to_int();
    format!("{}.{:01}", scaled / 10, scaled.unsigned_abs() % 10)
}

fn mission_index(world: &World) -> u8 {
    world
        .resource::<OrbitModel>()
        .map_or(0, OrbitModel::mission_index)
}
fn decimal_time(world: &World) -> alloc::string::String {
    world
        .resource::<OrbitModel>()
        .map_or("0.0".into(), |model| decimal(model.time(), 1))
}
fn queue_len(world: &World) -> usize {
    world
        .resource::<OrbitModel>()
        .map_or(0, OrbitModel::queue_len)
}
fn eligible(world: &World) -> bool {
    world
        .resource::<OrbitModel>()
        .is_some_and(OrbitModel::eligible)
}

fn node_text(node: Option<ManeuverNode>, number: usize) -> alloc::string::String {
    node.map_or_else(
        || format!("0{number}  空节点"),
        |node| {
            format!(
                "0{number}  T+{}  Δv {}.{}  {}°",
                decimal(node.at, 1),
                node.dv_tenths / 10,
                node.dv_tenths % 10,
                node.angle_degrees
            )
        },
    )
}

fn event_label(kind: OrbitEventKind) -> &'static str {
    match kind {
        OrbitEventKind::Loaded => "任务装载 · 等待机动",
        OrbitEventKind::Burn => "机动点火完成",
        OrbitEventKind::NodeSkipped => "计划节点跳过",
        OrbitEventKind::Goal => "目标窗口采样完成",
        OrbitEventKind::Won => "全部目标完成",
        OrbitEventKind::Crashed => "接近中心边界 · 中止",
        OrbitEventKind::Escaped => "越出模拟区域 · 中止",
    }
}

fn status_label(status: OrbitStatus) -> &'static str {
    match status {
        OrbitStatus::Ready => "READY",
        OrbitStatus::Running => "RUNNING",
        OrbitStatus::Paused => "PAUSED",
        OrbitStatus::Won => "COMPLETE",
        OrbitStatus::Crashed => "CRASHED",
        OrbitStatus::Escaped => "ESCAPED",
    }
}

fn page_index(page: OrbitPage) -> usize {
    match page {
        OrbitPage::Map => 0,
        OrbitPage::Plan => 1,
        OrbitPage::Record => 2,
    }
}

fn set_text(world: &mut World, entity: Entity, content: impl Into<alloc::string::String>) {
    if let Some(text) = world.get_mut::<Text>(entity) {
        text.set_content(content.into());
    }
    world.invalidate(entity);
}

fn set_hidden(world: &mut World, entity: Entity, hidden: bool) {
    if hidden {
        if !world.has::<Hidden>(entity) {
            world.insert(entity, Hidden);
        }
    } else {
        world.remove::<Hidden>(entity);
    }
    world.invalidate(entity);
}

fn set_button_state(world: &mut World, entity: Entity, active: bool, enabled: bool) {
    if let Some(button) = world.get_mut::<Button>(entity) {
        button.normal_color = if active {
            ORANGE.into()
        } else if enabled {
            INK.into()
        } else {
            LINE.into()
        };
        button.pressed_color = ORANGE.into();
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active || enabled {
            BACKGROUND.into()
        } else {
            MUTED.into()
        };
    }
    world.invalidate_visual(entity);
}

fn map_point(point: OrbitPoint) -> Point {
    Point {
        x: Fixed::from_int(154) + point.x.to_fixed() * Fixed::from_ratio(7, 10),
        y: Fixed::from_int(159) + point.y.to_fixed() * Fixed::from_ratio(7, 10),
    }
}

fn paint_shell(painter: &mut PlayPainter<'_, '_>) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 31), INK, Fixed::ZERO);
    painter.fill(
        Rect::new(0, 31, 480, 28),
        Color::rgb(226, 231, 228),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(0, 282, 480, 38), INK, Fixed::ZERO);
}

fn paint_map(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel, preview: &OrbitPreview) {
    painter.fill(Rect::new(11, 65, 287, 188), SPACE, Fixed::ZERO);
    painter.fill(Rect::new(307, 65, 162, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(307, 65, 162, 188), LINE, Fixed::ONE, Fixed::ZERO);
    let center = Point::new(154, 159);
    for radius in [27, 78, 113, 130] {
        painter.arc(
            center,
            Fixed::from_int(radius) * Fixed::from_ratio(7, 10),
            Fixed::ZERO,
            Fixed::from_int(360),
            if radius >= 113 {
                Color::rgba(244, 139, 54, 120)
            } else {
                Color::rgb(53, 75, 94)
            },
            Fixed::ONE,
        );
    }
    painter.circle(center, Fixed::from_int(17), Color::rgb(244, 173, 74));
    painter.circle(center, Fixed::from_int(11), Color::rgb(255, 221, 128));
    let mut previous = None;
    for index in 0..model.trail_len() {
        let Some(point) = model.trail(index) else {
            continue;
        };
        let point = map_point(point);
        if let Some(from) = previous {
            painter.line(from, point, CYAN, Fixed::ONE);
        }
        previous = Some(point);
    }
    if model.preview() {
        let mut previous = Some(map_point(model.body().position));
        for point in preview.points.iter().take(usize::from(preview.len)) {
            let point = map_point(*point);
            if let Some(from) = previous {
                painter.line(from, point, ORANGE, Fixed::from_ratio(1, 2));
            }
            previous = Some(point);
        }
    }
    let craft = map_point(model.body().position);
    painter.circle(craft, Fixed::from_int(5), CYAN);
    painter.circle(craft, Fixed::from_int(2), PANEL);
    if model.mission_index() == 2 {
        let beacon = map_point(OrbitPoint {
            x: crate::types::Fixed64::from_int(-148),
            y: crate::types::Fixed64::ZERO,
        });
        painter.arc(
            beacon,
            Fixed::from_int(16),
            Fixed::ZERO,
            Fixed::from_int(360),
            VIOLET,
            Fixed::from_int(2),
        );
    }
    painter.fill(Rect::new(318, 98, 139, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(318, 98, 139 * model.fuel().to_fixed().to_int() / 48, 4),
        CYAN,
        Fixed::ZERO,
    );
    painter.fill(Rect::new(318, 124, 139, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(318, 124, 139 * model.heat().to_fixed().to_int() / 100, 4),
        ORANGE,
        Fixed::ZERO,
    );
}

fn paint_plan(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    painter.fill(Rect::new(11, 65, 292, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(11, 65, 292, 188), LINE, Fixed::ONE, Fixed::ZERO);
    painter.fill(Rect::new(313, 65, 156, 188), SPACE, Fixed::ZERO);
    painter.circle(
        Point::new(391, 144),
        Fixed::from_int(45),
        Color::rgb(31, 50, 67),
    );
    painter.arc(
        Point::new(391, 144),
        Fixed::from_int(45),
        Fixed::from_int(-90),
        Fixed::from_int(-90 + i32::from(model.angle_degrees())),
        VIOLET,
        Fixed::from_int(3),
    );
    for index in 0..3 {
        let y = 91 + index as i32 * 48;
        painter.fill(
            Rect::new(24, y, 262, 38),
            if model.queue(index).is_some() {
                Color::rgb(231, 233, 226)
            } else {
                BACKGROUND
            },
            Fixed::ZERO,
        );
        painter.border(Rect::new(24, y, 262, 38), LINE, Fixed::ONE, Fixed::ZERO);
        if model.queue(index).is_some() {
            painter.fill(Rect::new(24, y, 4, 38), ORANGE, Fixed::ZERO);
        }
    }
}

fn paint_chart(
    painter: &mut PlayPainter<'_, '_>,
    model: &OrbitModel,
    rect: Rect,
    radius: bool,
    color: Color,
    max: i32,
) {
    painter.fill(rect, PANEL, Fixed::ZERO);
    painter.border(rect, LINE, Fixed::ONE, Fixed::ZERO);
    let len = model.telemetry_len();
    if len < 2 {
        return;
    }
    let mut previous = None;
    for index in 0..len {
        let Some(sample) = model.telemetry(index) else {
            continue;
        };
        let value = if radius { sample.radius } else { sample.speed };
        let point = Point {
            x: rect.x + rect.w * Fixed::from_int(index as i32) / Fixed::from_int((len - 1) as i32),
            y: rect.y + rect.h - rect.h * value.to_fixed() / Fixed::from_int(max),
        };
        if let Some(from) = previous {
            painter.line(from, point, color, Fixed::from_ratio(3, 2));
        }
        previous = Some(point);
    }
}

fn paint_record(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    paint_chart(
        painter,
        model,
        Rect::new(15, 88, 286, 74),
        true,
        ORANGE,
        160,
    );
    paint_chart(painter, model, Rect::new(15, 190, 286, 58), false, CYAN, 60);
    painter.fill(Rect::new(313, 65, 156, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(313, 65, 156, 188), LINE, Fixed::ONE, Fixed::ZERO);
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let (Some(model), Some(preview)) = (
        world.resource::<OrbitModel>(),
        world.resource::<OrbitPreview>(),
    ) else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_shell(&mut painter);
    match model.page() {
        OrbitPage::Map => paint_map(&mut painter, model, preview),
        OrbitPage::Plan => paint_plan(&mut painter, model),
        OrbitPage::Record => paint_record(&mut painter, model),
    }
}

fn modal_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if world
        .resource::<OrbitModel>()
        .is_none_or(|model| model.modal() == OrbitModal::None)
    {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(12, 20, 30, 220),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(38, 69, 404, 190), PANEL, Fixed::ZERO);
    painter.border(
        Rect::new(38, 69, 404, 190),
        INK,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(42, 73, 396, 32), INK, Fixed::ZERO);
}

fn surface_view() -> View {
    View::new("OrbitSurface", 60, surface_render).with_filter::<OrbitSurface>()
}
fn modal_view() -> View {
    View::new("OrbitModalSurface", 70, modal_render).with_filter::<OrbitModalSurface>()
}

fn footer_action(world: &mut World, index: usize) {
    match index {
        0 => OrbitNodes::update(world, OrbitModel::toggle_running),
        1 => OrbitNodes::result(world, OrbitModel::burn),
        2 => OrbitNodes::result(world, OrbitModel::scan),
        3 => OrbitNodes::result(world, OrbitModel::schedule),
        _ => {
            let mission = mission_index(world);
            OrbitNodes::update(world, |model| model.set_modal(OrbitModal::Confirm(mission)));
        }
    }
}

fn modal_action(world: &mut World, index: usize) {
    let modal = world
        .resource::<OrbitModel>()
        .map_or(OrbitModal::None, OrbitModel::modal);
    match modal {
        OrbitModal::Missions if index < 3 => OrbitNodes::update(world, |model| {
            model.set_modal(OrbitModal::Confirm(index as u8))
        }),
        OrbitModal::Missions if index == 3 => {
            OrbitNodes::update(world, |model| model.set_modal(OrbitModal::None));
        }
        OrbitModal::Confirm(_) if index == 0 => {
            OrbitNodes::update(world, |model| model.set_modal(OrbitModal::None))
        }
        OrbitModal::Confirm(mission) if index == 1 => {
            OrbitNodes::update(world, |model| model.load_mission(mission))
        }
        OrbitModal::Help => OrbitNodes::update(world, |model| model.set_modal(OrbitModal::None)),
        _ => {}
    }
}

#[mirui_macros::system(order = ANIMATION)]
fn orbit_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    OrbitNodes::update(world, |model| model.update(elapsed));
}

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

#[compose]
fn build_widgets() {
    ui! {
        OrbitSurface (id: "orbit_surface", width: 480, height: 320, clip_children: true) {
            Text (
                "轨道任务台",
                position: Position::Absolute,
                left: 24,
                top: 6,
                width: 160,
                height: 20,
                font_size: 14,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Button (
                "任务 01 · 轨道抬升",
                id: "orbit_mission",
                position: Position::Absolute,
                left: 284,
                top: 4,
                width: 158,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_modal(OrbitModal::Missions)); }
            Button (
                "?",
                position: Position::Absolute,
                left: 450,
                top: 4,
                width: 25,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 12,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_modal(OrbitModal::Help)); }
            Button (
                "航图",
                id: "orbit_tab_map",
                position: Position::Absolute,
                left: 24,
                top: 33,
                width: 90,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ORANGE,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_page(OrbitPage::Map)); }
            Button (
                "计划",
                id: "orbit_tab_plan",
                position: Position::Absolute,
                left: 119,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_page(OrbitPage::Plan)); }
            Button (
                "记录",
                id: "orbit_tab_record",
                position: Position::Absolute,
                left: 206,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_page(OrbitPage::Record)); }
            Text (
                "SIM T+0.0",
                id: "orbit_time",
                position: Position::Absolute,
                left: 356,
                top: 39,
                width: 102,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            View (
                id: "orbit_map_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "48.0 / 48",
                    id: "orbit_fuel",
                    position: Position::Absolute,
                    left: 318,
                    top: 78,
                    width: 139,
                    height: 15,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "0%",
                    id: "orbit_heat",
                    position: Position::Absolute,
                    left: 318,
                    top: 105,
                    width: 139,
                    height: 15,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "78.0",
                    id: "orbit_radius",
                    position: Position::Absolute,
                    left: 318,
                    top: 136,
                    width: 62,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "35.8",
                    id: "orbit_speed",
                    position: Position::Absolute,
                    left: 395,
                    top: 136,
                    width: 62,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "3.8",
                    id: "orbit_dv",
                    position: Position::Absolute,
                    left: 417,
                    top: 169,
                    width: 40,
                    height: 18,
                    font_size: 12,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Button (
                    "-",
                    position: Position::Absolute,
                    left: 318,
                    top: 191,
                    width: 34,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_dv(-2)); }
                Button (
                    "参考值",
                    position: Position::Absolute,
                    left: 357,
                    top: 191,
                    width: 61,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, OrbitModel::reset_dv); }
                Button (
                    "+",
                    position: Position::Absolute,
                    left: 423,
                    top: 191,
                    width: 34,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_dv(2)); }
                Button (
                    "-15",
                    position: Position::Absolute,
                    left: 318,
                    top: 222,
                    width: 58,
                    height: 23,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_angle(-15)); }
                Text (
                    "0°",
                    id: "orbit_angle",
                    position: Position::Absolute,
                    left: 377,
                    top: 226,
                    width: 36,
                    height: 13,
                    font_size: 8,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Center)
                )
                Button (
                    "+15",
                    position: Position::Absolute,
                    left: 414,
                    top: 222,
                    width: 43,
                    height: 23,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_angle(15)); }
                Button (
                    "预演 已开",
                    id: "orbit_preview",
                    position: Position::Absolute,
                    left: 18,
                    top: 231,
                    width: 78,
                    height: 18,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: SPACE,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, OrbitModel::toggle_preview); }
                Button (
                    "1×",
                    id: "orbit_warp",
                    position: Position::Absolute,
                    left: 244,
                    top: 231,
                    width: 45,
                    height: 18,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: SPACE,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, OrbitModel::cycle_warp); }
            }
            View (
                id: "orbit_plan_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "01  空节点",
                    id: "orbit_node_0",
                    position: Position::Absolute,
                    left: 35,
                    top: 103,
                    width: 198,
                    height: 17,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "取消",
                    position: Position::Absolute,
                    left: 239,
                    top: 98,
                    width: 40,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { cancel_node(ctx.world, 0); }
                Text (
                    "02  空节点",
                    id: "orbit_node_1",
                    position: Position::Absolute,
                    left: 35,
                    top: 151,
                    width: 198,
                    height: 17,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "取消",
                    position: Position::Absolute,
                    left: 239,
                    top: 146,
                    width: 40,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { cancel_node(ctx.world, 1); }
                Text (
                    "03  空节点",
                    id: "orbit_node_2",
                    position: Position::Absolute,
                    left: 35,
                    top: 199,
                    width: 198,
                    height: 17,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Button (
                    "取消",
                    position: Position::Absolute,
                    left: 239,
                    top: 194,
                    width: 40,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 7,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { cancel_node(ctx.world, 2); }
                Text (
                    "0 / 3 个节点",
                    id: "orbit_queue_count",
                    position: Position::Absolute,
                    left: 24,
                    top: 234,
                    width: 130,
                    height: 14,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "3 s",
                    id: "orbit_delay",
                    position: Position::Absolute,
                    left: 326,
                    top: 99,
                    width: 130,
                    height: 28,
                    font_size: 20,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Button (
                    "−1 s",
                    position: Position::Absolute,
                    left: 326,
                    top: 186,
                    width: 57,
                    height: 25,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: Color::rgb(31, 50, 67),
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_delay(-1)); }
                Button (
                    "＋1 s",
                    position: Position::Absolute,
                    left: 391,
                    top: 186,
                    width: 57,
                    height: 25,
                    size: ButtonSize::Compact,
                    font_size: 8,
                    normal_color: Color::rgb(31, 50, 67),
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.adjust_delay(1)); }
            }
            View (
                id: "orbit_record_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "0",
                    id: "orbit_samples",
                    position: Position::Absolute,
                    left: 325,
                    top: 92,
                    width: 45,
                    height: 20,
                    font_size: 14,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "0",
                    id: "orbit_trail",
                    position: Position::Absolute,
                    left: 392,
                    top: 92,
                    width: 45,
                    height: 20,
                    font_size: 14,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "78.0",
                    id: "orbit_record_radius",
                    position: Position::Absolute,
                    left: 325,
                    top: 134,
                    width: 112,
                    height: 18,
                    font_size: 12,
                    text_color: ORANGE,
                    paragraph: label_style()
                )
                Text (
                    "35.8",
                    id: "orbit_record_speed",
                    position: Position::Absolute,
                    left: 325,
                    top: 158,
                    width: 112,
                    height: 18,
                    font_size: 12,
                    text_color: CYAN,
                    paragraph: label_style()
                )
                Text (
                    "等待事件",
                    id: "orbit_event",
                    position: Position::Absolute,
                    left: 325,
                    top: 194,
                    width: 124,
                    height: 28,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "READY",
                    id: "orbit_status",
                    position: Position::Absolute,
                    left: 325,
                    top: 231,
                    width: 124,
                    height: 15,
                    font_size: 9,
                    text_color: ORANGE,
                    paragraph: label_style()
                )
            }
            Button (
                "▶ 滑行",
                id: "orbit_footer_run",
                position: Position::Absolute,
                left: 12,
                top: 288,
                width: 82,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ORANGE,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 0); }
            Button (
                "点火",
                id: "orbit_footer_burn",
                position: Position::Absolute,
                left: 99,
                top: 288,
                width: 70,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 1); }
            Button (
                "采样",
                id: "orbit_footer_scan",
                position: Position::Absolute,
                left: 174,
                top: 288,
                width: 91,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 2); }
            Button (
                "＋ 节点",
                id: "orbit_footer_schedule",
                position: Position::Absolute,
                left: 270,
                top: 288,
                width: 85,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 3); }
            Button (
                "重新装载",
                id: "orbit_footer_reset",
                position: Position::Absolute,
                left: 360,
                top: 288,
                width: 108,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ORANGE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 4); }
            OrbitModalSurface (
                id: "orbit_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320
            ) {
                Text (
                    "选择轨道任务",
                    id: "orbit_modal_title",
                    position: Position::Absolute,
                    left: 55,
                    top: 80,
                    width: 300,
                    height: 20,
                    font_size: 13,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "切换会清空当前轨迹、计划与遥测记录。",
                    id: "orbit_modal_subtitle",
                    position: Position::Absolute,
                    left: 55,
                    top: 116,
                    width: 360,
                    height: 28,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Button (
                    "轨道抬升",
                    id: "orbit_modal_0",
                    position: Position::Absolute,
                    left: 55,
                    top: 151,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: ORANGE,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 0); }
                Button (
                    "往返测绘",
                    id: "orbit_modal_1",
                    position: Position::Absolute,
                    left: 249,
                    top: 151,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 1); }
                Button (
                    "远端通信",
                    id: "orbit_modal_2",
                    position: Position::Absolute,
                    left: 55,
                    top: 198,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 2); }
                Button (
                    "关闭",
                    id: "orbit_modal_3",
                    position: Position::Absolute,
                    left: 249,
                    top: 198,
                    width: 176,
                    height: 35,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 3); }
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 404,
                    top: 77,
                    width: 27,
                    height: 24,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: INK,
                    pressed_color: ORANGE,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { OrbitNodes::update(ctx.world, |model| model.set_modal(OrbitModal::None)); }
            }
        }
    };
}

fn cancel_node(world: &mut World, index: usize) {
    if let Some(node) = world
        .resource::<OrbitModel>()
        .and_then(|model| model.queue(index))
    {
        OrbitNodes::result(world, |model| model.cancel(node.id));
    }
}

pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    register_play_font(&mut app.world);
    app.world.insert_resource(OrbitModel::default());
    app.world.insert_resource(OrbitPreview::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(orbit_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Orbital Mission node");
    let nodes = OrbitNodes {
        surface: find("orbit_surface"),
        mission_meta: find("orbit_mission"),
        time_meta: find("orbit_time"),
        tabs: [
            find("orbit_tab_map"),
            find("orbit_tab_plan"),
            find("orbit_tab_record"),
        ],
        pages: [
            find("orbit_map_page"),
            find("orbit_plan_page"),
            find("orbit_record_page"),
        ],
        map_values: [
            find("orbit_fuel"),
            find("orbit_heat"),
            find("orbit_radius"),
            find("orbit_speed"),
            find("orbit_dv"),
            find("orbit_angle"),
            find("orbit_preview"),
            find("orbit_warp"),
        ],
        plan_values: [
            find("orbit_delay"),
            find("orbit_node_0"),
            find("orbit_node_1"),
            find("orbit_node_2"),
            find("orbit_queue_count"),
        ],
        record_values: [
            find("orbit_samples"),
            find("orbit_trail"),
            find("orbit_record_radius"),
            find("orbit_record_speed"),
            find("orbit_event"),
            find("orbit_status"),
        ],
        footer: [
            find("orbit_footer_run"),
            find("orbit_footer_burn"),
            find("orbit_footer_scan"),
            find("orbit_footer_schedule"),
            find("orbit_footer_reset"),
        ],
        modal: find("orbit_modal"),
        modal_title: find("orbit_modal_title"),
        modal_subtitle: find("orbit_modal_subtitle"),
        modal_buttons: [
            find("orbit_modal_0"),
            find("orbit_modal_1"),
            find("orbit_modal_2"),
            find("orbit_modal_3"),
        ],
    };
    app.world.insert_resource(nodes);
    OrbitNodes::refresh_preview(&mut app.world);
    OrbitNodes::sync(&mut app.world);
}
