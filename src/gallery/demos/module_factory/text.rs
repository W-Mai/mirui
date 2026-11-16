use crate::core::i18n::{Locale, Phrase, locale};
use crate::gallery::play::factory::{
    FactoryCell, FactoryError, FactoryModal, FactoryStatus, FactoryTool, MISSIONS, MaterialStage,
    ModuleKind,
};
use crate::tr;
use core::fmt;

pub(super) const TITLE: Phrase = tr!(en: "MODULE FACTORY", zh: "模块工厂");

fn chinese() -> bool {
    locale() == Locale::ZhCn
}

pub(super) fn direction_glyph(direction: u8) -> &'static str {
    match direction % 4 {
        0 => "→",
        1 => "↓",
        2 => "←",
        _ => "↑",
    }
}

/// The build button names the tool in hand.
pub(super) struct ToolLabel(pub(super) FactoryTool);

impl fmt::Display for ToolLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            FactoryTool::Select => write!(out, "＋ {}", tr!(en: "Build", zh: "建造")),
            FactoryTool::Build(kind) => write!(out, "＋ {}", kind.label()),
            FactoryTool::Erase => write!(out, "－ {}", tr!(en: "Remove", zh: "拆除")),
        }
    }
}

pub(super) struct RunLabel {
    pub(super) has_next: bool,
    pub(super) running: bool,
}

impl fmt::Display for RunLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = if self.has_next {
            tr!(en: "Next ▶", zh: "下一单 ▶")
        } else if self.running {
            tr!(en: "Ⅱ Pause", zh: "Ⅱ 暂停")
        } else {
            tr!(en: "▶ Run", zh: "▶ 运行")
        };
        write!(out, "{words}")
    }
}

/// `120 ticks` / `120 刻`.
pub(super) struct Ticks(pub(super) u16);

impl fmt::Display for Ticks {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if chinese() {
            write!(out, "{} 刻", self.0)
        } else if self.0 == 1 {
            out.write_str("1 tick")
        } else {
            write!(out, "{} ticks", self.0)
        }
    }
}

pub(super) struct StatusText {
    pub(super) status: FactoryStatus,
    pub(super) notice: Option<FactoryError>,
    /// The order's task, shown until the first run.
    pub(super) brief: Phrase,
    pub(super) wip: u8,
    pub(super) blocked: u8,
    pub(super) rejected: u16,
    pub(super) delivered: u16,
    pub(super) goal: u16,
    pub(super) target: MaterialStage,
    pub(super) tick: u16,
    pub(super) power: u8,
    pub(super) power_cap: u8,
    pub(super) completed: u8,
}

impl fmt::Display for StatusText {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let zh = chinese();
        match self.notice {
            Some(FactoryError::BudgetExceeded) => {
                let words = tr!(
                    en: "Not enough credits: remove a module first.",
                    zh: "信用不足，先拆掉其他模块。"
                );
                return write!(out, "{words}");
            }
            Some(FactoryError::ImmutableCell) => {
                let words = tr!(
                    en: "Inlets, docks and walls stay where they are.",
                    zh: "进料口、交付口和隔墙不能改。"
                );
                return write!(out, "{words}");
            }
            _ => {}
        }
        if self.power > self.power_cap {
            return if zh {
                write!(
                    out,
                    "功率 {} / {} 超标，拆掉机器才能运行。",
                    self.power, self.power_cap
                )
            } else {
                write!(
                    out,
                    "Power {} / {} is over the cap: remove a machine to run.",
                    self.power, self.power_cap
                )
            };
        }
        let line = (self.wip, self.blocked, self.rejected);
        match self.status {
            FactoryStatus::Won if self.completed.count_ones() as usize == MISSIONS.len() => {
                if zh {
                    write!(out, "★ {} 张订单全部完成。", MISSIONS.len())
                } else {
                    write!(out, "★ All {} orders done.", MISSIONS.len())
                }
            }
            FactoryStatus::Won if zh => write!(out, "★ 订单完成，用时 {}。", Ticks(self.tick)),
            FactoryStatus::Won => write!(out, "★ Order done in {}.", Ticks(self.tick)),
            FactoryStatus::Timeout if zh => {
                write!(out, "超时：交付 {} / {}。", self.delivered, self.goal)
            }
            FactoryStatus::Timeout => write!(
                out,
                "Time is up: {} / {} {} delivered.",
                self.delivered,
                self.goal,
                self.target.plural()
            ),
            FactoryStatus::Ready => self
                .brief
                .get()
                .split('\n')
                .try_for_each(|part| out.write_str(part)),
            FactoryStatus::Running if zh => write!(
                out,
                "试产中 · 在制 {} · 堵塞 {} · 退回 {}",
                line.0, line.1, line.2
            ),
            FactoryStatus::Running => write!(
                out,
                "Running · on the line {} · blocked {} · sent back {}",
                line.0, line.1, line.2
            ),
            FactoryStatus::Paused if zh => write!(
                out,
                "已暂停 · 在制 {} · 堵塞 {} · 退回 {}",
                line.0, line.1, line.2
            ),
            FactoryStatus::Paused => write!(
                out,
                "Paused · on the line {} · blocked {} · sent back {}",
                line.0, line.1, line.2
            ),
        }
    }
}

/// The stars of an order, the earned ones filled: `★★☆`.
pub(super) struct StarRow(pub(super) u8);

impl fmt::Display for StarRow {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        for star in 0..3 {
            out.write_str(if star < self.0 { "★" } else { "☆" })?;
        }
        Ok(())
    }
}

/// `01  Trial Run`, the label of an order tile.
pub(super) struct OrderTitle(pub(super) usize);

impl fmt::Display for OrderTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{:02}  {}", self.0 + 1, MISSIONS[self.0].name)
    }
}

/// `01 · Trial Run` in the header.
pub(super) struct OrderMeta(pub(super) u8);

impl fmt::Display for OrderMeta {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let index = usize::from(self.0);
        write!(out, "{:02} · {}", index + 1, MISSIONS[index].name)
    }
}

/// `ORDER 01` over the brief of an order.
pub(super) struct OrderNumber(pub(super) u8);

impl fmt::Display for OrderNumber {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "{} {:02}",
            tr!(en: "ORDER", zh: "订单"),
            usize::from(self.0) + 1
        )
    }
}

pub(super) const SPEC_NAMES: Phrase = tr!(en: "TIME\nCREDITS\nPOWER", zh: "限时\n信用\n功率");

/// `Ship 6 gears`: what an order asks for.
pub(super) struct OrderGoal(pub(super) u8);

impl fmt::Display for OrderGoal {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let order = MISSIONS[usize::from(self.0)];
        write!(
            out,
            "{} {} {}",
            tr!(en: "Ship", zh: "交付"),
            order.goal,
            order.target.plural()
        )
    }
}

/// The run limit, credits and power of an order, one per line.
pub(super) struct OrderSpecs(pub(super) u8);

impl fmt::Display for OrderSpecs {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let order = MISSIONS[usize::from(self.0)];
        write!(
            out,
            "{}\n{}\n{}",
            Ticks(order.limit),
            order.budget,
            order.power
        )
    }
}

pub(super) struct ModuleName(pub(super) Option<FactoryCell>);

impl fmt::Display for ModuleName {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => write!(out, "{}", tr!(en: "Empty", zh: "空位")),
            Some(cell) if matches!(cell.kind, ModuleKind::Dock | ModuleKind::Wall) => {
                write!(out, "{}", cell.kind.label())
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

/// What the highlighted cell does, in a line or two.
pub(super) struct ModuleDetail {
    pub(super) cell: Option<FactoryCell>,
    pub(super) target: MaterialStage,
    pub(super) tool: FactoryTool,
}

impl fmt::Display for ModuleDetail {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let zh = chinese();
        let Some(cell) = self.cell else {
            return match self.tool {
                FactoryTool::Build(kind) if zh => write!(out, "点一下放{}", kind.label()),
                FactoryTool::Build(kind) => write!(out, "Tap to place a {}", kind.label()),
                FactoryTool::Select | FactoryTool::Erase => Ok(()),
            };
        };
        if let Some((input, output, ticks)) = cell.kind.process() {
            return write!(
                out,
                "{} → {} · {}",
                input.label(),
                output.label(),
                Ticks(u16::from(ticks))
            );
        }
        match cell.kind {
            ModuleKind::Source if zh => out.write_str("每 3 刻出一块矿石"),
            ModuleKind::Source => out.write_str("One ore every 3 ticks"),
            ModuleKind::Dock if zh => write!(out, "只认{}，其余退回", self.target.label()),
            ModuleKind::Dock => write!(
                out,
                "Takes {} only, sends the rest back",
                self.target.plural()
            ),
            ModuleKind::Wall if zh => out.write_str("挡路，不能拆"),
            ModuleKind::Wall => out.write_str("Blocks the way, stays put"),
            _ if zh => out.write_str("每刻送一格"),
            _ => out.write_str("Moves items one cell a tick"),
        }
    }
}

pub(super) struct ModalTitle {
    pub(super) modal: FactoryModal,
    pub(super) mission: u8,
}

impl fmt::Display for ModalTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.modal {
            FactoryModal::None | FactoryModal::Result => Ok(()),
            FactoryModal::Tools => write!(out, "{}", tr!(en: "Build", zh: "建造模块")),
            FactoryModal::Help => write!(out, "{}", tr!(en: "How to play", zh: "操作手册")),
            FactoryModal::Confirm {
                reference: true, ..
            } => write!(
                out,
                "{}",
                tr!(en: "Load the example line?", zh: "装载示范布局？")
            ),
            FactoryModal::Confirm { mission, .. } if mission == self.mission => write!(
                out,
                "{}",
                tr!(en: "Start this order over?", zh: "这一单从头来过？")
            ),
            FactoryModal::Confirm { mission, .. } => {
                let index = usize::from(mission);
                if chinese() {
                    write!(out, "换到 {:02} {}？", index + 1, MISSIONS[index].name)
                } else {
                    write!(out, "Switch to {:02} {}?", index + 1, MISSIONS[index].name)
                }
            }
        }
    }
}

pub(super) fn modal_subtitle(modal: FactoryModal) -> Phrase {
    match modal {
        FactoryModal::None | FactoryModal::Help | FactoryModal::Result => tr!(en: "", zh: ""),
        FactoryModal::Tools => tr!(
            en: "Pick one, then tap cells. Any edit restarts the trial at T+0.",
            zh: "选好后点格子放置。改布局会让试产从 T+0 重来。"
        ),
        FactoryModal::Confirm { .. } => tr!(
            en: "Your edits to this line will be lost.",
            zh: "改过的布局不会保留。"
        ),
    }
}

pub(super) fn modal_button_label(modal: FactoryModal, index: usize) -> Phrase {
    match modal {
        FactoryModal::Tools => [
            tr!(en: "Select", zh: "选择"),
            tr!(en: "Belt · 1 credit", zh: "传送带 · 1 信用"),
            tr!(en: "Furnace · 4 credits · 3 power", zh: "熔炼炉 · 4 信用 · 3 功率"),
            tr!(en: "Assembler · 5 credits · 4 power", zh: "装配机 · 5 信用 · 4 功率"),
            tr!(en: "Inspector · 3 credits · 2 power", zh: "质检台 · 3 信用 · 2 功率"),
            tr!(en: "Remove", zh: "拆除"),
        ][index],
        FactoryModal::Confirm { .. } if index == 0 => tr!(en: "Cancel", zh: "取消"),
        FactoryModal::Confirm { .. } if index == 1 => tr!(en: "OK", zh: "确认"),
        _ => tr!(en: "", zh: ""),
    }
}

pub(super) const HELP_FLOW: Phrase = tr!(
    en: "Ore\n↓ furnace, 3 ticks\nPlate\n↓ assembler, 4 ticks\nGear\n↓ inspector, 2 ticks\nCertified",
    zh: "矿石\n↓ 熔炼炉 3 刻\n板材\n↓ 装配机 4 刻\n零件\n↓ 质检台 2 刻\n合格品"
);

pub(super) const HELP_CONTROLS: Phrase = tr!(
    en: "Tap a cell to select its module\n＋ Build: pick a module, tap cells\n↻ turns the highlighted module\nNew modules join loose belt ends\nA dock takes only the ordered goods\nNo run over the credit or power cap",
    zh: "点格子：选中模块\n＋ 建造：选好模块，再点格子\n↻ 转动高亮的模块\n新模块会接上相邻断头的传送带\n交付口只认订单要的产品\n信用或功率超标时不能运行"
);

pub(super) const HELP_KEYS: Phrase = tr!(
    en: "Keys: arrows move · Enter places · R turns · Z undo · Space run · T step",
    zh: "按键：方向键 移动 · 回车 放置 · R 旋转 · Z 撤销 · 空格 运行 · T 单步"
);

/// `Order 01 done` on the result card.
pub(super) struct ResultTitle(pub(super) u8);

impl fmt::Display for ResultTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if chinese() {
            write!(out, "订单 {:02} 完成", self.0 + 1)
        } else {
            write!(out, "Order {:02} done", self.0 + 1)
        }
    }
}

/// Under the stars: the time, the credits and the example's credits.
pub(super) struct ResultLine {
    pub(super) tick: u16,
    pub(super) cost: u8,
    pub(super) par: u8,
}

impl fmt::Display for ResultLine {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if chinese() {
            write!(out, "用时 {} · 信用 {}", Ticks(self.tick), self.cost)?;
            if self.cost > self.par {
                write!(out, " · 最少 {}", self.par)?;
            }
        } else {
            write!(out, "{} · {} credits", Ticks(self.tick), self.cost)?;
            if self.cost > self.par {
                write!(out, " · par {}", self.par)?;
            }
        }
        Ok(())
    }
}

pub(super) struct NextLabel(pub(super) bool);

impl fmt::Display for NextLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = if self.0 {
            tr!(en: "Next order ▶", zh: "下一单 ▶")
        } else {
            tr!(en: "Orders", zh: "订单")
        };
        write!(out, "{words}")
    }
}
