use super::Session;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::ui::Theme;

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
        watch(bar_phase, beats, beat, pattern, playing)
    ),
    paint: paint!(),
    theme: res!(Theme),
) {
    let rect = paint.rect();
    let transform = paint.transform();
    let bright = theme.resolve(ColorToken::Primary);
    let muted = theme.resolve(ColorToken::OnSurfaceVariant);
    let inset = if component.strums {
        Fixed::from_int(4)
    } else {
        Fixed::ZERO
    };
    let width = (rect.w - inset * Fixed::from_int(2)).max(Fixed::ONE);
    let cursor = rect.x + inset + width * Fixed::from_ratio(i32::from(model.bar_phase), 256);
    if component.strums {
        for attack in model.pattern.attacks() {
            let x = rect.x + inset + width * Fixed::from_ratio(i32::from(attack.offset), 400);
            let active = (x - cursor).abs() < rect.w / Fixed::from_int(24);
            let color = if active { bright } else { muted };
            let top = rect.y + Fixed::from_int(3);
            let bottom = rect.y + rect.h - Fixed::from_int(5);
            let string_y = |string| top + (bottom - top) * Fixed::from_ratio(i32::from(string), 5);
            if attack.stroke == super::Stroke::Pick {
                paint.draw(&DrawCommand::Fill {
                    area: Rect::new(
                        x - Fixed::ONE,
                        string_y(attack.strings[0]) - Fixed::ONE,
                        Fixed::from_int(3),
                        Fixed::from_int(3),
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
                paint.draw(&DrawCommand::Line {
                    p1,
                    p2,
                    transform,
                    color,
                    width: Fixed::ONE,
                    opa: 100 + attack.velocity,
                });
            }
        }
        paint.draw(&DrawCommand::Line {
            p1: Point::new(cursor, rect.y),
            p2: Point::new(cursor, rect.y + rect.h),
            transform,
            color: bright,
            width: Fixed::ONE,
            opa: 255,
        });
    } else {
        let beats = i32::from(model.beats);
        for beat in 0..beats {
            let w = rect.w / Fixed::from_int(beats);
            let area = Rect::new(
                rect.x + w * Fixed::from_int(beat) + Fixed::ONE,
                rect.y + Fixed::from_int(3),
                (w - Fixed::from_int(3)).max(Fixed::ONE),
                rect.h - Fixed::from_int(6),
            );
            paint.draw(&DrawCommand::Fill {
                area,
                transform,
                quad: None,
                color: if beat == i32::from(model.beat) {
                    bright
                } else {
                    muted
                },
                radius: Fixed::ZERO,
                opa: if beat == i32::from(model.beat) {
                    255
                } else {
                    45
                },
            });
        }
    }
}
