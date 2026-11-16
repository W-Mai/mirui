use super::missions::MISSIONS;
use super::types::{
    Arrival, CELL_COUNT, FactoryCell, FactoryError, FactoryItem, FactoryLayout, FactoryMission,
    FactoryModal, FactoryPage, FactoryStatus, FactoryTelemetry, FactoryTool, GRID_HEIGHT,
    GRID_WIDTH, MAX_ARRIVALS, MAX_TELEMETRY, MAX_UNDO, MISSION_COUNT, MaterialStage, ModuleKind,
    stars_for,
};
use crate::anim::{Easing, Motion, PlayMode, SNAPPY, Spring, Transition, Tween, ease};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;
#[cfg(any(feature = "persistence", test))]
use crate::gallery::play::expeditions::{ExpeditionSaveError, finish_packet, validate_packet};
use crate::types::Fixed;

/// Two ticks a second, or eight at ×4.
const NORMAL_CLOCK: BoundedClock = BoundedClock::new(2, 1_000, 4);
const FAST_CLOCK: BoundedClock = BoundedClock::new(8, 1_000, 8);
/// How long items take to slide one cell: a whole tick while the line runs,
/// a short glide for a single step.
const NORMAL_TICK_MS: u16 = 500;
const FAST_TICK_MS: u16 = 125;
const STEP_GLIDE_MS: u16 = 200;
/// The stars of the result card pop in this far apart.
const POP_STAGGER_MS: u16 = 90;

#[cfg(any(feature = "persistence", test))]
const SAVE_MAGIC: [u8; 4] = *b"MFC1";
#[cfg(any(feature = "persistence", test))]
const SAVE_VERSION: u8 = 1;
#[cfg(any(feature = "persistence", test))]
const SAVE_PAYLOAD: usize = 6 + MISSION_COUNT;
#[cfg(any(feature = "persistence", test))]
pub(crate) const SAVE_LEN: usize = SAVE_PAYLOAD + 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FactorySnapshot {
    cells: FactoryLayout,
}

impl FactorySnapshot {
    const EMPTY: Self = Self {
        cells: [None; CELL_COUNT],
    };
}

/// The stars and the open order, kept between sessions.
#[cfg(any(feature = "persistence", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FactoryProgress {
    mission: u8,
    stars: [u8; MISSION_COUNT],
}

/// The cell one step from `index` in `direction`, if it is on the grid.
pub(super) fn neighbor(index: usize, direction: u8) -> Option<usize> {
    let (dx, dy) = match direction % 4 {
        0 => (1_i32, 0_i32),
        1 => (0, 1),
        2 => (-1, 0),
        _ => (0, -1),
    };
    let x = (index % GRID_WIDTH) as i32 + dx;
    let y = (index / GRID_WIDTH) as i32 + dy;
    if x < 0 || y < 0 || x >= GRID_WIDTH as i32 || y >= GRID_HEIGHT as i32 {
        return None;
    }
    Some(y as usize * GRID_WIDTH + x as usize)
}

/// Whether the module at `index` hands its items to nothing: the grid edge,
/// an empty cell, a wall or a source.
fn output_is_dead(cells: &FactoryLayout, index: usize) -> bool {
    let Some(cell) = cells[index] else {
        return false;
    };
    neighbor(index, cell.direction)
        .and_then(|target| cells[target])
        .is_none_or(|target| matches!(target.kind, ModuleKind::Wall | ModuleKind::Source))
}

fn feeds(cells: &FactoryLayout, from: usize, to: usize) -> bool {
    cells[from]
        .is_some_and(|cell| cell.kind.outputs() && neighbor(from, cell.direction) == Some(to))
}

/// The output of a module placed at `index`: into a neighbouring dock or
/// machine that does not feed it, else onward from the module that feeds it.
fn placement_direction(cells: &FactoryLayout, index: usize) -> Option<u8> {
    let sink = |wanted: fn(ModuleKind) -> bool| {
        (0..4).find(|&direction| {
            neighbor(index, direction).is_some_and(|other| {
                cells[other].is_some_and(|cell| wanted(cell.kind)) && !feeds(cells, other, index)
            })
        })
    };
    sink(|kind| kind == ModuleKind::Dock)
        .or_else(|| sink(|kind| kind.process().is_some()))
        .or_else(|| {
            (0..4).find_map(|direction| {
                let other = neighbor(index, direction)?;
                let cell = cells[other]?;
                feeds(cells, other, index).then_some(cell.direction)
            })
        })
}

fn layout_cost(cells: &FactoryLayout) -> u8 {
    cells.iter().flatten().map(|cell| cell.kind.cost()).sum()
}

/// Motion that has already arrived.
fn settled_glide() -> Tween {
    Tween::new(Fixed::ONE, Fixed::ONE, 0, &ease::LINEAR, PlayMode::Once)
}

fn resting_star() -> Motion {
    Spring::preset(Fixed::ZERO, Fixed::ZERO, SNAPPY).into()
}

#[crate::model(
    change = ChangeSet,
    watch(visual = ChangeSet::VISUAL),
    persistence(key = "module_factory/progress")
)]
pub(crate) struct FactoryModel {
    pub(super) cells: FactoryLayout,
    pub(super) items: [Option<FactoryItem>; CELL_COUNT],
    history: [FactorySnapshot; MAX_UNDO],
    telemetry: [FactoryTelemetry; MAX_TELEMETRY],
    clock: BoundedClock,
    /// How far the items of the last tick have slid toward their new cells.
    glide: Tween,
    /// Items the docks took in on the last tick.
    arrivals: [Option<Arrival>; MAX_ARRIVALS],
    /// The result card's stars, each a spring from 0 to full size.
    pop: [Motion; 3],
    pop_ms: u16,
    popped: u8,
    /// The best stars each order has earned; 0 until it is finished.
    stars: [u8; MISSION_COUNT],
    page: FactoryPage,
    tool: FactoryTool,
    modal: FactoryModal,
    notice: Option<FactoryError>,
    pub(super) status: FactoryStatus,
    mission: u8,
    browse: u8,
    pub(super) selection: u8,
    pub(super) tool_direction: u8,
    pub(super) history_len: u8,
    telemetry_start: u8,
    pub(super) telemetry_len: u8,
    blocked: u8,
    pub(super) tick: u16,
    pub(super) delivered: u16,
    pub(super) rejected: u16,
    pub(super) produced: u16,
    next_item_id: u16,
    pub(super) running: bool,
    fast: bool,
}

impl Default for FactoryModel {
    fn default() -> Self {
        let mut model = Self {
            cells: [None; CELL_COUNT],
            items: [None; CELL_COUNT],
            history: [FactorySnapshot::EMPTY; MAX_UNDO],
            telemetry: [FactoryTelemetry::default(); MAX_TELEMETRY],
            clock: NORMAL_CLOCK,
            glide: settled_glide(),
            arrivals: [None; MAX_ARRIVALS],
            pop: [resting_star(); 3],
            pop_ms: 0,
            popped: 0,
            stars: [0; MISSION_COUNT],
            page: FactoryPage::Line,
            tool: FactoryTool::Select,
            modal: FactoryModal::None,
            notice: None,
            status: FactoryStatus::Ready,
            mission: 0,
            browse: 0,
            selection: 0,
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
            fast: false,
        };
        model.load_mission_state(0, false);
        model
    }
}

#[crate::model]
impl FactoryModel {
    #[observe]
    pub(crate) fn page(&self) -> FactoryPage {
        self.page
    }

    #[observe]
    pub(crate) fn modal(&self) -> FactoryModal {
        self.modal
    }

    #[observe]
    pub(crate) fn tool(&self) -> FactoryTool {
        self.tool
    }

    #[observe]
    pub(crate) fn status(&self) -> FactoryStatus {
        self.status
    }

    /// The refusal of the last tap on the line, until an edit goes through.
    #[observe]
    pub(crate) fn notice(&self) -> Option<FactoryError> {
        self.notice
    }

    #[observe]
    pub(crate) fn mission_index(&self) -> u8 {
        self.mission
    }

    #[observe]
    pub(crate) fn mission(&self) -> FactoryMission {
        MISSIONS[self.mission as usize]
    }

    /// The order shown on the order page.
    #[observe]
    pub(crate) fn browse(&self) -> u8 {
        self.browse
    }

    #[observe]
    pub(crate) fn browsed_mission(&self) -> FactoryMission {
        MISSIONS[self.browse as usize]
    }

    /// One bit per finished order.
    #[observe]
    pub(crate) fn completed(&self) -> u8 {
        (0..MISSION_COUNT)
            .filter(|mission| self.stars[*mission] != 0)
            .fold(0, |mask, mission| mask | 1 << mission)
    }

    /// The best stars of every order.
    #[observe]
    pub(crate) fn stars(&self) -> [u8; MISSION_COUNT] {
        self.stars
    }

    /// The credits the example line spends: three stars at or under it.
    #[observe]
    pub(crate) fn par_cost(&self) -> u8 {
        layout_cost(self.mission().reference)
    }

    /// The order just won has a next one to offer.
    #[observe]
    pub(crate) fn has_next(&self) -> bool {
        self.status == FactoryStatus::Won && usize::from(self.mission) + 1 < MISSION_COUNT
    }

    #[observe]
    pub(crate) fn selected(&self) -> usize {
        self.selection as usize
    }

    #[observe]
    pub(crate) fn selected_cell(&self) -> Option<FactoryCell> {
        self.cell(self.selected())
    }

    #[observe]
    pub(crate) fn tool_direction(&self) -> u8 {
        self.tool_direction
    }

    /// The rotate button turns the highlighted module, or the next
    /// placement when a build tool points at an empty cell.
    #[observe]
    pub(crate) fn can_rotate(&self) -> bool {
        match self.selected_cell() {
            Some(cell) => !cell.kind.is_fixed(),
            None => matches!(self.tool, FactoryTool::Build(_)),
        }
    }

    #[observe]
    pub(crate) fn tick(&self) -> u16 {
        self.tick
    }

    #[observe]
    pub(crate) fn delivered(&self) -> u16 {
        self.delivered
    }

    #[observe]
    pub(crate) fn rejected(&self) -> u16 {
        self.rejected
    }

    #[observe]
    pub(crate) fn produced(&self) -> u16 {
        self.produced
    }

    #[observe]
    pub(crate) fn blocked(&self) -> u8 {
        self.blocked
    }

    #[observe]
    pub(crate) fn running(&self) -> bool {
        self.running
    }

    #[observe]
    pub(crate) fn fast(&self) -> bool {
        self.fast
    }

    #[observe]
    pub(crate) fn history_len(&self) -> u8 {
        self.history_len
    }

    #[observe]
    pub(crate) fn telemetry_len(&self) -> u8 {
        self.telemetry_len
    }

    pub(crate) fn cell(&self, index: usize) -> Option<FactoryCell> {
        if index < CELL_COUNT {
            self.cells[index]
        } else {
            None
        }
    }

    pub(crate) fn item(&self, index: usize) -> Option<FactoryItem> {
        if index < CELL_COUNT {
            self.items[index]
        } else {
            None
        }
    }

    /// The module at `index` pushes its items into nothing.
    pub(crate) fn dead_end(&self, index: usize) -> bool {
        index < CELL_COUNT
            && self.cells[index].is_some_and(|cell| cell.kind.outputs())
            && output_is_dead(&self.cells, index)
    }

    pub(crate) fn telemetry(&self, index: usize) -> Option<FactoryTelemetry> {
        if index >= usize::from(self.telemetry_len) {
            return None;
        }
        let physical = (usize::from(self.telemetry_start) + index) % MAX_TELEMETRY;
        Some(self.telemetry[physical])
    }

    #[observe]
    pub(crate) fn cost(&self) -> u8 {
        layout_cost(&self.cells)
    }

    #[observe]
    pub(crate) fn power(&self) -> u8 {
        self.cells
            .iter()
            .flatten()
            .map(|cell| cell.kind.power())
            .sum()
    }

    #[observe]
    pub(crate) fn wip(&self) -> u8 {
        self.items.iter().filter(|item| item.is_some()).count() as u8
    }

    pub(crate) fn set_page(&mut self, page: FactoryPage) -> ChangeSet {
        if self.page == page || self.modal != FactoryModal::None {
            return ChangeSet::NONE;
        }
        self.page = page;
        if page == FactoryPage::Orders {
            self.browse = self.mission;
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn browse_order(&mut self, mission: u8) -> ChangeSet {
        if usize::from(mission) >= MISSION_COUNT || self.browse == mission {
            return ChangeSet::NONE;
        }
        self.browse = mission;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn open_modal(&mut self, modal: FactoryModal) -> ChangeSet {
        if self.modal == modal {
            return ChangeSet::NONE;
        }
        self.modal = modal;
        self.clock.reset();
        if modal == FactoryModal::Result {
            self.pop = [resting_star(); 3];
            self.pop_ms = 0;
            self.popped = 0;
        }
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

    /// Picking a tool closes the picker and brings the line forward.
    pub(crate) fn set_tool(&mut self, tool: FactoryTool) -> ChangeSet {
        self.tool = tool;
        self.modal = FactoryModal::None;
        self.page = FactoryPage::Line;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn rotate_tool(&mut self) -> ChangeSet {
        self.tool_direction = (self.tool_direction + 1) % 4;
        self.page = FactoryPage::Line;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn toggle_speed(&mut self) -> ChangeSet {
        self.fast = !self.fast;
        self.clock = if self.fast { FAST_CLOCK } else { NORMAL_CLOCK };
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    /// Moves the highlight one cell, staying on the grid.
    pub(crate) fn move_selection(&mut self, direction: u8) -> ChangeSet {
        let Some(next) = neighbor(self.selected(), direction) else {
            return ChangeSet::NONE;
        };
        self.selection = next as u8;
        self.page = FactoryPage::Line;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn select_or_apply(&mut self, index: usize) -> Result<ChangeSet, FactoryError> {
        if index >= CELL_COUNT {
            return Err(FactoryError::InvalidCell);
        }
        let result = match self.tool {
            FactoryTool::Select => Ok(self.select(index)),
            FactoryTool::Build(kind) => self.place(index, kind),
            FactoryTool::Erase => self.edit(index, None, 0),
        };
        match result {
            Err(FactoryError::Duplicate) => Ok(self.select(index)),
            other => other,
        }
    }

    /// `select_or_apply` for a tap on the grid: a refusal moves the
    /// highlight to the cell and stays on the status line.
    pub(crate) fn tap_cell(&mut self, index: usize) -> ChangeSet {
        let result = self.select_or_apply(index);
        if result.is_err() && index < CELL_COUNT {
            self.selection = index as u8;
        }
        match result {
            Ok(change) => {
                self.notice = None;
                change | ChangeSet::VISUAL
            }
            Err(error) => {
                self.notice = Some(error);
                ChangeSet::MODEL | ChangeSet::VISUAL
            }
        }
    }

    /// Places `kind` at `index`. A replacement keeps the old output; on an
    /// empty cell the output points into a neighbouring dock or machine,
    /// else carries on from the module feeding it. A neighbouring module
    /// that pushes into nothing turns to feed the new one.
    pub(crate) fn place(
        &mut self,
        index: usize,
        kind: ModuleKind,
    ) -> Result<ChangeSet, FactoryError> {
        self.check_editable(index, Some(kind))?;
        let mut cells = self.cells;
        for side in 0..4 {
            let Some(other) = neighbor(index, side) else {
                continue;
            };
            let toward = (side + 2) % 4;
            let Some(cell) = cells[other] else {
                continue;
            };
            if !cell.kind.is_fixed()
                && cell.direction % 2 != toward % 2
                && output_is_dead(&cells, other)
            {
                cells[other] = Some(FactoryCell {
                    direction: toward,
                    ..cell
                });
            }
        }
        let direction = match self.cells[index] {
            Some(cell) => cell.direction,
            None => placement_direction(&cells, index).unwrap_or(self.tool_direction),
        };
        cells[index] = Some(FactoryCell { kind, direction });
        self.commit(cells, index)?;
        self.tool_direction = direction;
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn edit(
        &mut self,
        index: usize,
        kind: Option<ModuleKind>,
        direction: u8,
    ) -> Result<ChangeSet, FactoryError> {
        self.check_editable(index, kind)?;
        let mut cells = self.cells;
        cells[index] = kind.map(|kind| FactoryCell {
            kind,
            direction: direction % 4,
        });
        self.commit(cells, index)
    }

    pub(crate) fn rotate_selected(&mut self) -> Result<ChangeSet, FactoryError> {
        let index = self.selected();
        let Some(cell) = self.cells[index] else {
            return Err(FactoryError::NothingSelected);
        };
        if cell.kind.is_fixed() {
            return Err(FactoryError::ImmutableCell);
        }
        let direction = (cell.direction + 1) % 4;
        let change = self.edit(index, Some(cell.kind), direction)?;
        self.tool_direction = direction;
        self.page = FactoryPage::Line;
        Ok(change)
    }

    pub(crate) fn rotate_active(&mut self) -> Result<ChangeSet, FactoryError> {
        match (self.selected_cell(), self.tool) {
            (None, FactoryTool::Build(_)) => Ok(self.rotate_tool()),
            _ => self.rotate_selected(),
        }
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.history_len == 0 {
            return ChangeSet::NONE;
        }
        self.history_len -= 1;
        self.cells = self.history[usize::from(self.history_len)].cells;
        self.page = FactoryPage::Line;
        self.notice = None;
        self.reset_run();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn toggle_run(&mut self) -> Result<ChangeSet, FactoryError> {
        if self.power() > self.mission().power {
            return Err(FactoryError::PowerExceeded);
        }
        if self.finished() {
            self.reset_run();
        }
        self.running = !self.running;
        self.notice = None;
        self.status = if self.running {
            FactoryStatus::Running
        } else {
            FactoryStatus::Paused
        };
        self.clock.reset();
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    /// One tick by hand; a finished run starts over first.
    pub(crate) fn step_once(&mut self) -> Result<ChangeSet, FactoryError> {
        if self.power() > self.mission().power {
            return Err(FactoryError::PowerExceeded);
        }
        if self.finished() {
            self.reset_run();
        }
        self.running = false;
        self.notice = None;
        self.step_model();
        self.start_glide(STEP_GLIDE_MS, &ease::OUT_QUAD);
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    /// The frame clock: slides the items of the last tick, runs the line
    /// and pops the stars of the result card.
    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        let mut changes = ChangeSet::NONE;
        if elapsed_ms > 0 && !self.glide.is_finished() {
            self.glide.tick(elapsed_ms);
            changes = changes | ChangeSet::VISUAL;
        }
        let active = self.running && self.modal == FactoryModal::None;
        let steps = self.clock.steps(elapsed_ms, active);
        if steps > 0 {
            for _ in 0..steps {
                if !self.step_model() {
                    break;
                }
            }
            let tick_ms = if self.fast {
                FAST_TICK_MS
            } else {
                NORMAL_TICK_MS
            };
            self.start_glide(tick_ms, &ease::LINEAR);
            changes = changes | ChangeSet::MODEL | ChangeSet::VISUAL;
        }
        if elapsed_ms > 0 && self.modal == FactoryModal::Result && self.popping() {
            self.pop_ms = self.pop_ms.saturating_add(elapsed_ms);
            let earned = self.stars_earned();
            while self.popped < earned && self.pop_ms >= u16::from(self.popped) * POP_STAGGER_MS {
                Transition::from(SNAPPY)
                    .retarget(&mut self.pop[usize::from(self.popped)], Fixed::ONE);
                self.popped += 1;
            }
            for star in &mut self.pop {
                star.tick(elapsed_ms);
            }
            changes = changes | ChangeSet::VISUAL;
        }
        changes
    }

    /// Loads an order, asking first only when that would throw away edits.
    pub(crate) fn request_mission(&mut self, mission: u8, reference: bool) -> ChangeSet {
        let mission = mission.min((MISSION_COUNT - 1) as u8);
        if self.history_len == 0 {
            self.load_mission_state(mission, reference);
        } else {
            self.modal = FactoryModal::Confirm { mission, reference };
            self.clock.reset();
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn request_browsed(&mut self, reference: bool) -> ChangeSet {
        self.request_mission(self.browse, reference)
    }

    /// After a win, the next order starts at once: nothing is lost.
    pub(crate) fn next_mission(&mut self) -> ChangeSet {
        if !self.has_next() {
            return ChangeSet::NONE;
        }
        self.load_mission_state(self.mission + 1, false);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    /// The way on from the result card: the next order, or the order page
    /// after the last one.
    pub(crate) fn next_or_orders(&mut self) -> ChangeSet {
        if self.has_next() {
            return self.next_mission();
        }
        self.close_modal() | self.set_page(FactoryPage::Orders)
    }

    pub(crate) fn confirm_mission(&mut self) -> ChangeSet {
        let FactoryModal::Confirm { mission, reference } = self.modal else {
            return ChangeSet::NONE;
        };
        self.load_mission_state(mission, reference);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn select_modal_option(&mut self, index: usize) -> ChangeSet {
        match self.modal {
            FactoryModal::Tools => {
                let tool = match index {
                    0 => FactoryTool::Select,
                    1 => FactoryTool::Build(ModuleKind::Belt),
                    2 => FactoryTool::Build(ModuleKind::Furnace),
                    3 => FactoryTool::Build(ModuleKind::Assembler),
                    4 => FactoryTool::Build(ModuleKind::Inspector),
                    5 => FactoryTool::Erase,
                    _ => return ChangeSet::NONE,
                };
                self.set_tool(tool)
            }
            FactoryModal::Confirm { .. } if index == 0 => self.close_modal(),
            FactoryModal::Confirm { .. } if index == 1 => self.confirm_mission(),
            FactoryModal::Help if index == 0 => self.close_modal(),
            FactoryModal::Result if index == 0 => self.next_or_orders(),
            FactoryModal::Result if index == 1 => self.close_modal(),
            _ => ChangeSet::NONE,
        }
    }
}

impl FactoryModel {
    pub(super) fn load_mission_state(&mut self, mission: u8, reference: bool) {
        self.mission = mission.min((MISSION_COUNT - 1) as u8);
        self.browse = self.mission;
        let order = MISSIONS[usize::from(self.mission)];
        self.cells = if reference {
            *order.reference
        } else {
            *order.start
        };
        self.history_len = 0;
        self.selection = order.focus;
        self.page = FactoryPage::Line;
        self.tool = FactoryTool::Select;
        self.tool_direction = 0;
        self.modal = FactoryModal::None;
        self.notice = None;
        self.reset_run();
    }

    /// How far the items of the last tick have slid, from 0 to 1.
    pub(crate) fn glide(&self) -> Fixed {
        self.glide.value()
    }

    /// Item `index` a dock took in on the last tick.
    pub(crate) fn arrival(&self, index: usize) -> Option<Arrival> {
        self.arrivals.get(index).copied().flatten()
    }

    /// The size of result star `index`, from 0 to 1 with a little bounce.
    pub(crate) fn star_scale(&self, index: usize) -> Fixed {
        self.pop.get(index).map_or(Fixed::ZERO, Motion::value)
    }

    /// Stars the finished run earned, 0 before it is won.
    pub(crate) fn stars_earned(&self) -> u8 {
        if self.status == FactoryStatus::Won {
            stars_for(self.cost(), self.par_cost())
        } else {
            0
        }
    }

    fn popping(&self) -> bool {
        self.popped < self.stars_earned() || self.pop.iter().any(|star| !star.is_done())
    }

    fn start_glide(&mut self, duration_ms: u16, curve: &'static Easing) {
        self.glide = Tween::new(Fixed::ZERO, Fixed::ONE, duration_ms, curve, PlayMode::Once);
    }

    fn select(&mut self, index: usize) -> ChangeSet {
        self.selection = index as u8;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn finished(&self) -> bool {
        matches!(self.status, FactoryStatus::Won | FactoryStatus::Timeout)
    }

    fn check_editable(&self, index: usize, kind: Option<ModuleKind>) -> Result<(), FactoryError> {
        if index >= CELL_COUNT {
            return Err(FactoryError::InvalidCell);
        }
        if self.cells[index].is_some_and(|cell| cell.kind.is_fixed()) {
            return Err(FactoryError::ImmutableCell);
        }
        if kind.is_some_and(|kind| !ModuleKind::BUILDABLE.contains(&kind)) {
            return Err(FactoryError::InvalidModule);
        }
        Ok(())
    }

    /// Applies an edited floor plan as one undo step, if it changes anything
    /// and stays within the credits.
    fn commit(&mut self, cells: FactoryLayout, index: usize) -> Result<ChangeSet, FactoryError> {
        if cells == self.cells {
            return Err(FactoryError::Duplicate);
        }
        if layout_cost(&cells) > self.mission().budget {
            return Err(FactoryError::BudgetExceeded);
        }
        self.remember();
        self.cells = cells;
        self.selection = index as u8;
        self.page = FactoryPage::Line;
        self.reset_run();
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
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
        self.arrivals = [None; MAX_ARRIVALS];
        self.glide = settled_glide();
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

    /// One tick of the line. Each item remembers the cell it left, so the
    /// view can slide it; a finished order keeps its stars and opens the
    /// result card.
    pub(super) fn step_model(&mut self) -> bool {
        let order = self.mission();
        if self.power() > order.power || self.finished() {
            return false;
        }
        self.tick = self.tick.saturating_add(1);
        self.blocked = 0;
        for index in 0..CELL_COUNT {
            let (Some(mut item), Some(cell)) = (self.items[index], self.cells[index]) else {
                continue;
            };
            item.age = item.age.saturating_add(1);
            item.from = index as u8;
            if let Some((input, output, ticks)) = cell.kind.process()
                && item.stage == input
                && item.age >= ticks
            {
                item.stage = output;
                item.age = 0;
            }
            self.items[index] = Some(item);
        }

        let current = self.items;
        let mut next = current;
        let mut claimed = [false; CELL_COUNT];
        let mut arrivals = [None; MAX_ARRIVALS];
        let mut arrived = 0;
        for index in 0..CELL_COUNT {
            let (Some(cell), Some(item)) = (self.cells[index], current[index]) else {
                continue;
            };
            if cell
                .kind
                .process()
                .is_some_and(|(input, _, _)| item.stage == input)
            {
                continue;
            }
            let Some(target) = neighbor(index, cell.direction) else {
                self.blocked = self.blocked.saturating_add(1);
                continue;
            };
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
                let accepted = item.stage == order.target;
                if accepted {
                    self.delivered = self.delivered.saturating_add(1);
                } else {
                    self.rejected = self.rejected.saturating_add(1);
                }
                if let Some(slot) = arrivals.get_mut(arrived) {
                    *slot = Some(Arrival {
                        from: index as u8,
                        dock: target as u8,
                        stage: item.stage,
                        accepted,
                    });
                    arrived += 1;
                }
            } else {
                next[target] = Some(FactoryItem {
                    age: 0,
                    from: index as u8,
                    ..item
                });
            }
        }
        self.items = next;
        self.arrivals = arrivals;

        if self.tick % 3 == 1 {
            for index in 0..CELL_COUNT {
                if self.cells[index].is_some_and(|cell| cell.kind == ModuleKind::Source)
                    && self.items[index].is_none()
                {
                    self.items[index] = Some(FactoryItem {
                        id: self.next_item_id,
                        stage: MaterialStage::Ore,
                        age: 0,
                        from: index as u8,
                    });
                    self.next_item_id = self.next_item_id.saturating_add(1);
                    self.produced = self.produced.saturating_add(1);
                }
            }
        }
        self.push_telemetry();
        if self.delivered >= order.goal {
            self.status = FactoryStatus::Won;
            self.running = false;
            let mission = usize::from(self.mission);
            self.stars[mission] = self.stars[mission].max(self.stars_earned());
            self.open_modal(FactoryModal::Result);
        } else if self.tick >= order.limit {
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

    /// Writes the stars and the open order into `output`.
    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn encode_progress(&self, output: &mut [u8]) -> Result<usize, ExpeditionSaveError> {
        if output.len() < SAVE_LEN {
            return Err(ExpeditionSaveError::BufferTooSmall);
        }
        output[..4].copy_from_slice(&SAVE_MAGIC);
        output[4] = SAVE_VERSION;
        output[5] = self.mission;
        output[6..SAVE_PAYLOAD].copy_from_slice(&self.stars);
        finish_packet(output, SAVE_PAYLOAD);
        Ok(SAVE_LEN)
    }

    /// Reads a record written by `encode_progress`, checking every field.
    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn decode_progress(input: &[u8]) -> Result<FactoryProgress, ExpeditionSaveError> {
        validate_packet(input, SAVE_PAYLOAD)?;
        if input[..4] != SAVE_MAGIC {
            return Err(ExpeditionSaveError::InvalidMagic);
        }
        if input[4] != SAVE_VERSION {
            return Err(ExpeditionSaveError::UnsupportedVersion);
        }
        let mission = input[5];
        if usize::from(mission) >= MISSION_COUNT {
            return Err(ExpeditionSaveError::InvalidLevel);
        }
        let mut stars = [0; MISSION_COUNT];
        stars.copy_from_slice(&input[6..SAVE_PAYLOAD]);
        if stars.iter().any(|stars| *stars > 3) {
            return Err(ExpeditionSaveError::InvalidRecord);
        }
        Ok(FactoryProgress { mission, stars })
    }

    /// Takes back the saved stars and opens the saved order on its start.
    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn apply_progress(&mut self, progress: FactoryProgress) {
        self.stars = progress.stars;
        self.load_mission_state(progress.mission, false);
    }
}
