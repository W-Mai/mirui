use super::missions::MISSIONS;
use super::model::{FactoryModel, neighbor};
use super::types::{
    CELL_COUNT, FactoryError, FactoryLayout, FactoryModal, FactoryPage, FactoryStatus, FactoryTool,
    GRID_WIDTH, MAX_TELEMETRY, MAX_UNDO, MISSION_COUNT, ModuleKind,
};
use crate::gallery::play::change::ChangeSet;

fn assert_conserved(model: &FactoryModel) {
    assert_eq!(
        model.produced,
        model.delivered + model.rejected + u16::from(model.wip())
    );
    let mut ids = [0_u16; CELL_COUNT];
    for (len, item) in model.items.iter().flatten().enumerate() {
        assert!(!ids[..len].contains(&item.id));
        ids[len] = item.id;
    }
}

/// Runs the loaded line until it wins or runs out of time.
fn run_to_end(model: &mut FactoryModel) {
    while model.step_model() {
        assert_conserved(model);
    }
}

fn at(x: usize, y: usize) -> usize {
    y * GRID_WIDTH + x
}

fn layout_cost(cells: &FactoryLayout) -> u8 {
    cells.iter().flatten().map(|cell| cell.kind.cost()).sum()
}

fn layout_power(cells: &FactoryLayout) -> u8 {
    cells.iter().flatten().map(|cell| cell.kind.power()).sum()
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
        run_to_end(&mut model);
        let order = model.mission();
        assert_eq!(model.status, FactoryStatus::Won, "{}", order.name);
        assert_eq!(model.delivered, order.goal, "{}", order.name);
        assert!(model.tick <= order.limit, "{}", order.name);
        assert!(!model.running);
    }
}

#[test]
fn starter_lines_never_false_positive() {
    for mission in 0..MISSION_COUNT as u8 {
        let mut model = FactoryModel::default();
        model.load_mission_state(mission, false);
        run_to_end(&mut model);
        assert_eq!(
            model.status,
            FactoryStatus::Timeout,
            "{}",
            model.mission().name
        );
        assert_eq!(model.tick, model.mission().limit);
    }
}

#[test]
fn every_order_runs_within_its_budgets_and_keeps_its_floor_plan() {
    for order in &MISSIONS {
        for cells in [order.start, order.reference] {
            assert!(layout_cost(cells) <= order.budget, "{}", order.name);
            assert!(layout_power(cells) <= order.power, "{}", order.name);
        }
        let focus = usize::from(order.focus);
        assert_ne!(order.start[focus], order.reference[focus], "{}", order.name);
        for index in 0..CELL_COUNT {
            let fixed = |cells: &FactoryLayout| cells[index].filter(|cell| cell.kind.is_fixed());
            assert_eq!(fixed(order.start), fixed(order.reference), "{}", order.name);
        }
    }
}

/// The taps a player makes on each starter line, in flow order, with the
/// tool each needs. None of them needs a rotation.
const TAPS: [&[(usize, usize, ModuleKind)]; MISSION_COUNT] = [
    &[(5, 2, ModuleKind::Belt)],
    &[(4, 2, ModuleKind::Belt), (6, 4, ModuleKind::Belt)],
    &[(2, 1, ModuleKind::Furnace), (6, 3, ModuleKind::Assembler)],
    &[(6, 3, ModuleKind::Inspector)],
    &[
        (3, 1, ModuleKind::Belt),
        (3, 0, ModuleKind::Belt),
        (4, 0, ModuleKind::Belt),
        (5, 0, ModuleKind::Belt),
        (5, 1, ModuleKind::Belt),
    ],
    &[
        (1, 1, ModuleKind::Belt),
        (1, 0, ModuleKind::Belt),
        (2, 0, ModuleKind::Belt),
        (3, 0, ModuleKind::Belt),
        (3, 1, ModuleKind::Belt),
        (3, 2, ModuleKind::Belt),
        (5, 2, ModuleKind::Belt),
        (5, 1, ModuleKind::Belt),
        (5, 0, ModuleKind::Belt),
        (6, 0, ModuleKind::Belt),
        (7, 0, ModuleKind::Belt),
        (7, 1, ModuleKind::Belt),
        (7, 2, ModuleKind::Belt),
    ],
    &[
        (1, 4, ModuleKind::Belt),
        (2, 4, ModuleKind::Furnace),
        (3, 4, ModuleKind::Belt),
        (4, 4, ModuleKind::Assembler),
        (5, 4, ModuleKind::Belt),
        (6, 4, ModuleKind::Belt),
        (7, 4, ModuleKind::Belt),
    ],
    &[
        (5, 0, ModuleKind::Belt),
        (5, 1, ModuleKind::Belt),
        (5, 2, ModuleKind::Belt),
        (6, 2, ModuleKind::Inspector),
        (7, 2, ModuleKind::Belt),
        (5, 5, ModuleKind::Belt),
        (5, 4, ModuleKind::Belt),
        (5, 3, ModuleKind::Belt),
    ],
];

#[test]
fn tapping_along_each_line_builds_the_reference() {
    for (mission, taps) in TAPS.iter().enumerate() {
        let mut model = FactoryModel::default();
        model.load_mission_state(mission as u8, false);
        for &(x, y, kind) in *taps {
            model.set_tool(FactoryTool::Build(kind));
            model.tap_cell(at(x, y));
            assert_eq!(model.notice(), None, "{} at {x},{y}", model.mission().name);
        }
        assert_eq!(
            &model.cells,
            MISSIONS[mission].reference,
            "{}",
            model.mission().name
        );
    }
}

/// Fewest cells a line can use from the cell the source feeds to a cell
/// next to the dock, around the fixed cells of `cells`.
fn shortest_route(cells: &FactoryLayout) -> usize {
    let source = (0..CELL_COUNT)
        .find(|&index| cells[index].is_some_and(|cell| cell.kind == ModuleKind::Source))
        .unwrap();
    let start = neighbor(source, cells[source].unwrap().direction).unwrap();
    let mut distance = [usize::MAX; CELL_COUNT];
    let mut queue = [0; CELL_COUNT];
    let (mut head, mut tail) = (0, 1);
    distance[start] = 1;
    queue[0] = start;
    while head < tail {
        let index = queue[head];
        head += 1;
        for direction in 0..4 {
            let Some(next) = neighbor(index, direction) else {
                continue;
            };
            if cells[next].is_some_and(|cell| cell.kind == ModuleKind::Dock) {
                return distance[index];
            }
            if distance[next] == usize::MAX && !cells[next].is_some_and(|cell| cell.kind.is_fixed())
            {
                distance[next] = distance[index] + 1;
                queue[tail] = next;
                tail += 1;
            }
        }
    }
    panic!("the dock cannot be reached");
}

#[test]
fn shortest_route_order_has_no_spare_credit() {
    let order = MISSIONS
        .iter()
        .find(|order| order.name == "最短路线")
        .unwrap();
    let machines = ModuleKind::Furnace.cost() + ModuleKind::Assembler.cost();
    let cheapest = shortest_route(order.start) as u8 - 2 + machines;
    assert_eq!(order.budget, cheapest);
    assert_eq!(layout_cost(order.reference), cheapest);
}

#[test]
fn parallel_orders_need_both_lines() {
    let twin = MISSION_COUNT as u8 - 2;
    let mut model = FactoryModel::default();
    model.load_mission_state(twin, false);
    run_to_end(&mut model);
    assert_eq!(model.status, FactoryStatus::Timeout);
    assert!(model.delivered > model.mission().goal / 2);

    let merge = MISSION_COUNT as u8 - 1;
    let order = MISSIONS[usize::from(merge)];
    let line = ModuleKind::Furnace.power() + ModuleKind::Assembler.power();
    let inspector = ModuleKind::Inspector.power();
    assert!(2 * (line + inspector) > order.power);
    assert!(2 * line + inspector <= order.power);
    model.load_mission_state(merge, true);
    for (x, y) in [(5, 3), (5, 4), (5, 5)] {
        model.edit(at(x, y), None, 0).unwrap();
    }
    run_to_end(&mut model);
    assert_eq!(model.status, FactoryStatus::Timeout);
}

#[test]
fn winning_marks_the_order_and_offers_the_next() {
    let mut model = FactoryModel::default();
    model.load_mission_state(0, true);
    assert!(!model.has_next());
    run_to_end(&mut model);
    assert_eq!(model.completed(), 1);
    assert!(model.has_next());
    assert!(model.next_mission().contains(ChangeSet::MODEL));
    assert_eq!(model.mission_index(), 1);
    assert_eq!(model.cells, *MISSIONS[1].start);
    assert_eq!(model.completed(), 1);

    model.load_mission_state(MISSION_COUNT as u8 - 1, true);
    run_to_end(&mut model);
    assert!(!model.has_next());
    assert_eq!(model.next_mission(), ChangeSet::NONE);
    assert_eq!(model.completed(), 1 | 1 << (MISSION_COUNT - 1));
}

#[test]
fn loading_asks_only_when_edits_would_be_lost() {
    let mut model = FactoryModel::default();
    model.set_page(FactoryPage::Orders);
    model.browse_order(4);
    model.request_browsed(false);
    assert_eq!(model.modal(), FactoryModal::None);
    assert_eq!(model.mission_index(), 4);
    assert_eq!(model.page(), FactoryPage::Line);

    model.edit(0, Some(ModuleKind::Belt), 0).unwrap();
    model.request_mission(2, true);
    assert_eq!(
        model.modal(),
        FactoryModal::Confirm {
            mission: 2,
            reference: true
        }
    );
    assert_eq!(model.mission_index(), 4);
    assert!(model.select_modal_option(1).contains(ChangeSet::MODEL));
    assert_eq!(model.mission_index(), 2);
    assert_eq!(model.cells, *MISSIONS[2].reference);
}

#[test]
fn order_page_browses_from_the_current_order() {
    let mut model = FactoryModel::default();
    model.load_mission_state(3, false);
    model.set_page(FactoryPage::Orders);
    assert_eq!(model.browse(), 3);
    assert_eq!(model.browse_order(6), ChangeSet::MODEL | ChangeSet::VISUAL);
    assert_eq!(model.browse_order(6), ChangeSet::NONE);
    assert_eq!(model.browse_order(MISSION_COUNT as u8), ChangeSet::NONE);
    assert_eq!(model.browsed_mission(), MISSIONS[6]);
    assert_eq!(model.mission_index(), 3);
    model.set_page(FactoryPage::Line);
    model.set_page(FactoryPage::Orders);
    assert_eq!(model.browse(), 3);
}

#[test]
fn immutable_cells_and_budget_failures_are_atomic() {
    let mut model = FactoryModel::default();
    let before = model.cells;
    assert_eq!(
        model.edit(at(0, 2), None, 0),
        Err(FactoryError::ImmutableCell)
    );
    assert_eq!(model.cells, before);
    model.edit(0, Some(ModuleKind::Assembler), 0).unwrap();
    let cells = model.cells;
    let history = model.history_len;
    assert_eq!(
        model.edit(1, Some(ModuleKind::Furnace), 0),
        Err(FactoryError::BudgetExceeded)
    );
    assert_eq!(model.cells, cells);
    assert_eq!(model.history_len, history);

    model.load_mission_state(4, false);
    let wall = at(4, 1);
    model.set_tool(FactoryTool::Build(ModuleKind::Belt));
    model.tap_cell(wall);
    assert_eq!(model.notice(), Some(FactoryError::ImmutableCell));
    assert_eq!(model.selected(), wall);
    assert_eq!(model.cell(wall).unwrap().kind, ModuleKind::Wall);
    assert_eq!(model.rotate_active(), Err(FactoryError::ImmutableCell));
    model.tap_cell(at(3, 1));
    assert_eq!(model.notice(), None);
}

#[test]
fn spending_past_the_credits_is_refused_on_the_status_line() {
    let mut model = FactoryModel::default();
    model.load_mission_state(5, false);
    model.set_tool(FactoryTool::Build(ModuleKind::Belt));
    for &(x, y, _) in TAPS[5].iter().take(12) {
        model.tap_cell(at(x, y));
    }
    assert_eq!(model.cost(), model.mission().budget - 1);
    model.tap_cell(at(1, 3));
    assert_eq!(model.notice(), None);
    model.tap_cell(at(7, 2));
    assert_eq!(model.notice(), Some(FactoryError::BudgetExceeded));
    assert_eq!(model.cell(at(7, 2)), None);
}

#[test]
fn rotate_undo_and_edit_reset_the_run() {
    let mut model = FactoryModel::default();
    let before = model.cells;
    model.selection = at(1, 2) as u8;
    model.rotate_selected().unwrap();
    assert_eq!(model.cells[at(1, 2)].unwrap().direction, 1);
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
fn footer_edits_bring_the_line_forward() {
    let mut model = FactoryModel::default();
    model.load_mission_state(1, false);
    model.set_page(FactoryPage::Telemetry);
    model.rotate_active().unwrap();
    assert_eq!(model.page(), FactoryPage::Line);
    assert_eq!(model.cells[at(4, 1)].unwrap().direction, 1);
    model.set_page(FactoryPage::Orders);
    model.undo();
    assert_eq!(model.page(), FactoryPage::Line);
    model.set_page(FactoryPage::Telemetry);
    model.open_modal(FactoryModal::Tools);
    model.select_modal_option(1);
    assert_eq!(model.page(), FactoryPage::Line);
    assert_eq!(model.tool(), FactoryTool::Build(ModuleKind::Belt));
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
    run_to_end(&mut model);
    let limit = model.mission().limit;
    assert_eq!(usize::from(model.telemetry_len), MAX_TELEMETRY);
    assert_eq!(
        model.telemetry(0).unwrap().tick,
        limit + 1 - MAX_TELEMETRY as u16
    );
    assert_eq!(model.telemetry(MAX_TELEMETRY - 1).unwrap().tick, limit);
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
fn a_finished_run_restarts_from_a_single_step() {
    let mut model = FactoryModel::default();
    run_to_end(&mut model);
    assert_eq!(model.status, FactoryStatus::Timeout);
    model.step_once().unwrap();
    assert_eq!(model.tick, 1);
    assert_eq!(model.status, FactoryStatus::Paused);
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
fn fast_speed_runs_four_ticks_in_the_time_of_one() {
    let mut model = FactoryModel::default();
    model.toggle_run().unwrap();
    model.advance_ms(500);
    assert_eq!(model.tick, 1);
    model.toggle_speed();
    assert!(model.fast());
    model.advance_ms(500);
    assert_eq!(model.tick, 5);
}

#[test]
fn modal_options_apply_only_valid_transitions() {
    let mut model = FactoryModel::default();
    assert_eq!(model.select_modal_option(0), ChangeSet::NONE);
    assert_eq!(model.modal(), FactoryModal::None);

    let tools = [
        FactoryTool::Select,
        FactoryTool::Build(ModuleKind::Belt),
        FactoryTool::Build(ModuleKind::Furnace),
        FactoryTool::Build(ModuleKind::Assembler),
        FactoryTool::Build(ModuleKind::Inspector),
        FactoryTool::Erase,
    ];
    for (index, expected) in tools.into_iter().enumerate() {
        model.open_modal(FactoryModal::Tools);
        assert!(model.select_modal_option(index).contains(ChangeSet::MODEL));
        assert_eq!(model.modal(), FactoryModal::None);
        assert_eq!(model.tool(), expected);
    }

    model.open_modal(FactoryModal::Tools);
    let tool = model.tool();
    assert_eq!(model.select_modal_option(6), ChangeSet::NONE);
    assert_eq!(model.modal(), FactoryModal::Tools);
    assert_eq!(model.tool(), tool);

    model.open_modal(FactoryModal::Help);
    assert_eq!(model.select_modal_option(1), ChangeSet::NONE);
    assert_eq!(model.modal(), FactoryModal::Help);
    assert!(model.select_modal_option(0).contains(ChangeSet::MODEL));
    assert_eq!(model.modal(), FactoryModal::None);

    model.edit(0, Some(ModuleKind::Belt), 0).unwrap();
    model.request_mission(1, false);
    let confirm = model.modal();
    assert_eq!(model.select_modal_option(2), ChangeSet::NONE);
    assert_eq!(model.modal(), confirm);
    assert!(model.select_modal_option(0).contains(ChangeSet::MODEL));
    assert_eq!(model.modal(), FactoryModal::None);
    assert_eq!(model.mission_index(), 0);

    model.request_mission(1, false);
    assert!(model.select_modal_option(1).contains(ChangeSet::MODEL));
    assert_eq!(model.modal(), FactoryModal::None);
    assert_eq!(model.mission_index(), 1);
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
fn a_dead_end_turns_to_meet_a_new_belt_beside_it() {
    let mut model = FactoryModel::default();
    model.load_mission_state(1, false);
    assert!(model.dead_end(at(4, 1)));
    model.set_tool(FactoryTool::Build(ModuleKind::Belt));
    model.tap_cell(at(4, 2));
    assert_eq!(model.cells[at(4, 1)].unwrap().direction, 1);
    assert_eq!(model.cells[at(4, 2)].unwrap().direction, 1);
    assert!(!model.dead_end(at(4, 1)));
    assert_eq!(model.history_len, 1);

    model.tap_cell(at(4, 2));
    assert_eq!(model.history_len, 1);
    assert_eq!(model.selected(), at(4, 2));
}

#[test]
fn wrong_stage_at_dock_is_rejected() {
    let mut model = FactoryModel::default();
    model.edit(at(2, 2), Some(ModuleKind::Belt), 0).unwrap();
    model.edit(at(4, 2), Some(ModuleKind::Belt), 0).unwrap();
    model.edit(at(5, 2), Some(ModuleKind::Belt), 0).unwrap();
    for _ in 0..100 {
        model.step_model();
    }
    assert_eq!(model.delivered, 0);
    assert!(model.rejected > 0);
    assert_conserved(&model);
}
