use super::wings::{
    ANTENNAE, BODY, DISC, FOREWING, FOREWING_GLOW, FOREWING_VEINS, HINDWING, HINDWING_GLOW,
    HINDWING_VEINS,
};
use super::*;
use crate::prelude::*;
use crate::ui::Children;

#[test]
fn build_widgets_smoke() {
    let mut app = App::headless(64, 64);
    app.with_default_widgets().with_widget(butterfly_view());
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    assert_eq!(app.world.query::<Butterfly>().iter().count(), 1);
    assert!(
        app.world
            .get::<Children>(parent)
            .is_some_and(|c| !c.0.is_empty()),
    );
}

#[test]
fn wing_geometry_stays_in_static_storage() {
    for path in [
        &DISC,
        &FOREWING,
        &FOREWING_GLOW,
        &FOREWING_VEINS,
        &HINDWING,
        &HINDWING_GLOW,
        &HINDWING_VEINS,
        &BODY,
        &ANTENNAE,
    ] {
        assert!(path.is_borrowed());
    }
}

#[test]
fn the_three_motions_wind_up_the_pose() {
    let _guard = crate::core::time::mock::install();
    crate::core::time::mock::set_ms(0);
    let mut app = App::headless(128, 128);
    app.with_default_widgets().with_default_systems();
    app.add_resource(MonoClock::from_time_source());
    app.add_system(Flap::system())
        .add_system(DriftX::system())
        .add_system(DriftY::system());
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    app.systems.run_all(&mut app.world).unwrap();

    crate::core::time::mock::set_ms(90);
    app.systems.run_all(&mut app.world).unwrap();
    let query = app.world.query::<Butterfly>();
    let (_, pose) = query.iter().next().unwrap();
    assert!(pose.flap > Fixed::ZERO, "the wings have started to beat");
    assert!(pose.drift_x > Fixed::ZERO && pose.drift_y > Fixed::ZERO);
    assert!(
        pose.flap > pose.drift_x,
        "the wings beat faster than it drifts"
    );
}

#[test]
fn painting_follows_the_pose_without_allocating() {
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
    impl Renderer for Trace {
        type TargetState = ();

        fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            request.validate()?;
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, draw: &crate::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            match *request.command {
                DrawCommand::FillPath { transform, .. }
                | DrawCommand::StrokePath { transform, .. } => {
                    for value in [transform.m00, transform.m01, transform.tx, transform.ty] {
                        self.hash = self
                            .hash
                            .wrapping_mul(0x100_0000_01b3)
                            .wrapping_add(u64::from(value.to_f32().to_bits()));
                    }
                }
                _ => panic!("unexpected butterfly paint request"),
            }
            self.calls += 1;
            Ok(())
        }

        fn flush(&mut self) -> Result<(), RenderError> {
            Ok(())
        }
    }

    let render = butterfly_view().render().unwrap();
    let style = Style::default();
    let rect = Rect::new(0, 0, 320, 240);
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
    world.insert_resource(crate::ui::Theme::dark());
    let node = world.spawn_empty();
    world.insert(node, Butterfly::default());
    let (resting, calls) = frame(&world, node);
    assert!(
        calls > 30,
        "two pairs of wings, their markings and the body"
    );
    assert_eq!(frame(&world, node), (resting, calls));
    world.insert(
        node,
        Butterfly {
            flap: Fixed::from_int(90),
            ..Butterfly::default()
        },
    );
    assert_ne!(frame(&world, node).0, resting, "the wings follow the beat");
    world.insert(
        node,
        Butterfly {
            drift_x: Fixed::from_int(90),
            ..Butterfly::default()
        },
    );
    assert_ne!(frame(&world, node).0, resting, "the body follows the drift");
}
