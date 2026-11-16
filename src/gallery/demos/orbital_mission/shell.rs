use super::input::{keyboard, orbit_tick_system};
use super::render::{modal_render, surface_render};
use super::state::{OrbitModalSurface, OrbitSurface};
use super::style::{
    ANGLE_BOX, BACKGROUND, CLEAR, CYAN, DEEP, DV_BOX, FOOTER_TOP, INK, LINE, MUTED, ORANGE, SPACE,
    body_style, center_style, end_style, label_style,
};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::i18n::{Locale, Phrase, locale};
use crate::gallery::play::font::GameFontPlugin;
use crate::gallery::play::orbit::{
    MISSION_COUNT, MISSIONS, ManeuverNode, OrbitError, OrbitEvent, OrbitEventKind, OrbitGoal,
    OrbitModal, OrbitModel, OrbitPage, OrbitStatus, START_RADIUS,
};
use crate::prelude::*;
use crate::tr;
use crate::types::Fixed64;
use crate::ui::widgets::{Button, ButtonSize, Text};
use core::fmt;

/// Footer buttons sit this far below the top of the footer.
const FOOTER_BUTTON: i32 = FOOTER_TOP + 4;

fn control_color(active: bool, enabled: bool) -> Color {
    if !enabled {
        DEEP
    } else if active {
        ORANGE
    } else {
        INK
    }
}

fn control_text_color(enabled: bool) -> Color {
    if enabled { BACKGROUND } else { MUTED }
}

fn chinese() -> bool {
    locale() == Locale::ZhCn
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

/// "Outer 113~130" or "Beacon · max 24".
struct GoalName(OrbitGoal);

impl fmt::Display for GoalName {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            OrbitGoal::Band { min, max } => {
                let side = if i64::from(min) > START_RADIUS {
                    tr!(en: "Outer", zh: "外圈")
                } else {
                    tr!(en: "Inner", zh: "内圈")
                };
                write!(out, "{side} {min}~{max}")
            }
            OrbitGoal::Beacon { max_speed, .. } => {
                write!(
                    out,
                    "{} {max_speed}",
                    tr!(en: "Beacon · max", zh: "信标 · 限速")
                )
            }
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
        let goal = tr!(en: "Goal", zh: "目标");
        match mission.goal(usize::from(self.completed)) {
            Some(next) => write!(
                out,
                "{goal} {}/{total} · {}",
                self.completed + 1,
                GoalName(next)
            ),
            None => write!(
                out,
                "{goal} {total}/{total} · {}",
                tr!(en: "done", zh: "完成")
            ),
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

/// What the craft can do at the current goal, or why the last command was
/// refused.
struct ZoneNotice {
    refusal: Option<OrbitError>,
    goal: Option<OrbitGoal>,
    in_zone: bool,
    eligible: bool,
}

impl ZoneNotice {
    fn too_fast(&self, out: &mut fmt::Formatter<'_>) -> Option<fmt::Result> {
        match self.goal {
            Some(OrbitGoal::Beacon { max_speed, .. }) if self.in_zone => Some(write!(
                out,
                "{} {max_speed}",
                tr!(en: "Too fast · max", zh: "速度过快，限速")
            )),
            _ => None,
        }
    }
}

impl fmt::Display for ZoneNotice {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(refusal) = self.refusal {
            let reason = match refusal {
                OrbitError::Fuel => tr!(en: "Not enough fuel for this Δv", zh: "燃料不足，调小 Δv"),
                OrbitError::Heat => tr!(en: "Too hot · let it cool", zh: "过热，等它冷却"),
                OrbitError::NoContact => {
                    tr!(en: "Contact lost · only nodes fire", zh: "已失联，只执行节点")
                }
                OrbitError::QueueFull => tr!(en: "Three nodes at most", zh: "最多 3 个节点"),
                OrbitError::OutsideWindow => match self.too_fast(out) {
                    Some(result) => return result,
                    None => tr!(en: "Not at the goal yet", zh: "还没到目标"),
                },
                OrbitError::Terminal => tr!(en: "The flight is over", zh: "飞行已结束"),
                OrbitError::InvalidDelay | OrbitError::MissingNode => return Ok(()),
            };
            return write!(out, "{reason}");
        }
        if self.eligible {
            return write!(
                out,
                "{}",
                tr!(en: "● In range · scan now", zh: "● 已到目标，可以采样")
            );
        }
        self.too_fast(out).unwrap_or(Ok(()))
    }
}

struct ContactText(Option<u16>);

impl fmt::Display for ContactText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => Ok(()),
            Some(0) => write!(
                out,
                "{}",
                tr!(en: "Contact lost · nodes only", zh: "已失联 · 只执行节点")
            ),
            Some(tenths) if chinese() => write!(out, "{} s 后失联", Tenths(tenths)),
            Some(tenths) => write!(out, "Contact lost in {} s", Tenths(tenths)),
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
        let words = match event.kind {
            OrbitEventKind::Loaded => tr!(en: "Start", zh: "任务开始"),
            OrbitEventKind::Burn(dv) => {
                let burn = tr!(en: "Burn", zh: "点火");
                return write!(out, "{burn} Δv {}", Tenths(dv.into()));
            }
            OrbitEventKind::NodeBurn(dv) => {
                let node = tr!(en: "Node", zh: "节点点火");
                return write!(out, "{node} Δv {}", Tenths(dv.into()));
            }
            OrbitEventKind::NodeSkipped(OrbitError::Fuel) => {
                tr!(en: "Skipped: no fuel", zh: "节点跳过：缺燃料")
            }
            OrbitEventKind::NodeSkipped(OrbitError::Heat) => {
                tr!(en: "Skipped: too hot", zh: "节点跳过：过热")
            }
            OrbitEventKind::NodeSkipped(_) => tr!(en: "Node skipped", zh: "节点跳过"),
            OrbitEventKind::Goal(done) => {
                let scan = tr!(en: "Scan", zh: "采样");
                return write!(out, "{scan} {done}/{}", self.goals);
            }
            OrbitEventKind::Won => tr!(en: "Complete", zh: "任务完成"),
            OrbitEventKind::Crashed => tr!(en: "Crashed", zh: "撞上行星"),
            OrbitEventKind::Escaped => tr!(en: "Lost in space", zh: "飞出边界"),
        };
        write!(out, "{words}")
    }
}

fn cleared(mask: u8, mission: u8) -> bool {
    mask & (1 << mission) != 0
}

/// "03 Lower orbit ★".
struct MissionTitle {
    mission: u8,
    cleared: u8,
}

impl fmt::Display for MissionTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "0{} {}",
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

fn burn_label(block: Option<OrbitError>) -> Phrase {
    match block {
        None | Some(OrbitError::Terminal) => tr!(en: "Burn", zh: "点火"),
        Some(OrbitError::Fuel) => tr!(en: "No fuel", zh: "缺燃料"),
        Some(OrbitError::Heat) => tr!(en: "Too hot", zh: "过热"),
        Some(_) => tr!(en: "No link", zh: "已失联"),
    }
}

/// The plan page's action: plan a node, and when it fires.
struct NodeButton {
    block: Option<OrbitError>,
    time: Fixed64,
    delay: u8,
}

impl fmt::Display for NodeButton {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = match self.block {
            Some(OrbitError::QueueFull) => tr!(en: "Three nodes planned", zh: "已有 3 个节点"),
            Some(OrbitError::NoContact) => tr!(en: "No link", zh: "已失联"),
            Some(OrbitError::Terminal) => tr!(en: "＋ Node", zh: "＋ 节点"),
            _ => {
                let fire = FireTime {
                    time: self.time,
                    delay: self.delay,
                };
                return write!(out, "{} · {fire}", tr!(en: "＋ Node", zh: "＋ 节点"));
            }
        };
        write!(out, "{words}")
    }
}

fn run_label(status: OrbitStatus) -> &'static str {
    if status == OrbitStatus::Running {
        "Ⅱ"
    } else {
        "▶"
    }
}

fn run_enabled(status: OrbitStatus, has_next: bool) -> bool {
    !status.terminal() || (status == OrbitStatus::Won && has_next)
}

fn status_label(status: OrbitStatus) -> Phrase {
    match status {
        OrbitStatus::Ready => tr!(en: "Ready", zh: "待命"),
        OrbitStatus::Running => tr!(en: "Flying", zh: "飞行中"),
        OrbitStatus::Paused => tr!(en: "Paused", zh: "已暂停"),
        OrbitStatus::Won => tr!(en: "Complete", zh: "完成"),
        OrbitStatus::Crashed => tr!(en: "Crashed", zh: "撞上行星"),
        OrbitStatus::Escaped => tr!(en: "Lost", zh: "出界"),
    }
}

struct PlanTab(u8);

impl fmt::Display for PlanTab {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plan = tr!(en: "PLAN", zh: "计划");
        match self.0 {
            0 => write!(out, "{plan}"),
            count => write!(out, "{plan} · {count}"),
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
        write!(
            out,
            "{} {}",
            tr!(en: "NODE FUEL", zh: "节点燃料"),
            Tenths(self.planned)
        )?;
        if Fixed64::from_ratio(self.planned.into(), 10) > self.fuel {
            write!(out, "{}", tr!(en: " · short", zh: " · 不足"))?;
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
        let number = self.mission + 1;
        match self.modal {
            OrbitModal::None => Ok(()),
            OrbitModal::Missions => write!(out, "{}", tr!(en: "Orbital Mission", zh: "轨道任务台")),
            OrbitModal::Brief => write!(
                out,
                "{} 0{number} · {}",
                tr!(en: "Mission", zh: "任务"),
                name(self.mission)
            ),
            OrbitModal::Confirm(target) if target == self.mission => write!(
                out,
                "{}",
                tr!(en: "Restart this mission?", zh: "重来本任务？")
            ),
            OrbitModal::Confirm(target) if chinese() => {
                write!(out, "换到 0{} · {}？", target + 1, name(target))
            }
            OrbitModal::Confirm(target) => {
                write!(out, "Switch to 0{} · {}?", target + 1, name(target))
            }
            OrbitModal::Result => match self.status {
                OrbitStatus::Won if self.cleared.count_ones() as usize == MISSION_COUNT => {
                    if chinese() {
                        write!(out, "{MISSION_COUNT} 个任务全部完成 ★")
                    } else {
                        write!(out, "All {MISSION_COUNT} missions complete ★")
                    }
                }
                OrbitStatus::Won => write!(
                    out,
                    "0{number} {} {} ★",
                    name(self.mission),
                    tr!(en: "complete", zh: "完成")
                ),
                OrbitStatus::Crashed => {
                    write!(
                        out,
                        "{}",
                        tr!(en: "Crashed into the planet", zh: "撞上行星")
                    )
                }
                _ => write!(out, "{}", tr!(en: "Lost in space", zh: "飞出边界")),
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
        let words = match self.modal {
            OrbitModal::Brief => MISSIONS[usize::from(self.mission)].description,
            OrbitModal::Confirm(_) => tr!(
                en: "The flight and the plan will be cleared.",
                zh: "当前的飞行和计划会清空。"
            ),
            OrbitModal::Result => match self.status {
                OrbitStatus::Won => {
                    let (time, fuel) = (Decimal1(self.time), Decimal1(self.fuel));
                    return if chinese() {
                        write!(out, "用时 {time} s · 燃料还有 {fuel}")
                    } else {
                        write!(out, "Time {time} s · fuel left {fuel}")
                    };
                }
                OrbitStatus::Crashed => tr!(
                    en: "Below radius 27 the craft hits the planet.",
                    zh: "距离不到 27 就会撞上。"
                ),
                _ => tr!(
                    en: "Past radius 260 the craft is lost.",
                    zh: "距离超过 260 就出界了。"
                ),
            },
            OrbitModal::None | OrbitModal::Missions => return Ok(()),
        };
        write!(out, "{words}")
    }
}

/// The foot of the briefing: the hint once asked for, else how to fly and
/// the keys.
struct BriefFoot {
    mission: u8,
    hint: bool,
}

impl fmt::Display for BriefFoot {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.hint {
            return write!(
                out,
                "{}{}",
                tr!(en: "Hint: ", zh: "提示："),
                MISSIONS[usize::from(self.mission)].hint
            );
        }
        write!(
            out,
            "{}\n{}",
            tr!(
                en: "Set Δv and angle, burn, ▶ to coast, scan at the goal.",
                zh: "调好 Δv 和角度后点火，按 ▶ 滑行，到目标后采样。"
            ),
            tr!(
                en: "Keys: ↑↓ Δv  ←→ angle  B burn  Space ▶  E scan  N node",
                zh: "按键：↑↓ Δv  ←→ 角度  B 点火  空格 ▶  E 采样  N 节点"
            )
        )
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
            write!(
                out,
                "   {}0{} {}",
                tr!(en: "Next: ", zh: "下一个："),
                next + 1,
                mission.name
            )?;
        }
        Ok(())
    }
}

fn modal_left_label(modal: OrbitModal, status: OrbitStatus, hint: bool) -> Phrase {
    match modal {
        OrbitModal::Brief if hint => tr!(en: "Hide hint", zh: "收起提示"),
        OrbitModal::Brief => tr!(en: "Hint", zh: "提示"),
        OrbitModal::Confirm(_) => tr!(en: "Cancel", zh: "取消"),
        OrbitModal::Result if status == OrbitStatus::Won => {
            tr!(en: "↺ Fly again", zh: "↺ 再飞一次")
        }
        OrbitModal::Result => tr!(en: "Missions", zh: "选择任务"),
        OrbitModal::None | OrbitModal::Missions => tr!(en: "", zh: ""),
    }
}

fn modal_right_label(modal: OrbitModal, status: OrbitStatus, has_next: bool) -> Phrase {
    match modal {
        OrbitModal::Brief if status == OrbitStatus::Ready => tr!(en: "Start", zh: "开始"),
        OrbitModal::Brief => tr!(en: "Resume", zh: "继续"),
        OrbitModal::Confirm(_) => tr!(en: "OK", zh: "确定"),
        OrbitModal::Result if status == OrbitStatus::Won && has_next => {
            tr!(en: "Next mission ▶", zh: "下一个任务 ▶")
        }
        OrbitModal::Result if status == OrbitStatus::Won => tr!(en: "Missions", zh: "选择任务"),
        OrbitModal::Result => tr!(en: "↺ Retry", zh: "↺ 重来"),
        OrbitModal::None | OrbitModal::Missions => tr!(en: "", zh: ""),
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

/// The header tab colors: orange for the page on show.
fn tab_color(page: OrbitPage, current: OrbitPage) -> Color {
    if page == current { ORANGE } else { DEEP }
}

fn tab_text_color(page: OrbitPage, current: OrbitPage) -> Color {
    if page == current { SPACE } else { BACKGROUND }
}

#[compose]
fn compose_header(model: model!(OrbitModel)) -> Entity {
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
            left: 4,
            top: 3,
            width: 164,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: DEEP,
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_modal(OrbitModal::Missions); }
    };
    ui! {
        Button (
            tr!(en : "MAP", zh : "航图"),
            id: "orbit_tab_map",
            position: Position::Absolute,
            left: 174,
            top: 3,
            width: 50,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ tab_color(OrbitPage::Map, model.page()) },
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: ${ tab_text_color(OrbitPage::Map, model.page()) },
            border_radius: 0
        ) on Tap { model.set_page(OrbitPage::Map); }
    };
    ui! {
        Button (
            text: ${ PlanTab(model.queue_len()) },
            capacity: 16,
            id: "orbit_tab_plan",
            position: Position::Absolute,
            left: 226,
            top: 3,
            width: 66,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ tab_color(OrbitPage::Plan, model.page()) },
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: ${ tab_text_color(OrbitPage::Plan, model.page()) },
            border_radius: 0
        ) on Tap { model.set_page(OrbitPage::Plan); }
    };
    ui! {
        Button (
            tr!(en : "LOG", zh : "记录"),
            id: "orbit_tab_record",
            position: Position::Absolute,
            left: 294,
            top: 3,
            width: 50,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ tab_color(OrbitPage::Record, model.page()) },
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: ${ tab_text_color(OrbitPage::Record, model.page()) },
            border_radius: 0
        ) on Tap { model.set_page(OrbitPage::Record); }
    };
    ui! {
        Text (
            text: ${ format_args!("T+{}", Decimal1(model.time())) },
            capacity: 16,
            id: "orbit_time",
            position: Position::Absolute,
            left: 348,
            top: 8,
            width: 60,
            height: 14,
            font_size: 10,
            text_color: LINE,
            paragraph: end_style()
        )
    };
    ui! {
        Button (
            "↺",
            id: "orbit_restart",
            position: Position::Absolute,
            left: 414,
            top: 3,
            width: 28,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: DEEP,
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: ${ control_text_color(model.can_restart()) },
            border_radius: 0
        ) on Tap { model.restart(); }
    };
    ui! {
        Button (
            "?",
            id: "orbit_brief",
            position: Position::Absolute,
            left: 446,
            top: 3,
            width: 30,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: DEEP,
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_modal(OrbitModal::Brief); }
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
            height: 284
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
                left: 8,
                top: 35,
                width: 300,
                height: 14,
                font_size: 10,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                text: ${
                    ZoneNotice {
                        refusal: model.refusal(),
                        goal: model.current_goal(),
                        in_zone: model.in_zone(),
                        eligible: model.eligible(),
                    }
                },
                capacity: 64,
                id: "orbit_notice",
                position: Position::Absolute,
                left: 8,
                top: 50,
                width: 300,
                height: 14,
                font_size: 10,
                text_color: ORANGE,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{} {}", tr!(en : "FUEL", zh : "燃料"), Decimal1(model.fuel())) },
                capacity: 24,
                id: "orbit_fuel",
                position: Position::Absolute,
                left: 386,
                top: 35,
                width: 88,
                height: 14,
                font_size: 10,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{} {}%", tr!(en : "HEAT", zh : "热量"), model.heat().to_int()) },
                capacity: 24,
                id: "orbit_heat",
                position: Position::Absolute,
                left: 386,
                top: 56,
                width: 88,
                height: 14,
                font_size: 10,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{} {}", tr!(en : "RADIUS", zh : "距离"), Decimal1(model.radius())) },
                capacity: 24,
                id: "orbit_radius",
                position: Position::Absolute,
                left: 386,
                top: 78,
                width: 88,
                height: 14,
                font_size: 10,
                text_color: LINE,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{} {}", tr!(en : "SPEED", zh : "速度"), Decimal1(model.speed())) },
                capacity: 24,
                id: "orbit_speed",
                position: Position::Absolute,
                left: 386,
                top: 93,
                width: 88,
                height: 14,
                font_size: 10,
                text_color: LINE,
                paragraph: label_style()
            )
            Button (
                text: ${
                    if model.preview() {
                        tr!(en : "◉ Preview", zh : "◉ 预演")
                    } else {
                        tr!(en : "○ Preview", zh : "○ 预演")
                    }
                },
                capacity: 16,
                id: "orbit_preview",
                position: Position::Absolute,
                left: 4,
                top: 256,
                width: 80,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: SPACE,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: ${ if model.preview() { ORANGE } else { LINE } },
                border_radius: 0
            ) on Tap { model.toggle_preview(); }
            Text (
                text: ${ ContactText(model.contact_left()) },
                capacity: 48,
                id: "orbit_contact",
                position: Position::Absolute,
                left: 120,
                top: 261,
                width: 240,
                height: 14,
                font_size: 10,
                text_color: LINE,
                paragraph: center_style()
            )
            Button (
                text: ${ format_args!("{}×", model.warp()) },
                capacity: 8,
                id: "orbit_warp",
                position: Position::Absolute,
                left: 434,
                top: 256,
                width: 42,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: SPACE,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.cycle_warp(); }
        }
    }
}

#[compose]
fn node_row(model: model!(OrbitModel), index: usize) -> Entity {
    ui! {
        View (
            position: Position::Absolute,
            left: 8,
            top: 54 + 44 * index as i32,
            width: 300,
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
                left: 14,
                top: 12,
                width: 240,
                height: 14,
                font_size: 10,
                text_color: ${ if model.queue()[index].is_some() { INK } else { MUTED } },
                paragraph: label_style()
            )
            Button (
                "×",
                visible: ${ model.queue()[index].is_some() },
                position: Position::Absolute,
                left: 264,
                top: 7,
                width: 30,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.cancel_at(index).ok(); }
        }
    }
}

/// A delay step on the plan page.
#[compose]
fn delay_step(model: model!(OrbitModel), label: &'static str, left: i32, delta: i8) -> Entity {
    ui! {
        Button (
            label,
            position: Position::Absolute,
            left: left,
            top: 88,
            width: 34,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: DEEP,
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.adjust_delay(delta); }
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
            height: 284
        ) {
            Text (
                text: ${ format_args!("{}  {} / 3", tr!(en : "NODES", zh : "节点"), model.queue_len()) },
                capacity: 24,
                id: "orbit_queue_count",
                position: Position::Absolute,
                left: 12,
                top: 37,
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
                tr!(
                    en : "A node fires on time, even after contact is lost.", zh :
                    "节点到时自动点火，失联后也会执行。"
                ),
                visible: ${ model.queue_len() == 0 },
                position: Position::Absolute,
                left: 12,
                top: 192,
                width: 296,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                tr!(en : "DELAY", zh : "延时"),
                position: Position::Absolute,
                left: 326,
                top: 38,
                width: 70,
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
                left: 396,
                top: 38,
                width: 76,
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
                left: 326,
                top: 56,
                width: 146,
                height: 24,
                font_size: 20,
                text_color: BACKGROUND,
                paragraph: center_style()
            )
            delay_step (model, "－5", 326, -5)
            delay_step (model, "－1", 363, -1)
            delay_step (model, "＋1", 400, 1)
            delay_step (model, "＋5", 437, 5)
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
                left: 326,
                top: 128,
                width: 146,
                height: 14,
                font_size: 10,
                text_color: ORANGE,
                paragraph: label_style()
            )
            Text (
                tr!(
                    en : "Nodes burn with the Δv and angle set below.", zh :
                    "节点按下方的 Δv 和角度点火。"
                ),
                position: Position::Absolute,
                left: 326,
                top: 160,
                width: 146,
                height: 45,
                font_size: 10,
                text_color: LINE,
                paragraph: body_style()
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
            left: 348,
            top: 58 + 18 * line as i32,
            width: 124,
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
            height: 284
        ) {
            Text (
                tr!(en : "RADIUS · 30 s", zh : "距离 · 30 s"),
                position: Position::Absolute,
                left: 10,
                top: 37,
                width: 160,
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
                left: 230,
                top: 37,
                width: 100,
                height: 12,
                font_size: 10,
                text_color: ORANGE,
                paragraph: end_style()
            )
            Text (
                tr!(en : "SPEED · 30 s", zh : "速度 · 30 s"),
                position: Position::Absolute,
                left: 10,
                top: 157,
                width: 160,
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
                left: 230,
                top: 157,
                width: 100,
                height: 12,
                font_size: 10,
                text_color: CYAN,
                paragraph: end_style()
            )
            Text (
                tr!(en : "Press ▶ to start recording", zh : "按 ▶ 滑行后开始记录"),
                visible: ${ model.telemetry_len() < 2 },
                position: Position::Absolute,
                left: 10,
                top: 94,
                width: 320,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: center_style()
            )
            Text (
                tr!(en : "EVENTS", zh : "事件"),
                position: Position::Absolute,
                left: 348,
                top: 40,
                width: 120,
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
                left: 348,
                top: 258,
                width: 124,
                height: 14,
                font_size: 10,
                text_color: ORANGE,
                paragraph: label_style()
            )
        }
    }
}

/// A Δv step of the footer dial.
#[compose]
fn dv_step(
    model: model!(OrbitModel),
    label: &'static str,
    left: i32,
    width: i32,
    delta: i8,
) -> Entity {
    ui! {
        Button (
            label,
            position: Position::Absolute,
            left: left,
            top: FOOTER_BUTTON,
            width: width,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: INK,
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.adjust_dv(delta); }
    }
}

/// An angle step of the footer dial.
#[compose]
fn angle_step(model: model!(OrbitModel), label: &'static str, left: i32, delta: i16) -> Entity {
    ui! {
        Button (
            label,
            position: Position::Absolute,
            left: left,
            top: FOOTER_BUTTON,
            width: 30,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: INK,
            pressed_color: ORANGE,
            pressed_text_color: SPACE,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.adjust_angle(delta); }
    }
}

/// The footer: coast, the burn dial, and what the page does with it.
#[compose]
fn compose_footer(model: model!(OrbitModel)) -> Entity {
    ui! {
        Button (
            text: ${ run_label(model.status()) },
            capacity: 8,
            id: "orbit_footer_run",
            position: Position::Absolute,
            left: 6,
            top: FOOTER_BUTTON,
            width: 30,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(true, run_enabled(model.status(), model.has_next())) },
            pressed_color: INK,
            pressed_text_color: BACKGROUND,
            text_color: ${ if run_enabled(model.status(), model.has_next()) { SPACE } else { MUTED } },
            border_radius: 0
        ) on Tap { model.press_run(); }
    };
    ui! {
        View (
            id: "orbit_dial",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) {
            dv_step (model, "－1", 42, 24, -10)
            dv_step (model, "－.2", 68, 26, -2)
            Text (
                text: ${ format_args!("Δv {}", Tenths(model.dv_tenths().into())) },
                capacity: 12,
                id: "orbit_dv",
                position: Position::Absolute,
                left: DV_BOX.0,
                top: FOOTER_BUTTON + 7,
                width: DV_BOX.1,
                height: 14,
                font_size: 10,
                text_color: BACKGROUND,
                paragraph: center_style()
            )
            dv_step (model, "＋.2", 142, 26, 2)
            dv_step (model, "＋1", 170, 24, 10)
            angle_step (model, "－15°", 200, -15)
            Text (
                text: ${ format_args!("{}°", model.angle_degrees()) },
                capacity: 8,
                id: "orbit_angle",
                position: Position::Absolute,
                left: ANGLE_BOX.0,
                top: FOOTER_BUTTON + 7,
                width: ANGLE_BOX.1,
                height: 14,
                font_size: 10,
                text_color: BACKGROUND,
                paragraph: center_style()
            )
            angle_step (model, "＋15°", 272, 15)
            Button (
                "⇄",
                id: "orbit_flip",
                position: Position::Absolute,
                left: 304,
                top: FOOTER_BUTTON,
                width: 26,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.flip_angle(); }
        }
    };
    ui! {
        View (
            id: "orbit_flight_actions",
            visible: ${ model.page() != OrbitPage::Plan },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) {
            Button (
                text: ${ burn_label(model.burn_block()) },
                capacity: 16,
                id: "orbit_footer_burn",
                position: Position::Absolute,
                left: 336,
                top: FOOTER_BUTTON,
                width: 68,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ control_color(true, model.burn_block().is_none()) },
                pressed_color: INK,
                pressed_text_color: BACKGROUND,
                text_color: ${ if model.burn_block().is_none() { SPACE } else { MUTED } },
                border_radius: 0
            ) on Tap { model.press_burn(); }
            Button (
                text: ${
                    if model.eligible() {
                        tr!(en : "● Scan", zh : "● 采样")
                    } else {
                        tr!(en : "Scan", zh : "采样")
                    }
                },
                capacity: 16,
                id: "orbit_footer_scan",
                position: Position::Absolute,
                left: 408,
                top: FOOTER_BUTTON,
                width: 66,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ${ if model.eligible() { CYAN } else { DEEP } },
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: ${ if model.eligible() { SPACE } else { MUTED } },
                border_radius: 0
            ) on Tap { model.press_scan(); }
        }
    };
    ui! {
        Button (
            text: ${
                NodeButton {
                    block: model.schedule_block(),
                    time: model.time(),
                    delay: model.delay_seconds(),
                }
            },
            capacity: 40,
            id: "orbit_footer_schedule",
            visible: ${ model.page() == OrbitPage::Plan },
            position: Position::Absolute,
            left: 336,
            top: FOOTER_BUTTON,
            width: 138,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(true, model.schedule_block().is_none()) },
            pressed_color: INK,
            pressed_text_color: BACKGROUND,
            text_color: ${ if model.schedule_block().is_none() { SPACE } else { MUTED } },
            border_radius: 0
        ) on Tap { model.press_schedule(); }
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
            capacity: 40,
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
fn brief_goal(model: model!(OrbitModel), index: usize) -> Entity {
    ui! {
        Text (
            text: ${
                GoalEntry {
                    mission: model.mission(),
                    index,
                }
            },
            capacity: 48,
            position: Position::Absolute,
            left: 55,
            top: 148 + 14 * index as i32,
            width: 370,
            height: 14,
            font_size: 10,
            text_color: INK,
            paragraph: label_style()
        )
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
                capacity: 64,
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
                capacity: 160,
                id: "orbit_modal_body",
                position: Position::Absolute,
                left: 55,
                top: 114,
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
                capacity: 64,
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
                brief_goal (model, 0)
                brief_goal (model, 1)
                brief_goal (model, 2)
                Text (
                    text: ${
                        BriefFoot {
                            mission: model.mission(),
                            hint: model.hint_shown(),
                        }
                    },
                    capacity: 256,
                    id: "orbit_brief_foot",
                    position: Position::Absolute,
                    left: 55,
                    top: 192,
                    width: 370,
                    height: 30,
                    font_size: 10,
                    text_color: ${ if model.hint_shown() { ORANGE } else { MUTED } },
                    paragraph: body_style()
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
                top: 226,
                width: 176,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: INK,
                pressed_color: ORANGE,
                pressed_text_color: SPACE,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.modal_secondary(); }
            Button (
                text: ${ modal_right_label(model.modal(), model.status(), model.has_next()) },
                capacity: 24,
                visible: ${ has_actions(model.modal()) },
                id: "orbit_modal_right",
                position: Position::Absolute,
                left: 249,
                top: 226,
                width: 176,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ORANGE,
                pressed_color: INK,
                pressed_text_color: BACKGROUND,
                text_color: SPACE,
                border_radius: 0
            ) on Tap { model.modal_primary(); }
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
    app.add_plugin(GameFontPlugin);
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
    app.on_event(Key::EVENT, keyboard::callback(model.clone()));
    app.compose(parent, build_widgets(model));
}
