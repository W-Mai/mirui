use super::state::TypographyNodes;
use super::style::{BLUE, BORDER, CYAN, GOLD, VIOLET};
use crate::prelude::*;
use crate::render::command::{DrawCommand, LineCap, LineJoin, Paint};
use crate::render::font::ResolvedFontStack;
use crate::render::renderer::Renderer;
use crate::types::Transform;
use crate::ui::Theme;
use crate::ui::view::{View, ViewCtx};

#[crate::component]
#[derive(Default)]
pub(super) struct CaretOverlay {
    pub(super) target: Option<Entity>,
    pub(super) probe: Option<Point>,
}

fn draw_line(
    renderer: &mut dyn Renderer,
    ctx: &mut ViewCtx<'_>,
    p1: Point,
    p2: Point,
    color: Color,
) {
    ctx.draw(
        renderer,
        &DrawCommand::Line {
            p1,
            p2,
            transform: ctx.transform,
            color,
            width: Fixed::ONE,
            opa: 255,
        },
        ctx.clip,
    );
}

fn caret_overlay_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(overlay) = world.get::<CaretOverlay>(entity) else {
        return;
    };
    let Some(target) = overlay.target.filter(|target| world.is_alive(*target)) else {
        return;
    };
    let (Some(style), Some(handle), Some(resource)) = (
        world.get::<crate::ui::Style>(target),
        world.get::<crate::text::TextLayoutHandle>(target).copied(),
        world.resource::<crate::text::layout::TextLayoutResource>(),
    ) else {
        return;
    };
    let face_limit = resource.borrow().limits().fallback_faces;
    let Ok(Some(fonts)) =
        ResolvedFontStack::resolve(world, &style.font_stack, style.font_size, face_limit)
    else {
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
    if let Some(text_path) = world.get::<TextPath>(target).copied() {
        let Some(geometry) = crate::text::PathTextGeometry::for_widget(world, target) else {
            return;
        };
        let probe_hit = overlay
            .probe
            .and_then(|probe| geometry.hit_test(probe, Fixed::from_int(16)).ok().flatten());
        let Some(paths) = world.resource::<crate::render::path::PathStore>() else {
            return;
        };
        if let Ok(path) = paths.get(text_path.path()) {
            let paint = Paint::Color(secondary.into());
            ctx.draw(
                renderer,
                &DrawCommand::StrokePath {
                    path,
                    transform: ctx.transform.compose(&Transform::translate(rect.x, rect.y)),
                    paint: &paint,
                    width: Fixed::ONE,
                    opa: 120,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    miter_limit: Fixed::from_int(4),
                    dash: &[],
                },
                ctx.clip,
            );
        }
        let mut caret_storage = [crate::text::PathCaretGeometry::default(); 128];
        if let Ok(carets) = geometry.carets_into(&mut caret_storage) {
            for caret in carets {
                let [start, end] = caret.segment();
                draw_line(
                    renderer,
                    ctx,
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
            renderer,
            ctx,
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
            renderer,
            ctx,
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
    View::new("CaretOverlay", 61, caret_overlay_render)
}

pub(super) fn set_path_probe(world: &mut World, point: Point) {
    let Some(entity) = world
        .resource::<TypographyNodes>()
        .map(|nodes| nodes.path_overlay)
    else {
        return;
    };
    let Some(overlay) = world.get_mut::<CaretOverlay>(entity) else {
        return;
    };
    if overlay.probe != Some(point) {
        overlay.probe = Some(point);
        world.invalidate(entity);
    }
}

pub(super) fn bind_caret_overlay(
    world: &mut World,
    overlay_id: &'static str,
    target_id: &'static str,
) -> Entity {
    let overlay = world.find_by_id(overlay_id).expect("caret overlay id");
    let target = world.find_by_id(target_id).expect("caret target id");
    world
        .get_mut::<CaretOverlay>(overlay)
        .expect("caret overlay component")
        .target = Some(target);
    overlay
}
