use super::state::{OrbitModalSurface, OrbitPreview, OrbitSurface};
use super::style::{BACKGROUND, CYAN, INK, LINE, ORANGE, PANEL, SPACE, VIOLET};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::orbit::{OrbitModal, OrbitModel, OrbitPage, OrbitPoint};
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};

fn map_point(point: OrbitPoint) -> Point {
    Point {
        x: Fixed::from_int(154) + point.x.to_fixed() * Fixed::from_ratio(7, 10),
        y: Fixed::from_int(159) + point.y.to_fixed() * Fixed::from_ratio(7, 10),
    }
}

fn paint_shell(painter: &mut PlayPainter<'_, '_>) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 31), INK, Fixed::ZERO);
    painter.fill(
        Rect::new(0, 31, 480, 28),
        Color::rgb(226, 231, 228),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(0, 282, 480, 38), INK, Fixed::ZERO);
}

fn paint_map(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel, preview: &OrbitPreview) {
    painter.fill(Rect::new(11, 65, 287, 188), SPACE, Fixed::ZERO);
    painter.fill(Rect::new(307, 65, 162, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(307, 65, 162, 188), LINE, Fixed::ONE, Fixed::ZERO);
    let center = Point::new(154, 159);
    for radius in [27, 78, 113, 130] {
        painter.arc(
            center,
            Fixed::from_int(radius) * Fixed::from_ratio(7, 10),
            Fixed::ZERO,
            Fixed::from_int(360),
            if radius >= 113 {
                Color::rgba(244, 139, 54, 120)
            } else {
                Color::rgb(53, 75, 94)
            },
            Fixed::ONE,
        );
    }
    painter.circle(center, Fixed::from_int(17), Color::rgb(244, 173, 74));
    painter.circle(center, Fixed::from_int(11), Color::rgb(255, 221, 128));
    let mut previous = None;
    for index in 0..model.trail_len() {
        let Some(point) = model.trail(index) else {
            continue;
        };
        let point = map_point(point);
        if let Some(from) = previous {
            painter.line(from, point, CYAN, Fixed::ONE);
        }
        previous = Some(point);
    }
    if model.preview() {
        let mut previous = Some(map_point(model.body().position));
        for point in preview.points.iter().take(usize::from(preview.len)) {
            let point = map_point(*point);
            if let Some(from) = previous {
                painter.line(from, point, ORANGE, Fixed::from_ratio(1, 2));
            }
            previous = Some(point);
        }
    }
    let craft = map_point(model.body().position);
    painter.circle(craft, Fixed::from_int(5), CYAN);
    painter.circle(craft, Fixed::from_int(2), PANEL);
    if model.mission_index() == 2 {
        let beacon = map_point(OrbitPoint {
            x: crate::types::Fixed64::from_int(-148),
            y: crate::types::Fixed64::ZERO,
        });
        painter.arc(
            beacon,
            Fixed::from_int(16),
            Fixed::ZERO,
            Fixed::from_int(360),
            VIOLET,
            Fixed::from_int(2),
        );
    }
    painter.fill(Rect::new(318, 98, 139, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(318, 98, 139 * model.fuel().to_fixed().to_int() / 48, 4),
        CYAN,
        Fixed::ZERO,
    );
    painter.fill(Rect::new(318, 124, 139, 4), LINE, Fixed::ZERO);
    painter.fill(
        Rect::new(318, 124, 139 * model.heat().to_fixed().to_int() / 100, 4),
        ORANGE,
        Fixed::ZERO,
    );
}

fn paint_plan(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    painter.fill(Rect::new(11, 65, 292, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(11, 65, 292, 188), LINE, Fixed::ONE, Fixed::ZERO);
    painter.fill(Rect::new(313, 65, 156, 188), SPACE, Fixed::ZERO);
    painter.circle(
        Point::new(391, 144),
        Fixed::from_int(45),
        Color::rgb(31, 50, 67),
    );
    painter.arc(
        Point::new(391, 144),
        Fixed::from_int(45),
        Fixed::from_int(-90),
        Fixed::from_int(-90 + i32::from(model.angle_degrees())),
        VIOLET,
        Fixed::from_int(3),
    );
    for index in 0..3 {
        let y = 91 + index as i32 * 48;
        painter.fill(
            Rect::new(24, y, 262, 38),
            if model.queue(index).is_some() {
                Color::rgb(231, 233, 226)
            } else {
                BACKGROUND
            },
            Fixed::ZERO,
        );
        painter.border(Rect::new(24, y, 262, 38), LINE, Fixed::ONE, Fixed::ZERO);
        if model.queue(index).is_some() {
            painter.fill(Rect::new(24, y, 4, 38), ORANGE, Fixed::ZERO);
        }
    }
}

fn paint_chart(
    painter: &mut PlayPainter<'_, '_>,
    model: &OrbitModel,
    rect: Rect,
    radius: bool,
    color: Color,
    max: i32,
) {
    painter.fill(rect, PANEL, Fixed::ZERO);
    painter.border(rect, LINE, Fixed::ONE, Fixed::ZERO);
    let len = model.telemetry_len();
    if len < 2 {
        return;
    }
    let mut previous = None;
    for index in 0..len {
        let Some(sample) = model.telemetry(index) else {
            continue;
        };
        let value = if radius { sample.radius } else { sample.speed };
        let point = Point {
            x: rect.x + rect.w * Fixed::from_int(index as i32) / Fixed::from_int((len - 1) as i32),
            y: rect.y + rect.h - rect.h * value.to_fixed() / Fixed::from_int(max),
        };
        if let Some(from) = previous {
            painter.line(from, point, color, Fixed::from_ratio(3, 2));
        }
        previous = Some(point);
    }
}

fn paint_record(painter: &mut PlayPainter<'_, '_>, model: &OrbitModel) {
    paint_chart(
        painter,
        model,
        Rect::new(15, 88, 286, 74),
        true,
        ORANGE,
        160,
    );
    paint_chart(painter, model, Rect::new(15, 190, 286, 58), false, CYAN, 60);
    painter.fill(Rect::new(313, 65, 156, 188), PANEL, Fixed::ZERO);
    painter.border(Rect::new(313, 65, 156, 188), LINE, Fixed::ONE, Fixed::ZERO);
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let (Some(model), Some(preview)) = (
        world.resource::<OrbitModel>(),
        world.resource::<OrbitPreview>(),
    ) else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_shell(&mut painter);
    match model.page() {
        OrbitPage::Map => paint_map(&mut painter, model, preview),
        OrbitPage::Plan => paint_plan(&mut painter, model),
        OrbitPage::Record => paint_record(&mut painter, model),
    }
}

fn modal_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if world
        .resource::<OrbitModel>()
        .is_none_or(|model| model.modal() == OrbitModal::None)
    {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(12, 20, 30, 220),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(38, 69, 404, 190), PANEL, Fixed::ZERO);
    painter.border(
        Rect::new(38, 69, 404, 190),
        INK,
        Fixed::from_int(2),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(42, 73, 396, 32), INK, Fixed::ZERO);
}

pub(super) fn surface_view() -> View {
    View::new("OrbitSurface", 60, surface_render).with_filter::<OrbitSurface>()
}
pub(super) fn modal_view() -> View {
    View::new("OrbitModalSurface", 70, modal_render).with_filter::<OrbitModalSurface>()
}
