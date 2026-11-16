use super::state::{PostModalSurface, PostSurface};
use super::style::{
    ACCENT, BACKGROUND, BOARD, HEADER, MISS, PANEL, STAR_SLOT, STATION_COLORS, TEXT, TRACK,
    TRACK_INACTIVE,
};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::post::{
    A_BEND_X, C_BEND_X, INLET_X, MAIN_Y, PostModal, PostModel, PostParcel, STATION_X, STATION_Y,
    SWITCH_X,
};
use crate::prelude::{Color, Fixed, Point, Rect};
use crate::render::path::Path;
use crate::types::Transform;

/// The board under the rails, below the header and its controls.
const BOARD_RECT: (i32, i32, i32, i32) = (6, 36, 468, 278);
/// A station's box: its left edge, width and half height around its row.
pub(super) const STATION_LEFT: i32 = 362;
const STATION_WIDTH: i32 = 96;
const STATION_HALF: i32 = 22;
/// The queue slots along the foot of the board.
const QUEUE_LEFT: i32 = 50;
const QUEUE_PITCH: i32 = 24;
pub(super) const QUEUE_TOP: i32 = 287;

/// The left edge of queue slot `index`.
pub(super) const fn queue_slot_left(index: usize) -> i32 {
    QUEUE_LEFT + index as i32 * QUEUE_PITCH
}

/// A five-pointed star of radius 10, centred on the origin.
static STAR: Path = crate::path!(
    "M 0 -10 L 2.351 -3.236 L 9.511 -3.09 L 3.804 1.236 L 5.878 8.09 L 0 4 L -5.878 8.09 L -3.804 1.236 L -9.511 -3.09 L -2.351 -3.236 Z"
);

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

/// Station letters as 5 x 7 pixel glyphs, one row per byte, high bit on the left.
const LETTERS: [[u8; 7]; 3] = [
    [
        0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
    ],
    [
        0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
    ],
    [
        0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
    ],
];

/// Paints the letter of `target` centred on `center`, snapped to whole pixels
/// so it stays as sharp as the text around it.
fn paint_letter(painter: &mut PlayPainter<'_, '_>, target: u8, center: Point) {
    let color = STATION_COLORS[usize::from(target)];
    let left = center.x.round().to_int() - 2;
    let top = center.y.round().to_int() - 3;
    let glyph = LETTERS[usize::from(target).min(LETTERS.len() - 1)];
    for (row, bits) in glyph.iter().enumerate() {
        for column in 0..5 {
            if bits & (0b10000 >> column) != 0 {
                painter.fill(
                    Rect::new(left + column, top + row as i32, 1, 1),
                    color,
                    Fixed::ZERO,
                );
            }
        }
    }
}

fn paint_parcel(painter: &mut PlayPainter<'_, '_>, parcel: PostParcel) {
    let center = parcel.position();
    let color = STATION_COLORS[usize::from(parcel.target())];
    let frame = Rect {
        x: center.x - Fixed::from_int(9),
        y: center.y - Fixed::from_int(9),
        w: Fixed::from_int(18),
        h: Fixed::from_int(18),
    };
    painter.fill(frame, Color::rgb(23, 35, 38), Fixed::from_int(4));
    painter.border(frame, color, Fixed::from_ratio(3, 2), Fixed::from_int(4));
    paint_destination_shape(painter, parcel.target(), center, 3);
    paint_letter(
        painter,
        parcel.target(),
        Point::new(center.x, center.y - Fixed::from_int(15)),
    );
}

/// The IN chute, painted over the parcels that have not left it yet.
fn paint_inlet(painter: &mut PlayPainter<'_, '_>) {
    let arrow_y = MAIN_Y + 4;
    painter.fill(
        Rect::new(17, MAIN_Y - 22, 32, 43),
        Color::rgb(52, 68, 73),
        Fixed::from_int(6),
    );
    painter.border(
        Rect::new(17, MAIN_Y - 22, 32, 43),
        Color::rgb(108, 129, 128),
        Fixed::ONE,
        Fixed::from_int(6),
    );
    painter.line(
        Point::new(26, arrow_y),
        Point::new(39, arrow_y),
        TEXT,
        Fixed::from_ratio(3, 2),
    );
    painter.line(
        Point::new(35, arrow_y - 4),
        Point::new(39, arrow_y),
        TEXT,
        Fixed::from_ratio(3, 2),
    );
    painter.line(
        Point::new(39, arrow_y),
        Point::new(35, arrow_y + 4),
        TEXT,
        Fixed::from_ratio(3, 2),
    );
}

/// A circle outline of `radius` around `center`.
fn ring(center: Point, radius: Fixed) -> Rect {
    Rect {
        x: center.x - radius,
        y: center.y - radius,
        w: radius * 2,
        h: radius * 2,
    }
}

/// A station box with its shape, and the ring of the last parcel it took:
/// in its own colour when right, in red with a cross when wrong.
fn paint_station(painter: &mut PlayPainter<'_, '_>, model: &PostModel, station: usize) {
    let y = STATION_Y[station];
    let color = STATION_COLORS[station];
    let frame = Rect::new(
        STATION_LEFT,
        y - STATION_HALF,
        STATION_WIDTH,
        STATION_HALF * 2,
    );
    painter.fill(
        frame,
        if model.station_flashing(station) {
            Color::rgb(60, 82, 80)
        } else {
            PANEL
        },
        Fixed::from_int(6),
    );
    painter.border(frame, color, Fixed::ONE, Fixed::from_int(6));
    let marker = Point::new(STATION_LEFT + 15, y);
    paint_destination_shape(painter, station as u8, marker, 6);
    let Some((progress, correct)) = model.motion().ring(station) else {
        return;
    };
    let alpha = (Fixed::from_int(230) * (Fixed::ONE - progress))
        .to_int()
        .clamp(0, 255) as u8;
    let tint = Color {
        a: alpha,
        ..if correct { color } else { MISS }
    };
    let radius = Fixed::from_int(9) + Fixed::from_int(16) * progress;
    painter.border(ring(marker, radius), tint, Fixed::from_int(2), radius);
    if !correct {
        let arm = Fixed::from_int(7);
        painter.line(
            Point::new(marker.x - arm, marker.y - arm),
            Point::new(marker.x + arm, marker.y + arm),
            tint,
            Fixed::from_int(2),
        );
        painter.line(
            Point::new(marker.x - arm, marker.y + arm),
            Point::new(marker.x + arm, marker.y - arm),
            tint,
            Fixed::from_int(2),
        );
    }
}

/// A switch disc with its lever pointing `angle` degrees off the main rail.
fn paint_switch(painter: &mut PlayPainter<'_, '_>, x: i32, angle: Fixed) {
    let center = Point::new(x, MAIN_Y);
    painter.circle(center, Fixed::from_int(18), Color::rgb(38, 56, 61));
    painter.border(
        Rect::new(x - 18, MAIN_Y - 18, 36, 36),
        ACCENT,
        Fixed::ONE,
        Fixed::from_int(18),
    );
    painter.circle(center, Fixed::from_int(12), Color::rgb(52, 73, 81));
    let pivot = Point::new(x - 1, MAIN_Y);
    let reach = Transform::rotate_deg(angle).apply_point(Point::new(10, 0));
    painter.line(Point::new(x - 8, MAIN_Y), pivot, TEXT, Fixed::from_int(2));
    painter.line(
        pivot,
        Point {
            x: pivot.x + reach.x,
            y: pivot.y + reach.y,
        },
        TEXT,
        Fixed::from_int(2),
    );
}

fn paint_post_surface(painter: &mut PlayPainter<'_, '_>, model: &PostModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 32), HEADER, Fixed::ZERO);
    painter.line(
        Point::new(12, 32),
        Point::new(468, 32),
        Color::rgb(48, 65, 66),
        Fixed::ONE,
    );
    for (from, to) in [
        (Point::new(20, 10), Point::new(26, 16)),
        (Point::new(26, 16), Point::new(20, 22)),
        (Point::new(20, 22), Point::new(14, 16)),
        (Point::new(14, 16), Point::new(20, 10)),
    ] {
        painter.line(from, to, ACCENT, Fixed::from_int(3));
    }
    let (left, top, width, height) = BOARD_RECT;
    painter.fill(
        Rect::new(left, top, width, height),
        BOARD,
        Fixed::from_int(9),
    );
    painter.border(
        Rect::new(left, top, width, height),
        Color::rgb(53, 73, 76),
        Fixed::ONE,
        Fixed::from_int(9),
    );
    for x in (20..466).step_by(22) {
        for y in (50..306).step_by(22) {
            painter.circle(
                Point::new(x, y),
                Fixed::from_ratio(2, 3),
                Color::rgb(51, 68, 74),
            );
        }
    }
    let [a_y, _, c_y] = STATION_Y;
    let to_a = model.switch(0) == 0;
    let to_c = model.switch(1) == 1;
    paint_track(
        painter,
        Point::new(INLET_X, MAIN_Y),
        Point::new(SWITCH_X[0], MAIN_Y),
        true,
    );
    paint_track(
        painter,
        Point::new(SWITCH_X[0], MAIN_Y),
        Point::new(SWITCH_X[1], MAIN_Y),
        !to_a,
    );
    paint_track(
        painter,
        Point::new(SWITCH_X[0], MAIN_Y),
        Point::new(A_BEND_X, a_y),
        to_a,
    );
    paint_track(
        painter,
        Point::new(A_BEND_X, a_y),
        Point::new(STATION_X, a_y),
        to_a,
    );
    paint_track(
        painter,
        Point::new(SWITCH_X[1], MAIN_Y),
        Point::new(STATION_X, MAIN_Y),
        !to_c,
    );
    paint_track(
        painter,
        Point::new(SWITCH_X[1], MAIN_Y),
        Point::new(C_BEND_X, c_y),
        to_c,
    );
    paint_track(
        painter,
        Point::new(C_BEND_X, c_y),
        Point::new(STATION_X, c_y),
        to_c,
    );
    for station in 0..3 {
        paint_station(painter, model, station);
    }
    for (switch, x) in SWITCH_X.into_iter().enumerate() {
        paint_switch(painter, x, model.motion().lever(switch));
    }
    for index in 0..usize::from(model.active_len()) {
        if let Some(parcel) = model.active(index) {
            paint_parcel(painter, parcel);
        }
    }
    paint_inlet(painter);
    if !model.started()
        && let Some(target) = model.queued(0)
    {
        painter.fill(
            Rect::new(62, MAIN_Y - 11, 22, 22),
            Color::rgb(34, 55, 55),
            Fixed::from_int(4),
        );
        paint_destination_shape(painter, target, Point::new(73, MAIN_Y), 5);
    }
    for index in 0..5 {
        if model.queued(index).is_some() {
            painter.fill(
                Rect::new(queue_slot_left(index), QUEUE_TOP, 19, 18),
                Color::rgb(39, 58, 60),
                Fixed::from_int(4),
            );
        }
    }
}

#[crate::view(component = PostSurface, name = "PostSurface", priority = 60)]
pub(super) fn surface_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    paint_post_surface(&mut painter, model);
}

/// The panel a dialog sits on: the shift list needs the most room.
pub(super) fn modal_frame(modal: PostModal) -> Rect {
    if modal == PostModal::Shifts {
        Rect::new(17, 30, 446, 268)
    } else {
        Rect::new(17, 65, 446, 206)
    }
}

fn paint_star(painter: &mut PlayPainter<'_, '_>, center: Point, radius: Fixed, color: Color) {
    let size = radius / Fixed::from_int(10);
    painter
        .transformed(
            Transform::translate(center.x, center.y).compose(&Transform::scale(size, size)),
        )
        .fill_path(&STAR, color);
}

#[crate::view(
    component = PostModalSurface,
    name = "PostModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    if model.modal() == PostModal::None {
        return;
    }
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(7, 13, 15, 226),
        Fixed::ZERO,
    );
    let frame = modal_frame(model.modal());
    painter.fill(frame, HEADER, Fixed::from_int(10));
    painter.border(frame, ACCENT, Fixed::ONE, Fixed::from_int(10));
    if model.modal() == PostModal::Summary {
        painter.line(
            Point::new(239, 113),
            Point::new(239, 200),
            Color::rgb(54, 72, 74),
            Fixed::ONE,
        );
        let center = Point::new(420, 136);
        let radius = Fixed::from_int(18);
        paint_star(&mut painter, center, radius, STAR_SLOT);
        let scale = model.motion().star();
        if scale > Fixed::ZERO {
            paint_star(&mut painter, center, radius * scale, STATION_COLORS[0]);
        }
    }
}
