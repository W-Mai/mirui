extern crate alloc;

use alloc::format;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::post::{MAX_PARCELS, PostModal, PostModel, PostParcel};
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::TouchAction;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::surface::InputEvent;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use crate::ui::{ComputedRect, Hidden};

pub const VIEWPORT: (u16, u16) = (480, 320);

const BACKGROUND: Color = Color::rgb(27, 40, 45);
const HEADER: Color = Color::rgb(30, 43, 48);
const BOARD: Color = Color::rgb(20, 34, 39);
const PANEL: Color = Color::rgb(37, 55, 59);
const CONTROL: Color = Color::rgb(43, 61, 55);
const CONTROL_DISABLED: Color = Color::rgb(34, 48, 44);
const ACCENT: Color = Color::rgb(170, 208, 231);
const TEXT: Color = Color::rgb(224, 233, 218);
const MUTED: Color = Color::rgb(151, 168, 157);
const TRACK: Color = Color::rgb(112, 134, 127);
const TRACK_INACTIVE: Color = Color::rgb(53, 72, 75);
const STATION_COLORS: [Color; 3] = [
    Color::rgb(181, 221, 176),
    Color::rgb(202, 180, 237),
    Color::rgb(237, 186, 145),
];

#[derive(crate::Component, Default)]
struct PostSurface;

#[derive(crate::Component, Default)]
struct PostModalSurface;

#[derive(Clone, Copy)]
struct PostNodes {
    surface: Entity,
    status: Entity,
    sorted: Entity,
    misses: Entity,
    queue: [Entity; 5],
    in_transit: Entity,
    station_counts: [Entity; 3],
    run: Entity,
    send: Entity,
    speed: Entity,
    modal: Entity,
    modal_title: Entity,
    modal_subtitle: Entity,
    manifest_controls: [Entity; 3],
    summary_controls: [Entity; 6],
    reset_controls: [Entity; 4],
}

impl PostNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut PostModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<PostModel>()
            .map(update)
            .unwrap_or(ChangeSet::NONE);
        if changes.contains(ChangeSet::VISUAL)
            && let Some(surface) = world.resource::<Self>().map(|nodes| nodes.surface)
        {
            world.invalidate_visual(surface);
        }
        if changes.contains(ChangeSet::MODEL) || changes.contains(ChangeSet::LAYOUT) {
            Self::sync(world);
        }
    }

    fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<PostModel>() else {
            return;
        };
        let modal = model.modal();
        let running = model.running();
        let started = model.started();
        let finished = model.finished();
        let manifest_id = model.manifest_id();
        let speed_x2 = model.speed_x2();
        let delivered = model.delivered();
        let missed = model.missed();
        let streak = model.streak();
        let score = model.score();
        let manifest_len = model.manifest_len();
        let active_len = model.active_len();
        let can_send = !finished
            && modal == PostModal::None
            && model.cursor() < manifest_len
            && usize::from(active_len) < MAX_PARCELS;
        let queued = core::array::from_fn::<_, 5, _>(|index| model.queued(index));
        let station_counts = core::array::from_fn::<_, 3, _>(|index| model.arrivals(index));
        let status = if finished {
            "班次完成"
        } else if running {
            "包裹正在路上"
        } else if started {
            "已暂停调度"
        } else {
            "准备好，拨动你的第一班轨道"
        };
        let run = if running {
            "暂停"
        } else if finished {
            "再开一班"
        } else if started {
            "继续"
        } else {
            "开始"
        };
        let speed = match speed_x2 {
            1 => "0.5× 速度",
            3 => "1.5× 速度",
            _ => "1× 速度",
        };
        let texts = [
            (nodes.status, status.into()),
            (nodes.sorted, format!("SORTED {delivered} / {manifest_len}")),
            (nodes.misses, format!("错投 {missed}   连对 {streak}")),
            (nodes.in_transit, format!("{active_len} / 3 在途")),
            (nodes.run, run.into()),
            (nodes.speed, speed.into()),
        ];
        let _ = model;
        for (entity, content) in texts {
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(content);
            }
            world.invalidate(entity);
        }
        for (index, entity) in nodes.queue.into_iter().enumerate() {
            let content = queued[index].map_or("", destination_letter);
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(content);
            }
            if let Some(target) = queued[index]
                && let Some(style) = world.get_mut::<Style>(entity)
            {
                style.text_color = STATION_COLORS[usize::from(target)].into();
            }
            set_hidden(world, entity, queued[index].is_none());
        }
        for (index, entity) in nodes.station_counts.into_iter().enumerate() {
            if let Some(text) = world.get_mut::<Text>(entity) {
                text.set_content(format!("{}", station_counts[index]));
            }
            world.invalidate(entity);
        }
        set_button_state(
            world,
            nodes.run,
            running || !started,
            modal == PostModal::None,
        );
        set_button_state(world, nodes.send, false, can_send);
        set_button_state(world, nodes.speed, false, modal == PostModal::None);

        set_hidden(world, nodes.modal, modal == PostModal::None);
        let manifests_open = modal == PostModal::Manifests;
        let summary_open = modal == PostModal::Summary;
        let reset_open = modal == PostModal::Reset;
        for (index, entity) in nodes.manifest_controls.into_iter().enumerate() {
            set_hidden(world, entity, !manifests_open);
            set_button_state(world, entity, manifest_id as usize == index, manifests_open);
        }
        for entity in nodes.summary_controls {
            set_hidden(world, entity, !summary_open);
        }
        for entity in nodes.reset_controls {
            set_hidden(world, entity, !reset_open);
        }
        let (title, subtitle) = match modal {
            PostModal::Manifests => (
                "选择今天的班次",
                "更换班次会从头开始；没有时间惩罚，可以随时暂停。",
            ),
            PostModal::Summary => ("本班投递完成", "不需要抢时间，准确到达就很棒。"),
            PostModal::Reset => ("重新开始这一班？", "当前包裹、计分和道岔都会复位。"),
            PostModal::None => ("", ""),
        };
        if let Some(text) = world.get_mut::<Text>(nodes.modal_title) {
            text.set_content(title);
        }
        if let Some(text) = world.get_mut::<Text>(nodes.modal_subtitle) {
            text.set_content(subtitle);
        }
        world.invalidate(nodes.modal_title);
        world.invalidate(nodes.modal_subtitle);

        if summary_open {
            let summary_texts = [
                (nodes.summary_controls[0], format!("{score:04}")),
                (nodes.summary_controls[2], format!("正确 {delivered} 件")),
                (nodes.summary_controls[3], format!("错投 {missed} 件")),
            ];
            for (entity, content) in summary_texts {
                if let Some(text) = world.get_mut::<Text>(entity) {
                    text.set_content(content);
                }
                world.invalidate(entity);
            }
        }
    }
}

fn destination_letter(destination: u8) -> &'static str {
    match destination {
        0 => "A",
        1 => "B",
        _ => "C",
    }
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
            ACCENT.into()
        } else if enabled {
            CONTROL.into()
        } else {
            CONTROL_DISABLED.into()
        };
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.text_color = if active {
            BACKGROUND.into()
        } else if enabled {
            TEXT.into()
        } else {
            Color::rgb(103, 120, 111).into()
        };
    }
    world.invalidate_visual(entity);
}

fn paint_track(painter: &mut PlayPainter<'_, '_>, from: Point, to: Point, active: bool) {
    painter.line(from, to, Color::rgb(12, 22, 26), Fixed::from_int(12));
    painter.line(
        from,
        to,
        if active { TRACK } else { TRACK_INACTIVE },
        Fixed::from_int(8),
    );
    painter.line(
        from,
        to,
        if active {
            Color::rgb(192, 204, 192)
        } else {
            Color::rgb(86, 105, 101)
        },
        Fixed::ONE,
    );
}

fn paint_destination_shape(
    painter: &mut PlayPainter<'_, '_>,
    target: u8,
    center: Point,
    size: i32,
) {
    let color = STATION_COLORS[usize::from(target)];
    match target {
        0 => painter.circle(center, Fixed::from_int(size), color),
        1 => painter.fill(
            Rect::new(
                center.x.to_int() - size,
                center.y.to_int() - size,
                size * 2,
                size * 2,
            ),
            color,
            Fixed::from_int(2),
        ),
        _ => {
            let top = Point::new(center.x, center.y - Fixed::from_int(size + 1));
            let left = Point::new(
                center.x - Fixed::from_int(size + 1),
                center.y + Fixed::from_int(size),
            );
            let right = Point::new(
                center.x + Fixed::from_int(size + 1),
                center.y + Fixed::from_int(size),
            );
            painter.line(top, left, color, Fixed::from_int(2));
            painter.line(left, right, color, Fixed::from_int(2));
            painter.line(right, top, color, Fixed::from_int(2));
        }
    }
}

fn paint_letter(painter: &mut PlayPainter<'_, '_>, target: u8, center: Point) {
    let color = STATION_COLORS[usize::from(target)];
    let x = center.x;
    let y = center.y;
    match target {
        0 => {
            painter.line(
                Point::new(x - Fixed::from_int(3), y + Fixed::from_int(3)),
                Point::new(x, y - Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x, y - Fixed::from_int(3)),
                Point::new(x + Fixed::from_int(3), y + Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x - Fixed::from_int(2), y + Fixed::ONE),
                Point::new(x + Fixed::from_int(2), y + Fixed::ONE),
                color,
                Fixed::ONE,
            );
        }
        1 => {
            painter.line(
                Point::new(x - Fixed::from_int(2), y - Fixed::from_int(3)),
                Point::new(x - Fixed::from_int(2), y + Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
            for offset in [-2, 1] {
                painter.line(
                    Point::new(x - Fixed::from_int(2), y + Fixed::from_int(offset)),
                    Point::new(x + Fixed::from_int(2), y + Fixed::from_int(offset)),
                    color,
                    Fixed::ONE,
                );
            }
            painter.line(
                Point::new(x + Fixed::from_int(2), y - Fixed::from_int(2)),
                Point::new(x + Fixed::from_int(2), y + Fixed::from_int(2)),
                color,
                Fixed::ONE,
            );
        }
        _ => {
            painter.line(
                Point::new(x + Fixed::from_int(2), y - Fixed::from_int(3)),
                Point::new(x - Fixed::from_int(2), y - Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x - Fixed::from_int(2), y - Fixed::from_int(3)),
                Point::new(x - Fixed::from_int(2), y + Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x - Fixed::from_int(2), y + Fixed::from_int(3)),
                Point::new(x + Fixed::from_int(2), y + Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
        }
    }
}

fn paint_parcel(painter: &mut PlayPainter<'_, '_>, parcel: PostParcel) {
    let center = parcel.position();
    let color = STATION_COLORS[usize::from(parcel.target())];
    painter.fill(
        Rect {
            x: center.x - Fixed::from_int(9),
            y: center.y - Fixed::from_int(9),
            w: Fixed::from_int(18),
            h: Fixed::from_int(18),
        },
        Color::rgb(23, 35, 38),
        Fixed::from_int(4),
    );
    painter.border(
        Rect {
            x: center.x - Fixed::from_int(9),
            y: center.y - Fixed::from_int(9),
            w: Fixed::from_int(18),
            h: Fixed::from_int(18),
        },
        color,
        Fixed::from_ratio(3, 2),
        Fixed::from_int(4),
    );
    paint_destination_shape(painter, parcel.target(), center, 3);
    paint_letter(
        painter,
        parcel.target(),
        Point::new(center.x, center.y - Fixed::from_int(15)),
    );
}

fn paint_post_surface(painter: &mut PlayPainter<'_, '_>, model: &PostModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 35), HEADER, Fixed::ZERO);
    painter.line(
        Point::new(12, 34),
        Point::new(468, 34),
        Color::rgb(48, 65, 66),
        Fixed::ONE,
    );
    for (from, to) in [
        (Point::new(20, 8), Point::new(26, 14)),
        (Point::new(26, 14), Point::new(20, 20)),
        (Point::new(20, 20), Point::new(14, 14)),
        (Point::new(14, 14), Point::new(20, 8)),
    ] {
        painter.line(from, to, ACCENT, Fixed::from_int(3));
    }
    painter.fill(Rect::new(12, 62, 456, 207), BOARD, Fixed::from_int(9));
    painter.border(
        Rect::new(12, 62, 456, 207),
        Color::rgb(53, 73, 76),
        Fixed::ONE,
        Fixed::from_int(9),
    );
    for x in (26..460).step_by(22) {
        for y in (77..260).step_by(22) {
            painter.circle(
                Point::new(x, y),
                Fixed::from_ratio(2, 3),
                Color::rgb(51, 68, 74),
            );
        }
    }
    paint_track(painter, Point::new(32, 157), Point::new(151, 157), true);
    paint_track(
        painter,
        Point::new(151, 157),
        Point::new(265, 157),
        model.switch(0) == 1,
    );
    paint_track(
        painter,
        Point::new(151, 157),
        Point::new(195, 89),
        model.switch(0) == 0,
    );
    paint_track(
        painter,
        Point::new(195, 89),
        Point::new(400, 89),
        model.switch(0) == 0,
    );
    paint_track(
        painter,
        Point::new(265, 157),
        Point::new(400, 157),
        model.switch(1) == 0,
    );
    paint_track(
        painter,
        Point::new(265, 157),
        Point::new(306, 225),
        model.switch(1) == 1,
    );
    paint_track(
        painter,
        Point::new(306, 225),
        Point::new(400, 225),
        model.switch(1) == 1,
    );
    painter.fill(
        Rect::new(17, 135, 32, 43),
        Color::rgb(52, 68, 73),
        Fixed::from_int(6),
    );
    painter.border(
        Rect::new(17, 135, 32, 43),
        Color::rgb(108, 129, 128),
        Fixed::ONE,
        Fixed::from_int(6),
    );
    painter.line(
        Point::new(26, 161),
        Point::new(39, 161),
        TEXT,
        Fixed::from_ratio(3, 2),
    );
    painter.line(
        Point::new(35, 157),
        Point::new(39, 161),
        TEXT,
        Fixed::from_ratio(3, 2),
    );
    painter.line(
        Point::new(39, 161),
        Point::new(35, 165),
        TEXT,
        Fixed::from_ratio(3, 2),
    );
    for station in 0..3 {
        let y = [89, 157, 225][station];
        let color = STATION_COLORS[station];
        painter.fill(
            Rect::new(365, y - 19, 89, 38),
            if model.station_flashing(station) {
                Color::rgb(60, 82, 80)
            } else {
                PANEL
            },
            Fixed::from_int(6),
        );
        painter.border(
            Rect::new(365, y - 19, 89, 38),
            color,
            Fixed::ONE,
            Fixed::from_int(6),
        );
        paint_destination_shape(painter, station as u8, Point::new(380, y), 5);
    }
    for switch in 0..2 {
        let x = [151, 265][switch];
        painter.circle(
            Point::new(x, 157),
            Fixed::from_int(18),
            Color::rgb(38, 56, 61),
        );
        painter.border(
            Rect::new(x - 18, 139, 36, 36),
            ACCENT,
            Fixed::ONE,
            Fixed::from_int(18),
        );
        painter.circle(
            Point::new(x, 157),
            Fixed::from_int(12),
            Color::rgb(52, 73, 81),
        );
        let branch = model.switch(switch);
        let end = if switch == 0 && branch == 0 {
            Point::new(x + 5, 149)
        } else if switch == 1 && branch == 1 {
            Point::new(x + 5, 165)
        } else {
            Point::new(x + 8, 157)
        };
        painter.line(
            Point::new(x - 8, 157),
            Point::new(x - 1, 157),
            TEXT,
            Fixed::from_int(2),
        );
        painter.line(Point::new(x - 1, 157), end, TEXT, Fixed::from_int(2));
    }
    for index in 0..usize::from(model.active_len()) {
        if let Some(parcel) = model.active(index) {
            paint_parcel(painter, parcel);
        }
    }
    if !model.started()
        && let Some(target) = model.queued(0)
    {
        painter.fill(
            Rect::new(62, 146, 22, 22),
            Color::rgb(34, 55, 55),
            Fixed::from_int(4),
        );
        paint_destination_shape(painter, target, Point::new(73, 157), 5);
    }
    for index in 0..5 {
        if model.queued(index).is_some() {
            painter.fill(
                Rect::new(46 + index as i32 * 25, 239, 19, 18),
                Color::rgb(39, 58, 60),
                Fixed::from_int(4),
            );
        }
    }
    painter.fill(
        Rect::new(0, 282, 480, 38),
        Color::rgb(16, 32, 25),
        Fixed::ZERO,
    );
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<PostModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_post_surface(&mut painter, model);
}

fn modal_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<PostModel>() else {
        return;
    };
    if model.modal() == PostModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(7, 13, 15, 226),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(17, 65, 446, 206), HEADER, Fixed::from_int(10));
    painter.border(
        Rect::new(17, 65, 446, 206),
        ACCENT,
        Fixed::ONE,
        Fixed::from_int(10),
    );
    if model.modal() == PostModal::Summary {
        painter.line(
            Point::new(239, 113),
            Point::new(239, 192),
            Color::rgb(54, 72, 74),
            Fixed::ONE,
        );
    } else if model.modal() == PostModal::Reset {
        painter.fill(Rect::new(49, 124, 96, 96), BOARD, Fixed::from_int(48));
        painter.circle(Point::new(97, 172), Fixed::from_int(8), ACCENT);
    }
}

fn surface_view() -> View {
    View::new("PostSurface", 60, surface_render).with_filter::<PostSurface>()
}

fn modal_view() -> View {
    View::new("PostModalSurface", 70, modal_render).with_filter::<PostModalSurface>()
}

fn local_point(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<Point> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    Some(Point {
        x: (x - rect.x) * Fixed::from_int(480) / rect.w,
        y: (y - rect.y) * Fixed::from_int(320) / rect.h,
    })
}

fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let GestureEvent::Tap { x, y, .. } = event else {
        return false;
    };
    let Some(point) = local_point(world, entity, *x, *y) else {
        return false;
    };
    for (index, center_x) in [151, 265].into_iter().enumerate() {
        let dx = point.x - Fixed::from_int(center_x);
        let dy = point.y - Fixed::from_int(157);
        if dx * dx + dy * dy <= Fixed::from_int(23) * Fixed::from_int(23) {
            PostNodes::update(world, |model| model.toggle_switch(index));
            return true;
        }
    }
    false
}

#[mirui_macros::system(order = ANIMATION)]
fn post_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    PostNodes::update(world, |model| model.advance_ms(elapsed));
}

struct PostKeyboardPlugin;

impl<B, F> Plugin<B, F> for PostKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        match ch {
            'a' | 'A' => PostNodes::update(world, |model| model.toggle_switch(0)),
            's' | 'S' => PostNodes::update(world, |model| model.toggle_switch(1)),
            ' ' => PostNodes::update(world, PostModel::toggle_running),
            _ => return false,
        }
        true
    }
}

#[compose]
fn build_widgets() {
    ui! {
        PostSurface (id: "post_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "POCKET POST",
                position: Position::Absolute,
                left: 35,
                top: 7,
                width: 240,
                height: 22,
                font_size: 14,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "SORTED 0 / 9",
                id: "post_sorted",
                position: Position::Absolute,
                left: 306,
                top: 9,
                width: 158,
                height: 18,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "准备好，拨动你的第一班轨道",
                id: "post_status",
                position: Position::Absolute,
                left: 16,
                top: 42,
                width: 282,
                height: 17,
                font_size: 10,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "错投 0   连对 0",
                id: "post_misses",
                position: Position::Absolute,
                left: 304,
                top: 43,
                width: 160,
                height: 15,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "IN",
                position: Position::Absolute,
                left: 19,
                top: 142,
                width: 28,
                height: 13,
                font_size: 8,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "S1",
                position: Position::Absolute,
                left: 137,
                top: 181,
                width: 28,
                height: 14,
                font_size: 8,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "S2",
                position: Position::Absolute,
                left: 251,
                top: 181,
                width: 28,
                height: 14,
                font_size: 8,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "A",
                position: Position::Absolute,
                left: 393,
                top: 75,
                width: 18,
                height: 14,
                font_size: 9,
                text_color: STATION_COLORS[0],
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "薄荷港",
                position: Position::Absolute,
                left: 393,
                top: 90,
                width: 48,
                height: 12,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "0",
                id: "post_station_0",
                position: Position::Absolute,
                left: 432,
                top: 80,
                width: 15,
                height: 18,
                font_size: 13,
                text_color: STATION_COLORS[0],
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "B",
                position: Position::Absolute,
                left: 393,
                top: 143,
                width: 18,
                height: 14,
                font_size: 9,
                text_color: STATION_COLORS[1],
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "紫藤站",
                position: Position::Absolute,
                left: 393,
                top: 158,
                width: 48,
                height: 12,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "0",
                id: "post_station_1",
                position: Position::Absolute,
                left: 432,
                top: 148,
                width: 15,
                height: 18,
                font_size: 13,
                text_color: STATION_COLORS[1],
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "C",
                position: Position::Absolute,
                left: 393,
                top: 211,
                width: 18,
                height: 14,
                font_size: 9,
                text_color: STATION_COLORS[2],
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "日落湾",
                position: Position::Absolute,
                left: 393,
                top: 226,
                width: 48,
                height: 12,
                font_size: 7,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "0",
                id: "post_station_2",
                position: Position::Absolute,
                left: 432,
                top: 216,
                width: 15,
                height: 18,
                font_size: 13,
                text_color: STATION_COLORS[2],
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "待发",
                position: Position::Absolute,
                left: 23,
                top: 243,
                width: 28,
                height: 13,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "",
                id: "post_queue_0",
                position: Position::Absolute,
                left: 47,
                top: 241,
                width: 17,
                height: 15,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "",
                id: "post_queue_1",
                position: Position::Absolute,
                left: 72,
                top: 241,
                width: 17,
                height: 15,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "",
                id: "post_queue_2",
                position: Position::Absolute,
                left: 97,
                top: 241,
                width: 17,
                height: 15,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "",
                id: "post_queue_3",
                position: Position::Absolute,
                left: 122,
                top: 241,
                width: 17,
                height: 15,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "",
                id: "post_queue_4",
                position: Position::Absolute,
                left: 147,
                top: 241,
                width: 17,
                height: 15,
                font_size: 8,
                text_color: TEXT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "0 / 3 在途",
                id: "post_in_transit",
                position: Position::Absolute,
                left: 218,
                top: 243,
                width: 86,
                height: 13,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Button (
                "班次",
                position: Position::Absolute,
                left: 387,
                top: 242,
                width: 66,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: Color::rgb(49, 72, 76),
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { PostNodes::update(ctx.world, PostModel::open_manifests); }
            Button (
                "开始",
                id: "post_run",
                position: Position::Absolute,
                left: 12,
                top: 288,
                width: 129,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACCENT,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 7
            ) on Tap { PostNodes::update(ctx.world, PostModel::toggle_running); }
            Button (
                "加发一件",
                id: "post_send",
                position: Position::Absolute,
                left: 148,
                top: 288,
                width: 113,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PostNodes::update(ctx.world, PostModel::spawn_manual); }
            Button (
                "1× 速度",
                id: "post_speed",
                position: Position::Absolute,
                left: 268,
                top: 288,
                width: 95,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PostNodes::update(ctx.world, PostModel::cycle_speed); }
            Button (
                "重来",
                position: Position::Absolute,
                left: 370,
                top: 288,
                width: 98,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                text_color: TEXT,
                border_radius: 7
            ) on Tap { PostNodes::update(ctx.world, PostModel::open_reset); }
            PostModalSurface (
                id: "post_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                clip_children: true
            ) [
                TouchAction::None,
            ] on Tap { }
            {
                Text (
                    "",
                    id: "post_modal_title",
                    position: Position::Absolute,
                    left: 29,
                    top: 76,
                    width: 390,
                    height: 22,
                    font_size: 13,
                    text_color: TEXT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "",
                    id: "post_modal_subtitle",
                    position: Position::Absolute,
                    left: 29,
                    top: 96,
                    width: 410,
                    height: 15,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 428,
                    top: 73,
                    width: 24,
                    height: 22,
                    size: ButtonSize::Compact,
                    font_size: 12,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 6
                ) on Tap { PostNodes::update(ctx.world, PostModel::close_modal); }
                Button (
                    "晨间邮路 · 9 件",
                    id: "post_manifest_0",
                    position: Position::Absolute,
                    left: 29,
                    top: 113,
                    width: 420,
                    height: 37,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, |model| model.load_manifest(0)); }
                Button (
                    "忙碌午后 · 12 件",
                    id: "post_manifest_1",
                    position: Position::Absolute,
                    left: 29,
                    top: 160,
                    width: 420,
                    height: 37,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, |model| model.load_manifest(1)); }
                Button (
                    "慢慢练习 · 6 件",
                    id: "post_manifest_2",
                    position: Position::Absolute,
                    left: 29,
                    top: 207,
                    width: 420,
                    height: 37,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, |model| model.load_manifest(2)); }
                Text (
                    "0000",
                    id: "post_summary_score",
                    position: Position::Absolute,
                    left: 42,
                    top: 119,
                    width: 175,
                    height: 48,
                    font_size: 35,
                    text_color: ACCENT,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "POST POINTS",
                    id: "post_summary_points",
                    position: Position::Absolute,
                    left: 44,
                    top: 166,
                    width: 130,
                    height: 13,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "正确 0 件",
                    id: "post_summary_correct",
                    position: Position::Absolute,
                    left: 270,
                    top: 121,
                    width: 150,
                    height: 22,
                    font_size: 14,
                    text_color: STATION_COLORS[0],
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "错投 0 件",
                    id: "post_summary_missed",
                    position: Position::Absolute,
                    left: 270,
                    top: 150,
                    width: 150,
                    height: 20,
                    font_size: 12,
                    text_color: STATION_COLORS[2],
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "再开一班",
                    id: "post_summary_again",
                    position: Position::Absolute,
                    left: 29,
                    top: 217,
                    width: 204,
                    height: 34,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ACCENT,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, PostModel::restart); }
                Button (
                    "换个班次",
                    id: "post_summary_choose",
                    position: Position::Absolute,
                    left: 245,
                    top: 217,
                    width: 204,
                    height: 34,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, PostModel::open_manifests); }
                Text (
                    "只影响当前内存中的进度。",
                    id: "post_reset_note",
                    position: Position::Absolute,
                    left: 171,
                    top: 146,
                    width: 257,
                    height: 16,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Button (
                    "重新开始",
                    id: "post_reset_confirm",
                    position: Position::Absolute,
                    left: 171,
                    top: 185,
                    width: 257,
                    height: 32,
                    size: ButtonSize::Compact,
                    font_size: 11,
                    normal_color: ACCENT,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, PostModel::restart); }
                Button (
                    "保留班次",
                    id: "post_reset_cancel",
                    position: Position::Absolute,
                    left: 171,
                    top: 225,
                    width: 257,
                    height: 25,
                    size: ButtonSize::Compact,
                    font_size: 10,
                    normal_color: CONTROL,
                    pressed_color: ACCENT,
                    text_color: TEXT,
                    border_radius: 7
                ) on Tap { PostNodes::update(ctx.world, PostModel::close_modal); }
                Text (
                    "当前班次",
                    id: "post_reset_badge",
                    position: Position::Absolute,
                    left: 67,
                    top: 226,
                    width: 61,
                    height: 14,
                    font_size: 8,
                    text_color: ACCENT,
                    paragraph: ParagraphStyle::label()
                )
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
    app.add_plugin(PostKeyboardPlugin);
    register_play_font(&mut app.world);
    app.world.insert_resource(PostModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(post_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Pocket Post node");
    let nodes = PostNodes {
        surface: find("post_surface"),
        status: find("post_status"),
        sorted: find("post_sorted"),
        misses: find("post_misses"),
        queue: [
            find("post_queue_0"),
            find("post_queue_1"),
            find("post_queue_2"),
            find("post_queue_3"),
            find("post_queue_4"),
        ],
        in_transit: find("post_in_transit"),
        station_counts: [
            find("post_station_0"),
            find("post_station_1"),
            find("post_station_2"),
        ],
        run: find("post_run"),
        send: find("post_send"),
        speed: find("post_speed"),
        modal: find("post_modal"),
        modal_title: find("post_modal_title"),
        modal_subtitle: find("post_modal_subtitle"),
        manifest_controls: [
            find("post_manifest_0"),
            find("post_manifest_1"),
            find("post_manifest_2"),
        ],
        summary_controls: [
            find("post_summary_score"),
            find("post_summary_points"),
            find("post_summary_correct"),
            find("post_summary_missed"),
            find("post_summary_again"),
            find("post_summary_choose"),
        ],
        reset_controls: [
            find("post_reset_note"),
            find("post_reset_confirm"),
            find("post_reset_cancel"),
            find("post_reset_badge"),
        ],
    };
    app.world.insert_resource(nodes);
    PostNodes::sync(&mut app.world);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_uses_one_dense_route_surface_and_semantic_controls() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        assert!(app.world.find_by_id("post_surface").is_some());
        assert!(app.world.find_by_id("post_run").is_some());
        assert!(app.world.find_by_id("post_manifest_2").is_some());
        assert_eq!(app.world.query::<PostSurface>().iter().count(), 1);
    }

    #[test]
    fn switch_hit_targets_map_through_the_surface_geometry() {
        let mut app = App::headless(960, 640);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        app.systems.run_all(&mut app.world);
        let surface = app.world.find_by_id("post_surface").unwrap();
        app.world
            .insert(surface, ComputedRect(Rect::new(20, 30, 720, 480)));
        let rect = app.world.get::<ComputedRect>(surface).unwrap().0;
        let x = rect.x + rect.w * Fixed::from_int(151) / Fixed::from_int(480);
        let y = rect.y + rect.h * Fixed::from_int(157) / Fixed::from_int(320);
        assert_eq!(app.world.resource::<PostModel>().unwrap().switch(0), 0);
        assert!(surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::Tap {
                x,
                y,
                target: surface,
            },
        ));
        assert_eq!(app.world.resource::<PostModel>().unwrap().switch(0), 1);
    }
}
