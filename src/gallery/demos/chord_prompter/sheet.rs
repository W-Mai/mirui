use super::font::CONTENT_SIZE;
use super::fretboard::{Fretboard, Role, STRINGS, Tones};
use super::label;
use super::{Score, Session};
use crate::prelude::*;
use crate::render::{command::DrawCommand, font::Font};
use crate::text::{FlowPoint, GlyphId, PositionedGlyph, TextFlow};
use crate::types::Transform;
use alloc::{string::String, vec::Vec};

const BAR_BYTES: usize = super::ScoreLimits::DEVICE.bytes;

/// Narrowest page the lyrics are laid out for.
const MIN_PAGE_WIDTH: i32 = 120;

/// Width of one page of lyrics on a stage `stage` wide.
pub(super) fn page_width(stage: Fixed) -> u16 {
    stage.to_int().max(MIN_PAGE_WIDTH) as u16
}

/// The pixel where column `index` of `columns` begins across `width`. A bar
/// that fits one row has a column per chord, so this is also where the chord
/// sits in time.
pub(super) fn column_x(index: usize, columns: usize, width: u16) -> u16 {
    (index * usize::from(width) / columns.max(1)) as u16
}

#[derive(Clone, Copy, Default)]
pub(super) struct Slot {
    pub start: u16,
    pub end: u16,
    pub first_line: u16,
    pub end_line: u16,
    pub x: u16,
    pub width: u16,
    pub y: u16,
    pub height: u16,
    pub chord: &'static str,
    /// The notes of the chord's triad, for the neck of a solo.
    pub tones: Tones,
}

pub(super) struct Sheet {
    pub text: String,
    pub lines: Vec<[u16; 2]>,
    pub slots: [Slot; 48],
    /// A page of a bar without lyrics shows where to play instead of words.
    pub boards: [Option<Fretboard>; 3],
    pub count: usize,
    pub current_count: usize,
    pub current_start: usize,
    pub previous_height: u16,
    pub next_height: u16,
    pub height: u16,
    accents: [u8; BAR_BYTES / 8],
}

fn keep_largest(largest: &mut [usize; 3], value: usize) {
    let smallest = largest.iter_mut().min().expect("three entries");
    *smallest = (*smallest).max(value);
}

impl Sheet {
    pub fn new() -> Self {
        Self {
            text: String::with_capacity(128),
            lines: Vec::with_capacity(128),
            slots: [Slot::default(); 48],
            boards: [None; 3],
            count: 0,
            current_count: 0,
            current_start: 0,
            previous_height: 0,
            next_height: 0,
            height: 0,
            accents: [0; BAR_BYTES / 8],
        }
    }

    /// Sizes the buffers once. A queued jump can put any three bars side by
    /// side, so the reserve covers the three largest bars.
    pub fn prepare(&mut self, score: &Score, width: u16, height: u16) {
        let (mut text, mut lines) = ([0usize; 3], [0usize; 3]);
        for bar in 0..score.bars() {
            self.rebuild_pages(score, [None, Some(bar), None], width, height);
            keep_largest(&mut text, self.text.len());
            keep_largest(&mut lines, self.lines.len());
        }
        self.text.clear();
        self.text.reserve(text.iter().sum());
        self.lines.clear();
        self.lines.reserve(lines.iter().sum());
    }

    /// Lays out the bar before, the bar itself and the bar after it.
    #[cfg(test)]
    pub fn rebuild(&mut self, score: &Score, bar: u16, width: u16, viewport_height: u16) {
        let next = (bar + 1 < score.bars()).then_some(bar + 1);
        self.rebuild_pages(
            score,
            [bar.checked_sub(1), Some(bar), next],
            width,
            viewport_height,
        );
    }

    /// Lays out the previous, current and next pages. A bar shown twice
    /// shares the text and layout of its first page.
    pub fn rebuild_pages(
        &mut self,
        score: &Score,
        pages: [Option<u16>; 3],
        width: u16,
        viewport_height: u16,
    ) {
        self.text.clear();
        self.accents.fill(0);
        self.count = 0;
        let first = |page: usize| {
            (0..page).find(|&earlier| pages[earlier].is_some() && pages[earlier] == pages[page])
        };
        let mut ranges = [0..0, 0..0, 0..0];
        self.boards = [None; 3];
        for page in 0..3 {
            let start = self.count;
            if let Some(earlier) = first(page) {
                self.boards[page] = self.boards[earlier];
                for index in ranges[earlier].clone() {
                    self.slots[self.count] = self.slots[index];
                    self.count += 1;
                }
            } else {
                for chord in pages[page].map(|bar| score.bar_chords(bar)).unwrap_or(&[]) {
                    let start = self.text.len();
                    for span in score.lyric(chord).segments() {
                        let offset = self.text.len();
                        self.text.push_str(span.text);
                        if span.accented {
                            for byte in offset..self.text.len() {
                                self.accents[byte / 8] |= 1 << (byte % 8);
                            }
                        }
                    }
                    self.slots[self.count] = Slot {
                        start: start as u16,
                        end: self.text.len() as u16,
                        chord: score.chord_name(chord.degree),
                        tones: score.tones(chord.degree),
                        ..Slot::default()
                    };
                    self.count += 1;
                }
                self.boards[page] = pages[page].and_then(|bar| score.solo(bar));
            }
            ranges[page] = start..self.count;
            if page == 0 {
                self.current_start = self.count;
            }
            if page == 1 {
                self.current_count = self.count;
            }
        }
        self.lines.clear();
        let width = width.max(CONTENT_SIZE * 2);
        let mut heights = [0u16; 3];
        for page in 0..3 {
            let range = ranges[page].clone();
            if let Some(earlier) = first(page) {
                for (to, from) in range.zip(ranges[earlier].clone()) {
                    self.slots[to] = self.slots[from];
                }
                heights[page] = heights[earlier];
                continue;
            }
            if self.boards[page].is_some() {
                // The neck takes the whole strip, so nothing scrolls.
                for slot in &mut self.slots[range.clone()] {
                    slot.x = 0;
                    slot.width = width;
                    slot.y = 0;
                    slot.height = 0;
                    slot.first_line = self.lines.len() as u16;
                    slot.end_line = slot.first_line;
                }
                heights[page] = viewport_height.max(1);
                continue;
            }
            let columns = range
                .len()
                .min(usize::from(width / (CONTENT_SIZE * 2)))
                .max(1);
            let first_line = self.lines.len();
            let (mut height, overflow) = self.wrap(width, columns, viewport_height, range.clone());
            if overflow && columns > 1 {
                self.lines.truncate(first_line);
                height = self.wrap(width, 1, viewport_height, range).0;
            }
            heights[page] = height;
        }
        [self.previous_height, self.height, self.next_height] = heights;
    }

    #[cfg(test)]
    pub fn current_text(&self) -> &str {
        let end = if self.current_count == 0 {
            0
        } else {
            self.slots[self.current_count - 1].end as usize
        };
        &self.text[usize::from(self.slots[self.current_start].start)..end]
    }

    #[cfg(test)]
    pub fn page_text(&self, page: usize) -> String {
        let range = [
            0..self.current_start,
            self.current_start..self.current_count,
            self.current_count..self.count,
        ][page]
            .clone();
        self.slots[range]
            .iter()
            .map(|slot| &self.text[usize::from(slot.start)..usize::from(slot.end)])
            .collect()
    }

    fn wrap(
        &mut self,
        width: u16,
        columns: usize,
        viewport_height: u16,
        slots: core::ops::Range<usize>,
    ) -> (u16, bool) {
        let mut y = 0u16;
        let mut overflow = false;
        let line_height = CONTENT_SIZE + 2;
        for group in (slots.start..slots.end).step_by(columns) {
            let mut group_height = 0;
            for index in group..(group + columns).min(slots.end) {
                let slot = &mut self.slots[index];
                slot.x = column_x(index - group, columns, width);
                let end = column_x(index - group + 1, columns, width);
                slot.width = end - slot.x;
                slot.y = y;
                slot.first_line = self.lines.len() as u16;
                let text = &self.text[usize::from(slot.start)..usize::from(slot.end)];
                let units = usize::from(slot.width.saturating_sub(4) / (CONTENT_SIZE / 2)).max(2);
                for line in TextFlow::new(text, units) {
                    let range = line.range();
                    self.lines.push([
                        slot.start + range.start as u16,
                        slot.start + range.end as u16,
                    ]);
                }
                slot.end_line = self.lines.len() as u16;
                slot.height = 2 + (slot.end_line - slot.first_line).max(1) * line_height;
                group_height = group_height.max(slot.height);
            }
            overflow |= group_height > viewport_height;
            y += group_height + 6;
        }
        (y.max(1), overflow)
    }

    pub fn accented(&self, byte: usize) -> bool {
        self.accents[byte / 8] & (1 << (byte % 8)) != 0
    }

    pub fn scroll(&self, slot: usize, elapsed: u64, duration: u64, viewport: u16) -> Fixed {
        if self.height <= viewport || self.current_count == 0 {
            return Fixed::ZERO;
        }
        let slot = (self.current_start + slot).min(self.current_count - 1);
        let max = self.height - viewport;
        let current = &self.slots[slot];
        let line_height = u64::from(CONTENT_SIZE + 2);
        let rows = u64::from(viewport.saturating_sub(4) / (CONTENT_SIZE + 2)).max(1);
        let lines = u64::from(current.end_line - current.first_line).max(1);
        let pages = lines.div_ceil(rows);
        let duration = duration.max(1);
        let progress = elapsed.min(duration) * pages;
        let page = (progress / duration).min(pages - 1);
        let start = (u64::from(current.y) + page * rows * line_height).min(u64::from(max));
        let end = if page + 1 < pages {
            (u64::from(current.y) + (page + 1) * rows * line_height).min(u64::from(max))
        } else if slot + 1 < self.current_count {
            u64::from(self.slots[slot + 1].y.min(max))
        } else {
            u64::from(max)
        };
        // Hold complete reading windows; reveal the next one before its interval.
        let phase = progress - page * duration;
        let travel = (phase * 4).saturating_sub(duration * 2).min(duration);
        let offset = start + (end - start) * travel / duration;
        Fixed::from_int(offset as i32)
    }
}

#[crate::component]
pub(super) struct SheetView {
    pub model: model!(Session),
    pub page: u8,
}

#[crate::view(component=SheetView,name="PrompterSheet")]
pub(super) fn sheet_view(
    component: component!(),
    model: model!(
        component.model,
        watch(sheet_revision, slot_index, clock_active)
    ),
    fonts: res!(crate::render::font::FontManager),
    theme: res!(crate::ui::Theme),
    palette: res!(super::palette::PaletteCache),
    paint: paint!(),
) {
    let rect = paint.rect();
    let transform = paint.transform();
    let clip = paint.clip();
    let sheet = &model.sheet;
    let handle = fonts.resolve(crate::render::font::FontToken::Default.cache_key());
    let mut body = handle.as_ref().clone();
    body.size = CONTENT_SIZE;
    let colors = palette.get(&theme);
    let mut glyphs = [PositionedGlyph::new(GlyphId::new(0), FlowPoint { x: 0, y: 0 }); 32];
    let range = match component.page {
        0 => 0..sheet.current_start,
        1 => sheet.current_start..sheet.current_count,
        _ => sheet.current_count..sheet.count,
    };
    if let Some(board) = &sheet.boards[usize::from(component.page)] {
        // The chord that sounds lights its notes in the box.
        let tones = (component.page == 1)
            .then(|| {
                let at =
                    (range.start + usize::from(model.slot_index)).min(range.end.saturating_sub(1));
                sheet.slots.get(at).map(|slot| slot.tones)
            })
            .flatten();
        draw_neck(
            rect,
            transform,
            board,
            tones,
            component.page == 0,
            &colors,
            &mut |command| {
                paint.draw(command);
            },
        );
        return;
    }
    for (index, slot) in sheet.slots[range].iter().enumerate() {
        // What was played recedes; the chord that sounds is lit; the rest is
        // ahead and stays bright, with the word each coming chord lands on
        // only underlined.
        let (color, accent) = match (component.page, index.cmp(&usize::from(model.slot_index))) {
            (0, _) | (1, core::cmp::Ordering::Less) => (colors.past, colors.past),
            (1, core::cmp::Ordering::Equal) => (
                colors.ink,
                if model.clock_active {
                    colors.now
                } else {
                    colors.chrome
                },
            ),
            _ => (colors.ink, colors.chrome),
        };
        let x = rect.x + Fixed::from_int(i32::from(slot.x));
        let y = rect.y + Fixed::from_int(i32::from(slot.y));
        if transform
            .apply_rect_bbox(Rect::new(x, y, slot.width, slot.height))
            .intersect(&clip)
            .is_none()
        {
            continue;
        }
        for (line_index, range) in sheet.lines
            [usize::from(slot.first_line)..usize::from(slot.end_line)]
            .iter()
            .enumerate()
        {
            let pos = Point::new(
                x,
                y + Fixed::from_int(2 + line_index as i32 * i32::from(CONTENT_SIZE + 2)),
            );
            if transform
                .apply_rect_bbox(Rect::new(pos.x, pos.y, slot.width, CONTENT_SIZE + 2))
                .intersect(&clip)
                .is_none()
            {
                continue;
            }
            draw_text(
                &sheet.text[usize::from(range[0])..usize::from(range[1])],
                TextDraw {
                    start: usize::from(range[0]),
                    sheet,
                    font: &body,
                    pos,
                    color,
                    accent,
                    transform,
                },
                &mut glyphs,
                |command| {
                    paint.draw(command);
                },
            );
        }
    }
}

struct TextDraw<'a> {
    start: usize,
    sheet: &'a Sheet,
    font: &'a Font,
    pos: Point,
    color: Color,
    accent: Color,
    transform: Transform,
}

fn draw_text(
    text: &str,
    run: TextDraw<'_>,
    glyphs: &mut [PositionedGlyph; 32],
    mut draw: impl FnMut(&DrawCommand<'_>),
) {
    let TextDraw {
        start,
        sheet,
        font,
        pos,
        color,
        accent,
        transform,
    } = run;
    let mut x = Fixed::ZERO;
    let mut count = 0;
    let mut active = false;
    for (offset, ch) in text.char_indices() {
        let accented = sheet.accented(start + offset);
        if count > 0 && (active != accented || count == glyphs.len()) {
            draw(&DrawCommand::GlyphRun {
                pos: Point::new(
                    pos.x + crate::types::fixed::from_textflow(glyphs[0].origin.x),
                    pos.y,
                ),
                transform,
                glyphs: &glyphs[..count],
                font,
                color: if active { accent } else { color },
                opa: 255,
            });
            count = 0;
        }
        active = accented;
        if ch == '\t' {
            x += font
                .map_char(' ')
                .and_then(|id| font.glyph_advance(id, font.size))
                .unwrap_or(Fixed::ZERO)
                * 4;
            continue;
        }
        let Some(id) = font.map_char(ch) else {
            continue;
        };
        let advance = font.glyph_advance(id, font.size).unwrap_or(Fixed::ZERO);
        glyphs[count] = PositionedGlyph::new(
            id,
            FlowPoint {
                x: crate::types::fixed::to_textflow(x),
                y: 0,
            },
        );
        count += 1;
        if accented {
            draw(&DrawCommand::Line {
                p1: Point::new(pos.x + x, pos.y + Fixed::from_int(i32::from(font.size))),
                p2: Point::new(
                    pos.x + x + advance,
                    pos.y + Fixed::from_int(i32::from(font.size)),
                ),
                transform,
                color: accent,
                width: Fixed::ONE,
                opa: 255,
            });
        }
        x += advance;
    }
    if count > 0 {
        draw(&DrawCommand::GlyphRun {
            pos: Point::new(
                pos.x + crate::types::fixed::from_textflow(glyphs[0].origin.x),
                pos.y,
            ),
            transform,
            glyphs: &glyphs[..count],
            font,
            color: if active { accent } else { color },
            opa: 255,
        });
    }
}

/// Fretted squares of a box on a neck: the frets run across and the strings
/// as on a tab, the first string at the top. The lowest fret is written on the
/// left, big, with the same box an octave up under it; every other fret sits on
/// a lighter cell; and the notes of the chord that sounds stand out.
fn draw_neck(
    rect: Rect,
    transform: Transform,
    board: &Fretboard,
    tones: Option<Tones>,
    past: bool,
    colors: &super::palette::Palette,
    draw: &mut impl FnMut(&DrawCommand<'_>),
) {
    const FRETS: i32 = 5;
    let (x, y, w, h) = (
        rect.x.to_int(),
        rect.y.to_int(),
        rect.w.to_int(),
        rect.h.to_int(),
    );
    if w < 40 || h < 20 {
        return;
    }
    let strings = STRINGS as i32;
    let gap = ((h - 2) / strings).clamp(3, 12);
    let height = gap * strings;
    let top = y + (h - height) / 2;
    let scale = if gap >= 6 { 2 } else { 1 };
    let label_w = 12 * scale + 7;
    let cell = ((w - label_w - 2) / FRETS).max(1);
    let left = x + label_w;

    // The fret numbers, the box's own big and its octave small under it.
    let stacked = 7 * scale + board.high.map_or(0, |_| 3 + 7);
    let label_top = y + (h - stacked) / 2;
    let main = if past { colors.past } else { colors.ink };
    number(board.low, (x + 5, label_top), scale, transform, main, draw);
    if let Some(high) = board.high {
        number(
            high,
            (x + 5, label_top + 7 * scale + 3),
            1,
            transform,
            colors.chrome,
            draw,
        );
    }

    for column in 0..FRETS {
        if (i32::from(board.low) + column) % 2 == 1 {
            label::block(
                left + column * cell,
                top,
                cell,
                height,
                transform,
                colors.cell,
                draw,
            );
        }
    }
    for row in 0..strings {
        label::block(
            left,
            top + row * gap + gap / 2,
            cell * FRETS,
            1,
            transform,
            colors.past,
            draw,
        );
    }
    let big = (gap - 1).clamp(3, 9);
    let small = (big - 2).max(2);
    for string in 0..STRINGS {
        for note in 0..2 {
            let column = i32::from(board.frets[string][note]) - i32::from(board.low);
            if !(0..FRETS).contains(&column) {
                continue;
            }
            let role = tones.map_or(Role::Scale, |tones| tones.role(board.notes[string][note]));
            let (size, color) = match role {
                Role::Root => (big, colors.now),
                Role::Tone => (big, colors.ink),
                Role::Scale if past => (small, colors.past),
                Role::Scale => (small, colors.chrome),
            };
            let center_x = left + column * cell + cell / 2;
            let center_y = top + (strings - 1 - string as i32) * gap + gap / 2;
            label::block(
                center_x - size / 2,
                center_y - size / 2,
                size,
                size,
                transform,
                color,
                draw,
            );
        }
    }
}

/// A fret number of one or two digits at `at`, every pixel `scale` wide.
fn number(
    value: u8,
    at: (i32, i32),
    scale: i32,
    transform: Transform,
    color: Color,
    draw: &mut impl FnMut(&DrawCommand<'_>),
) {
    let mut x = at.0;
    let mut put = |digit: u8| {
        label::bitmap(
            &label::DIGITS[usize::from(digit)],
            5,
            x,
            at.1,
            scale,
            transform,
            color,
            draw,
        );
        x += 6 * scale;
    };
    if value > 9 {
        put(value / 10);
    }
    put(value % 10);
}
