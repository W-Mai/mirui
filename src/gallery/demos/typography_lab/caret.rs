use super::state::TypographyModel;
use super::style::{BLUE, BORDER, CYAN, GOLD, VIOLET};
use crate::prelude::*;
use crate::render::command::{DrawCommand, LineCap, LineJoin, Paint};
use crate::render::path::PathStore;
use crate::text::TextGeometry;
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::outputs::Outputs;
use crate::ui::view::PaintCtx;

#[crate::component]
#[depends_on(TextGeometry)]
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

#[crate::view(component = CaretOverlay, name = "CaretOverlay", priority = 61)]
pub(super) fn caret_overlay_render(
    component: component!(),
    model: model!(component.model, watch(path_probe)),
    outputs: res!(Outputs<TextGeometry>),
    theme: res!(Theme),
    paths: res!(PathStore),
    entity: entity!(),
    paint: paint!(),
) {
    let Some(geometry) = outputs.get(entity) else {
        return;
    };
    let layout = geometry.layout();
    let metrics = geometry.metrics();
    let border = theme.resolve(BORDER);
    let primary = theme.resolve(CYAN);
    let secondary = theme.resolve(BLUE);
    let tertiary = theme.resolve(VIOLET);
    let success = theme.resolve(GOLD);
    let rect = paint.rect();
    if let Some(text_path) = geometry.text_path() {
        let probe = if component.follows_probe {
            model.path_probe
        } else {
            None
        };
        let probe_hit =
            probe.and_then(|probe| geometry.hit_test(probe, Fixed::from_int(16)).ok().flatten());
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
                    Point::new(rect.x + start.x, rect.y + start.y),
                    Point::new(rect.x + end.x, rect.y + end.y),
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
    let content = geometry.content_rect();
    let content = Rect::new(rect.x + content.x, rect.y + content.y, content.w, content.h);
    for line in layout.lines() {
        let baseline = content.y + crate::types::fixed::from_textflow(line.origin().y);
        draw_line(
            paint,
            Point {
                x: content.x,
                y: baseline,
            },
            Point {
                x: content.x + content.w,
                y: baseline,
            },
            border,
        );
    }
    for caret in layout.carets() {
        let x = content.x + crate::types::fixed::from_textflow(caret.position.x);
        if x < content.x || x > content.x + content.w {
            continue;
        }
        let baseline = content.y + crate::types::fixed::from_textflow(caret.position.y);
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

pub(super) use caret_overlay_render::view as caret_overlay_view;
