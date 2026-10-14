use super::*;

#[test]
fn repeated_targets_do_not_restart_progress() {
    let config = TweenConfig::new(300, &ease::OUT_CUBIC);
    let mut tween = Tween::preset(Fixed::ZERO, 100.into(), config);
    for elapsed in (30..=300).step_by(30) {
        for _ in 0..4 {
            tween.retarget(100.into(), config);
        }
        tween.tick(30);
        assert_eq!(tween.elapsed_ms, elapsed);
    }
    assert!(tween.is_finished());
    assert_eq!(tween.value(), 100.into());
    tween.retarget(100.into(), config);
    assert!(tween.is_finished());
}

#[test]
fn retarget_and_reconfiguration_start_at_the_displayed_value() {
    let mut config = TweenConfig::new(300, &ease::OUT_CUBIC);
    let mut tween = Tween::preset(Fixed::ZERO, 100.into(), config);
    for target in [(-20).into(), 100.into(), 100.into()] {
        tween.tick(80);
        let before = tween.value();
        config = TweenConfig::new(config.duration_ms + 10, &ease::IN_OUT_CUBIC);
        tween.retarget(target, config);
        assert_eq!(tween.value(), before);
        assert_eq!(tween.elapsed_ms, 0);
        tween.tick(config.duration_ms);
        assert_eq!(tween.value(), target);
        assert!(tween.is_finished());
        // Leave rest before the next reconfiguration.
        tween.retarget(200.into(), config);
    }
    tween.tick(config.duration_ms);
    tween.retarget(200.into(), TweenConfig::new(250, &ease::LINEAR));
    assert!(tween.is_finished());
}

#[test]
fn zero_duration_and_stationary_once_motion_finish_immediately() {
    for mode in [PlayMode::Once, PlayMode::Loop, PlayMode::PingPong] {
        let mut tween = Tween::new(Fixed::ZERO, Fixed::ONE, 0, &ease::LINEAR, mode);
        for dt in [0, 1, u16::MAX] {
            assert!(tween.is_finished());
            assert_eq!(tween.value(), Fixed::ONE);
            tween.tick(dt);
        }
    }
    let mut tween = Tween::ease_to(Fixed::ONE, Fixed::ONE, 300);
    assert!(tween.is_finished());
    tween.retarget(100.into(), TweenConfig::new(0, &ease::OUT_CUBIC));
    assert!(tween.is_finished());
    assert_eq!(tween.value(), 100.into());
}

#[test]
fn periodic_ticks_preserve_all_elapsed_time_and_pingpong_parity() {
    for mode in [PlayMode::Loop, PlayMode::PingPong] {
        for duration in [1, 7, 100, 40_000, u16::MAX] {
            for ticks in [[0, 200], [100, 250], [u16::MAX, u16::MAX]] {
                let mut tween = Tween::new(Fixed::ZERO, Fixed::ONE, duration, &ease::LINEAR, mode);
                for dt in ticks {
                    tween.tick(dt);
                }
                let elapsed = u32::from(ticks[0]) + u32::from(ticks[1]);
                let periods = elapsed / u32::from(duration);
                assert_eq!(u32::from(tween.elapsed_ms), elapsed % u32::from(duration));
                let reverse = mode == PlayMode::PingPong && periods % 2 == 1;
                assert_eq!(tween.from, if reverse { Fixed::ONE } else { Fixed::ZERO });
                assert_eq!(tween.to, if reverse { Fixed::ZERO } else { Fixed::ONE });
                assert!(!tween.is_finished());
            }
        }
    }
}

#[test]
fn once_motion_finishes_at_the_exact_endpoint_without_sampling_past_it() {
    fn quantized(t: Fixed) -> Fixed {
        assert!((Fixed::ZERO..=Fixed::ONE).contains(&t));
        t * Fixed::from_ratio(255, 256)
    }
    static QUANTIZED: Easing = Easing::new(quantized);
    let mut tween = Tween::new(
        (-32).into(),
        101.into(),
        u16::MAX,
        &QUANTIZED,
        PlayMode::Once,
    );
    tween.tick(40_000);
    tween.tick(40_000);
    assert!(tween.is_finished());
    assert_eq!(tween.elapsed_ms, u16::MAX);
    assert_eq!(tween.value(), 101.into());
}

#[test]
fn reconfiguration_and_sampling_do_not_allocate() {
    let config = TweenConfig::new(300, &ease::OUT_CUBIC);
    let mut tween = Tween::preset(Fixed::ZERO, Fixed::ONE, config);
    let allocations = crate::test_support::tracked_allocations(|| {
        for i in 0..100 {
            tween.tick(16);
            tween.retarget(Fixed::from_int(i % 5), config);
            core::hint::black_box(tween.value());
        }
    });
    assert_eq!(allocations, 0);
}
