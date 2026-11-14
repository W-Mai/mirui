use super::composition::build_widgets;
use super::view::{Shapes, shapes_view};
use crate::prelude::*;
use crate::ui::Children;

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(64, 64);
    app.with_default_widgets().with_widget(shapes_view());
    let parent = app.spawn_root().id();
    assert!(app.world.resource::<MonoClock>().is_none());
    app.compose(parent, build_widgets());
    app.add_resource(MonoClock::new(|| 12_000_000));
    app.compose(parent, build_widgets());
    let world = &app.world;
    assert!(
        world
            .query::<Shapes>()
            .iter()
            .map(|(_, state)| state.start_ms)
            .eq([0_u32, 12])
    );
    assert!(
        world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn every_dial_paints_without_allocating_and_hands_follow_the_clock() {
    use crate::ecs::World;
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
    impl Trace {
        fn mix(&mut self, value: Fixed) {
            self.hash = self
                .hash
                .wrapping_mul(0x100_0000_01b3)
                .wrapping_add(u64::from(value.to_f32().to_bits()));
        }
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
                DrawCommand::Line { p2, .. } => {
                    self.mix(p2.x);
                    self.mix(p2.y);
                }
                DrawCommand::FillPath { transform, .. } => {
                    self.mix(transform.m00);
                    self.mix(transform.m01);
                }
                DrawCommand::Fill { .. } | DrawCommand::Arc { .. } => {}
                _ => panic!("unexpected clock paint request"),
            }
            self.calls += 1;
            Ok(())
        }

        fn flush(&mut self) -> Result<(), RenderError> {
            Ok(())
        }
    }

    let render = shapes_view().render().unwrap();
    let style = Style::default();
    let rect = Rect::new(0, 0, 320, 320);
    let frame = |world: &World, node: Entity| {
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
            crate::test_support::tracked_allocations(|| render(world, node, &mut paint)),
            0
        );
        assert_eq!(paint.error(), None);
        (trace.hash, trace.calls)
    };

    let mut world = World::new();
    let node = world.spawn_empty();
    let mut shapes = Shapes::default();
    let mut hashes = [0_u64; 2];
    for _ in 0..3 {
        world.insert(node, shapes);
        world.insert_resource(MonoClock::new(|| 1_337_000_000));
        let (first, calls) = frame(&world, node);
        assert!(calls > 100, "a chronograph face has many parts");
        assert_eq!(frame(&world, node), (first, calls));
        world.insert_resource(MonoClock::new(|| 2_000_000_000));
        let (later, _) = frame(&world, node);
        assert_ne!(first, later, "the hands move with the clock");
        hashes = [first, later];
        shapes = shapes.next_dial();
    }
    assert_ne!(hashes, [0, 0]);
    assert_eq!(shapes.dial, 0, "the dials cycle");
}
