use super::missions::MISSION_COUNT;
use super::model::OrbitModel;
use super::physics::{MU, impulse};
use super::types::{
    MAX_EVENTS, MAX_PREVIEW, MAX_TELEMETRY, MAX_TRAIL, OrbitBody, OrbitError, OrbitEventKind,
    OrbitModal, OrbitPoint, OrbitStatus,
};
use crate::types::Fixed64;

fn complete(mission: u8) -> OrbitModel {
    let mut model = OrbitModel::default();
    model.load_mission(mission);
    model.burn().unwrap();
    for _ in 0..3_600 {
        model.step();
        if model.eligible() {
            model.scan().unwrap();
        }
        if model.status() == OrbitStatus::Won {
            break;
        }
    }
    model
}

#[test]
fn documented_maneuvers_complete_all_missions() {
    for mission in 0..MISSION_COUNT as u8 {
        let model = complete(mission);
        assert_eq!(model.status(), OrbitStatus::Won, "mission {mission}");
        assert_eq!(
            model.completed as usize,
            model.mission().goals.iter().flatten().count()
        );
        assert_eq!(model.queue_len(), 0);
    }
}

#[test]
fn prediction_is_bounded_and_does_not_mutate_model() {
    let model = OrbitModel::default();
    let body = model.body();
    let fuel = model.fuel();
    let time = model.time();
    let mut points = [OrbitPoint::default(); MAX_PREVIEW];
    assert!(model.predict(&mut points) <= MAX_PREVIEW);
    assert_eq!(model.body(), body);
    assert_eq!(model.fuel(), fuel);
    assert_eq!(model.time(), time);
}

#[test]
fn burn_costs_once_and_failures_are_atomic() {
    let mut model = OrbitModel::default();
    model.dv_tenths = 30;
    model.burn().unwrap();
    assert_eq!(model.fuel(), Fixed64::from_int(42));
    assert_eq!(model.heat(), Fixed64::from_int(21));

    let mut fuel_failure = OrbitModel::default();
    fuel_failure.fuel = Fixed64::ONE;
    let body = fuel_failure.body();
    assert_eq!(fuel_failure.burn(), Err(OrbitError::Fuel));
    assert_eq!(fuel_failure.body(), body);

    let mut heat_failure = OrbitModel::default();
    heat_failure.heat = Fixed64::from_int(99);
    heat_failure.dv_tenths = 10;
    let body = heat_failure.body();
    assert_eq!(heat_failure.burn(), Err(OrbitError::Heat));
    assert_eq!(heat_failure.body(), body);
}

#[test]
fn scheduled_nodes_snapshot_values_execute_once_and_cancel_by_id() {
    let mut model = OrbitModel::default();
    model.dv_tenths = 20;
    model.angle_degrees = 15;
    model.delay_seconds = 1;
    model.schedule().unwrap();
    model.dv_tenths = 70;
    model.angle_degrees = -90;
    assert_eq!(model.queue(0).unwrap().dv_tenths, 20);
    assert_eq!(model.queue(0).unwrap().angle_degrees, 15);
    model.schedule().unwrap();
    model.schedule().unwrap();
    assert_eq!(model.schedule(), Err(OrbitError::QueueFull));
    let middle = model.queue(1).unwrap().id;
    model.cancel_raw(middle).unwrap();
    assert_eq!(model.queue_len(), 2);
    for _ in 0..180 {
        model.step();
    }
    assert_eq!(model.queue_len(), 0);
    assert!(model.fuel() < Fixed64::from_int(48));
}

#[test]
fn nominal_orbit_and_energy_remain_bounded() {
    let mut circular = OrbitModel::default();
    for _ in 0..7_200 {
        circular.step();
    }
    let drift = (circular.body().radius() - Fixed64::from_int(78)).abs();
    assert!(drift < Fixed64::from_ratio(3, 10));

    let mut raised = OrbitModel::default();
    raised.burn().unwrap();
    let initial = energy(raised.body());
    for _ in 0..3_600 {
        raised.step();
    }
    let drift = (energy(raised.body()) - initial).abs() / initial.abs();
    assert!(drift < Fixed64::from_ratio(2, 1_000));
}

#[test]
fn terminal_boundaries_stop_simulation() {
    let mut crash = OrbitModel::default();
    crash.body.position.x = Fixed64::from_int(25);
    crash.status = OrbitStatus::Running;
    crash.step();
    assert_eq!(crash.status(), OrbitStatus::Crashed);
    assert_eq!(crash.burn(), Err(OrbitError::Terminal));

    let mut escape = OrbitModel::default();
    escape.body.position.x = Fixed64::from_int(261);
    escape.body.velocity.x = Fixed64::from_int(10);
    escape.status = OrbitStatus::Running;
    escape.step();
    assert_eq!(escape.status(), OrbitStatus::Escaped);
}

#[test]
fn histories_and_model_memory_are_bounded() {
    let mut model = OrbitModel::default();
    for _ in 0..4_000 {
        model.step();
    }
    for _ in 0..40 {
        model.push_event(OrbitEventKind::Burn);
    }
    assert_eq!(model.trail_len(), MAX_TRAIL);
    assert_eq!(model.telemetry_len(), MAX_TELEMETRY);
    assert_eq!(model.event_len(), MAX_EVENTS);
    assert!(core::mem::size_of::<OrbitModel>() <= 12 * 1024);
}

#[test]
fn modal_time_does_not_accumulate_debt() {
    let mut model = OrbitModel::default();
    model.toggle_running();
    model.advance_ms(16);
    let time = model.time();
    model.set_modal(OrbitModal::Help);
    model.advance_ms(1_000);
    assert_eq!(model.time(), time);
    model.set_modal(OrbitModal::None);
    model.advance_ms(1);
    assert_eq!(model.time(), time);
}

#[test]
fn reload_request_confirms_the_current_mission() {
    let mut model = OrbitModel::default();
    model.load_mission(2);

    model.request_mission_reload();

    assert_eq!(model.mission_index(), 2);
    assert_eq!(model.modal(), OrbitModal::Confirm(2));
}

#[test]
fn modal_options_follow_the_active_modal_state() {
    let mut model = OrbitModel::default();

    model.set_modal(OrbitModal::Missions);
    model.select_modal_option(1);
    assert_eq!(model.modal(), OrbitModal::Confirm(1));
    assert_eq!(model.mission_index(), 0);

    model.select_modal_option(0);
    assert_eq!(model.modal(), OrbitModal::None);
    assert_eq!(model.mission_index(), 0);

    model.set_modal(OrbitModal::Missions);
    model.select_modal_option(MISSION_COUNT + 1);
    assert_eq!(model.modal(), OrbitModal::Missions);
    model.select_modal_option(MISSION_COUNT);
    assert_eq!(model.modal(), OrbitModal::None);

    model.set_modal(OrbitModal::Missions);
    model.select_modal_option(2);
    model.select_modal_option(1);
    assert_eq!(model.modal(), OrbitModal::None);
    assert_eq!(model.mission_index(), 2);

    model.set_modal(OrbitModal::Help);
    model.select_modal_option(0);
    assert_eq!(model.modal(), OrbitModal::None);
}

#[test]
fn prograde_and_retrograde_impulses_diverge() {
    let mut prograde = OrbitModel::default().body();
    let mut retrograde = prograde;
    impulse(&mut prograde, Fixed64::from_int(3), 0);
    impulse(&mut retrograde, Fixed64::from_int(3), 180);
    assert!(prograde.speed() > retrograde.speed());
}

fn energy(body: OrbitBody) -> Fixed64 {
    body.speed() * body.speed() / 2 - MU / body.radius()
}
