use super::data::{SHIFT_COUNT, SHIFTS};
use super::model::{PostModel, SAVE_LEN};
use super::motion::lever_angle;
use super::route::PostRoute;
use super::types::{MAX_EVENTS, MAX_PARCELS, PostModal, PostParcel};
use crate::gallery::play::change::ChangeSet;
use crate::types::Fixed;

fn run_steps(model: &mut PostModel, count: usize) {
    for _ in 0..count {
        let _ = model.step_fixed();
    }
}

/// Sets each switch for the parcel that reaches it next, like a player who
/// never slips.
fn steer(model: &mut PostModel) {
    let leader = |model: &PostModel, route: PostRoute| {
        (0..usize::from(model.active_len()))
            .filter_map(|index| model.active(index))
            .filter(|parcel| parcel.route() == route)
            .max_by_key(|parcel| parcel.distance())
    };
    if let Some(parcel) = leader(model, PostRoute::Entry) {
        model.switches[0] = u8::from(parcel.target() != 0);
    }
    if let Some(parcel) = leader(model, PostRoute::ToSecondSwitch) {
        model.switches[1] = u8::from(parcel.target() == 2);
    }
}

/// Plays the current shift to its end, steering every parcel when `steered`.
fn play_out(model: &mut PostModel, steered: bool) {
    model.toggle_running();
    for _ in 0..40_000 {
        if steered {
            steer(model);
        }
        let _ = model.step_fixed();
        if model.finished() {
            return;
        }
    }
    panic!("shift {} never finished", model.shift());
}

fn perfect_score(parcels: usize) -> u16 {
    (0..parcels)
        .map(|index| 100 + 20 * index.min(5) as u16)
        .sum()
}

fn settle(model: &mut PostModel) {
    for _ in 0..60 {
        model.advance_ms(16);
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
fn the_rails_hold_only_what_the_shift_allows_without_a_hidden_queue() {
    for shift in 0..SHIFT_COUNT as u8 {
        let mut model = PostModel::default();
        model.load_shift(shift);
        let allowed = model.in_flight();
        for _ in 0..allowed {
            assert!(model.spawn());
        }
        assert!(!model.spawn());
        assert!(!model.can_send());
        let period = SHIFTS[usize::from(shift)].spawn_period;
        model.spawn_clock_half_ticks = period;
        let cursor = model.cursor();
        let _ = model.step_fixed();
        assert_eq!(model.active_len(), allowed);
        assert_eq!(model.cursor(), cursor);
        assert_eq!(model.spawn_clock_half_ticks, period);
    }
}

#[test]
fn terminal_delivery_is_recorded_exactly_once() {
    let mut model = PostModel::default();
    model.toggle_running();
    // The first parcel rides the inlet and the whole branch to A at one
    // pixel a step; one more step passes its arrival.
    let arrival = (PostRoute::Entry.length() + PostRoute::StationA.length()).to_int();
    run_steps(&mut model, arrival as usize + 2);
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
    model.open_shifts();
    assert!(!model.running());
    assert_eq!(model.modal(), PostModal::Shifts);
    model.close_modal();
    assert!(!model.running());
    assert_eq!(model.modal(), PostModal::None);
}

#[test]
fn each_shift_finishes_with_a_complete_terminal_partition() {
    for shift in 0..SHIFT_COUNT as u8 {
        let mut model = PostModel::default();
        model.load_shift(shift);
        play_out(&mut model, false);
        assert!(model.finished());
        assert_eq!(model.modal(), PostModal::Summary);
        assert_eq!(model.event_len(), model.parcel_count());
        assert_eq!(model.delivered() + model.missed(), model.parcel_count());
        assert_eq!(model.active_len(), 0);
    }
}

#[test]
fn the_day_ramps_up_one_shift_at_a_time() {
    for pair in SHIFTS.windows(2) {
        let (easier, harder) = (&pair[0], &pair[1]);
        assert!(easier.parcels.len() <= harder.parcels.len());
        assert!(easier.spawn_period > harder.spawn_period);
        assert!(easier.in_flight <= harder.in_flight);
    }
    for shift in &SHIFTS {
        assert!(shift.parcels.len() <= MAX_EVENTS);
        assert!(usize::from(shift.in_flight) <= MAX_PARCELS);
        for station in 0..3 {
            let count = shift
                .parcels
                .iter()
                .filter(|target| **target == station)
                .count();
            assert_eq!(
                count * 3,
                shift.parcels.len(),
                "{} station {station}",
                shift.name
            );
        }
    }
}

#[test]
fn a_clean_run_through_the_day_stars_every_shift() {
    let mut model = PostModel::default();
    for (index, shift) in SHIFTS.iter().enumerate() {
        assert_eq!(usize::from(model.shift()), index);
        play_out(&mut model, true);
        assert_eq!(model.missed(), 0, "{}", shift.name);
        let expected = perfect_score(shift.parcels.len());
        assert_eq!(model.score(), expected);
        assert_eq!(model.best()[index], expected);
        assert_eq!(model.starred(), (1 << (index + 1)) - 1);
        assert_eq!(model.has_next(), index + 1 < SHIFT_COUNT);
        model.next_shift();
    }
    assert_eq!(model.modal(), PostModal::Shifts);
    assert_eq!(usize::from(model.shift()), SHIFT_COUNT - 1);
}

#[test]
fn a_wrong_delivery_finishes_the_shift_without_a_star() {
    let mut model = PostModel::default();
    play_out(&mut model, false);
    assert_eq!(model.delivered(), 2);
    assert_eq!(model.missed(), 4);
    assert_eq!(model.starred(), 0);
    assert_eq!(model.best()[0], 220);
    model.restart();
    play_out(&mut model, true);
    assert_eq!(model.starred(), 1);
    assert_eq!(model.best()[0], perfect_score(6));
    model.restart();
    play_out(&mut model, false);
    assert_eq!(model.starred(), 1, "a later slip keeps the star");
    assert_eq!(model.best()[0], perfect_score(6), "and the best score");
}

#[test]
fn changing_shift_keeps_stars_best_scores_and_speed() {
    let mut model = PostModel::default();
    model.cycle_speed();
    play_out(&mut model, true);
    let speed = model.speed_x2();
    model.load_shift(3);
    assert_eq!(model.shift(), 3);
    assert_eq!(model.speed_x2(), speed);
    assert_eq!(model.starred(), 1);
    assert_eq!(model.best()[0], perfect_score(6));
    assert_eq!(model.score(), 0);
    assert!(!model.started());
    assert_eq!(model.load_shift(SHIFT_COUNT as u8), ChangeSet::NONE);
    assert_eq!(model.shift(), 3);
}

#[test]
fn a_switch_lever_swings_over_instead_of_jumping() {
    let mut model = PostModel::default();
    let start = Fixed::from_int(lever_angle(0, 0));
    assert_eq!(model.motion().lever(0), start);
    model.toggle_switch(0);
    assert_eq!(
        model.motion().lever(0),
        start,
        "it leaves from where it was"
    );
    assert!(model.advance_ms(16).contains(ChangeSet::VISUAL));
    let swinging = model.motion().lever(0);
    assert!(swinging > start && swinging < Fixed::ZERO);
    settle(&mut model);
    assert_eq!(model.motion().lever(0), Fixed::from_int(lever_angle(0, 1)));
    assert_eq!(
        model.advance_ms(16),
        ChangeSet::NONE,
        "nothing left to draw"
    );
}

#[test]
fn each_delivery_rings_its_station_and_a_wrong_one_is_marked() {
    let mut model = PostModel::default();
    model.record_delivery(PostParcel::new(0, 0), PostRoute::StationA);
    model.record_delivery(PostParcel::new(1, 0), PostRoute::StationB);
    let correct = |model: &PostModel, station| model.motion().ring(station).map(|ring| ring.1);
    assert_eq!(correct(&model, 0), Some(true));
    assert_eq!(correct(&model, 1), Some(false));
    assert_eq!(correct(&model, 2), None);
    settle(&mut model);
    assert_eq!(model.motion().ring(0), None);
}

#[test]
fn a_clean_shift_pops_its_star_and_only_a_higher_score_is_a_new_best() {
    let mut model = PostModel::default();
    play_out(&mut model, false);
    let _ = model.step_fixed();
    assert!(model.new_best(), "the first finish sets the best, once");
    settle(&mut model);
    assert_eq!(model.motion().star(), Fixed::ZERO, "no star with misses");
    model.restart();
    assert!(!model.new_best());
    play_out(&mut model, true);
    assert!(model.new_best());
    settle(&mut model);
    assert_eq!(model.motion().star(), Fixed::ONE);
    model.restart();
    play_out(&mut model, true);
    assert!(!model.new_best(), "the same score again is not a new best");
}

#[test]
fn enter_and_escape_follow_the_panel_in_front() {
    let mut model = PostModel::default();
    model.confirm();
    assert!(model.running());
    model.back();
    assert_eq!(model.modal(), PostModal::Shifts);
    assert_eq!(model.confirm(), ChangeSet::NONE);
    model.back();
    assert_eq!(model.modal(), PostModal::None);
    model.open_reset();
    model.confirm();
    assert_eq!(model.modal(), PostModal::None);
    assert!(
        !model.started(),
        "a confirmed restart opens the shift fresh"
    );
    play_out(&mut model, true);
    model.confirm();
    assert_eq!(model.shift(), 1, "Enter on the summary moves on");
}

#[test]
fn progress_keeps_stars_best_scores_and_the_shift_in_play() {
    let mut source = PostModel::default();
    play_out(&mut source, true);
    source.load_shift(2);
    source.toggle_running();
    let mut bytes = [0_u8; SAVE_LEN];
    assert_eq!(source.encode_into(&mut bytes), Ok(SAVE_LEN));
    assert_eq!(&bytes[..4], b"PST1");

    let progress = PostModel::decode_progress(&bytes).unwrap();
    let mut restored = PostModel::default();
    restored.cycle_speed();
    restored.restore(progress);
    assert_eq!(restored.shift(), 2);
    assert!(!restored.started(), "the shift opens fresh");
    assert_eq!(restored.starred(), 1);
    assert_eq!(restored.best(), source.best());
    assert_eq!(restored.speed_x2(), 3, "the chosen speed stays");

    let mut corrupt = bytes;
    corrupt[7] ^= 1;
    assert!(PostModel::decode_progress(&corrupt).is_err());
    assert!(PostModel::decode_progress(&bytes[..SAVE_LEN - 1]).is_err());
}

#[test]
fn model_storage_stays_bounded() {
    assert!(core::mem::size_of::<PostModel>() <= 512);
}
