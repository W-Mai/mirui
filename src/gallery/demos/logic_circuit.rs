extern crate alloc;

use alloc::format;

#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::ecs::DeltaTimeMs;
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::circuit::{
    CircuitError, CircuitModal, CircuitModel, CircuitPage, GateKind, MAX_GATES, SignalSource,
};
use crate::gallery::play::font::register_play_font;
use crate::gallery::play::paint::PlayPainter;
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use crate::ui::{ComputedRect, Hidden};

pub const VIEWPORT: (u16, u16) = (480, 320);

const INK: Color = Color::rgb(32, 41, 50);
const BACKGROUND: Color = Color::rgb(237, 238, 232);
const PANEL: Color = Color::rgb(252, 252, 246);
const WORK: Color = Color::rgb(229, 233, 227);
const LINE: Color = Color::rgb(201, 207, 205);
const MUTED: Color = Color::rgb(116, 128, 138);
const ACCENT: Color = Color::rgb(245, 107, 56);
const SIGNAL_OFF: Color = Color::rgb(139, 154, 155);
const SIGNAL_ON: Color = ACCENT;
const GREEN: Color = Color::rgb(102, 130, 86);
const RED: Color = Color::rgb(189, 83, 72);

#[derive(crate::Component, Default)]
struct CircuitSurface;

#[derive(crate::Component, Default)]
struct CircuitModalSurface;

#[derive(Clone, Copy)]
struct CircuitNodes {
    surface: Entity,
    gate_count: Entity,
    task: Entity,
    tabs: [Entity; 3],
    pages: [Entity; 3],
    inputs: [Entity; 3],
    gates: [Entity; MAX_GATES],
    output: Entity,
    live_code: Entity,
    wire_status: Entity,
    live_rows: [Entity; 8],
    truth_rows: [Entity; 8],
    truth_title: Entity,
    truth_desc: Entity,
    trace_labels: [Entity; 4],
    trace_mode: Entity,
    trace_count: Entity,
    footer: [Entity; 5],
    modal: Entity,
    modal_title: Entity,
    modal_subtitle: Entity,
    modal_buttons: [Entity; 6],
}

impl CircuitNodes {
    fn update(world: &mut World, update: impl FnOnce(&mut CircuitModel) -> ChangeSet) {
        let changes = world
            .resource_mut::<CircuitModel>()
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

    fn result(
        world: &mut World,
        update: impl FnOnce(&mut CircuitModel) -> Result<ChangeSet, CircuitError>,
    ) {
        Self::update(world, |model| update(model).unwrap_or(ChangeSet::VISUAL));
    }

    fn sync(world: &mut World) {
        let Some(nodes) = world.resource::<Self>().copied() else {
            return;
        };
        let Some(model) = world.resource::<CircuitModel>() else {
            return;
        };
        let page = model.page();
        let modal = model.modal();
        let task = model.task();
        let gate_len = model.gate_len();
        let selected = model.selected();
        let pending = model.pending();
        let disconnecting = model.disconnecting();
        let scanning = model.scanning();
        let history_len = model.history_len();
        let trace_len = model.trace_len();
        let input_count = model.input_count();
        let input_values = [model.input(0), model.input(1), model.input(2)];
        let evaluation = model.evaluation();
        let verify = model.verify_result();
        let task_name = model.task_name();
        let target_code = model.target_code();
        let task_description = model.task_description();
        let gates = core::array::from_fn::<_, MAX_GATES, _>(|index| model.gate(index));
        let positions = core::array::from_fn::<_, MAX_GATES, _>(|index| {
            gates[index].and_then(|gate| model.visual_gate_position(gate.id))
        });
        let rows = core::array::from_fn::<_, 8, _>(|index| model.truth_row(index as u8));
        let _ = model;

        set_text(world, nodes.gate_count, format!("NET / {gate_len}:6 GATES"));
        set_text(
            world,
            nodes.task,
            format!("任务 0{} · {task_name}", task + 1),
        );
        for (index, entity) in nodes.tabs.into_iter().enumerate() {
            set_button_state(world, entity, index == page_index(page), true);
        }
        for (index, entity) in nodes.pages.into_iter().enumerate() {
            set_hidden(world, entity, index != page_index(page));
        }
        for (index, entity) in nodes.inputs.into_iter().enumerate() {
            let visible = index < usize::from(input_count);
            set_hidden(world, entity, !visible);
            if visible {
                let on = input_values[index];
                set_text(
                    world,
                    entity,
                    format!("{} {}", (b'A' + index as u8) as char, u8::from(on)),
                );
                set_button_state(world, entity, on, true);
            }
        }
        for (index, entity) in nodes.gates.into_iter().enumerate() {
            let Some(gate) = gates[index] else {
                set_hidden(world, entity, true);
                continue;
            };
            set_hidden(world, entity, false);
            set_text(
                world,
                entity,
                format!("G{}\n{}", gate.id, gate.kind.label()),
            );
            if let Some((x, y)) = positions[index]
                && let Some(style) = world.get_mut::<Style>(entity)
            {
                style.layout.left = Dimension::px(i32::from(x) - 29);
                style.layout.top = Dimension::px(i32::from(y) - 14);
                style.text_color = if gate.id == selected {
                    ACCENT.into()
                } else {
                    INK.into()
                };
            }
            world.invalidate(entity);
        }
        set_text(
            world,
            nodes.output,
            if evaluation.complete {
                format!("Y\n{}", u8::from(evaluation.value))
            } else {
                "Y\n?".into()
            },
        );
        set_text(world, nodes.live_code, target_code);
        let wire_status = if let Some(result) = verify {
            if result.won {
                "✓ 逻辑验证通过，可以选择下一项任务。".into()
            } else {
                format!("通过 {}/{} 组，请检查真值表。", result.passed, result.total)
            }
        } else if disconnecting {
            "断线模式：点输入端取消连线。".into()
        } else if pending != SignalSource::None {
            "正在接线：选择一个输入端。".into()
        } else {
            "点输出端，再点输入端；拖动模块改变位置。".into()
        };
        set_text(world, nodes.wire_status, wire_status);
        for (index, entity) in nodes.live_rows.into_iter().enumerate() {
            if let Some(row) = rows[index] {
                set_hidden(world, entity, false);
                set_text(
                    world,
                    entity,
                    compact_truth_row(
                        row.inputs,
                        input_count,
                        row.actual,
                        row.expected,
                        row.passes(),
                    ),
                );
                if let Some(style) = world.get_mut::<Style>(entity) {
                    let step = if input_count == 2 { 30 } else { 16 };
                    style.layout.top = Dimension::px(115 + index as i32 * step);
                }
            } else {
                set_hidden(world, entity, true);
            }
        }
        set_text(world, nodes.truth_title, task_name);
        set_text(world, nodes.truth_desc, task_description);
        for (index, entity) in nodes.truth_rows.into_iter().enumerate() {
            if let Some(row) = rows[index] {
                set_hidden(world, entity, false);
                set_text(
                    world,
                    entity,
                    expanded_truth_row(
                        row.inputs,
                        input_count,
                        row.expected,
                        row.actual,
                        row.passes(),
                    ),
                );
            } else {
                set_hidden(world, entity, true);
            }
        }
        for (index, entity) in nodes.trace_labels.into_iter().enumerate() {
            set_hidden(
                world,
                entity,
                index >= usize::from(input_count) && index != 3,
            );
            let channel = if index == 3 {
                usize::from(input_count)
            } else {
                index
            };
            let channels = usize::from(input_count) + 1;
            if let Some(style) = world.get_mut::<Style>(entity) {
                style.layout.top = Dimension::px(83 + channel as i32 * (150 / channels as i32));
            }
        }
        set_text(
            world,
            nodes.trace_mode,
            if scanning {
                "AUTO / ON"
            } else {
                "STEP / READY"
            },
        );
        set_text(world, nodes.trace_count, format!("{trace_len} / 32"));

        let footer_labels = if page == CircuitPage::Trace {
            [
                "单步",
                if scanning { "暂停" } else { "扫描" },
                "清空记录",
                "验证",
                "返回布线",
            ]
        } else {
            [
                "＋ 逻辑门",
                "类型 / 删除",
                if disconnecting {
                    "取消断线"
                } else {
                    "断开连线"
                },
                "撤销",
                "✓ 验证",
            ]
        };
        for (index, entity) in nodes.footer.into_iter().enumerate() {
            set_text(world, entity, footer_labels[index]);
            let enabled = if page == CircuitPage::Trace {
                index != 2 || trace_len != 0
            } else {
                match index {
                    0 => usize::from(gate_len) < MAX_GATES,
                    1 => selected != 0,
                    3 => history_len != 0,
                    _ => true,
                }
            };
            let active = (page == CircuitPage::Trace && index == 1 && scanning)
                || (page != CircuitPage::Trace && index == 2 && disconnecting)
                || index == 4;
            set_button_state(world, entity, active, enabled);
        }

        set_hidden(world, nodes.modal, modal == CircuitModal::None);
        let (title, subtitle) = match modal {
            CircuitModal::None => ("", ""),
            CircuitModal::Tasks => ("选择逻辑任务", "切换会清空当前网络、时序和撤销记录。"),
            CircuitModal::GateTypes { adding: true } => {
                ("添加逻辑门", "选择一个组合逻辑门；最多放置 6 个。")
            }
            CircuitModal::GateTypes { adding: false } => (
                "修改逻辑门",
                "改变类型会保留可用输入；NOT 只保留第一个输入。",
            ),
            CircuitModal::Help => (
                "逻辑工作台 / 操作手册",
                "输出端 → 输入端接线。真值表枚举全部输入；时序页记录离散状态。",
            ),
        };
        set_text(world, nodes.modal_title, title);
        set_text(world, nodes.modal_subtitle, subtitle);
        for (index, entity) in nodes.modal_buttons.into_iter().enumerate() {
            let (label, visible, active) = match modal {
                CircuitModal::Tasks => {
                    let task_index = index / 2;
                    let reference = index % 2 == 1;
                    (
                        if reference {
                            "示范布局".into()
                        } else {
                            format!("任务 0{}", task_index + 1)
                        },
                        task_index < 3,
                        !reference && task_index == usize::from(task),
                    )
                }
                CircuitModal::GateTypes { adding } => {
                    if index < GateKind::ALL.len() {
                        (GateKind::ALL[index].label().into(), true, false)
                    } else {
                        ("删除选中门".into(), !adding, false)
                    }
                }
                CircuitModal::Help => (
                    if index == 0 {
                        "明白了".into()
                    } else {
                        "".into()
                    },
                    index == 0,
                    index == 0,
                ),
                CircuitModal::None => ("".into(), false, false),
            };
            set_hidden(world, entity, !visible);
            if visible {
                set_text(world, entity, label);
                set_button_state(world, entity, active, true);
            }
        }
    }
}

fn page_index(page: CircuitPage) -> usize {
    match page {
        CircuitPage::Wire => 0,
        CircuitPage::Truth => 1,
        CircuitPage::Trace => 2,
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
            ACCENT.into()
        } else if enabled {
            INK.into()
        } else {
            LINE.into()
        };
        button.pressed_color = ACCENT.into();
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

fn compact_truth_row(
    inputs: u8,
    count: u8,
    actual: bool,
    expected: bool,
    pass: bool,
) -> alloc::string::String {
    let a = u8::from(inputs & 1 != 0);
    let b = u8::from(inputs & 2 != 0);
    let c = u8::from(inputs & 4 != 0);
    if count == 2 {
        format!(
            "{a}  {b}       {} / {}   {}",
            u8::from(actual),
            u8::from(expected),
            if pass { "✓" } else { "·" }
        )
    } else {
        format!(
            "{a} {b} {c}     {} / {}   {}",
            u8::from(actual),
            u8::from(expected),
            if pass { "✓" } else { "·" }
        )
    }
}

fn expanded_truth_row(
    inputs: u8,
    count: u8,
    expected: bool,
    actual: bool,
    pass: bool,
) -> alloc::string::String {
    let a = u8::from(inputs & 1 != 0);
    let b = u8::from(inputs & 2 != 0);
    let c = u8::from(inputs & 4 != 0);
    if count == 2 {
        format!(
            "{a}      {b}          {}          {}        {}",
            u8::from(expected),
            u8::from(actual),
            if pass { "PASS" } else { "FAIL" }
        )
    } else {
        format!(
            "{a}   {b}   {c}       {}          {}       {}",
            u8::from(expected),
            u8::from(actual),
            if pass { "PASS" } else { "FAIL" }
        )
    }
}

fn signal_position(model: &CircuitModel, source: SignalSource) -> Option<Point> {
    match source {
        SignalSource::None => None,
        SignalSource::Input(index) if index < model.input_count() => {
            Some(Point::new(54, 100 + i32::from(index) * 54))
        }
        SignalSource::Gate(id) => model
            .visual_gate_position(id)
            .map(|(x, y)| Point::new(i32::from(x) + 31, i32::from(y))),
        _ => None,
    }
}

fn gate_input_position(model: &CircuitModel, gate_id: u8, pin: u8) -> Option<Point> {
    let gate = model.gate_by_id(gate_id)?;
    let (x, y) = model.visual_gate_position(gate_id)?;
    let dy = if gate.kind == GateKind::Not {
        0
    } else if pin == 0 {
        -9
    } else {
        9
    };
    Some(Point::new(i32::from(x) - 31, i32::from(y) + dy))
}

fn draw_wire(painter: &mut PlayPainter<'_, '_>, from: Point, to: Point, on: bool) {
    let color = if on { SIGNAL_ON } else { SIGNAL_OFF };
    let width = if on {
        Fixed::from_ratio(3, 2)
    } else {
        Fixed::ONE
    };
    let middle = (from.x + to.x) / Fixed::from_int(2);
    painter.line(
        from,
        Point {
            x: middle,
            y: from.y,
        },
        color,
        width,
    );
    painter.line(
        Point {
            x: middle,
            y: from.y,
        },
        Point { x: middle, y: to.y },
        color,
        width,
    );
    painter.line(Point { x: middle, y: to.y }, to, color, width);
}

fn paint_shell(painter: &mut PlayPainter<'_, '_>) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 31), INK, Fixed::ZERO);
    painter.fill(
        Rect::new(0, 31, 480, 28),
        Color::rgb(226, 231, 226),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(0, 282, 480, 38), INK, Fixed::ZERO);
    painter.fill(Rect::new(11, 65, 305, 188), WORK, Fixed::ZERO);
    painter.border(Rect::new(11, 65, 305, 188), LINE, Fixed::ONE, Fixed::ZERO);
    painter.fill(Rect::new(324, 66, 145, 186), PANEL, Fixed::ZERO);
    painter.border(Rect::new(324, 66, 145, 186), LINE, Fixed::ONE, Fixed::ZERO);
}

fn paint_wire_page(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    for x in (20..313).step_by(12) {
        for y in (74..252).step_by(12) {
            painter.circle(
                Point::new(x, y),
                Fixed::from_ratio(1, 2),
                Color::rgb(196, 207, 202),
            );
        }
    }
    for index in 0..usize::from(model.gate_len()) {
        let Some(gate) = model.gate(index) else {
            continue;
        };
        if let Some(to) = gate_input_position(model, gate.id, 0)
            && let Some(from) = signal_position(model, gate.a)
        {
            draw_wire(painter, from, to, model.source_value(gate.a));
        }
        if gate.kind != GateKind::Not
            && let Some(to) = gate_input_position(model, gate.id, 1)
            && let Some(from) = signal_position(model, gate.b)
        {
            draw_wire(painter, from, to, model.source_value(gate.b));
        }
    }
    if let Some(from) = signal_position(model, model.output_source()) {
        draw_wire(
            painter,
            from,
            Point::new(288, 153),
            model.source_value(model.output_source()),
        );
    }
    for index in 0..usize::from(model.input_count()) {
        let y = 100 + index as i32 * 54;
        let on = model.input(index as u8);
        painter.fill(
            Rect::new(17, y - 13, 30, 26),
            if on { Color::rgb(255, 238, 228) } else { PANEL },
            Fixed::ZERO,
        );
        painter.border(
            Rect::new(17, y - 13, 30, 26),
            if on { ACCENT } else { LINE },
            Fixed::ONE,
            Fixed::ZERO,
        );
        painter.line(Point::new(47, y), Point::new(54, y), INK, Fixed::ONE);
        painter.circle(Point::new(54, y), Fixed::from_int(3), PANEL);
        painter.border(
            Rect::new(51, y - 3, 6, 6),
            if on { ACCENT } else { INK },
            Fixed::ONE,
            Fixed::from_int(3),
        );
    }
    for index in 0..usize::from(model.gate_len()) {
        let Some(gate) = model.gate(index) else {
            continue;
        };
        let Some((x, y)) = model.visual_gate_position(gate.id) else {
            continue;
        };
        let selected = gate.id == model.selected();
        painter.fill(
            Rect::new(i32::from(x) - 30, i32::from(y) - 18, 60, 36),
            PANEL,
            Fixed::ZERO,
        );
        painter.border(
            Rect::new(i32::from(x) - 30, i32::from(y) - 18, 60, 36),
            if selected { ACCENT } else { MUTED },
            if selected {
                Fixed::from_ratio(3, 2)
            } else {
                Fixed::ONE
            },
            Fixed::ZERO,
        );
        painter.fill(
            Rect::new(i32::from(x) - 29, i32::from(y) - 17, 58, 8),
            if selected { ACCENT } else { INK },
            Fixed::ZERO,
        );
        for pin in 0..if gate.kind == GateKind::Not { 1 } else { 2 } {
            if let Some(point) = gate_input_position(model, gate.id, pin) {
                painter.circle(point, Fixed::from_int(3), PANEL);
                painter.border(
                    Rect {
                        x: point.x - Fixed::from_int(3),
                        y: point.y - Fixed::from_int(3),
                        w: Fixed::from_int(6),
                        h: Fixed::from_int(6),
                    },
                    if model.disconnecting() { RED } else { INK },
                    Fixed::ONE,
                    Fixed::from_int(3),
                );
            }
        }
        if let Some(point) = signal_position(model, SignalSource::gate(gate.id)) {
            painter.circle(
                point,
                Fixed::from_int(3),
                if model.source_value(SignalSource::gate(gate.id)) {
                    ACCENT
                } else {
                    PANEL
                },
            );
            painter.border(
                Rect {
                    x: point.x - Fixed::from_int(3),
                    y: point.y - Fixed::from_int(3),
                    w: Fixed::from_int(6),
                    h: Fixed::from_int(6),
                },
                if model.pending() == SignalSource::gate(gate.id) {
                    ACCENT
                } else {
                    INK
                },
                Fixed::ONE,
                Fixed::from_int(3),
            );
        }
    }
    let evaluation = model.evaluation();
    painter.fill(
        Rect::new(288, 139, 24, 28),
        if evaluation.value { ACCENT } else { INK },
        Fixed::ZERO,
    );
    painter.circle(Point::new(288, 153), Fixed::from_int(3), PANEL);
    painter.border(
        Rect::new(285, 150, 6, 6),
        INK,
        Fixed::ONE,
        Fixed::from_int(3),
    );
    let rows = 1_u8 << model.input_count();
    let passed = (0..rows)
        .filter(|row| model.truth_row(*row).is_some_and(|row| row.passes()))
        .count() as i32;
    painter.fill(Rect::new(334, 239, 123, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(334, 239, 123 * passed / i32::from(rows), 4),
        if passed == i32::from(rows) {
            GREEN
        } else {
            ACCENT
        },
        Fixed::ZERO,
    );
}

fn paint_truth_page(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    let rows = 1_u8 << model.input_count();
    let step = if rows == 8 { 16 } else { 30 };
    for index in 0..rows {
        if index % 2 == 0 {
            painter.fill(
                Rect::new(18, 112 + i32::from(index) * step, 276, step),
                Color::rgb(237, 240, 233),
                Fixed::ZERO,
            );
        }
    }
}

fn paint_trace_page(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    painter.fill(Rect::new(13, 67, 286, 186), INK, Fixed::ZERO);
    let channels = usize::from(model.input_count()) + 1;
    for channel in 0..channels {
        let logical = if channel == channels - 1 { 3 } else { channel };
        let base = 84 + channel as i32 * (150 / channels as i32);
        painter.line(
            Point::new(43, base + 20),
            Point::new(289, base + 20),
            Color::rgb(70, 87, 99),
            Fixed::ONE,
        );
        let mut previous = None;
        for sample_index in 0..usize::from(model.trace_len()) {
            let Some(sample) = model.trace(sample_index) else {
                continue;
            };
            let value = if logical == 3 {
                sample.output
            } else {
                sample.inputs & (1 << logical) != 0
            };
            let x = 47 + sample_index as i32 * 7;
            let y = if value { base } else { base + 17 };
            if let Some((last_x, last_y)) = previous {
                painter.line(
                    Point::new(last_x, last_y),
                    Point::new(x, last_y),
                    if logical == 3 {
                        ACCENT
                    } else {
                        Color::rgb(182, 204, 211)
                    },
                    Fixed::from_ratio(3, 2),
                );
                painter.line(
                    Point::new(x, last_y),
                    Point::new(x, y),
                    if logical == 3 {
                        ACCENT
                    } else {
                        Color::rgb(182, 204, 211)
                    },
                    Fixed::from_ratio(3, 2),
                );
            }
            previous = Some((x, y));
        }
    }
}

fn paint_circuit_surface(painter: &mut PlayPainter<'_, '_>, model: &CircuitModel) {
    paint_shell(painter);
    match model.page() {
        CircuitPage::Wire => paint_wire_page(painter, model),
        CircuitPage::Truth => paint_truth_page(painter, model),
        CircuitPage::Trace => paint_trace_page(painter, model),
    }
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<CircuitModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_circuit_surface(&mut painter, model);
}

fn modal_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<CircuitModel>() else {
        return;
    };
    if model.modal() == CircuitModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(20, 28, 34, 220),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(25, 62, 430, 209), PANEL, Fixed::ZERO);
    painter.border(
        Rect::new(25, 62, 430, 209),
        INK,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(29, 66, 422, 32), INK, Fixed::ZERO);
}

fn surface_view() -> View {
    View::new("CircuitSurface", 60, surface_render).with_filter::<CircuitSurface>()
}

fn modal_view() -> View {
    View::new("CircuitModalSurface", 70, modal_render).with_filter::<CircuitModalSurface>()
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

fn within(point: Point, center: Point, radius: i32) -> bool {
    let dx = point.x - center.x;
    let dy = point.y - center.y;
    dx * dx + dy * dy <= Fixed::from_int(radius * radius)
}

fn hit_gate(model: &CircuitModel, point: Point) -> Option<u8> {
    (0..usize::from(model.gate_len())).find_map(|index| {
        let gate = model.gate(index)?;
        let (x, y) = model.visual_gate_position(gate.id)?;
        ((point.x - Fixed::from_int(i32::from(x))).abs() <= Fixed::from_int(30)
            && (point.y - Fixed::from_int(i32::from(y))).abs() <= Fixed::from_int(18))
        .then_some(gate.id)
    })
}

fn tap_surface(model: &mut CircuitModel, point: Point) -> ChangeSet {
    if model.page() != CircuitPage::Wire || model.modal() != CircuitModal::None {
        return ChangeSet::NONE;
    }
    for index in 0..model.input_count() {
        let y = 100 + i32::from(index) * 54;
        if point.x >= Fixed::from_int(17)
            && point.x <= Fixed::from_int(47)
            && point.y >= Fixed::from_int(y - 13)
            && point.y <= Fixed::from_int(y + 13)
        {
            return model.toggle_input(index);
        }
        if within(point, Point::new(54, y), 9) {
            return model.select_source(SignalSource::input(index));
        }
    }
    for index in 0..usize::from(model.gate_len()) {
        let Some(gate) = model.gate(index) else {
            continue;
        };
        if let Some(output) = signal_position(model, SignalSource::gate(gate.id))
            && within(point, output, 9)
        {
            return model.select_source(SignalSource::gate(gate.id));
        }
        for pin in 0..if gate.kind == GateKind::Not { 1 } else { 2 } {
            if let Some(input) = gate_input_position(model, gate.id, pin)
                && within(point, input, 9)
            {
                return model
                    .connect_pending_to(Some(gate.id), pin)
                    .unwrap_or(ChangeSet::VISUAL);
            }
        }
    }
    if within(point, Point::new(288, 153), 12) {
        return model
            .connect_pending_to(None, 0)
            .unwrap_or(ChangeSet::VISUAL);
    }
    if let Some(gate) = hit_gate(model, point) {
        return model.select_gate(gate);
    }
    model.clear_pending()
}

fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let (x, y) = match event {
        GestureEvent::Tap { x, y, .. }
        | GestureEvent::DragStart { x, y, .. }
        | GestureEvent::DragMove { x, y, .. }
        | GestureEvent::DragEnd { x, y, .. }
        | GestureEvent::DragCancel { x, y, .. } => (*x, *y),
        _ => return false,
    };
    let Some(point) = local_point(world, entity, x, y) else {
        return false;
    };
    CircuitNodes::update(world, |model| match event {
        GestureEvent::Tap { .. } => tap_surface(model, point),
        GestureEvent::DragStart { .. } => hit_gate(model, point).map_or(ChangeSet::NONE, |gate| {
            model.begin_drag_at(gate, point.x.to_int() as i16, point.y.to_int() as i16)
        }),
        GestureEvent::DragMove { .. } => {
            model.move_drag(point.x.to_int() as i16, point.y.to_int() as i16)
        }
        GestureEvent::DragEnd { .. } => model.end_drag(false).unwrap_or(ChangeSet::VISUAL),
        GestureEvent::DragCancel { .. } => model.end_drag(true).unwrap_or(ChangeSet::VISUAL),
        _ => ChangeSet::NONE,
    });
    true
}

fn footer_action(world: &mut World, index: usize) {
    let page = world
        .resource::<CircuitModel>()
        .map_or(CircuitPage::Wire, CircuitModel::page);
    if page == CircuitPage::Trace {
        match index {
            0 => CircuitNodes::update(world, CircuitModel::step_trace),
            1 => CircuitNodes::update(world, CircuitModel::toggle_scanning),
            2 => CircuitNodes::update(world, CircuitModel::clear_trace),
            3 => CircuitNodes::update(world, CircuitModel::verify),
            _ => CircuitNodes::update(world, |model| model.set_page(CircuitPage::Wire)),
        }
    } else {
        match index {
            0 => CircuitNodes::update(world, |model| {
                model.open_modal(CircuitModal::GateTypes { adding: true })
            }),
            1 => CircuitNodes::update(world, |model| {
                model.open_modal(CircuitModal::GateTypes { adding: false })
            }),
            2 => CircuitNodes::update(world, CircuitModel::toggle_disconnecting),
            3 => CircuitNodes::update(world, CircuitModel::undo),
            _ => CircuitNodes::update(world, CircuitModel::verify),
        }
    }
}

fn modal_action(world: &mut World, index: usize) {
    let modal = world
        .resource::<CircuitModel>()
        .map_or(CircuitModal::None, CircuitModel::modal);
    match modal {
        CircuitModal::Tasks => CircuitNodes::update(world, |model| {
            model.load_task((index / 2) as u8, index % 2 == 1)
        }),
        CircuitModal::GateTypes { adding } if index < GateKind::ALL.len() => {
            if adding {
                CircuitNodes::result(world, |model| model.add_gate(GateKind::ALL[index]));
            } else {
                CircuitNodes::result(world, |model| model.set_selected_kind(GateKind::ALL[index]));
            }
            CircuitNodes::update(world, CircuitModel::close_modal);
        }
        CircuitModal::GateTypes { adding: false } if index == 5 => {
            CircuitNodes::result(world, CircuitModel::remove_selected);
            CircuitNodes::update(world, CircuitModel::close_modal);
        }
        CircuitModal::Help => CircuitNodes::update(world, CircuitModel::close_modal),
        _ => {}
    }
}

#[mirui_macros::system(order = ANIMATION)]
fn circuit_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    CircuitNodes::update(world, |model| model.advance_ms(elapsed));
}

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

#[compose]
fn build_widgets() {
    ui! {
        CircuitSurface (id: "circuit_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragStart { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragMove { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragEnd { surface_gesture(ctx.world, ctx.entity, ctx.event); } on DragCancel { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
            Text (
                "逻辑工作台",
                position: Position::Absolute,
                left: 24,
                top: 6,
                width: 180,
                height: 20,
                font_size: 14,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                "NET / 1:6 GATES",
                id: "circuit_gate_count",
                position: Position::Absolute,
                left: 320,
                top: 8,
                width: 130,
                height: 16,
                font_size: 9,
                text_color: Color::rgb(174, 186, 190),
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Button (
                "?",
                position: Position::Absolute,
                left: 451,
                top: 4,
                width: 24,
                height: 23,
                size: ButtonSize::Compact,
                font_size: 12,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { CircuitNodes::update(ctx.world, |model| model.open_modal(CircuitModal::Help)); }
            Button (
                "布线",
                id: "circuit_tab_wire",
                position: Position::Absolute,
                left: 24,
                top: 33,
                width: 96,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACCENT,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { CircuitNodes::update(ctx.world, |model| model.set_page(CircuitPage::Wire)); }
            Button (
                "真值",
                id: "circuit_tab_truth",
                position: Position::Absolute,
                left: 124,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { CircuitNodes::update(ctx.world, |model| model.set_page(CircuitPage::Truth)); }
            Button (
                "时序",
                id: "circuit_tab_trace",
                position: Position::Absolute,
                left: 210,
                top: 33,
                width: 82,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { CircuitNodes::update(ctx.world, |model| model.set_page(CircuitPage::Trace)); }
            Button (
                "任务 01 · 不同才亮",
                id: "circuit_task",
                position: Position::Absolute,
                left: 310,
                top: 34,
                width: 159,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: PANEL,
                pressed_color: ACCENT,
                text_color: INK,
                border_radius: 0
            ) on Tap { CircuitNodes::update(ctx.world, |model| model.open_modal(CircuitModal::Tasks)); }
            View (
                id: "circuit_wire_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Button (
                    "A 0",
                    id: "circuit_input_0",
                    position: Position::Absolute,
                    left: 17,
                    top: 87,
                    width: 30,
                    height: 26,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: PANEL,
                    pressed_color: ACCENT,
                    text_color: INK,
                    border_radius: 0
                ) on Tap { CircuitNodes::update(ctx.world, |model| model.toggle_input(0)); }
                Button (
                    "B 0",
                    id: "circuit_input_1",
                    position: Position::Absolute,
                    left: 17,
                    top: 141,
                    width: 30,
                    height: 26,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: PANEL,
                    pressed_color: ACCENT,
                    text_color: INK,
                    border_radius: 0
                ) on Tap { CircuitNodes::update(ctx.world, |model| model.toggle_input(1)); }
                Button (
                    "C 0",
                    id: "circuit_input_2",
                    position: Position::Absolute,
                    left: 17,
                    top: 195,
                    width: 30,
                    height: 26,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: PANEL,
                    pressed_color: ACCENT,
                    text_color: INK,
                    border_radius: 0
                ) on Tap { CircuitNodes::update(ctx.world, |model| model.toggle_input(2)); }
                Text (
                    "",
                    id: "circuit_gate_0",
                    position: Position::Absolute,
                    left: 116,
                    top: 136,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "",
                    id: "circuit_gate_1",
                    position: Position::Absolute,
                    left: 186,
                    top: 136,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "",
                    id: "circuit_gate_2",
                    position: Position::Absolute,
                    left: 116,
                    top: 190,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "",
                    id: "circuit_gate_3",
                    position: Position::Absolute,
                    left: 186,
                    top: 82,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "",
                    id: "circuit_gate_4",
                    position: Position::Absolute,
                    left: 186,
                    top: 190,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "",
                    id: "circuit_gate_5",
                    position: Position::Absolute,
                    left: 116,
                    top: 82,
                    width: 58,
                    height: 28,
                    font_size: 8,
                    text_color: INK,
                    line_height: 9,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "Y\n?",
                    id: "circuit_output",
                    position: Position::Absolute,
                    left: 288,
                    top: 139,
                    width: 24,
                    height: 28,
                    font_size: 8,
                    text_color: BACKGROUND,
                    line_height: 10,
                    paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
                )
                Text (
                    "LIVE TRUTH TABLE",
                    position: Position::Absolute,
                    left: 334,
                    top: 76,
                    width: 125,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "Y = A XOR B",
                    id: "circuit_live_code",
                    position: Position::Absolute,
                    left: 334,
                    top: 94,
                    width: 125,
                    height: 13,
                    font_size: 9,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "A B C → Y / 目标",
                    position: Position::Absolute,
                    left: 334,
                    top: 108,
                    width: 125,
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_0",
                    position: Position::Absolute,
                    left: 335,
                    top: 121,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_1",
                    position: Position::Absolute,
                    left: 335,
                    top: 134,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_2",
                    position: Position::Absolute,
                    left: 335,
                    top: 147,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_3",
                    position: Position::Absolute,
                    left: 335,
                    top: 160,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_4",
                    position: Position::Absolute,
                    left: 335,
                    top: 173,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_5",
                    position: Position::Absolute,
                    left: 335,
                    top: 186,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_6",
                    position: Position::Absolute,
                    left: 335,
                    top: 199,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_live_7",
                    position: Position::Absolute,
                    left: 335,
                    top: 212,
                    width: 126,
                    height: 13,
                    font_size: 7,
                    text_color: GREEN,
                    paragraph: label_style()
                )
                Text (
                    "点输出端，再点输入端；拖动模块改变位置。",
                    id: "circuit_wire_status",
                    position: Position::Absolute,
                    left: 16,
                    top: 259,
                    width: 447,
                    height: 18,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
            }
            View (
                id: "circuit_truth_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "EXHAUSTIVE INPUT TEST",
                    position: Position::Absolute,
                    left: 23,
                    top: 77,
                    width: 250,
                    height: 13,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "A   B   C      EXPECT     ACTUAL     RESULT",
                    position: Position::Absolute,
                    left: 27,
                    top: 98,
                    width: 265,
                    height: 13,
                    font_size: 7,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_0",
                    position: Position::Absolute,
                    left: 27,
                    top: 115,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_1",
                    position: Position::Absolute,
                    left: 27,
                    top: 131,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_2",
                    position: Position::Absolute,
                    left: 27,
                    top: 147,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_3",
                    position: Position::Absolute,
                    left: 27,
                    top: 163,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_4",
                    position: Position::Absolute,
                    left: 27,
                    top: 179,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_5",
                    position: Position::Absolute,
                    left: 27,
                    top: 195,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_6",
                    position: Position::Absolute,
                    left: 27,
                    top: 211,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "",
                    id: "circuit_truth_7",
                    position: Position::Absolute,
                    left: 27,
                    top: 227,
                    width: 265,
                    height: 16,
                    font_size: 7,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "TARGET SPECIFICATION",
                    position: Position::Absolute,
                    left: 334,
                    top: 78,
                    width: 125,
                    height: 13,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "不同才亮",
                    id: "circuit_truth_title",
                    position: Position::Absolute,
                    left: 334,
                    top: 100,
                    width: 125,
                    height: 22,
                    font_size: 13,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "A 与 B 不同时，Y 才为 1。",
                    id: "circuit_truth_desc",
                    position: Position::Absolute,
                    left: 334,
                    top: 132,
                    width: 120,
                    height: 54,
                    font_size: 9,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::default()
                )
                Text (
                    "未连接输入按 0 求值，但不会通过完整性验证。",
                    position: Position::Absolute,
                    left: 334,
                    top: 190,
                    width: 120,
                    height: 48,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::default()
                )
            }
            View (
                id: "circuit_trace_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 282
            ) {
                Text (
                    "A",
                    id: "circuit_trace_a",
                    position: Position::Absolute,
                    left: 22,
                    top: 83,
                    width: 18,
                    height: 14,
                    font_size: 9,
                    text_color: Color::rgb(182, 204, 211),
                    paragraph: label_style()
                )
                Text (
                    "B",
                    id: "circuit_trace_b",
                    position: Position::Absolute,
                    left: 22,
                    top: 120,
                    width: 18,
                    height: 14,
                    font_size: 9,
                    text_color: Color::rgb(182, 204, 211),
                    paragraph: label_style()
                )
                Text (
                    "C",
                    id: "circuit_trace_c",
                    position: Position::Absolute,
                    left: 22,
                    top: 157,
                    width: 18,
                    height: 14,
                    font_size: 9,
                    text_color: Color::rgb(182, 204, 211),
                    paragraph: label_style()
                )
                Text (
                    "Y",
                    id: "circuit_trace_y",
                    position: Position::Absolute,
                    left: 22,
                    top: 194,
                    width: 18,
                    height: 14,
                    font_size: 9,
                    text_color: ACCENT,
                    paragraph: label_style()
                )
                Text (
                    "LAST 32 SAMPLES / LOGICAL TIME",
                    position: Position::Absolute,
                    left: 22,
                    top: 240,
                    width: 260,
                    height: 12,
                    font_size: 7,
                    text_color: Color::rgb(156, 174, 183),
                    paragraph: label_style()
                )
                Text (
                    "SIGNAL SCANNER",
                    position: Position::Absolute,
                    left: 334,
                    top: 78,
                    width: 125,
                    height: 13,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Text (
                    "STEP / READY",
                    id: "circuit_trace_mode",
                    position: Position::Absolute,
                    left: 334,
                    top: 103,
                    width: 125,
                    height: 16,
                    font_size: 10,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    "0 / 32",
                    id: "circuit_trace_count",
                    position: Position::Absolute,
                    left: 334,
                    top: 127,
                    width: 125,
                    height: 28,
                    font_size: 18,
                    text_color: INK,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                )
                Text (
                    "扫描按顺序遍历输入组合，记录 A/B/C/Y。它不模拟传播延迟。",
                    position: Position::Absolute,
                    left: 334,
                    top: 164,
                    width: 120,
                    height: 70,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::default()
                )
            }
            Button (
                "＋ 逻辑门",
                id: "circuit_footer_0",
                position: Position::Absolute,
                left: 7,
                top: 287,
                width: 90,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 0); }
            Button (
                "类型 / 删除",
                id: "circuit_footer_1",
                position: Position::Absolute,
                left: 101,
                top: 287,
                width: 90,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 1); }
            Button (
                "断开连线",
                id: "circuit_footer_2",
                position: Position::Absolute,
                left: 195,
                top: 287,
                width: 90,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 2); }
            Button (
                "撤销",
                id: "circuit_footer_3",
                position: Position::Absolute,
                left: 289,
                top: 287,
                width: 90,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 3); }
            Button (
                "✓ 验证",
                id: "circuit_footer_4",
                position: Position::Absolute,
                left: 383,
                top: 287,
                width: 90,
                height: 27,
                size: ButtonSize::Compact,
                font_size: 8,
                normal_color: ACCENT,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { footer_action(ctx.world, 4); }
            CircuitModalSurface (
                id: "circuit_modal",
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
                    "选择逻辑任务",
                    id: "circuit_modal_title",
                    position: Position::Absolute,
                    left: 43,
                    top: 72,
                    width: 330,
                    height: 20,
                    font_size: 12,
                    text_color: BACKGROUND,
                    paragraph: label_style()
                )
                Text (
                    "切换会清空当前网络、时序和撤销记录。",
                    id: "circuit_modal_subtitle",
                    position: Position::Absolute,
                    left: 43,
                    top: 105,
                    width: 360,
                    height: 18,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: label_style()
                )
                Button (
                    "×",
                    position: Position::Absolute,
                    left: 417,
                    top: 69,
                    width: 28,
                    height: 26,
                    size: ButtonSize::Compact,
                    font_size: 13,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { CircuitNodes::update(ctx.world, CircuitModel::close_modal); }
                Button (
                    "任务 01",
                    id: "circuit_modal_0",
                    position: Position::Absolute,
                    left: 43,
                    top: 128,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 0); }
                Button (
                    "示范布局",
                    id: "circuit_modal_1",
                    position: Position::Absolute,
                    left: 173,
                    top: 128,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 1); }
                Button (
                    "任务 02",
                    id: "circuit_modal_2",
                    position: Position::Absolute,
                    left: 303,
                    top: 128,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 2); }
                Button (
                    "示范布局",
                    id: "circuit_modal_3",
                    position: Position::Absolute,
                    left: 43,
                    top: 183,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 3); }
                Button (
                    "任务 03",
                    id: "circuit_modal_4",
                    position: Position::Absolute,
                    left: 173,
                    top: 183,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 4); }
                Button (
                    "示范布局",
                    id: "circuit_modal_5",
                    position: Position::Absolute,
                    left: 303,
                    top: 183,
                    width: 120,
                    height: 42,
                    size: ButtonSize::Compact,
                    font_size: 9,
                    normal_color: INK,
                    pressed_color: ACCENT,
                    text_color: BACKGROUND,
                    border_radius: 0
                ) on Tap { modal_action(ctx.world, 5); }
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
    register_play_font(&mut app.world);
    app.world.insert_resource(CircuitModel::default());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_system(circuit_tick_system::system());
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Logic Circuit node");
    let nodes = CircuitNodes {
        surface: find("circuit_surface"),
        gate_count: find("circuit_gate_count"),
        task: find("circuit_task"),
        tabs: [
            find("circuit_tab_wire"),
            find("circuit_tab_truth"),
            find("circuit_tab_trace"),
        ],
        pages: [
            find("circuit_wire_page"),
            find("circuit_truth_page"),
            find("circuit_trace_page"),
        ],
        inputs: [
            find("circuit_input_0"),
            find("circuit_input_1"),
            find("circuit_input_2"),
        ],
        gates: core::array::from_fn(|index| {
            find(match index {
                0 => "circuit_gate_0",
                1 => "circuit_gate_1",
                2 => "circuit_gate_2",
                3 => "circuit_gate_3",
                4 => "circuit_gate_4",
                _ => "circuit_gate_5",
            })
        }),
        output: find("circuit_output"),
        live_code: find("circuit_live_code"),
        wire_status: find("circuit_wire_status"),
        live_rows: core::array::from_fn(|index| {
            find(match index {
                0 => "circuit_live_0",
                1 => "circuit_live_1",
                2 => "circuit_live_2",
                3 => "circuit_live_3",
                4 => "circuit_live_4",
                5 => "circuit_live_5",
                6 => "circuit_live_6",
                _ => "circuit_live_7",
            })
        }),
        truth_rows: core::array::from_fn(|index| {
            find(match index {
                0 => "circuit_truth_0",
                1 => "circuit_truth_1",
                2 => "circuit_truth_2",
                3 => "circuit_truth_3",
                4 => "circuit_truth_4",
                5 => "circuit_truth_5",
                6 => "circuit_truth_6",
                _ => "circuit_truth_7",
            })
        }),
        truth_title: find("circuit_truth_title"),
        truth_desc: find("circuit_truth_desc"),
        trace_labels: [
            find("circuit_trace_a"),
            find("circuit_trace_b"),
            find("circuit_trace_c"),
            find("circuit_trace_y"),
        ],
        trace_mode: find("circuit_trace_mode"),
        trace_count: find("circuit_trace_count"),
        footer: [
            find("circuit_footer_0"),
            find("circuit_footer_1"),
            find("circuit_footer_2"),
            find("circuit_footer_3"),
            find("circuit_footer_4"),
        ],
        modal: find("circuit_modal"),
        modal_title: find("circuit_modal_title"),
        modal_subtitle: find("circuit_modal_subtitle"),
        modal_buttons: [
            find("circuit_modal_0"),
            find("circuit_modal_1"),
            find("circuit_modal_2"),
            find("circuit_modal_3"),
            find("circuit_modal_4"),
            find("circuit_modal_5"),
        ],
    };
    app.world.insert_resource(nodes);
    CircuitNodes::sync(&mut app.world);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(480, 320);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_uses_one_dense_surface_and_semantic_controls() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        assert!(app.world.find_by_id("circuit_surface").is_some());
        assert!(app.world.find_by_id("circuit_footer_4").is_some());
        assert!(app.world.find_by_id("circuit_task").is_some());
        assert_eq!(app.world.query::<CircuitSurface>().iter().count(), 1);
    }

    #[test]
    fn gate_drag_cancel_leaves_authoritative_position_unchanged() {
        let mut app = App::headless(480, 320);
        app.with_default_widgets().with_default_systems();
        let root = app.spawn_root().id();
        setup_app(&mut app, root);
        app.set_root(root);
        app.systems.run_all(&mut app.world);
        let surface = app.world.find_by_id("circuit_surface").unwrap();
        app.world
            .insert(surface, ComputedRect(Rect::new(0, 0, 480, 320)));
        let before = app
            .world
            .resource::<CircuitModel>()
            .unwrap()
            .gate_by_id(1)
            .unwrap();
        assert!(surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::DragStart {
                x: Fixed::from_int(145),
                y: Fixed::from_int(150),
                target: surface
            }
        ));
        assert!(surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::DragMove {
                x: Fixed::from_int(180),
                y: Fixed::from_int(210),
                dx: Fixed::from_int(35),
                dy: Fixed::from_int(60),
                target: surface
            }
        ));
        assert!(surface_gesture(
            &mut app.world,
            surface,
            &GestureEvent::DragCancel {
                x: Fixed::from_int(180),
                y: Fixed::from_int(210),
                target: surface
            }
        ));
        assert_eq!(
            app.world
                .resource::<CircuitModel>()
                .unwrap()
                .gate_by_id(1)
                .unwrap(),
            before
        );
    }
}
