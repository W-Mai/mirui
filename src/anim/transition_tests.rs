use super::*;

const TWEEN: TweenConfig = TweenConfig::new(300, &ease::OUT_CUBIC);

#[test]
fn transition_dispatches_to_the_existing_tween() {
    let config: Transition = TWEEN.into();
    let mut motion = config.motion(Fixed::ZERO, 100.into());
    let mut expected = Tween::preset(Fixed::ZERO, 100.into(), TWEEN);
    let allocations = crate::test_support::tracked_allocations(|| {
        for i in 0..50 {
            let target = if i < 10 { 100.into() } else { (-20).into() };
            config.retarget(&mut motion, target);
            expected.retarget(target, TWEEN);
            motion.tick(16);
            expected.tick(16);
            assert_eq!(motion.value(), expected.value());
            assert_eq!(motion.is_done(), expected.is_finished());
        }
    });
    assert_eq!(allocations, 0);
    assert!(motion.is_done());
}

#[test]
fn kind_switches_start_at_the_displayed_value_and_finish() {
    for configs in [
        [Transition::from(TWEEN), BOUNCY.into()],
        [BOUNCY.into(), TWEEN.into()],
    ] {
        let mut motion = configs[0].motion(Fixed::ZERO, 100.into());
        motion.tick(80);
        let value = motion.value();
        configs[1].retarget(&mut motion, (-20).into());
        assert_eq!(motion.value(), value);
        assert!(matches!(
            (configs[1], motion),
            (Transition::Tween(_), Motion::Tween(_)) | (Transition::Spring(_), Motion::Spring(_))
        ));
        for _ in 0..300 {
            motion.tick(16);
        }
        assert!(motion.is_done());
        assert_eq!(motion.value(), (-20).into());
    }
}

#[test]
fn immediate_configs_finish_at_construction_and_redirection() {
    for config in [
        Transition::from(TweenConfig::new(0, &ease::LINEAR)),
        SpringConfig {
            duration_ms: 0,
            ..BOUNCY
        }
        .into(),
    ] {
        assert!(config.is_immediate());
        let direct = config.motion(Fixed::ZERO, 100.into());
        assert!(direct.is_done());
        assert_eq!(direct.value(), 100.into());
        for initial in [Transition::from(TWEEN), BOUNCY.into()] {
            let mut motion = initial.motion(Fixed::ZERO, 100.into());
            motion.tick(16);
            config.retarget(&mut motion, (-40).into());
            for dt in [0, 16, 100] {
                assert!(motion.is_done());
                assert_eq!(motion.value(), (-40).into());
                motion.tick(dt);
            }
        }
    }
}

#[test]
fn spring_reconfiguration_retains_motion_even_at_a_target_crossing() {
    let spring = Spring::preset(Fixed::ONE, Fixed::ONE, BOUNCY).with_velocity(10.into());
    let mut motion: Motion = spring.into();
    Transition::from(SMOOTH).retarget(&mut motion, Fixed::ONE);
    assert!(!motion.is_done());
    let Motion::Spring(actual) = motion else {
        panic!("expected spring")
    };
    assert_eq!(actual.position, spring.position);
    assert_eq!(actual.velocity, spring.velocity);
    assert_eq!(actual.integrated_position, spring.integrated_position);
    assert_eq!(actual.integrated_velocity, spring.integrated_velocity);
    assert_eq!(actual.perceptual_duration(), SMOOTH.duration_ms);
}

#[test]
fn finite_transitions_do_not_inherit_looping_runtime_modes() {
    let mut tween: Motion = Tween::new(
        Fixed::ZERO,
        Fixed::ONE,
        300,
        &ease::OUT_CUBIC,
        PlayMode::Loop,
    )
    .into();
    let mut spring: Motion = Spring::preset(Fixed::ZERO, Fixed::ONE, BOUNCY)
        .repeat()
        .into();
    Transition::from(TWEEN).retarget(&mut tween, Fixed::ONE);
    Transition::from(BOUNCY).retarget(&mut spring, Fixed::ONE);
    for _ in 0..300 {
        tween.tick(16);
        spring.tick(16);
    }
    assert!(tween.is_done() && spring.is_done());
}
