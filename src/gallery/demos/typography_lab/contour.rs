use super::runtime::UI;
use super::style::{BLUE, CYAN, GOLD, PANEL_ALT};
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::font::FontManager;
use crate::render::font::scalar::ScalarField;
use crate::ui::Theme;

#[crate::component]
pub(super) struct RasterContour {
    pub(super) font: FontToken,
    pub(super) character: char,
    pub(super) ppem: u16,
}

impl Default for RasterContour {
    fn default() -> Self {
        Self {
            font: UI,
            character: 'S',
            ppem: 56,
        }
    }
}

fn contour_color(theme: &Theme, value: Fixed) -> Color {
    let surface = theme.resolve(PANEL_ALT);
    let primary = theme.resolve(CYAN);
    let secondary = theme.resolve(BLUE);
    let success = theme.resolve(GOLD);
    match crate::types::fixed::to_textflow(value).clamp(0, 256) {
        0..=63 => surface,
        64..=111 => surface.blend_with(secondary, Fixed::from_ratio(1, 2)),
        112..=144 => success,
        145..=207 => secondary,
        _ => primary,
    }
}

#[crate::view(component = RasterContour, name = "RasterContour", priority = 61)]
fn raster_contour_render(
    spec: component!(),
    manager: res!(FontManager),
    theme: res!(Theme),
    paint: paint!(),
) {
    let font = manager.resolve(spec.font.cache_key());
    let Some(raster) = font
        .map_char(spec.character)
        .and_then(|glyph| font.raster(glyph, spec.ppem))
    else {
        return;
    };
    let Some(region) = raster.region else {
        return;
    };
    let bits = match raster.representation.kind() {
        mirx::font::FontRepresentationKind::Coverage { bits }
        | mirx::font::FontRepresentationKind::SignedDistance { bits, .. } => bits,
        _ => return,
    };
    let Some(field) = ScalarField::new(
        raster.surface.samples(),
        raster.surface.stride(),
        region,
        bits,
    ) else {
        return;
    };
    let rect = paint.rect();
    let width = i32::try_from(field.width()).unwrap_or(i32::MAX).max(1);
    let height = i32::try_from(field.height()).unwrap_or(i32::MAX).max(1);
    let cell = (rect.w.to_int() / width)
        .min(rect.h.to_int() / height)
        .max(1);
    let grid_width = Fixed::from_int(width * cell);
    let grid_height = Fixed::from_int(height * cell);
    let origin = Point {
        x: rect.x + (rect.w - grid_width) / Fixed::from_int(2),
        y: rect.y + (rect.h - grid_height) / Fixed::from_int(2),
    };
    let extent = Fixed::from_int((cell - 1).max(1));
    for y in 0..height {
        for x in 0..width {
            paint.draw(&DrawCommand::Fill {
                area: Rect {
                    x: origin.x + Fixed::from_int(x * cell),
                    y: origin.y + Fixed::from_int(y * cell),
                    w: extent,
                    h: extent,
                },
                transform: paint.transform(),
                quad: None,
                color: contour_color(&theme, field.sample(x, y)),
                radius: Fixed::ZERO,
                opa: 255,
            });
        }
    }
}

pub use raster_contour_render::view as raster_contour_view;

#[cfg(test)]
mod paint_contract_tests {
    use super::*;
    use crate::render::font::{Font, default_font_manager};
    use crate::render::renderer::{DrawRequest, RenderError, Renderer};
    use crate::test_support::tracked_allocations;
    use crate::ui::view::PaintCtx;
    use crate::ui::{Style, theme::WidgetState};

    struct Trace {
        calls: usize,
        fail: bool,
    }
    impl Renderer for Trace {
        fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            assert!(matches!(request.command, DrawCommand::Fill { .. }));
            self.calls += 1;
            if self.fail {
                Err(RenderError::BackendFailure)
            } else {
                Ok(())
            }
        }
        fn flush(&mut self) {}
    }

    fn prepared_manager(with_raster: bool) -> FontManager {
        let manager = default_font_manager();
        if with_raster {
            let font = Font::from_mirx(
                "contour",
                24,
                super::super::UI_FONT,
                &mirx::reader::PayloadLimits::HOST,
            )
            .unwrap();
            manager.add_static(UI.cache_key(), font);
        }
        manager.prefetch(UI.cache_key());
        manager
    }

    #[test]
    fn contour_view_refetches_resources_without_allocating_parameter_storage() {
        let mut app = App::headless(1, 1);
        let node = app.world.spawn_empty();
        app.world.insert(node, RasterContour::default());
        let view = raster_contour_view();
        assert_eq!(view.name(), "RasterContour");
        assert_eq!(
            view.component_filter(),
            Some(core::any::TypeId::of::<RasterContour>())
        );
        assert_eq!(view.priority(), Some(61));
        assert_eq!(
            view.invalidation(),
            crate::ui::view::ViewInvalidation::Visual
        );
        let render = view.render().unwrap();
        let rect = Rect::new(11, 17, 256, 256);
        let style = Style::default();
        for with_raster in [false, true, false, true] {
            app.world.insert_resource(prepared_manager(with_raster));
            for light in [false, true] {
                if light {
                    app.world.insert_resource(Theme::light());
                } else {
                    app.world.insert_resource(Theme::default());
                }
                for fail in [false, true] {
                    for _ in 0..2 {
                        let mut renderer = Trace { calls: 0, fail };
                        let mut paint = PaintCtx::new(
                            &mut renderer,
                            &style,
                            &rect,
                            &rect,
                            crate::types::Transform::IDENTITY,
                            None,
                            WidgetState::Enabled,
                        );
                        assert_eq!(
                            tracked_allocations(|| render(&app.world, node, &mut paint)),
                            0
                        );
                        let draws = with_raster;
                        assert_eq!(
                            paint.error(),
                            if fail && draws {
                                Some(RenderError::BackendFailure)
                            } else {
                                None
                            }
                        );
                        assert_eq!(renderer.calls != 0, draws);
                        if fail && draws {
                            assert_eq!(renderer.calls, 1);
                        }
                    }
                }
            }
        }
    }
}
