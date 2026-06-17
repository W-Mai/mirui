extern crate alloc;

use alloc::vec::Vec;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::ui::dirty::Dirty;
use crate::ui::theme::ColorToken;
use crate::ui::view::{View, ViewCtx};

const PX_PER_CELL: i32 = 1;

const GOSPER_GUN: &[(i32, i32)] = &[
    (0, 24),
    (1, 22),
    (1, 24),
    (2, 12),
    (2, 13),
    (2, 20),
    (2, 21),
    (2, 34),
    (2, 35),
    (3, 11),
    (3, 15),
    (3, 20),
    (3, 21),
    (3, 34),
    (3, 35),
    (4, 0),
    (4, 1),
    (4, 10),
    (4, 16),
    (4, 20),
    (4, 21),
    (5, 0),
    (5, 1),
    (5, 10),
    (5, 14),
    (5, 16),
    (5, 17),
    (5, 22),
    (5, 24),
    (6, 10),
    (6, 16),
    (6, 24),
    (7, 11),
    (7, 15),
    (8, 12),
    (8, 13),
];

const ACORN: &[(i32, i32)] = &[(0, 1), (1, 3), (2, 0), (2, 1), (2, 4), (2, 5), (2, 6)];

const GLIDER: &[(i32, i32)] = &[(0, 1), (1, 2), (2, 0), (2, 1), (2, 2)];

pub struct LifeBoard {
    pub cols: i32,
    pub rows: i32,
    pub cell: Vec<bool>,
    scratch: Vec<bool>,
    rng: u32,
    next_drop: i32,
}

impl LifeBoard {
    fn new(cols: i32, rows: i32) -> Self {
        let n = (cols * rows).max(0) as usize;
        let mut b = Self {
            cols,
            rows,
            cell: alloc::vec![false; n],
            scratch: alloc::vec![false; n],
            rng: 0x9e37_79b9,
            next_drop: 0,
        };
        b.next_drop = b.roll_interval();
        b
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn roll_interval(&mut self) -> i32 {
        25 + (self.rand() % 100) as i32
    }

    fn set(&mut self, r: i32, c: i32) {
        let r = r.rem_euclid(self.rows);
        let c = c.rem_euclid(self.cols);
        self.cell[(r * self.cols + c) as usize] = true;
    }

    fn seed(&mut self, origin: (i32, i32), pattern: &[(i32, i32)]) {
        for &(dr, dc) in pattern {
            self.set(origin.0 + dr, origin.1 + dc);
        }
    }

    // keep overlapping top-left cells so a running pattern survives a resize
    fn resize(&mut self, cols: i32, rows: i32) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        let mut next = alloc::vec![false; (cols * rows).max(0) as usize];
        for r in 0..rows.min(self.rows) {
            for c in 0..cols.min(self.cols) {
                if self.cell[(r * self.cols + c) as usize] {
                    next[(r * cols + c) as usize] = true;
                }
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.cell = next;
        self.scratch.clear();
    }

    fn step(&mut self) {
        let (rows, cols) = (self.rows, self.cols);
        self.scratch.resize((rows * cols) as usize, false);
        for r in 0..rows {
            // single-step edge wrap, no modulo
            let up = if r == 0 { rows - 1 } else { r - 1 } * cols;
            let mid = r * cols;
            let down = if r == rows - 1 { 0 } else { r + 1 } * cols;
            for c in 0..cols {
                let left = if c == 0 { cols - 1 } else { c - 1 };
                let right = if c == cols - 1 { 0 } else { c + 1 };
                let cell = &self.cell;
                let live = cell[(up + left) as usize] as i32
                    + cell[(up + c) as usize] as i32
                    + cell[(up + right) as usize] as i32
                    + cell[(mid + left) as usize] as i32
                    + cell[(mid + right) as usize] as i32
                    + cell[(down + left) as usize] as i32
                    + cell[(down + c) as usize] as i32
                    + cell[(down + right) as usize] as i32;
                let idx = (mid + c) as usize;
                self.scratch[idx] = matches!((cell[idx], live), (true, 2) | (_, 3));
            }
        }
        core::mem::swap(&mut self.cell, &mut self.scratch);
    }

    // periodic glider injection keeps the soup from thinning out
    fn advance(&mut self) {
        self.next_drop -= 1;
        if self.next_drop <= 0 {
            self.seed((1, 1), GLIDER);
            self.next_drop = self.roll_interval();
        }
        self.step();
    }

    #[cfg(test)]
    fn alive_count(&self) -> usize {
        self.cell.iter().filter(|&&a| a).count()
    }
}

fn life_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(board) = world.get::<LifeBoard>(entity) else {
        return;
    };
    if board.cols == 0 || board.rows == 0 {
        return;
    }
    ctx.bg_handled = true;
    let theme = ctx.theme(world);
    let bg = theme.resolve(ColorToken::Surface);
    let alive = theme.resolve(ColorToken::Primary);
    let (cols, rows) = (board.cols, board.rows);
    let x0 = rect.x.round().to_int();
    let y0 = rect.y.round().to_int();
    let bw = rect.w.round().to_int();
    let bh = rect.h.round().to_int();
    let mut fill = |area: Rect, color: Color| {
        renderer.draw(
            &DrawCommand::Fill {
                area,
                transform: ctx.transform,
                quad: ctx.quad,
                color,
                radius: Fixed::ZERO,
                opa: 255,
            },
            ctx.clip,
        );
    };

    fill(*rect, bg);
    for r in 0..rows {
        let py = y0 + bh * r / rows;
        let ph = (y0 + bh * (r + 1) / rows) - py;
        let row = (r * cols) as usize;
        for c in 0..cols {
            if !board.cell[row + c as usize] {
                continue;
            }
            let px = x0 + bw * c / cols;
            let pw = (x0 + bw * (c + 1) / cols) - px;
            fill(
                Rect {
                    x: Fixed::from_int(px),
                    y: Fixed::from_int(py),
                    w: Fixed::from_int(pw.max(1)),
                    h: Fixed::from_int(ph.max(1)),
                },
                alive,
            );
        }
    }
}

pub fn life_view() -> View {
    View::new("LifeBoard", 60, life_render).with_filter::<LifeBoard>()
}

#[mirui_macros::system]
pub fn life_step_system(world: &mut World) {
    let mut boards = alloc::vec::Vec::new();
    world.query::<LifeBoard>().collect_into(&mut boards);
    for e in boards {
        // re-grid to the laid-out size: cell count tracks the canvas, no stretch
        if let Some(rect) = world.get::<crate::ui::ComputedRect>(e).map(|c| c.0) {
            let (cols, rows) = dims_from_px(rect.w.to_int(), rect.h.to_int());
            if let Some(b) = world.get_mut::<LifeBoard>(e) {
                b.resize(cols, rows);
            }
        }
        if let Some(b) = world.get_mut::<LifeBoard>(e) {
            b.advance();
        }
        world.insert(e, Dirty);
    }
}

fn dims_from_px(w: i32, h: i32) -> (i32, i32) {
    let cols = (w / PX_PER_CELL).clamp(48, 240);
    let rows = (h / PX_PER_CELL).clamp(48, 240);
    (cols, rows)
}

pub fn build_widgets(world: &mut World, parent: Entity, view_w: u16, view_h: u16) {
    let (cols, rows) = dims_from_px(view_w as i32, view_h as i32);
    let mut board = LifeBoard::new(cols, rows);
    board.seed((3, 2), GOSPER_GUN);
    board.seed((rows * 2 / 3, cols / 2), ACORN);

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            bg_color: ColorToken::Surface,
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) {
            View (
                bg_color: ColorToken::Surface,
                grow: 1.0
            ) [
                board,
            ]
        }
    };
    //~focus-end
}

mirui_macros::timer!(LifeTick, every: 90, |world, _entity| {
    life_step_system(world);
});

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::app::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
    let info = app.backend.display_info();
    app.with_widget(life_view())
        .add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    build_widgets(&mut app.world, parent, info.width, info.height);
    LifeTick::install(&mut app.world);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dims_track_viewport() {
        assert_eq!(dims_from_px(10, 10), (48, 48), "tiny viewport floors at 48");
        assert_eq!(
            dims_from_px(10_000, 10_000),
            (240, 240),
            "huge viewport caps at 240",
        );
        let mid = dims_from_px(200, 100);
        assert!(
            mid.0 > mid.1,
            "wider-than-tall viewport: more cols than rows"
        );
    }

    #[test]
    fn blinker_oscillates_period_2() {
        let mut b = LifeBoard::new(32, 32);
        b.seed((10, 5), &[(0, 0), (0, 1), (0, 2)]);
        let gen0 = b.cell.clone();
        b.step();
        let gen1 = b.cell.clone();
        b.step();
        let gen2 = b.cell.clone();
        assert_ne!(gen0, gen1, "blinker must change on step 1");
        assert_eq!(gen0, gen2, "blinker returns to itself after 2 steps");
    }

    #[test]
    fn glider_wraps_toroidal_after_one_period() {
        let mut b = LifeBoard::new(32, 32);
        let glider = [(0, 1), (1, 2), (2, 0), (2, 1), (2, 2)];
        b.seed((0, 0), &glider);
        let count0 = b.alive_count();
        for _ in 0..4 {
            b.step();
        }
        let mut expected = LifeBoard::new(32, 32);
        for &(r, c) in &glider {
            expected.set(r + 1, c + 1);
        }
        assert_eq!(b.alive_count(), count0, "glider preserves cell count");
        assert_eq!(b.cell, expected.cell, "glider shifts (1,1) after 4 steps");
    }

    #[test]
    fn gosper_gun_stays_active() {
        let mut b = LifeBoard::new(64, 64);
        b.seed((3, 2), GOSPER_GUN);
        let start = b.alive_count();
        assert_eq!(start, GOSPER_GUN.len(), "gun seeds all 36 cells");
        for _ in 0..120 {
            b.step();
        }
        assert!(
            b.alive_count() > start,
            "gun keeps the board active: {start} -> {}",
            b.alive_count(),
        );
    }

    #[test]
    fn random_glider_drops_keep_an_empty_board_alive() {
        // No seed pattern; only the periodic glider injection runs. The board
        // must gain life from the drops rather than staying empty.
        let mut b = LifeBoard::new(64, 64);
        assert_eq!(b.alive_count(), 0);
        let mut max_seen = 0;
        for _ in 0..400 {
            b.advance();
            max_seen = max_seen.max(b.alive_count());
        }
        assert!(
            max_seen >= GLIDER.len(),
            "drops must inject gliders: {max_seen}"
        );
    }
}
