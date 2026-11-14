use super::render::{Field, field_view};
use super::state::{DOT_RADIUS, DOTS, STAGE, Swarm};
use super::*;
use crate::prelude::*;
use crate::ui::Children;

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(DEFAULT_VIEW.0, DEFAULT_VIEW.1);
    app.with_default_widgets().with_widget(field_view());
    let swarm = app.add_model(Swarm::default()).finish();
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets(swarm));
    assert_eq!(app.world.query::<Field>().iter().count(), 1);
    assert!(
        app.world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn dots_turn_round_at_the_walls_and_never_leave_the_stage() {
    let mut app = App::headless(64, 64);
    let swarm = app.add_model(Swarm::default()).finish();
    let margin = Fixed::from_int(DOT_RADIUS);
    let (width, height) = (Fixed::from_int(STAGE.0), Fixed::from_int(STAGE.1));
    let start = swarm.positions();
    let mut moved = false;
    for _ in 0..4_000 {
        swarm.tick();
        let positions = swarm.positions();
        assert_eq!(positions.len(), DOTS);
        for at in positions {
            assert!(at.x >= margin && at.x <= width - margin);
            assert!(at.y >= margin && at.y <= height - margin);
        }
        moved |= positions != start;
    }
    assert!(moved);
    assert_eq!(swarm.frame(), 4_000);
}

#[test]
fn painting_follows_the_frame_without_allocating() {
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
            let moving = match *request.command {
                DrawCommand::FillPath { transform, .. } => [transform.tx, transform.ty],
                DrawCommand::Fill { area, .. } => [area.x, area.y],
                DrawCommand::Arc { radius, width, .. } => [radius, width],
                _ => panic!("unexpected particle paint request"),
            };
            for value in moving {
                self.hash = self
                    .hash
                    .wrapping_mul(0x100_0000_01b3)
                    .wrapping_add(u64::from(value.to_f32().to_bits()));
            }
            self.calls += 1;
            Ok(())
        }

        fn flush(&mut self) -> Result<(), RenderError> {
            Ok(())
        }
    }

    let mut app = App::headless(DEFAULT_VIEW.0, DEFAULT_VIEW.1);
    app.with_default_widgets().with_widget(field_view());
    let swarm = app.add_model(Swarm::default()).finish();
    let node = app.world.spawn_empty();
    app.world.insert(
        node,
        Field {
            swarm: swarm.clone(),
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
    assert!(calls > 3 * 2 + DOTS * 2, "rings, bars and glowing dots");
    assert_eq!(frame(&app), (first, calls));
    for _ in 0..20 {
        swarm.tick();
    }
    assert_ne!(frame(&app).0, first, "dots, rings and bars have all moved");
}
