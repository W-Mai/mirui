use super::model::FactoryModel;
use super::types::{
    CELL_COUNT, FactoryError, FactoryModal, FactoryStatus, MAX_TELEMETRY, MAX_UNDO, MISSION_COUNT,
    ModuleKind,
};
use crate::gallery::play::change::ChangeSet;

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
fn modal_options_apply_only_valid_transitions() {
    let mut model = FactoryModel::default();
    assert_eq!(model.select_modal_option(0), ChangeSet::NONE);
    assert_eq!(model.modal(), FactoryModal::None);

    let tools = [
        super::types::FactoryTool::Select,
        super::types::FactoryTool::Build(ModuleKind::Belt),
        super::types::FactoryTool::Build(ModuleKind::Furnace),
        super::types::FactoryTool::Build(ModuleKind::Assembler),
        super::types::FactoryTool::Build(ModuleKind::Inspector),
        super::types::FactoryTool::Erase,
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

    model.request_mission(1, false);
    let confirm = model.modal();
    assert_eq!(model.select_modal_option(2), ChangeSet::NONE);
    assert_eq!(model.modal(), confirm);
    assert!(model.select_modal_option(0).contains(ChangeSet::MODEL));
    assert_eq!(model.modal(), FactoryModal::None);

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
