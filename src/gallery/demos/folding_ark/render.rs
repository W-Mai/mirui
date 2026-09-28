use super::state::FoldSurface;
use super::style::{APRICOT, BG, BLUE, FLOOR, GRID, LAVENDER, MINT, MUTED, PANEL};
use crate::gallery::fit_logical_canvas;
use crate::gallery::play::expeditions::FoldCell;
use crate::gallery::play::fold::FoldModel;
use crate::gallery::play::paint::PlayPainter;
use crate::prelude::*;
use crate::render::renderer::Renderer;
use crate::ui::view::{View, ViewCtx};

fn paint_map(painter: &mut PlayPainter<'_, '_>, model: &FoldModel) {
    let level = model.level();
    painter.fill(Rect::new(13, 50, 292, 205), PANEL, Fixed::from_int(9));
    painter.border(
        Rect::new(13, 50, 292, 205),
        Color::rgb(42, 68, 81),
        Fixed::ONE,
        Fixed::from_int(9),
    );
    for cell in 0..70_u8 {
        let x = 27 + i32::from(cell % 10) * 26;
        let y = 63 + i32::from(cell / 10) * 26;
        let kind = level.cell(cell);
        if kind == FoldCell::Void {
            continue;
        }
        let fill = match kind {
            FoldCell::Solid => FLOOR,
            FoldCell::Fragile => Color::rgb(62, 55, 76),
            FoldCell::Bridge if model.bridge_on() => BLUE,
            FoldCell::Bridge => Color::rgb(20, 35, 47),
            FoldCell::Switch => Color::rgb(45, 81, 74),
            FoldCell::Void => BG,
        };
        painter.fill(Rect::new(x, y, 23, 23), fill, Fixed::from_int(4));
        painter.border(
            Rect::new(x, y, 23, 23),
            GRID,
            Fixed::ONE,
            Fixed::from_int(4),
        );
        if level.goal() == cell {
            painter.border(
                Rect::new(x + 5, y + 5, 13, 13),
                MINT,
                Fixed::ONE,
                Fixed::from_int(3),
            );
        }
        for seal in 0..usize::from(level.seal_count()) {
            if level.seal(seal) == Some(cell) && model.seal_bits() & (1 << seal) == 0 {
                painter.circle(Point::new(x + 11, y + 11), Fixed::from_int(4), APRICOT);
            }
        }
        if kind == FoldCell::Switch {
            painter.circle(Point::new(x + 11, y + 11), Fixed::from_int(3), LAVENDER);
        }
    }
    let (occupied, len) = model.occupied();
    for cell in &occupied[..usize::from(len)] {
        let x = 27 + i32::from(*cell % 10) * 26;
        let y = 63 + i32::from(*cell / 10) * 26;
        painter.fill(Rect::new(x + 2, y + 2, 19, 19), MINT, Fixed::from_int(5));
        painter.border(
            Rect::new(x + 5, y + 5, 13, 13),
            BG,
            Fixed::ONE,
            Fixed::from_int(3),
        );
    }
}

fn surface_render(
    renderer: &mut dyn Renderer,
    world: &World,
    _entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(model) = world.resource::<FoldModel>() else {
        return;
    };
    ctx.bg_handled = true;
    let transform = fit_logical_canvas(*rect, ctx.transform, 480, 320);
    let mut painter = PlayPainter::new(renderer, ctx, transform, *ctx.clip);
    painter.fill(Rect::new(0, 0, 480, 320), BG, Fixed::ZERO);
    painter.fill(Rect::new(0, 0, 480, 39), PANEL, Fixed::ZERO);
    painter.line(Point::new(14, 39), Point::new(466, 39), GRID, Fixed::ONE);
    paint_map(&mut painter, model);
    painter.fill(Rect::new(317, 50, 149, 205), PANEL, Fixed::from_int(9));
    painter.border(
        Rect::new(317, 50, 149, 205),
        Color::rgb(42, 68, 81),
        Fixed::ONE,
        Fixed::from_int(9),
    );
    painter.circle(
        Point::new(337, 70),
        Fixed::from_int(5),
        if model.bridge_on() { BLUE } else { MUTED },
    );
    painter.circle(Point::new(355, 70), Fixed::from_int(5), APRICOT);
    painter.line(Point::new(329, 91), Point::new(454, 91), GRID, Fixed::ONE);
}

pub(super) fn surface_view() -> View {
    View::new("FoldSurface", 60, surface_render).with_filter::<FoldSurface>()
}
