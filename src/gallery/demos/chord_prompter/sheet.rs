use super::{Score, Session};
use crate::prelude::*;
use crate::render::{command::DrawCommand, font::Font};
use crate::text::{FlowPoint, GlyphId, PositionedGlyph, TextFlow};
use crate::types::Transform;
use alloc::{string::String, vec::Vec};

const BAR_BYTES: usize = super::ScoreLimits::DEVICE.bytes;

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
}

pub(super) struct Sheet {
    pub text: String,
    pub lines: Vec<[u16; 2]>,
    pub slots: [Slot; 48],
    pub count: usize,
    pub current_count: usize,
    pub current_start: usize,
    pub previous_height: u16,
    pub next_height: u16,
    pub height: u16,
    pub size: u16,
    accents: [u8; BAR_BYTES / 8],
}

impl Sheet {
    pub fn new() -> Self {
        Self {
            text: String::with_capacity(128),
            lines: Vec::with_capacity(128),
            slots: [Slot::default(); 48],
            count: 0,
            current_count: 0,
            current_start: 0,
            previous_height: 0,
            next_height: 0,
            height: 0,
            size: 18,
            accents: [0; BAR_BYTES / 8],
        }
    }

    pub fn prepare(&mut self, score: &Score, width: u16, height: u16, large: bool) {
        for bar in 0..score.bars() {
            self.rebuild(score, bar, width, height, large);
        }
    }

    pub fn rebuild(
        &mut self,
        score: &Score,
        bar: u16,
        width: u16,
        viewport_height: u16,
        large: bool,
    ) {
        self.text.clear();
        self.accents.fill(0);
        self.size = if large { 24 } else { 18 };
        self.count = 0;
        for page in 0..3 {
            let source_bar = match page {
                0 => bar.checked_sub(1),
                1 => Some(bar),
                _ => Some(bar + 1),
            };
            for chord in source_bar.map(|bar| score.bar_chords(bar)).unwrap_or(&[]) {
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
                    ..Slot::default()
                };
                self.count += 1;
            }
            if page == 0 {
                self.current_start = self.count;
            }
            if page == 1 {
                self.current_count = self.count;
            }
        }
        self.lines.clear();
        let width = width.max(self.size * 2);
        for (page, range) in [
            0..self.current_start,
            self.current_start..self.current_count,
            self.current_count..self.count,
        ]
        .into_iter()
        .enumerate()
        {
            let columns = range.len().min(usize::from(width / (self.size * 2))).max(1);
            let first_line = self.lines.len();
            let (mut height, overflow) = self.wrap(width, columns, viewport_height, range.clone());
            if overflow && columns > 1 {
                self.lines.truncate(first_line);
                height = self.wrap(width, 1, viewport_height, range).0;
            }
            match page {
                0 => self.previous_height = height,
                1 => self.height = height,
                _ => self.next_height = height,
            }
        }
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

    fn wrap(
        &mut self,
        width: u16,
        columns: usize,
        viewport_height: u16,
        slots: core::ops::Range<usize>,
    ) -> (u16, bool) {
        let mut y = 0u16;
        let mut overflow = false;
        let line_height = self.size + 2;
        for group in (slots.start..slots.end).step_by(columns) {
            let mut group_height = 0;
            for index in group..(group + columns).min(slots.end) {
                let slot = &mut self.slots[index];
                slot.x = ((index - group) * usize::from(width) / columns) as u16;
                let end = ((index - group + 1) * usize::from(width) / columns) as u16;
                slot.width = end - slot.x;
                slot.y = y;
                slot.first_line = self.lines.len() as u16;
                let text = &self.text[usize::from(slot.start)..usize::from(slot.end)];
                let units = usize::from(slot.width.saturating_sub(4) / (self.size / 2)).max(2);
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
        let start = self.slots[slot].y.min(max);
        let end = if slot + 1 < self.current_count {
            self.slots[slot + 1].y.min(max)
        } else {
            max
        };
        // Leave the final screen visible before the musical interval ends.
        let travel_ticks = (duration * 3 / 4).max(1);
        let offset = u64::from(end - start) * elapsed.min(travel_ticks) / travel_ticks;
        Fixed::from_int(i32::from(start) + offset as i32)
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
    paint: paint!(),
) {
    let rect = paint.rect();
    let transform = paint.transform();
    let clip = paint.clip();
    let sheet = &model.sheet;
    let handle = fonts.resolve(crate::render::font::FontToken::Default.cache_key());
    let mut body = handle.as_ref().clone();
    body.size = sheet.size;
    let ink = theme.resolve(ColorToken::OnSurface);
    let accent = theme.resolve(ColorToken::Primary);
    let muted = theme.resolve(ColorToken::OnSurfaceVariant);
    let mut glyphs = [PositionedGlyph::new(GlyphId::new(0), FlowPoint { x: 0, y: 0 }); 32];
    let range = match component.page {
        0 => 0..sheet.current_start,
        1 => sheet.current_start..sheet.current_count,
        _ => sheet.current_count..sheet.count,
    };
    for (index, slot) in sheet.slots[range].iter().enumerate() {
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
                y + Fixed::from_int(2 + line_index as i32 * i32::from(sheet.size + 2)),
            );
            if transform
                .apply_rect_bbox(Rect::new(pos.x, pos.y, slot.width, sheet.size + 2))
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
                    color: ink,
                    accent: if model.clock_active
                        && component.page == 1
                        && index == usize::from(model.slot_index)
                    {
                        accent
                    } else {
                        muted
                    },
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
