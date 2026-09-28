use super::state::{PixelModalSurface, PixelSurface};
use super::style::{ACTIVE, BACKGROUND, BOARD, HEADER, PALETTE, PANEL};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::paint::PlayPainter;
use crate::gallery::play::pixel::{
    FRAME_COUNT, GRID_HEIGHT, GRID_WIDTH, PixelFrames, PixelModal, PixelModel,
};
use crate::prelude::{Color, Fixed, Point, Rect};
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};

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

fn paint_pixel_surface(painter: &mut PlayPainter<'_, '_>, model: &PixelModel) {
    painter.fill(Rect::new(0, 0, 480, 320), BACKGROUND, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 35), HEADER, Fixed::ZERO);
    painter.line(
        Point::new(12, 34),
        Point::new(468, 34),
        Color::rgb(55, 58, 65),
        Fixed::ONE,
    );
    for (x, y) in [(15, 12), (22, 12), (15, 19), (22, 19)] {
        painter.fill(Rect::new(x, y, 4, 4), ACTIVE, Fixed::ONE);
    }
    painter.fill(Rect::new(12, 54, 202, 202), BOARD, Fixed::from_int(6));
    painter.border(
        Rect::new(12, 54, 202, 202),
        Color::rgb(76, 70, 95),
        Fixed::ONE,
        Fixed::from_int(6),
    );
    let frame = model.visible_frame();
    let previous = (model.frame() + FRAME_COUNT - 1) % FRAME_COUNT;
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let color = model.frames().get(frame, x, y);
            let ghost = !model.playing()
                && model.onion()
                && color == 0
                && model.frames().get(previous, x, y) != 0;
            let fill = if color != 0 {
                PALETTE[color as usize]
            } else if ghost {
                Color::rgb(76, 66, 92)
            } else if (x + y) & 1 == 0 {
                Color::rgb(35, 40, 51)
            } else {
                Color::rgb(40, 44, 53)
            };
            painter.fill(
                Rect::new(17 + i32::from(x) * 16, 59 + i32::from(y) * 16, 15, 15),
                fill,
                Fixed::ONE,
            );
        }
    }
    painter.fill(Rect::new(229, 45, 239, 104), PANEL, Fixed::from_int(8));
    painter.border(
        Rect::new(229, 45, 239, 104),
        Color::rgb(69, 67, 83),
        Fixed::ONE,
        Fixed::from_int(8),
    );
    paint_sprite(
        painter,
        model.frames(),
        frame,
        Point::new(245, 71),
        Fixed::from_ratio(11, 2),
        true,
    );
    for index in 0..4_u8 {
        let x = 230 + i32::from(index) * 60;
        painter.fill(
            Rect::new(x, 162, 55, 50),
            if frame == index {
                Color::rgb(67, 55, 79)
            } else {
                Color::rgb(41, 45, 54)
            },
            Fixed::from_int(6),
        );
        painter.border(
            Rect::new(x, 162, 55, 50),
            if frame == index {
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
            Point::new(x + 13, 166),
            Fixed::from_ratio(5, 2),
            false,
        );
    }
    painter.fill(
        Rect::new(0, 282, 480, 38),
        Color::rgb(24, 34, 31),
        Fixed::ZERO,
    );
}

#[crate::view(
    component = PixelSurface,
    read(model),
    watch(model.visual_revision()),
    name = "PixelSurface",
    priority = 60
)]
pub(super) fn surface_render(
    renderer: &mut dyn Renderer,
    model: &PixelModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    paint_pixel_surface(&mut painter, model);
}

#[crate::view(
    component = PixelModalSurface,
    read(model),
    watch(model.visual_revision()),
    name = "PixelModalSurface",
    priority = 70
)]
pub(super) fn modal_render(
    renderer: &mut dyn Renderer,
    model: &PixelModel,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    if model.modal() == PixelModal::None {
        return;
    }
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
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
            painter.fill(Rect::new(x, 104, 135, 111), PANEL, Fixed::from_int(7));
            painter.border(
                Rect::new(x, 104, 135, 111),
                Color::rgb(89, 80, 104),
                Fixed::ONE,
                Fixed::from_int(7),
            );
            let frames = PixelFrames::template(template).expect("built-in template");
            paint_sprite(
                &mut painter,
                &frames,
                0,
                Point::new(x + 32, 113),
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

pub(super) fn surface_view() -> View {
    surface_render::view()
}

pub(super) fn modal_view() -> View {
    modal_render::view()
}
