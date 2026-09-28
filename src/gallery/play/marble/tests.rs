use super::model::*;
use super::physics::STEP;
use super::transport::MAX_SOUND_EVENTS;
use super::types::*;
use crate::app::App;
use crate::gallery::play::change::ChangeSet;
use crate::types::Fixed64;
use alloc::rc::Rc;
use core::cell::Cell;

#[test]
fn registered_model_forwards_actions_observations_and_sound() {
    let mut app = App::headless(480, 320);
    let model = app.add_model(MarbleModel::new());
    let sounds = Rc::new(Cell::new(0));
    let received = sounds.clone();
    app.on_effect(&model, move |_sound: MarbleSound| {
        received.set(received.get() + 1);
    })
    .unwrap();

    let visual = model.visual_revision();
    assert_eq!(model.bpm(), 96);
    assert_eq!(model.ball_count(), 5);
    model.set_bpm(Fixed64::from_int(120));
    assert_eq!(model.bpm(), 120);
    assert!(model.visual_revision() > visual);

    let pitch = model.selected_pitch();
    model.adjust_pitch(1);
    assert_ne!(model.selected_pitch(), pitch);
    assert_eq!(sounds.get(), 1);
    model.cycle_timbre();
    assert_eq!(sounds.get(), 2);
    model.set_page(Page::Edit);
    assert_eq!(model.page(), Page::Edit);
}

#[test]
fn equivalent_bpm_inputs_do_not_publish_visual_changes() {
    let mut app = App::headless(480, 320);
    let model = app.add_model(MarbleModel::new());
    let visual = model.visual_revision();

    model.set_bpm(Fixed64::from_ratio(9_625, 100));
    assert_eq!(model.bpm(), 96);
    assert_eq!(model.visual_revision(), visual);

    model.set_bpm(Fixed64::from_ratio(9_650, 100));
    assert_eq!(model.bpm(), 97);
    assert_eq!(model.visual_revision(), visual + 1);

    model.set_bpm(Fixed64::from_ratio(9_725, 100));
    assert_eq!(model.bpm(), 97);
    assert_eq!(model.visual_revision(), visual + 1);
}

#[test]
fn equivalent_gravity_and_bounce_inputs_do_not_publish_visual_changes() {
    let mut app = App::headless(480, 320);
    let model = app.add_model(MarbleModel::new());
    let visual = model.visual_revision();

    let gravity = model.gravity();
    model.set_gravity(gravity + Fixed64::from_ratio(1, 100));
    assert_eq!(model.gravity(), gravity);
    assert_eq!(model.visual_revision(), visual);

    let bounce = model.selected_bounce();
    model.set_bounce(bounce + Fixed64::from_ratio(1, 100));
    assert_eq!(model.selected_bounce(), bounce);
    assert_eq!(model.visual_revision(), visual);

    model.set_gravity(gravity + Fixed64::from_ratio(5, 100));
    model.set_bounce(bounce + Fixed64::from_ratio(5, 100));
    assert_eq!(model.visual_revision(), visual + 2);
}

#[test]
fn default_scene_respects_fixed_capacities() {
    let model = MarbleModel::new();
    assert_eq!(model.ball_count(), 5);
    assert_eq!(model.pad_count(), 5);
    assert_eq!(model.page, Page::Play);
    assert!(core::mem::size_of::<MarbleModel>() <= 5 * 1024);
}

#[test]
fn squared_collision_rejection_matches_sqrt_at_boundaries() {
    let unit = Fixed64::from_ratio(1, 65_536);
    for threshold in [
        Fixed64::from_ratio(63, 10),
        Fixed64::from_int(8),
        Fixed64::from_int(20),
        Fixed64::from_int(27),
    ] {
        let values = [
            Fixed64::ZERO,
            unit,
            Fixed64::ONE,
            threshold - unit,
            threshold,
            threshold + unit,
            threshold + Fixed64::ONE,
            Fixed64::from_int(100),
        ];
        for x in values {
            for y in values {
                for sx in [-Fixed64::ONE, Fixed64::ONE] {
                    for sy in [-Fixed64::ONE, Fixed64::ONE] {
                        let delta = Vec2::fixed(x * sx, y * sy);
                        let original = delta.length();
                        assert_eq!(
                            delta.length_below(threshold),
                            (original < threshold).then_some(original),
                            "delta={delta:?}, threshold={threshold:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn ball_and_feedback_storage_stay_bounded() {
    let mut model = MarbleModel::new();
    for _ in 0..20 {
        let _ = model.drop_ball();
    }
    for _ in 0..100 {
        model.pulse(0, false);
    }
    assert_eq!(model.ball_count(), MAX_BALLS);
    assert_eq!(model.rings.iter().flatten().count(), MAX_RINGS);
    assert_eq!(model.particles.iter().flatten().count(), MAX_PARTICLES);
}

#[test]
fn pausing_and_editing_freeze_physics() {
    let mut model = MarbleModel::new();
    model.paused = true;
    let paused = model.balls[0].unwrap().pos;
    for _ in 0..60 {
        let _ = model.advance_ms(16);
    }
    assert_eq!(model.balls[0].unwrap().pos, paused);
    model.paused = false;
    let _ = model.set_page(Page::Edit);
    let time = model.sim_time;
    let _ = model.advance_ms(60);
    assert_eq!(model.sim_time, time);
}

#[test]
fn catch_up_never_exceeds_eight_steps() {
    let mut model = MarbleModel::new();
    let _ = model.advance_ms(u16::MAX);
    assert!(model.sim_time <= STEP * 8);
}

#[test]
fn drag_cancel_restores_pad_and_commit_keeps_it() {
    let mut model = MarbleModel::new();
    let _ = model.set_page(Page::Edit);
    let original = model.selected_pad().pos;
    let _ = model.begin_board_drag(original);
    let _ = model.move_board_drag(Vec2::fixed(original.x + Fixed64::from_int(40), original.y));
    let _ = model.end_board_drag(true);
    assert_eq!(model.selected_pad().pos, original);

    let _ = model.begin_board_drag(original);
    let _ = model.move_board_drag(Vec2::fixed(original.x + Fixed64::from_int(20), original.y));
    let _ = model.end_board_drag(false);
    assert_eq!(
        model.selected_pad().pos.x,
        original.x + Fixed64::from_int(20)
    );
}

#[test]
fn deleting_and_adding_preserves_unique_ids() {
    let mut model = MarbleModel::new();
    for _ in 0..4 {
        let _ = model.remove_selected();
    }
    assert_eq!(model.pad_count(), 1);
    let _ = model.remove_selected();
    assert_eq!(model.pad_count(), 1);
    let _ = model.add_pad(Vec2::new(143, 108));
    let mut ids = [0_u32; MAX_PADS];
    let mut len = 0;
    for pad in model.pads.iter().flatten() {
        assert!(!ids[..len].contains(&pad.id));
        ids[len] = pad.id;
        len += 1;
    }
}

#[test]
fn long_run_stays_inside_world_bounds() {
    let mut model = MarbleModel::new();
    for _ in 0..7_200 {
        let _ = model.advance_ms(8);
    }
    for ball in model.balls.iter().flatten() {
        assert!((Fixed64::from_int(19)..=Fixed64::from_int(461)).contains(&ball.pos.x));
        assert!((Fixed64::from_int(59)..=Fixed64::from_int(244)).contains(&ball.pos.y));
        assert!(ball.trail_len <= TRAIL_LEN);
    }
}

#[test]
fn property_controls_enforce_documented_ranges() {
    let mut model = MarbleModel::new();
    let _ = model.set_gravity(Fixed64::from_int(9));
    let _ = model.set_radius(Fixed64::from_int(99));
    let _ = model.set_bounce(Fixed64::ZERO);
    assert_eq!(model.gravity, Fixed64::from_ratio(16, 10));
    assert_eq!(model.selected_pad().radius, Fixed64::from_int(23));
    assert_eq!(model.selected_pad().bounce, Fixed64::from_ratio(7, 10));
}

#[test]
fn disabling_feedback_releases_all_transient_slots() {
    let mut model = MarbleModel::new();
    model.pulse(0, true);
    assert!(model.rings.iter().any(Option::is_some));
    let _ = model.toggle_feedback();
    assert!(model.rings.iter().all(Option::is_none));
    assert!(model.particles.iter().all(Option::is_none));
    model.pulse(0, true);
    assert!(model.rings.iter().all(Option::is_none));
}

#[test]
fn change_sets_distinguish_layout_and_visual_updates() {
    let mut model = MarbleModel::new();
    let page = model.set_page(Page::Settings);
    assert!(page.contains(ChangeSet::LAYOUT));
    let tick = model.advance_ms(16);
    assert!(!tick.contains(ChangeSet::LAYOUT));
}

#[test]
fn sound_events_use_bounded_storage_and_drain_atomically() {
    let mut model = MarbleModel::new();
    for _ in 0..(MAX_SOUND_EVENTS + 5) {
        model.emit_pad_sound(0, 180, true);
    }
    assert_eq!(
        model.take_sounds().into_iter().flatten().count(),
        MAX_SOUND_EVENTS
    );
    assert!(model.take_sounds().into_iter().all(|sound| sound.is_none()));
}

#[test]
fn pad_collision_cooldown_suppresses_dense_polyphony() {
    let mut model = MarbleModel::new();
    let _ = model.take_sounds();
    model.pulse(0, true);
    model.pulse(0, true);
    assert_eq!(model.take_sounds().into_iter().flatten().count(), 1);
    model.sim_time += Fixed64::from_ratio(11, 100);
    model.pulse(0, true);
    assert_eq!(model.take_sounds().into_iter().flatten().count(), 1);
}

#[test]
fn recorded_pad_events_loop_only_after_recording_finishes() {
    let mut model = MarbleModel::new();
    let _ = model.take_sounds();
    let _ = model.toggle_recording();
    model.transport_steps = Fixed64::from_int(i64::from(model.record_start_step + 1));
    model.emit_pad_sound(2, 180, true);
    let recorded = model.recorded[0].unwrap();
    let _ = model.take_sounds();
    assert!(model.recording);
    assert!(!model.looping);

    model.pads[2].as_mut().unwrap().pitch = 60;
    model.pads[2].as_mut().unwrap().timbre = PadTimbre::Drum;

    let _ = model.toggle_recording();
    assert!(!model.recording);
    assert!(model.looping);
    model.transport_steps = Fixed64::from_int(i64::from(model.loop_start_step + 1));
    model.last_audio_step = model.loop_start_step;
    model.advance_audio();
    assert_eq!(
        model.take_sounds()[0],
        Some(MarbleSound::Pad {
            slot: 2,
            pitch: recorded.pitch,
            timbre: recorded.timbre,
            gain: (u16::from(recorded.gain) * 174 / 255) as u8,
            delay_ms: 0,
        })
    );
}

#[test]
fn empty_recording_never_enters_loop_playback() {
    let mut model = MarbleModel::new();
    let _ = model.toggle_recording();
    let _ = model.toggle_recording();
    assert!(!model.recording);
    assert!(!model.looping);
}

#[test]
fn pad_pitch_and_timbre_controls_emit_current_sound() {
    let mut model = MarbleModel::new();
    let old_pitch = model.selected_pad().pitch;
    let old_timbre = model.selected_pad().timbre;
    let _ = model.take_sounds();
    let _ = model.adjust_pitch(1);
    assert!(model.selected_pad().pitch > old_pitch);
    assert!(matches!(
        model.take_sounds()[0],
        Some(MarbleSound::Pad { pitch, .. }) if pitch == model.selected_pad().pitch
    ));
    let _ = model.cycle_timbre();
    assert_ne!(model.selected_pad().timbre, old_timbre);
    assert!(matches!(
        model.take_sounds()[0],
        Some(MarbleSound::Pad { timbre, .. }) if timbre == model.selected_pad().timbre
    ));
}

#[test]
fn bpm_change_preserves_transport_phase() {
    let mut model = MarbleModel::new();
    let _ = model.advance_ms(625);
    let before = model.transport_steps;
    let step = model.current_audio_step();
    let _ = model.set_bpm(Fixed64::from_int(120));
    assert_eq!(model.transport_steps, before);
    assert_eq!(model.current_audio_step(), step);
    let _ = model.advance_ms(125);
    assert_eq!(model.current_audio_step(), step + 1);
}

#[test]
fn collision_audio_is_scheduled_on_the_even_step_grid() {
    let mut model = MarbleModel::new();
    model.transport_steps = Fixed64::from_ratio(1, 2);
    let _ = model.take_sounds();
    model.emit_pad_sound(0, 180, false);
    assert!(matches!(
        model.take_sounds()[0],
        Some(MarbleSound::Pad { delay_ms, .. }) if delay_ms > 200 && delay_ms < 250
    ));
}
