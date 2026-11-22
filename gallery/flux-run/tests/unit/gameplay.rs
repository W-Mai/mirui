use super::*;

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
                "autoplay stalled at {} with {} recovered, ship {:?}, tether {:?}, at {:?}",
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
