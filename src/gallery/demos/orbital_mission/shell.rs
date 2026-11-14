use super::input::orbit_tick_system;
use super::render::{modal_render, surface_render};
use super::state::{OrbitModalSurface, OrbitSurface};
use super::style::{
    BACKGROUND, CLEAR, CYAN, DEEP, INK, LINE, MUTED, ORANGE, SPACE, body_style, center_style,
    end_style, label_style,
};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::PlayFontPlugin;
use crate::gallery::play::orbit::{
    MISSION_COUNT, MISSIONS, ManeuverNode, OrbitError, OrbitEvent, OrbitEventKind, OrbitGoal,
    OrbitModal, OrbitModel, OrbitPage, OrbitStatus, START_RADIUS,
};
use crate::prelude::*;
use crate::types::Fixed64;
use crate::ui::widgets::{Button, ButtonSize, Text};
use core::fmt;

fn control_color(active: bool, enabled: bool) -> Color {
    if !enabled {
        LINE
    } else if active {
        ORANGE
    } else {
        INK
    }
}

fn control_text_color(enabled: bool) -> Color {
    if enabled { BACKGROUND } else { MUTED }
}

struct Decimal1(Fixed64);

impl fmt::Display for Decimal1 {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tenths = (self.0.abs() * 10 + Fixed64::from_ratio(1, 2)).to_int();
        let sign = if self.0.is_negative() && tenths != 0 {
            "-"
        } else {
            ""
        };
        write!(out, "{sign}{}.{}", tenths / 10, tenths % 10)
    }
}

/// A value kept in tenths, such as Δv.
struct Tenths(u16);

impl fmt::Display for Tenths {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{}.{}", self.0 / 10, self.0 % 10)
    }
}

/// "90° 向内": the angle, named when it is one of the four plain directions.
struct AngleText(i16);

impl fmt::Display for AngleText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.0 {
            0 => " 顺向",
            180 => " 反向",
            90 => " 向内",
            -90 => " 向外",
            _ => "",
        };
        write!(out, "{}°{name}", self.0)
    }
}

struct NodeText {
    node: Option<ManeuverNode>,
    number: usize,
}

impl fmt::Display for NodeText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(node) = self.node else {
            return write!(out, "0{}  —", self.number);
        };
        write!(
            out,
            "0{}  T+{}  Δv {}  {}°",
            self.number,
            Decimal1(node.at),
            Tenths(node.dv_tenths.into()),
            node.angle_degrees
        )
    }
}

/// "FUEL  33.6 / 48".
struct FuelText {
    fuel: Fixed64,
    mission: u8,
}

impl fmt::Display for FuelText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let full = MISSIONS[usize::from(self.mission)].fuel;
        write!(out, "FUEL  {} / {full}", Decimal1(self.fuel))
    }
}

/// When a node planned now would fire: "T+13.0".
struct FireTime {
    time: Fixed64,
    delay: u8,
}

impl fmt::Display for FireTime {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = self.time + Fixed64::from_int(self.delay.into());
        write!(out, "T+{}", Decimal1(at))
    }
}

/// "外圈 113~130" or "信标 · 限速 24".
struct GoalName(OrbitGoal);

impl fmt::Display for GoalName {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            OrbitGoal::Band { min, max } => {
                let side = if i64::from(min) > START_RADIUS {
                    "外圈"
                } else {
                    "内圈"
                };
                write!(out, "{side} {min}~{max}")
            }
            OrbitGoal::Beacon { max_speed, .. } => write!(out, "信标 · 限速 {max_speed}"),
        }
    }
}

/// The map's goal line: which goal is next and where it is.
struct GoalLine {
    mission: u8,
    completed: u8,
}

impl fmt::Display for GoalLine {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mission = MISSIONS[usize::from(self.mission)];
        let total = mission.goal_count();
        match mission.goal(usize::from(self.completed)) {
            Some(goal) => write!(
                out,
                "目标 {}/{total} · {}",
                self.completed + 1,
                GoalName(goal)
            ),
            None => write!(out, "目标 {total}/{total} · 完成"),
        }
    }
}

/// A goal in the briefing list.
struct GoalEntry {
    mission: u8,
    index: usize,
}

impl fmt::Display for GoalEntry {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match MISSIONS[usize::from(self.mission)].goal(self.index) {
            Some(goal) => write!(out, "{}  {}", self.index + 1, GoalName(goal)),
            None => Ok(()),
        }
    }
}

/// What the craft can do at the current goal.
struct ZoneNotice {
    goal: Option<OrbitGoal>,
    in_zone: bool,
    eligible: bool,
}

impl fmt::Display for ZoneNotice {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.eligible {
            return out.write_str("● 已到目标，可以采样");
        }
        match self.goal {
            Some(OrbitGoal::Beacon { max_speed, .. }) if self.in_zone => {
                write!(out, "速度过快，限速 {max_speed}")
            }
            _ => Ok(()),
        }
    }
}

struct ContactText(Option<u16>);

impl fmt::Display for ContactText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => Ok(()),
            Some(0) => out.write_str("已失联 · 只执行节点"),
            Some(tenths) => write!(out, "{} s 后失联", Tenths(tenths)),
        }
    }
}

struct EventLine {
    event: Option<OrbitEvent>,
    goals: usize,
}

impl fmt::Display for EventLine {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(event) = self.event else {
            return Ok(());
        };
        write!(out, "T+{}  ", Decimal1(event.time))?;
        match event.kind {
            OrbitEventKind::Loaded => out.write_str("任务开始"),
            OrbitEventKind::Burn(dv) => write!(out, "点火 Δv {}", Tenths(dv.into())),
            OrbitEventKind::NodeBurn(dv) => write!(out, "节点点火 Δv {}", Tenths(dv.into())),
            OrbitEventKind::NodeSkipped(OrbitError::Fuel) => out.write_str("节点跳过：缺燃料"),
            OrbitEventKind::NodeSkipped(OrbitError::Heat) => out.write_str("节点跳过：过热"),
            OrbitEventKind::NodeSkipped(_) => out.write_str("节点跳过"),
            OrbitEventKind::Goal(done) => write!(out, "采样 {done}/{}", self.goals),
            OrbitEventKind::Won => out.write_str("任务完成"),
            OrbitEventKind::Crashed => out.write_str("撞上行星"),
            OrbitEventKind::Escaped => out.write_str("飞出边界"),
        }
    }
}

fn cleared(mask: u8, mission: u8) -> bool {
    mask & (1 << mission) != 0
}

/// "任务 03 · 降低轨道 ★".
struct MissionTitle {
    mission: u8,
    cleared: u8,
}

impl fmt::Display for MissionTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "任务 0{} · {}",
            self.mission + 1,
            MISSIONS[usize::from(self.mission)].name
        )?;
        if cleared(self.cleared, self.mission) {
            out.write_str(" ★")?;
        }
        Ok(())
    }
}

struct MissionTile {
    mission: u8,
    cleared: u8,
}

impl fmt::Display for MissionTile {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let star = if cleared(self.cleared, self.mission) {
            "★"
        } else {
            "☆"
        };
        write!(
            out,
            "0{} {star}\n{}",
            self.mission + 1,
            MISSIONS[usize::from(self.mission)].name
        )
    }
}

fn burn_label(block: Option<OrbitError>) -> &'static str {
    match block {
        None | Some(OrbitError::Terminal) => "点火",
        Some(OrbitError::Fuel) => "缺燃料",
        Some(OrbitError::Heat) => "过热",
        Some(_) => "已失联",
    }
}

fn node_label(block: Option<OrbitError>) -> &'static str {
    match block {
        Some(OrbitError::QueueFull) => "最多 3 个",
        Some(OrbitError::NoContact) => "已失联",
        _ => "＋ 节点",
    }
}

fn run_label(status: OrbitStatus, has_next: bool) -> &'static str {
    match status {
        OrbitStatus::Running => "Ⅱ 暂停",
        OrbitStatus::Won if has_next => "下一个 ▶",
        _ => "▶ 滑行",
    }
}

fn run_enabled(status: OrbitStatus, has_next: bool) -> bool {
    !status.terminal() || (status == OrbitStatus::Won && has_next)
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

struct PlanTab(u8);

impl fmt::Display for PlanTab {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            0 => out.write_str("计划"),
            count => write!(out, "计划 · {count}"),
        }
    }
}

/// What the planned nodes will burn, and whether the tank holds it.
struct PlannedFuel {
    planned: u16,
    fuel: Fixed64,
}

impl fmt::Display for PlannedFuel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "节点用燃料 {}", Tenths(self.planned))?;
        if Fixed64::from_ratio(self.planned.into(), 10) > self.fuel {
            out.write_str(" · 不足")?;
        }
        Ok(())
    }
}

struct ModalTitle {
    modal: OrbitModal,
    mission: u8,
    status: OrbitStatus,
    cleared: u8,
}

impl fmt::Display for ModalTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = |mission: u8| MISSIONS[usize::from(mission)].name;
        match self.modal {
            OrbitModal::None => Ok(()),
            OrbitModal::Missions => out.write_str("选择任务"),
            OrbitModal::Brief => write!(out, "任务 0{} · {}", self.mission + 1, name(self.mission)),
            OrbitModal::Confirm(target) if target == self.mission => out.write_str("重来本任务？"),
            OrbitModal::Confirm(target) => write!(out, "换到 0{} · {}？", target + 1, name(target)),
            OrbitModal::Result => match self.status {
                OrbitStatus::Won if self.cleared.count_ones() as usize == MISSION_COUNT => {
                    write!(out, "{MISSION_COUNT} 个任务全部完成 ★")
                }
                OrbitStatus::Won => {
                    write!(out, "0{} {} 完成 ★", self.mission + 1, name(self.mission))
                }
                OrbitStatus::Crashed => out.write_str("撞上行星"),
                _ => out.write_str("飞出边界"),
            },
        }
    }
}

struct ModalBody {
    modal: OrbitModal,
    mission: u8,
    status: OrbitStatus,
    time: Fixed64,
    fuel: Fixed64,
}

impl fmt::Display for ModalBody {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.modal {
            OrbitModal::Brief => out.write_str(MISSIONS[usize::from(self.mission)].description),
            OrbitModal::Confirm(_) => out.write_str("当前的飞行和计划会清空。"),
            OrbitModal::Result => match self.status {
                OrbitStatus::Won => write!(
                    out,
                    "用时 {} s · 燃料还有 {}",
                    Decimal1(self.time),
                    Decimal1(self.fuel)
                ),
                OrbitStatus::Crashed => out.write_str("RADIUS 不到 27 就会撞上。"),
                _ => out.write_str("RADIUS 超过 260 就出界了。"),
            },
            OrbitModal::None | OrbitModal::Missions => Ok(()),
        }
    }
}

/// The last line of the briefing: the hint once asked for, else how to fly.
struct BriefFoot {
    mission: u8,
    hint: bool,
}

impl fmt::Display for BriefFoot {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.hint {
            write!(out, "提示：{}", MISSIONS[usize::from(self.mission)].hint)
        } else {
            out.write_str("调好 Δv 和角度后点火，按 ▶ 滑行，到目标后采样。")
        }
    }
}

struct ClearedCount(u8);

impl fmt::Display for ClearedCount {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "★ {}/{MISSION_COUNT}", self.0.count_ones())
    }
}

/// Progress and the next mission, under a won flight's result.
struct ResultNote {
    modal: OrbitModal,
    status: OrbitStatus,
    mission: u8,
    cleared: u8,
}

impl fmt::Display for ResultNote {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modal != OrbitModal::Result || self.status != OrbitStatus::Won {
            return Ok(());
        }
        write!(out, "{}", ClearedCount(self.cleared))?;
        let next = usize::from(self.mission) + 1;
        if let Some(mission) = MISSIONS.get(next) {
            write!(out, "   下一个：0{} {}", next + 1, mission.name)?;
        }
        Ok(())
    }
}

fn modal_left_label(modal: OrbitModal, status: OrbitStatus, hint: bool) -> &'static str {
    match modal {
        OrbitModal::Brief if hint => "收起提示",
        OrbitModal::Brief => "提示",
        OrbitModal::Confirm(_) => "取消",
        OrbitModal::Result if status == OrbitStatus::Won => "↺ 再飞一次",
        OrbitModal::Result => "选择任务",
        OrbitModal::None | OrbitModal::Missions => "",
    }
}

fn modal_right_label(modal: OrbitModal, status: OrbitStatus, has_next: bool) -> &'static str {
    match modal {
        OrbitModal::Brief if status == OrbitStatus::Ready => "开始",
        OrbitModal::Brief => "继续",
        OrbitModal::Confirm(_) => "确定",
        OrbitModal::Result if status == OrbitStatus::Won && has_next => "下一个任务 ▶",
        OrbitModal::Result if status == OrbitStatus::Won => "选择任务",
        OrbitModal::Result => "↺ 重来",
        OrbitModal::None | OrbitModal::Missions => "",
    }
}

/// A page shows its controls only while no dialog covers it.
fn page_shown(page: OrbitPage, current: OrbitPage, modal: OrbitModal) -> bool {
    page == current && modal == OrbitModal::None
}

fn has_actions(modal: OrbitModal) -> bool {
    matches!(
        modal,
        OrbitModal::Brief | OrbitModal::Confirm(_) | OrbitModal::Result
    )
}

#[compose]
fn compose_header(model: model!(OrbitModel)) -> Entity {
    ui! {
        Text (
            "轨道任务台",
            position: Position::Absolute,
            left: 24,
            top: 6,
            width: 160,
            height: 20,
            font_size: 10,
            text_color: BACKGROUND,
            paragraph: label_style()
        )
    };
    ui! {
        Button (
            text: ${
                MissionTitle {
                    mission: model.mission(),
                    cleared: model.cleared(),
                }
            },
            capacity: 48,
            id: "orbit_mission",
            position: Position::Absolute,
            left: 270,
            top: 4,
            width: 172,
            height: 23,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: INK,
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_modal(OrbitModal::Missions); }
    };
    ui! {
        Button (
            "?",
            id: "orbit_brief",
            position: Position::Absolute,
            left: 450,
            top: 4,
            width: 25,
            height: 23,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: INK,
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_modal(OrbitModal::Brief); }
    };
    ui! {
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
            normal_color: ${ control_color(model.page() == OrbitPage::Map, true) },
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(OrbitPage::Map); }
    };
    ui! {
        Button (
            text: ${ PlanTab(model.queue_len()) },
            capacity: 16,
            id: "orbit_tab_plan",
            position: Position::Absolute,
            left: 119,
            top: 33,
            width: 82,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.page() == OrbitPage::Plan, true) },
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(OrbitPage::Plan); }
    };
    ui! {
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
            normal_color: ${ control_color(model.page() == OrbitPage::Record, true) },
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(OrbitPage::Record); }
    };
    ui! {
        Text (
            text: ${ format_args!("SIM T+{}", Decimal1(model.time())) },
            capacity: 24,
            id: "orbit_time",
            position: Position::Absolute,
            left: 356,
            top: 38,
            width: 102,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: end_style()
        )
    }
}

#[compose]
fn compose_map_page(model: model!(OrbitModel)) -> Entity {
    ui! {
        View (
            id: "orbit_map_page",
            visible: ${ page_shown(OrbitPage::Map, model.page(), model.modal()) },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 282
        ) {
            Text (
                text: ${
                    GoalLine {
                        mission: model.mission(),
                        completed: model.completed(),
                    }
                },
                capacity: 48,
                id: "orbit_goal",
                position: Position::Absolute,
                left: 19,
                top: 70,
                width: 272,
                height: 14,
                font_size: 10,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                text: ${
                    ZoneNotice {
                        goal: model.current_goal(),
                        in_zone: model.in_zone(),
                        eligible: model.eligible(),
                    }
                },
                capacity: 48,
                id: "orbit_notice",
                position: Position::Absolute,
                left: 19,
                top: 86,
                width: 272,
                height: 14,
                font_size: 10,
                text_color: ORANGE,
                paragraph: label_style()
            )
            Text (
                text: ${ ContactText(model.contact_left()) },
                capacity: 40,
                id: "orbit_contact",
                position: Position::Absolute,
                left: 96,
                top: 233,
                width: 144,
                height: 14,
                font_size: 10,
                text_color: LINE,
                paragraph: center_style()
            )
            Button (
                text: ${ if model.preview() { "◉ 预演" } else { "○ 预演" } },
                capacity: 16,
                id: "orbit_preview",
                position: Position::Absolute,
                left: 18,
                top: 230,
                width: 72,
                height: 20,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: SPACE,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: ${ if model.preview() { ORANGE } else { LINE } },
                border_radius: 0
            ) on Tap { model.toggle_preview(); }
            Button (
                text: ${ format_args!("{}×", model.warp()) },
                capacity: 8,
                id: "orbit_warp",
                position: Position::Absolute,
                left: 246,
                top: 230,
                width: 44,
                height: 20,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: SPACE,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.cycle_warp(); }
            Text (
                text: ${
                    FuelText {
                        fuel: model.fuel(),
                        mission: model.mission(),
                    }
                },
                capacity: 24,
                id: "orbit_fuel",
                position: Position::Absolute,
                left: 322,
                top: 72,
                width: 132,
                height: 14,
                font_size: 10,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("HEAT  {}%", model.heat().to_int()) },
                capacity: 20,
                id: "orbit_heat",
                position: Position::Absolute,
                left: 322,
                top: 96,
                width: 132,
                height: 14,
                font_size: 10,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ Decimal1(model.radius()) },
                capacity: 16,
                id: "orbit_radius",
                position: Position::Absolute,
                left: 322,
                top: 120,
                width: 66,
                height: 14,
                font_size: 10,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ Decimal1(model.speed()) },
                capacity: 16,
                id: "orbit_speed",
                position: Position::Absolute,
                left: 388,
                top: 120,
                width: 66,
                height: 14,
                font_size: 10,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                "RADIUS",
                position: Position::Absolute,
                left: 322,
                top: 134,
                width: 66,
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                "SPEED",
                position: Position::Absolute,
                left: 388,
                top: 134,
                width: 66,
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: end_style()
            )
            burn_dial (model, 322, 154, false)
        }
    }
}

/// Δv and burn angle, shared by the map page (a burn now) and the plan page
/// (the next node).
#[compose]
fn burn_dial(model: model!(OrbitModel), left: i32, top: i32, dark: bool) -> Entity {
    let (label, value, button) = if dark {
        (LINE, BACKGROUND, DEEP)
    } else {
        (MUTED, INK, INK)
    };
    ui! {
        View (
            position: Position::Absolute,
            left: left,
            top: top,
            width: 132,
            height: 82
        ) {
            Text (
                "BURN ΔV",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 70,
                height: 14,
                font_size: 10,
                text_color: label,
                paragraph: label_style()
            )
            Text (
                text: ${ Tenths(model.dv_tenths().into()) },
                capacity: 8,
                position: Position::Absolute,
                left: 72,
                top: 0,
                width: 60,
                height: 14,
                font_size: 10,
                text_color: value,
                paragraph: end_style()
            )
            Button (
                "－1",
                position: Position::Absolute,
                left: 0,
                top: 16,
                width: 30,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: button,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.adjust_dv(-10); }
            Button (
                "－.2",
                position: Position::Absolute,
                left: 34,
                top: 16,
                width: 30,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: button,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.adjust_dv(-2); }
            Button (
                "＋.2",
                position: Position::Absolute,
                left: 68,
                top: 16,
                width: 30,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: button,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.adjust_dv(2); }
            Button (
                "＋1",
                position: Position::Absolute,
                left: 102,
                top: 16,
                width: 30,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: button,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.adjust_dv(10); }
            Text (
                "ANGLE",
                position: Position::Absolute,
                left: 0,
                top: 44,
                width: 50,
                height: 14,
                font_size: 10,
                text_color: label,
                paragraph: label_style()
            )
            Text (
                text: ${ AngleText(model.angle_degrees()) },
                capacity: 16,
                position: Position::Absolute,
                left: 42,
                top: 44,
                width: 90,
                height: 14,
                font_size: 10,
                text_color: value,
                paragraph: end_style()
            )
            Button (
                "－15°",
                position: Position::Absolute,
                left: 0,
                top: 60,
                width: 42,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: button,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.adjust_angle(-15); }
            Button (
                "＋15°",
                position: Position::Absolute,
                left: 45,
                top: 60,
                width: 42,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: button,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.adjust_angle(15); }
            Button (
                "⇄ 180°",
                position: Position::Absolute,
                left: 90,
                top: 60,
                width: 42,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: button,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.flip_angle(); }
        }
    }
}

#[compose]
fn node_row(model: model!(OrbitModel), index: usize) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 24,
            top: 91 + 48 * index as i32,
            width: 262,
            height: 38
        ) {
            Text (
                text: ${
                    NodeText {
                        node: model.queue()[index],
                        number: index + 1,
                    }
                },
                capacity: 48,
                position: Position::Absolute,
                left: 11,
                top: 12,
                width: 200,
                height: 14,
                font_size: 10,
                text_color: ${ if model.queue()[index].is_some() { INK } else { MUTED } },
                paragraph: label_style()
            )
            Button (
                "×",
                visible: ${ model.queue()[index].is_some() },
                position: Position::Absolute,
                left: 222,
                top: 7,
                width: 32,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { let _ = model.cancel_at(index); }
        }
    }
}

#[compose]
fn compose_plan_page(model: model!(OrbitModel)) -> Entity {
    ui! {
        View (
            id: "orbit_plan_page",
            visible: ${ page_shown(OrbitPage::Plan, model.page(), model.modal()) },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 282
        ) {
            Text (
                text: ${ format_args!("节点  {} / 3", model.queue_len()) },
                capacity: 24,
                id: "orbit_queue_count",
                position: Position::Absolute,
                left: 24,
                top: 72,
                width: 200,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: label_style()
            )
            node_row (model, 0)
            node_row (model, 1)
            node_row (model, 2)
            Text (
                "按 ＋ 节点，到时自动点火。失联后也会执行。",
                visible: ${ model.queue_len() == 0 },
                position: Position::Absolute,
                left: 24,
                top: 234,
                width: 262,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                "节点延时",
                position: Position::Absolute,
                left: 325,
                top: 70,
                width: 66,
                height: 14,
                font_size: 10,
                text_color: LINE,
                paragraph: label_style()
            )
            Text (
                text: ${
                    FireTime {
                        time: model.time(),
                        delay: model.delay_seconds(),
                    }
                },
                capacity: 16,
                id: "orbit_next_node",
                position: Position::Absolute,
                left: 391,
                top: 70,
                width: 66,
                height: 14,
                font_size: 10,
                text_color: LINE,
                paragraph: end_style()
            )
            Text (
                text: ${ format_args!("{} s", model.delay_seconds()) },
                capacity: 8,
                id: "orbit_delay",
                position: Position::Absolute,
                left: 325,
                top: 85,
                width: 132,
                height: 24,
                font_size: 20,
                text_color: BACKGROUND,
                paragraph: center_style()
            )
            Button (
                "－5",
                position: Position::Absolute,
                left: 325,
                top: 112,
                width: 30,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: DEEP,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.adjust_delay(-5); }
            Button (
                "－1",
                position: Position::Absolute,
                left: 359,
                top: 112,
                width: 30,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: DEEP,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.adjust_delay(-1); }
            Button (
                "＋1",
                position: Position::Absolute,
                left: 393,
                top: 112,
                width: 30,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: DEEP,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.adjust_delay(1); }
            Button (
                "＋5",
                position: Position::Absolute,
                left: 427,
                top: 112,
                width: 30,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: DEEP,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.adjust_delay(5); }
            burn_dial (model, 325, 142, true)
            Text (
                text: ${
                    PlannedFuel {
                        planned: model.planned_fuel(),
                        fuel: model.fuel(),
                    }
                },
                capacity: 40,
                id: "orbit_plan_cost",
                position: Position::Absolute,
                left: 325,
                top: 229,
                width: 132,
                height: 14,
                font_size: 10,
                text_color: ORANGE,
                paragraph: label_style()
            )
        }
    }
}

#[compose]
fn log_line(model: model!(OrbitModel), line: usize) -> Entity {
    ui! {
        Text (
            text: ${
                EventLine {
                    event: model.log()[line],
                    goals: MISSIONS[usize::from(model.mission())].goal_count(),
                }
            },
            capacity: 48,
            position: Position::Absolute,
            left: 323,
            top: 90 + 18 * line as i32,
            width: 140,
            height: 14,
            font_size: 10,
            text_color: INK,
            paragraph: label_style()
        )
    }
}

#[compose]
fn compose_record_page(model: model!(OrbitModel)) -> Entity {
    ui! {
        View (
            id: "orbit_record_page",
            visible: ${ page_shown(OrbitPage::Record, model.page(), model.modal()) },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 282
        ) {
            Text (
                "RADIUS · 30 s",
                position: Position::Absolute,
                left: 15,
                top: 73,
                width: 140,
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ Decimal1(model.radius()) },
                capacity: 16,
                id: "orbit_record_radius",
                position: Position::Absolute,
                left: 201,
                top: 73,
                width: 100,
                height: 12,
                font_size: 10,
                text_color: ORANGE,
                paragraph: end_style()
            )
            Text (
                "SPEED · 30 s",
                position: Position::Absolute,
                left: 15,
                top: 175,
                width: 140,
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ Decimal1(model.speed()) },
                capacity: 16,
                id: "orbit_record_speed",
                position: Position::Absolute,
                left: 201,
                top: 175,
                width: 100,
                height: 12,
                font_size: 10,
                text_color: CYAN,
                paragraph: end_style()
            )
            Text (
                "▶ 滑行后开始记录",
                visible: ${ model.telemetry_len() < 2 },
                position: Position::Absolute,
                left: 15,
                top: 118,
                width: 286,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: center_style()
            )
            Text (
                "LOG",
                position: Position::Absolute,
                left: 323,
                top: 73,
                width: 60,
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: label_style()
            )
            log_line (model, 0)
            log_line (model, 1)
            log_line (model, 2)
            log_line (model, 3)
            log_line (model, 4)
            log_line (model, 5)
            Text (
                text: ${ status_label(model.status()) },
                capacity: 16,
                id: "orbit_status",
                position: Position::Absolute,
                left: 323,
                top: 232,
                width: 134,
                height: 14,
                font_size: 10,
                text_color: ORANGE,
                paragraph: label_style()
            )
        }
    }
}

#[compose]
fn compose_footer(model: model!(OrbitModel)) -> Entity {
    ui! {
        Button (
            text: ${ run_label(model.status(), model.has_next()) },
            capacity: 16,
            id: "orbit_footer_run",
            position: Position::Absolute,
            left: 12,
            top: 288,
            width: 82,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(true, run_enabled(model.status(), model.has_next())) },
            pressed_color: ORANGE,
            text_color: ${ control_text_color(run_enabled(model.status(), model.has_next())) },
            border_radius: 0
        ) on Tap {
            if model.status() == OrbitStatus::Won {
                model.next_mission();
            } else {
                model.toggle_running();
            }
        }
    };
    ui! {
        Button (
            text: ${ burn_label(model.burn_block()) },
            capacity: 16,
            id: "orbit_footer_burn",
            position: Position::Absolute,
            left: 99,
            top: 288,
            width: 70,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(false, model.burn_block().is_none()) },
            pressed_color: ORANGE,
            text_color: ${ control_text_color(model.burn_block().is_none()) },
            border_radius: 0
        ) on Tap { let _ = model.burn(); }
    };
    ui! {
        Button (
            text: ${ if model.eligible() { "● 采样" } else { "采样" } },
            capacity: 16,
            id: "orbit_footer_scan",
            position: Position::Absolute,
            left: 174,
            top: 288,
            width: 91,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.eligible(), model.eligible()) },
            pressed_color: ORANGE,
            text_color: ${ control_text_color(model.eligible()) },
            border_radius: 0
        ) on Tap { let _ = model.scan(); }
    };
    ui! {
        Button (
            text: ${ node_label(model.schedule_block()) },
            capacity: 16,
            id: "orbit_footer_schedule",
            position: Position::Absolute,
            left: 270,
            top: 288,
            width: 85,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(false, model.schedule_block().is_none()) },
            pressed_color: ORANGE,
            text_color: ${ control_text_color(model.schedule_block().is_none()) },
            border_radius: 0
        ) on Tap { let _ = model.schedule(); }
    };
    ui! {
        Button (
            "↺ 重来",
            id: "orbit_footer_reset",
            position: Position::Absolute,
            left: 360,
            top: 288,
            width: 108,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(false, model.can_restart()) },
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: ${ control_text_color(model.can_restart()) },
            border_radius: 0
        ) on Tap { model.restart(); }
    }
}

#[compose]
fn mission_tile(model: model!(OrbitModel), mission: u8) -> Entity {
    ui! {
        Button (
            text: ${
                MissionTile {
                    mission,
                    cleared: model.cleared(),
                }
            },
            capacity: 32,
            position: Position::Absolute,
            left: 50 + 97 * i32::from(mission % 4),
            top: 114 + 70 * i32::from(mission / 4),
            width: 89,
            height: 62,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.mission() == mission, true) },
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.choose_mission(mission); }
    }
}

#[compose]
fn compose_modal(model: model!(OrbitModel)) -> Entity {
    ui! {
        View (
            id: "orbit_modal",
            visible: ${ model.modal() != OrbitModal::None },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) [
            OrbitModalSurface {
                model: model.clone(),
            },
        ] {
            Button (
                "",
                id: "orbit_modal_shade",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                size: ButtonSize::Compact,
                normal_color: CLEAR,
                pressed_color: CLEAR,
                text_color: CLEAR,
                border_radius: 0
            ) on Tap { model.set_modal(OrbitModal::None); }
            Text (
                text: ${
                    ModalTitle {
                        modal: model.modal(),
                        mission: model.mission(),
                        status: model.status(),
                        cleared: model.cleared(),
                    }
                },
                capacity: 48,
                id: "orbit_modal_title",
                position: Position::Absolute,
                left: 55,
                top: 80,
                width: 260,
                height: 18,
                font_size: 10,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                text: ${ ClearedCount(model.cleared()) },
                capacity: 16,
                visible: ${ model.modal() == OrbitModal::Missions },
                id: "orbit_modal_count",
                position: Position::Absolute,
                left: 318,
                top: 80,
                width: 78,
                height: 18,
                font_size: 10,
                text_color: ORANGE,
                paragraph: end_style()
            )
            Text (
                text: ${
                    ModalBody {
                        modal: model.modal(),
                        mission: model.mission(),
                        status: model.status(),
                        time: model.time(),
                        fuel: model.fuel(),
                    }
                },
                capacity: 128,
                id: "orbit_modal_body",
                position: Position::Absolute,
                left: 55,
                top: 116,
                width: 370,
                height: 30,
                font_size: 10,
                text_color: INK,
                paragraph: body_style()
            )
            Text (
                text: ${
                    ResultNote {
                        modal: model.modal(),
                        status: model.status(),
                        mission: model.mission(),
                        cleared: model.cleared(),
                    }
                },
                capacity: 48,
                id: "orbit_modal_note",
                position: Position::Absolute,
                left: 55,
                top: 150,
                width: 370,
                height: 14,
                font_size: 10,
                text_color: ORANGE,
                paragraph: label_style()
            )
            View (
                visible: ${ model.modal() == OrbitModal::Brief },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320
            ) {
                Text (
                    text: ${
                        GoalEntry {
                            mission: model.mission(),
                            index: 0,
                        }
                    },
                    capacity: 40,
                    id: "orbit_brief_goal_0",
                    position: Position::Absolute,
                    left: 55,
                    top: 148,
                    width: 370,
                    height: 14,
                    font_size: 10,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    text: ${
                        GoalEntry {
                            mission: model.mission(),
                            index: 1,
                        }
                    },
                    capacity: 40,
                    position: Position::Absolute,
                    left: 55,
                    top: 163,
                    width: 370,
                    height: 14,
                    font_size: 10,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    text: ${
                        GoalEntry {
                            mission: model.mission(),
                            index: 2,
                        }
                    },
                    capacity: 40,
                    position: Position::Absolute,
                    left: 55,
                    top: 178,
                    width: 370,
                    height: 14,
                    font_size: 10,
                    text_color: INK,
                    paragraph: label_style()
                )
                Text (
                    text: ${
                        BriefFoot {
                            mission: model.mission(),
                            hint: model.hint_shown(),
                        }
                    },
                    capacity: 128,
                    id: "orbit_brief_foot",
                    position: Position::Absolute,
                    left: 55,
                    top: 195,
                    width: 370,
                    height: 14,
                    font_size: 10,
                    text_color: ${ if model.hint_shown() { ORANGE } else { MUTED } },
                    paragraph: label_style()
                )
            }
            View (
                visible: ${ model.modal() == OrbitModal::Missions },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320
            ) {
                mission_tile (model, 0)
                mission_tile (model, 1)
                mission_tile (model, 2)
                mission_tile (model, 3)
                mission_tile (model, 4)
                mission_tile (model, 5)
                mission_tile (model, 6)
                mission_tile (model, 7)
            }
            Button (
                text: ${ modal_left_label(model.modal(), model.status(), model.hint_shown()) },
                capacity: 24,
                visible: ${ has_actions(model.modal()) },
                id: "orbit_modal_left",
                position: Position::Absolute,
                left: 55,
                top: 216,
                width: 176,
                height: 32,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap {
                match model.modal() {
                    OrbitModal::Brief => {
                        model.toggle_hint();
                    }
                    OrbitModal::Result if model.status() == OrbitStatus::Won => {
                        model.restart();
                    }
                    OrbitModal::Result => {
                        model.set_modal(OrbitModal::Missions);
                    }
                    _ => {
                        model.set_modal(OrbitModal::None);
                    }
                }
            }
            Button (
                text: ${ modal_right_label(model.modal(), model.status(), model.has_next()) },
                capacity: 24,
                visible: ${ has_actions(model.modal()) },
                id: "orbit_modal_right",
                position: Position::Absolute,
                left: 249,
                top: 216,
                width: 176,
                height: 32,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ORANGE,
                pressed_color: INK,
                pressed_text_color: BACKGROUND,
                text_color: SPACE,
                border_radius: 0
            ) on Tap {
                match model.modal() {
                    OrbitModal::Confirm(_) => {
                        model.confirm();
                    }
                    OrbitModal::Result if model.status() == OrbitStatus::Won && model.has_next() => {
                        model.next_mission();
                    }
                    OrbitModal::Result if model.status() == OrbitStatus::Won => {
                        model.set_modal(OrbitModal::Missions);
                    }
                    OrbitModal::Result => {
                        model.restart();
                    }
                    _ => {
                        model.set_modal(OrbitModal::None);
                    }
                }
            }
            Button (
                "×",
                id: "orbit_modal_close",
                position: Position::Absolute,
                left: 404,
                top: 77,
                width: 27,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.set_modal(OrbitModal::None); }
        }
    }
}

#[compose]
fn build_widgets(model: model!(OrbitModel)) {
    ui! {
        View (id: "orbit_surface", width: 480, height: 320, clip_children: true) [
            OrbitSurface {
                model: model.clone(),
            },
        ] {
            compose_header (model)
            compose_map_page (model)
            compose_plan_page (model)
            compose_record_page (model)
            compose_footer (model)
            compose_modal (model)
        }
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(PlayFontPlugin);
    #[cfg(feature = "persistence")]
    match crate::gallery::play::storage::gallery_storage("mirui_orbital_mission.bin") {
        Ok(storage) => {
            app.add_plugin(
                crate::core::persistence::PersistencePlugin::new(storage).autosave(1000),
            );
        }
        Err(error) => crate::warn!("orbital mission progress is not saved: {error}"),
    }
    let model = app.add_model(OrbitModel::default()).finish();
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    app.add_system(orbit_tick_system::system(model.clone()));
    app.compose(parent, build_widgets(model));
}
