use crate::render::command::DrawCommand;
use crate::types::Fixed;

use super::Style;

#[crate::view(component = Style, name = "Style", priority = 50, invalidation = Layout)]
fn style_render(theme: res!(crate::ui::Theme), paint: paint!()) {
    let style = paint.style();
    let bg_color = style.bg_color;
    let border_color = style.border_color;
    let border_width = style.border_width;
    let border_radius = style.border_radius;
    if !paint.background_handled() {
        if let Some(color) = bg_color {
            paint.draw(&DrawCommand::Fill {
                area: paint.rect(),
                transform: paint.transform(),
                quad: paint.quad(),
                color: color.resolve_in(theme, paint.state()),
                radius: border_radius,
                opa: 255,
            });
            paint.set_background_handled(true);
        }
    }

    if let Some(border_color) = border_color {
        if border_width > Fixed::ZERO {
            paint.draw(&DrawCommand::Border {
                area: paint.rect(),
                transform: paint.transform(),
                quad: paint.quad(),
                color: border_color.resolve_in(theme, paint.state()),
                width: border_width,
                radius: border_radius,
                opa: 255,
            });
        }
    }
}

pub use style_render::view;

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::render::renderer::{DrawRequest, RenderError, Renderer};
    use crate::test_support::tracked_allocations;
    use crate::types::{Color, Rect, Transform};
    use crate::ui::theme::WidgetState;
    use crate::ui::view::{PaintCtx, ViewInvalidation};
    use crate::ui::{ColorToken, Theme};
    use core::any::TypeId;

    #[derive(Default)]
    struct Trace {
        colors: [Option<Color>; 2],
        draws: usize,
        failure: Option<RenderError>,
    }

    impl Renderer for Trace {
        fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            match *request.command {
                DrawCommand::Fill { color, .. } => self.colors[0] = Some(color),
                DrawCommand::Border { color, .. } => self.colors[1] = Some(color),
                _ => panic!("unexpected Style request"),
            }
            self.draws += 1;
            self.failure.map_or(Ok(()), Err)
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn typed_style_descriptor_preserves_layout_invalidation_without_allocating() {
        for _ in 0..2 {
            let mut descriptor = None;
            assert_eq!(tracked_allocations(|| descriptor = Some(view())), 0);
            let descriptor = descriptor.unwrap();
            assert_eq!(descriptor.name(), "Style");
            assert_eq!(descriptor.component_filter(), Some(TypeId::of::<Style>()));
            assert_eq!(descriptor.priority(), Some(50));
            assert_eq!(descriptor.invalidation(), ViewInvalidation::Layout);
            assert!(descriptor.render().is_some());
            assert!(descriptor.auto_attach().is_none());
            assert!(descriptor.inputs().is_none());
            assert!(descriptor.observation().is_none());
            let mut installed = 0;
            assert_eq!(
                tracked_allocations(|| {
                    descriptor.install(|_| installed += 1);
                }),
                0
            );
            assert_eq!(installed, 0);
        }
    }

    #[test]
    fn required_theme_is_refetched_and_background_and_failure_state_are_shared() {
        let mut world = World::new();
        let node = world.spawn_empty();
        let style = Style {
            bg_color: Some(ColorToken::Primary.into()),
            border_color: Some(ColorToken::Outline.into()),
            border_width: Fixed::ONE,
            border_radius: Fixed::from_int(4),
            ..Style::default()
        };
        world.insert(node, style.clone());
        let render = view().render().unwrap();
        let rect = Rect::new(7, 11, 40, 20);

        for theme in [Theme::dark(), Theme::light()] {
            world.insert_resource(theme);
            for state in [
                WidgetState::Enabled,
                WidgetState::Hovered,
                WidgetState::Disabled,
            ] {
                let theme = world.resource::<Theme>().unwrap();
                let expected = [
                    Some(theme.resolve_in(ColorToken::Primary, state)),
                    Some(theme.resolve_in(ColorToken::Outline, state)),
                ];
                for _ in 0..2 {
                    let mut trace = Trace::default();
                    {
                        let mut paint = PaintCtx::new(
                            &mut trace,
                            &style,
                            &rect,
                            &rect,
                            Transform::IDENTITY,
                            None,
                            state,
                        );
                        assert_eq!(tracked_allocations(|| render(&world, node, &mut paint)), 0);
                        assert!(paint.background_handled());
                        assert_eq!(paint.error(), None);
                        assert_eq!(tracked_allocations(|| render(&world, node, &mut paint)), 0);
                    }
                    assert_eq!(trace.colors, expected);
                    assert_eq!(trace.draws, 3);
                }
            }
        }

        let mut trace = Trace {
            failure: Some(RenderError::BackendFailure),
            ..Trace::default()
        };
        {
            let mut paint = PaintCtx::new(
                &mut trace,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            assert_eq!(tracked_allocations(|| render(&world, node, &mut paint)), 0);
            assert_eq!(paint.error(), Some(RenderError::BackendFailure));
            assert!(paint.background_handled());
            assert_eq!(tracked_allocations(|| render(&world, node, &mut paint)), 0);
        }
        assert_eq!(trace.draws, 1);

        world.remove_resource::<Theme>();
        let mut trace = Trace::default();
        {
            let mut paint = PaintCtx::new(
                &mut trace,
                &style,
                &rect,
                &rect,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                render(&world, node, &mut paint);
            }));
            assert!(error.is_err());
            assert!(!paint.background_handled());
        }
        assert_eq!(trace.draws, 0);
    }
}
