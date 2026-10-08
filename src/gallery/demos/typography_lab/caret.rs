use super::state::TypographyModel;
use super::style::{BLUE, BORDER, CYAN, GOLD, VIOLET};
use crate::ecs::World;
use crate::prelude::*;
use crate::render::command::{DrawCommand, LineCap, LineJoin, Paint};
use crate::render::font::ResolvedFontStack;
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::view::{PaintCtx, View};

#[crate::component]
pub(super) struct CaretOverlay {
    pub(super) model: model!(TypographyModel),
    pub(super) follows_probe: bool,
}

fn draw_line(paint: &mut PaintCtx<'_>, p1: Point, p2: Point, color: Color) {
    paint.draw(&DrawCommand::Line {
        p1,
        p2,
        transform: paint.transform(),
        color,
        width: Fixed::ONE,
        opa: 255,
    });
}

fn caret_overlay_render(world: &World, entity: Entity, paint: &mut PaintCtx<'_>) {
    let Some(overlay) = world.get::<CaretOverlay>(entity) else {
        return;
    };
    let target = entity;
    let (Some(style), Some(handle), Some(resource)) = (
        world.get::<crate::ui::Style>(target),
        world.get::<crate::text::TextLayoutHandle>(target).copied(),
        world.resource::<crate::text::layout::TextLayoutResource>(),
    ) else {
        return;
    };
    let face_limit = resource.borrow().limits().fallback_faces;
    let Ok(Some(fonts)) = ResolvedFontStack::resolve(
        world.resource::<crate::render::font::FontManager>(),
        &style.font_stack,
        style.font_size,
        face_limit,
    ) else {
        return;
    };
    let metrics = fonts.primary().metrics(fonts.primary().size);
    let cache = resource.borrow();
    let Some(layout) = cache.get(handle) else {
        return;
    };
    let default_theme = Theme::default();
    let theme = world.resource::<Theme>().unwrap_or(&default_theme);
    let border = theme.resolve(BORDER);
    let primary = theme.resolve(CYAN);
    let secondary = theme.resolve(BLUE);
    let tertiary = theme.resolve(VIOLET);
    let success = theme.resolve(GOLD);
    let rect = paint.rect();
    if let Some(text_path) = world.get::<TextPath>(target).copied() {
        let Some(geometry) = crate::text::PathTextGeometry::for_widget(world, target) else {
            return;
        };
        let probe = if overlay.follows_probe {
            crate::core::model::with_model_read_only(|| {
                crate::core::model::ModelHandle::read(&overlay.model, |model| model.path_probe)
            })
        } else {
            None
        };
        let probe_hit =
            probe.and_then(|probe| geometry.hit_test(probe, Fixed::from_int(16)).ok().flatten());
        let Some(paths) = world.resource::<crate::render::path::PathStore>() else {
            return;
        };
        if let Ok(path) = paths.get(text_path.path()) {
            let path_paint = Paint::Color(secondary.into());
            paint.draw(&DrawCommand::StrokePath {
                path,
                transform: paint
                    .transform()
                    .compose(&Transform::translate(rect.x, rect.y)),
                paint: &path_paint,
                width: Fixed::ONE,
                opa: 120,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                miter_limit: Fixed::from_int(4),
                dash: &[],
            });
        }
        let mut caret_storage = [crate::text::PathCaretGeometry::default(); 128];
        if let Ok(carets) = geometry.carets_into(&mut caret_storage) {
            for caret in carets {
                let [start, end] = caret.segment();
                draw_line(
                    paint,
                    start,
                    end,
                    if probe_hit.is_some_and(|hit| hit.index() == caret.index()) {
                        success
                    } else if caret.bidi_level() & 1 == 0 {
                        primary
                    } else {
                        tertiary
                    },
                );
            }
        }
        return;
    }
    for line in layout.lines() {
        let baseline = rect.y + crate::types::fixed::from_textflow(line.origin().y);
        draw_line(
            paint,
            Point {
                x: rect.x,
                y: baseline,
            },
            Point {
                x: rect.x + rect.w,
                y: baseline,
            },
            border,
        );
    }
    for caret in layout.carets() {
        let x = rect.x + crate::types::fixed::from_textflow(caret.position.x);
        if x < rect.x || x > rect.x + rect.w {
            continue;
        }
        let baseline = rect.y + crate::types::fixed::from_textflow(caret.position.y);
        draw_line(
            paint,
            Point {
                x,
                y: baseline - metrics.ascender,
            },
            Point {
                x,
                y: baseline - metrics.ascender + metrics.line_height,
            },
            if caret.bidi_level & 1 == 0 {
                primary
            } else {
                tertiary
            },
        );
    }
}

pub fn caret_overlay_view() -> View {
    fn observe(
        world: &World,
        entity: Entity,
        bindings: &mut crate::ui::view::ViewObservationBindings,
    ) {
        let overlay = world.get::<CaretOverlay>(entity).expect("caret overlay");
        bindings.watch(|invalidation| {
            overlay
                .model
                .__mirui_subscribe_path_probe(world, entity, invalidation)
        });
    }

    View::new("CaretOverlay", 61, caret_overlay_render)
        .with_filter::<CaretOverlay>()
        .with_observation(observe, 1)
}
