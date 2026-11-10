use super::Session;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::ui::Theme;

pub(super) const CHORD_TRACK_HEIGHT: i32 = super::font::CONTENT_SIZE as i32 + 4;

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
) {
    let rect = paint.rect();
    let transform = paint.transform();
    let bright = theme.resolve(ColorToken::Primary);
    let muted = theme.resolve(ColorToken::OnSurfaceVariant);
    let width = rect.w.max(Fixed::ONE);
    let center = (rect.x + rect.w / 2).round();
    let bar_ticks = u64::from(model.beats) * super::TICKS_PER_BEAT;
    let local = model.position_ticks % bar_ticks;
    let shift = Fixed::from_int((width.to_int().max(1) as u64 * local / bar_ticks) as i32);
    let Some(clip) = transform.apply_rect_bbox(rect).intersect(&paint.clip()) else {
        return;
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
                        opa: 100 + attack.velocity,
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
                for (p1, p2) in [
                    (Point::new(x, a), Point::new(x, b)),
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
                        opa: 100 + attack.velocity,
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
                opa: if active { 255 } else { 100 },
            });
        }
        draw(&DrawCommand::Line {
            p1: Point::new(center, rect.y),
            p2: Point::new(center, rect.y + rect.h),
            transform,
            color: bright,
            width: Fixed::ONE,
            opa: 130,
        });
    }
}
#[crate::component]
pub(super) struct TimeNeedle {}

#[crate::view(component=TimeNeedle,name="TimeNeedle")]
pub(super) fn time_needle(component: component!(), paint: paint!(), theme: res!(Theme)) {
    let _ = component;
    let rect = paint.rect();
    let x = (rect.x + rect.w / 2).round();
    paint.draw(&DrawCommand::Line {
        p1: Point::new(x, rect.y),
        p2: Point::new(x, rect.y + rect.h),
        transform: paint.transform(),
        color: theme.resolve(ColorToken::Primary),
        width: Fixed::ONE,
        opa: 90,
    });
    for dx in [-2, 2] {
        paint.draw(&DrawCommand::Line {
            p1: Point::new(x + Fixed::from_int(dx), rect.y),
            p2: Point::new(x, rect.y + Fixed::from_int(3)),
            transform: paint.transform(),
            color: theme.resolve(ColorToken::Primary),
            width: Fixed::ONE,
            opa: 255,
        });
    }
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
        watch(position_ticks, sheet_revision, slot_index, clock_active)
    ),
    fonts: res!(crate::render::font::FontManager),
    paint: paint!(),
    theme: res!(Theme),
) {
    let _ = component;
    let rect = paint.rect();
    let transform = paint.transform();
    let width = model.stage_width.max(Fixed::ONE);
    let handle = fonts.resolve(crate::render::font::FontToken::Default.cache_key());
    let mut font = handle.as_ref().clone();
    font.size = super::font::CONTENT_SIZE;
    let muted = theme.resolve(ColorToken::OnSurfaceVariant);
    let bright = theme.resolve(ColorToken::Primary);
    let mut glyphs = [crate::text::PositionedGlyph::new(
        crate::text::GlyphId::new(0),
        crate::text::FlowPoint { x: 0, y: 0 },
    ); 4];
    for (page, range) in [
        0..model.sheet.current_start,
        model.sheet.current_start..model.sheet.current_count,
        model.sheet.current_count..model.sheet.count,
    ]
    .into_iter()
    .enumerate()
    {
        let slots = range.len().max(1);
        for (index, slot) in model.sheet.slots[range].iter().enumerate() {
            let anchor = rect.x
                + width * Fixed::from_int(page as i32)
                + width * Fixed::from_ratio(index as i32, slots as i32);
            if transform
                .apply_rect_bbox(Rect::new(
                    anchor - Fixed::from_int(24),
                    rect.y,
                    48,
                    CHORD_TRACK_HEIGHT,
                ))
                .intersect(&paint.clip())
                .is_none()
            {
                continue;
            }
            let mut advance = Fixed::ZERO;
            let mut count = 0;
            for ch in slot.chord.chars() {
                if let Some(id) = font.map_char(ch) {
                    glyphs[count] = crate::text::PositionedGlyph::new(
                        id,
                        crate::text::FlowPoint {
                            x: crate::types::fixed::to_textflow(advance),
                            y: 0,
                        },
                    );
                    count += 1;
                    advance += font.glyph_advance(id, font.size).unwrap_or(Fixed::ZERO);
                }
            }
            let active = page == 1 && index == usize::from(model.slot_index);
            paint.draw(&DrawCommand::GlyphRun {
                pos: Point::new((anchor - advance / 2).round(), rect.y),
                transform,
                glyphs: &glyphs[..count],
                font: &font,
                color: if active { bright } else { muted },
                opa: 255,
            });
            paint.draw(&DrawCommand::Line {
                p1: Point::new(anchor, rect.y + Fixed::from_int(i32::from(font.size) + 1)),
                p2: Point::new(anchor, rect.y + rect.h),
                transform,
                color: if active
                    && model.clock_active
                    && model.position_ticks % (u64::from(model.beats) * super::TICKS_PER_BEAT)
                        - (u64::from(model.beats) * super::TICKS_PER_BEAT * index as u64)
                            .div_ceil(slots as u64)
                        <= u64::from(model.beats) * super::TICKS_PER_BEAT * 2
                            / (width.to_int().max(1) as u64)
                {
                    bright
                } else {
                    muted
                },
                width: Fixed::ONE,
                opa: 255,
            });
        }
    }
}
