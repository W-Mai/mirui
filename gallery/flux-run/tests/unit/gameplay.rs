use super::*;

#[test]
fn generated_harbors_keep_a_connected_lower_bypass() {
    let mut different = 0;
    for seed in 0..64 {
        let first = Harbor::generate(seed, 1);
        for level in 1..9 {
            let harbor = Harbor::generate(seed, level);
            assert_eq!(harbor, Harbor::generate(seed, level));
            let extent = [72, 84, 96, 108][(level as usize - 1).min(3)];
            assert_eq!(harbor.extent(), extent);
            assert_eq!(harbor.exit.point[0], Fixed::from_int(extent - 14));
            assert_eq!(harbor.gate.point[1], Fixed::from_int(extent - 10));
            let boundary = harbor.advance(harbor.exit, [Fixed::MAX, Fixed::from_int(13)], 0);
            assert_eq!(boundary.point[0], Fixed::from_int(extent - 5));
            different += usize::from(harbor.layout != first.layout);
            let launch = Location::new(harbor.bridge, 8, false);
            assert_eq!(harbor.cargo_gate.is_some(), level > 1);
            if let (Some(other), Some(start)) = (harbor.cargo_gate, harbor.layout.freight_row()) {
                let cargo = harbor.cores[2];
                assert!(harbor.layout.cell(cargo.point)[1] >= start);
                assert!((0..harbor.layout.side()).any(|x| harbor.layout.blocked(x, start)));
                for gate in [harbor.gate, other] {
                    assert!(harbor.layout.way_to(cargo.point, gate.point).is_some());
                }
                assert_eq!(harbor.nearest_gate(other), other);
            }
            for destination in harbor
                .cores
                .iter()
                .copied()
                .filter(|core| !core.upper)
                .chain([harbor.gate, harbor.exit, launch])
                .chain(harbor.cargo_gate)
            {
                assert!(
                    harbor
                        .layout
                        .way_to(SPAWN.point, destination.point)
                        .is_some()
                );
                assert!(harbor.layout.direct(destination.point, destination.point));
            }
        }
    }
    assert!(different > 350);
}

#[test]
fn pilot_crosses_the_open_gate_and_changes_harbor_behind_the_cover() {
    let mut game = Game::new(1);
    game.start();
    game.ship.location = Location::new(58, 12, false);
    game.ship.velocity[1] = Fixed::from_int(2);
    game.elapsed_ms(STEP_MS, Controls::default());
    assert_eq!(game.session, Session::Running);
    assert_eq!(game.ship.location.point[1], Fixed::from_int(12));

    game.result.collected = game.harbor.quota;
    game.result.score = 500;
    game.ship.velocity[1] = Fixed::from_int(2);
    game.elapsed_ms(STEP_MS, Controls::default());
    assert_eq!(game.session, Session::Departed);
    assert_eq!(game.level, 1);
    assert_eq!(game.passage_frame().unwrap().cover, 0);
    let score = game.total;

    for _ in 0..40 {
        game.elapsed_ms(STEP_MS, Controls::default());
        if game.level == 2 {
            break;
        }
    }
    assert_eq!(game.level, 2);
    assert_eq!(game.passage_frame().unwrap().cover, 255);
    assert!(game.ship.location.point[1] < SPAWN.point[1]);
    for _ in 0..40 {
        game.elapsed_ms(STEP_MS, Controls::default());
    }
    assert_eq!(game.session, Session::Running);
    assert!(game.passage_frame().is_none());
    assert_eq!(game.ship.location, SPAWN);
    assert_eq!(game.total, score);
}

#[test]
fn freight_gate_recovers_a_core_and_counts_it_once() {
    let mut game = Game::new(7);
    game.start();
    game.level = 2;
    game.restart();
    let gate = game.harbor.cargo_gate.unwrap();
    let [x, z] = gate.point;
    game.cores[0].body = Body {
        location: Location {
            point: [x, z - Fixed::from_int(5)],
            upper: false,
        },
        velocity: [Fixed::ZERO, Fixed::from_int(2)],
    };
    game.elapsed_ms(STEP_MS, Controls::default());
    assert!(game.cores[0].collected);
    assert_eq!(game.result.collected, 1);
    game.elapsed_ms(STEP_MS, Controls::default());
    assert_eq!(game.result.collected, 1);
}

#[test]
fn autoplay_advances_and_is_deterministic() {
    for seed in [7, 51, 942, 0x4652_554e, 22, 24, 30, 2_899] {
        let play = || {
            let mut game = Game::new(seed);
            game.set_mode(Mode::Auto);
            game.start();
            for step in 0..36_000 {
                game.elapsed_ms(STEP_MS, Controls::default());
                if game.level == 9 {
                    assert_eq!(game.best_manual, 0);
                    assert!(game.best_assisted > 0);
                    return (step, game.total, game.best_assisted);
                }
            }
            panic!(
                "seed {seed}: autoplay stalled at {} with {} recovered, ship {:?}, tether {:?}, at {:?}",
                game.level, game.result.collected, game.ship.location, game.tether, game.cores
            );
        };
        assert_eq!(play(), play());
    }
}

#[test]
fn revival_preserves_progress_and_resupplies_lost_core() {
    let mut game = Game::new(7);
    game.start();
    game.result.collected = 1;
    game.result.score = 240;
    game.cores[0].collected = true;
    game.cores[1].stability = 0;
    game.cores[1].replenish = 2;
    game.life = 0;
    game.respawn = 2;
    for _ in 0..5 {
        game.elapsed_ms(STEP_MS, Controls::default());
    }
    assert_eq!(game.result.collected, 1);
    assert_eq!(game.result.score, 240);
    assert_eq!(game.ship.location, SPAWN);
    assert!(game.cores[0].collected);
    assert!(game.cores[1].active());
    assert_eq!(game.life, 3);
}

#[test]
fn different_decks_cannot_collide_or_collect() {
    let lower = Location::new(36, 54, false);
    let upper = Location::new(36, 54, true);
    assert!(!swept(lower, lower, upper, IMPACT_RADIUS));
    assert!(swept(upper, upper, upper, IMPACT_RADIUS));
}

#[test]
fn an_armed_core_can_stun_a_guard_and_lose_stability() {
    let mut game = Game::new(1);
    game.start();
    game.guards[0] = Guard::new(Location::new(23, 20, false));
    game.cores[0].body = Body {
        location: Location::new(19, 20, false),
        velocity: [Fixed::from_int(2), Fixed::ZERO],
    };
    game.tether = Some(0);
    game.magnet = true;
    game.elapsed_ms(STEP_MS, Controls::default());
    assert!(game.tether.is_none());
    assert!(matches!(game.guards[0].phase, GuardPhase::Stunned(_)));
    assert_eq!(game.cores[0].stability, 2);
    assert_eq!(game.result.collected, 0);
}

#[test]
fn ending_a_run_commits_partial_score_once_in_its_own_category() {
    let mut game = Game::new(1);
    game.start();
    game.result.score = 90;
    game.set_mode(Mode::Auto);
    game.set_mode(Mode::Manual);
    game.finish();
    game.finish();
    assert_eq!(game.total, 90);
    assert_eq!(game.best_assisted, 90);
    assert_eq!(game.best_manual, 0);
}
