//! Small helpers for text and pixel shapes drawn straight into a view.

use crate::render::{command::DrawCommand, font::Font};
use crate::text::{FlowPoint, GlyphId, PositionedGlyph};
use crate::types::{Color, Fixed, Point, Rect, Transform, fixed::to_textflow};

const CAPACITY: usize = 24;

fn blank() -> PositionedGlyph {
    PositionedGlyph::new(GlyphId::new(0), FlowPoint { x: 0, y: 0 })
}

/// Glyphs laid out on one line, without allocating.
pub(super) struct Run {
    glyphs: [PositionedGlyph; CAPACITY],
    count: usize,
    pub width: Fixed,
}

/// Lays `text` out in `font` and stops before a glyph would pass `max`.
/// Characters the font does not map take no room.
pub(super) fn shape(font: &Font, text: &str, max: Fixed) -> Run {
    let mut run = Run {
        glyphs: [blank(); CAPACITY],
        count: 0,
        width: Fixed::ZERO,
    };
    for ch in text.chars() {
        let Some(id) = font.map_char(ch) else {
            continue;
        };
        let advance = font.glyph_advance(id, font.size).unwrap_or(Fixed::ZERO);
        if run.count == CAPACITY || run.width + advance > max {
            break;
        }
        run.glyphs[run.count] = PositionedGlyph::new(
            id,
            FlowPoint {
                x: to_textflow(run.width),
                y: 0,
            },
        );
        run.count += 1;
        run.width += advance;
    }
    run
}

impl Run {
    pub(super) fn draw(
        &self,
        font: &Font,
        pos: Point,
        transform: Transform,
        color: Color,
        draw: &mut impl FnMut(&DrawCommand<'_>),
    ) {
        if self.count == 0 {
            return;
        }
        draw(&DrawCommand::GlyphRun {
            pos,
            transform,
            glyphs: &self.glyphs[..self.count],
            font,
            color,
            opa: 255,
        });
    }
}

/// An opaque pixel-aligned rectangle.
pub(super) fn block(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    transform: Transform,
    color: Color,
    draw: &mut impl FnMut(&DrawCommand<'_>),
) {
    if w <= 0 || h <= 0 {
        return;
    }
    draw(&DrawCommand::Fill {
        area: Rect::new(x, y, w, h),
        transform,
        quad: None,
        color,
        radius: Fixed::ZERO,
        opa: 255,
    });
}

/// Draws a bitmap whose rows are bit masks, the left pixel in the highest of
/// `width` bits, with every pixel `scale` device pixels wide. Horizontal runs
/// become one fill each.
#[allow(clippy::too_many_arguments)]
pub(super) fn bitmap(
    rows: &[u8],
    width: u32,
    x: i32,
    y: i32,
    scale: i32,
    transform: Transform,
    color: Color,
    draw: &mut impl FnMut(&DrawCommand<'_>),
) {
    for (row, bits) in rows.iter().enumerate() {
        let mut column = 0;
        while column < width {
            if bits & (1 << (width - 1 - column)) == 0 {
                column += 1;
                continue;
            }
            let start = column;
            while column < width && bits & (1 << (width - 1 - column)) != 0 {
                column += 1;
            }
            block(
                x + start as i32 * scale,
                y + row as i32 * scale,
                (column - start) as i32 * scale,
                scale,
                transform,
                color,
                draw,
            );
        }
    }
}

/// Seven-row, five-column digits.
pub(super) const DIGITS: [[u8; 7]; 10] = [
    [
        0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
    ],
    [
        0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
    ],
    [
        0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
    ],
    [
        0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
    ],
    [
        0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
    ],
    [
        0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
    ],
    [
        0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
    ],
    [
        0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
    ],
    [
        0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
    ],
    [
        0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
    ],
];

/// Transport signs, five columns by nine rows, and a small arrow, seven by five.
pub(super) const PLAY: [u8; 9] = [
    0b10000, 0b11000, 0b11100, 0b11110, 0b11111, 0b11110, 0b11100, 0b11000, 0b10000,
];
pub(super) const PAUSE: [u8; 9] = [0b11011; 9];
pub(super) const STOP: [u8; 9] = [0, 0, 0b11111, 0b11111, 0b11111, 0b11111, 0b11111, 0, 0];
pub(super) const JUMP: [u8; 5] = [0b0000100, 0b0000010, 0b1111111, 0b0000010, 0b0000100];
