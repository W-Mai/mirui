use super::state::LifeBoard;
use crate::prelude::*;
use crate::render::command::DrawCommand;

#[crate::view(name = "LifeBoard", priority = 60)]
pub fn life_render(component: component!(LifeBoard), paint: paint!(), theme: res!(Theme)) {
    if component.cols == 0 || component.rows == 0 {
        return;
    }
    paint.set_background_handled(true);
    let bg = theme.resolve(ColorToken::Surface);
    let primary = theme.resolve(ColorToken::Primary);
    let secondary = theme.resolve(ColorToken::Secondary);
    let success = theme.resolve(ColorToken::Success);
    let rect = paint.rect();
    let transform = paint.transform();
    let quad = paint.quad();
    let (cols, rows) = (component.cols, component.rows);
    let x0 = rect.x.round().to_int();
    let y0 = rect.y.round().to_int();
    let bw = rect.w.round().to_int();
    let bh = rect.h.round().to_int();
    let mut fill = |area: Rect, color: Color| {
        paint.draw(&DrawCommand::Fill {
            area,
            transform,
            quad,
            color,
            radius: Fixed::ZERO,
            opa: 255,
        });
    };

    fill(rect, bg);
    for r in 0..rows {
        let py = y0 + bh * r / rows;
        let ph = (y0 + bh * (r + 1) / rows) - py;
        let row = (r * cols) as usize;
        for c in 0..cols {
            if !component.cell[row + c as usize] {
                continue;
            }
            let px = x0 + bw * c / cols;
            let pw = (x0 + bw * (c + 1) / cols) - px;
            let inset = i32::from(pw > 2 && ph > 2);
            let color = match (r / 8 + c / 8).rem_euclid(3) {
                0 => primary,
                1 => secondary,
                _ => success,
            };
            fill(
                Rect {
                    x: Fixed::from_int(px + inset),
                    y: Fixed::from_int(py + inset),
                    w: Fixed::from_int((pw - inset).max(1)),
                    h: Fixed::from_int((ph - inset).max(1)),
                },
                color,
            );
        }
    }
}
