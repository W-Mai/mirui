use super::super::tests::{TestApp, queue, setup};
use super::*;
use crate::anim::{BOUNCY, SNAPPY, TweenConfig, ease};
use crate::prelude::*;
use crate::surface::FramebufferAccess;

fn enable(app: &mut TestApp, container: Entity, config: Transition) {
    app.world
        .get_mut::<ScrollConfig>(container)
        .unwrap()
        .transition = Some(config);
    crate::ui::lifecycle::prepare(&mut app.world);
    app.render().unwrap();
}

fn tick(app: &mut TestApp) {
    app.world.insert_resource(crate::ecs::DeltaTimeMs(16));
    programmatic_scroll_system(&mut app.world).unwrap();
    app.render_dirty().unwrap();
}

#[test]
fn both_transition_kinds_match_shared_motion_and_queries_do_not_advance_them() {
    for config in [
        Transition::from(BOUNCY),
        TweenConfig::new(160, &ease::OUT_QUAD).into(),
    ] {
        let (mut app, _, container, row) = setup();
        enable(&mut app, container, config);
        let mut queue = queue();
        let mut batch = queue.begin();
        batch.commands().scroll_to(container, row);
        batch.finish(&mut app.world).unwrap();
        app.render_dirty().unwrap();
        assert!(active(&app.world, container));
        assert_eq!(
            app.world.get::<ScrollOffset>(container).unwrap().y,
            Fixed::ZERO
        );
        let mut expected = config.motion(Fixed::ZERO, 60.into());
        for _ in 0..100 {
            let before = app.world.get::<ScrollOffset>(container).unwrap().y;
            app.dirty_region().unwrap();
            assert_eq!(app.world.get::<ScrollOffset>(container).unwrap().y, before);
            expected.tick(16);
            tick(&mut app);
            assert_eq!(
                app.world.get::<ScrollOffset>(container).unwrap().y,
                expected.value()
            );
        }
        assert!(!active(&app.world, container));
        assert_eq!(
            app.world.get::<ScrollOffset>(container).unwrap().y,
            60.into()
        );
    }
}

#[test]
fn spring_retarget_preserves_velocity_and_warmed_animation_does_not_allocate() {
    let (mut app, _, container, row) = setup();
    enable(&mut app, container, SNAPPY.into());
    let mut queue = queue();
    let mut batch = queue.begin();
    batch.commands().scroll_to(container, row);
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    for _ in 0..4 {
        tick(&mut app);
    }
    let velocity = match app.world.get::<ProgrammaticMotion>(container).unwrap().axes[1] {
        Motion::Spring(spring) => spring.velocity,
        _ => unreachable!(),
    };
    let mut batch = queue.begin();
    batch
        .commands()
        .scroll_to(container, ScrollOffset::default());
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    match app.world.get::<ProgrammaticMotion>(container).unwrap().axes[1] {
        Motion::Spring(spring) => assert_eq!(spring.velocity, velocity),
        _ => unreachable!(),
    }
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..100 {
                tick(&mut app);
            }
        }),
        0
    );
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        Fixed::ZERO
    );
    assert!(!active(&app.world, container));
}

#[test]
fn cancellation_resize_and_boundary_contact_do_not_leave_old_motion_running() {
    let (mut app, _, container, row) = setup();
    enable(&mut app, container, BOUNCY.into());
    let mut queue = queue();
    let mut batch = queue.begin();
    batch.commands().scroll_to(container, row);
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    tick(&mut app);
    super::super::cancel_chain(&mut app.world, Some(row));
    let stopped = *app.world.get::<ScrollOffset>(container).unwrap();
    tick(&mut app);
    assert_eq!(app.world.get::<ScrollOffset>(container), Some(&stopped));
    assert!(!active(&app.world, container));
    let mut batch = queue.begin();
    batch.commands().scroll_to(
        container,
        ScrollOffset {
            x: Fixed::ZERO,
            y: 10_000.into(),
        },
    );
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    for _ in 0..100 {
        tick(&mut app);
        let y = app.world.get::<ScrollOffset>(container).unwrap().y;
        assert!((Fixed::ZERO..=Fixed::from_int(100)).contains(&y));
    }
    assert!(!active(&app.world, container));
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        100.into()
    );
    let mut batch = queue.begin();
    batch
        .commands()
        .scroll_to(container, ScrollOffset::default());
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    app.world.insert(row, crate::ui::Hidden);
    tick(&mut app);
    app.world
        .get_mut::<ScrollConfig>(container)
        .unwrap()
        .transition = None;
    crate::ui::lifecycle::prepare(&mut app.world);
    app.render_dirty().unwrap();
    assert!(!app.world.has::<ProgrammaticMotion>(container));
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        Fixed::ZERO
    );
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        pixels
    );
}

#[test]
fn nested_motion_plans_final_inner_positions_without_chasing_each_frame() {
    let mut app = App::headless(120, 90);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! {
            Column (id: "outer", width: 80, height: 40, clip_children: true) [
                ScrollOffset::default(),
                ScrollConfig::default().with_transition(SNAPPY),
            ] {
                View (height: 60, shrink: 0.0)
                Column (id: "inner", width: 80, height: 40, shrink: 0.0, clip_children: true) [
                    ScrollOffset::default(),
                    ScrollConfig::default().with_transition(SNAPPY),
                ] {
                    View (height: 60, shrink: 0.0)
                    View (id: "target", height: 20, shrink: 0.0)
                    View (height: 60, shrink: 0.0)
                }
            }
        };
    });
    app.render().unwrap();
    let inner = app.world.find_by_id("inner").unwrap();
    let outer = app.world.find_by_id("outer").unwrap();
    let target = app.world.find_by_id("target").unwrap();
    let mut queue = queue();
    let mut batch = queue.begin();
    batch.commands().scroll_into(target);
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    assert_eq!(destination(&app.world, inner).unwrap().y, 40.into());
    assert_eq!(destination(&app.world, outer).unwrap().y, 60.into());
    assert_eq!(app.world.get::<ScrollOffset>(inner).unwrap().y, Fixed::ZERO);
    for _ in 0..100 {
        tick(&mut app);
        if let Some(goal) = destination(&app.world, outer) {
            assert_eq!(goal.y, 60.into());
        }
    }
    assert_eq!(app.world.get::<ScrollOffset>(inner).unwrap().y, 40.into());
    assert_eq!(app.world.get::<ScrollOffset>(outer).unwrap().y, 60.into());
}

#[crate::event]
fn record_start(count: res!(mut Counters), _event: event!(ScrollStarted)) {
    count.started += 1;
}

#[crate::event]
fn record_end(count: res!(mut Counters), event: event!(ScrollEnded)) {
    let (settled, cancelled) = count.ended;
    count.ended = match event.reason {
        ScrollEndReason::Settled => (settled + 1, cancelled),
        ScrollEndReason::Cancelled => (settled, cancelled + 1),
    };
}

#[derive(Default)]
struct Counters {
    started: u32,
    ended: (u32, u32),
}

#[test]
fn notifications_cover_the_entire_programmatic_motion_and_user_cancellation() {
    let (mut app, root, container, row) = setup();
    app.world.insert_resource(Counters::default());
    app.compose(root, |cx| {
        cx.on_event(container, ScrollStarted::EVENT, record_start::callback());
        cx.on_event(container, ScrollEnded::EVENT, record_end::callback());
    });
    enable(
        &mut app,
        container,
        TweenConfig::new(160, &ease::OUT_QUAD).into(),
    );
    let mut queue = queue();
    let mut batch = queue.begin();
    batch.commands().scroll_to(container, row);
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    for _ in 0..20 {
        tick(&mut app);
    }
    assert_eq!(app.world.resource::<Counters>().unwrap().started, 1);
    assert_eq!(app.world.resource::<Counters>().unwrap().ended, (1, 0));
    let mut batch = queue.begin();
    batch
        .commands()
        .scroll_to(container, ScrollOffset::default());
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    tick(&mut app);
    assert_eq!(app.world.resource::<Counters>().unwrap().started, 2);
    super::super::cancel_chain(&mut app.world, Some(row));
    tick(&mut app);
    assert_eq!(app.world.resource::<Counters>().unwrap().ended, (1, 1));
}

#[test]
fn resize_clamps_before_paint_and_disabling_motion_preserves_the_latest_request() {
    let (mut app, _, container, _) = setup();
    enable(
        &mut app,
        container,
        TweenConfig::new(160, &ease::OUT_QUAD).into(),
    );
    let mut queue = queue();
    let mut batch = queue.begin();
    batch.commands().scroll_to(
        container,
        ScrollOffset {
            x: Fixed::ZERO,
            y: 1000.into(),
        },
    );
    batch.finish(&mut app.world).unwrap();
    app.render_dirty().unwrap();
    for _ in 0..4 {
        tick(&mut app);
    }
    assert!(app.world.get::<ScrollOffset>(container).unwrap().y > 50.into());
    crate::ui::property::apply_to_world::<crate::ui::property::prop::Height>(
        &mut app.world,
        container,
        crate::types::Dimension::px(90),
    );
    app.render_dirty().unwrap();
    assert!(app.world.get::<ScrollOffset>(container).unwrap().y <= 50.into());
    let mut batch = queue.begin();
    batch.commands().scroll_to(
        container,
        ScrollOffset {
            x: Fixed::ZERO,
            y: 20.into(),
        },
    );
    batch.finish(&mut app.world).unwrap();
    app.world
        .get_mut::<ScrollConfig>(container)
        .unwrap()
        .transition = None;
    crate::ui::lifecycle::prepare(&mut app.world);
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<ScrollOffset>(container).unwrap().y,
        20.into()
    );
    assert!(!app.world.has::<ProgrammaticMotion>(container));
}
