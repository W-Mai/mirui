use super::*;

#[test]
fn harbor_difficulty_adds_reachable_platforms_and_raised_crossings() {
    let mut most_platforms = 0;
    let mut directions = [false; 2];
    for seed in 0..64 {
        for level in 1..=10 {
            let harbor = Harbor::generate(seed, level);
            assert_eq!(harbor, Harbor::generate(seed, level));
            let platforms = harbor.docks.iter().flatten().count();
            let bridges = harbor.bridges.iter().flatten().count();
            if level <= 2 {
                assert_eq!(platforms, 1);
                assert_eq!(bridges, 1);
            } else {
                assert!(platforms > 1 && platforms <= 1 + (level as usize - 1) / 2);
                assert_eq!(bridges, platforms - 1);
            }
            most_platforms = most_platforms.max(platforms);
            for dock in harbor.docks.into_iter().flatten() {
                assert!(
                    harbor.cores.iter().any(|core| {
                        let cell = harbor.layout.cell(core.point);
                        (0..2).all(|axis| (dock.min[axis]..dock.end[axis]).contains(&cell[axis]))
                            && !harbor.layout.on_bridge(cell)
                    }),
                    "{seed} {level}: empty platform {dock:?}"
                );
            }
            for core in harbor.cores {
                let [x, z] = harbor.layout.cell(core.point);
                assert!(harbor.layout.on_land(x, z), "{seed} {level}: {core:?}");
                assert!(!harbor.layout.on_bridge([x, z]), "{seed} {level}: {core:?}");
                assert_eq!(harbor.height(core), Fixed::ZERO);
            }
            for destination in harbor
                .cores
                .into_iter()
                .chain(harbor.recovery_gates())
                .chain([harbor.exit])
            {
                assert!(
                    harbor
                        .layout
                        .way_to(SPAWN.point, destination.point)
                        .is_some(),
                    "{seed} {level}: {destination:?}"
                );
                assert!(harbor.layout.direct(destination.point, destination.point));
            }
            for bridge in harbor.bridges.into_iter().flatten() {
                directions[bridge.axis] = true;
                let footprint = bridge.footprint();
                let center = bridge.center();
                assert_eq!(
                    harbor.height(center),
                    Fixed::from_int(crate::world::BRIDGE_HEIGHT)
                );
                let mut start = center.point;
                let mut end = start;
                let step = crate::world::CELL_SIZE;
                start[bridge.axis] = Fixed::from_int(bridge.start as i32 * step - 2);
                end[bridge.axis] = Fixed::from_int(bridge.end as i32 * step + 2);
                assert!(
                    harbor.layout.direct(start, end),
                    "{seed} {level}: {bridge:?}"
                );
                for gate in harbor.recovery_gates().chain([SPAWN, harbor.exit]) {
                    let [x, z] = gate.point.map(Fixed::to_int);
                    assert!(
                        x + 7 <= footprint.min[0] as i32 * step
                            || x - 7 >= footprint.end[0] as i32 * step
                            || z + 3 <= footprint.min[1] as i32 * step
                            || z - 3 >= footprint.end[1] as i32 * step,
                        "{seed} {level}: overlapping gate {gate:?}, {bridge:?}"
                    );
                }
            }
        }
    }
    assert_eq!(most_platforms, crate::world::MAX_DOCKS);
    assert_eq!(directions, [true; 2]);
}

#[test]
fn pilot_crosses_the_open_gate_and_changes_harbor_behind_the_cover() {
    let mut game = Game::new(1);
    game.start();
    game.ship.location = Location {
        point: [
            game.harbor.exit.point[0],
            game.harbor.exit.point[1] - Fixed::ONE,
        ],
    };
    game.ship.velocity[1] = Fixed::from_int(2);
    game.elapsed_ms(STEP_MS, Controls::default());
    assert_eq!(game.session, Session::Running);
    assert_eq!(game.ship.location.point[1], Fixed::from_int(12));

    game.result.collected = CORES as u8;
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
    game.level = 3;
    game.restart();
    let gate = game.harbor.cargo_gate.unwrap();
    let [x, z] = gate.point;
    game.cores[0].body = Body {
        location: Location {
            point: [x, z - Fixed::from_int(5)],
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
fn the_exit_stays_locked_until_the_last_core_is_recovered() {
    let mut game = Game::new(51);
    game.start();
    for core in game.cores.iter_mut().take(CORES - 1) {
        core.collected = true;
    }
    game.result.collected = CORES as u8 - 1;
    assert!(!game.ready());
    let mut ship = game.harbor.exit;
    ship.point[1] -= Fixed::ONE;
    game.ship.location = ship;
    game.ship.velocity[1] = Fixed::from_int(2);
    game.elapsed_ms(STEP_MS, Controls::default());
    assert_eq!(game.session, Session::Running);
    assert_eq!(game.ship.location, ship);

    game.cores[CORES - 1].body = Body::at(game.harbor.gate);
    game.elapsed_ms(STEP_MS, Controls::default());
    assert!(game.ready());
    assert!(game.cores.iter().all(|core| core.collected));
    game.ship.velocity[1] = Fixed::from_int(2);
    game.elapsed_ms(STEP_MS, Controls::default());
    assert_eq!(game.session, Session::Departed);
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
fn autoplay_tows_the_last_core_past_a_bridge_corner() {
    let mut game = Game::new(0x4652_554e);
    game.set_mode(Mode::Auto);
    game.start();
    for _ in 0..1_200 {
        game.elapsed_ms(48, Controls::default());
        if game.level == 3 {
            return;
        }
    }
    panic!(
        "stuck in harbor {} after recovering {} cores",
        game.level, game.result.collected
    );
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
fn an_armed_core_can_stun_a_guard_and_lose_stability() {
    let mut game = Game::new(1);
    game.start();
    game.guards[0] = Guard::new(Location::new(23, 20));
    game.cores[0].body = Body {
        location: Location::new(19, 20),
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
