use crate::core::i18n::{Locale, Translation};
use crate::prelude::*;
use crate::render::font::{Font, FontManager};

const UI_FONT: &[u8] = include_bytes!("../assets/misans_ui.mirx");
pub(super) const TOKEN_CJK: FontToken = FontToken::Custom("misans24");

pub(super) const TRANSLATIONS: &[Translation] = &[
    (Locale::EnUs, "welcome", "Welcome to mirui"),
    (Locale::EnUs, "greeting", "Hello, friend"),
    (Locale::EnUs, "goodbye", "See you soon"),
    (Locale::EnUs, "toggle", "Switch language"),
    (Locale::ZhCn, "welcome", "欢迎使用 mirui"),
    (Locale::ZhCn, "greeting", "你好,朋友"),
    (Locale::ZhCn, "goodbye", "回头见"),
    (Locale::ZhCn, "toggle", "切换语言"),
];

pub(super) fn register_font(world: &mut World) {
    let Some(mgr) = world.resource::<FontManager>() else {
        return;
    };
    let font = Font::from_mirx("MiSans UI", 14, UI_FONT, &mirx::reader::PayloadLimits::HOST)
        .expect("UI font");
    mgr.add_static(TOKEN_CJK.cache_key(), font);
}
