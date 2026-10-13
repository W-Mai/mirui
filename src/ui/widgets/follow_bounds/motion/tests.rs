use super::*;
use crate::anim::{BOUNCY, SMOOTH};
use crate::surface::FramebufferAccess;
use crate::types::Dimension;
use crate::ui::widgets::follow_bounds::tests::{fixture, rect};
use crate::ui::{Hidden, Style};

#[test]
fn first_placement_is_immediate_and_registered_motion_matches_shared_spring() {
    let (mut app, _, source, follower) = fixture();
    app.world.get_mut::<FollowBounds>(follower).unwrap().spring = Some(SMOOTH);
    app.render().unwrap();
    assert_eq!(rect(&app, follower), rect(&app, source));
    app.world.get_mut::<Style>(source).unwrap().layout.left = Dimension::px(24);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    assert_eq!(rect(&app, follower).x, Fixed::from_int(16));
    let mut expected = Spring::preset((-54).into(), (-34).into(), SMOOTH);
    for _ in 0..160 {
        app.tick();
        expected.tick(16);
        app.render_dirty().unwrap();
        assert_eq!(
            rect(&app, follower).x,
            expected.value() + Fixed::from_int(70)
        );
        if expected.is_settled() {
            break;
        }
    }
    assert!(expected.is_settled());
    assert_eq!(rect(&app, follower), rect(&app, source));
}

#[test]
fn retarget_keeps_velocity_and_dirty_pixels_match_full_frames() {
    let (mut app, _, source, follower) = fixture();
    app.world.get_mut::<FollowBounds>(follower).unwrap().spring = Some(BOUNCY);
    app.render().unwrap();
    app.world.get_mut::<Style>(source).unwrap().layout.left = Dimension::px(24);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    for _ in 0..5 {
        app.tick();
        app.render_dirty().unwrap();
    }
    let Motion::Spring(mut expected) = app.world.get::<FollowMotion>(follower).unwrap().axes[0]
    else {
        panic!("expected spring")
    };
    let before = rect(&app, follower);
    app.world.get_mut::<Style>(source).unwrap().layout.left = Dimension::px(-4);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    assert_eq!(rect(&app, follower), before);
    let Motion::Spring(actual) = app.world.get::<FollowMotion>(follower).unwrap().axes[0] else {
        panic!("expected spring")
    };
    assert_eq!(actual.velocity, expected.velocity);
    expected.retarget((-62).into(), Some(BOUNCY));
    for _ in 0..120 {
        app.tick();
        expected.tick(16);
        app.render_dirty().unwrap();
        assert_eq!(
            rect(&app, follower).x,
            expected.value() + Fixed::from_int(70)
        );
        let dirty = app.backend.framebuffer().buf.as_slice().to_vec();
        app.render().unwrap();
        assert_eq!(app.backend.framebuffer().buf.as_slice(), dirty);
    }
    assert!(expected.is_settled());
}

#[test]
fn hidden_recovery_and_disabling_motion_do_not_replay_stale_bounds() {
    let (mut app, _, source, follower) = fixture();
    app.world.get_mut::<FollowBounds>(follower).unwrap().spring = Some(SMOOTH);
    app.render().unwrap();
    app.world.insert(source, Hidden);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    app.tick();
    app.world.get_mut::<Style>(source).unwrap().layout.width = Dimension::px(12);
    app.world.remove::<Hidden>(source);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    assert_eq!(rect(&app, follower), rect(&app, source));
    for config in [
        None,
        Some(SpringConfig {
            duration_ms: 0,
            ..SMOOTH
        }),
    ] {
        app.world.get_mut::<FollowBounds>(follower).unwrap().spring = config;
        app.world.get_mut::<Style>(source).unwrap().layout.width = Dimension::px(10);
        app.world.invalidate(source);
        app.world.invalidate(follower);
        app.render_dirty().unwrap();
        assert!(!app.world.has::<FollowMotion>(follower));
        assert_eq!(rect(&app, follower), rect(&app, source));
    }
}

#[test]
fn prepared_animation_reuses_storage_and_static_followers_have_no_motion_state() {
    let (mut app, _, source, follower) = fixture();
    app.render().unwrap();
    assert!(!app.world.has::<FollowMotion>(follower));
    app.world.get_mut::<FollowBounds>(follower).unwrap().spring = Some(SMOOTH);
    app.world.invalidate(follower);
    app.render_dirty().unwrap();
    for _ in 0..3 {
        app.tick();
        app.render_dirty().unwrap();
    }
    app.world.get_mut::<Style>(source).unwrap().layout.width = Dimension::px(12);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..120 {
            app.tick();
            app.render_dirty().unwrap();
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(rect(&app, follower), rect(&app, source));
}

#[test]
fn bouncy_sizes_remain_nonnegative_and_input_uses_displayed_bounds() {
    let (mut app, root, source, follower) = fixture();
    app.world.get_mut::<FollowBounds>(follower).unwrap().spring = Some(BOUNCY);
    app.render().unwrap();
    app.world.get_mut::<Style>(source).unwrap().layout.width = Dimension::px(1);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    let mut undershot = false;
    for _ in 0..120 {
        app.tick();
        crate::ui::render_system::prepare_input_geometry(
            &mut app.world,
            root,
            &crate::types::Viewport::new(128, 96, Fixed::ONE),
        )
        .unwrap();
        let displayed = rect(&app, follower);
        assert!(displayed.w >= Fixed::ZERO);
        let value = app.world.get::<FollowMotion>(follower).unwrap().axes[2].value();
        undershot |= value < Fixed::ZERO;
        assert_eq!(displayed.w, value.max(Fixed::ZERO));
        if displayed.w > Fixed::from_int(2) {
            assert_eq!(
                crate::input::event::hit_test::hit_test(
                    &app.world,
                    root,
                    displayed.x + Fixed::ONE,
                    displayed.y + Fixed::ONE,
                    128,
                    96,
                ),
                Some(follower),
            );
        }
        app.render_dirty().unwrap();
    }
    assert!(undershot);
    assert_eq!(rect(&app, follower), rect(&app, source));
}
