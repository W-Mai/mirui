use crate::gallery::play::factory::{
    FactoryCell, FactoryError, FactoryModal, FactoryStatus, FactoryTool, GRID_WIDTH, MISSIONS,
    MaterialStage, ModuleKind,
};
use core::fmt;

pub(super) fn direction_glyph(direction: u8) -> &'static str {
    match direction % 4 {
        0 => "→",
        1 => "↓",
        2 => "←",
        _ => "↑",
    }
}

/// The build button names the tool in hand.
pub(super) fn tool_button_label(tool: FactoryTool) -> &'static str {
    match tool {
        FactoryTool::Select => "＋ 建造",
        FactoryTool::Build(ModuleKind::Belt) => "＋ 传送带",
        FactoryTool::Build(ModuleKind::Furnace) => "＋ 熔炼炉",
        FactoryTool::Build(ModuleKind::Assembler) => "＋ 装配机",
        FactoryTool::Build(ModuleKind::Inspector) => "＋ 质检台",
        FactoryTool::Build(_) => "＋ 建造",
        FactoryTool::Erase => "－ 拆除",
    }
}

pub(super) fn run_label(has_next: bool, running: bool) -> &'static str {
    if has_next {
        "下一单 ▶"
    } else if running {
        "Ⅱ 暂停"
    } else {
        "▶ 运行"
    }
}

pub(super) struct StatusText {
    pub(super) status: FactoryStatus,
    pub(super) notice: Option<FactoryError>,
    /// The order's task, shown until the first run.
    pub(super) brief: &'static str,
    pub(super) wip: u8,
    pub(super) blocked: u8,
    pub(super) rejected: u16,
    pub(super) delivered: u16,
    pub(super) goal: u16,
    pub(super) tick: u16,
    pub(super) power: u8,
    pub(super) power_cap: u8,
    pub(super) completed: u8,
}

impl fmt::Display for StatusText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.notice {
            Some(FactoryError::BudgetExceeded) => {
                return out.write_str("信用不足，先拆掉其他模块。");
            }
            Some(FactoryError::ImmutableCell) => {
                return out.write_str("进料口、交付口和隔墙不能改。");
            }
            _ => {}
        }
        if self.power > self.power_cap {
            return write!(
                out,
                "功率 {} / {} 超标，拆掉机器才能运行。",
                self.power, self.power_cap
            );
        }
        match self.status {
            FactoryStatus::Won if self.completed.count_ones() as usize == MISSIONS.len() => {
                write!(out, "★ {} 张订单全部完成。", MISSIONS.len())
            }
            FactoryStatus::Won => write!(out, "★ 订单完成，用时 {} tick。", self.tick),
            FactoryStatus::Timeout => {
                write!(out, "超时：交付 {} / {}。", self.delivered, self.goal)
            }
            FactoryStatus::Ready => self
                .brief
                .split('\n')
                .try_for_each(|line| out.write_str(line)),
            FactoryStatus::Running => write!(
                out,
                "试产中 · 在制 {} · 堵塞 {} · 退回 {}",
                self.wip, self.blocked, self.rejected
            ),
            FactoryStatus::Paused => write!(
                out,
                "已暂停 · 在制 {} · 堵塞 {} · 退回 {}",
                self.wip, self.blocked, self.rejected
            ),
        }
    }
}

/// `01  试产零件`, the label of an order tile.
pub(super) struct OrderTitle(pub(super) usize);

impl fmt::Display for OrderTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{:02}  {}", self.0 + 1, MISSIONS[self.0].name)
    }
}

/// `ORDER 01 · 试产零件` in the header.
pub(super) struct OrderMeta(pub(super) u8);

impl fmt::Display for OrderMeta {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let index = usize::from(self.0);
        write!(out, "ORDER {:02} · {}", index + 1, MISSIONS[index].name)
    }
}

/// The goal, run limit, credits and power of an order, one per line.
pub(super) struct OrderSpecs(pub(super) u8);

impl fmt::Display for OrderSpecs {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let order = MISSIONS[usize::from(self.0)];
        write!(
            out,
            "{} {}\n{} tick\n{}\n{}",
            order.goal,
            order.target.label(),
            order.limit,
            order.budget,
            order.power
        )
    }
}

pub(super) struct ModuleName(pub(super) Option<FactoryCell>);

impl fmt::Display for ModuleName {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => out.write_str("空位"),
            Some(cell) if matches!(cell.kind, ModuleKind::Dock | ModuleKind::Wall) => {
                out.write_str(cell.kind.label())
            }
            Some(cell) => write!(
                out,
                "{} {}",
                cell.kind.label(),
                direction_glyph(cell.direction)
            ),
        }
    }
}

/// What the highlighted cell does, in one line.
pub(super) struct ModuleDetail {
    pub(super) cell: Option<FactoryCell>,
    pub(super) target: MaterialStage,
    pub(super) tool: FactoryTool,
}

impl fmt::Display for ModuleDetail {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(cell) = self.cell else {
            return match self.tool {
                FactoryTool::Build(kind) => write!(out, "点一下放{}", kind.label()),
                FactoryTool::Select | FactoryTool::Erase => Ok(()),
            };
        };
        if let Some((input, output, ticks)) = cell.kind.process() {
            return write!(
                out,
                "{} → {} · {} tick",
                input.label(),
                output.label(),
                ticks
            );
        }
        match cell.kind {
            ModuleKind::Source => out.write_str("每 3 tick 出一块矿石"),
            ModuleKind::Dock => write!(out, "只认{}，其余退回", self.target.label()),
            ModuleKind::Wall => out.write_str("挡路，不能拆"),
            _ => out.write_str("每 tick 送一格"),
        }
    }
}

pub(super) struct CellCoordinate(pub(super) usize);

impl fmt::Display for CellCoordinate {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "C{} · R{}",
            self.0 % GRID_WIDTH + 1,
            self.0 / GRID_WIDTH + 1
        )
    }
}

pub(super) struct ModalTitle {
    pub(super) modal: FactoryModal,
    pub(super) mission: u8,
}

impl fmt::Display for ModalTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.modal {
            FactoryModal::None => Ok(()),
            FactoryModal::Tools => out.write_str("建造模块"),
            FactoryModal::Help => out.write_str("操作手册"),
            FactoryModal::Confirm {
                reference: true, ..
            } => out.write_str("装载示范布局？"),
            FactoryModal::Confirm { mission, .. } if mission == self.mission => {
                out.write_str("这一单从头来过？")
            }
            FactoryModal::Confirm { mission, .. } => {
                let index = usize::from(mission);
                write!(out, "换到 {:02} {}？", index + 1, MISSIONS[index].name)
            }
        }
    }
}

pub(super) fn modal_subtitle(modal: FactoryModal) -> &'static str {
    match modal {
        FactoryModal::None | FactoryModal::Help => "",
        FactoryModal::Tools => "选好后点格子放置。改布局会让试产从 T+0 重来。",
        FactoryModal::Confirm { .. } => "改过的布局不会保留。",
    }
}

pub(super) fn modal_button_label(modal: FactoryModal, index: usize) -> &'static str {
    match modal {
        FactoryModal::Tools => [
            "选择",
            "传送带 · 1 信用",
            "熔炼炉 · 4 信用 · 3 功率",
            "装配机 · 5 信用 · 4 功率",
            "质检台 · 3 信用 · 2 功率",
            "拆除",
        ][index],
        FactoryModal::Confirm { .. } => ["取消", "确认", "", "", "", ""][index],
        FactoryModal::Help | FactoryModal::None => "",
    }
}

pub(super) const HELP_FLOW: &str = "矿石 / ORE\n↓ 熔炼炉 3 tick\n板材 / PLATE\n↓ 装配机 4 tick\n零件 / GEAR\n↓ 质检台 2 tick\n合格品 / CERT";

pub(super) const HELP_CONTROLS: &str = "点格子：选中模块\n＋ 建造：选好模块，再点格子\n旋转 ↻：转动高亮的模块\n新模块会接上相邻断头的传送带\n交付口只认订单要的产品\n信用或功率超标时不能运行";
