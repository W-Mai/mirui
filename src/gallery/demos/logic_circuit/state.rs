use alloc::format;

use super::style::{ACCENT, BACKGROUND, INK, LINE, MUTED};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::circuit::{
    CircuitError, CircuitModal, CircuitModel, CircuitPage, GateKind, MAX_GATES, SignalSource,
};
use crate::prelude::{Dimension, Entity, Style, World};
use crate::ui::Hidden;
use crate::ui::widgets::{Button, Text};

#[crate::component]
#[derive(Default)]
pub(super) struct CircuitSurface;

#[crate::component]
#[derive(Default)]
pub(super) struct CircuitModalSurface;

#[derive(Clone, Copy)]
pub(super) struct CircuitNodes {
    pub(super) surface: Entity,
    pub(super) gate_count: Entity,
    pub(super) task: Entity,
    pub(super) tabs: [Entity; 3],
    pub(super) pages: [Entity; 3],
    pub(super) inputs: [Entity; 3],
    pub(super) gates: [Entity; MAX_GATES],
    pub(super) output: Entity,
    pub(super) live_code: Entity,
    pub(super) wire_status: Entity,
    pub(super) live_rows: [Entity; 8],
    pub(super) truth_rows: [Entity; 8],
    pub(super) truth_title: Entity,
    pub(super) truth_desc: Entity,
    pub(super) trace_labels: [Entity; 4],
    pub(super) trace_mode: Entity,
    pub(super) trace_count: Entity,
    pub(super) footer: [Entity; 5],
    pub(super) modal: Entity,
    pub(super) modal_title: Entity,
    pub(super) modal_subtitle: Entity,
    pub(super) modal_buttons: [Entity; 6],
}

impl CircuitNodes {
    pub(super) fn update(world: &mut World, update: impl FnOnce(&mut CircuitModel) -> ChangeSet) {
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

    pub(super) fn result(
        world: &mut World,
        update: impl FnOnce(&mut CircuitModel) -> Result<ChangeSet, CircuitError>,
    ) {
        Self::update(world, |model| update(model).unwrap_or(ChangeSet::VISUAL));
    }

    pub(super) fn sync(world: &mut World) {
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
