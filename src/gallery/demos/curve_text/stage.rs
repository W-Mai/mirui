use super::geometry::CurvePaths;
use super::state::CurveModel;
use super::style::{BORDER, LANE_COLORS, PANEL};
use crate::prelude::*;
use crate::render::command::{DrawCommand, LineCap, LineJoin, Paint};
use crate::render::path::PathStore;
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::view::PaintCtx;

#[crate::component]
pub(super) struct CurveStage {
    pub(super) model: model!(CurveModel),
}

fn fill(paint: &mut PaintCtx<'_>, area: Rect, color: Color, radius: Fixed, opacity: u8) {
    paint.draw(&DrawCommand::Fill {
        area,
        transform: paint.transform(),
        quad: None,
        color,
        radius,
        opa: opacity,
    });
}

#[crate::view(
    component = CurveStage,
    name = "CurveStageBackdrop",
    priority = 60,
    invalidation = Visual
)]
pub(super) fn curve_stage_background_render(
    model: model!(component.model, watch(phase)),
    paint: paint!(),
    theme: res!(Theme),
) {
    let panel = theme.resolve(PANEL);
    let border = theme.resolve(BORDER);
    let lane_colors = LANE_COLORS.map(|token| theme.resolve(token));
    paint.set_background_handled(true);
    let rect = paint.rect();
    fill(paint, rect, panel, Fixed::from_int(18), 255);
    for index in 0..12 {
        let x = rect.x + rect.w * Fixed::from_ratio(index, 11);
        fill(
            paint,
            Rect::new(x, rect.y, Fixed::ONE, rect.h),
            border,
            Fixed::ZERO,
            if index % 3 == 0 { 48 } else { 22 },
        );
    }
    for index in 1..4 {
        let y = rect.y + rect.h * Fixed::from_ratio(index, 4);
        fill(
            paint,
            Rect::new(rect.x, y, rect.w, Fixed::ONE),
            border,
            Fixed::ZERO,
            28,
        );
    }

    let phase = model.phase();
    for index in 0..5 {
        let angle = phase + Fixed::from_int(index * 72);
        let x = rect.x + rect.w / 2 + Fixed::cos_deg(angle) * Fixed::from_int(300);
        let y = rect.y + rect.h / 2 + Fixed::sin_deg(angle * 2) * Fixed::from_int(126);
        let radius = Fixed::from_int(10 + index % 3 * 4);
        fill(
            paint,
            Rect::new(x - radius, y - radius, radius * 2, radius * 2),
            lane_colors[index as usize % lane_colors.len()],
            radius,
            32,
        );
    }
}

#[crate::view(component = CurveStage, name = "CurvePaths", priority = 61)]
fn curve_paths_render(
    paths: res!(CurvePaths),
    store: res!(PathStore),
    theme: res!(Theme),
    paint: paint!(),
) {
    let lane_colors = LANE_COLORS.map(|token| theme.resolve(token));
    let rect = paint.rect();
    let transform = paint
        .transform()
        .compose(&Transform::translate(rect.x, rect.y));
    for (index, id) in paths.ids.into_iter().enumerate() {
        let Ok(path) = store.get(id) else {
            continue;
        };
        let stroke_paint = Paint::Color(lane_colors[index].into());
        paint.draw(&DrawCommand::StrokePath {
            path,
            transform,
            paint: &stroke_paint,
            width: Fixed::from_int(5),
            opa: 16,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            miter_limit: Fixed::from_int(4),
            dash: &[],
        });
        paint.draw(&DrawCommand::StrokePath {
            path,
            transform,
            paint: &stroke_paint,
            width: Fixed::ONE,
            opa: 150,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            miter_limit: Fixed::from_int(4),
            dash: &[],
        });
    }
}

pub(super) use curve_paths_render::view as curve_paths_view;

#[cfg(test)]
mod paint_contract_tests {
    use super::*;
    use crate::render::path::PathCmd;
    use crate::render::renderer::{DrawRequest, RenderError, Renderer};
    use crate::test_support::tracked_allocations;
    use crate::ui::theme::WidgetState;

    struct Trace {
        paths: [Option<*const PathCmd>; 3],
        colors: [Color; 3],
        transform: Transform,
        clip: Rect,
        calls: usize,
        fail: bool,
    }

    impl Renderer for Trace {
        fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            self.calls += 1;
            if self.fail {
                return Err(RenderError::BackendFailure);
            }
            let DrawCommand::StrokePath {
                path,
                transform,
                paint: Paint::Color(color),
                ..
            } = request.command
            else {
                panic!("expected a path stroke");
            };
            let lane = self
                .paths
                .iter()
                .position(|ptr| *ptr == Some(path.commands().as_ptr()))
                .unwrap();
            assert_eq!(Color::from(*color), self.colors[lane]);
            assert_eq!(*transform, self.transform);
            assert_eq!(request.clip, self.clip);
            assert_eq!(request.projective, crate::types::Transform3D::IDENTITY);
            Ok(())
        }
        fn flush(&mut self) {}
    }

    fn assert_resource_contract(render: crate::ui::view::ViewRender) {
        let mut app = App::headless(1, 1);
        let model = app.add_model(CurveModel::default()).finish();
        let node = app.world.spawn_empty();
        app.world.insert(node, CurveStage { model });
        let rect = Rect::new(11, 17, 640, 360);
        let style = Style::default();
        for step in 0..4 {
            let mut store = PathStore::new(3).unwrap();
            let ids = core::array::from_fn(|lane| {
                store
                    .insert(super::super::geometry::make_lane(lane))
                    .unwrap()
            });
            if step == 2 {
                store.remove(ids[0]).unwrap();
            }
            let paths = ids.map(|id| store.get(id).ok().map(|path| path.commands().as_ptr()));
            app.world.insert_resource(CurvePaths { ids });
            app.world.insert_resource(store);
            if step >= 1 {
                app.world.insert_resource(Theme::light());
            } else {
                app.world.insert_resource(Theme::default());
            }
            let theme = app.world.resource::<Theme>().unwrap();
            let colors = LANE_COLORS.map(|token| theme.resolve(token));
            let expected = 2 * paths.iter().flatten().count();
            for transform in [
                Transform::IDENTITY,
                Transform {
                    m00: Fixed::from_ratio(5, 4),
                    m11: Fixed::from_ratio(3, 4),
                    tx: Fixed::from_int(7),
                    ty: Fixed::from_int(13),
                    ..Transform::IDENTITY
                },
            ] {
                for fail in [false, true] {
                    for _ in 0..2 {
                        let mut renderer = Trace {
                            paths,
                            colors,
                            transform: transform.compose(&Transform::translate(rect.x, rect.y)),
                            clip: rect,
                            calls: 0,
                            fail,
                        };
                        let mut paint = PaintCtx::new(
                            &mut renderer,
                            &style,
                            &rect,
                            &rect,
                            transform,
                            None,
                            WidgetState::Enabled,
                        );
                        assert_eq!(
                            tracked_allocations(|| render(&app.world, node, &mut paint)),
                            0
                        );
                        assert_eq!(
                            paint.error(),
                            if fail && expected != 0 {
                                Some(RenderError::BackendFailure)
                            } else {
                                None
                            }
                        );
                        assert_eq!(
                            renderer.calls,
                            if fail {
                                usize::from(expected != 0)
                            } else {
                                expected
                            }
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn curve_paths_view_borrows_current_resources_and_retained_paths_without_allocating() {
        let view = curve_paths_view();
        assert_eq!(view.name(), "CurvePaths");
        assert_eq!(view.priority(), Some(61));
        assert_eq!(
            view.component_filter(),
            Some(core::any::TypeId::of::<CurveStage>())
        );
        assert_eq!(
            view.invalidation(),
            crate::ui::view::ViewInvalidation::Visual
        );
        assert_resource_contract(view.render().unwrap());
    }
}
