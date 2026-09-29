use super::state::{PostModalSurface, PostSurface};
use super::style::{
    ACCENT, BACKGROUND, BOARD, HEADER, PANEL, STATION_COLORS, TEXT, TRACK, TRACK_INACTIVE,
};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::post::{PostModal, PostModel, PostParcel};
use crate::prelude::{Color, Fixed, Point, Rect};
use crate::render::renderer::Renderer;
use crate::ui::view::ViewCtx;

fn paint_track(painter: &mut PlayPainter<'_, '_>, from: Point, to: Point, active: bool) {
    painter.line(from, to, Color::rgb(12, 22, 26), Fixed::from_int(12));
    painter.line(
        from,
        to,
        if active { TRACK } else { TRACK_INACTIVE },
        Fixed::from_int(8),
    );
    painter.line(
        from,
        to,
        if active {
            Color::rgb(192, 204, 192)
        } else {
            Color::rgb(86, 105, 101)
        },
        Fixed::ONE,
    );
}

fn paint_destination_shape(
    painter: &mut PlayPainter<'_, '_>,
    target: u8,
    center: Point,
    size: i32,
) {
    let color = STATION_COLORS[usize::from(target)];
    match target {
        0 => painter.circle(center, Fixed::from_int(size), color),
        1 => painter.fill(
            Rect::new(
                center.x.to_int() - size,
                center.y.to_int() - size,
                size * 2,
                size * 2,
            ),
            color,
            Fixed::from_int(2),
        ),
        _ => {
            let top = Point::new(center.x, center.y - Fixed::from_int(size + 1));
            let left = Point::new(
                center.x - Fixed::from_int(size + 1),
                center.y + Fixed::from_int(size),
            );
            let right = Point::new(
                center.x + Fixed::from_int(size + 1),
                center.y + Fixed::from_int(size),
            );
            painter.line(top, left, color, Fixed::from_int(2));
            painter.line(left, right, color, Fixed::from_int(2));
            painter.line(right, top, color, Fixed::from_int(2));
        }
    }
}

fn paint_letter(painter: &mut PlayPainter<'_, '_>, target: u8, center: Point) {
    let color = STATION_COLORS[usize::from(target)];
    let x = center.x;
    let y = center.y;
    match target {
        0 => {
            painter.line(
                Point::new(x - Fixed::from_int(3), y + Fixed::from_int(3)),
                Point::new(x, y - Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x, y - Fixed::from_int(3)),
                Point::new(x + Fixed::from_int(3), y + Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x - Fixed::from_int(2), y + Fixed::ONE),
                Point::new(x + Fixed::from_int(2), y + Fixed::ONE),
                color,
                Fixed::ONE,
            );
        }
        1 => {
            painter.line(
                Point::new(x - Fixed::from_int(2), y - Fixed::from_int(3)),
                Point::new(x - Fixed::from_int(2), y + Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
            for offset in [-2, 1] {
                painter.line(
                    Point::new(x - Fixed::from_int(2), y + Fixed::from_int(offset)),
                    Point::new(x + Fixed::from_int(2), y + Fixed::from_int(offset)),
                    color,
                    Fixed::ONE,
                );
            }
            painter.line(
                Point::new(x + Fixed::from_int(2), y - Fixed::from_int(2)),
                Point::new(x + Fixed::from_int(2), y + Fixed::from_int(2)),
                color,
                Fixed::ONE,
            );
        }
        _ => {
            painter.line(
                Point::new(x + Fixed::from_int(2), y - Fixed::from_int(3)),
                Point::new(x - Fixed::from_int(2), y - Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x - Fixed::from_int(2), y - Fixed::from_int(3)),
                Point::new(x - Fixed::from_int(2), y + Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
            painter.line(
                Point::new(x - Fixed::from_int(2), y + Fixed::from_int(3)),
                Point::new(x + Fixed::from_int(2), y + Fixed::from_int(3)),
                color,
                Fixed::ONE,
            );
        }
    }
}

fn paint_parcel(painter: &mut PlayPainter<'_, '_>, parcel: PostParcel) {
    let center = parcel.position();
    let color = STATION_COLORS[usize::from(parcel.target())];
    painter.fill(
        Rect {
            x: center.x - Fixed::from_int(9),
            y: center.y - Fixed::from_int(9),
            w: Fixed::from_int(18),
            h: Fixed::from_int(18),
        },
        Color::rgb(23, 35, 38),
        Fixed::from_int(4),
    );
    painter.border(
        Rect {
            x: center.x - Fixed::from_int(9),
            y: center.y - Fixed::from_int(9),
            w: Fixed::from_int(18),
            h: Fixed::from_int(18),
        },
        color,
        Fixed::from_ratio(3, 2),
        Fixed::from_int(4),
    );
    paint_destination_shape(painter, parcel.target(), center, 3);
    paint_letter(
        painter,
        parcel.target(),
        Point::new(center.x, center.y - Fixed::from_int(15)),
    );
}

fn paint_post_surface(painter: &mut PlayPainter<'_, '_>, model: &PostModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 35), HEADER, Fixed::ZERO);
    painter.line(
        Point::new(12, 34),
        Point::new(468, 34),
        Color::rgb(48, 65, 66),
        Fixed::ONE,
    );
    for (from, to) in [
        (Point::new(20, 8), Point::new(26, 14)),
        (Point::new(26, 14), Point::new(20, 20)),
        (Point::new(20, 20), Point::new(14, 14)),
        (Point::new(14, 14), Point::new(20, 8)),
    ] {
        painter.line(from, to, ACCENT, Fixed::from_int(3));
    }
    painter.fill(Rect::new(12, 62, 456, 207), BOARD, Fixed::from_int(9));
    painter.border(
        Rect::new(12, 62, 456, 207),
        Color::rgb(53, 73, 76),
        Fixed::ONE,
        Fixed::from_int(9),
    );
    for x in (26..460).step_by(22) {
        for y in (77..260).step_by(22) {
            painter.circle(
                Point::new(x, y),
                Fixed::from_ratio(2, 3),
                Color::rgb(51, 68, 74),
            );
        }
    }
    paint_track(painter, Point::new(32, 157), Point::new(151, 157), true);
    paint_track(
        painter,
        Point::new(151, 157),
        Point::new(265, 157),
        model.switch(0) == 1,
    );
    paint_track(
        painter,
        Point::new(151, 157),
        Point::new(195, 89),
        model.switch(0) == 0,
    );
    paint_track(
        painter,
        Point::new(195, 89),
        Point::new(400, 89),
        model.switch(0) == 0,
    );
    paint_track(
        painter,
        Point::new(265, 157),
        Point::new(400, 157),
        model.switch(1) == 0,
    );
    paint_track(
        painter,
        Point::new(265, 157),
        Point::new(306, 225),
        model.switch(1) == 1,
    );
    paint_track(
        painter,
        Point::new(306, 225),
        Point::new(400, 225),
        model.switch(1) == 1,
    );
    painter.fill(
        Rect::new(17, 135, 32, 43),
        Color::rgb(52, 68, 73),
        Fixed::from_int(6),
    );
    painter.border(
        Rect::new(17, 135, 32, 43),
        Color::rgb(108, 129, 128),
        Fixed::ONE,
        Fixed::from_int(6),
    );
    painter.line(
        Point::new(26, 161),
        Point::new(39, 161),
        TEXT,
        Fixed::from_ratio(3, 2),
    );
    painter.line(
        Point::new(35, 157),
        Point::new(39, 161),
        TEXT,
        Fixed::from_ratio(3, 2),
    );
    painter.line(
        Point::new(39, 161),
        Point::new(35, 165),
        TEXT,
        Fixed::from_ratio(3, 2),
    );
    for station in 0..3 {
        let y = [89, 157, 225][station];
        let color = STATION_COLORS[station];
        painter.fill(
            Rect::new(365, y - 19, 89, 38),
            if model.station_flashing(station) {
                Color::rgb(60, 82, 80)
            } else {
                PANEL
            },
            Fixed::from_int(6),
        );
        painter.border(
            Rect::new(365, y - 19, 89, 38),
            color,
            Fixed::ONE,
            Fixed::from_int(6),
        );
        paint_destination_shape(painter, station as u8, Point::new(380, y), 5);
    }
    for switch in 0..2 {
        let x = [151, 265][switch];
        painter.circle(
            Point::new(x, 157),
            Fixed::from_int(18),
            Color::rgb(38, 56, 61),
        );
        painter.border(
            Rect::new(x - 18, 139, 36, 36),
            ACCENT,
            Fixed::ONE,
            Fixed::from_int(18),
        );
        painter.circle(
            Point::new(x, 157),
            Fixed::from_int(12),
            Color::rgb(52, 73, 81),
        );
        let branch = model.switch(switch);
        let end = if switch == 0 && branch == 0 {
            Point::new(x + 5, 149)
        } else if switch == 1 && branch == 1 {
            Point::new(x + 5, 165)
        } else {
            Point::new(x + 8, 157)
        };
        painter.line(
            Point::new(x - 8, 157),
            Point::new(x - 1, 157),
            TEXT,
            Fixed::from_int(2),
        );
        painter.line(Point::new(x - 1, 157), end, TEXT, Fixed::from_int(2));
    }
    for index in 0..usize::from(model.active_len()) {
        if let Some(parcel) = model.active(index) {
            paint_parcel(painter, parcel);
        }
    }
    if !model.started()
        && let Some(target) = model.queued(0)
    {
        painter.fill(
            Rect::new(62, 146, 22, 22),
            Color::rgb(34, 55, 55),
            Fixed::from_int(4),
        );
        paint_destination_shape(painter, target, Point::new(73, 157), 5);
    }
    for index in 0..5 {
        if model.queued(index).is_some() {
            painter.fill(
                Rect::new(46 + index as i32 * 25, 239, 19, 18),
                Color::rgb(39, 58, 60),
                Fixed::from_int(4),
            );
        }
    }
    painter.fill(
        Rect::new(0, 282, 480, 38),
        Color::rgb(16, 32, 25),
        Fixed::ZERO,
    );
}

#[crate::view(
    component = PostSurface,
    read(model),
    watch(model.visual_revision()),
    name = "PostSurface",
    priority = 60
)]
pub(super) fn surface_render(
    renderer: &mut dyn Renderer,
    model: &PostModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_post_surface(&mut painter, model);
}

#[crate::view(
    component = PostModalSurface,
    read(model),
    watch(model.visual_revision()),
    name = "PostModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    renderer: &mut dyn Renderer,
    model: &PostModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if model.modal() == PostModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(7, 13, 15, 226),
        Fixed::ZERO,
    );
    painter.fill(Rect::new(17, 65, 446, 206), HEADER, Fixed::from_int(10));
    painter.border(
        Rect::new(17, 65, 446, 206),
        ACCENT,
        Fixed::ONE,
        Fixed::from_int(10),
    );
    if model.modal() == PostModal::Summary {
        painter.line(
            Point::new(239, 113),
            Point::new(239, 192),
            Color::rgb(54, 72, 74),
            Fixed::ONE,
        );
    } else if model.modal() == PostModal::Reset {
        painter.fill(Rect::new(49, 124, 96, 96), BOARD, Fixed::from_int(48));
        painter.circle(Point::new(97, 172), Fixed::from_int(8), ACCENT);
    }
}
