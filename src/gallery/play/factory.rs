use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;

pub(crate) const CELL_COUNT: usize = 54;
pub(crate) const GRID_WIDTH: usize = 9;
pub(crate) const GRID_HEIGHT: usize = 6;
pub(crate) const MAX_UNDO: usize = 32;
pub(crate) const MAX_TELEMETRY: usize = 80;
pub(crate) const MISSION_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModuleKind {
    Source,
    Dock,
    Belt,
    Furnace,
    Assembler,
    Inspector,
}

impl ModuleKind {
    pub(crate) const BUILDABLE: [Self; 4] =
        [Self::Belt, Self::Furnace, Self::Assembler, Self::Inspector];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Source => "IN",
            Self::Dock => "OUT",
            Self::Belt => "传送带",
            Self::Furnace => "熔炼炉",
            Self::Assembler => "装配机",
            Self::Inspector => "质检台",
        }
    }

    pub(crate) const fn cost(self) -> u8 {
        match self {
            Self::Belt => 1,
            Self::Furnace => 4,
            Self::Assembler => 5,
            Self::Inspector => 3,
            Self::Source | Self::Dock => 0,
        }
    }

    pub(crate) const fn power(self) -> u8 {
        match self {
            Self::Furnace => 3,
            Self::Assembler => 4,
            Self::Inspector => 2,
            Self::Source | Self::Dock | Self::Belt => 0,
        }
    }

    const fn accepts(self, stage: MaterialStage) -> bool {
        match self {
            Self::Source => false,
            Self::Furnace => matches!(stage, MaterialStage::Ore),
            Self::Assembler => matches!(stage, MaterialStage::Plate),
            Self::Inspector => matches!(stage, MaterialStage::Gear),
            Self::Dock | Self::Belt => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FactoryCell {
    pub(crate) kind: ModuleKind,
    pub(crate) direction: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaterialStage {
    Ore,
    Plate,
    Gear,
    Certified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FactoryItem {
    pub(crate) id: u16,
    pub(crate) stage: MaterialStage,
    pub(crate) age: u8,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct FactoryTelemetry {
    pub(crate) tick: u16,
    pub(crate) delivered: u16,
    pub(crate) blocked: u8,
    pub(crate) wip: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactoryPage {
    Line,
    Orders,
    Telemetry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactoryTool {
    Select,
    Build(ModuleKind),
    Erase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactoryModal {
    None,
    Tools,
    Confirm { mission: u8, reference: bool },
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactoryStatus {
    Ready,
    Running,
    Paused,
    Won,
    Timeout,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FactoryError {
    InvalidCell,
    ImmutableCell,
    InvalidModule,
    Duplicate,
    BudgetExceeded,
    PowerExceeded,
    NothingSelected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FactoryMission {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) goal: u16,
    pub(crate) budget: u8,
    pub(crate) power: u8,
    pub(crate) target: MaterialStage,
}

pub(crate) const MISSIONS: [FactoryMission; MISSION_COUNT] = [
    FactoryMission {
        name: "微型装配",
        description: "把矿石变成零件，交付 8 件。先补齐第 6 列缺口。",
        goal: 8,
        budget: 23,
        power: 9,
        target: MaterialStage::Gear,
    },
    FactoryMission {
        name: "折返产线",
        description: "跨两条走廊生产零件。转弯必须指向下一格。",
        goal: 10,
        budget: 34,
        power: 10,
        target: MaterialStage::Gear,
    },
    FactoryMission {
        name: "质量检验",
        description: "装配后的零件要经过质检，才能计入订单。",
        goal: 8,
        budget: 27,
        power: 10,
        target: MaterialStage::Certified,
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FactorySnapshot {
    cells: [Option<FactoryCell>; CELL_COUNT],
}

impl FactorySnapshot {
    const EMPTY: Self = Self {
        cells: [None; CELL_COUNT],
    };
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FactoryModel {
    cells: [Option<FactoryCell>; CELL_COUNT],
    items: [Option<FactoryItem>; CELL_COUNT],
    history: [FactorySnapshot; MAX_UNDO],
    telemetry: [FactoryTelemetry; MAX_TELEMETRY],
    clock: BoundedClock,
    page: FactoryPage,
    tool: FactoryTool,
    modal: FactoryModal,
    status: FactoryStatus,
    mission: u8,
    selection: u8,
    tool_direction: u8,
    history_len: u8,
    telemetry_start: u8,
    telemetry_len: u8,
    blocked: u8,
    tick: u16,
    delivered: u16,
    rejected: u16,
    produced: u16,
    next_item_id: u16,
    running: bool,
}

impl Default for FactoryModel {
    fn default() -> Self {
        let mut model = Self {
            cells: [None; CELL_COUNT],
            items: [None; CELL_COUNT],
            history: [FactorySnapshot::EMPTY; MAX_UNDO],
            telemetry: [FactoryTelemetry::default(); MAX_TELEMETRY],
            clock: BoundedClock::new(2, 1_000, 2),
            page: FactoryPage::Line,
            tool: FactoryTool::Select,
            modal: FactoryModal::None,
            status: FactoryStatus::Ready,
            mission: 0,
            selection: 23,
            tool_direction: 0,
            history_len: 0,
            telemetry_start: 0,
            telemetry_len: 0,
            blocked: 0,
            tick: 0,
            delivered: 0,
            rejected: 0,
            produced: 0,
            next_item_id: 1,
            running: false,
        };
        model.load_mission_state(0, false);
        model
    }
}

impl FactoryModel {
    pub(crate) const fn page(&self) -> FactoryPage {
        self.page
    }

    pub(crate) const fn modal(&self) -> FactoryModal {
        self.modal
    }

    pub(crate) const fn tool(&self) -> FactoryTool {
        self.tool
    }

    pub(crate) const fn status(&self) -> FactoryStatus {
        self.status
    }

    pub(crate) const fn mission_index(&self) -> u8 {
        self.mission
    }

    pub(crate) const fn mission(&self) -> FactoryMission {
        MISSIONS[self.mission as usize]
    }

    pub(crate) const fn selected(&self) -> usize {
        self.selection as usize
    }

    pub(crate) const fn tool_direction(&self) -> u8 {
        self.tool_direction
    }

    pub(crate) const fn tick(&self) -> u16 {
        self.tick
    }

    pub(crate) const fn delivered(&self) -> u16 {
        self.delivered
    }

    pub(crate) const fn rejected(&self) -> u16 {
        self.rejected
    }

    pub(crate) const fn produced(&self) -> u16 {
        self.produced
    }

    pub(crate) const fn blocked(&self) -> u8 {
        self.blocked
    }

    pub(crate) const fn running(&self) -> bool {
        self.running
    }

    pub(crate) const fn history_len(&self) -> u8 {
        self.history_len
    }

    pub(crate) const fn telemetry_len(&self) -> u8 {
        self.telemetry_len
    }

    pub(crate) const fn cell(&self, index: usize) -> Option<FactoryCell> {
        if index < CELL_COUNT {
            self.cells[index]
        } else {
            None
        }
    }

    pub(crate) const fn item(&self, index: usize) -> Option<FactoryItem> {
        if index < CELL_COUNT {
            self.items[index]
        } else {
            None
        }
    }

    pub(crate) fn telemetry(&self, index: usize) -> Option<FactoryTelemetry> {
        if index >= usize::from(self.telemetry_len) {
            return None;
        }
        let physical = (usize::from(self.telemetry_start) + index) % MAX_TELEMETRY;
        Some(self.telemetry[physical])
    }

    pub(crate) fn cost(&self) -> u8 {
        self.cells
            .iter()
            .flatten()
            .map(|cell| cell.kind.cost())
            .sum()
    }

    pub(crate) fn power(&self) -> u8 {
        self.cells
            .iter()
            .flatten()
            .map(|cell| cell.kind.power())
            .sum()
    }

    pub(crate) fn wip(&self) -> u8 {
        self.items.iter().filter(|item| item.is_some()).count() as u8
    }

    pub(crate) fn set_page(&mut self, page: FactoryPage) -> ChangeSet {
        if self.page == page || self.modal != FactoryModal::None {
            return ChangeSet::NONE;
        }
        self.page = page;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn open_modal(&mut self, modal: FactoryModal) -> ChangeSet {
        if self.modal == modal {
            return ChangeSet::NONE;
        }
        self.modal = modal;
        self.clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn close_modal(&mut self) -> ChangeSet {
        if self.modal == FactoryModal::None {
            return ChangeSet::NONE;
        }
        self.modal = FactoryModal::None;
        self.clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn set_tool(&mut self, tool: FactoryTool) -> ChangeSet {
        if self.tool == tool {
            self.modal = FactoryModal::None;
            return ChangeSet::MODEL | ChangeSet::VISUAL;
        }
        self.tool = tool;
        self.modal = FactoryModal::None;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn rotate_tool(&mut self) -> ChangeSet {
        self.tool_direction = (self.tool_direction + 1) % 4;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn select_or_apply(&mut self, index: usize) -> Result<ChangeSet, FactoryError> {
        match self.tool {
            FactoryTool::Select => {
                if index >= CELL_COUNT {
                    return Err(FactoryError::InvalidCell);
                }
                self.selection = index as u8;
                Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
            }
            FactoryTool::Build(kind) => self.edit(index, Some(kind), self.tool_direction),
            FactoryTool::Erase => self.edit(index, None, 0),
        }
    }

    pub(crate) fn edit(
        &mut self,
        index: usize,
        kind: Option<ModuleKind>,
        direction: u8,
    ) -> Result<ChangeSet, FactoryError> {
        if index >= CELL_COUNT {
            return Err(FactoryError::InvalidCell);
        }
        if self.cells[index]
            .is_some_and(|cell| matches!(cell.kind, ModuleKind::Source | ModuleKind::Dock))
        {
            return Err(FactoryError::ImmutableCell);
        }
        if kind.is_some_and(|kind| !ModuleKind::BUILDABLE.contains(&kind)) {
            return Err(FactoryError::InvalidModule);
        }
        let next = kind.map(|kind| FactoryCell {
            kind,
            direction: direction % 4,
        });
        if self.cells[index] == next {
            return Err(FactoryError::Duplicate);
        }
        let current_cost = self.cells[index].map_or(0, |cell| cell.kind.cost());
        let next_cost = next.map_or(0, |cell| cell.kind.cost());
        let cost = self.cost() - current_cost + next_cost;
        if cost > self.mission().budget {
            return Err(FactoryError::BudgetExceeded);
        }
        self.remember();
        self.cells[index] = next;
        self.selection = index as u8;
        self.reset_run();
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn rotate_selected(&mut self) -> Result<ChangeSet, FactoryError> {
        let index = self.selected();
        let Some(cell) = self.cells[index] else {
            return Err(FactoryError::NothingSelected);
        };
        if matches!(cell.kind, ModuleKind::Source | ModuleKind::Dock) {
            return Err(FactoryError::ImmutableCell);
        }
        self.edit(index, Some(cell.kind), cell.direction + 1)
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.history_len == 0 {
            return ChangeSet::NONE;
        }
        self.history_len -= 1;
        self.cells = self.history[usize::from(self.history_len)].cells;
        self.reset_run();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn toggle_run(&mut self) -> Result<ChangeSet, FactoryError> {
        if self.power() > self.mission().power {
            return Err(FactoryError::PowerExceeded);
        }
        if matches!(self.status, FactoryStatus::Won | FactoryStatus::Timeout) {
            self.reset_run();
        }
        self.running = !self.running;
        self.status = if self.running {
            FactoryStatus::Running
        } else {
            FactoryStatus::Paused
        };
        self.clock.reset();
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn step_once(&mut self) -> Result<ChangeSet, FactoryError> {
        if self.power() > self.mission().power {
            return Err(FactoryError::PowerExceeded);
        }
        self.running = false;
        self.step_model();
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        let active = self.running && self.modal == FactoryModal::None;
        let steps = self.clock.steps(elapsed_ms, active);
        if steps == 0 {
            return ChangeSet::NONE;
        }
        for _ in 0..steps {
            if !self.step_model() {
                break;
            }
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn request_mission(&mut self, mission: u8, reference: bool) -> ChangeSet {
        self.modal = FactoryModal::Confirm {
            mission: mission.min((MISSION_COUNT - 1) as u8),
            reference,
        };
        self.clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn confirm_mission(&mut self) -> ChangeSet {
        let FactoryModal::Confirm { mission, reference } = self.modal else {
            return ChangeSet::NONE;
        };
        self.load_mission_state(mission, reference);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn load_mission_state(&mut self, mission: u8, reference: bool) {
        self.mission = mission.min((MISSION_COUNT - 1) as u8);
        self.cells = [None; CELL_COUNT];
        self.history_len = 0;
        self.selection = 23;
        self.page = FactoryPage::Line;
        self.tool = FactoryTool::Select;
        self.tool_direction = 0;
        self.modal = FactoryModal::None;
        match self.mission {
            0 => {
                self.put(0, 2, ModuleKind::Source, 0);
                self.put(1, 2, ModuleKind::Belt, 0);
                self.put(2, 2, ModuleKind::Furnace, 0);
                self.put(3, 2, ModuleKind::Belt, 0);
                self.put(4, 2, ModuleKind::Assembler, 0);
                if reference {
                    self.put(5, 2, ModuleKind::Belt, 0);
                }
                self.put(6, 2, ModuleKind::Belt, 0);
                self.put(7, 2, ModuleKind::Belt, 0);
                self.put(8, 2, ModuleKind::Dock, 0);
            }
            1 => {
                self.put(0, 1, ModuleKind::Source, 0);
                self.put(1, 1, ModuleKind::Belt, 0);
                self.put(2, 1, ModuleKind::Furnace, 0);
                self.put(3, 1, ModuleKind::Belt, 0);
                self.put(4, 1, ModuleKind::Belt, u8::from(reference));
                self.put(4, 2, ModuleKind::Belt, 1);
                self.put(4, 3, ModuleKind::Belt, 1);
                self.put(4, 4, ModuleKind::Assembler, 0);
                self.put(5, 4, ModuleKind::Belt, 0);
                if reference {
                    self.put(6, 4, ModuleKind::Belt, 0);
                }
                self.put(7, 4, ModuleKind::Belt, 0);
                self.put(8, 4, ModuleKind::Dock, 0);
            }
            _ => {
                self.put(0, 3, ModuleKind::Source, 0);
                self.put(1, 3, ModuleKind::Belt, 0);
                self.put(2, 3, ModuleKind::Furnace, 0);
                self.put(3, 3, ModuleKind::Belt, 0);
                self.put(4, 3, ModuleKind::Assembler, 0);
                self.put(5, 3, ModuleKind::Belt, 0);
                self.put(
                    6,
                    3,
                    if reference {
                        ModuleKind::Inspector
                    } else {
                        ModuleKind::Belt
                    },
                    0,
                );
                self.put(7, 3, ModuleKind::Belt, 0);
                self.put(8, 3, ModuleKind::Dock, 0);
            }
        }
        self.reset_run();
    }

    fn put(&mut self, x: usize, y: usize, kind: ModuleKind, direction: u8) {
        self.cells[y * GRID_WIDTH + x] = Some(FactoryCell { kind, direction });
    }

    fn remember(&mut self) {
        let snapshot = FactorySnapshot { cells: self.cells };
        if usize::from(self.history_len) < MAX_UNDO {
            self.history[usize::from(self.history_len)] = snapshot;
            self.history_len += 1;
        } else {
            self.history.copy_within(1..MAX_UNDO, 0);
            self.history[MAX_UNDO - 1] = snapshot;
        }
    }

    fn reset_run(&mut self) {
        self.items = [None; CELL_COUNT];
        self.telemetry = [FactoryTelemetry::default(); MAX_TELEMETRY];
        self.telemetry_start = 0;
        self.telemetry_len = 0;
        self.blocked = 0;
        self.tick = 0;
        self.delivered = 0;
        self.rejected = 0;
        self.produced = 0;
        self.next_item_id = 1;
        self.running = false;
        self.status = FactoryStatus::Ready;
        self.clock.reset();
    }

    fn step_model(&mut self) -> bool {
        if self.power() > self.mission().power
            || matches!(self.status, FactoryStatus::Won | FactoryStatus::Timeout)
        {
            return false;
        }
        self.tick = self.tick.saturating_add(1);
        self.blocked = 0;
        for index in 0..CELL_COUNT {
            let (Some(mut item), Some(cell)) = (self.items[index], self.cells[index]) else {
                continue;
            };
            item.age = item.age.saturating_add(1);
            let processed = match (cell.kind, item.stage, item.age) {
                (ModuleKind::Furnace, MaterialStage::Ore, age) if age >= 3 => {
                    Some(MaterialStage::Plate)
                }
                (ModuleKind::Assembler, MaterialStage::Plate, age) if age >= 4 => {
                    Some(MaterialStage::Gear)
                }
                (ModuleKind::Inspector, MaterialStage::Gear, age) if age >= 2 => {
                    Some(MaterialStage::Certified)
                }
                _ => None,
            };
            if let Some(stage) = processed {
                item.stage = stage;
                item.age = 0;
            }
            self.items[index] = Some(item);
        }

        let current = self.items;
        let mut next = current;
        let mut claimed = [false; CELL_COUNT];
        for index in 0..CELL_COUNT {
            let (Some(cell), Some(item)) = (self.cells[index], current[index]) else {
                continue;
            };
            let processing = matches!(
                (cell.kind, item.stage),
                (ModuleKind::Furnace, MaterialStage::Ore)
                    | (ModuleKind::Assembler, MaterialStage::Plate)
                    | (ModuleKind::Inspector, MaterialStage::Gear)
            );
            if processing {
                continue;
            }
            let (dx, dy) = match cell.direction % 4 {
                0 => (1_i32, 0_i32),
                1 => (0, 1),
                2 => (-1, 0),
                _ => (0, -1),
            };
            let x = (index % GRID_WIDTH) as i32 + dx;
            let y = (index / GRID_WIDTH) as i32 + dy;
            if x < 0 || y < 0 || x >= GRID_WIDTH as i32 || y >= GRID_HEIGHT as i32 {
                self.blocked = self.blocked.saturating_add(1);
                continue;
            }
            let target = y as usize * GRID_WIDTH + x as usize;
            let Some(target_cell) = self.cells[target] else {
                self.blocked = self.blocked.saturating_add(1);
                continue;
            };
            if current[target].is_some() || claimed[target] || !target_cell.kind.accepts(item.stage)
            {
                self.blocked = self.blocked.saturating_add(1);
                continue;
            }
            claimed[target] = true;
            next[index] = None;
            if target_cell.kind == ModuleKind::Dock {
                if item.stage == self.mission().target {
                    self.delivered = self.delivered.saturating_add(1);
                } else {
                    self.rejected = self.rejected.saturating_add(1);
                }
            } else {
                next[target] = Some(FactoryItem { age: 0, ..item });
            }
        }
        self.items = next;

        if self.tick % 3 == 1 {
            for index in 0..CELL_COUNT {
                if self.cells[index].is_some_and(|cell| cell.kind == ModuleKind::Source)
                    && self.items[index].is_none()
                {
                    self.items[index] = Some(FactoryItem {
                        id: self.next_item_id,
                        stage: MaterialStage::Ore,
                        age: 0,
                    });
                    self.next_item_id = self.next_item_id.saturating_add(1);
                    self.produced = self.produced.saturating_add(1);
                }
            }
        }
        self.push_telemetry();
        if self.delivered >= self.mission().goal {
            self.status = FactoryStatus::Won;
            self.running = false;
        } else if self.tick >= 300 {
            self.status = FactoryStatus::Timeout;
            self.running = false;
        } else if self.running {
            self.status = FactoryStatus::Running;
        } else {
            self.status = FactoryStatus::Paused;
        }
        true
    }

    fn push_telemetry(&mut self) {
        let value = FactoryTelemetry {
            tick: self.tick,
            delivered: self.delivered,
            blocked: self.blocked,
            wip: self.wip(),
        };
        if usize::from(self.telemetry_len) < MAX_TELEMETRY {
            let index = (usize::from(self.telemetry_start) + usize::from(self.telemetry_len))
                % MAX_TELEMETRY;
            self.telemetry[index] = value;
            self.telemetry_len += 1;
        } else {
            self.telemetry[usize::from(self.telemetry_start)] = value;
            self.telemetry_start = (self.telemetry_start + 1) % MAX_TELEMETRY as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_conserved(model: &FactoryModel) {
        assert_eq!(
            model.produced,
            model.delivered + model.rejected + u16::from(model.wip())
        );
        let mut ids = [0_u16; CELL_COUNT];
        let mut len = 0;
        for item in model.items.iter().flatten() {
            assert!(!ids[..len].contains(&item.id));
            ids[len] = item.id;
            len += 1;
        }
    }

    #[test]
    fn fixed_storage_budget_stays_small() {
        assert!(core::mem::size_of::<FactoryModel>() <= 8 * 1024);
    }

    #[test]
    fn reference_lines_finish_and_conserve_every_item() {
        for mission in 0..MISSION_COUNT as u8 {
            let mut model = FactoryModel::default();
            model.load_mission_state(mission, true);
            for _ in 0..300 {
                if !model.step_model() {
                    break;
                }
                assert_conserved(&model);
            }
            assert_eq!(model.status, FactoryStatus::Won);
            assert_eq!(model.delivered, model.mission().goal);
            assert!(!model.running);
        }
    }

    #[test]
    fn starter_lines_never_false_positive() {
        for mission in 0..MISSION_COUNT as u8 {
            let mut model = FactoryModel::default();
            model.load_mission_state(mission, false);
            for _ in 0..300 {
                model.step_model();
                assert_conserved(&model);
            }
            assert_ne!(model.status, FactoryStatus::Won);
        }
    }

    #[test]
    fn immutable_endpoints_and_budget_failures_are_atomic() {
        let mut model = FactoryModel::default();
        let before = model.cells;
        assert_eq!(model.edit(18, None, 0), Err(FactoryError::ImmutableCell));
        assert_eq!(model.cells, before);
        for index in [0, 1] {
            model.edit(index, Some(ModuleKind::Assembler), 0).unwrap();
        }
        let cells = model.cells;
        let history = model.history_len;
        assert_eq!(
            model.edit(2, Some(ModuleKind::Assembler), 0),
            Err(FactoryError::BudgetExceeded)
        );
        assert_eq!(model.cells, cells);
        assert_eq!(model.history_len, history);
    }

    #[test]
    fn rotate_undo_and_edit_reset_the_run() {
        let mut model = FactoryModel::default();
        let before = model.cells;
        model.selection = 19;
        model.rotate_selected().unwrap();
        assert_eq!(model.cells[19].unwrap().direction, 1);
        model.undo();
        assert_eq!(model.cells, before);
        model.load_mission_state(0, true);
        for _ in 0..20 {
            model.step_model();
        }
        model.edit(0, Some(ModuleKind::Belt), 0).unwrap();
        assert_eq!(model.tick, 0);
        assert_eq!(model.wip(), 0);
        assert_eq!(model.delivered, 0);
    }

    #[test]
    fn history_and_telemetry_are_bounded() {
        let mut model = FactoryModel::default();
        for index in 0..80 {
            model
                .edit(
                    0,
                    if index % 2 == 0 {
                        Some(ModuleKind::Belt)
                    } else {
                        None
                    },
                    0,
                )
                .unwrap();
        }
        assert_eq!(usize::from(model.history_len), MAX_UNDO);
        for _ in 0..300 {
            model.step_model();
        }
        assert_eq!(usize::from(model.telemetry_len), MAX_TELEMETRY);
        assert_eq!(model.telemetry(0).unwrap().tick, 221);
        assert_eq!(model.telemetry(MAX_TELEMETRY - 1).unwrap().tick, 300);
    }

    #[test]
    fn over_power_blocks_manual_and_automatic_steps() {
        let mut model = FactoryModel::default();
        model.edit(0, Some(ModuleKind::Assembler), 0).unwrap();
        assert!(model.power() > model.mission().power);
        assert_eq!(model.toggle_run(), Err(FactoryError::PowerExceeded));
        assert_eq!(model.step_once(), Err(FactoryError::PowerExceeded));
        assert_eq!(model.tick, 0);
    }

    #[test]
    fn modal_time_does_not_accumulate_debt() {
        let mut model = FactoryModel::default();
        model.toggle_run().unwrap();
        assert_eq!(model.advance_ms(400), ChangeSet::NONE);
        model.open_modal(FactoryModal::Help);
        assert_eq!(model.advance_ms(1_000), ChangeSet::NONE);
        model.close_modal();
        assert_eq!(model.advance_ms(100), ChangeSet::NONE);
        assert_eq!(model.tick, 0);
        assert!(model.advance_ms(400).contains(ChangeSet::MODEL));
        assert_eq!(model.tick, 1);
    }

    #[test]
    fn build_direction_cycles_without_editing_the_line() {
        let mut model = FactoryModel::default();
        let cells = model.cells;
        for expected in [1, 2, 3, 0] {
            model.rotate_tool();
            assert_eq!(model.tool_direction, expected);
            assert_eq!(model.cells, cells);
        }
    }

    #[test]
    fn wrong_stage_at_dock_is_rejected() {
        let mut model = FactoryModel::default();
        model.edit(20, Some(ModuleKind::Belt), 0).unwrap();
        model.edit(22, Some(ModuleKind::Belt), 0).unwrap();
        model.edit(23, Some(ModuleKind::Belt), 0).unwrap();
        for _ in 0..100 {
            model.step_model();
        }
        assert_eq!(model.delivered, 0);
        assert!(model.rejected > 0);
        assert_conserved(&model);
    }
}
