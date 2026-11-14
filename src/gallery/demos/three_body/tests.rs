use super::render::{Field, field_view};
use super::state::{ARENA, BODIES, BODY_RADIUS, Orbit, TRAIL};
use super::*;
use crate::prelude::*;
use crate::ui::Children;

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_widget(field_view());
    let orbit = app.add_model(Orbit::default()).finish();
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets(orbit));
    assert_eq!(app.world.query::<Field>().iter().count(), 1);
    assert!(
        app.world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn bodies_stay_inside_the_ring_however_long_they_run() {
    let mut app = App::headless(64, 64);
    let orbit = app.add_model(Orbit::default()).finish();
    let limit = Fixed::from_int(ARENA - BODY_RADIUS);
    let mut moved = false;
    let start = orbit.positions();
    for _ in 0..3_000 {
        orbit.tick();
        for position in orbit.positions() {
            assert!(position.length() <= limit + Fixed::ONE, "left the ring");
        }
        moved |= orbit.positions() != start;
    }
    assert!(moved);
    assert_eq!(orbit.frame(), 3_000);
}

#[test]
fn the_kicks_keep_the_bodies_moving() {
    let mut app = App::headless(64, 64);
    let orbit = app.add_model(Orbit::default()).finish();
    for _ in 0..2_000 {
        orbit.tick();
    }
    let before = orbit.positions();
    for _ in 0..30 {
        orbit.tick();
    }
    assert_ne!(before, orbit.positions());
}

#[test]
fn a_tail_lists_the_newest_position_last() {
    use super::state::Body;
    use crate::gallery::play::plane::Vec2;

    let mut body = Body::new(
        Vec2::new(Fixed::ZERO, Fixed::ZERO),
        Vec2::new(Fixed::from_int(2), Fixed::ZERO),
    );
    for _ in 0..10 {
        body.advance();
    }
    let trail: alloc::vec::Vec<_> = body.trail().collect();
    assert_eq!(trail.len(), TRAIL);
    assert_eq!(trail.last().copied(), Some(body.at));
    assert!(trail.windows(2).all(|pair| pair[0].x <= pair[1].x));
    assert_eq!(BODIES, 3);
}

#[test]
fn painting_follows_the_simulation_without_allocating() {
    use crate::render::command::DrawCommand;
    use crate::render::renderer::{DrawRequest, RenderError, RenderRoute, Renderer};
    use crate::types::{Rect, Transform};
    use crate::ui::Style;
    use crate::ui::theme::WidgetState;
    use crate::ui::view::PaintCtx;

    #[derive(Default)]
    struct Trace {
        hash: u64,
        calls: usize,
    }
    impl Renderer for Trace {
        type TargetState = ();

        fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            request.validate()?;
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &crate::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            match *request.command {
                DrawCommand::FillPath { transform, .. } => {
                    for value in [transform.tx, transform.ty] {
                        self.hash = self
                            .hash
                            .wrapping_mul(0x100_0000_01b3)
                            .wrapping_add(u64::from(value.to_f32().to_bits()));
                    }
                }
                DrawCommand::Line { .. } | DrawCommand::Arc { .. } => {}
                _ => panic!("unexpected orbit paint request"),
            }
            self.calls += 1;
            Ok(())
        }

        fn flush(&mut self) -> Result<(), RenderError> {
            Ok(())
        }
    }

    let mut app = App::headless(480, 320);
    app.with_default_widgets().with_widget(field_view());
    let orbit = app.add_model(Orbit::default()).finish();
    let node = app.world.spawn_empty();
    app.world.insert(
        node,
        Field {
            orbit: orbit.clone(),
        },
    );

    let render = field_view().render().unwrap();
    let style = Style::default();
    let rect = Rect::new(0, 0, 440, 260);
    let frame = |app: &App<_>| {
        let mut trace = Trace::default();
        let mut paint = PaintCtx::new(
            &mut trace,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        assert_eq!(
            crate::test_support::tracked_allocations(|| render(&app.world, node, &mut paint)),
            0
        );
        assert_eq!(paint.error(), None);
        (trace.hash, trace.calls)
    };

    let (first, calls) = frame(&app);
    assert!(calls > BODIES * TRAIL, "rings, springs, tails and bodies");
    assert_eq!(frame(&app), (first, calls));
    for _ in 0..20 {
        orbit.tick();
    }
    assert_ne!(frame(&app).0, first, "the bodies have moved");
}
