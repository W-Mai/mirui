use super::{ARABIC_FONT, CJK_FONT, DEVANAGARI_FONT, ELLIPSIS_FONT, THAI_FONT, UI_FONT};
use crate::prelude::*;
use crate::render::font::{Font, FontManager, FontStack};
use crate::ui::widgets::text::FontFeature;

pub(super) const UI: FontToken = FontToken::Custom("typography_ui");
pub(super) const CJK: FontToken = FontToken::Custom("typography_cjk");
pub(super) const ARABIC: FontToken = FontToken::Custom("typography_arabic");
pub(super) const DEVANAGARI: FontToken = FontToken::Custom("typography_devanagari");
pub(super) const THAI: FontToken = FontToken::Custom("typography_thai");
pub(super) const ELLIPSIS: FontToken = FontToken::Custom("typography_ellipsis");
pub(super) const FALLBACKS: [FontToken; 5] = [CJK, ARABIC, DEVANAGARI, THAI, ELLIPSIS];
pub(super) const FEATURES_OFF: [FontFeature; 2] =
    [FontFeature::new(*b"liga", 0), FontFeature::new(*b"kern", 0)];
pub(super) const LIVE_SAMPLE: &str = "office AVATAR · 中文字体排版 · مرحبا · किरण · ภาษาไทย";

static WAVE_BASELINE: Path = path!(M 4 68 C 38 16 92 14 126 50 C 148 74 170 68 188 34);

fn font(bytes: &'static [u8], family: &'static str) -> Font {
    Font::from_mirx(family, 24, bytes, &mirx::reader::PayloadLimits::HOST)
        .expect("Typography Lab font")
}

pub(crate) fn register_fonts(world: &mut World) {
    let Some(manager) = world.resource::<FontManager>() else {
        return;
    };
    manager.add_static(UI.cache_key(), font(UI_FONT, "MiSans UI"));
    manager.add_static(CJK.cache_key(), font(CJK_FONT, "MiSans CJK"));
    manager.add_static(ARABIC.cache_key(), font(ARABIC_FONT, "MiSans Arabic"));
    manager.add_static(
        DEVANAGARI.cache_key(),
        font(DEVANAGARI_FONT, "Noto Sans Devanagari"),
    );
    manager.add_static(THAI.cache_key(), font(THAI_FONT, "Noto Sans Thai"));
    manager.add_static(ELLIPSIS.cache_key(), font(ELLIPSIS_FONT, "Noto Sans"));
}

pub(super) fn register_path(world: &mut World) -> PathId {
    world
        .paths()
        .insert_static(WAVE_BASELINE.commands())
        .expect("static typography path")
}

pub(super) fn mixed_stack() -> FontStack {
    FontStack::new(UI).with_fallbacks(&FALLBACKS[..])
}
