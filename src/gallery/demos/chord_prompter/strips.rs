use super::Session;
use super::label;
use super::palette::PaletteCache;
use crate::prelude::*;
use crate::render::{command::DrawCommand, font::Font};
use crate::text::{FlowPoint, GlyphId, PositionedGlyph};
use crate::types::{Color, Transform};
use crate::ui::Theme;

pub(super) const CHORD_TRACK_HEIGHT: i32 = super::font::CONTENT_SIZE as i32 + 4;

#[crate::component]
pub(super) struct RhythmStrip {
    pub model: model!(Session),
    pub strums: bool,
}

/// Where a mark's bar position puts it, given the whole bar's width.
fn at(center: Fixed, width: Fixed, shift: Fixed, ratio: Fixed) -> i32 {
    (center + width * ratio - shift).round().to_int()
}

/// A down stroke, drawn from the sixth string to the first: the arrow points
/// up, as the strings run on a tab. It is the heavy one: a wide shaft and, on
/// an accent, a wide head.
fn down_stroke(
    x: i32,
    top: i32,
    bottom: i32,
    strong: bool,
    transform: Transform,
    color: Color,
    draw: &mut impl FnMut(&DrawCommand<'_>),
) {
    let rows = if strong { 4 } else { 3 };
    let bottom = bottom.max(top + rows + 1);
    for row in 0..rows {
        label::block(x - row, top + row, 2 * row + 1, 1, transform, color, draw);
    }
    label::block(
        x - 1,
        top + rows,
        3,
        bottom - top - rows + 1,
        transform,
        color,
        draw,
    );
}

/// An up stroke, from the first string to the sixth: the arrow points down.
/// It is the light one: a thin shaft and a small head.
fn up_stroke(
    x: i32,
    top: i32,
    bottom: i32,
    transform: Transform,
    color: Color,
    draw: &mut impl FnMut(&DrawCommand<'_>),
) {
    let bottom = bottom.max(top + 4);
    label::block(x, top, 1, bottom - top - 2, transform, color, draw);
    for row in 0..3 {
        label::block(
            x - 2 + row,
            bottom - 2 + row,
            5 - 2 * row,
            1,
            transform,
            color,
            draw,
        );
    }
}

/// The two rows under the lyrics, both drawn around the needle: the count of
/// the bar with its half beats, and the strokes of the pattern. Every other
/// beat sits on a lighter cell, so that the beat reads without lines.
#[crate::view(component = RhythmStrip, name = "RhythmStrip")]
pub(super) fn rhythm_strip(
    component: component!(),
    model: model!(
        component.model,
        watch(
            position_ticks,
            beats,
            beat,
            pattern,
            clock_active,
            sheet_revision
        )
    ),
    paint: paint!(),
    theme: res!(Theme),
    palette: res!(PaletteCache),
) {
    let rect = paint.rect();
    let transform = paint.transform();
    let colors = palette.get(&theme);
    let width = rect.w.max(Fixed::ONE);
    let center = (rect.x + rect.w / 2).round();
    let bar_ticks = u64::from(model.beats) * super::TICKS_PER_BEAT;
    let local = model.position_ticks % bar_ticks;
    let shift = Fixed::from_int((width.to_int().max(1) as u64 * local / bar_ticks) as i32);
    let Some(clip) = transform.apply_rect_bbox(rect).intersect(&paint.clip()) else {
        return;
    };
    let mut draw = |command: &DrawCommand<'_>| paint.draw_clipped(command, clip);
    let (left, right) = (rect.x.to_int(), (rect.x + rect.w).to_int());
    let (y, height) = (rect.y.to_int(), rect.h.to_int());
    let beats = i32::from(model.beats);
    let beat_x = |beat: i32| at(center, width, shift, Fixed::from_ratio(beat, beats));
    let playing = model.clock_active;
    let now_beat = (local / super::TICKS_PER_BEAT) as i32;
    let now_half = (local / (super::TICKS_PER_BEAT / 2)) as i32;

    for beat in -beats..beats * 2 {
        if beat.rem_euclid(beats) % 2 == 1 {
            let (from, to) = (beat_x(beat).max(left), beat_x(beat + 1).min(right));
            label::block(
                from,
                y,
                to - from,
                height,
                transform,
                colors.cell,
                &mut draw,
            );
        }
    }

    if component.strums {
        let top = y + 1;
        let bottom = y + height - 2;
        let string_y =
            |string: u8, top: i32, bottom: i32| bottom - (bottom - top) * i32::from(string) / 5;
        for repeat in -1..=1 {
            for attack in model.pattern.attacks() {
                let x = at(
                    center,
                    width,
                    shift,
                    Fixed::from_int(repeat) + Fixed::from_ratio(i32::from(attack.offset), 400),
                );
                if x < left + 3 || x > right - 3 {
                    continue;
                }
                let tick = bar_ticks * u64::from(attack.offset) / 400;
                let lit =
                    playing && repeat == 0 && tick <= local && (x - center.to_int()).abs() <= 2;
                let past = repeat < 0 || (repeat == 0 && tick < local && !lit);
                let color = if lit {
                    colors.now
                } else if past {
                    colors.past
                } else {
                    colors.ink
                };
                let [low, high] = attack.strings;
                match attack.stroke {
                    super::Stroke::Pick => {
                        let (top, bottom) = (y + 3, y + height - 4);
                        let at_y = string_y(low, top, bottom);
                        label::block(x - 2, at_y - 2, 5, 5, transform, color, &mut draw);
                    }
                    super::Stroke::Down => down_stroke(
                        x,
                        string_y(high, top, bottom),
                        string_y(low, top, bottom),
                        attack.velocity >= 90,
                        transform,
                        color,
                        &mut draw,
                    ),
                    super::Stroke::Up => up_stroke(
                        x,
                        string_y(high, top, bottom),
                        string_y(low, top, bottom),
                        transform,
                        color,
                        &mut draw,
                    ),
                }
            }
        }
        label::block(
            center.to_int(),
            y,
            1,
            height,
            transform,
            colors.now,
            &mut draw,
        );
        return;
    }

    // The count: a number on each beat, a plus between beats where there is room.
    let cell = width.to_int() / beats;
    let plus = cell >= 12;
    for repeat in -1..=1 {
        for beat in 0..beats {
            let number = beat + 1;
            let wide = number > 9;
            let columns = if wide { 11 } else { 5 };
            let beat_at = beat_x(repeat * beats + beat);
            if beat_at < left - columns || beat_at > right + columns {
                continue;
            }
            let lit = playing && repeat == 0 && beat == now_beat;
            let past = repeat < 0 || (repeat == 0 && beat < now_beat);
            let color = if lit {
                colors.now
            } else if past {
                colors.past
            } else {
                colors.ink
            };
            let digits_y = y + (height - 7) / 2;
            if cell < columns + 2 {
                label::block(beat_at - 1, digits_y + 2, 3, 3, transform, color, &mut draw);
            } else if wide {
                for (index, digit) in [number / 10, number % 10].into_iter().enumerate() {
                    label::bitmap(
                        &label::DIGITS[digit as usize],
                        5,
                        beat_at - 5 + index as i32 * 6,
                        digits_y,
                        1,
                        transform,
                        color,
                        &mut draw,
                    );
                }
            } else {
                label::bitmap(
                    &label::DIGITS[number as usize],
                    5,
                    beat_at - 2,
                    digits_y,
                    1,
                    transform,
                    color,
                    &mut draw,
                );
            }
            if plus {
                let half = 2 * (repeat * beats + beat) + 1;
                let plus_at = at(
                    center,
                    width,
                    shift,
                    Fixed::from_ratio(repeat * beats + beat, beats)
                        + Fixed::from_ratio(1, 2 * beats),
                );
                let lit = playing && repeat == 0 && half == now_half;
                let past = repeat < 0 || (repeat == 0 && half < now_half);
                let color = if lit {
                    colors.now
                } else if past {
                    colors.past
                } else {
                    colors.chrome
                };
                label::block(
                    plus_at - 1,
                    y + height / 2,
                    3,
                    1,
                    transform,
                    color,
                    &mut draw,
                );
                label::block(
                    plus_at,
                    y + height / 2 - 1,
                    1,
                    3,
                    transform,
                    color,
                    &mut draw,
                );
            }
        }
    }
    label::block(
        center.to_int(),
        y,
        1,
        height,
        transform,
        colors.needle,
        &mut draw,
    );
}

/// The needle: the line the music flows through, starting under the chord
/// row so the chord that sounds now sits on top of it.
#[crate::component]
pub(super) struct TimeNeedle {}

#[crate::view(component=TimeNeedle,name="TimeNeedle")]
pub(super) fn time_needle(
    component: component!(),
    paint: paint!(),
    theme: res!(Theme),
    palette: res!(PaletteCache),
) {
    let _ = component;
    let colors = palette.get(&theme);
    let rect = paint.rect();
    let transform = paint.transform();
    let x = (rect.x + rect.w / 2).round();
    let top = rect.y + Fixed::from_int(CHORD_TRACK_HEIGHT);
    paint.draw(&DrawCommand::Line {
        p1: Point::new(x, top),
        p2: Point::new(x, rect.y + rect.h),
        transform,
        color: colors.needle,
        width: Fixed::ONE,
        opa: 255,
    });
    for dx in [-2, 2] {
        paint.draw(&DrawCommand::Line {
            p1: Point::new(x + Fixed::from_int(dx), top),
            p2: Point::new(x, top + Fixed::from_int(3)),
            transform,
            color: colors.now,
            width: Fixed::ONE,
            opa: 255,
        });
    }
}

/// One chord, or the end of the song, on the chord row.
#[derive(Clone, Copy, Default)]
struct Mark {
    label: &'static str,
    /// Where its bar position puts its left edge, which is also where the
    /// first letter or character of its lyric begins.
    plain: Fixed,
    advance: Fixed,
    /// The chord that sounds now.
    current: bool,
    end: bool,
}

/// At most three bars of slots and the end of the song.
const MARKS: usize = 49;
/// Clear space kept between neighbouring chord names.
const MARK_GAP: i32 = 3;

fn advance_of(font: &Font, text: &str) -> Fixed {
    text.chars()
        .filter_map(|ch| font.map_char(ch))
        .map(|id| font.glyph_advance(id, font.size).unwrap_or(Fixed::ZERO))
        .fold(Fixed::ZERO, |sum, advance| sum + advance)
}

/// Where a mark's left edge is drawn, given where the needle is.
///
/// A chord is never left of its bar position and never right of the needle.
/// Once the needle passes it, it stays on the needle, so the chord that
/// sounds is always in the same place. The next chord pushes it aside as it
/// arrives, and carries it away at the speed of the music. Because a
/// chord's place depends only on its own and its successor's bar position,
/// it is continuous in time, also where the page behind it is replaced.
fn place(marks: &[Mark], index: usize, needle: Fixed) -> Fixed {
    let mark = marks[index];
    match marks.get(index + 1) {
        Some(next) => {
            let room = next.plain - mark.advance - Fixed::from_int(MARK_GAP);
            mark.plain.max(needle.min(room))
        }
        None => mark.plain.max(needle),
    }
}

/// Where the needle is, in the coordinates of a row that begins at `origin`.
fn needle_at(model: &Session, origin: Fixed) -> Fixed {
    origin + model.page_scroll + model.stage_width.max(Fixed::ONE) / 2
}

/// The chords of the three pages in order, then the end of the song when no
/// bar follows, for a row that begins at `origin`.
fn collect(model: &Session, font: &Font, origin: Fixed) -> ([Mark; MARKS], usize) {
    let width = model.stage_width.max(Fixed::ONE);
    let sheet_width = super::sheet::page_width(model.stage_width);
    let sheet = &model.sheet;
    let mut marks = [Mark::default(); MARKS];
    let mut count = 0;
    for (page, range) in [
        0..sheet.current_start,
        sheet.current_start..sheet.current_count,
        sheet.current_count..sheet.count,
    ]
    .into_iter()
    .enumerate()
    {
        let slots = range.len().max(1);
        for (index, slot) in sheet.slots[range].iter().enumerate() {
            marks[count] = Mark {
                label: slot.chord,
                // The pixel the sheet starts this chord's lyric on whenever
                // the bar fits one row, so the chord's left edge sits right
                // over the first letter or character of its words.
                plain: origin
                    + width * Fixed::from_int(page as i32)
                    + Fixed::from_int(i32::from(super::sheet::column_x(index, slots, sheet_width))),
                advance: advance_of(font, slot.chord),
                current: page == 1 && index == usize::from(model.slot_index),
                end: false,
            };
            count += 1;
        }
    }
    if !model.has_next {
        marks[count] = Mark {
            label: "END",
            plain: origin + width * Fixed::from_int(2),
            advance: advance_of(font, "END"),
            current: false,
            end: true,
        };
        count += 1;
    }
    (marks, count)
}

#[crate::component]
pub(super) struct ChordTimeline {
    pub model: model!(Session),
}

#[crate::view(component=ChordTimeline,name="ChordTimeline")]
pub(super) fn chord_timeline(
    component: component!(),
    model: model!(
        component.model,
        watch(
            position_ticks,
            sheet_revision,
            slot_index,
            clock_active,
            has_next
        )
    ),
    fonts: res!(crate::render::font::FontManager),
    paint: paint!(),
    theme: res!(Theme),
    palette: res!(PaletteCache),
) {
    let _ = component;
    let rect = paint.rect();
    let transform = paint.transform();
    let colors = palette.get(&theme);
    let handle = fonts.resolve(crate::render::font::FontToken::Default.cache_key());
    let mut font = handle.as_ref().clone();
    font.size = super::font::CONTENT_SIZE;
    let needle = needle_at(model, rect.x);
    let (marks, count) = collect(model, &font, rect.x);
    let marks = &marks[..count];
    let mut glyphs = [PositionedGlyph::new(GlyphId::new(0), FlowPoint { x: 0, y: 0 }); 4];
    // The sounding chord goes last, over any neighbour it overlaps.
    for pass in [false, true] {
        for (index, mark) in marks.iter().enumerate() {
            let past = mark.plain < needle;
            let color = if mark.current {
                colors.now
            } else if mark.end {
                if past { colors.ink } else { colors.chrome }
            } else if past {
                colors.past
            } else {
                colors.ink
            };
            if mark.current == pass {
                let left = place(marks, index, needle);
                if transform
                    .apply_rect_bbox(Rect::new(
                        left - Fixed::from_int(2),
                        rect.y,
                        mark.advance + Fixed::from_int(4),
                        CHORD_TRACK_HEIGHT,
                    ))
                    .intersect(&paint.clip())
                    .is_some()
                {
                    let mut advance = Fixed::ZERO;
                    let mut used = 0;
                    for ch in mark.label.chars() {
                        if let Some(id) = font.map_char(ch) {
                            glyphs[used] = PositionedGlyph::new(
                                id,
                                FlowPoint {
                                    x: crate::types::fixed::to_textflow(advance),
                                    y: 0,
                                },
                            );
                            used += 1;
                            advance += font.glyph_advance(id, font.size).unwrap_or(Fixed::ZERO);
                        }
                    }
                    paint.draw(&DrawCommand::GlyphRun {
                        pos: Point::new(left.round(), rect.y),
                        transform,
                        glyphs: &glyphs[..used],
                        font: &font,
                        color,
                        opa: 255,
                    });
                }
            }
            // A short tick under the row marks where a chord begins; it moves
            // away with the music while the sounding chord stays on the needle.
            if mark.current == pass && !mark.end {
                let hot = mark.current
                    && model.clock_active
                    && needle >= mark.plain
                    && needle - mark.plain <= Fixed::from_int(2);
                let x = mark.plain.round();
                if transform
                    .apply_rect_bbox(Rect::new(x - Fixed::ONE, rect.y, 2, CHORD_TRACK_HEIGHT))
                    .intersect(&paint.clip())
                    .is_some()
                {
                    paint.draw(&DrawCommand::Line {
                        p1: Point::new(x, rect.y + Fixed::from_int(i32::from(font.size) + 1)),
                        p2: Point::new(x, rect.y + rect.h),
                        transform,
                        color: if hot {
                            colors.now
                        } else if past {
                            colors.past
                        } else {
                            colors.chrome
                        },
                        width: Fixed::ONE,
                        opa: 255,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
