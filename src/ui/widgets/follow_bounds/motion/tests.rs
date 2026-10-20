use super::*;
use crate::anim::{BOUNCY, SMOOTH, Spring, SpringConfig};
use crate::surface::FramebufferAccess;
use crate::types::Dimension;
use crate::ui::widgets::follow_bounds::tests::{fixture, rect};
use crate::ui::{Hidden, Style};

#[test]
fn position_motion_reuses_layout_without_losing_children_or_painted_damage() {
    for config in [
        Transition::from(BOUNCY),
        crate::anim::TweenConfig::new(160, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, root, source, follower) = fixture();
        app.world
            .get_mut::<FollowBounds>(follower)
            .unwrap()
            .transition = Some(config);
        app.render().unwrap();
        app.world.get_mut::<Style>(source).unwrap().layout.left = Dimension::px(24);
        app.world.invalidate(source);
        app.render_dirty().unwrap();
        let child = app.world.find_by_id("fill").unwrap();
        let revision = app.world.component_revision::<crate::ui::dirty::Dirty>();
        let viewport = crate::types::Viewport::new(128, 96, Fixed::ONE);
        let start = rect(&app, follower);
        for _ in 0..120 {
            follow_bounds_motion(&mut app.world).unwrap();
            for _ in 0..2 {
                crate::ui::render_system::prepare_input_geometry(&mut app.world, root, &viewport)
                    .unwrap();
                app.dirty_regions().unwrap();
            }
            assert_eq!(
                app.world.component_revision::<crate::ui::dirty::Dirty>(),
                revision
            );
            assert_eq!(rect(&app, child), rect(&app, follower));
            app.render_dirty().unwrap();
            let pixels = app
                .backend
                .framebuffer()
                .read_pixels()
                .unwrap()
                .bytes()
                .to_vec();
            app.render().unwrap();
            assert_eq!(
                pixels,
                app.backend.framebuffer().read_pixels().unwrap().bytes()
            );
        }
        assert_ne!(rect(&app, follower), start);
        assert_eq!(rect(&app, follower), rect(&app, source));
    }
}

#[test]
fn size_motion_keeps_layout_invalidation_and_resolves_percentage_children() {
    let (mut app, root, source, follower) = fixture();
    app.world
        .get_mut::<FollowBounds>(follower)
        .unwrap()
        .transition = Some(SMOOTH.into());
    app.render().unwrap();
    app.world.get_mut::<Style>(source).unwrap().layout.width = Dimension::px(12);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    follow_bounds_motion(&mut app.world).unwrap();
    assert!(app.world.has::<crate::ui::dirty::Dirty>(follower));
    let viewport = crate::types::Viewport::new(128, 96, Fixed::ONE);
    crate::ui::render_system::prepare_input_geometry(&mut app.world, root, &viewport).unwrap();
    let child = app.world.find_by_id("fill").unwrap();
    assert_eq!(rect(&app, child), rect(&app, follower));
    assert!(rect(&app, follower).w < Fixed::from_int(30));
}

#[test]
fn scheduler_follows_motion_storage_presence_without_spurious_completions() {
    struct Completions(u32);
    let (mut app, _, _, follower) = fixture();
    app.render().unwrap();
    app.world.insert_resource(Completions(0));
    app.world.set_callback_observer(|world| {
        world.resource_mut::<Completions>().unwrap().0 += 1;
    });
    let mut scheduler = crate::ecs::SystemScheduler::new();
    scheduler.add(follow_bounds_motion::system());
    scheduler.run_all(&mut app.world).unwrap();
    assert_eq!(app.world.resource::<Completions>().unwrap().0, 0);

    app.world
        .get_mut::<FollowBounds>(follower)
        .unwrap()
        .transition = Some(SMOOTH.into());
    for expected in [1, 2] {
        assert!(prepare(&mut app.world, follower));
        scheduler.run_all(&mut app.world).unwrap();
        assert_eq!(app.world.resource::<Completions>().unwrap().0, expected);
        app.world.remove::<FollowMotion>(follower).unwrap();
        let allocations = crate::test_support::tracked_allocations(|| {
            scheduler.run_all(&mut app.world).unwrap();
        });
        assert_eq!(allocations, 0);
        assert_eq!(app.world.resource::<Completions>().unwrap().0, expected);
    }
}

#[test]
fn tween_following_preserves_progress_across_layout_preparations_and_reconfiguration() {
    use crate::anim::{Tween, TweenConfig, ease};
    let (mut app, root, source, follower) = fixture();
    let config = TweenConfig::new(160, &ease::OUT_CUBIC);
    app.world
        .get_mut::<FollowBounds>(follower)
        .unwrap()
        .transition = Some(config.into());
    app.render().unwrap();
    app.world.get_mut::<Style>(source).unwrap().layout.left = Dimension::px(24);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    let mut expected = Tween::preset((-54).into(), (-34).into(), config);
    let viewport = crate::types::Viewport::new(128, 96, Fixed::ONE);
    for _ in 0..10 {
        app.tick();
        expected.tick(16);
        for _ in 0..3 {
            crate::ui::render_system::prepare_input_geometry(&mut app.world, root, &viewport)
                .unwrap();
        }
        app.render_dirty().unwrap();
        let Motion::Tween(actual) = app.world.get::<FollowMotion>(follower).unwrap().axes[0] else {
            panic!("expected tween");
        };
        assert_eq!(actual.elapsed_ms, expected.elapsed_ms);
        assert_eq!(
            rect(&app, follower).x,
            expected.value() + Fixed::from_int(70)
        );
        let dirty = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert_eq!(
            dirty,
            app.backend.framebuffer().read_pixels().unwrap().bytes()
        );
    }
    assert!(expected.is_finished());
    assert_eq!(rect(&app, follower), rect(&app, source));
    app.world.get_mut::<Style>(source).unwrap().layout.left = Dimension::px(4);
    app.world.invalidate(source);
    app.render_dirty().unwrap();
    for config in [
        Transition::from(BOUNCY),
        TweenConfig::new(250, &ease::IN_OUT_CUBIC).into(),
    ] {
        app.tick();
        app.render_dirty().unwrap();
        let before = rect(&app, follower);
        app.world
            .get_mut::<FollowBounds>(follower)
            .unwrap()
            .transition = Some(config);
        app.world.invalidate(follower);
        app.render_dirty().unwrap();
        assert_eq!(rect(&app, follower), before);
        assert!(matches!(
            (
                config,
                app.world.get::<FollowMotion>(follower).unwrap().axes[0]
            ),
            (Transition::Tween(_), Motion::Tween(_)) | (Transition::Spring(_), Motion::Spring(_))
        ));
    }
    for _ in 0..20 {
        app.tick();
        app.render_dirty().unwrap();
    }
    assert_eq!(rect(&app, follower), rect(&app, source));
}

#[test]
fn first_placement_is_immediate_and_registered_motion_matches_shared_spring() {
    let (mut app, _, source, follower) = fixture();
    app.world
        .get_mut::<FollowBounds>(follower)
        .unwrap()
        .transition = Some(SMOOTH.into());
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
    app.world
        .get_mut::<FollowBounds>(follower)
        .unwrap()
        .transition = Some(BOUNCY.into());
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
        let dirty = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert_eq!(
            app.backend.framebuffer().read_pixels().unwrap().bytes(),
            dirty
        );
    }
    assert!(expected.is_settled());
}

#[test]
fn hidden_recovery_and_disabling_motion_do_not_replay_stale_bounds() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, _, source, follower) = fixture();
        app.world
            .get_mut::<FollowBounds>(follower)
            .unwrap()
            .transition = Some(config);
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
            Some(Transition::from(SpringConfig {
                duration_ms: 0,
                ..SMOOTH
            })),
            Some(crate::anim::TweenConfig::new(0, &crate::anim::ease::LINEAR).into()),
        ] {
            app.world
                .get_mut::<FollowBounds>(follower)
                .unwrap()
                .transition = config;
            app.world.get_mut::<Style>(source).unwrap().layout.width = Dimension::px(10);
            app.world.invalidate(source);
            app.world.invalidate(follower);
            app.render_dirty().unwrap();
            assert!(!app.world.has::<FollowMotion>(follower));
            assert_eq!(rect(&app, follower), rect(&app, source));
        }
    }
}

#[test]
fn prepared_animation_reuses_storage_and_static_followers_have_no_motion_state() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, _, source, follower) = fixture();
        app.render().unwrap();
        assert!(!app.world.has::<FollowMotion>(follower));
        app.world
            .get_mut::<FollowBounds>(follower)
            .unwrap()
            .transition = Some(config);
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
}

#[test]
fn bouncy_sizes_remain_nonnegative_and_input_uses_displayed_bounds() {
    let (mut app, root, source, follower) = fixture();
    app.world
        .get_mut::<FollowBounds>(follower)
        .unwrap()
        .transition = Some(BOUNCY.into());
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
