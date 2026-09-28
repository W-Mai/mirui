use super::data::{MANIFEST_COUNT, SPAWN_PERIOD_HALF_TICKS};
use super::model::PostModel;
use super::route::PostRoute;
use super::types::{MAX_PARCELS, PostModal, PostParcel};
use crate::types::Fixed;

fn run_steps(model: &mut PostModel, count: usize) {
    for _ in 0..count {
        let _ = model.step_fixed();
    }
}

#[test]
fn initial_start_dispatches_once_and_pause_preserves_position() {
    let mut model = PostModel::default();
    model.toggle_running();
    assert!(model.running());
    assert_eq!(model.cursor(), 1);
    assert_eq!(model.active_len(), 1);
    run_steps(&mut model, 17);
    let before = model.active(0).unwrap().distance();
    model.toggle_running();
    let _ = model.advance_ms(500);
    assert_eq!(model.active(0).unwrap().distance(), before);
}

#[test]
fn switch_choice_locks_when_the_parcel_crosses_the_junction() {
    let mut model = PostModel::default();
    model.toggle_running();
    run_steps(&mut model, 118);
    assert_eq!(model.active(0).unwrap().route(), PostRoute::Entry);
    model.toggle_switch(0);
    run_steps(&mut model, 1);
    assert_eq!(model.active(0).unwrap().route(), PostRoute::ToSecondSwitch);
    model.toggle_switch(0);
    assert_eq!(model.active(0).unwrap().route(), PostRoute::ToSecondSwitch);
    assert_eq!(model.active(0).unwrap().decision_count(), 1);
}

#[test]
fn residual_distance_survives_a_junction_step() {
    let mut model = PostModel::default();
    model.toggle_running();
    model.speed_index = 2;
    run_steps(&mut model, 80);
    let parcel = model.active(0).unwrap();
    assert_eq!(parcel.route(), PostRoute::StationA);
    assert_eq!(parcel.distance(), Fixed::ONE);
}

#[test]
fn capacity_is_bounded_without_a_hidden_queue() {
    let mut model = PostModel::default();
    assert!(model.spawn());
    assert!(model.spawn());
    assert!(model.spawn());
    assert!(!model.spawn());
    model.spawn_clock_half_ticks = SPAWN_PERIOD_HALF_TICKS;
    let cursor = model.cursor();
    let _ = model.step_fixed();
    assert_eq!(model.active_len(), MAX_PARCELS as u8);
    assert_eq!(model.cursor(), cursor);
    assert_eq!(model.spawn_clock_half_ticks, SPAWN_PERIOD_HALF_TICKS);
}

#[test]
fn terminal_delivery_is_recorded_exactly_once() {
    let mut model = PostModel::default();
    model.toggle_running();
    run_steps(&mut model, 406);
    assert_eq!(model.event_len(), 1);
    assert_eq!(model.delivered(), 1);
    let event = model.event(0).unwrap();
    assert_eq!(event.parcel_id, 0);
    assert!(event.correct);
    run_steps(&mut model, 1);
    assert_eq!(model.event_len(), 1);
}

#[test]
fn wrong_station_resets_the_streak_and_scores_nothing() {
    let mut model = PostModel::default();
    model.parcels[0] = PostParcel {
        route: PostRoute::StationB,
        distance: PostRoute::StationB.length() - Fixed::HALF,
        ..PostParcel::new(0, 0)
    };
    model.parcel_len = 1;
    model.running = true;
    model.started = true;
    model.streak = 3;
    let _ = model.step_fixed();
    assert_eq!(model.missed(), 1);
    assert_eq!(model.streak(), 0);
    assert_eq!(model.score(), 0);
}

#[test]
fn correct_streak_bonus_caps_at_one_hundred() {
    let mut model = PostModel::default();
    for id in 0..8 {
        model.record_delivery(PostParcel::new(id, 0), PostRoute::StationA);
    }
    assert_eq!(model.score(), 1_300);
    assert_eq!(model.delivered(), 8);
}

#[test]
fn speed_changes_dispatch_and_motion_rate_without_rerouting() {
    let mut slow = PostModel::default();
    slow.toggle_running();
    slow.speed_index = 0;
    let mut fast = PostModel::default();
    fast.toggle_running();
    fast.speed_index = 2;
    run_steps(&mut slow, 60);
    run_steps(&mut fast, 60);
    assert_eq!(slow.active(0).unwrap().distance(), Fixed::from_int(30));
    assert_eq!(fast.active(0).unwrap().distance(), Fixed::from_int(90));
    assert_eq!(slow.cursor(), 1);
    assert_eq!(fast.cursor(), 1);
}

#[test]
fn opening_and_closing_a_panel_never_resumes_the_shift() {
    let mut model = PostModel::default();
    model.toggle_running();
    model.open_manifests();
    assert!(!model.running());
    assert_eq!(model.modal(), PostModal::Manifests);
    model.close_modal();
    assert!(!model.running());
    assert_eq!(model.modal(), PostModal::None);
}

#[test]
fn each_manifest_finishes_with_a_complete_terminal_partition() {
    for manifest_id in 0..MANIFEST_COUNT as u8 {
        let mut model = PostModel::default();
        model.load_manifest(manifest_id);
        model.toggle_running();
        for _ in 0..20_000 {
            let _ = model.step_fixed();
            if model.finished() {
                break;
            }
        }
        assert!(model.finished());
        assert_eq!(model.event_len(), model.manifest_len());
        assert_eq!(model.delivered() + model.missed(), model.manifest_len());
        assert_eq!(model.active_len(), 0);
    }
}

#[test]
fn model_storage_stays_bounded() {
    assert!(core::mem::size_of::<PostModel>() <= 512);
}
