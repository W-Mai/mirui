//! The status row and the count-in, both drawn straight from the model so a
//! display without touch shows what was asked of it from the browser.

use super::Session;
use super::label::{self, DIGITS, JUMP, PAUSE, PLAY, STOP};
use super::palette::PaletteCache;
use super::state::NO_JUMP;
use super::strips::CHORD_TRACK_HEIGHT;
use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::ui::Theme;

/// Counted beats, for bars of up to sixteen beats.
const BEATS: [&str; 16] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
];

/// Decimal digits of `value` in `buffer`, without allocating.
fn number(mut value: u32, buffer: &mut [u8; 5]) -> &str {
    let mut at = buffer.len();
    loop {
        at -= 1;
        buffer[at] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 || at == 0 {
            break;
        }
    }
    core::str::from_utf8(&buffer[at..]).unwrap_or("")
}

#[crate::component]
pub(super) struct Header {
    pub model: model!(Session),
}

/// From the left: whether it plays, the section or a queued jump; at the
/// needle, the beat being counted; from the right: the bar and whether the
/// board is connected. A song arriving over Bluetooth draws a bar along the
/// top edge.
#[crate::view(component = Header, name = "PrompterHeader")]
pub(super) fn header(
    component: component!(),
    model: model!(
        component.model,
        watch(
            playing,
            clock_active,
            connected,
            beat,
            bar_label,
            section,
            jump,
            count_in,
            upload,
            at_start
        )
    ),
    fonts: res!(crate::render::font::FontManager),
    paint: paint!(),
    theme: res!(Theme),
    palette: res!(PaletteCache),
) {
    let _ = component;
    let colors = palette.get(&theme);
    let rect = paint.rect();
    let transform = paint.transform();
    let handle = fonts.resolve(crate::render::font::FontToken::Default.cache_key());
    let mut font = handle.as_ref().clone();
    font.size = 12;
    let (x, y, width) = (rect.x.to_int(), rect.y.to_int(), rect.w.to_int());
    let needle = x + width / 2;
    let mut draw = |command: &DrawCommand<'_>| paint.draw(command);
    let running = model.playing && model.clock_active;
    let counting = model.playing && !model.clock_active;

    let (sign, sign_color): (&[u8], _) = if running {
        (&PLAY, colors.now)
    } else if counting {
        (&PLAY, colors.chrome)
    } else if model.at_start {
        (&STOP, colors.chrome)
    } else {
        (&PAUSE, colors.ink)
    };
    label::bitmap(sign, 5, x + 1, y + 1, 1, transform, sign_color, &mut draw);

    let text_x = x + 10;
    let room = Fixed::from_int((needle - 9 - text_x).max(0));
    if model.jump != NO_JUMP {
        label::bitmap(
            &JUMP,
            7,
            text_x,
            y + 3,
            1,
            transform,
            colors.queued,
            &mut draw,
        );
        let mut digits = [0; 5];
        let target = number(u32::from(model.jump) + 1, &mut digits);
        let run = label::shape(&font, target, room - Fixed::from_int(9));
        run.draw(
            &font,
            Point::new(text_x + 9, y),
            transform,
            colors.queued,
            &mut draw,
        );
    } else {
        let run = label::shape(&font, &model.section, room);
        run.draw(
            &font,
            Point::new(text_x, y),
            transform,
            colors.chrome,
            &mut draw,
        );
    }

    let beat = if model.count_in != 0 {
        Some((usize::from(model.count_in), colors.now))
    } else if running {
        let color = if model.beat == 0 {
            colors.now
        } else {
            colors.chrome
        };
        Some((usize::from(model.beat) + 1, color))
    } else {
        None
    };
    if let Some((beat, color)) = beat {
        let run = label::shape(&font, BEATS[beat.clamp(1, 16) - 1], Fixed::from_int(24));
        run.draw(
            &font,
            Point::new(needle - run.width.to_int() / 2, y),
            transform,
            color,
            &mut draw,
        );
    }

    // The bar to the left of the slash is lit, the song's length is not.
    let edge = x + width - 9;
    let slash = model.bar_label.find('/').unwrap_or(model.bar_label.len());
    let (current, total) = model.bar_label.split_at(slash);
    let total = label::shape(&font, total, Fixed::from_int(36));
    let current = label::shape(&font, current, Fixed::from_int(36));
    let total_x = edge - total.width.to_int();
    total.draw(
        &font,
        Point::new(total_x, y),
        transform,
        colors.chrome,
        &mut draw,
    );
    current.draw(
        &font,
        Point::new(total_x - current.width.to_int(), y),
        transform,
        colors.ink,
        &mut draw,
    );

    let (pip_x, pip_y) = (x + width - 5, y + 4);
    if model.upload.is_some() {
        label::block(pip_x, pip_y, 5, 5, transform, colors.queued, &mut draw);
    } else if model.connected {
        label::block(pip_x, pip_y, 5, 5, transform, colors.ok, &mut draw);
    } else {
        label::block(pip_x, pip_y, 5, 1, transform, colors.chrome, &mut draw);
        label::block(pip_x, pip_y + 4, 5, 1, transform, colors.chrome, &mut draw);
        label::block(pip_x, pip_y + 1, 1, 3, transform, colors.chrome, &mut draw);
        label::block(
            pip_x + 4,
            pip_y + 1,
            1,
            3,
            transform,
            colors.chrome,
            &mut draw,
        );
    }
    if let Some(percent) = model.upload {
        let filled = (width * i32::from(percent) / 100).max(2);
        label::block(x, y, filled, 2, transform, colors.queued, &mut draw);
    }
}

#[crate::component]
pub(super) struct CountIn {
    pub model: model!(Session),
}

/// Big numbers over the lyrics while a delayed start is counted. The chord
/// row stays clear, so the first chord can be set up during the count.
#[crate::view(component = CountIn, name = "PrompterCountIn")]
pub(super) fn count_in(
    component: component!(),
    model: model!(component.model, watch(count_in)),
    paint: paint!(),
    theme: res!(Theme),
    palette: res!(PaletteCache),
) {
    let _ = component;
    if model.count_in == 0 {
        return;
    }
    let colors = palette.get(&theme);
    let rect = paint.rect();
    let transform = paint.transform();
    let mut draw = |command: &DrawCommand<'_>| paint.draw(command);
    let top = rect.y.to_int() + CHORD_TRACK_HEIGHT;
    let height = rect.h.to_int() - CHORD_TRACK_HEIGHT;
    if height < 14 {
        return;
    }
    draw(&DrawCommand::Fill {
        area: Rect::new(rect.x, top, rect.w, height),
        transform,
        quad: None,
        color: colors.bg,
        radius: Fixed::ZERO,
        opa: 210,
    });
    let scale = (height * 3 / 4 / 7).clamp(2, 8);
    let mut buffer = [0; 5];
    let text = number(u32::from(model.count_in), &mut buffer);
    let columns = text.len() as i32;
    let total = columns * 5 * scale + (columns - 1) * scale;
    let mut at = rect.x.to_int() + (rect.w.to_int() - total) / 2;
    let y = top + (height - 7 * scale) / 2;
    for digit in text.bytes() {
        label::bitmap(
            &DIGITS[usize::from(digit - b'0')],
            5,
            at,
            y,
            scale,
            transform,
            colors.now,
            &mut draw,
        );
        at += 6 * scale;
    }
}
