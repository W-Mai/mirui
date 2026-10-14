use super::*;
use crate::types::Fixed;
use crate::ui::widgets::navigation::{PageTransition, transition};
use crate::ui::widgets::transform::WidgetTransform;

fn animated() -> (TestApp, Entity, SelectionModelHandle) {
    let (mut app, container, selection) = setup();
    app.world
        .get_mut::<PageView<Route>>(container)
        .unwrap()
        .transition = PageTransition::slide(crate::anim::INTERACTIVE);
    (app, container, selection)
}

fn advance(app: &mut TestApp, dt: u16) {
    app.world.insert_resource(crate::ecs::DeltaTimeMs(dt));
    transition::advance::<PageState<Route>>(&mut app.world).unwrap();
    crate::ui::delivery::flush(&mut app.world).unwrap();
    app.render().unwrap();
}

fn finish_pair(app: &mut TestApp, container: Entity) {
    let destination = transition::running(&app.world, container).unwrap().to;
    for _ in 0..240 {
        advance(app, 16);
        if transition::running(&app.world, container).is_none_or(|pair| pair.to != destination) {
            return;
        }
    }
    use crate::anim::MotionComponent;
    let pair = transition::running(&app.world, container).unwrap();
    panic!(
        "page motion did not finish: value={}",
        pair.motion().value()
    );
}

#[test]
fn tween_pages_keep_active_configuration_on_reversal_and_apply_changes_to_the_next_pair() {
    use crate::anim::{Motion, MotionComponent, Tween, TweenConfig, ease};
    let config = TweenConfig::new(300, &ease::OUT_CUBIC);
    let (mut app, container, selection) = animated();
    app.world
        .get_mut::<PageView<Route>>(container)
        .unwrap()
        .transition = PageTransition::slide(config);
    let home = mounted(&app.world, container);
    selection.select(Route::Details);
    app.render().unwrap();
    let mut expected = Tween::preset(Fixed::ZERO, Fixed::ONE, config);
    for (index, dt) in [16, 24, 8, 16, 32, 16, 4, 16].into_iter().enumerate() {
        if index == 3 || index == 5 {
            let returning = index == 3;
            app.world
                .get_mut::<PageView<Route>>(container)
                .unwrap()
                .transition = PageTransition::slide(crate::anim::BOUNCY);
            selection.select(if returning {
                Route::Home
            } else {
                Route::Details
            });
            crate::ui::delivery::flush(&mut app.world).unwrap();
            expected.retarget(if returning { Fixed::ZERO } else { Fixed::ONE }, config);
            assert_eq!(
                transition::running(&app.world, container)
                    .unwrap()
                    .motion()
                    .value(),
                expected.value()
            );
        }
        expected.tick(dt);
        advance(&mut app, dt);
        let pair = transition::running(&app.world, container).unwrap();
        let Motion::Tween(actual) = pair.motion() else {
            panic!("expected tween")
        };
        assert_eq!(actual.elapsed_ms, expected.elapsed_ms);
        assert_eq!(actual.value(), expected.value());
        assert_eq!(offset(&app, home), -expected.value() * 120);
    }
    finish_pair(&mut app, container);
    selection.select(Route::Home);
    app.render().unwrap();
    assert!(matches!(
        transition::running(&app.world, container).unwrap().motion(),
        Motion::Spring(_)
    ));
    finish_pair(&mut app, container);
    app.world
        .get_mut::<PageView<Route>>(container)
        .unwrap()
        .transition = PageTransition::slide(TweenConfig::new(0, &ease::LINEAR));
    let previous = mounted(&app.world, container);
    selection.select(Route::Details);
    app.render().unwrap();
    assert!(transition::running(&app.world, container).is_none());
    assert!(!app.world.is_alive(previous));
}

fn offset(app: &TestApp, host: Entity) -> Fixed {
    app.world
        .get::<WidgetTransform>(host)
        .unwrap()
        .0
        .apply_point(crate::types::Point::ZERO)
        .x
}

#[test]
fn page_motion_matches_the_shared_spring_and_retargets_without_losing_velocity() {
    use crate::anim::{Motion, MotionComponent, Spring};
    for config in [
        crate::anim::INTERACTIVE,
        crate::anim::SMOOTH,
        crate::anim::BOUNCY,
    ] {
        let (mut app, container, selection) = animated();
        app.world
            .get_mut::<PageView<Route>>(container)
            .unwrap()
            .transition = PageTransition::slide(config);
        let home = mounted(&app.world, container);
        selection.select(Route::Details);
        app.render().unwrap();
        let mut expected = Spring::preset(Fixed::ZERO, Fixed::ONE, config);
        for (index, dt) in [16, 24, 8, 16, 32, 16, 4, 16].into_iter().enumerate() {
            if index == 3 || index == 5 {
                let route = if index == 3 {
                    Route::Home
                } else {
                    Route::Details
                };
                let target = if index == 3 { Fixed::ZERO } else { Fixed::ONE };
                selection.select(route);
                crate::ui::delivery::flush(&mut app.world).unwrap();
                let pair = transition::running(&app.world, container).unwrap();
                let Motion::Spring(actual) = pair.motion() else {
                    panic!("expected spring")
                };
                assert_eq!(actual.position, expected.position);
                assert_eq!(actual.velocity, expected.velocity);
                expected.retarget(target, None);
                assert_eq!(actual.target, expected.target);
            }
            expected.tick(dt);
            advance(&mut app, dt);
            let pair = transition::running(&app.world, container).unwrap();
            let Motion::Spring(actual) = pair.motion() else {
                panic!("expected spring")
            };
            assert_eq!(actual.position, expected.position);
            assert_eq!(actual.velocity, expected.velocity);
            assert_eq!(offset(&app, home), -expected.value() * 120);
        }
        finish_pair(&mut app, container);
        assert!(!app.world.is_alive(home));
    }
}

#[test]
fn slide_keeps_both_pages_and_reverses_without_rebuilding() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, container, selection) = animated();
        app.world
            .get_mut::<PageView<Route>>(container)
            .unwrap()
            .transition = PageTransition::slide(config);
        let home = mounted(&app.world, container);
        selection.select(Route::Details);
        app.render().unwrap();
        let details = mounted(&app.world, container);
        assert!(app.world.is_alive(home));
        assert!(transition::running(&app.world, container).is_some());
        assert_eq!(offset(&app, home), Fixed::ZERO);
        assert_eq!(offset(&app, details), Fixed::from_int(120));
        advance(&mut app, 80);
        let before = offset(&app, home);
        assert!(before < Fixed::ZERO);
        assert!(before > Fixed::from_int(-120));
        selection.select(Route::Home);
        crate::ui::delivery::flush(&mut app.world).unwrap();
        assert_eq!(offset(&app, home), before);
        finish_pair(&mut app, container);
        assert_eq!(mounted(&app.world, container), home);
        assert!(!app.world.is_alive(details));
        assert!(transition::running(&app.world, container).is_none());
        assert_eq!(offset(&app, home), Fixed::ZERO);
    }
}

#[test]
fn reversing_twice_near_the_destination_still_completes() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, container, selection) = animated();
        app.world
            .get_mut::<PageView<Route>>(container)
            .unwrap()
            .transition = PageTransition::slide(config);
        selection.select(Route::Details);
        app.render().unwrap();
        let details = mounted(&app.world, container);
        for _ in 0..240 {
            advance(&mut app, 16);
            if offset(&app, details) < Fixed::ONE {
                break;
            }
        }
        assert!(transition::running(&app.world, container).is_some());
        selection.select(Route::Home);
        crate::ui::delivery::flush(&mut app.world).unwrap();
        selection.select(Route::Details);
        crate::ui::delivery::flush(&mut app.world).unwrap();
        finish_pair(&mut app, container);
    }
}

#[test]
fn rapid_requests_keep_only_the_latest_destination_and_a_single_transition_pair() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, container, selection) = animated();
        app.world
            .get_mut::<PageView<Route>>(container)
            .unwrap()
            .transition = PageTransition::slide(config);
        selection.select(Route::Details);
        app.render().unwrap();
        for id in 0..12 {
            selection.select(Route::Other(id));
            crate::ui::delivery::flush(&mut app.world).unwrap();
            assert_eq!(app.world.query::<Text>().iter().count(), 2);
        }
        finish_pair(&mut app, container);
        assert!(transition::running(&app.world, container).is_some());
        assert_eq!(
            app.world
                .get::<PageState<Route>>(container)
                .unwrap()
                .displayed,
            Some(Route::Other(11))
        );
        assert_eq!(app.world.query::<Text>().iter().count(), 2);
        finish_pair(&mut app, container);
        assert!(transition::running(&app.world, container).is_none());
        assert_eq!(app.world.query::<Text>().iter().count(), 1);
    }
}

#[test]
fn motion_frames_reuse_storage_and_zero_duration_has_no_retained_departure() {
    let (mut app, container, selection) = animated();
    selection.select(Route::Details);
    app.render().unwrap();
    advance(&mut app, 16);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..4 {
                transition::advance::<PageState<Route>>(&mut app.world).unwrap();
                crate::ui::delivery::flush(&mut app.world).unwrap();
            }
        }),
        0
    );
    finish_pair(&mut app, container);
    let previous = mounted(&app.world, container);
    app.world
        .get_mut::<PageView<Route>>(container)
        .unwrap()
        .transition = PageTransition::slide(crate::anim::SpringConfig {
        duration_ms: 0,
        bounce: Fixed::ZERO,
    });
    selection.select(Route::Home);
    app.render().unwrap();
    assert!(transition::running(&app.world, container).is_none());
    assert!(!app.world.is_alive(previous));
}

#[test]
fn transition_locks_input_without_disabling_the_visual_theme_or_reactive_effects() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, container, selection) = animated();
        app.world
            .get_mut::<PageView<Route>>(container)
            .unwrap()
            .transition = PageTransition::slide(config);
        let home = mounted(&app.world, container);
        let old = crate::core::reactive::retained::InputLease::capture(&app.world, home).unwrap();
        selection.select(Route::Details);
        app.render().unwrap();
        for host in [home, mounted(&app.world, container)] {
            let child = app.world.get::<Children>(host).unwrap().0[0];
            assert!(
                !crate::input::event::listeners::InputBranch::scan(&app.world, child)
                    .unwrap()
                    .allowed()
            );
            assert!(crate::core::reactive::retained::enabled(&app.world, child));
            assert!(!app.world.has::<crate::ui::state::UserState>(host));
        }
        assert!(!old.valid(&app.world));
        finish_pair(&mut app, container);
        assert!(
            crate::input::event::listeners::InputBranch::scan(
                &app.world,
                mounted(&app.world, container)
            )
            .unwrap()
            .allowed()
        );
    }
}

#[test]
fn resizing_during_slide_updates_translation_in_the_same_layout_settle() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, container, selection) = animated();
        app.world
            .get_mut::<PageView<Route>>(container)
            .unwrap()
            .transition = PageTransition::slide(config);
        selection.select(Route::Details);
        app.render().unwrap();
        advance(&mut app, 100);
        let details = mounted(&app.world, container);
        let before = offset(&app, details);
        let flush: fn(&[u8], crate::types::PhysicalRect) = |_, _| {};
        app.backend = crate::surface::framebuf::FramebufSurface::new(240, 80, flush);
        app.render_dirty().unwrap();
        assert_eq!(offset(&app, details), before * 2);
        assert_eq!(
            app.world
                .get::<crate::ui::ComputedRect>(details)
                .unwrap()
                .0
                .w,
            Fixed::from_int(240)
        );
    }
}

#[test]
fn slide_width_uses_the_padded_content_viewport() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, container, selection) = animated();
        app.world
            .get_mut::<PageView<Route>>(container)
            .unwrap()
            .transition = PageTransition::slide(config);
        app.world
            .get_mut::<Style>(container)
            .unwrap()
            .layout
            .padding = Padding::all(10);
        app.world.invalidate(container);
        app.render().unwrap();
        selection.select(Route::Details);
        app.render().unwrap();
        let page = mounted(&app.world, container);
        let rect = app.world.get::<crate::ui::ComputedRect>(page).unwrap().0;
        assert_eq!(rect.x, Fixed::from_int(10));
        assert_eq!(rect.w, Fixed::from_int(100));
        assert_eq!(offset(&app, page), rect.w);
    }
}

#[crate::event]
fn color_page(event: event!(CreatePage<Route>)) -> Entity {
    let color = if *event.route() == Route::Home {
        Color::rgb(255, 0, 0)
    } else {
        Color::rgb(0, 0, 255)
    };
    ui! {
        View(grow: 1.0, bg_color: color) {
            if *event.route() != Route::Home {
                View(position: crate::ui::layout::Position::Absolute, left: 4, top: 4,
                    width: 20, height: 20, bg_color: Color::rgb(0, 255, 0))
            }
        }
    }
}

#[crate::compose]
fn color_panel(selection: model!(SelectionModel), transition: PageTransition) -> Entity {
    ui! {
        PageView(page: ${ selection.selected() }, width: 80, height: 60, transition: transition)
        on CreatePage(color_page::callback())
    }
}

fn colors(budget: usize, transition: PageTransition) -> (TestApp, Entity, SelectionModelHandle) {
    let mut app = App::headless(100, 80);
    app.with_default_widgets();
    app.with_widget(view::<Route>());
    app.with_offscreen_pool_budget(budget);
    let root = app.spawn_root().bg_color(Color::rgb(0, 0, 0)).id();
    app.world.get_mut::<Style>(root).unwrap().layout.padding = Padding::all(10);
    let selection = app
        .add_model(SelectionModel {
            selected: Route::Home,
        })
        .finish();
    let container = app.compose(root, color_panel(selection.clone(), transition));
    app.render().unwrap();
    assert!(transition::running(&app.world, container).is_none());
    (app, container, selection)
}

fn pixel(app: &mut TestApp, x: i32, y: i32) -> Color {
    use crate::surface::FramebufferAccess;
    app.backend.framebuffer().get_pixel(x, y)
}

#[test]
fn fade_composes_complete_groups_and_does_not_flash_before_outputs_are_ready() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        let (mut app, container, selection) = colors(80 * 60 * 8, PageTransition::fade(config));
        selection.select(Route::Details);
        app.render_dirty().unwrap();
        assert_eq!(pixel(&mut app, 20, 20), Color::rgb(255, 0, 0));
        advance(&mut app, 100);
        assert!(transition::running(&app.world, container).is_some());
        let overlap = pixel(&mut app, 20, 20);
        let pair = transition::running(&app.world, container).unwrap();
        let alpha = app
            .world
            .get::<crate::ui::OffscreenRender>(pair.to)
            .unwrap()
            .opacity;
        let red = (255 - u32::from(alpha)).pow(2) / 255;
        assert!(
            (i32::from(overlap.r) - red as i32).abs() <= 2,
            "{overlap:?}"
        );
        assert!(
            (i32::from(overlap.g) - i32::from(alpha)).abs() <= 2,
            "{overlap:?}"
        );
        assert_eq!(overlap.b, 0);
        finish_pair(&mut app, container);
        assert_eq!(pixel(&mut app, 20, 20), Color::rgb(0, 255, 0));
        assert!(transition::running(&app.world, container).is_none());
    }
}

#[test]
fn unavailable_fade_buffers_present_the_complete_destination_on_the_first_frame() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        for budget in [0, 80 * 60 * 4] {
            let (mut app, container, selection) = colors(budget, PageTransition::fade(config));
            selection.select(Route::Details);
            app.render_dirty().unwrap();
            assert_eq!(pixel(&mut app, 20, 20), Color::rgb(0, 255, 0));
            assert_eq!(pixel(&mut app, 60, 40), Color::rgb(0, 0, 255));
            advance(&mut app, 16);
            assert!(transition::running(&app.world, container).is_none());
            assert_eq!(
                app.world
                    .query::<crate::ui::OffscreenRender>()
                    .iter()
                    .count(),
                0
            );
        }
    }
}

#[test]
fn fade_reverses_without_reallocation_and_matches_dirty_composition() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        use crate::surface::FramebufferAccess;
        let (mut app, container, selection) = colors(80 * 60 * 8, PageTransition::fade(config));
        let home = mounted(&app.world, container);
        selection.select(Route::Details);
        app.render_dirty().unwrap();
        advance(&mut app, 80);
        selection.select(Route::Home);
        crate::ui::delivery::flush(&mut app.world).unwrap();
        app.world.insert_resource(crate::ecs::DeltaTimeMs(8));
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..4 {
                    transition::advance::<PageState<Route>>(&mut app.world).unwrap();
                    crate::ui::delivery::flush(&mut app.world).unwrap();
                    app.render_dirty().unwrap();
                }
            }),
            0
        );
        let dirty = app.backend.framebuffer().buf.as_slice().to_vec();
        app.render().unwrap();
        assert_eq!(dirty, app.backend.framebuffer().buf.as_slice());
        finish_pair(&mut app, container);
        assert_eq!(mounted(&app.world, container), home);
        assert_eq!(pixel(&mut app, 20, 20), Color::rgb(255, 0, 0));
    }
}

#[test]
fn a_renderer_without_offscreen_support_draws_the_destination_inline() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        use crate::render::renderer::{DrawRequest, Renderer};
        use crate::render::sw::SwRenderer;
        use crate::render::texture::{ColorFormat, Texture};
        struct InlineOnly<'a>(SwRenderer<'a>);
        impl Renderer for InlineOnly<'_> {
            fn submit(
                &mut self,
                request: &DrawRequest<'_, '_>,
            ) -> Result<(), crate::render::RenderError> {
                self.0.submit(request)
            }
            fn flush(&mut self) {
                self.0.flush();
            }
        }
        let (mut app, container, selection) = colors(80 * 60 * 8, PageTransition::fade(config));
        selection.select(Route::Details);
        crate::ui::delivery::flush(&mut app.world).unwrap();
        let root = app.root.unwrap();
        let viewport = crate::types::Viewport::new(100, 80, Fixed::ONE);
        crate::ui::render_system::try_update_layout(&mut app.world, root, &viewport).unwrap();
        let mut pixels = [0; 100 * 80 * 4];
        let texture = Texture::new(&mut pixels, 100, 80, ColorFormat::RGBA8888);
        let mut renderer = InlineOnly(SwRenderer::new(texture));
        crate::ui::render_system::render(&app.world, root, &viewport, &mut renderer).unwrap();
        let texture = Texture::new(&mut pixels, 100, 80, ColorFormat::RGBA8888);
        assert_eq!(texture.get_pixel(20, 20), Color::rgb(0, 255, 0));
        advance(&mut app, 16);
        assert!(transition::running(&app.world, container).is_none());
    }
}

#[test]
fn slide_is_clipped_and_dirty_frames_match_full_frames() {
    for config in [
        crate::anim::Transition::from(crate::anim::INTERACTIVE),
        crate::anim::TweenConfig::new(300, &crate::anim::ease::OUT_CUBIC).into(),
    ] {
        use crate::surface::FramebufferAccess;
        let (mut app, container, selection) = colors(0, PageTransition::slide(config));
        selection.select(Route::Details);
        for _ in 0..120 {
            app.render_dirty().unwrap();
            assert_eq!(pixel(&mut app, 5, 35), Color::rgb(0, 0, 0));
            assert_eq!(pixel(&mut app, 95, 35), Color::rgb(0, 0, 0));
            let dirty = app.backend.framebuffer().buf.as_slice().to_vec();
            app.render().unwrap();
            assert_eq!(dirty, app.backend.framebuffer().buf.as_slice());
            app.world.insert_resource(crate::ecs::DeltaTimeMs(16));
            transition::advance::<PageState<Route>>(&mut app.world).unwrap();
            crate::ui::delivery::flush(&mut app.world).unwrap();
        }
        assert!(transition::running(&app.world, container).is_none());
    }
}

#[test]
fn registered_animation_system_advances_once_per_tick_and_clamps_fade_overshoot() {
    use crate::anim::{BOUNCY, Motion, MotionComponent, Spring};
    let (mut app, container, selection) = colors(80 * 60 * 8, PageTransition::fade(BOUNCY));
    selection.select(Route::Details);
    app.render().unwrap();
    let mut expected = Spring::preset(Fixed::ZERO, Fixed::ONE, BOUNCY);
    let mut overshot = false;
    for _ in 0..120 {
        app.tick();
        expected.tick(16);
        let Some(pair) = transition::running(&app.world, container) else {
            assert!(expected.is_settled());
            break;
        };
        let Motion::Spring(spring) = pair.motion() else {
            panic!("expected spring")
        };
        assert_eq!(spring.position, expected.position);
        if spring.value() > Fixed::ONE {
            overshot = true;
            assert_eq!(
                app.world
                    .get::<crate::ui::OffscreenRender>(pair.to)
                    .unwrap()
                    .opacity,
                255
            );
            assert_eq!(pixel(&mut app, 20, 20), Color::rgb(0, 255, 0));
        }
    }
    assert!(overshot);
    assert!(transition::running(&app.world, container).is_none());
}
