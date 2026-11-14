use super::missions::{MISSION_COUNT, MISSIONS};
use super::model::OrbitModel;
use super::physics::{MU, impulse};
use super::types::{
    MAX_EVENTS, MAX_PREVIEW, MAX_TELEMETRY, MAX_TRAIL, OrbitBody, OrbitError, OrbitEventKind,
    OrbitModal, OrbitPage, OrbitStatus,
};
use crate::types::Fixed64;

/// One part of a documented maneuver.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// Set the dial and burn once the clock reaches `at` tenths of a second.
    Burn { at: u16, dv: u8, angle: i16 },
    /// Before anything else, plan a node `delay` seconds ahead.
    Node { delay: u8, dv: u8, angle: i16 },
}

use Step::{Burn, Node};

/// The maneuver each mission's hint describes.
const REFERENCE: [&[Step]; MISSION_COUNT] = [
    &[Burn {
        at: 0,
        dv: 38,
        angle: 0,
    }],
    &[Burn {
        at: 0,
        dv: 52,
        angle: 0,
    }],
    &[Burn {
        at: 0,
        dv: 38,
        angle: 180,
    }],
    &[
        Burn {
            at: 0,
            dv: 38,
            angle: 0,
        },
        Burn {
            at: 100,
            dv: 32,
            angle: 180,
        },
    ],
    &[
        Burn {
            at: 0,
            dv: 38,
            angle: 0,
        },
        Burn {
            at: 100,
            dv: 34,
            angle: 0,
        },
    ],
    &[
        Burn {
            at: 0,
            dv: 52,
            angle: 0,
        },
        Burn {
            at: 115,
            dv: 80,
            angle: 180,
        },
    ],
    &[
        Node {
            delay: 10,
            dv: 34,
            angle: 0,
        },
        Burn {
            at: 0,
            dv: 38,
            angle: 0,
        },
    ],
    &[
        Node {
            delay: 10,
            dv: 34,
            angle: 0,
        },
        Node {
            delay: 26,
            dv: 66,
            angle: 180,
        },
        Burn {
            at: 0,
            dv: 38,
            angle: 0,
        },
    ],
];

/// Flies `steps` on `mission` for up to a minute, sampling every goal as
/// soon as it is in reach. Rejected commands are left to show in the result.
fn fly(mission: u8, steps: &[Step]) -> OrbitModel {
    let mut model = OrbitModel::default();
    model.load_mission(mission);
    let mut next = 0;
    for _ in 0..3_600 {
        while let Some(step) = steps.get(next) {
            let due = match *step {
                Burn { at, .. } => (model.time() * 10).to_int() >= i64::from(at),
                Node { .. } => true,
            };
            if !due {
                break;
            }
            match *step {
                Burn { dv, angle, .. } => {
                    model.dv_tenths = dv;
                    model.angle_degrees = angle;
                    let _ = model.burn();
                }
                Node { delay, dv, angle } => {
                    model.dv_tenths = dv;
                    model.angle_degrees = angle;
                    model.delay_seconds = delay;
                    let _ = model.schedule();
                }
            }
            next += 1;
        }
        if model.eligible() {
            model.scan().unwrap();
        }
        if !model.step() {
            break;
        }
    }
    model
}

fn decimal(tenths: u8) -> String {
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[test]
fn documented_maneuvers_complete_all_missions() {
    for mission in 0..MISSION_COUNT as u8 {
        let model = fly(mission, REFERENCE[usize::from(mission)]);
        assert_eq!(model.status(), OrbitStatus::Won, "mission {mission}");
        assert_eq!(model.completed(), model.mission().goal_count());
        assert_eq!(model.queue_len(), 0);
        assert_eq!(model.modal(), OrbitModal::Result);
        assert_ne!(model.cleared_mask() & (1 << mission), 0);
        for index in 0..model.event_len() {
            assert!(
                !matches!(
                    model.event(index).unwrap().kind,
                    OrbitEventKind::NodeSkipped(_)
                ),
                "mission {mission} skipped a node"
            );
        }
    }
}

#[test]
fn hints_name_every_value_of_the_reference() {
    for (mission, steps) in REFERENCE.iter().enumerate() {
        let hint = MISSIONS[mission].hint;
        for step in *steps {
            let (dv, angle) = match *step {
                Burn { dv, angle, .. } => (dv, angle),
                Node { delay, dv, angle } => {
                    assert!(hint.contains(&format!("{delay} s")), "{hint}");
                    (dv, angle)
                }
            };
            assert!(hint.contains(&format!("Δv {}", decimal(dv))), "{hint}");
            assert!(hint.contains(&format!("{angle}°")) || angle == 0, "{hint}");
        }
    }
}

#[test]
fn idle_flights_complete_nothing() {
    for mission in 0..MISSION_COUNT as u8 {
        let model = fly(mission, &[]);
        assert_eq!(model.completed(), 0, "mission {mission}");
        assert_ne!(model.status(), OrbitStatus::Won);
    }
}

#[test]
fn each_mission_needs_more_than_the_maneuver_before_it() {
    for mission in 1..MISSION_COUNT as u8 {
        let model = fly(mission, REFERENCE[usize::from(mission) - 1]);
        assert_ne!(model.status(), OrbitStatus::Won, "mission {mission}");
    }
}

#[test]
fn lost_contact_rejects_commands_but_planned_nodes_still_fire() {
    let mut model = OrbitModel::default();
    model.load_mission(6);
    assert_eq!(model.mission().contact_seconds, 4);
    model.delay_seconds = 10;
    model.dv_tenths = 34;
    model.schedule().unwrap();
    model.dv_tenths = 38;
    model.burn().unwrap();
    while model.time() < Fixed64::from_int(4) {
        model.step();
    }
    assert_eq!(model.burn(), Err(OrbitError::NoContact));
    assert_eq!(model.schedule(), Err(OrbitError::NoContact));
    assert_eq!(model.cancel_at(0), Err(OrbitError::NoContact));
    while model.time() < Fixed64::from_int(11) {
        model.step();
    }
    assert_eq!(model.queue_len(), 0);
    assert!(model.fuel() < Fixed64::from_int(2));

    let manual = fly(
        6,
        &[
            Burn {
                at: 0,
                dv: 38,
                angle: 0,
            },
            Burn {
                at: 100,
                dv: 34,
                angle: 0,
            },
        ],
    );
    assert_ne!(manual.status(), OrbitStatus::Won);
}

#[test]
fn arriving_at_a_goal_pauses_the_flight() {
    let mut model = OrbitModel::default();
    model.burn().unwrap();
    model.toggle_running();
    for _ in 0..1_000 {
        model.advance_ms(16);
        if model.status() != OrbitStatus::Running {
            break;
        }
    }
    assert_eq!(model.status(), OrbitStatus::Paused);
    assert!(model.eligible());
    model.scan().unwrap();
    assert_eq!(model.status(), OrbitStatus::Won);
    assert_eq!(model.modal(), OrbitModal::Result);
}

#[test]
fn winning_marks_the_mission_and_offers_the_next() {
    let mut model = fly(0, REFERENCE[0]);
    assert!(model.has_next());
    assert!(model.can_restart());
    model.next_mission();
    assert_eq!(model.mission_index(), 1);
    assert_eq!(model.modal(), OrbitModal::Brief);
    assert_eq!(model.status(), OrbitStatus::Ready);
    assert_eq!(model.cleared_mask(), 1);

    let last = fly(7, REFERENCE[7]);
    assert!(!last.has_next());
    let mut last = last;
    assert_eq!(
        last.next_mission(),
        crate::gallery::play::change::ChangeSet::NONE
    );
    last.restart();
    assert_eq!(last.mission_index(), 7);
    assert_eq!(last.status(), OrbitStatus::Ready);
    assert_eq!(last.cleared_mask(), 1 << 7);
}

#[test]
fn forecast_follows_the_plan() {
    let mut model = OrbitModel::default();
    model.load_mission(4);
    let unplanned = model.coast().to_vec();
    model.delay_seconds = 10;
    model.dv_tenths = 34;
    model.schedule().unwrap();
    model.dv_tenths = 38;
    model.burn().unwrap();
    assert_ne!(model.coast(), unplanned.as_slice());
    let mark = model
        .node_mark(0)
        .expect("the node fires inside the forecast");
    let ahead = model.coast()[149];
    assert!(model.aim().len() <= MAX_PREVIEW);
    assert!(model.coast().len() <= MAX_PREVIEW);
    while model.time() < Fixed64::from_int(15) {
        model.step();
    }
    let flown = super::types::TrackPoint::from_point(model.body().position);
    let error = (i32::from(flown.x) - i32::from(ahead.x)).abs()
        + (i32::from(flown.y) - i32::from(ahead.y)).abs();
    assert!(error < 4 * 4, "forecast drifted {error} quarter units");
    assert!(mark.x < 0, "the node fires on the far side");
}

#[test]
fn dial_changes_do_not_move_the_craft() {
    let mut model = OrbitModel::default();
    let body = model.body();
    let fuel = model.fuel();
    let time = model.time();
    model.adjust_dv(20);
    model.flip_angle();
    model.adjust_delay(5);
    assert_eq!(model.body(), body);
    assert_eq!(model.fuel(), fuel);
    assert_eq!(model.time(), time);
    assert!(!model.aim().is_empty());
}

#[test]
fn angles_wrap_and_flip() {
    let mut model = OrbitModel::default();
    model.flip_angle();
    assert_eq!(model.angle_degrees(), 180);
    model.adjust_angle(15);
    assert_eq!(model.angle_degrees(), -165);
    model.adjust_angle(-15);
    model.adjust_angle(-15);
    assert_eq!(model.angle_degrees(), 165);
    model.flip_angle();
    assert_eq!(model.angle_degrees(), -15);
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
    assert_eq!(fuel_failure.burn_block(), Some(OrbitError::Fuel));
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
    assert_eq!(model.schedule_block(), Some(OrbitError::QueueFull));
    let middle = model.queue(1).unwrap().id;
    model.cancel_raw(middle).unwrap();
    assert_eq!(model.queue_len(), 2);
    assert_eq!(model.page(), OrbitPage::Map);
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
fn terminal_boundaries_stop_simulation_and_report() {
    let mut crash = OrbitModel::default();
    crash.body.position.x = Fixed64::from_int(25);
    crash.status = OrbitStatus::Running;
    crash.step();
    assert_eq!(crash.status(), OrbitStatus::Crashed);
    assert_eq!(crash.modal(), OrbitModal::Result);
    assert_eq!(crash.burn(), Err(OrbitError::Terminal));
    assert!(crash.coast().is_empty());

    let mut escape = OrbitModel::default();
    escape.body.position.x = Fixed64::from_int(261);
    escape.body.velocity.x = Fixed64::from_int(10);
    escape.status = OrbitStatus::Running;
    escape.step();
    assert_eq!(escape.status(), OrbitStatus::Escaped);
    escape.restart();
    assert_eq!(escape.status(), OrbitStatus::Ready);
    assert_eq!(escape.modal(), OrbitModal::None);
}

#[test]
fn histories_and_model_memory_are_bounded() {
    let mut model = OrbitModel::default();
    for _ in 0..8_000 {
        model.step();
    }
    for _ in 0..40 {
        model.push_event(OrbitEventKind::Burn(10));
    }
    assert_eq!(model.trail_len(), MAX_TRAIL);
    assert_eq!(model.telemetry_len(), MAX_TELEMETRY);
    assert_eq!(model.event_len(), MAX_EVENTS);
    assert!(model.log().iter().all(Option::is_some));
    assert!(core::mem::size_of::<OrbitModel>() <= 5 * 1024);
}

#[test]
fn modal_time_does_not_accumulate_debt() {
    let mut model = OrbitModel::default();
    model.toggle_running();
    model.advance_ms(16);
    let time = model.time();
    model.set_modal(OrbitModal::Brief);
    model.advance_ms(1_000);
    assert_eq!(model.time(), time);
    model.set_modal(OrbitModal::None);
    model.advance_ms(1);
    assert_eq!(model.time(), time);
}

#[test]
fn restarting_a_flight_in_progress_asks_first() {
    let mut model = OrbitModel::default();
    assert!(!model.can_restart());
    model.restart();
    assert_eq!(model.modal(), OrbitModal::None);

    model.load_mission(2);
    model.burn().unwrap();
    model.restart();
    assert_eq!(model.modal(), OrbitModal::Confirm(2));
    model.confirm();
    assert_eq!(model.mission_index(), 2);
    assert_eq!(model.modal(), OrbitModal::None);
    assert_eq!(model.fuel(), Fixed64::from_int(48));
}

#[test]
fn choosing_a_mission_asks_only_when_a_flight_would_be_lost() {
    let mut model = OrbitModel::default();
    model.set_modal(OrbitModal::Missions);
    model.choose_mission(MISSION_COUNT as u8);
    assert_eq!(model.modal(), OrbitModal::Missions);
    model.choose_mission(3);
    assert_eq!(model.mission_index(), 3);
    assert_eq!(model.modal(), OrbitModal::Brief);

    model.toggle_hint();
    model.set_modal(OrbitModal::None);
    model.burn().unwrap();
    model.set_modal(OrbitModal::Missions);
    model.choose_mission(5);
    assert_eq!(model.modal(), OrbitModal::Confirm(5));
    assert_eq!(model.mission_index(), 3);
    model.set_modal(OrbitModal::None);
    assert_eq!(model.mission_index(), 3);

    model.set_modal(OrbitModal::Missions);
    model.choose_mission(5);
    model.confirm();
    assert_eq!(model.mission_index(), 5);
    assert_eq!(model.modal(), OrbitModal::Brief);
    assert!(!model.hint_shown);
    assert_eq!(model.fuel(), Fixed64::from_int(28));
}

#[cfg(feature = "persistence")]
#[test]
fn won_missions_are_saved_and_nothing_else_is() {
    use crate::core::persistence::{FieldCodec, ModelCodec, PersistencePlugin};
    use crate::core::storage::{MemoryStorage, Storage};
    use crate::prelude::App;

    let mut won = fly(2, REFERENCE[2]);
    won.cleared |= 1;
    let mut bytes = alloc::vec::Vec::new();
    FieldCodec::default().encode(&won, &mut bytes).unwrap();
    let mut storage = MemoryStorage::new();
    storage.write("orbital_mission/progress", &bytes).unwrap();
    let mut app = App::headless(16, 16);
    app.add_plugin(PersistencePlugin::new(storage));
    let model = app.add_model(OrbitModel::default()).finish();
    assert_eq!(model.cleared(), 0b101);
    assert_eq!(model.mission(), 0);
    assert_eq!(model.status(), OrbitStatus::Ready);
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
