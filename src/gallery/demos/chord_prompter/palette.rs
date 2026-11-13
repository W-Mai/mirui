//! Colour roles for a small display read from a music stand.
//!
//! Each role starts from the host theme and moves away from the background
//! until it reaches the contrast its job needs, so a theme with dim greys
//! still gives a legible prompter. Contrast is the WCAG ratio, computed with
//! integers and a cubic fit of the sRGB curve.

use crate::prelude::*;
use crate::ui::Theme;
use core::cell::Cell;

/// What each colour is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Palette {
    pub bg: Color,
    /// Chords and lyrics that are still to come.
    pub ink: Color,
    /// What was already played; dim but readable.
    pub past: Color,
    /// The present moment: the sounding chord, the downbeat, a count.
    pub now: Color,
    /// Labels and idle marks.
    pub chrome: Color,
    /// The thin line the music flows through.
    pub needle: Color,
    /// A cell of the beat grid that is a little lighter than the page.
    pub cell: Color,
    pub ok: Color,
    /// Something asked for that has not happened yet: a jump, an upload.
    pub queued: Color,
}

const WHITE: Color = Color::rgb(255, 255, 255);
const BLACK: Color = Color::rgb(0, 0, 0);
/// Five percent of the white point, the WCAG flare term, in millionths.
const FLARE: u64 = 50_000;

/// sRGB channel to linear light in millionths, by a cubic fit that stays
/// within one percent of the transfer curve.
fn linear(channel: u8) -> u64 {
    let v = u64::from(channel) * 1_000_000 / 255;
    let inner = v * 305_306 / 1_000_000 + 682_171;
    let mid = v * inner / 1_000_000 + 12_523;
    v * mid / 1_000_000
}

/// Relative luminance in millionths.
fn luminance(c: Color) -> u64 {
    (2126 * linear(c.r) + 7152 * linear(c.g) + 722 * linear(c.b)) / 10_000
}

/// WCAG contrast ratio in thousandths.
pub(super) fn contrast(a: Color, b: Color) -> u32 {
    let (a, b) = (luminance(a), luminance(b));
    let (high, low) = if a >= b { (a, b) } else { (b, a) };
    ((high + FLARE) * 1000 / (low + FLARE)) as u32
}

fn mix(from: Color, to: Color, step: u32, of: u32) -> Color {
    let channel =
        |a: u8, b: u8| ((u32::from(a) * (of - step) + u32::from(b) * step + of / 2) / of) as u8;
    Color::rgb(
        channel(from.r, to.r),
        channel(from.g, to.g),
        channel(from.b, to.b),
    )
}

/// `fg` when it already has `min` contrast on `bg`, otherwise `fg` moved
/// toward the far pole until it does.
fn lift(fg: Color, bg: Color, min: u32) -> Color {
    if contrast(fg, bg) >= min {
        return fg;
    }
    let pole = if luminance(bg) < 180_000 {
        WHITE
    } else {
        BLACK
    };
    (1..=32)
        .map(|step| mix(fg, pole, step, 32))
        .find(|candidate| contrast(*candidate, bg) >= min)
        .unwrap_or(pole)
}

/// The mix of `bg` toward `fg` that first reaches `min` contrast.
fn dimmest(fg: Color, bg: Color, min: u32) -> Color {
    (1..=32)
        .map(|step| mix(bg, fg, step, 32))
        .find(|candidate| contrast(*candidate, bg) >= min)
        .unwrap_or(fg)
}

impl Palette {
    pub(super) fn of(theme: &Theme) -> Self {
        let bg = theme.resolve(ColorToken::Surface);
        let ink = lift(theme.resolve(ColorToken::OnSurface), bg, 7_000);
        let now = lift(theme.resolve(ColorToken::Primary), bg, 4_500);
        Self {
            bg,
            ink,
            past: dimmest(ink, bg, 3_200),
            now,
            chrome: lift(theme.resolve(ColorToken::OnSurfaceVariant), bg, 4_500),
            needle: dimmest(now, bg, 3_000),
            cell: dimmest(ink, bg, 1_120),
            ok: lift(theme.resolve(ColorToken::Success), bg, 4_500),
            queued: lift(theme.resolve(ColorToken::Tertiary), bg, 4_500),
        }
    }
}

const SOURCES: [ColorToken; 6] = [
    ColorToken::Surface,
    ColorToken::OnSurface,
    ColorToken::Primary,
    ColorToken::OnSurfaceVariant,
    ColorToken::Success,
    ColorToken::Tertiary,
];

/// The palette of the current theme, rebuilt only when a source colour
/// changes, so painting never repeats the contrast search.
#[derive(Default)]
pub(super) struct PaletteCache(Cell<Option<([Color; 6], Palette)>>);

impl PaletteCache {
    pub(super) fn get(&self, theme: &Theme) -> Palette {
        let key = SOURCES.map(|token| theme.resolve(token));
        if let Some((cached, palette)) = self.0.get()
            && cached == key
        {
            return palette;
        }
        let palette = Palette::of(theme);
        self.0.set(Some((key, palette)));
        palette
    }
}
