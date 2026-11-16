use super::state::{PixelModalSurface, PixelPreview, PixelSurface};
use super::style::{ACTIVE, BACKGROUND, BOARD, PALETTE, PANEL};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::pixel::{
    FRAME_COUNT, GRID_HEIGHT, GRID_WIDTH, PixelFrames, PixelModal, PixelModel,
};
use crate::prelude::{Color, Fixed, Point, Rect};

/// Top-left corner of the drawing grid and the size of one of its cells.
pub(super) const GRID_LEFT: i32 = 16;
pub(super) const GRID_TOP: i32 = 16;
pub(super) const CELL: i32 = 24;
/// The control column beside the canvas: its left edge and width. The
/// controls inside it are placed relative to that edge.
pub(super) const COLUMN_LEFT: i32 = 316;
pub(super) const COLUMN_WIDTH: i32 = 156;
/// The looping preview panel in the column: left, top, width and height.
pub(super) const PREVIEW: (i32, i32, i32, i32) = (0, 26, COLUMN_WIDTH, 72);
/// The four frame thumbnails under the preview.
pub(super) const FRAMES_TOP: i32 = 102;
pub(super) const THUMB_WIDTH: i32 = 36;
pub(super) const THUMB_HEIGHT: i32 = 38;
pub(super) const THUMB_STEP: i32 = 40;
const BOARD_RECT: Rect = Rect {
    x: Fixed::from_int(8),
    y: Fixed::from_int(8),
    w: Fixed::from_int(304),
    h: Fixed::from_int(304),
};

fn checker(x: u8, y: u8) -> Color {
    if (x + y) & 1 == 0 {
        Color::rgb(35, 40, 51)
    } else {
        Color::rgb(40, 44, 53)
    }
}

fn paint_sprite(
    painter: &mut PlayPainter<'_, '_>,
    frames: &PixelFrames,
    frame: u8,
    origin: Point,
    cell: Fixed,
    checker: bool,
) {
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let color = frames.get(frame, x, y);
            if color == 0 && !checker {
                continue;
            }
            let fill = if color == 0 {
                if (x + y) & 1 == 0 {
                    Color::rgb(32, 37, 44)
                } else {
                    Color::rgb(37, 41, 50)
                }
            } else {
                PALETTE[color as usize]
            };
            painter.fill(
                Rect {
                    x: origin.x + Fixed::from_int(i32::from(x)) * cell,
                    y: origin.y + Fixed::from_int(i32::from(y)) * cell,
                    w: cell,
                    h: cell,
                },
                fill,
                Fixed::ZERO,
            );
        }
    }
}

/// One canvas cell. With onion skin on, an empty cell shows the frame
/// before it as a faded fill and the frame after it as a small outline.
fn paint_cell(painter: &mut PlayPainter<'_, '_>, model: &PixelModel, frame: u8, x: u8, y: u8) {
    let left = GRID_LEFT + i32::from(x) * CELL;
    let top = GRID_TOP + i32::from(y) * CELL;
    let area = Rect::new(left, top, CELL - 1, CELL - 1);
    let frames = model.frames();
    let color = frames.get(frame, x, y);
    if color != 0 {
        painter.fill(area, PALETTE[color as usize], Fixed::ONE);
        return;
    }
    painter.fill(area, checker(x, y), Fixed::ONE);
    if model.playing() || !model.onion() {
        return;
    }
    let before = frames.get((frame + FRAME_COUNT - 1) % FRAME_COUNT, x, y);
    if before != 0 {
        painter.fill(area, PALETTE[before as usize].scale_alpha(80), Fixed::ONE);
    }
    let after = frames.get((frame + 1) % FRAME_COUNT, x, y);
    if after != 0 {
        painter.border(
            Rect::new(left + 7, top + 7, CELL - 15, CELL - 15),
            PALETTE[after as usize].scale_alpha(160),
            Fixed::ONE,
            Fixed::ONE,
        );
    }
}

fn paint_pixel_surface(painter: &mut PlayPainter<'_, '_>, model: &PixelModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    for (x, y) in [(316, 11), (320, 11), (316, 15), (320, 15)] {
        painter.fill(Rect::new(x, y, 3, 3), ACTIVE, Fixed::ZERO);
    }
    painter.fill(BOARD_RECT, BOARD, Fixed::from_int(6));
    painter.border(
        BOARD_RECT,
        Color::rgb(76, 70, 95),
        Fixed::ONE,
        Fixed::from_int(6),
    );
    let frame = model.visible_frame();
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            paint_cell(painter, model, frame, x, y);
        }
    }
    for index in 0..FRAME_COUNT {
        let x = COLUMN_LEFT + i32::from(index) * THUMB_STEP;
        let area = Rect::new(x, FRAMES_TOP, THUMB_WIDTH, THUMB_HEIGHT);
        let selected = frame == index;
        painter.fill(
            area,
            if selected {
                Color::rgb(67, 55, 79)
            } else {
                Color::rgb(41, 45, 54)
            },
            Fixed::from_int(6),
        );
        painter.border(
            area,
            if selected {
                ACTIVE
            } else {
                Color::rgb(72, 80, 90)
            },
            Fixed::ONE,
            Fixed::from_int(6),
        );
        paint_sprite(
            painter,
            model.frames(),
            index,
            Point::new(x + 3, FRAMES_TOP + 4),
            Fixed::from_ratio(5, 2),
            false,
        );
    }
}

#[crate::view(
    component = PixelSurface,
    name = "PixelSurface",
    priority = 60,
    input = (
        super::clipboard::select_all::callback(),
        super::clipboard::inspect::callback(),
        super::clipboard::copy::callback(),
        super::clipboard::cut::callback(),
        super::clipboard::paste::callback(),
    ),
)]
pub(super) fn surface_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    paint_pixel_surface(&mut painter, model);
}

#[crate::view(component = PixelPreview, name = "PixelPreview", priority = 60)]
pub(super) fn preview_render(
    model: model!(component.model, watch(visual_revision, preview_frame)),
    paint: paint!(),
) {
    paint.set_background_handled(true);
    let (_, _, width, height) = PREVIEW;
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), width, height);
    let mut painter = PlayPainter::new(paint, transform);
    let area = Rect::new(0, 0, width, height);
    painter.fill(area, BACKGROUND, Fixed::ZERO);
    painter.fill(area, PANEL, Fixed::from_int(8));
    painter.border(area, Color::rgb(69, 67, 83), Fixed::ONE, Fixed::from_int(8));
    paint_sprite(
        &mut painter,
        model.frames(),
        model.preview_frame(),
        Point::new(8, 6),
        Fixed::from_int(5),
        true,
    );
}

#[crate::view(
    component = PixelModalSurface,
    name = "PixelModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    model: model!(component.model, watch(visual_revision)),
    paint: paint!(),
) {
    if model.modal() == PixelModal::None {
        return;
    }
    paint.set_background_handled(true);
    let transform = fit_logical_canvas(paint.rect(), paint.transform(), 480, 320);
    let mut painter = PlayPainter::new(paint, transform);
    painter.fill(
        Rect::new(0, 0, 480, 320),
        Color::rgba(8, 9, 14, 220),
        Fixed::ZERO,
    );
    painter.fill(
        Rect::new(17, 65, 446, 206),
        Color::rgb(39, 40, 51),
        Fixed::from_int(10),
    );
    painter.border(
        Rect::new(17, 65, 446, 206),
        ACTIVE,
        Fixed::ONE,
        Fixed::from_int(10),
    );
    if model.modal() == PixelModal::Templates {
        for template in 0..3_u8 {
            let x = 29 + i32::from(template) * 142;
            painter.fill(Rect::new(x, 112, 135, 104), PANEL, Fixed::from_int(7));
            painter.border(
                Rect::new(x, 112, 135, 104),
                Color::rgb(89, 80, 104),
                Fixed::ONE,
                Fixed::from_int(7),
            );
            let frames = PixelFrames::template(template).expect("built-in template");
            paint_sprite(
                &mut painter,
                &frames,
                0,
                Point::new(x + 32, 121),
                Fixed::from_int(6),
                false,
            );
        }
    } else {
        painter.fill(Rect::new(57, 110, 96, 96), BOARD, Fixed::from_int(5));
        paint_sprite(
            &mut painter,
            model.frames(),
            model.frame(),
            Point::new(57, 110),
            Fixed::from_int(8),
            true,
        );
    }
}
