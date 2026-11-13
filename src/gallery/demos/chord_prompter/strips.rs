use super::Session;
use super::palette::PaletteCache;
use crate::prelude::*;
use crate::render::{command::DrawCommand, font::Font};
use crate::text::{FlowPoint, GlyphId, PositionedGlyph};
use crate::ui::Theme;

pub(super) const CHORD_TRACK_HEIGHT: i32 = super::font::CONTENT_SIZE as i32 + 4;

/// Opacity of a mark that is not lit yet, so that it stays readable on a
/// small panel in a bright room.
const IDLE_OPA: u8 = 170;

#[crate::component]
pub(super) struct RhythmStrip {
    pub model: model!(Session),
    pub strums: bool,
}

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
    images: res!(crate::core::resource::ResourceManager<crate::render::texture::Texture<'static>>),
    sprites: res!(super::sprites::Sprites),
) {
    let rect = paint.rect();
    let transform = paint.transform();
    let colors = palette.get(&theme);
    let bright = colors.now;
    let muted = colors.chrome;
    let width = rect.w.max(Fixed::ONE);
    let center = (rect.x + rect.w / 2).round();
    let bar_ticks = u64::from(model.beats) * super::TICKS_PER_BEAT;
    let local = model.position_ticks % bar_ticks;
    let shift = Fixed::from_int((width.to_int().max(1) as u64 * local / bar_ticks) as i32);
    let Some(clip) = transform.apply_rect_bbox(rect).intersect(&paint.clip()) else {
        return;
    };
    let atlas = if component.strums
        && super::sprites::eligible(paint.output_scale(), transform, paint.quad())
        && model
            .pattern
            .attacks()
            .iter()
            .any(|attack| attack.stroke != super::Stroke::Pick)
    {
        sprites.atlas(&images, [muted, bright])
    } else {
        None
    };
    let mut draw = |command: &DrawCommand<'_>| paint.draw_clipped(command, clip);
    if component.strums {
        for repeat in -1..=1 {
            for attack in model.pattern.attacks() {
                let x = (center
                    + width
                        * (Fixed::from_int(repeat)
                            + Fixed::from_ratio(i32::from(attack.offset), 400))
                    - shift)
                    .round();
                if x < rect.x + Fixed::from_int(3) || x > rect.x + rect.w - Fixed::from_int(3) {
                    continue;
                }
                let tick = bar_ticks * u64::from(attack.offset) / 400;
                let active = model.clock_active
                    && repeat == 0
                    && tick <= local
                    && (x - center).abs() <= Fixed::from_int(2);
                let color = if active { bright } else { muted };
                let top = rect.y + Fixed::from_int(3);
                let bottom = rect.y + rect.h - Fixed::from_int(3);
                let string_y = |string| {
                    (top + (bottom - top) * Fixed::from_ratio(i32::from(string), 5)).round()
                };
                if attack.stroke == super::Stroke::Pick {
                    draw(&DrawCommand::Fill {
                        area: Rect::new(
                            x - Fixed::from_int(2),
                            string_y(attack.strings[0]) - Fixed::from_int(2),
                            Fixed::from_int(5),
                            Fixed::from_int(5),
                        ),
                        transform,
                        quad: None,
                        color,
                        radius: Fixed::ZERO,
                        opa: super::sprites::BASE_OPA + attack.velocity,
                    });
                    continue;
                }
                let top = string_y(attack.strings[0]);
                let bottom = string_y(attack.strings[1]);
                let (a, b) = if attack.stroke == super::Stroke::Up {
                    (bottom, top)
                } else {
                    (top, bottom)
                };
                let tip = Fixed::from_int(if attack.stroke == super::Stroke::Up {
                    3
                } else {
                    -3
                });
                draw(&DrawCommand::Line {
                    p1: Point::new(x, a),
                    p2: Point::new(x, b),
                    transform,
                    color,
                    width: Fixed::from_int(2),
                    opa: super::sprites::BASE_OPA + attack.velocity,
                });
                if atlas.as_ref().is_some_and(|atlas| {
                    super::sprites::draw_heads(
                        atlas,
                        Point::new(x, b),
                        attack.stroke == super::Stroke::Up,
                        attack.velocity,
                        active,
                        transform,
                        &mut draw,
                    )
                }) {
                    continue;
                }
                for (p1, p2) in [
                    (
                        Point::new(x, b),
                        Point::new(x - Fixed::from_int(3), b + tip),
                    ),
                    (
                        Point::new(x, b),
                        Point::new(x + Fixed::from_int(3), b + tip),
                    ),
                ] {
                    draw(&DrawCommand::Line {
                        p1,
                        p2,
                        transform,
                        color,
                        width: Fixed::from_int(2),
                        opa: super::sprites::BASE_OPA + attack.velocity,
                    });
                }
            }
        }
        draw(&DrawCommand::Line {
            p1: Point::new(center, rect.y),
            p2: Point::new(center, rect.y + rect.h),
            transform,
            color: bright,
            width: Fixed::ONE,
            opa: 255,
        });
    } else {
        let beats = i32::from(model.beats);
        for beat in -beats..beats * 2 {
            let x = (center + width * Fixed::from_ratio(beat, beats) - shift).round();
            if x < rect.x + Fixed::ONE || x > rect.x + rect.w - Fixed::ONE {
                continue;
            }
            let strong = beat.rem_euclid(beats) == 0
                || (model.unit() == 8 && beats % 3 == 0 && beat.rem_euclid(3) == 0);
            let height = if strong {
                rect.h - Fixed::ONE
            } else {
                (rect.h / 2).max(Fixed::ONE)
            };
            let tick = if beat >= 0 {
                beat as u64 * super::TICKS_PER_BEAT
            } else {
                u64::MAX
            };
            let active =
                model.clock_active && tick <= local && (x - center).abs() <= Fixed::from_int(2);
            draw(&DrawCommand::Fill {
                area: Rect::new(
                    x - Fixed::from_int(2),
                    rect.y + rect.h - height,
                    if strong { 4 } else { 3 },
                    height,
                ),
                transform,
                quad: None,
                color: if active { bright } else { muted },
                radius: Fixed::ZERO,
                opa: if active { 255 } else { IDLE_OPA },
            });
        }
        draw(&DrawCommand::Line {
            p1: Point::new(center, rect.y),
            p2: Point::new(center, rect.y + rect.h),
            transform,
            color: colors.needle,
            width: Fixed::ONE,
            opa: 255,
        });
    }
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
